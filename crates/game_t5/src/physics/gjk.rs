//! Black Ops' convex cast: a GJK ray cast that advances the moving shape
//! until its core comes within the two shapes' rounding of the other, with
//! its own simplex solver, tolerances and penetration search.

pub(super) type V3 = [f32; 3];

pub(super) fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(super) fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(super) fn scale(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

pub(super) fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn neg(a: V3) -> V3 {
    [-a[0], -a[1], -a[2]]
}

/// A convex core: the farthest point along a direction, and its centre.
pub(super) trait Convex {
    fn support(&self, dir: V3) -> V3;
    fn center(&self) -> V3;
}

/// The other shape's space in the moving shape's: its axes by rows and its
/// origin.
#[derive(Clone, Copy, Debug)]
pub(super) struct Frame {
    pub(super) rows: [V3; 3],
    pub(super) origin: V3,
}

impl Frame {
    fn local(self, d: V3) -> V3 {
        [
            dot(self.rows[0], d),
            dot(self.rows[1], d),
            dot(self.rows[2], d),
        ]
    }

    fn outer(self, p: V3) -> V3 {
        let r = &self.rows;
        [
            self.origin[0] + r[0][0] * p[0] + r[1][0] * p[1] + r[2][0] * p[2],
            self.origin[1] + r[0][1] * p[0] + r[1][1] * p[1] + r[2][1] * p[2],
            self.origin[2] + r[0][2] * p[0] + r[1][2] * p[1] + r[2][2] * p[2],
        ]
    }
}

pub(super) struct CastInput<'a> {
    pub(super) a: &'a dyn Convex,
    pub(super) b: &'a dyn Convex,
    pub(super) frame: Frame,
    /// The moving shape's sweep.
    pub(super) delta: V3,
    /// The farthest fraction worth reaching.
    pub(super) cap: f32,
    pub(super) radius_a: f32,
    pub(super) radius_b: f32,
    pub(super) skin: f32,
}

/// Where the cast stopped: the fraction, the normal from the other shape
/// towards the moving one, and the two surface points (the moving shape's
/// at its start).
#[derive(Clone, Copy, Debug)]
pub(super) struct Contact {
    pub(super) fraction: f32,
    pub(super) normal: V3,
    pub(super) point_a: V3,
    pub(super) point_b: V3,
}

const MARGIN_FLOOR: f32 = f32::from_bits(0x3d50_e560);
const TINY: f32 = f32::from_bits(0x322b_cc76);
const INTERSECT_EPS2: f32 = f32::from_bits(0x3a97_84ed);
const REL_TOL: f32 = 0.001;
const ADVANCE_MIN: f32 = f32::from_bits(0x38d1_b717);
const MAX_ADVANCES: i32 = 5;
const DUPLICATE2: f32 = f32::from_bits(0x3586_37be);
const EDGE_EPS: f32 = f32::from_bits(0x2edb_e6fe);
const DET_EPS: f32 = f32::from_bits(0x3727_c5ac);
const PLANE_EPS: f32 = f32::from_bits(0x3c23_d70b);
const MAX_ITER: i32 = 30;
const BOUND_START: f32 = -34.0;
const LENGTH_START: f32 = 34.0;
const PEN_REACH: f32 = 17.0;
const PEN_REACH2: f32 = 289.0;
const PEN_TRIES: i32 = 10;
const FULL: u32 = 0xf;

const FLAG_BOUNDED: u32 = 1;
const FLAG_FROZEN: u32 = 4;
const FLAG_SWEEP: u32 = 8;
const FLAG_ADVANCED: u32 = 0x10;
const FLAG_SEEN: u32 = 0x20;

/// Twelve directions around the sphere, and the twenty faces between them.
const PEN_DIRS: [V3; 12] = [
    [-0.515_304, 0.445_123, -0.732_344],
    [-0.222_601, 0.275_979, 0.935_032],
    [0.497_603, 0.665_553, -0.556_264],
    [0.515_302, -0.445_124, 0.732_344],
    [-0.960_414, 0.204_353, 0.189_327],
    [-0.678_502, -0.561_016, -0.474_231],
    [0.960_414, -0.204_353, -0.189_327],
    [-0.497_604, -0.665_553, 0.556_264],
    [0.222_601, -0.275_977, -0.935_032],
    [0.678_504, 0.561_015, 0.474_231],
    [-0.233_54, 0.962_414, 0.138_628],
    [0.233_54, -0.962_414, -0.138_628],
];
const PEN_FACES: [[usize; 3]; 20] = [
    [10, 1, 4],
    [8, 11, 6],
    [0, 10, 4],
    [2, 8, 6],
    [0, 2, 10],
    [2, 0, 8],
    [9, 1, 10],
    [9, 2, 6],
    [2, 9, 10],
    [1, 7, 4],
    [5, 11, 8],
    [0, 5, 8],
    [5, 7, 11],
    [5, 0, 4],
    [7, 5, 4],
    [7, 3, 11],
    [11, 3, 6],
    [3, 9, 6],
    [9, 3, 1],
    [3, 7, 1],
];

struct Gjk<'a> {
    input: &'a CastInput<'a>,
    delta: V3,
    lambda: f32,
    offset: V3,
    w: [V3; 4],
    a: [V3; 4],
    b: [V3; 4],
    v: V3,
    margin: f32,
    advances: i32,
    flags: u32,
    bits: u32,
    last_bits: u32,
    iter: i32,
    reach: f32,
    eps2: f32,
    v2: f32,
    lb2: f32,
    bary: [[f32; 4]; 16],
}

/// Casts `input.a` along `input.delta` against `input.b`; `None` when it
/// reaches nothing (or the cores overlap past recovery).
pub(super) fn cast(input: &CastInput<'_>) -> Option<Contact> {
    let mut g = Gjk {
        input,
        delta: input.delta,
        lambda: 0.0,
        offset: [0.0; 3],
        w: [[0.0; 3]; 4],
        a: [[0.0; 3]; 4],
        b: [[0.0; 3]; 4],
        v: [0.0; 3],
        margin: input.radius_a + input.radius_b,
        advances: 0,
        flags: FLAG_SWEEP,
        bits: 0,
        last_bits: 0,
        iter: 0,
        reach: input.radius_a + input.radius_b + input.skin,
        eps2: INTERSECT_EPS2,
        v2: 0.0,
        lb2: 0.0,
        bary: [[0.0; 4]; 16],
    };
    match g.run() {
        0 | 3 => return None,
        _ => {}
    }
    if g.bits == FULL {
        return None;
    }
    let (mut point_a, mut point_b) = g.closest(g.bits);
    let v = g.v_from(g.bits);
    let length = dot(v, v).sqrt();
    let normal = scale(v, 1.0 / length);
    point_a = sub(point_a, scale(normal, input.radius_a));
    point_b = add(point_b, scale(normal, input.radius_b));
    Some(Contact {
        fraction: g.lambda,
        normal,
        point_a,
        point_b,
    })
}

impl Gjk<'_> {
    fn support_a(&self, dir: V3) -> V3 {
        self.input.a.support(dir)
    }

    fn support_b(&self, dir: V3) -> V3 {
        let frame = &self.input.frame;
        frame.outer(self.input.b.support(frame.local(dir)))
    }

    fn initial_dir(&self) -> V3 {
        let frame = &self.input.frame;
        let center_b = frame.outer(self.input.b.center());
        let v = sub(self.input.a.center(), center_b);
        if dot(v, v) >= TINY {
            v
        } else {
            [1.0, 0.0, 0.0]
        }
    }

    fn free_slot(&self) -> usize {
        if self.bits & 1 == 0 {
            0
        } else if self.bits & 2 == 0 {
            1
        } else {
            (((self.bits & 4) | 8) >> 2) as usize
        }
    }

    fn run(&mut self) -> i32 {
        self.offset = [0.0; 3];
        self.flags |= FLAG_BOUNDED;
        let v0 = self.initial_dir();
        let r = if self.flags & FLAG_SWEEP != 0 {
            self.ray_cast(v0)
        } else {
            self.distance(v0, false)
        };
        if r != 2 {
            return r;
        }
        self.lambda = 0.0;
        let mut dir = self.penetration_dir();
        if dot(dir, dir) < TINY {
            if dot(v0, v0) < TINY {
                return 3;
            }
            dir = v0;
        }
        self.flags &= !FLAG_BOUNDED;
        let mut tries = 0;
        loop {
            tries += 1;
            let pa = self.support_a(neg(dir));
            let pb = self.support_b(dir);
            let w = sub(pa, pb);
            let d2 = dot(dir, dir);
            let t = dot(dir, w) / d2 - PEN_REACH / d2.sqrt();
            self.offset = scale(dir, t);
            if self.distance(dir, tries > 1) == 2 {
                return 3;
            }
            dir = self.v_from(self.bits);
            let bound = (1.0 - REL_TOL) * (1.0 - REL_TOL) * self.lb2;
            if !(PEN_REACH2 <= bound && tries < PEN_TRIES) {
                break;
            }
        }
        2
    }

    /// Starts the simplex: rebuilt at the current offset when `keep`, else
    /// empty with `v` as the first direction. Returns the iteration count
    /// to start from.
    fn init(&mut self, v: V3, keep: bool) -> i32 {
        if keep {
            let n = self.compact(self.bits);
            if n != 0 {
                self.bits = 0;
                for i in 0..n {
                    self.w[i] = sub(sub(self.a[i], self.b[i]), self.offset);
                    self.bits |= 1 << i;
                }
                self.bits = self.solve(0, true);
                self.v = self.v_from(self.bits);
                self.last_bits = self.bits;
                return 1;
            }
        }
        self.v = v;
        self.last_bits = 0;
        self.bits = 0;
        0
    }

    fn compact(&mut self, bits: u32) -> usize {
        let mut n = 0;
        for i in 0..4 {
            if bits & 1 << i != 0 {
                self.a[n] = self.a[i];
                self.b[n] = self.b[i];
                n += 1;
            }
        }
        n
    }

    fn ray_cast(&mut self, v0: V3) -> i32 {
        let input = self.input;
        self.lambda = 0.0;
        self.offset = scale(self.delta, -self.lambda);
        self.lb2 = BOUND_START;
        self.v2 = LENGTH_START;
        self.iter = self.init(v0, false);
        self.flags &= !(FLAG_BOUNDED | FLAG_FROZEN | FLAG_SEEN | FLAG_ADVANCED);
        self.advances = 0;
        let margin = if self.margin >= 0.0 {
            self.margin
        } else {
            MARGIN_FLOOR
        };
        loop {
            self.v2 = dot(self.v, self.v);
            if self.iter != 0 {
                if self.flags & FLAG_ADVANCED != 0 && self.v2 < self.reach * self.reach {
                    self.flags |= FLAG_BOUNDED;
                }
                if self.v2 < self.eps2 {
                    return 2;
                }
            }
            let slot = self.free_slot();
            self.a[slot] = self.support_a(neg(self.v));
            self.b[slot] = self.support_b(self.v);
            let w = sub(sub(self.a[slot], self.b[slot]), self.offset);
            let vw = dot(w, self.v);
            if vw > 0.0 && self.v2 > 0.0 {
                self.flags |= FLAG_SEEN;
                let q = vw * vw / self.v2;
                if self.lb2 < q {
                    self.lb2 = q;
                    if self.flags & FLAG_BOUNDED != 0 && self.reach * self.reach < q {
                        return 0;
                    }
                }
            }
            let mut done = false;
            if self.iter != 0 && self.lb2 > 0.0 {
                if (1.0 - REL_TOL) * (1.0 - REL_TOL) * self.v2 < self.lb2 {
                    done = true;
                }
                for j in 0..4 {
                    if done {
                        break;
                    }
                    let d = sub(w, self.w[j]);
                    if self.last_bits & 1 << j != 0 && dot(d, d) < DUPLICATE2 {
                        done = true;
                    }
                }
            }
            if self.flags & FLAG_FROZEN == 0 && vw > 0.0 && margin * margin * self.v2 <= vw * vw {
                let length = self.v2.sqrt();
                let approach = -dot(self.delta, self.v);
                let gap = vw - length * margin;
                if approach > 0.0 {
                    if gap >= 0.0 {
                        let along = dot(self.v, self.offset);
                        let mut next = (along + gap) / approach;
                        if self.lambda + ADVANCE_MIN < next {
                            if input.cap < next {
                                if self.reach * length < along + (gap - input.cap * approach) {
                                    return 0;
                                }
                                next = input.cap;
                                self.flags |= FLAG_BOUNDED | FLAG_FROZEN;
                            }
                            if next - self.lambda <= ADVANCE_MIN {
                                self.flags |= FLAG_BOUNDED | FLAG_FROZEN;
                            }
                            self.lambda = next;
                            self.offset = scale(self.delta, -self.lambda);
                            self.advances += 1;
                            self.lb2 = BOUND_START;
                            self.v2 = LENGTH_START;
                            if self.advances > 1 {
                                self.flags |= FLAG_ADVANCED;
                            }
                            if self.advances >= MAX_ADVANCES {
                                self.flags |= FLAG_BOUNDED | FLAG_FROZEN;
                            }
                            if self.bits == 0 {
                                self.bits = 1 << slot;
                            }
                            let v = self.v;
                            self.init(v, true);
                            self.iter += 1;
                            continue;
                        }
                    }
                } else if gap > 0.0 {
                    if self.reach * self.reach < self.lb2 {
                        return 0;
                    }
                    self.flags |= FLAG_BOUNDED | FLAG_FROZEN;
                }
            }
            if done {
                return 1;
            }
            self.w[slot] = w;
            self.bits |= 1 << slot;
            self.last_bits = self.bits;
            self.bits = self.solve(slot, false);
            if self.bits == FULL {
                return 2;
            }
            self.v = self.v_from(self.bits);
            self.iter += 1;
            if self.iter > MAX_ITER - 1 {
                return if self.lb2 > 0.0 { 1 } else { 2 };
            }
        }
    }

    fn distance(&mut self, v0: V3, keep: bool) -> i32 {
        self.lb2 = BOUND_START;
        self.v2 = LENGTH_START;
        self.iter = self.init(v0, keep);
        loop {
            self.v2 = dot(self.v, self.v);
            if self.iter != 0 && self.v2 < self.eps2 {
                return 2;
            }
            let slot = self.free_slot();
            self.a[slot] = self.support_a(neg(self.v));
            self.b[slot] = self.support_b(self.v);
            let w = sub(sub(self.a[slot], self.b[slot]), self.offset);
            let vw = dot(w, self.v);
            if vw > 0.0 && self.v2 > 0.0 {
                let q = vw * vw / self.v2;
                if self.lb2 < q {
                    self.lb2 = q;
                    if self.flags & FLAG_BOUNDED != 0 && self.reach * self.reach < q {
                        return 0;
                    }
                }
            }
            if self.iter != 0 && self.lb2 > 0.0 {
                if (1.0 - REL_TOL) * (1.0 - REL_TOL) * self.v2 < self.lb2 {
                    return 1;
                }
                for j in 0..4 {
                    let d = sub(w, self.w[j]);
                    if self.last_bits & 1 << j != 0 && dot(d, d) < DUPLICATE2 {
                        return 1;
                    }
                }
            }
            self.w[slot] = w;
            self.bits |= 1 << slot;
            self.last_bits = self.bits;
            self.bits = self.solve(slot, false);
            if self.bits == FULL {
                return 2;
            }
            self.v = self.v_from(self.bits);
            self.iter += 1;
            if self.iter >= MAX_ITER {
                return 1;
            }
        }
    }

    /// The smallest sub-simplex holding the point nearest the origin, with
    /// slot `k` new; `keep` also weighs the older points alone.
    fn solve(&mut self, k: usize, keep: bool) -> u32 {
        let wk = self.w[k];
        let kbit = 1u32 << k;
        let mut best = dot(wk, wk);
        self.bary[kbit as usize][k] = 1.0;
        let bits_in = self.bits;
        let mut result = kbit;
        let mut idx = [0usize; 3];
        let mut e = [[0.0f32; 3]; 3];
        let mut len2 = [0.0f32; 3];
        let mut dots = [0.0f32; 3];
        let mut count = 0;
        for j in 0..4 {
            let jbit = 1u32 << j;
            if bits_in & !kbit & jbit == 0 {
                continue;
            }
            idx[count] = j;
            e[count] = sub(self.w[j], wk);
            len2[count] = dot(e[count], e[count]);
            dots[count] = dot(e[count], wk);
            if EDGE_EPS < len2[count] {
                let t = -dots[count] / len2[count];
                if 0.0 < t && 0.0 < 1.0 - t {
                    let p = add(wk, scale(e[count], t));
                    let d2 = dot(p, p);
                    if d2 < best {
                        result = jbit | kbit;
                        self.bary[result as usize][j] = t;
                        self.bary[result as usize][k] = 1.0 - t;
                        best = d2;
                    }
                }
            }
            if keep {
                let d2 = dot(self.w[j], self.w[j]);
                if d2 < best {
                    self.bary[jbit as usize][j] = 1.0;
                    result = jbit;
                    best = d2;
                }
            }
            count += 1;
        }
        if keep && count == 2 {
            let edge = sub(self.w[2], self.w[1]);
            let l2 = dot(edge, edge);
            if EDGE_EPS < l2 {
                let t = -dot(self.w[1], edge) / l2;
                if 0.0 < t && 0.0 < 1.0 - t {
                    let p = add(self.w[1], scale(edge, t));
                    let d2 = dot(p, p);
                    if d2 < best {
                        result = 6;
                        self.bary[6][1] = 1.0 - t;
                        self.bary[6][2] = t;
                        best = d2;
                    }
                }
            }
        }
        for a in 0..count.saturating_sub(1) {
            for b in a + 1..count {
                let ab = dot(e[b], e[a]);
                let det = len2[b] * len2[a] - ab * ab;
                if DET_EPS < det {
                    let s = (-dots[a] * len2[b] - -dots[b] * ab) / det;
                    let t = (len2[a] * -dots[b] - ab * -dots[a]) / det;
                    let u = (1.0 - s) - t;
                    if 0.0 < s && 0.0 < t && 0.0 < u {
                        let p = add(add(wk, scale(e[a], s)), scale(e[b], t));
                        let d2 = dot(p, p);
                        if d2 < best {
                            result = 1 << idx[b] | 1 << idx[a] | kbit;
                            let r = result as usize;
                            self.bary[r][idx[a]] = s;
                            self.bary[r][idx[b]] = t;
                            self.bary[r][k] = u;
                            best = d2;
                        }
                    }
                }
            }
        }
        if count == 3 && self.lb2 <= 0.0 {
            let c = cross(e[0], e[1]);
            let det = dot(c, e[2]);
            if det.abs() <= DET_EPS {
                return result;
            }
            let a1 = -dot(c, wk) / det;
            if a1.is_nan() || a1 < 0.0 {
                return result;
            }
            let c2 = cross(wk, e[2]);
            let a2 = dot(c2, e[1]) / det;
            if a2 < 0.0 {
                return result;
            }
            let a3 = -dot(e[0], c2) / det;
            if 0.0 <= a3 {
                let rest = ((1.0 - a2) - a3) - a1;
                if !rest.is_nan() && rest >= 0.0 {
                    return FULL;
                }
            }
        }
        result
    }

    /// The point of the simplex's affine hull nearest the origin.
    fn v_from(&self, bits: u32) -> V3 {
        let mut p = [[0.0f32; 3]; 4];
        let mut n = 0;
        for i in 0..4 {
            if bits & 1 << i != 0 {
                p[n] = self.w[i];
                n += 1;
            }
        }
        if n == 1 {
            return p[0];
        }
        let e0 = sub(p[1], p[0]);
        if n == 2 {
            let t = dot(e0, p[0]) / dot(e0, e0);
            return sub(p[0], scale(e0, t));
        }
        let e2 = sub(p[2], p[0]);
        let normal = cross(e0, e2);
        let l2 = dot(normal, normal);
        if PLANE_EPS < l2 {
            return scale(normal, dot(normal, p[0]) / l2);
        }
        let edges = [e0, sub(p[2], p[1]), sub(p[0], p[2])];
        let lengths = edges.map(|edge| dot(edge, edge));
        let pick = if lengths[1] > lengths[0] {
            if lengths[2] <= lengths[1] { 1 } else { 2 }
        } else if lengths[2] <= lengths[0] {
            0
        } else {
            2
        };
        let base = p[pick];
        let edge = edges[pick];
        sub(base, scale(edge, dot(base, edge) / lengths[pick]))
    }

    fn closest(&self, bits: u32) -> (V3, V3) {
        let mut pa = [0.0; 3];
        let mut pb = [0.0; 3];
        let mut sum = 0.0;
        for i in 0..4 {
            if bits & 1 << i != 0 {
                let weight = self.bary[bits as usize][i];
                pa = add(pa, scale(self.a[i], weight));
                pb = add(pb, scale(self.b[i], weight));
                sum += weight;
            }
        }
        let inv = 1.0 / sum;
        (scale(pa, inv), scale(pb, inv))
    }

    fn penetration_dir(&self) -> V3 {
        let mut w = [[0.0f32; 3]; 12];
        let mut sum_a = [0.0; 3];
        let mut sum_b = [0.0; 3];
        let mut sum_w = [0.0; 3];
        for (k, dir) in PEN_DIRS.iter().enumerate() {
            let pa = self.support_a(*dir);
            let pb = self.support_b(neg(*dir));
            w[k] = sub(pa, pb);
            sum_a = add(sum_a, pa);
            sum_b = add(sum_b, pb);
            sum_w = add(sum_w, w[k]);
        }
        let twelfth = f32::from_bits(0x3daa_aaab);
        let centre = scale(sum_w, twelfth);
        let mut best: Option<V3> = None;
        let mut best_q = 0.0;
        for face in PEN_FACES {
            let p0 = w[face[0]];
            let mut n = cross(sub(w[face[1]], p0), sub(w[face[2]], p0));
            if dot(sub(p0, centre), n) < 0.0 {
                n = neg(n);
            }
            let l2 = dot(n, n);
            if TINY <= l2 {
                let s = dot(n, neg(p0));
                let mut q = s * s / l2;
                if s < 0.0 {
                    q = -q;
                }
                if best.is_none() || best_q < q {
                    best = Some(n);
                    best_q = q;
                }
            }
        }
        match best {
            Some(n) => neg(n),
            None => {
                let d = sub(scale(sum_a, twelfth), scale(sum_b, twelfth));
                if dot(d, d) <= TINY {
                    [1.0, 0.0, 0.0]
                } else {
                    d
                }
            }
        }
    }
}
