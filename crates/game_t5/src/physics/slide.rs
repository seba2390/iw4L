//! Black Ops' physics slide, its player slide move in physics collision:
//! the body is cast along its move again and again, each contact becomes a
//! plane it may not cross, and the move and the velocity are bent to stay
//! clear of every plane met so far; a body that ends up inside something is
//! pushed out, and one walking on the ground is kept on it.
//!
//! A piece met once is held by its plane from then on: the later casts of
//! the same slide leave it out. That is what lets a body that starts a
//! little inside something (a spawn beside a sloped clip) move out of it.

use super::gjk::{V3, add, dot, scale, sub};
use super::shape::Character;
use super::{Contact, PhysicsWorld, PieceKey, Sweep};

use movement_t5::{PhysicsSlid as SlideResult, PhysicsSlide as Slide};

/// A step of the slide shorter than this (in seconds) does not count as a
/// move: its contacts are planes met where the body stands.
const SHORT_STEP: f32 = f32::from_bits(0x3c88_8889);
const MAX_PLANES: usize = 128;
const MAX_STEPS: i32 = 128;
const MIN_MOVE2: f32 = f32::from_bits(0x38d1_b717);
/// The slide gives up and pushes the body out when it would end farther
/// than this many times its move from where it began, or this far.
const STRAY_SCALE: f32 = 16.0;
const STRAY_FLOOR: f32 = 400.0;
const MOST_VIOLATED: f32 = f32::from_bits(0x5511_84e7);
const SOLVE_DET: f32 = f32::from_bits(0x3727_c5ac);
/// Pushing out fails when a plane would still be this far inside.
const PUSH_FAIL: f32 = -5.0;
const PUSH_TRIES: i32 = 5;
const PUSH_SETTLE2: f32 = f32::from_bits(0x3c23_d70b);
const PUSH_MIN: f32 = f32::from_bits(0x3a83_126f);
const PUSH_REACH: f32 = 1e9;
const SNAP_FAR: f32 = 10_000.0;
const SNAP_GAP: f32 = f32::from_bits(0x3dcc_cccd);
const SNAP_DOWN: f32 = -30.0;
const SNAP_EDGE: f32 = f32::from_bits(0x3a83_126f);

/// A plane the slide may not cross: points `p` of the body with
/// `normal · p >= rhs` relative to where it is solved from.
#[derive(Clone, Copy, Debug)]
struct Plane<'w> {
    normal: V3,
    point_a: V3,
    dist: f32,
    active: bool,
    walkable: bool,
    character: bool,
    in_set: bool,
    rhs: f32,
    contact: Contact<'w>,
}

impl<'w> Plane<'w> {
    fn new(contact: Contact<'w>) -> Self {
        Self {
            normal: contact.normal,
            point_a: contact.point_a,
            dist: dot(contact.normal, contact.contact),
            active: true,
            walkable: contact.walkable(),
            character: contact.character(),
            in_set: false,
            rhs: 0.0,
            contact,
        }
    }

    /// How far inside the plane the body's contact lies with the body at
    /// `at`.
    fn inside(&self, at: V3) -> f32 {
        self.dist - dot(self.normal, add(self.point_a, at))
    }
}

enum Pass {
    Done(bool),
    Stuck,
}

impl<'w> PhysicsWorld<'w> {
    /// The player's slide: where it ends and with what velocity; `None`
    /// when the match cannot list the pieces.
    pub(crate) fn slide(&self, slide: &Slide, pass: i32) -> Option<SlideResult> {
        let mut out = SlideResult {
            origin: slide.origin,
            velocity: slide.velocity,
            contacted: false,
            touched: [0; 32],
            touched_count: 0,
        };
        match self.slide_pass(slide, pass, false, &mut out)? {
            Pass::Done(contacted) => out.contacted = contacted,
            Pass::Stuck => {
                if let Some(origin) = self.push_out(slide, pass, &mut out)? {
                    out.origin = origin;
                    out.velocity = slide.velocity;
                } else if let Pass::Stuck = self.slide_pass(slide, pass, true, &mut out)? {
                    out.origin = slide.origin;
                    out.velocity = slide.velocity;
                }
                out.contacted = true;
            }
        }
        Some(out)
    }

    fn slide_pass(
        &self,
        slide: &Slide,
        pass: i32,
        second: bool,
        out: &mut SlideResult,
    ) -> Option<Pass> {
        let mut body = Character::from_bounds(slide.mins, slide.maxs, 0.0);
        let ft = slide.frametime;
        let vel = slide.velocity;
        let mut pos = slide.origin;
        let end_vz = match slide.gravity {
            Some(gravity) => vel[2] - gravity * ft,
            None => vel[2],
        };
        let half = ft * 0.5;
        let end = [
            pos[0] + (vel[0] + vel[0]) * half,
            pos[1] + (vel[1] + vel[1]) * half,
            pos[2] + (vel[2] + end_vz) * half,
        ];
        let mut delta = sub(end, pos);
        let mut vel_disp = [vel[0] * ft, vel[1] * ft, end_vz * ft];
        let stray2 = (dot(delta, delta) * STRAY_SCALE).max(STRAY_FLOOR);
        let start = pos;
        let mut rest = delta;
        let mut vel_in = vel_disp;
        let mut used = 0.0f32;
        let mut steps = 0;
        let mut planes: Vec<Plane<'w>> = Vec::new();
        let mut touched: Vec<PieceKey> = Vec::new();
        loop {
            let remaining = ft - used;
            let cap_add = if SHORT_STEP <= remaining {
                SHORT_STEP / remaining
            } else {
                1.0
            };
            let sweep = Sweep {
                cap_add,
                stop_on_solid: false,
                keep_leaving: false,
                skip: &touched,
            };
            let contacts = self.sweep(
                &mut body,
                pos,
                add(pos, delta),
                pass,
                slide.contentmask,
                sweep,
            )?;
            let Some(best) = contacts.best else {
                pos = add(pos, delta);
                break;
            };
            let mut advanced = 0.0;
            let moved = contacts.all[best].fraction * remaining;
            if SHORT_STEP < moved {
                let f = contacts.all[best].fraction;
                pos = add(pos, scale(delta, f));
                rest = scale(delta, 1.0 - f);
                used += moved;
                advanced = moved;
                vel_in = vel_disp;
            }
            for &contact in &contacts.all {
                if contact.fraction * remaining - advanced > SHORT_STEP {
                    continue;
                }
                let mut contact = contact;
                if second && contact.dist < 0.0 {
                    contact.contact = add(contact.contact, scale(contact.normal, contact.dist));
                    contact.dist = 0.0;
                }
                if planes.len() < MAX_PLANES {
                    planes.push(Plane::new(contact));
                }
                touched.push(PieceKey::of(&contact.piece));
                touch(out, contact.piece.entity);
            }
            for plane in &mut planes {
                plane.rhs = plane.inside(pos);
                if (second || plane.character) && plane.rhs > 0.0 {
                    plane.rhs = 0.0;
                }
            }
            delta = solve(rest, &mut planes);
            let reach = sub(add(pos, delta), start);
            if stray2 < dot(reach, reach) {
                return Some(Pass::Stuck);
            }
            for plane in &mut planes {
                if plane.rhs > 0.0 {
                    plane.rhs = 0.0;
                }
            }
            vel_disp = solve(vel_in, &mut planes);
            steps += 1;
            if dot(delta, delta) < MIN_MOVE2 || MAX_STEPS <= steps {
                break;
            }
        }
        let inv = 1.0 / ft;
        let mut velocity = scale(vel_disp, inv);
        if slide.stick_to_ground {
            snap_to_ground(&mut planes, &mut pos, &mut velocity);
        }
        out.origin = pos;
        out.velocity = velocity;
        Some(Pass::Done(!planes.is_empty()))
    }

    /// Pushes a stuck body out of what it is inside, where it stands;
    /// `Some(None)` when that fails.
    fn push_out(&self, slide: &Slide, pass: i32, out: &mut SlideResult) -> Option<Option<V3>> {
        let mut body = Character::from_bounds(slide.mins, slide.maxs, 0.0);
        let origin = slide.origin;
        let mut pos = origin;
        let mut planes: Vec<Plane<'w>> = Vec::new();
        let mut touched: Vec<PieceKey> = Vec::new();
        for _ in 0..PUSH_TRIES {
            let sweep = Sweep {
                cap_add: 1.0,
                stop_on_solid: false,
                keep_leaving: true,
                skip: &touched,
            };
            let contacts = self.sweep(&mut body, origin, pos, pass, slide.contentmask, sweep)?;
            if contacts.best.is_none() {
                break;
            }
            for &contact in &contacts.all {
                if planes.len() < MAX_PLANES {
                    planes.push(Plane::new(contact));
                }
                touched.push(PieceKey::of(&contact.piece));
                touch(out, contact.piece.entity);
            }
            for plane in &mut planes {
                plane.rhs = plane.inside(origin);
                if plane.character && plane.rhs > 0.0 {
                    plane.rhs = 0.0;
                }
            }
            let mut push = solve([0.0; 3], &mut planes);
            if planes
                .iter()
                .any(|p| dot(p.normal, push) - p.rhs < PUSH_FAIL)
            {
                return Some(None);
            }
            let length = dot(push, push).sqrt();
            if length <= PUSH_MIN {
                break;
            }
            pos = add(origin, push);
            for _ in 0..PUSH_TRIES {
                for plane in &mut planes {
                    if let Some(contact) = super::cast(
                        &mut body,
                        plane.contact.piece,
                        pos,
                        [0.0; 3],
                        1.0,
                        length + PUSH_REACH,
                        true,
                    ) {
                        *plane = Plane::new(contact);
                    }
                }
                for plane in &mut planes {
                    plane.rhs = plane.inside(origin);
                    if plane.character && plane.rhs > 0.0 {
                        plane.rhs = 0.0;
                    }
                }
                let before = push;
                push = solve([0.0; 3], &mut planes);
                if planes
                    .iter()
                    .any(|p| dot(p.normal, push) - p.rhs < PUSH_FAIL)
                {
                    return Some(None);
                }
                pos = add(origin, push);
                let change = sub(push, before);
                if dot(change, change) <= PUSH_SETTLE2 {
                    break;
                }
            }
        }
        Some(Some(pos))
    }
}

fn touch(out: &mut SlideResult, entity: i32) {
    if out.touched_count < out.touched.len() {
        out.touched[out.touched_count] = entity;
        out.touched_count += 1;
    }
}

/// A body walking on the ground and now above it is moved down onto the
/// nearest walkable plane it met, its velocity laid along that plane.
fn snap_to_ground(planes: &mut [Plane<'_>], pos: &mut V3, velocity: &mut V3) {
    let mut gap = SNAP_FAR;
    for plane in planes.iter_mut() {
        plane.active = false;
        if plane.walkable {
            let sep = -plane.inside(*pos);
            if sep < gap {
                gap = sep;
            }
        }
    }
    if !(SNAP_GAP < gap && gap != SNAP_FAR) {
        return;
    }
    let target = [0.0, 0.0, SNAP_DOWN];
    let mut dir = target;
    let mut tries = 0;
    while !planes.is_empty() {
        let mut chosen = None;
        let mut nearest = 1.0f32;
        for (i, plane) in planes.iter().enumerate() {
            if plane.active {
                continue;
            }
            let along = dot(plane.normal, dir);
            if SNAP_EDGE < along.abs() {
                let t = -plane.inside(*pos) * (-1.0 / along);
                if t < nearest {
                    chosen = Some(i);
                    nearest = t;
                }
            }
        }
        let Some(chosen) = chosen else {
            break;
        };
        planes[chosen].active = true;
        let candidate = add(*pos, scale(dir, nearest));
        let floor = if planes[chosen].walkable {
            Some(chosen)
        } else {
            let mut floor = None;
            let mut closest = SNAP_GAP;
            for (i, plane) in planes.iter().enumerate() {
                if plane.walkable {
                    let sep = -plane.inside(candidate);
                    if sep <= closest {
                        floor = Some(i);
                        closest = sep;
                    }
                }
            }
            floor
        };
        if let Some(floor) = floor {
            if 0.0 < nearest {
                *velocity = along_plane(*velocity, planes[floor].normal);
                *pos = candidate;
            }
            break;
        }
        for plane in planes.iter_mut() {
            plane.rhs = plane.inside(*pos);
            if plane.rhs > 0.0 {
                plane.rhs = 0.0;
            }
        }
        dir = solve(target, planes);
        tries += 1;
        if planes.len() <= tries {
            break;
        }
    }
}

/// `v` laid along a sloped plane: its horizontal part kept in direction,
/// its length kept, unless that would make it longer or turn it upward.
fn along_plane(v: V3, normal: V3) -> V3 {
    let flat = v[0] * v[0] + v[1] * v[1];
    if normal[2].abs() < SNAP_EDGE || flat == 0.0 {
        return v;
    }
    let vz = -((normal[1] * v[1] + normal[0] * v[0]) / normal[2]);
    let s = ((v[2] * v[2] + flat) / (vz * vz + flat)).sqrt();
    if s < 1.0 || vz < 0.0 || 0.0 < v[2] {
        [v[0] * s, v[1] * s, s * vz]
    } else {
        v
    }
}

/// The point nearest `target` on the side of every active plane, found by
/// adding the most violated plane to a set of at most four and keeping the
/// subset whose correction is longest.
fn solve(target: V3, planes: &mut [Plane<'_>]) -> V3 {
    for plane in planes.iter_mut() {
        plane.in_set = false;
    }
    let mut y = target;
    let mut previous = 0.0f32;
    let mut set = 0u32;
    let mut slots = [0usize; 4];
    let mut r = [0.0f32; 4];
    let mut g = [[0.0f32; 4]; 4];
    let mut lambda = [0.0f32; 4];
    let mut rounds = 0;
    while !planes.is_empty() {
        let mut pick = None;
        let mut least = MOST_VIOLATED;
        for (i, plane) in planes.iter().enumerate() {
            if plane.active && !plane.in_set {
                let v = dot(plane.normal, y) - plane.rhs;
                if v < least {
                    pick = Some(i);
                    least = v;
                }
            }
        }
        let Some(pick) = pick else {
            break;
        };
        if 0.0 <= least {
            break;
        }
        let range = if set & 8 != 0 {
            4
        } else if set & 4 != 0 {
            3
        } else if set & 2 != 0 {
            2
        } else {
            (set & 1) as usize
        };
        let k = if set & 1 == 0 {
            0
        } else if set & 2 == 0 {
            1
        } else {
            (((set & 4) | 8) >> 2) as usize
        };
        planes[pick].in_set = true;
        let nk = planes[pick].normal;
        r[k] = planes[pick].rhs - dot(nk, target);
        slots[k] = pick;
        g[k][k] = 1.0;
        for i in 0..range {
            if set & 1 << i != 0 {
                let gi = dot(planes[slots[i]].normal, nk);
                g[k][i] = gi;
                g[i][k] = gi;
            }
        }
        let mut result = 1u32 << k;
        lambda[k] = r[k];
        let mut best = r[k] * r[k];
        for i in 0..range {
            if set & 1 << i == 0 {
                continue;
            }
            let gki = g[k][i];
            let ri = r[i];
            let rk = r[k];
            let a = rk - gki * ri;
            let b = ri - gki * rk;
            let det = 1.0 - gki * gki;
            if 0.0 <= a && 0.0 <= b && SOLVE_DET < det {
                let value = ((gki * b * 2.0 + a) * a + b * b) / (det * det);
                if best < value {
                    result = 1 << i | 1 << k;
                    lambda[k] = (1.0 / det) * a;
                    lambda[i] = (1.0 / det) * b;
                    best = value;
                }
            }
            for j in i + 1..range {
                if set & 1 << j == 0 {
                    continue;
                }
                let rj = r[j];
                let c_k = rj * g[j][i] - g[j][j] * ri;
                let c_i = g[j][j] * rk - rj * g[j][k];
                let c_j = ri * g[j][k] - rk * g[j][i];
                let n_k = c_k * g[i][k] + c_i * g[i][i] + g[i][j] * c_j;
                if n_k < 0.0 {
                    continue;
                }
                let n_i = -(g[k][k] * c_k + gki * c_i + g[k][j] * c_j);
                if n_i < 0.0 {
                    continue;
                }
                let a_k = g[i][j] * gki - g[k][j] * g[i][i];
                let a_j = g[k][k] * g[i][i] - gki * g[i][k];
                let a_i = g[k][j] * g[i][k] - g[i][j] * g[k][k];
                let n_j = a_i * ri + a_k * rk + a_j * rj;
                if n_j < 0.0 {
                    continue;
                }
                let det = a_k * g[j][k] + a_i * g[j][i] + a_j * g[j][j];
                if det <= SOLVE_DET {
                    continue;
                }
                let value = (((gki * n_i + g[k][j] * n_j) * n_k + g[i][j] * n_j * n_i) * 2.0
                    + n_j * n_j
                    + n_i * n_i
                    + n_k * n_k)
                    / (det * det);
                if best < value {
                    let inv = 1.0 / det;
                    lambda[k] = inv * n_k;
                    lambda[i] = inv * n_i;
                    lambda[j] = inv * n_j;
                    result = 1 << j | 1 << i | 1 << k;
                    best = value;
                }
            }
        }
        if best < previous {
            break;
        }
        y = target;
        for (slot, &plane) in slots.iter().enumerate() {
            if result & 1 << slot != 0 {
                y = add(y, scale(planes[plane].normal, lambda[slot]));
            }
        }
        rounds += 1;
        set = result;
        previous = best;
        if planes.len() <= rounds {
            break;
        }
    }
    y
}
