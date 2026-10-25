//! Black Ops' actor path (`path_t`): the points an actor follows, kept from
//! the path's end (index 0) back to where it started, each with the flat
//! direction and length of the segment that ends at it, and the lookahead
//! the actor steers by. The lookahead point lies along the path where the
//! area between the path and the straight line from the actor (each
//! segment's length times the actor's distance from its line) reaches the
//! lookahead amount; the amount grows while the point can be stepped to and
//! shrinks when it cannot. The state lives on the actor across its paths.
//! What is not ported is listed under "AI and actors" in
//! `docs/fidelity/t5.md`.

/// The most points a path keeps; a longer one ends at its 32nd node.
const MAX_POINTS: usize = 32;

/// `flags`: the lookahead reached the path's end or its first traversal.
const AT_END: u32 = 0x1;
/// The lookahead could be stepped to (and grew); twice in a row it also
/// turns on the forward lookahead.
const GREW: u32 = 0x2;
const GREW_TWICE: u32 = 0x200;
/// The path was cut at 32 points.
const TRUNCATED: u32 = 0x4;
/// The path may cross traversals.
const NEGOTIATION: u32 = 0x10;
/// The backtrack stopped 256 units out.
const BACKTRACK_FAR: u32 = 0x20;
/// The lookahead is the path's end (cleared with `AT_END` when a point
/// along the path is found).
const LOOK_AT_END: u32 = 0x40;
/// Planned again from where the actor stood after a block or a traversal.
pub(crate) const REPLANNED: u32 = 0x80;
/// Planned again for a script move (the start with two lookahead nodes).
const REPLANNED_SCRIPT: u32 = 0x100;

/// The lookahead amount bounds and steps.
const MAX_AMOUNT: f32 = 65_536.0;
const MIN_AMOUNT: f32 = 0.001;
const FLOOR_AMOUNT: f32 = 64.0;
const END_AMOUNT: f32 = 21_845.334;
const SCRIPT_REPLAN_AMOUNT: f32 = 4096.0;
const TRIM_AMOUNT: f32 = 16_384.0;
const NON_CODE_MOVE_AMOUNT: f32 = 1024.0;
const GROW_SCALE: f32 = 1.176_470_5;
const GROW_STEP: f32 = 6.4;
const SHRINK: f32 = 0.5625;
const SHRINK_GREW: f32 = 0.75;
/// The share of a backtrack's area the amount takes unless it just grew.
const BACKTRACK_SHARE: f32 = 0.75;
/// The share of the amount a backtrack may walk back over.
const BACKTRACK_LIMIT: f32 = 0.176_470_55;
/// How far back (squared) a backtrack looks.
const BACKTRACK_RANGE_SQ: f32 = 65_536.0;
/// How far behind the actor's plane a trimmed point stays.
const TRIM_EPSILON: f32 = 0.0001;
/// A truncated path this short is planned again.
const SHORT_TRUNCATED: usize = 9;
/// How far ahead (at most) the forward lookahead starts.
const FORWARD_REACH: f32 = 60.0;
/// A move shorter than this (squared) leaves a non-code move's path as it
/// is.
const NON_CODE_MOVE_MIN_SQ: f32 = 9.0;
/// The corner a two-node script lookahead turns into three.
const CORNER_STRAIGHT: f32 = 0.866;
const CORNER_TURN: f32 = 0.173;

/// A point of the path: where it is, the flat direction and length of the
/// segment that ends at it (from the next point toward this one), and the
/// node it is when it is one.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Point {
    pub(crate) origin: [f32; 3],
    dir: [f32; 2],
    len: f32,
    pub(crate) node: Option<u16>,
}

/// What the path asks of the world.
pub(crate) trait Probe {
    /// Whether the actor's box steps from `from` to `to` (the lookahead
    /// trace), with the entity it dodges when it has one.
    fn clear(&mut self, from: [f32; 3], to: [f32; 3], dodge: Option<u16>) -> bool;
    /// Whether a node still has a link standing.
    fn linked(&mut self, node: u16) -> bool;
}

/// The actor's path and lookahead.
#[derive(Clone, Debug)]
pub(crate) struct ActorPath {
    /// `pts[0]` is the path's end, `pts[len - 1]` where the actor is
    /// heading from; the points past `len` (up to `orig_len`) were passed.
    pts: Vec<Point>,
    pub(crate) len: usize,
    orig_len: usize,
    /// Below zero once the actor bumped into the entity it dodges.
    dodge_count: i32,
    /// The index of the first traversal's begin node (0: none).
    pub(crate) negotiation_start: usize,
    next: usize,
    pub(crate) dodge_entity: Option<u16>,
    /// Where the path ends (`pathgoalpos`).
    pub(crate) final_goal: [f32; 3],
    /// The way to the lookahead, flat, with its height as a slope
    /// (`lookaheaddir`), and its flat distance (`lookaheaddist`).
    pub(crate) look_dir: [f32; 3],
    pub(crate) look_dist: f32,
    /// The way a little ahead along the path, flat.
    pub(crate) forward_dir: [f32; 2],
    amount: f32,
    to_next: f32,
    min_look: usize,
    pub(crate) flags: u32,
    /// When the path was planned.
    pub(crate) time: i64,
    /// Where the actor's current segment starts, and its length left.
    curr: [f32; 3],
    curr_len: f32,
    /// When the path's end ease ends (0: not easing).
    pub(crate) end_time: i64,
    pub(crate) end_notified: bool,
    grows: i32,
    shrinks: i32,
}

impl Default for ActorPath {
    fn default() -> Self {
        Self {
            pts: Vec::new(),
            len: 0,
            orig_len: 0,
            dodge_count: 0,
            negotiation_start: 0,
            next: 0,
            dodge_entity: None,
            final_goal: [0.0; 3],
            look_dir: [0.0; 3],
            look_dist: 0.0,
            forward_dir: [0.0; 2],
            amount: 0.0,
            to_next: 0.0,
            min_look: 0,
            flags: 0,
            time: 0,
            curr: [0.0; 3],
            curr_len: 0.0,
            end_time: 0,
            end_notified: false,
            grows: 0,
            shrinks: 0,
        }
    }
}

fn flat_len(v: [f32; 2]) -> f32 {
    (v[0] * v[0] + v[1] * v[1]).sqrt()
}

/// The engine's guard: a length that is not positive divides as 1.
fn safe(len: f32) -> f32 {
    if -len >= 0.0 { 1.0 } else { len }
}

/// The distance of `pos` from the line of the segment ending at `p`.
fn perp(p: &Point, pos: [f32; 3]) -> f32 {
    (p.dir[0] * (p.origin[1] - pos[1]) - p.dir[1] * (p.origin[0] - pos[0])).abs()
}

impl ActorPath {
    pub(crate) fn has_path(&self) -> bool {
        self.len != 0
    }

    /// The point at `index`, nearest the end first.
    pub(crate) fn point(&self, index: usize) -> &Point {
        &self.pts[index]
    }

    /// Whether the lookahead reached the path's end or its traversal.
    pub(crate) fn at_end(&self) -> bool {
        self.flags & AT_END != 0
    }

    /// Whether the lookahead stopped at the path's end.
    pub(crate) fn looks_at_end(&self) -> bool {
        self.flags & LOOK_AT_END != 0
    }

    /// The direction of the segment the actor walks.
    pub(crate) fn segment_dir(&self) -> [f32; 2] {
        self.pts[self.len.saturating_sub(2)].dir
    }

    /// Where the actor's current segment starts.
    pub(crate) fn current(&self) -> [f32; 3] {
        self.curr
    }

    /// The traversal's begin node, when the path's first traversal is
    /// next.
    pub(crate) fn negotiation_node(&self) -> Option<u16> {
        (self.negotiation_start > 0)
            .then(|| self.pts[self.negotiation_start].node)
            .flatten()
    }

    /// The actor bumped into a sentient: it dodges it from now on.
    pub(crate) fn bumped(&mut self, entity: u16) {
        self.dodge_entity = Some(entity);
        if self.dodge_count == 0 {
            self.dodge_count = -1;
        }
    }

    /// The dodge entity the lookahead trace considers.
    fn dodge(&self) -> Option<u16> {
        self.dodge_entity.filter(|_| self.dodge_count != 0)
    }

    /// The area of the path back to the point nearest the actor comes back
    /// onto the amount, as a path is dropped or replaced.
    fn restore_area(&mut self, pos: [f32; 3]) {
        let last = self.len as isize - 1;
        if last <= self.negotiation_start as isize {
            return;
        }
        let at = |p: &Point| [p.origin[0] - pos[0], p.origin[1] - pos[1]];
        let mut i = last as usize;
        let mut best = flat_len(at(&self.pts[i - 1]));
        let mut area = 0.0f32;
        let mut best_area = 0.0f32;
        loop {
            let p = self.pts[i];
            let d = at(&p);
            let dist = flat_len(d);
            if dist <= best {
                best = dist;
                best_area = area;
            }
            i += 1;
            if i >= self.orig_len {
                break;
            }
            area += (p.dir[0] * d[1] - p.dir[1] * d[0]).abs() * p.len;
            if area > END_AMOUNT {
                break;
            }
        }
        let k = self.len - 2;
        let q = self.pts[k];
        self.amount += perp(&q, pos) * (q.len - self.curr_len) + best_area;
    }

    /// Drops the path at `pos`.
    pub(crate) fn clear(&mut self, pos: [f32; 3]) {
        if self.len == 0 {
            return;
        }
        self.restore_area(pos);
        self.negotiation_start = 0;
        self.len = 0;
        self.orig_len = 0;
    }

    /// Builds the path through `route` (its nodes from the start node to
    /// the goal node) as Black Ops does, then sets the lookahead for it.
    /// `traversal(a, b)` tells whether the link from `a` to `b` crosses a
    /// traversal. Answers whether a path was made.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn build(
        &mut self,
        route: &[(u16, [f32; 3])],
        traversal: impl Fn(u16, u16) -> bool,
        start: [f32; 3],
        goal: [f32; 3],
        allow_negotiation: bool,
        now: i64,
        probe: &mut dyn Probe,
    ) -> bool {
        let Some(&(goal_node, goal_node_at)) = route.last() else {
            return false;
        };
        self.restore_area(start);
        if self.pts.is_empty() {
            self.pts.push(Point::default());
        }
        // The goal itself leads the path unless it stands on the goal node.
        let past_goal = usize::from(goal[0] != goal_node_at[0] || goal[1] != goal_node_at[1]);
        self.pts[0] = Point {
            origin: goal,
            dir: [0.0; 2],
            len: 0.0,
            node: (past_goal == 0).then_some(goal_node),
        };
        let old_flags = self.flags;
        self.flags = 0;
        if self.len == 0 || self.pts[0].origin != self.final_goal {
            if self.pts[0].origin[0] != self.final_goal[0]
                || self.pts[0].origin[1] != self.final_goal[1]
            {
                self.end_time = 0;
            }
            self.final_goal = self.pts[0].origin;
        }
        self.negotiation_start = 0;
        self.len = 0;
        self.orig_len = 0;
        self.dodge_count = 0;
        let mut n = route.len() + past_goal;
        // The traversal nearest the start, by its begin node's index.
        let mut negotiation = 0usize;
        for j in (0..route.len() - 1).rev() {
            if traversal(route[j].0, route[j + 1].0) {
                if !allow_negotiation {
                    return false;
                }
                negotiation = n - 1 - j;
            }
        }
        if n > MAX_POINTS {
            let over = n - MAX_POINTS;
            self.flags |= TRUNCATED;
            negotiation = negotiation.saturating_sub(over);
            n = MAX_POINTS;
        }
        self.pts.resize(n, Point::default());
        let filled = if n < route.len() + past_goal {
            n
        } else {
            n - 1
        };
        for (j, &(node, origin)) in route.iter().take(filled).enumerate() {
            self.pts[n - 1 - j] = Point {
                origin,
                dir: [0.0; 2],
                len: 0.0,
                node: Some(node),
            };
        }
        self.negotiation_start = negotiation;
        self.pts[n - 1].dir = [0.0; 2];
        self.pts[n - 1].len = 0.0;
        self.curr = self.pts[n - 1].origin;
        for i in 0..n - 1 {
            let d = [
                self.pts[i].origin[0] - self.pts[i + 1].origin[0],
                self.pts[i].origin[1] - self.pts[i + 1].origin[1],
            ];
            let len = flat_len(d);
            // Black Ops divides without a guard; points stand apart.
            let scale = if len > 0.0 { 1.0 / len } else { 0.0 };
            self.pts[i].dir = [d[0] * scale, d[1] * scale];
            self.pts[i].len = len;
        }
        self.curr_len = if n >= 2 { self.pts[n - 2].len } else { 0.0 };
        self.len = n;
        self.orig_len = n;
        if allow_negotiation {
            self.flags |= NEGOTIATION;
        }
        self.time = now;
        self.grows = 0;
        self.shrinks = 0;
        if self.amount != 0.0 {
            if old_flags & (REPLANNED | REPLANNED_SCRIPT) != 0 {
                if old_flags & REPLANNED != 0 {
                    self.min_look = 0;
                    self.amount = END_AMOUNT;
                } else {
                    self.min_look = 2;
                    self.amount = SCRIPT_REPLAN_AMOUNT;
                }
                self.look_dir = [0.0; 3];
                self.update(start, (false, false, true), now, probe);
                self.min_look = 0;
            } else {
                self.keep_similar(start, now, probe);
            }
        }
        true
    }

    /// A new path keeps the lookahead the actor had when the path bends
    /// away from it no more than the amount's area allows, else the
    /// lookahead turns to the path's current point.
    fn keep_similar(&mut self, start: [f32; 3], now: i64, probe: &mut dyn Probe) {
        if self.look_dist == 0.0 {
            self.look_at_current(start, false, now, probe);
            return;
        }
        let amount = self.amount;
        let [lx, ly, _] = self.look_dir;
        let v = [self.curr[0] - start[0], self.curr[1] - start[1]];
        let vlen = flat_len(v);
        let u = [v[0] / safe(vlen), v[1] / safe(vlen)];
        let mut side = (u[1] * lx - u[0] * ly) * vlen;
        let mut best = ly * u[1] + lx * u[0];
        let mut best_area = 0.0f32;
        let mut area = 0.0f32;
        let mut first = true;
        let mut i = self.len as isize - 2;
        while i >= self.negotiation_start as isize {
            let p = self.pts[i as usize];
            let w = [p.origin[0] - start[0], p.origin[1] - start[1]];
            let seg = if first { self.curr_len } else { p.len };
            let turn = p.dir[0] * ly - p.dir[1] * lx;
            let signed = p.dir[0] * w[1] - p.dir[1] * w[0];
            let q = w[1] * lx - w[0] * ly;
            if turn * signed > 0.0 && q * side < 0.0 {
                self.update(start, (false, true, true), now, probe);
                return;
            }
            area += signed.abs() * seg;
            if area >= amount {
                let t = -((area - amount) / signed.abs());
                let to = [
                    p.dir[0] * t + p.origin[0] - start[0],
                    p.dir[1] * t + p.origin[1] - start[1],
                ];
                let scale = 1.0 / safe(flat_len(to));
                if best > to[0] * scale * lx + to[1] * scale * ly {
                    self.keep(start, best_area, now, probe);
                } else {
                    self.look_at_current(start, false, now, probe);
                }
                return;
            }
            side = q;
            let scale = 1.0 / safe(flat_len(w));
            let d = w[1] * scale * ly + w[0] * scale * lx;
            if d >= best {
                best = d;
                best_area = area;
            }
            first = false;
            i -= 1;
        }
        if best_area != area {
            self.keep(start, best_area, now, probe);
        } else {
            self.look_at_current(start, false, now, probe);
        }
    }

    fn keep(&mut self, start: [f32; 3], best_area: f32, now: i64, probe: &mut dyn Probe) {
        self.look_at_current(start, true, now, probe);
        if self.flags & GREW == 0 && self.amount > best_area {
            self.amount = best_area.max(FLOOR_AMOUNT);
        }
    }

    /// The lookahead at the actor's current point, then one update.
    fn look_at_current(
        &mut self,
        start: [f32; 3],
        shrink_passed: bool,
        now: i64,
        probe: &mut dyn Probe,
    ) {
        self.aim(self.curr, start);
        self.to_next = 0.0;
        self.next = self.len - 1;
        self.update(start, (false, shrink_passed, true), now, probe);
    }

    /// Points the lookahead at `point` from `start`: flat, its height as a
    /// slope.
    fn aim(&mut self, point: [f32; 3], start: [f32; 3]) {
        let d = [
            point[0] - start[0],
            point[1] - start[1],
            point[2] - start[2],
        ];
        let len = flat_len([d[0], d[1]]);
        let scale = 1.0 / safe(len);
        self.look_dir = [
            d[0] * scale,
            d[1] * scale,
            if len == 0.0 { 0.0 } else { d[2] / len },
        ];
        self.look_dist = len;
    }

    /// Black Ops' lookahead update for the actor at `start`: past its
    /// current point the passed part is trimmed (and with `shrink_passed`
    /// its area comes off the amount), short of it the path may walk back
    /// (`backtrack`); `decrease` shrinks the amount instead of tracing the
    /// new lookahead. Answers false (leaving the lookahead) when the path
    /// is to be planned again from `start` to its end.
    pub(crate) fn update(
        &mut self,
        start: [f32; 3],
        (decrease, shrink_passed, backtrack): (bool, bool, bool),
        now: i64,
        probe: &mut dyn Probe,
    ) -> bool {
        self.flags &= !BACKTRACK_FAR;
        if self.negotiation_start + 1 == self.len {
            self.aim(self.curr, start);
            self.flags |= AT_END | LOOK_AT_END;
            self.to_next = 0.0;
            self.next = self.negotiation_start;
            self.forward(start);
            return true;
        }
        let ahead = (self.curr[0] - start[0]) * self.look_dir[0]
            + (self.curr[1] - start[1]) * self.look_dir[1];
        if ahead > 0.0 {
            if backtrack && self.amount >= FLOOR_AMOUNT {
                self.backtrack(start);
            }
        } else {
            if self.time < now && self.needs_replan(probe) {
                return false;
            }
            if self.flags & GREW != 0 || self.amount <= TRIM_AMOUNT {
                self.trim(start);
            }
            if shrink_passed {
                self.shrink_passed(start);
            }
        }
        self.calc(start, decrease, probe);
        self.forward(start);
        true
    }

    /// Whether the path is to be planned again: cut short and nearly done,
    /// or the node ahead lost its links.
    pub(crate) fn needs_replan(&self, probe: &mut dyn Probe) -> bool {
        if self.flags & TRUNCATED != 0 && self.len < SHORT_TRUNCATED {
            return true;
        }
        self.pts
            .get(self.next)
            .and_then(|p| p.node)
            .is_some_and(|node| !probe.linked(node))
    }

    /// Walks the path from the actor's segment on until the area reaches
    /// the amount, and sets the lookahead there (or at the path's end, or
    /// its first traversal's begin node).
    fn calc(&mut self, start: [f32; 3], decrease: bool, probe: &mut dyn Probe) {
        let amount = self.amount;
        let mut area = 0.0f32;
        let last = self.len as isize - 2;
        let mut i = last;
        let mut first = true;
        while i >= self.negotiation_start as isize {
            let k = i as usize;
            let p = self.pts[k];
            let perp = perp(&p, start);
            let seg = if first { self.curr_len } else { p.len };
            area += seg * perp;
            if self.min_look == 2 && k + 3 == self.len && k >= 1 {
                let before = self.pts[k - 1];
                let after = self.pts[k + 1].origin;
                let d = [before.origin[0] - after[0], before.origin[1] - after[1]];
                let scale = 1.0 / safe(flat_len(d));
                let straight = before.dir[0] * d[0] * scale + before.dir[1] * d[1] * scale;
                let turn = before.dir[0] * p.dir[0] + before.dir[1] * p.dir[1];
                if straight >= CORNER_STRAIGHT && turn >= CORNER_TURN {
                    self.min_look = 3;
                }
            }
            if self.min_look as isize + i <= last && area >= amount {
                let t = if perp > 0.0 {
                    let t = (area - amount) / perp;
                    if t > seg { seg } else { t }
                } else {
                    seg
                };
                let prev = if first {
                    self.curr
                } else {
                    self.pts[k + 1].origin
                };
                let z = if seg != 0.0 {
                    p.origin[2] - (p.origin[2] - prev[2]) / seg * t
                } else {
                    p.origin[2]
                };
                let point = [p.origin[0] - p.dir[0] * t, p.origin[1] - p.dir[1] * t, z];
                self.flags &= !(AT_END | LOOK_AT_END);
                self.set_lookahead(point, start, (decrease, t, k, amount), probe);
                return;
            }
            first = false;
            i -= 1;
        }
        self.flags |= AT_END | LOOK_AT_END;
        let point = if self.len - 1 == self.negotiation_start {
            self.curr
        } else {
            self.pts[self.negotiation_start].origin
        };
        let at = self.negotiation_start;
        self.set_lookahead(point, start, (decrease, 0.0, at, area), probe);
        let cap = if area - END_AMOUNT < 0.0 {
            END_AMOUNT
        } else {
            area
        };
        if cap - self.amount < 0.0 {
            self.amount = cap;
        }
    }

    /// The lookahead to `point`, `t` along the segment ending at point
    /// `index`. The amount grows when the actor can step there and shrinks
    /// when it cannot (or with `decrease`); blocked while the last
    /// lookahead grew and still lies ahead on its segment, the lookahead
    /// stays.
    fn set_lookahead(
        &mut self,
        point: [f32; 3],
        start: [f32; 3],
        (decrease, t, index, amount): (bool, f32, usize, f32),
        probe: &mut dyn Probe,
    ) {
        if self.next + 1 >= self.len {
            self.to_next = 0.0;
            self.next = self.len - 1;
        }
        if decrease {
            self.shrink(amount);
        } else if probe.clear(start, point, self.dodge()) {
            self.grow();
        } else {
            let seg = if self.next + 2 == self.len {
                self.curr_len
            } else {
                self.pts[self.next].len
            };
            if self.flags & GREW != 0 && self.next < self.len && seg >= self.to_next {
                self.shrink(amount);
                return;
            }
            self.shrink(amount);
        }
        self.aim(point, start);
        self.to_next = t;
        self.next = index;
    }

    fn shrink(&mut self, amount: f32) {
        self.shrinks += 1;
        self.grows = 0;
        let factor = if self.flags & GREW != 0 {
            SHRINK_GREW
        } else {
            SHRINK
        };
        self.amount = amount * factor;
        if self.amount < MIN_AMOUNT {
            self.amount = MIN_AMOUNT;
        }
        self.flags &= !(GREW | GREW_TWICE | AT_END);
    }

    fn grow(&mut self) {
        self.grows += 1;
        self.shrinks = 0;
        self.amount = (self.amount * GROW_SCALE + GROW_STEP).min(MAX_AMOUNT);
        self.flags |= if self.flags & GREW != 0 {
            GREW_TWICE
        } else {
            GREW
        };
    }

    /// Adds a backtrack's area to the amount.
    fn add_area(&mut self, mut area: f32) {
        if area <= 0.0 {
            return;
        }
        if self.flags & GREW == 0 {
            area *= BACKTRACK_SHARE;
        }
        self.amount = (self.amount + area).min(MAX_AMOUNT);
    }

    /// The actor stands behind its current point: the current point comes
    /// back to where the actor's plane across the lookahead meets the path,
    /// walking back over passed points (within 256 units, and while their
    /// area stays within a share of the amount).
    fn backtrack(&mut self, start: [f32; 3]) {
        let limit = self.amount * BACKTRACK_LIMIT;
        let look = [self.look_dir[0], self.look_dir[1]];
        let mut i = self.len - 1;
        let mut p = self.pts[i];
        let q = self.pts[i - 1];
        let mut perp = perp(&q, start);
        let mut d = [p.origin[0] - start[0], p.origin[1] - start[1]];
        let mut acc = -(self.curr_len * perp);
        // `r` is the point ahead of `p` the segment from `p` ends at.
        let mut r = q;
        let mut last = q;
        if d[0] * d[0] + d[1] * d[1] <= BACKTRACK_RANGE_SQ {
            loop {
                last = p;
                let proj = look[0] * d[0] + look[1] * d[1];
                let cut = if proj <= 0.0 {
                    let ahead =
                        look[1] * (r.origin[1] - start[1]) + (r.origin[0] - start[0]) * look[0];
                    if ahead > 0.0 {
                        Some(proj / (proj - ahead))
                    } else {
                        // Both behind: the current point is `r`.
                        self.backtrack_to(r, i, perp, acc);
                        return;
                    }
                } else {
                    let seg_area = r.len * perp;
                    acc += seg_area;
                    if acc >= limit {
                        let frac = (acc - limit) / seg_area;
                        acc -= seg_area;
                        Some(frac)
                    } else {
                        None
                    }
                };
                if let Some(frac) = cut {
                    let along = r.len * frac;
                    if along < r.len {
                        self.curr_len = r.len - along;
                        self.curr = [
                            r.dir[0] * along + p.origin[0],
                            r.dir[1] * along + p.origin[1],
                            (r.origin[2] - p.origin[2]) * frac + p.origin[2],
                        ];
                        acc += (r.len - along) * perp;
                    } else {
                        i -= 1;
                        if i <= self.negotiation_start {
                            return;
                        }
                        self.curr = r.origin;
                        self.curr_len = self.pts[i - 1].len;
                    }
                    self.len = i + 1;
                    self.add_area(acc);
                    return;
                }
                i += 1;
                if i >= self.orig_len {
                    self.curr_len = r.len;
                    self.curr = p.origin;
                    self.len = self.orig_len;
                    self.add_area(acc);
                    return;
                }
                perp = (p.dir[0] * d[1] - p.dir[1] * d[0]).abs();
                r = p;
                p = self.pts[i];
                d = [p.origin[0] - start[0], p.origin[1] - start[1]];
                if d[0] * d[0] + d[1] * d[1] > BACKTRACK_RANGE_SQ {
                    break;
                }
            }
        }
        self.flags |= BACKTRACK_FAR;
        self.backtrack_to(last, i, perp, acc);
    }

    /// The current point becomes `r`, the last point of a path `i` long.
    fn backtrack_to(&mut self, r: Point, i: usize, perp: f32, acc: f32) {
        self.curr = r.origin;
        self.curr_len = if self.negotiation_start + 1 < i {
            self.pts[i - 2].len
        } else {
            0.0
        };
        self.len = i;
        self.add_area(acc + r.len * perp);
    }

    /// The path is `index + 2` points long, the dodge count kept relative
    /// to it.
    fn set_len(&mut self, index: usize) {
        if self.dodge_count < 0 {
            self.len = index + 2;
            return;
        }
        self.dodge_count += (index + 2) as i32 - self.len as i32;
        self.len = index + 2;
        if self.dodge_count < 0 {
            self.dodge_count = 0;
        }
    }

    /// Moves the actor's current point up to where its plane across the
    /// lookahead meets the path, dropping the points behind.
    fn trim(&mut self, start: [f32; 3]) {
        let look = [self.look_dir[0], self.look_dir[1]];
        let along = |p: [f32; 3]| look[0] * (p[0] - start[0]) + look[1] * (p[1] - start[1]);
        let mut i = self.len as isize - 2;
        let mut first = true;
        if (self.negotiation_start as isize) > i {
            return;
        }
        let proj = loop {
            let proj = along(self.pts[i as usize].origin);
            if proj > 0.0 {
                break proj;
            }
            i -= 1;
            first = false;
            if i < self.negotiation_start as isize {
                return;
            }
        };
        let k = i as usize;
        let (seg, from) = if first {
            (self.curr_len, self.curr)
        } else {
            (self.pts[k].len, self.pts[k + 1].origin)
        };
        let behind = along(from) - TRIM_EPSILON;
        let frac = behind / (behind - proj);
        let moved = frac * seg;
        let p = self.pts[k];
        if moved < seg {
            self.curr = [
                p.dir[0] * moved + from[0],
                p.dir[1] * moved + from[1],
                (p.origin[2] - from[2]) * frac + from[2],
            ];
            self.curr_len = seg - moved;
            self.set_len(k);
        } else if self.negotiation_start < k {
            self.curr = p.origin;
            self.curr_len = self.pts[k - 1].len;
            self.set_len(k - 1);
        }
    }

    /// The area of the part of the path the actor passed comes off the
    /// amount (at least 64 stays).
    fn shrink_passed(&mut self, start: [f32; 3]) {
        if self.len == self.orig_len {
            return;
        }
        let k = self.len - 2;
        let q = self.pts[k];
        let mut area = perp(&q, start) * (q.len - self.curr_len);
        for j in self.len - 1..self.orig_len - 1 {
            let p = self.pts[j];
            area += perp(&p, start) * p.len;
        }
        self.amount -= area;
        if self.amount < FLOOR_AMOUNT {
            self.amount = FLOOR_AMOUNT;
        }
    }

    /// The forward lookahead: once the lookahead grew twice, the way from
    /// a point a little ahead of the actor to where the area from there
    /// reaches the amount; else the lookahead's own way.
    fn forward(&mut self, start: [f32; 3]) {
        let look = self.look_dir;
        self.forward_dir = [look[0], look[1]];
        if self.flags & GREW_TWICE == 0 || self.len <= 1 || self.flags & AT_END != 0 {
            return;
        }
        let reach = if FORWARD_REACH - self.look_dist >= 0.0 {
            self.look_dist
        } else {
            FORWARD_REACH
        };
        let f = [start[0] + look[0] * reach, start[1] + look[1] * reach, 0.0];
        let along = |p: [f32; 3]| look[0] * (p[0] - f[0]) + look[1] * (p[1] - f[1]);
        if along(self.curr) > 0.0 {
            return;
        }
        let mut i = self.len as isize - 2;
        let mut first = true;
        let proj = loop {
            if i < self.negotiation_start as isize {
                return;
            }
            let proj = along(self.pts[i as usize].origin);
            if proj > 0.0 {
                break proj;
            }
            i -= 1;
            first = false;
        };
        let k = i as usize;
        let (from, seg) = if first {
            (self.curr, self.curr_len)
        } else {
            (self.pts[k + 1].origin, self.pts[k].len)
        };
        let behind = along(from);
        let frac = behind / (behind - proj);
        let fperp =
            |p: &Point| (p.dir[0] * (p.origin[1] - f[1]) - p.dir[1] * (p.origin[0] - f[0])).abs();
        let mut perp = fperp(&self.pts[k]);
        let mut area = (seg - frac * seg) * perp;
        let mut at = k;
        if area < self.amount {
            let mut j = k;
            loop {
                if j <= self.negotiation_start {
                    let end = self.pts[self.negotiation_start].origin;
                    let d = [end[0] - f[0], end[1] - f[1]];
                    let scale = 1.0 / safe(flat_len(d));
                    self.forward_dir = [d[0] * scale, d[1] * scale];
                    return;
                }
                let q = self.pts[j - 1];
                perp = fperp(&q);
                area += perp * q.len;
                j -= 1;
                if area >= self.amount {
                    at = j;
                    break;
                }
            }
        }
        let p = self.pts[at];
        let t = -((area - self.amount) / perp);
        let d = [
            p.dir[0] * t + p.origin[0] - f[0],
            p.dir[1] * t + p.origin[1] - f[1],
        ];
        let scale = 1.0 / safe(flat_len(d));
        self.forward_dir = [d[0] * scale, d[1] * scale];
    }

    /// After a move the AI did not steer (an animation's own motion from
    /// `from` to `to`): a path point the move passed becomes the current
    /// point, the amount is at least 1024, and the lookahead is set again.
    /// Answers false when the path is to be planned again.
    pub(crate) fn non_code_move(
        &mut self,
        from: [f32; 3],
        to: [f32; 3],
        now: i64,
        probe: &mut dyn Probe,
    ) -> bool {
        let m = [to[0] - from[0], to[1] - from[1]];
        if m[0] * m[0] + m[1] * m[1] > NON_CODE_MOVE_MIN_SQ {
            let mut i = self.len as isize - 2;
            while i >= self.negotiation_start as isize {
                let k = i as usize;
                let p = self.pts[k];
                if (p.origin[0] - from[0]) * m[0] + (p.origin[1] - from[1]) * m[1] >= 0.0 {
                    if (p.origin[0] - to[0]) * m[0] + (p.origin[1] - to[1]) * m[1] <= 0.0
                        && self.negotiation_start < k
                    {
                        self.curr = p.origin;
                        self.curr_len = self.pts[k - 1].len;
                        self.set_len(k - 1);
                    }
                    break;
                }
                i -= 1;
            }
        }
        if self.amount - NON_CODE_MOVE_AMOUNT < 0.0 {
            self.amount = NON_CODE_MOVE_AMOUNT;
        }
        self.update(to, (false, false, false), now, probe)
    }

    /// Cuts the path back to the last point within the goal, walking from
    /// the actor's current point; answers false (for the caller to drop
    /// the path) when the current point itself lies outside it. Answers the
    /// traversal node the cut let go of, if any.
    pub(crate) fn trim_to_goal(&mut self, mut in_goal: impl FnMut([f32; 3]) -> bool) -> bool {
        if self.len == 0 {
            return true;
        }
        if !in_goal(self.curr) {
            return false;
        }
        let mut i = self.len as isize - 2;
        while i >= 0 {
            if !in_goal(self.pts[i as usize].origin) {
                self.drop_end((i + 1) as usize);
                return true;
            }
            i -= 1;
        }
        true
    }

    /// Drops the `count` points nearest the path's end: the path now ends
    /// at the next one (at the current point when fewer than two stay).
    fn drop_end(&mut self, count: usize) {
        if count == 0 {
            return;
        }
        let k = count as i32;
        self.len = (self.len as i32 - k).max(0) as usize;
        self.orig_len = (self.orig_len as i32 - k).max(0) as usize;
        if self.dodge_count >= 0 {
            self.dodge_count = (self.dodge_count - k).max(0);
        }
        let next = self.next as i32 - k;
        if next < 0 {
            self.next = 0;
            self.to_next = 0.0;
        } else {
            self.next = next as usize;
        }
        let negotiation = self.negotiation_start as i32 - k;
        if negotiation < 1 {
            self.negotiation_start = 0;
        } else {
            self.negotiation_start = negotiation as usize;
        }
        if self.len < 2 {
            self.pts.truncate(1);
            self.pts[0] = Point {
                origin: self.curr,
                dir: [0.0; 2],
                len: 0.0,
                node: None,
            };
            self.orig_len = 1;
            self.curr_len = 0.0;
        } else {
            self.pts.drain(..count);
        }
        self.final_goal = self.pts[0].origin;
        self.flags &= !TRUNCATED;
        self.end_time = 0;
    }

    /// Whether more than `limit` of the path lies past the lookahead
    /// (counting a traversal ahead as 128 more).
    pub(crate) fn longer_than(&self, limit: f32) -> bool {
        let mut length = self.to_next + self.look_dist;
        if self.negotiation_start != 0 {
            length += 128.0;
        }
        if length > limit {
            return true;
        }
        if self.next == 0 {
            return false;
        }
        let mut i = self.next as isize - 1;
        if i == self.len as isize - 2 {
            length += self.curr_len;
            if limit < length {
                return true;
            }
            i = self.next as isize - 2;
        }
        while i >= 0 {
            length += self.pts[i as usize].len;
            if limit < length {
                return true;
            }
            i -= 1;
        }
        false
    }
}

/// Black Ops' facing smoothing: the last ten flat steer directions,
/// weighted by how recent they are (the newest whole, each older one a
/// tenth less).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct LeanRing {
    slots: [[f32; 2]; 10],
    index: usize,
}

impl LeanRing {
    /// Writes this frame's direction.
    pub(crate) fn set(&mut self, dir: [f32; 2]) {
        self.slots[self.index] = dir;
    }

    /// Adds to this frame's direction.
    pub(crate) fn add(&mut self, dir: [f32; 2]) {
        let slot = &mut self.slots[self.index];
        slot[0] += dir[0];
        slot[1] += dir[1];
    }

    /// Ends the frame: a zigzag over the last three directions drops the
    /// middle's predecessor, the older directions fade, and the next slot
    /// is this frame's.
    pub(crate) fn step(&mut self) {
        let idx = self.index;
        let prev = if idx == 0 { 9 } else { idx - 1 };
        let prev2 = if prev == 0 { 9 } else { prev - 1 };
        let cur = self.slots[idx];
        let p = self.slots[prev];
        let pp = self.slots[prev2];
        let side = |v: [f32; 2]| v[1] * -p[0] + v[0] * p[1];
        if side(pp) * side(cur) <= 0.0
            && p[1] * cur[1] + p[0] * cur[0] >= 0.0
            && p[1] * pp[1] + p[0] * pp[0] >= 0.0
        {
            self.slots[prev2] = cur;
        }
        let mut weight = 1.0f32;
        for k in (idx + 1..10).chain(0..idx) {
            let next = weight + 1.0;
            let scale = weight / next;
            self.slots[k][0] *= scale;
            self.slots[k][1] *= scale;
            weight = next;
        }
        self.index = (idx + 1) % 10;
    }

    /// The weighted sum.
    pub(crate) fn sum(&self) -> [f32; 2] {
        self.slots
            .iter()
            .fold([0.0; 2], |acc, v| [acc[0] + v[0], acc[1] + v[1]])
    }
}
