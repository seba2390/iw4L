//! Black Ops' player sweep through its physics world, its default player
//! collision: the player's body cast against the brushes, partitions,
//! entity brushes and bodies near the sweep, nearest first, keeping the
//! earliest contact.

mod gjk;
mod shape;

use core::cell::RefCell;

use game_api::collision::{Placement, SolidPiece, SolidShape};
use game_api::movement::MoveWorld;
use gjk::{CastInput, Contact, Frame, V3, add, dot, scale, sub};
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
        let mut player = Character::from_bounds(mins, maxs, 0.0);
        let (lo, hi) = player.bounds();
        let shape_lo = lo.map(|v| v - BOX_MARGIN);
        let shape_hi = hi.map(|v| v + BOX_MARGIN);
        let delta = sub(end, start);
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
        if !fresh {
            let list_lo = sweep_lo.map(|v| v - LIST_MARGIN);
            let list_hi = sweep_hi.map(|v| v + LIST_MARGIN);
            let mut pieces = listed.take().map(|l| l.pieces).unwrap_or_else(Vec::new);
            pieces.clear();
            if !self
                .world
                .solid_pieces(list_lo, list_hi, mask, pass, &mut pieces)
            {
                return None;
            }
            *listed = Some(Listed {
                mins: list_lo,
                maxs: list_hi,
                mask,
                pass,
                pieces,
            });
        }
        let pieces = &listed.as_ref()?.pieces;
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
        let mut best: Option<(Found, usize)> = None;
        for (enter, index) in order {
            if cap < enter {
                break;
            }
            let piece = &pieces[index];
            let Some(found) = self.cast(&mut player, piece, start, delta, cap) else {
                continue;
            };
            if found.fraction < cap {
                cap = (found.fraction).min(1.0);
            }
            let better = match &best {
                None => true,
                Some((b, _)) => {
                    found.fraction < b.fraction
                        || (found.fraction == 0.0 && found.dist < b.dist)
                }
            };
            if better {
                best = Some((found, index));
            }
            if best.as_ref().is_some_and(|(b, _)| b.dist < 0.0) {
                break;
            }
        }
        let Some((found, index)) = best else {
            return Some(Hit {
                fraction: 1.0,
                normal: [0.0; 3],
                startsolid: false,
                walkable: false,
                surface_flags: 0,
                contents: 0,
                entity: None,
            });
        };
        let piece = &pieces[index];
        let walkable = found.foot
            && (WALKABLE_NORMAL <= found.normal[2] || piece_walkable(piece, found.contact));
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
        Some(Hit {
            fraction: found.fraction,
            normal: found.normal,
            startsolid: found.dist < 0.0,
            walkable,
            surface_flags,
            contents,
            entity: Some(piece.entity),
        })
    }

    fn cast(
        &self,
        player: &mut Character,
        piece: &SolidPiece<'_>,
        start: V3,
        delta: V3,
        cap: f32,
    ) -> Option<Found> {
        let points;
        let indexed;
        let boxed;
        let body;
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
                body = body_shape(mins, maxs);
                (&body, false, ROUNDING)
            }
        };
        player.capsule = !brush;
        let placement = piece.placement.unwrap_or(Placement::at([0.0; 3]));
        let frame = Frame {
            rows: placement.axis,
            origin: sub(placement.origin, start),
        };
        let input = CastInput {
            a: &*player,
            b: other,
            frame,
            delta,
            cap,
            radius_a: player.rounding(),
            radius_b,
            skin: SKIN,
        };
        let contact: Contact = gjk::cast(&input)?;
        let along = dot(contact.normal, delta);
        let dist = dot(contact.normal, sub(contact.point_a, contact.point_b))
            + contact.fraction * along
            + SKIN;
        let fraction = if along < 0.0 || dist <= LEAVING_SLACK {
            contact.fraction
        } else {
            1.0
        };
        if !(fraction < cap || dist < 0.0) {
            return None;
        }
        Some(Found {
            fraction,
            dist,
            normal: contact.normal,
            foot: player.foot(contact.point_a),
            contact: add(start, contact.point_b),
        })
    }
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

struct Found {
    fraction: f32,
    dist: f32,
    normal: V3,
    foot: bool,
    contact: V3,
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
