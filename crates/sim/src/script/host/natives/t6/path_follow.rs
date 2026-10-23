//! Black Ops 2's path follower: the path an actor walks, kept as the engine
//! keeps it (its points from the end back to where the actor started, each
//! with the flat direction and length of the segment that ends there), and
//! the lookahead the actor steers by. The lookahead point lies along the
//! path where the area between the path and the straight line from the
//! actor (each segment's length times the actor's distance from its line)
//! reaches the lookahead amount; the amount grows while the point can be
//! walked to and shrinks when it cannot. What IW4L does not do of it is
//! listed under `t6.actors.path_following` in `docs/fidelity/t6.md`.

/// The most points a path keeps; a longer one ends at its 32nd node.
const MAX_POINTS: usize = 32;
/// The lookahead amount a new actor's path starts with.
pub(super) const START_AMOUNT: f32 = 32_768.0;
/// The amount bounds and steps.
const MAX_AMOUNT: f32 = 65_536.0;
const MIN_AMOUNT: f32 = 0.001;
const GROW_OWNED: f32 = 2.352_941;
const GROW_STEP: f32 = 6.4;
const SHRINK: f32 = 0.8;
const SHRINK_DODGE: f32 = 0.5625;
const SHRINK_HELD_OK: f32 = 0.75;
/// Below this amount the completed part of the path is trimmed even when
/// the lookahead is not settled; above the other a trim keeps it.
const TRIM_AMOUNT: f32 = 16_384.0;
const FLOOR_AMOUNT: f32 = 64.0;
/// How far behind the actor's plane a trimmed point stays.
const TRIM_EPSILON: f32 = 0.0001;
/// A path this short (or shorter) that was cut at 32 points is planned
/// again from where the actor stands.
const SHORT_TRUNCATED: usize = 8;

/// `flags`: the lookahead reached the path's end, the lookahead was walked
/// to (and grew twice), the path was cut at 32 points.
const REACHED_END: u32 = 0x41;
const SETTLED: u32 = 0x2;
const SETTLED_TWICE: u32 = 0x400;
const TRUNCATED: u32 = 0x4;
const HELD: u32 = 0x1000;
const CLEARED_EACH_UPDATE: u32 = 0x8020;
const SHRINK_CLEARS: u32 = 0x403;

/// A point of the path: where it is, the flat direction and length of the
/// segment that ends at it, and the node it is when it is one.
#[derive(Clone, Copy, Debug)]
pub(super) struct Point {
    pub(super) origin: [f32; 3],
    dir: [f32; 2],
    length: f32,
    pub(super) node: Option<u16>,
}

/// The lookahead state the engine keeps on the actor across its paths.
#[derive(Clone, Copy, Debug)]
pub(super) struct Lookahead {
    pub(super) dir: [f32; 3],
    pub(super) dist: f32,
    pub(super) amount: f32,
}

impl Default for Lookahead {
    fn default() -> Self {
        Self {
            dir: [0.0; 3],
            dist: 0.0,
            amount: START_AMOUNT,
        }
    }
}

/// The path an actor follows (`path_t`).
#[derive(Clone, Debug)]
pub(super) struct Follow {
    /// `pts[0]` is the path's end, `pts[len - 1]` where it started.
    pts: Vec<Point>,
    len: usize,
    orig_len: usize,
    /// The index of the first traversal's begin node (0: none).
    pub(super) negotiation_start: usize,
    next: usize,
    to_next: f32,
    flags: u32,
    /// Where the actor's current segment starts, and its length left.
    curr: [f32; 3],
    curr_length: f32,
    /// The last lookahead point the actor could walk to.
    saved: Option<[f32; 3]>,
    pub(super) look: Lookahead,
}

fn flat_len(v: [f32; 2]) -> f32 {
    (v[0] * v[0] + v[1] * v[1]).sqrt()
}

/// The engine's guard: a length that is not positive divides as 1.
fn safe(len: f32) -> f32 {
    if -len >= 0.0 { 1.0 } else { len }
}

/// What the lookahead asks of the world: whether the actor can walk from
/// `from` to `to`, and whether the link between two nodes still stands.
pub(super) trait Probe {
    fn walkable(&mut self, from: [f32; 3], to: [f32; 3]) -> bool;
    fn linked(&mut self, from: u16, to: u16) -> bool;
}

impl Follow {
    /// The path through `route` (node origins from the start on, then the
    /// goal), as the engine builds it: at most 32 points from the start,
    /// the first traversal's begin node marked, and the actor's lookahead
    /// carried over and set for the new path (`initial`).
    pub(super) fn new(
        route: &[([f32; 3], Option<u16>)],
        negotiation: Option<usize>,
        look: Lookahead,
        pos: [f32; 3],
        probe: &mut dyn Probe,
    ) -> Self {
        let count = route.len().min(MAX_POINTS);
        let truncated = route.len() > MAX_POINTS;
        // Points run from the end (index 0) back to the start.
        let mut pts: Vec<Point> = route[..count]
            .iter()
            .rev()
            .map(|&(origin, node)| Point {
                origin,
                dir: [0.0; 2],
                length: 0.0,
                node,
            })
            .collect();
        for i in 0..count.saturating_sub(1) {
            let d = [
                pts[i].origin[0] - pts[i + 1].origin[0],
                pts[i].origin[1] - pts[i + 1].origin[1],
            ];
            let length = flat_len(d);
            pts[i].dir = [d[0] / safe(length), d[1] / safe(length)];
            pts[i].length = length;
        }
        // The begin node's index counted from the end.
        let negotiation_start = negotiation
            .filter(|&index| index < count)
            .map_or(0, |index| count - 1 - index);
        let curr = pts[count - 1].origin;
        let curr_length = if count >= 2 {
            pts[count - 2].length
        } else {
            0.0
        };
        let mut follow = Self {
            pts,
            len: count,
            orig_len: count,
            negotiation_start,
            next: 0,
            to_next: 0.0,
            flags: if truncated { TRUNCATED } else { 0 },
            curr,
            curr_length,
            saved: None,
            look,
        };
        if follow.look.amount != 0.0 {
            follow.initial(pos, probe);
        }
        follow
    }

    /// The points left ahead of the actor, nearest first: the end of its
    /// current segment on, or the begin node it is heading straight for.
    pub(super) fn ahead(&self) -> impl Iterator<Item = &Point> {
        let first = if self.negotiation_start + 1 == self.len {
            self.len - 1
        } else {
            self.len.saturating_sub(2)
        };
        self.pts[..=first.min(self.len - 1)].iter().rev()
    }

    /// Whether the path was cut at 32 points.
    pub(super) fn truncated(&self) -> bool {
        self.flags & TRUNCATED != 0
    }

    fn seg_len(&self, i: usize) -> f32 {
        if i + 2 == self.len {
            self.curr_length
        } else {
            self.pts[i].length
        }
    }

    fn shrink(&mut self, amount: f32) {
        let factor = if self.flags & HELD != 0 {
            if self.flags & SETTLED != 0 {
                SHRINK_HELD_OK
            } else {
                SHRINK_DODGE
            }
        } else {
            // No dodge entity.
            SHRINK
        };
        self.look.amount = (amount * factor).max(MIN_AMOUNT);
        self.flags &= !SHRINK_CLEARS;
    }

    /// The amount grows while the lookahead can be walked to; the actor
    /// owns its path, so it grows by the larger step.
    fn grow(&mut self) {
        self.look.amount = (self.look.amount * GROW_OWNED + GROW_STEP).min(MAX_AMOUNT);
        self.flags |= if self.flags & SETTLED != 0 {
            SETTLED_TWICE
        } else {
            SETTLED
        };
    }

    /// Points the lookahead at `point` from `pos` (flat, its height as a
    /// slope).
    fn aim(&mut self, point: [f32; 3], pos: [f32; 3]) {
        let d = [point[0] - pos[0], point[1] - pos[1], point[2] - pos[2]];
        let len = flat_len([d[0], d[1]]);
        let scale = 1.0 / safe(len);
        self.look.dir = [
            d[0] * scale,
            d[1] * scale,
            if len == 0.0 { 0.0 } else { d[2] / len },
        ];
        self.look.dist = len;
    }

    /// `Path_SetLookahead`: the lookahead to `point`, the `t` along the
    /// segment ending at point `idx`; the amount grows when the actor can
    /// walk there, else the last point it could is kept and the amount
    /// shrinks.
    fn set_lookahead(
        &mut self,
        point: [f32; 3],
        pos: [f32; 3],
        (force, t, idx, amount): (bool, f32, usize, f32),
        probe: &mut dyn Probe,
    ) {
        if self.next + 1 >= self.len {
            self.to_next = 0.0;
            self.next = self.len - 1;
        }
        let aim;
        if force {
            self.shrink(amount);
            aim = point;
            self.saved = None;
        } else if probe.walkable(pos, point) {
            self.saved = Some(point);
            aim = point;
            self.grow();
        } else {
            aim = match self.saved {
                Some(saved) if (pos[0] - saved[0]).powi(2) + (pos[1] - saved[1]).powi(2) > 1.0 => {
                    saved
                }
                _ => {
                    self.saved = None;
                    point
                }
            };
            let seg = self.seg_len(self.next);
            if self.flags & SETTLED != 0 && self.next < self.len && seg >= self.to_next {
                self.shrink(amount);
                return;
            }
            self.shrink(amount);
        }
        self.aim(aim, pos);
        self.to_next = t;
        self.next = idx;
    }

    /// `Path_LookaheadStep`: walks the path from the actor's segment on
    /// until the area reaches the amount, and sets the lookahead there (or
    /// at the path's end, or its first traversal's begin node).
    fn step(&mut self, pos: [f32; 3], force: bool, probe: &mut dyn Probe) {
        let amount = self.look.amount;
        let mut area = 0.0f32;
        let last = self.len as isize - 2;
        let mut i = last;
        let mut first = true;
        while i >= self.negotiation_start as isize {
            let k = i as usize;
            let p = self.pts[k];
            let perp =
                ((p.origin[1] - pos[1]) * p.dir[0] - p.dir[1] * (p.origin[0] - pos[0])).abs();
            let seg = if first { self.curr_length } else { p.length };
            area += seg * perp;
            if perp > 0.0 && area >= amount {
                let t = ((area - amount) / perp).min(seg);
                let prev = if first {
                    self.curr
                } else {
                    self.pts[k + 1].origin
                };
                let point = [
                    p.origin[0] - t * p.dir[0],
                    p.origin[1] - t * p.dir[1],
                    p.origin[2] - (p.origin[2] - prev[2]) * t / seg,
                ];
                self.flags &= !REACHED_END;
                self.set_lookahead(point, pos, (force, t, k, amount), probe);
                return;
            }
            first = false;
            i -= 1;
        }
        self.flags |= REACHED_END;
        let point = if self.len - 1 == self.negotiation_start {
            self.curr
        } else {
            self.pts[self.negotiation_start].origin
        };
        let start = self.negotiation_start;
        self.set_lookahead(point, pos, (force, 0.0, start, area), probe);
        self.look.amount = area.max(START_AMOUNT).min(self.look.amount);
    }

    /// `Path_TrimCompleted`: moves the actor's current point up to where
    /// the plane through it across the lookahead meets the path, dropping
    /// the points behind.
    fn trim(&mut self, pos: [f32; 3]) {
        let along =
            |p: [f32; 3], dir: [f32; 3]| (p[0] - pos[0]) * dir[0] + (p[1] - pos[1]) * dir[1];
        let dir = self.look.dir;
        let mut i = self.len as isize - 2;
        let mut first = true;
        let ahead = loop {
            if i < self.negotiation_start as isize {
                return;
            }
            let d = along(self.pts[i as usize].origin, dir);
            if d > 0.0 {
                break d;
            }
            i -= 1;
            first = false;
        };
        let k = i as usize;
        let (seg, from) = if first {
            (self.curr_length, self.curr)
        } else {
            (self.pts[k].length, self.pts[k + 1].origin)
        };
        let behind = along(from, dir) - TRIM_EPSILON;
        let f = behind / (behind - ahead);
        let moved = f * seg;
        let p = self.pts[k];
        if moved < seg {
            self.curr = [
                p.dir[0] * moved + from[0],
                p.dir[1] * moved + from[1],
                (p.origin[2] - from[2]) * f + from[2],
            ];
            self.curr_length = seg - moved;
            self.len = k + 2;
        } else if self.negotiation_start < k {
            self.curr = p.origin;
            self.curr_length = self.pts[k - 1].length;
            self.len = k + 1;
        }
    }

    /// `Path_AdjustLookaheadAmount`: the area of the part of the path the
    /// actor completed comes off the amount (at least 64 stays).
    fn completed_area(&mut self, pos: [f32; 3]) {
        if self.len == self.orig_len || self.len < 2 {
            return;
        }
        let area_of = |p: &Point| {
            ((p.origin[1] - pos[1]) * p.dir[0] - p.dir[1] * (p.origin[0] - pos[0])).abs()
        };
        let k = self.len - 2;
        let mut area = area_of(&self.pts[k]) * (self.pts[k].length - self.curr_length);
        for j in self.len - 1..self.orig_len - 1 {
            area += area_of(&self.pts[j]) * self.pts[j].length;
        }
        self.look.amount = (self.look.amount - area).max(FLOOR_AMOUNT);
    }

    /// `Path_UpdateLookahead`. The actor at `pos`; `adjust` takes the area
    /// of the completed path off the amount. With `may_replan`, returns
    /// false (and leaves the lookahead) once the actor is past its current
    /// point and the link it walks has been cut, or its cut-short path runs
    /// low: the engine plans again from there, and goes on with this path
    /// when that fails.
    pub(super) fn update(
        &mut self,
        pos: [f32; 3],
        (force, adjust, may_replan): (bool, bool, bool),
        probe: &mut dyn Probe,
    ) -> bool {
        self.flags &= !CLEARED_EACH_UPDATE;
        if self.negotiation_start + 1 == self.len {
            self.aim(self.curr, pos);
            self.flags |= REACHED_END;
            self.to_next = 0.0;
            self.next = self.negotiation_start;
            return true;
        }
        let d =
            (self.curr[0] - pos[0]) * self.look.dir[0] + (self.curr[1] - pos[1]) * self.look.dir[1];
        if d <= 0.0 {
            let cut = match (self.pts[self.len - 1].node, self.pts[self.len - 2].node) {
                (Some(from), Some(to)) => !probe.linked(from, to),
                _ => false,
            };
            if may_replan && (cut || (self.truncated() && self.len <= SHORT_TRUNCATED)) {
                return false;
            }
            if self.flags & SETTLED != 0 || self.look.amount <= TRIM_AMOUNT {
                self.trim(pos);
            }
            if adjust {
                self.completed_area(pos);
            }
        }
        self.step(pos, force, probe);
        true
    }

    /// `Path_InitialLookahead`: on a new path, the lookahead the actor
    /// had is kept when the new path bends away from it no more than the
    /// area allows, else it turns to the path's first point.
    fn initial(&mut self, pos: [f32; 3], probe: &mut dyn Probe) {
        if self.look.dist == 0.0 {
            self.look_at_curr(pos, false, probe);
            return;
        }
        let amount = self.look.amount;
        let [lx, ly, _] = self.look.dir;
        let v = [self.curr[0] - pos[0], self.curr[1] - pos[1]];
        let len = flat_len(v);
        let u = [v[0] / safe(len), v[1] / safe(len)];
        let mut side = (u[1] * lx - ly * u[0]) * len;
        let mut best = lx * u[0] + u[1] * ly;
        let mut best_area = 0.0f32;
        let mut area = 0.0f32;
        let mut first = true;
        let mut i = self.len as isize - 2;
        while i >= self.negotiation_start as isize {
            let p = self.pts[i as usize];
            let w = [p.origin[0] - pos[0], p.origin[1] - pos[1]];
            let seg = if first { self.curr_length } else { p.length };
            let perp = w[1] * p.dir[0] - p.dir[1] * w[0];
            let crosses = (ly * p.dir[0] - p.dir[1] * lx) * perp > 0.0;
            let q = w[1] * lx - w[0] * ly;
            if crosses && q * side < 0.0 {
                self.update(pos, (false, true, false), probe);
                return;
            }
            area += perp.abs() * seg;
            if area >= amount {
                let t = -((area - amount) / perp.abs());
                let to = [
                    p.dir[0] * t + p.origin[0] - pos[0],
                    p.dir[1] * t + p.origin[1] - pos[1],
                ];
                let n = 1.0 / safe(flat_len(to));
                if best > to[0] * n * lx + to[1] * n * ly {
                    self.keep(pos, best_area, probe);
                } else {
                    self.look_at_curr(pos, false, probe);
                }
                return;
            }
            side = q;
            let n = 1.0 / safe(flat_len(w));
            let d = w[1] * n * ly + w[0] * n * lx;
            if d >= best {
                best = d;
                best_area = area;
            }
            first = false;
            i -= 1;
        }
        if best_area != area {
            self.keep(pos, best_area, probe);
        } else {
            self.look_at_curr(pos, false, probe);
        }
    }

    fn keep(&mut self, pos: [f32; 3], best_area: f32, probe: &mut dyn Probe) {
        self.look_at_curr(pos, true, probe);
        if self.flags & SETTLED == 0 && self.look.amount > best_area {
            self.look.amount = best_area.max(FLOOR_AMOUNT);
        }
    }

    /// The lookahead at the actor's current point, then one update.
    fn look_at_curr(&mut self, pos: [f32; 3], adjust: bool, probe: &mut dyn Probe) {
        self.aim(self.curr, pos);
        self.to_next = 0.0;
        self.next = self.len - 1;
        self.update(pos, (false, adjust, false), probe);
    }
}
