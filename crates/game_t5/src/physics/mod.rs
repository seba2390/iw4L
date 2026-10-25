//! Black Ops' player sweep through its physics world, its default player
//! collision: the player's body cast against the brushes, partitions,
//! entity brushes and bodies near the sweep, nearest first, keeping the
//! earliest contact.

mod gjk;
mod shape;
mod slide;

use core::cell::RefCell;

use game_api::collision::{Placement, SolidPiece, SolidShape};
use game_api::movement::MoveWorld;
use gjk::{CastInput, Frame, V3, add, dot, scale, sub};
use shape::{BoxShape, Character, Indexed, Points, ROUNDING};

/// How far short of a contact a sweep still counts as touching.
const SKIN: f32 = 0.125;
/// The body's box is widened by this for the broad test.
const BOX_MARGIN: f32 = 0.125;
/// The pieces listed for one sweep reach this far beyond it, so the next
/// sweeps nearby reuse them.
const LIST_MARGIN: f32 = 10.0;
const BROAD_EARLY: f32 = 0.125;
const BROAD_SLACK: f32 = 0.0625;
const LEAVING_SLACK: f32 = 0.0625;
/// A body's box is grown by this before its shape is built.
const BODY_GROWTH: f32 = 1.0;
const WALKABLE_NORMAL: f32 = f32::from_bits(0x3f33_3333);
const ON_PLANE: f32 = f32::from_bits(0x3f7f_be77);
const NOT_OUTSIDE: f32 = -100_000.0;
const MIN_TRIANGLE: f32 = f32::from_bits(0x38d1_b717);
const SIDE_FIRST: f32 = -10.0;
/// The ground sweep weighs contacts within this much height of the
/// earliest.
const GROUND_REACH: f32 = 0.5;
/// A partition contact counts as this much earlier in the ground sweep.
const PARTITION_PREFERENCE: f32 = f32::from_bits(0x3a83_126f);
const NO_FLAGS: u32 = 0x7fff_ffff;
const NO_CAULK: u32 = 0x7fff_fffe;

/// One sweep's result in the game's terms.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Hit {
    pub(crate) fraction: f32,
    pub(crate) normal: V3,
    pub(crate) startsolid: bool,
    pub(crate) walkable: bool,
    pub(crate) surface_flags: u32,
    pub(crate) contents: u32,
    /// `None` when nothing was met.
    pub(crate) entity: Option<i32>,
}

impl Hit {
    const NONE: Self = Self {
        fraction: 1.0,
        normal: [0.0; 3],
        startsolid: false,
        walkable: false,
        surface_flags: 0,
        contents: 0,
        entity: None,
    };
}

/// How a sweep keeps its contacts.
#[derive(Clone, Copy, Debug)]
struct Sweep {
    /// Contacts up to this far past the earliest one are kept.
    cap_add: f32,
    /// The sweep ends at the first contact that starts inside.
    stop_on_solid: bool,
    /// A contact the body moves away from still stops it.
    keep_leaving: bool,
}

impl Sweep {
    const PLAYER: Self = Self {
        cap_add: 0.0,
        stop_on_solid: true,
        keep_leaving: false,
    };
}

/// One contact a sweep met.
#[derive(Clone, Copy, Debug)]
struct Contact<'w> {
    fraction: f32,
    /// How far apart the two are at the contact, short of the skin;
    /// negative when the body starts inside.
    dist: f32,
    normal: V3,
    /// The contact on the body, in its own space.
    point_a: V3,
    /// The contact on what was met, in the world.
    contact: V3,
    /// The contact lies on the body's foot.
    foot: bool,
    piece: SolidPiece<'w>,
}

impl Contact<'_> {
    /// Whether the body may stand on what it met there.
    fn walkable(&self) -> bool {
        self.foot
            && (WALKABLE_NORMAL <= self.normal[2] || piece_walkable(&self.piece, self.contact))
    }

    /// The contact met a player's or an actor's body.
    fn character(&self) -> bool {
        matches!(self.piece.shape, SolidShape::Body { .. })
    }
}

/// Every contact a sweep kept, in the order met, and the earliest.
struct Contacts<'w> {
    all: Vec<Contact<'w>>,
    best: Option<usize>,
}

struct Listed<'w> {
    mins: V3,
    maxs: V3,
    mask: u32,
    pass: i32,
    pieces: Vec<SolidPiece<'w>>,
}

/// The physics world around one player's move.
pub(crate) struct PhysicsWorld<'w> {
    world: &'w dyn MoveWorld,
    caulk: u32,
    listed: RefCell<Option<Listed<'w>>>,
}

impl<'w> PhysicsWorld<'w> {
    pub(crate) fn new(world: &'w dyn MoveWorld) -> Self {
        Self {
            world,
            caulk: world
                .clip_material_surface_flags("caulk")
                .unwrap_or(NO_CAULK),
            listed: RefCell::new(None),
        }
    }

    /// Sweeps the player's body for `mins`..`maxs` from `start` to `end`;
    /// `None` when the match cannot list the pieces.
    pub(crate) fn trace(
        &self,
        start: V3,
        end: V3,
        mins: V3,
        maxs: V3,
        pass: i32,
        mask: u32,
    ) -> Option<Hit> {
        let mut body = Character::from_bounds(mins, maxs, 0.0);
        let contacts = self.sweep(&mut body, start, end, pass, mask, Sweep::PLAYER)?;
        Some(match contacts.best {
            Some(best) => {
                let contact = &contacts.all[best];
                self.hit(contact, contact.walkable())
            }
            None => Hit::NONE,
        })
    }

    /// The ground sweep: of the contacts within half a unit of height of
    /// the earliest, a walkable one is preferred, then the earliest.
    pub(crate) fn ground_trace(
        &self,
        start: V3,
        end: V3,
        mins: V3,
        maxs: V3,
        pass: i32,
        mask: u32,
    ) -> Option<Hit> {
        let drop = start[2] - end[2];
        let sweep = Sweep {
            cap_add: if GROUND_REACH <= drop {
                GROUND_REACH / drop
            } else {
                1.0
            },
            stop_on_solid: false,
            keep_leaving: false,
        };
        let mut body = Character::from_bounds(mins, maxs, 0.0);
        let contacts = self.sweep(&mut body, start, end, pass, mask, sweep)?;
        let Some(best) = contacts.best else {
            return Some(Hit::NONE);
        };
        let height = |f: f32| (end[2] - start[2]) * f + start[2];
        let best_height = height(contacts.all[best].fraction);
        let mut pick = 0;
        let mut pick_fraction = contacts.all[0].fraction;
        let mut pick_walkable = contacts.all[0].walkable();
        for (i, contact) in contacts.all.iter().enumerate().skip(1) {
            let mut fraction = contact.fraction;
            let at = height(fraction);
            if matches!(contact.piece.shape, SolidShape::Partition { .. }) {
                fraction -= PARTITION_PREFERENCE;
            }
            if best_height - at > GROUND_REACH {
                continue;
            }
            if !pick_walkable {
                let walkable = contact.walkable();
                if fraction < pick_fraction || walkable {
                    pick = i;
                    pick_fraction = fraction;
                    pick_walkable = walkable;
                }
            } else if fraction < pick_fraction && contact.walkable() {
                pick = i;
                pick_fraction = fraction;
            }
        }
        Some(self.hit(&contacts.all[pick], pick_walkable))
    }

    fn hit(&self, contact: &Contact<'_>, walkable: bool) -> Hit {
        let piece = &contact.piece;
        let (surface_flags, contents) = match piece.shape {
            SolidShape::Brush {
                axial_surface_flags,
                sides,
                side_surface_flags,
                ..
            } => (
                brush_surface(axial_surface_flags, sides, side_surface_flags, self.caulk),
                piece.contents,
            ),
            SolidShape::Partition { surface_flags, .. } => (surface_flags, piece.contents),
            SolidShape::Body { .. } => (0, 0),
        };
        Hit {
            fraction: contact.fraction,
            normal: contact.normal,
            startsolid: contact.dist < 0.0,
            walkable,
            surface_flags,
            contents,
            entity: Some(piece.entity),
        }
    }

    /// Lists the pieces near a sweep of `body` from `start` to `end`;
    /// `false` when the match cannot list them.
    fn list(&self, body: &Character, start: V3, end: V3, pass: i32, mask: u32) -> bool {
        let (shape_lo, shape_hi) = widened(body);
        let sweep_lo: V3 =
            core::array::from_fn(|i| start[i].min(end[i]) + shape_lo[i] - BOX_MARGIN);
        let sweep_hi: V3 =
            core::array::from_fn(|i| start[i].max(end[i]) + shape_hi[i] + BOX_MARGIN);
        let mut listed = self.listed.borrow_mut();
        let fresh = listed.as_ref().is_some_and(|l| {
            l.mask == mask
                && l.pass == pass
                && (0..3).all(|i| l.mins[i] <= sweep_lo[i] && sweep_hi[i] <= l.maxs[i])
        });
        if fresh {
            return true;
        }
        let list_lo = sweep_lo.map(|v| v - LIST_MARGIN);
        let list_hi = sweep_hi.map(|v| v + LIST_MARGIN);
        let mut pieces = listed.take().map(|l| l.pieces).unwrap_or_else(Vec::new);
        pieces.clear();
        if !self
            .world
            .solid_pieces(list_lo, list_hi, mask, pass, &mut pieces)
        {
            return false;
        }
        *listed = Some(Listed {
            mins: list_lo,
            maxs: list_hi,
            mask,
            pass,
            pieces,
        });
        true
    }

    /// Casts `body` from `start` to `end` against the pieces it may meet,
    /// nearest first.
    fn sweep(
        &self,
        body: &mut Character,
        start: V3,
        end: V3,
        pass: i32,
        mask: u32,
        sweep: Sweep,
    ) -> Option<Contacts<'w>> {
        if !self.list(body, start, end, pass, mask) {
            return None;
        }
        let listed = self.listed.borrow();
        let pieces = &listed.as_ref()?.pieces;
        let (shape_lo, shape_hi) = widened(body);
        let delta = sub(end, start);
        let mut order: Vec<(f32, usize)> = pieces
            .iter()
            .enumerate()
            .filter_map(|(i, piece)| {
                let (piece_lo, piece_hi) = piece_box(piece);
                let lo = sub(piece_lo, shape_hi);
                let hi = sub(piece_hi, shape_lo);
                entry(start, delta, lo, hi).map(|f| (f, i))
            })
            .collect();
        order.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut cap = 1.0f32;
        let mut contacts = Contacts {
            all: Vec::new(),
            best: None,
        };
        for (enter, index) in order {
            if cap < enter {
                break;
            }
            let piece = pieces[index];
            let Some(found) = cast(body, piece, start, delta, cap, SKIN, sweep.keep_leaving)
            else {
                continue;
            };
            if found.fraction < cap {
                cap = (sweep.cap_add + found.fraction).min(1.0);
            }
            let better = match contacts.best {
                None => true,
                Some(b) => {
                    let b = &contacts.all[b];
                    found.fraction < b.fraction || (found.fraction == 0.0 && found.dist < b.dist)
                }
            };
            contacts.all.push(found);
            if better {
                contacts.best = Some(contacts.all.len() - 1);
            }
            if sweep.stop_on_solid
                && contacts
                    .best
                    .is_some_and(|b| contacts.all[b].dist < 0.0)
            {
                break;
            }
        }
        Some(contacts)
    }
}

/// The body's box for the broad test, widened.
fn widened(body: &Character) -> (V3, V3) {
    let (lo, hi) = body.bounds();
    (lo.map(|v| v - BOX_MARGIN), hi.map(|v| v + BOX_MARGIN))
}

/// Casts `body` from `start` along `delta` against `piece`; `None` when it
/// misses or reaches it past `cap`. `reach_skin` widens how far apart the
/// two may be and still be measured.
fn cast<'w>(
    body: &mut Character,
    piece: SolidPiece<'w>,
    start: V3,
    delta: V3,
    cap: f32,
    reach_skin: f32,
    keep_leaving: bool,
) -> Option<Contact<'w>> {
    let points;
    let indexed;
    let boxed;
    let other_body;
    let (other, brush, radius_b): (&dyn gjk::Convex, bool, f32) = match piece.shape {
        SolidShape::Brush {
            mins,
            maxs,
            sides,
            points: hull,
            ..
        } => {
            if sides.is_empty() || hull.is_empty() {
                boxed = BoxShape {
                    center: scale(add(mins, maxs), 0.5),
                    half: scale(sub(maxs, mins), 0.5),
                };
                (&boxed, true, 0.0)
            } else {
                points = Points(hull);
                (&points, true, 0.0)
            }
        }
        SolidShape::Partition {
            verts,
            points: hull,
            ..
        } => {
            if hull.is_empty() {
                return None;
            }
            indexed = Indexed {
                verts,
                points: hull,
            };
            (&indexed, false, 0.0)
        }
        SolidShape::Body { mins, maxs } => {
            other_body = body_shape(mins, maxs);
            (&other_body, false, ROUNDING)
        }
    };
    body.capsule = !brush;
    let placement = piece.placement.unwrap_or(Placement::at([0.0; 3]));
    let frame = Frame {
        rows: placement.axis,
        origin: sub(placement.origin, start),
    };
    let input = CastInput {
        a: &*body,
        b: other,
        frame,
        delta,
        cap,
        radius_a: body.rounding(),
        radius_b,
        skin: reach_skin,
    };
    let contact: gjk::Contact = gjk::cast(&input)?;
    let along = dot(contact.normal, delta);
    let dist = dot(contact.normal, sub(contact.point_a, contact.point_b))
        + contact.fraction * along
        + SKIN;
    let fraction = if along < 0.0 || dist <= LEAVING_SLACK || keep_leaving {
        contact.fraction
    } else {
        1.0
    };
    if !(fraction < cap || dist < 0.0) {
        return None;
    }
    Some(Contact {
        fraction,
        dist,
        normal: contact.normal,
        point_a: contact.point_a,
        contact: add(start, contact.point_b),
        foot: body.foot(contact.point_a),
        piece,
    })
}

fn body_shape(mins: V3, maxs: V3) -> Character {
    Character::from_bounds(
        mins.map(|v| v - BODY_GROWTH),
        maxs.map(|v| v + BODY_GROWTH),
        0.0,
    )
}

/// The piece's box for the broad test: a body's is its shape's.
fn piece_box(piece: &SolidPiece<'_>) -> (V3, V3) {
    match piece.shape {
        SolidShape::Body { mins, maxs } => {
            let origin = piece.placement.map_or([0.0; 3], |p| p.origin);
            let (lo, hi) = body_shape(mins, maxs).bounds();
            (add(origin, lo), add(origin, hi))
        }
        _ => (piece.mins, piece.maxs),
    }
}

/// The fraction at which the sweep from `start` along `delta` enters the box
/// `lo`..`hi`, reaching for it a little early; `None` when it misses.
fn entry(start: V3, delta: V3, lo: V3, hi: V3) -> Option<f32> {
    let mut enter = 0.0f32;
    let mut exit = 1.0f32;
    for i in 0..3 {
        let a = start[i] - hi[i];
        if !slab_side(a, delta[i] + a, &mut enter, &mut exit) {
            return None;
        }
        let a = -(start[i] - lo[i]);
        if !slab_side(a, a - delta[i], &mut enter, &mut exit) {
            return None;
        }
    }
    Some(enter)
}

/// One face of the box: `a` how far the start lies outside it, `b` how far
/// the end does.
fn slab_side(a: f32, b: f32, enter: &mut f32, exit: &mut f32) -> bool {
    if a <= 0.0 {
        if 0.0 < b && a - BROAD_SLACK < (a - b) * *exit {
            *exit = (a - BROAD_SLACK) / (a - b);
            if *exit <= *enter {
                return false;
            }
        }
    } else {
        let lim = if a < BROAD_EARLY { a } else { BROAD_EARLY };
        if b < lim {
            if (a - b) * *enter < a - BROAD_EARLY {
                *enter = (a - BROAD_EARLY) / (a - b);
                if *exit <= *enter {
                    return false;
                }
            }
        } else if BROAD_SLACK < a {
            return false;
        }
    }
    true
}

/// The surface a brush hit reports: its top's, unless that is caulk; then
/// its most upward side that is not, else its first axial side that is not.
fn brush_surface(axial: [u32; 6], sides: &[[f32; 4]], side_flags: &[u32], caulk: u32) -> u32 {
    if axial[5] != caulk {
        return axial[5];
    }
    let mut result = NO_FLAGS;
    let mut best = SIDE_FIRST;
    for (plane, &flags) in sides.iter().zip(side_flags) {
        if flags != caulk {
            let up = (plane[1] + plane[0]) * 0.0 + plane[2];
            if best < up {
                result = flags;
                best = up;
            }
        }
    }
    if result != NO_FLAGS {
        return result;
    }
    axial.into_iter().find(|&f| f != caulk).unwrap_or(caulk)
}

/// Whether `point` lies on a plane facing up enough to stand on, or else the
/// plane it is most outside of faces up enough.
struct Footing {
    worst: f32,
    worst_up: f32,
}

impl Footing {
    fn new() -> Self {
        Self {
            worst: NOT_OUTSIDE,
            worst_up: 0.0,
        }
    }

    /// `true` when the point lies on a walkable plane.
    fn plane(&mut self, dist: f32, point_dist: f32, up: f32) -> bool {
        if (dist * ON_PLANE <= point_dist || dist <= point_dist * ON_PLANE) && WALKABLE_NORMAL <= up
        {
            return true;
        }
        if self.worst < point_dist - dist {
            self.worst = point_dist - dist;
            self.worst_up = up;
        }
        false
    }

    fn outside_walkable(&self) -> bool {
        WALKABLE_NORMAL <= self.worst_up
    }
}

fn piece_walkable(piece: &SolidPiece<'_>, contact: V3) -> bool {
    let (point, up) = match piece.placement {
        Some(p) => {
            let rel = sub(contact, p.origin);
            (
                [dot(p.axis[0], rel), dot(p.axis[1], rel), dot(p.axis[2], rel)],
                [p.axis[0][2], p.axis[1][2], p.axis[2][2]],
            )
        }
        None => (contact, [0.0, 0.0, 1.0]),
    };
    match piece.shape {
        SolidShape::Brush {
            mins, maxs, sides, ..
        } => {
            let mut footing = Footing::new();
            for i in 0..3 {
                if footing.plane(-mins[i], -point[i], -up[i])
                    || footing.plane(maxs[i], point[i], up[i])
                {
                    return true;
                }
            }
            for plane in sides {
                let n = [plane[0], plane[1], plane[2]];
                if footing.plane(plane[3], dot(point, n), dot(up, n)) {
                    return true;
                }
            }
            footing.outside_walkable()
        }
        SolidShape::Partition { verts, tris, .. } => {
            if tris.is_empty() {
                return false;
            }
            let vert = |i: u32| verts.get(i as usize).copied().unwrap_or([0.0; 3]);
            let mut footing = Footing::new();
            for tri in tris.chunks_exact(3) {
                let v0 = vert(tri[0]);
                let a = sub(v0, vert(tri[1]));
                let b = sub(v0, vert(tri[2]));
                let n = [
                    a[2] * b[1] - a[1] * b[2],
                    a[0] * b[2] - a[2] * b[0],
                    a[1] * b[0] - a[0] * b[1],
                ];
                let length = dot(n, n).sqrt();
                if MIN_TRIANGLE < length {
                    let inv = 1.0 / length;
                    if footing.plane(dot(v0, n) * inv, dot(point, n) * inv, dot(up, n) * inv) {
                        return true;
                    }
                }
            }
            footing.outside_walkable()
        }
        SolidShape::Body { .. } => false,
    }
}
