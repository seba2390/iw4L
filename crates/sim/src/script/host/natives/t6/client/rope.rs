//! Black Ops 2's client ropes: the map's rope constraints, each a chain of
//! particles laid from its start toward its end, pinned where the map pins
//! it, settled at the start and then, each client frame it is near enough to
//! be seen, moved one fixed step by gravity and the global wind and pulled
//! back to its links. `ropegetposition` reads a cubic Bezier curve through
//! the particles by its length.
//!
//! The wind a rope feels is one of 16 slots of the client's global wind
//! (`enable_global_wind`, `wind_global_vector`): each slot is the vector
//! scaled by one plus three entries of a 17-entry table of small offsets the
//! client draws from its random generator, seeded with 32767, whenever the
//! vector changes. A rope takes the slot of its number.
//!
//! t6.csc.ropes.cull: the client also skips a rope while both its ends lie
//! more than 100 units behind one of the view's cull planes; those planes
//! are the renderer's, so IW4L skips a rope by its distance alone.
//! t6.csc.ropes.frame: the client steps a rope once per rendered frame;
//! IW4L steps it once per client frame.
//! t6.csc.ropes.random: a close rope's wind scale is a fresh draw from the
//! client's random generator each step; IW4L draws from its own copy of
//! that generator, continued from the wind table's draws.

use bevy_ecs::prelude::{Resource, World};

/// A rope's particles are spaced by its length times this slack.
const SLACK: f32 = 0.9;
/// The map's rope length per particle, and the particle count's bounds.
const LENGTH_PER_PARTICLE: f32 = 25.0;
const MIN_PARTICLES: i32 = 4;
const MAX_PARTICLES: i32 = 25;
/// The step's time, gravity and the per-particle damping.
const STEP_SECONDS: f32 = 0.017;
const GRAVITY: f32 = -800.0;
/// The damping of a particle with no holding flags of its own (0.9 and 0.2
/// with them; a map rope's particles have none).
const DAMPING_FREE: f32 = 0.01;
/// A 15-bit draw's scale to an offset of at most a twentieth.
const OFFSET_SCALE: f32 = 0.1;
const DRAW_SCALE: f32 = 1.0 / 32767.0;
/// A particle that moved no more than this (squared) is still.
const STILL: f32 = 0.0625;
/// Wind stronger than this (squared, across the ground) wakes a rope.
const STRONG_WIND: f32 = 22500.0;
/// A rope is stepped within this distance (squared) of a view, and takes a
/// random wind scale within the nearer one.
const SIMULATE_RANGE: f32 = 64_000_000.0;
const GUST_RANGE: f32 = 2_560_000.0;
/// The steps a rope settles for when it is made, at most.
const SETTLE_STEPS: i32 = 1000;

/// A rope's flag bits, as the client keeps them.
mod flags {
    pub const SLEEPING: u32 = 0x2;
    /// Its render points have been built twice, so it may sleep.
    pub const MAY_SLEEP: u32 = 0x4;
    /// A pin changed: it settles before its next step.
    pub const UNSETTLED: u32 = 0x8;
    /// A view is near: its wind scale is random.
    pub const GUSTY: u32 = 0x200;
    pub const STRONG_WIND: u32 = 0x400;
}

#[derive(Clone, Copy)]
struct Particle {
    pos: [f32; 3],
    prev: [f32; 3],
}

#[derive(Clone, Copy)]
enum Link {
    /// Two neighbouring particles held at the rope's spacing.
    Spacing(usize, usize),
    /// A particle held at a world point.
    Pin(usize, [f32; 3]),
}

struct Rope {
    name: String,
    particles: Vec<Particle>,
    links: Vec<Link>,
    spacing: f32,
    wind_scale: f32,
    flags: u32,
    still_frames: i32,
    /// Which of its two render point buffers it last built.
    render_buffer: u8,
}

/// The client's 16 wind slots and the offsets they are built from.
#[derive(Default)]
struct Wind {
    vector: Option<[f32; 3]>,
    offsets: [f32; 17],
    slots: [[f32; 3]; 16],
    /// When each slot next takes its vector again.
    renew_ms: [i64; 16],
}

#[derive(Resource, Default)]
pub(super) struct ClientRopes {
    made: bool,
    ropes: Vec<Rope>,
    wind: Wind,
    /// The client's random generator's state.
    seed: u32,
    last_ms: i64,
}

impl ClientRopes {
    /// The client's `rand`: the next 15-bit draw.
    fn rand(&mut self) -> i32 {
        self.seed = self.seed.wrapping_mul(0x343fd).wrapping_add(0x269ec3);
        ((self.seed >> 16) & 0x7fff) as i32
    }
}

/// The map's rope lines (`physconstraints/ropes.txt`).
fn rope_lines(world: &World) -> Vec<String> {
    crate::script::host::client_scripts::with_server(world, |server| {
        server
            .resource::<crate::script::RoundScript>()
            .restart
            .as_ref()?
            .raw_files
            .get("physconstraints/ropes.txt")
            .map(|text| text.lines().map(str::to_owned).collect::<Vec<_>>())
    })
    .flatten()
    .unwrap_or_else(Vec::new)
}

/// The rope numbered `index` among the map's ropes by its targetname.
pub(super) fn find(world: &mut World, name: &str) -> i32 {
    ensure(world);
    world
        .resource::<ClientRopes>()
        .ropes
        .iter()
        .position(|rope| rope.name == name)
        .map_or(-1, |index| index as i32)
}

fn ensure(world: &mut World) {
    if world
        .get_resource::<ClientRopes>()
        .is_some_and(|ropes| ropes.made)
    {
        return;
    }
    let lines = rope_lines(world);
    let mut ropes = ClientRopes {
        made: true,
        ..Default::default()
    };
    for line in &lines {
        if let Some(rope) = make(line) {
            ropes.ropes.push(rope);
        }
    }
    for index in 0..ropes.ropes.len() {
        settle(&mut ropes, index);
    }
    world.insert_resource(ropes);
}

/// A rope as the client makes it from the map's constraint: as many
/// particles as its length holds (4 to 25), laid from its start toward its
/// end (straight down when they meet) at its spacing, linked in a chain,
/// its start pinned and its end pinned when it is elsewhere.
fn make(line: &str) -> Option<Rope> {
    let mut fields = line.split('\t');
    let name = fields.next()?.to_owned();
    let pair = |text: Option<&str>| -> Option<[i32; 2]> {
        let mut words = text?.split(' ').map(|w| w.parse::<i32>().ok());
        Some([words.next()??, words.next()??])
    };
    let point = |text: Option<&str>| -> Option<[f32; 3]> {
        let mut words = text?.split(' ').map(|w| w.parse::<f32>().ok());
        Some([words.next()??, words.next()??, words.next()??])
    };
    let [start_type, _] = pair(fields.next())?;
    let [end_type, _] = pair(fields.next())?;
    let start = point(fields.next())?;
    let end = point(fields.next())?;
    let length: f32 = fields.next()?.parse().ok()?;
    let wind_scale: f32 = fields.next()?.parse().ok()?;
    let count = ((length / LENGTH_PER_PARTICLE) as i32).clamp(MIN_PARTICLES, MAX_PARTICLES);
    let mut dir = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
    let mut len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    if len < 0.001 {
        dir = [0.0, 0.0, -1.0];
        len = 1.0;
    }
    let inverse = 1.0 / len;
    let spacing = (length * SLACK) / (count - 1) as f32;
    let step = [
        dir[0] * inverse * spacing,
        inverse * dir[1] * spacing,
        dir[2] * inverse * spacing,
    ];
    let mut at = start;
    let mut particles = Vec::with_capacity(count as usize);
    for _ in 0..count {
        particles.push(Particle { pos: at, prev: at });
        at = [at[0] + step[0], at[1] + step[1], at[2] + step[2]];
    }
    let last = particles.len() - 1;
    let mut links: Vec<Link> = (0..last).map(|i| Link::Spacing(i, i + 1)).collect();
    // t6.csc.ropes.attach: a rope attached to an entity or a dynamic entity
    // (types 1 and 2) is not attached, and the flag that makes a rope collide
    // with the world is not read; every rope of the installed maps is pinned
    // to the world (type 0) and none collides.
    if start_type == 0 {
        links.push(Link::Pin(0, start));
    }
    if end_type == 0 && start != end {
        links.push(Link::Pin(last, end));
    }
    Some(Rope {
        name,
        particles,
        links,
        spacing,
        wind_scale: if wind_scale <= 0.0 { 1.0 } else { wind_scale },
        flags: flags::UNSETTLED,
        still_frames: 0,
        render_buffer: 0,
    })
}

/// The settling a rope gets when it is made: steps until it has been still
/// for 30 of them, at most a thousand.
fn settle(ropes: &mut ClientRopes, index: usize) {
    for _ in 0..SETTLE_STEPS {
        if ropes.ropes[index].still_frames > 29 {
            return;
        }
        if ropes.ropes[index].flags & flags::SLEEPING == 0 {
            ropes.ropes[index].flags &= !flags::UNSETTLED;
            advance(ropes, index);
        }
    }
}

/// One step of a rope, and its links: gravity and its wind slot move each
/// particle (damped by what holds it), its stillness is counted, then six
/// passes over its links, backward first.
fn advance(ropes: &mut ClientRopes, index: usize) {
    step(ropes, index);
    let rope = &mut ropes.ropes[index];
    let still = rope.particles.iter().all(|p| {
        let d = [
            p.pos[0] - p.prev[0],
            p.pos[1] - p.prev[1],
            p.pos[2] - p.prev[2],
        ];
        STILL >= d[1] * d[1] + d[0] * d[0] + d[2] * d[2]
    });
    if still {
        rope.still_frames += 1;
        if rope.still_frames > 30 && rope.flags & flags::MAY_SLEEP != 0 {
            rope.flags |= flags::SLEEPING;
        }
    } else {
        rope.still_frames = 0;
    }
    let mut backward = true;
    for _ in 0..6 {
        let links = rope.links.clone();
        if backward {
            for link in links.iter().rev() {
                apply(rope, *link);
            }
        } else {
            for link in &links {
                apply(rope, *link);
            }
        }
        backward = !backward;
    }
}

fn step(ropes: &mut ClientRopes, index: usize) {
    let slot = ropes.wind.slots[index % 16];
    let (mut wx, mut wy) = (slot[0], slot[1]);
    let rope_flags = &mut ropes.ropes[index].flags;
    if wy * wy + wx * wx <= STRONG_WIND {
        *rope_flags &= !flags::STRONG_WIND;
    } else {
        *rope_flags |= flags::STRONG_WIND;
    }
    let scale = if ropes.ropes[index].flags & flags::GUSTY != 0 {
        (ropes.rand() % 101) as f32 * 0.01 * 0.4 + 0.3
    } else {
        0.5
    };
    let rope = &mut ropes.ropes[index];
    wx = wx * scale * rope.wind_scale;
    wy = wy * scale * rope.wind_scale;
    let dt2 = STEP_SECONDS * STEP_SECONDS;
    let gravity = dt2 * GRAVITY;
    let keep = 2.0 - DAMPING_FREE;
    let lose = 1.0 - DAMPING_FREE;
    for particle in &mut rope.particles {
        let pos = particle.pos;
        particle.pos = [
            (pos[0] * keep + wx * dt2) - particle.prev[0] * lose,
            (pos[1] * keep + wy * dt2) - particle.prev[1] * lose,
            (pos[2] * keep + gravity) - particle.prev[2] * lose,
        ];
        particle.prev = pos;
    }
}

fn apply(rope: &mut Rope, link: Link) {
    match link {
        Link::Spacing(a, b) => {
            let pa = rope.particles[a].pos;
            let pb = rope.particles[b].pos;
            let d = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
            let len = (d[1] * d[1] + d[0] * d[0] + d[2] * d[2]).sqrt();
            let inverse = 1.0 / len;
            let half = (rope.spacing + len) * 0.5;
            rope.particles[b].pos = [
                d[0] * inverse * half + pa[0],
                d[1] * inverse * half + pa[1],
                d[2] * inverse * half + pa[2],
            ];
            let back = -half;
            rope.particles[a].pos = [
                d[0] * inverse * back + pb[0],
                d[1] * inverse * back + pb[1],
                d[2] * inverse * back + pb[2],
            ];
        }
        Link::Pin(p, at) => {
            rope.particles[p] = Particle { pos: at, prev: at };
        }
    }
}

/// The client's wind for the frame: while `enable_global_wind` is set and
/// the game is not paused, a new `wind_global_vector` rebuilds the offset
/// table (the generator seeded with 32767, 17 draws, then each slot's own
/// draws), and each slot whose time has come takes the vector, scaled by one
/// plus its three offsets, for 30 seconds.
fn dvar_text(world: &World, name: &str) -> String {
    world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get(name)
        .cloned()
        .or_else(|| super::super::dvars::lookup(name).map(|spec| spec.default.to_owned()))
        .unwrap_or_else(String::new)
}

fn update_wind(world: &World, ropes: &mut ClientRopes, now: i64) {
    let dvar = |name: &str| dvar_text(world, name);
    if super::super::atoi(&dvar("enable_global_wind")) == 0
        || super::super::atoi(&dvar("cl_paused")) != 0
    {
        return;
    }
    let text = dvar("wind_global_vector");
    let mut words = text.split_whitespace().map(super::super::atof);
    let vector: [f32; 3] = std::array::from_fn(|_| words.next().unwrap_or(0.0));
    if ropes.wind.vector != Some(vector) {
        ropes.wind.vector = Some(vector);
        ropes.seed = 0x7fff;
        let mut offsets = [0.0; 17];
        for offset in &mut offsets {
            *offset = (ropes.rand() - 0x3fff) as f32 * OFFSET_SCALE * DRAW_SCALE;
        }
        ropes.wind.offsets = offsets;
        // Each slot's gust and sway draws.
        for slot in 0..16 {
            for _ in 0..7 {
                ropes.rand();
            }
            ropes.wind.renew_ms[slot] = now - 30000 + 0x753 * slot as i64;
        }
    }
    for slot in 0..16 {
        if ropes.wind.renew_ms[slot] <= now {
            ropes.wind.renew_ms[slot] = now + 30000;
            let offsets = &ropes.wind.offsets;
            let r = [
                offsets[slot % 17],
                offsets[(slot + 1) % 17],
                offsets[(slot + 2) % 17],
            ];
            ropes.wind.slots[slot] = std::array::from_fn(|i| vector[i] * r[i] + vector[i]);
        }
    }
}

/// The client's frame: its wind, then each rope the view is near enough to
/// see: unless it sleeps, a settling if a pin changed, one step, its links,
/// and the rebuild of its render points (on every second rebuild it may
/// sleep).
pub(super) fn frame(world: &mut World, now: i64, eye: [f32; 3]) {
    ensure(world);
    let Some(mut ropes) = world.remove_resource::<ClientRopes>() else {
        return;
    };
    update_wind(world, &mut ropes, now);
    let disabled = super::super::atoi(&dvar_text(world, "disable_rope")) != 0;
    if !disabled && now - ropes.last_ms > 0 {
        ropes.last_ms = now;
        for index in 0..ropes.ropes.len() {
            // A rope whose pin changed is stepped whatever the view.
            if ropes.ropes[index].flags & flags::UNSETTLED == 0
                && !seen(&mut ropes.ropes[index], eye)
            {
                continue;
            }
            if ropes.ropes[index].flags & flags::SLEEPING == 0 {
                if ropes.ropes[index].flags & flags::UNSETTLED != 0 {
                    settle(&mut ropes, index);
                }
                ropes.ropes[index].flags &= !flags::UNSETTLED;
                advance(&mut ropes, index);
            }
            let rope = &mut ropes.ropes[index];
            if rope.flags & flags::SLEEPING == 0 {
                rope.render_buffer ^= 1;
                if rope.render_buffer == 0 {
                    rope.flags |= flags::MAY_SLEEP;
                }
            }
        }
    }
    world.insert_resource(ropes);
}

/// Whether a view sees the rope: its eye's distance (squared) from the line
/// between the rope's ends is within range; a near one gusts, and wakes in a
/// strong wind.
fn seen(rope: &mut Rope, eye: [f32; 3]) -> bool {
    rope.flags &= !flags::GUSTY;
    let (Some(first), Some(last)) = (rope.particles.first(), rope.particles.last()) else {
        return false;
    };
    let a = first.pos;
    let b = last.pos;
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ae = [eye[0] - a[0], eye[1] - a[1], eye[2] - a[2]];
    let be = [eye[0] - b[0], eye[1] - b[1], eye[2] - b[2]];
    let along = ae[1] * ab[1] + ae[0] * ab[0] + ae[2] * ab[2];
    let distance = if 0.0 < along {
        let span = ab[1] * ab[1] + ab[0] * ab[0] + ab[2] * ab[2];
        if along < span {
            (ae[1] * ae[1] + ae[0] * ae[0] + ae[2] * ae[2]) - (along * along) / span
        } else {
            be[1] * be[1] + be[0] * be[0] + be[2] * be[2]
        }
    } else {
        ae[1] * ae[1] + ae[0] * ae[0] + ae[2] * ae[2]
    };
    if distance >= SIMULATE_RANGE {
        return false;
    }
    if distance < GUST_RANGE {
        rope.flags |= flags::GUSTY;
        if rope.flags & flags::STRONG_WIND != 0 {
            rope.flags &= !flags::SLEEPING;
            rope.still_frames = 0;
        }
    }
    true
}

/// `ropegetposition`: the point `fraction` of the way along the rope's
/// curve by length, a cubic Bezier curve through its particles (the last
/// repeated until the segments come out whole); `None` for a rope that is
/// not there.
pub(super) fn position(world: &mut World, index: i32, fraction: f32) -> Option<[f32; 3]> {
    ensure(world);
    let ropes = world.resource::<ClientRopes>();
    let rope = ropes.ropes.get(usize::try_from(index).ok()?)?;
    let mut points: Vec<[f32; 3]> = rope.particles.iter().map(|p| p.pos).collect();
    let &last = points.last()?;
    let pad = if points.len() < 4 {
        4 - points.len()
    } else {
        match (points.len() - 1) % 3 {
            0 => 0,
            r => 3 - r,
        }
    };
    points.extend(std::iter::repeat_n(last, pad));
    // Each point's distance along the curve.
    let mut along = vec![0.0f32; points.len()];
    let mut total = 0.0f32;
    for i in 1..points.len() {
        let d = [
            points[i - 1][0] - points[i][0],
            points[i - 1][1] - points[i][1],
            points[i - 1][2] - points[i][2],
        ];
        let segment = (d[1] * d[1] + d[0] * d[0] + d[2] * d[2]).sqrt();
        total += segment;
        along[i] = total;
    }
    let s = total * fraction.clamp(0.0, 1.0);
    let mut reached = 0;
    while reached < points.len() && reached < 300 && along[reached] < s {
        reached += 1;
    }
    let (first, t) = if reached < 4 {
        let end = *along.get(3)?;
        (0, if end > 1e-6 { s / end } else { 1.0 })
    } else {
        let rounded = (f64::from((reached - 4) as f32 * 0.333_333_34) - 0.499_999_999_068_677_4)
            .round_ties_even();
        let first = (rounded as usize) * 3 + 3;
        let first = if first + 3 >= 300 { 0x128 } else { first };
        let base = *along.get(first)?;
        (first, (s - base) / (*along.get(first + 3)? - base))
    };
    if *along.get(first + 3)? <= 1e-6 {
        return Some(points[first]);
    }
    let [p0, p1, p2, p3] = [
        points[first],
        points[first + 1],
        points[first + 2],
        points[first + 3],
    ];
    let u = 1.0 - t;
    let c2 = t * 3.0 * t * u;
    let c1 = t * 3.0 * u * u;
    let c3 = t * t * t;
    let c0 = u * u * u;
    Some([
        p0[0] * c0 + p1[0] * c1 + p2[0] * c2 + p3[0] * c3,
        p1[1] * c1 + p0[1] * c0 + p2[1] * c2 + p3[1] * c3,
        p1[2] * c1 + p0[2] * c0 + p2[2] * c2 + p3[2] * c3,
    ])
}
