//! Black Ops' physics shapes: a character (a player's or an actor's body),
//! a brush's corner points or its box, a collision partition's vertices.

use super::gjk::{Convex, V3};

/// How far a character's core sits inside its box, and how much it is
/// rounded beyond that core.
const INSET: f32 = 0.125;
pub(super) const ROUNDING: f32 = 0.5;
const TOP_CAP: f32 = 10.0;
const BOTTOM_CAP: f32 = 2.0;
const SECTORS_PER_RADIAN: f32 = f32::from_bits(0x3ff4_7645);
const TWO_PI: f32 = f32::from_bits(0x40c9_0fdb);
/// The prism's side faces lie on the core's radius, so its corners reach
/// past it by 1 / cos(15°).
const PRISM_HALF_ANGLE: f64 = 0.261_799_395_084_381_1;
const QUADRANT_STEP: [i32; 4] = [1, -1, 1, -1];
const QUADRANT_BASE: [i32; 4] = [0, 6, -6, 12];
const QUADRANT_X: [f32; 4] = [1.0, -1.0, -1.0, 1.0];
const QUADRANT_Y: [f32; 4] = [1.0, 1.0, -1.0, -1.0];

fn ring_table() -> &'static ([f32; 4], [f32; 4]) {
    static TABLE: std::sync::OnceLock<([f32; 4], [f32; 4])> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let step = f32::from_bits(0x3f06_0a91);
        let mut cos = [0.0; 4];
        let mut sin = [0.0; 4];
        for i in 0..4 {
            let angle = f64::from(i as f32 * step);
            cos[i] = angle.cos() as f32;
            sin[i] = angle.sin() as f32;
        }
        (cos, sin)
    })
}

fn prism_scale() -> f32 {
    1.0 / (PRISM_HALF_ANGLE.cos() as f32)
}

/// A character: a twelve-sided prism with a low cone below and a tall one
/// above when it meets a brush, a capsule when it meets anything else; both
/// rounded by [`ROUNDING`].
#[derive(Clone, Copy, Debug)]
pub(super) struct Character {
    pub(super) center: f32,
    pub(super) radius: f32,
    prism_radius: f32,
    pub(super) half_height: f32,
    top: f32,
    pub(super) bottom: f32,
    pub(super) capsule: bool,
}

impl Character {
    /// Built from a box, its core raised by `adjust`.
    pub(super) fn from_bounds(mins: V3, maxs: V3, adjust: f32) -> Self {
        let e = ROUNDING - INSET;
        let zlo = if maxs[2] < mins[2] + e {
            maxs[2]
        } else {
            mins[2] + e
        };
        let xlo = if maxs[0] < mins[0] + e {
            maxs[0]
        } else {
            mins[0] + e
        };
        let zhi = if maxs[2] - e < mins[2] {
            mins[2]
        } else {
            maxs[2] - e
        };
        let xhi = if mins[0] <= maxs[0] - e {
            maxs[0] - e
        } else {
            mins[0]
        };
        let height = zhi - zlo;
        let width = if height < xhi - xlo {
            height
        } else {
            xhi - xlo
        };
        let radius = (width * 0.5).max(1.0);
        let half_height = (height * 0.5).max(1.0);
        Self {
            center: (zhi + zlo) * 0.5 + adjust,
            radius,
            prism_radius: prism_scale() * radius,
            half_height,
            top: half_height.min(TOP_CAP),
            bottom: half_height.min(BOTTOM_CAP),
            capsule: false,
        }
    }

    /// Its rounding: a capsule's is its radius as well.
    pub(super) fn rounding(&self) -> f32 {
        if self.capsule {
            ROUNDING + self.radius
        } else {
            ROUNDING
        }
    }

    /// Its box in its own space.
    pub(super) fn bounds(&self) -> (V3, V3) {
        let r = self.prism_radius + ROUNDING;
        (
            [-r, -r, self.center - self.half_height - ROUNDING],
            [r, r, self.center + self.half_height + ROUNDING],
        )
    }

    /// Whether a contact at `point` (its own space) lies on its foot.
    pub(super) fn foot(&self, point: V3) -> bool {
        let base = self.center - self.half_height;
        if self.capsule {
            point[2] <= base + self.radius - 1.0
        } else {
            let lip = (f64::from(core::f32::consts::FRAC_PI_6).sin() as f32) * ROUNDING;
            point[2] <= base + self.bottom - lip
        }
    }
}

impl Convex for Character {
    fn support(&self, d: V3) -> V3 {
        if self.capsule {
            let half = self.half_height - self.radius;
            let top = self.center + half;
            let bottom = self.center - half;
            return [
                0.0,
                0.0,
                if 0.0 <= d[2] * (top - bottom) {
                    top
                } else {
                    bottom
                },
            ];
        }
        let mut angle = f64::from(d[1]).atan2(f64::from(d[0])) as f32;
        if angle < 0.0 {
            angle += TWO_PI;
        }
        let sector = (angle * SECTORS_PER_RADIAN + 0.5) as i32 % 12;
        let quadrant = (sector / 3) as usize;
        let index = (QUADRANT_STEP[quadrant] * sector + QUADRANT_BASE[quadrant]) as usize;
        let (cos, sin) = ring_table();
        let x = cos[index] * QUADRANT_X[quadrant] * self.prism_radius;
        let y = sin[index] * QUADRANT_Y[quadrant] * self.prism_radius;
        let (ring, apex) = if d[2] >= 0.0 {
            (
                self.center + (self.half_height - self.top),
                self.center + self.half_height,
            )
        } else {
            (
                self.center + (self.bottom - self.half_height),
                self.center - self.half_height,
            )
        };
        if 0.0 <= d[2] * (ring - apex) + d[0] * x + d[1] * y {
            [x, y, ring]
        } else {
            [0.0, 0.0, apex]
        }
    }

    fn center(&self) -> V3 {
        [0.0, 0.0, self.center]
    }
}

/// A brush's corner points.
pub(super) struct Points<'a>(pub(super) &'a [[f32; 3]]);

impl Convex for Points<'_> {
    fn support(&self, d: V3) -> V3 {
        let mut best = self.0[0];
        let mut best_dot = super::gjk::dot(best, d);
        for &p in &self.0[1..] {
            let dot = super::gjk::dot(p, d);
            if best_dot < dot {
                best = p;
                best_dot = dot;
            }
        }
        best
    }

    fn center(&self) -> V3 {
        [0.0; 3]
    }
}

/// A partition's distinct vertices.
pub(super) struct Indexed<'a> {
    pub(super) verts: &'a [[f32; 3]],
    pub(super) points: &'a [u16],
}

impl Convex for Indexed<'_> {
    fn support(&self, d: V3) -> V3 {
        let vert = |i: u16| self.verts.get(usize::from(i)).copied().unwrap_or([0.0; 3]);
        let mut best = vert(self.points[0]);
        let mut best_dot = super::gjk::dot(best, d);
        for &i in &self.points[1..] {
            let p = vert(i);
            let dot = super::gjk::dot(p, d);
            if best_dot < dot {
                best = p;
                best_dot = dot;
            }
        }
        best
    }

    fn center(&self) -> V3 {
        [0.0; 3]
    }
}

/// A brush kept as its box.
pub(super) struct BoxShape {
    pub(super) center: V3,
    pub(super) half: V3,
}

impl Convex for BoxShape {
    fn support(&self, d: V3) -> V3 {
        let sx = if 0.0 <= d[0] { 1.0 } else { -1.0 };
        let sy = if d[1] < 0.0 { -1.0 } else { 1.0 };
        let sz = if d[2] < 0.0 { -1.0 } else { 1.0 };
        [
            self.center[0] + self.half[0] * sx,
            self.center[1] + self.half[1] * sy,
            self.center[2] + self.half[2] * sz,
        ]
    }

    fn center(&self) -> V3 {
        self.center
    }
}
