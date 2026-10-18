//! Black Ops 2's grenades: thrown or launched along the aim with the
//! thrower's speed added, falling under gravity, bouncing off each surface
//! by its 32-surface tables (rolling, bumping and wobbling when they roll),
//! coming to rest with `stationary` (or sticking with `grenade_stuck`), and
//! exploding with the surface they lie on (`G_ExplodeMissile`).

use fastfile_t6::weapon::{WeaponDefView, def};
use game_api::fire::{
    BulletTrace, FireDamage, FireEntity, FireEntityKind, FireWorld, GAME_MISSILE_BYTES,
    GameMissile, GameTrajectory, MissileExplosion, MissileRun, NotifyArg, RadiusDamage,
    TrajectoryKind,
};
use game_api::movement::{ENTITY_NONE, ENTITY_WORLD};

use crate::missile::{
    DEG2RAD, HIT_LOCATIONS, MEANS, MOD_EXPLOSIVE, MOD_GRENADE_SPLASH, MOD_PROJECTILE_SPLASH,
    SURFACES, State, angle_vectors, clipmask, evaluate, evaluate_delta, hit_id, in_world, mad,
    missile_trace, sentient, surface_type, vectoangles,
};

const REST_SLOPE: f32 = 0.7;
const DEFAULT_FUSE_MS: i32 = 30_000;
const MAX_FUSE_MS: i32 = 60_000;
const GROUND_WAIT_MS: i32 = 60_000;
const GROUND_RETRY: i32 = 50;
const KEEP_ROLLING_SPEED: f32 = 30.0;
const KEEP_ROLLING_BOOST: f32 = 0.5;
const ROLL_RADIUS: f32 = 5.0;
const TWO_PI: f32 = 6.283_18;
const ROLL_STRIKE: f32 = 25.0;
const BOUNCE_HEARD: f32 = 100.0;
const CONTENTS_WATER: u32 = 0x20;
const SURF_FLESH: u8 = 7;
const EXPLOSION_FLOOR: f32 = 16.0;
const SIGHT_MASK: u32 = 0x0080_2013;
const BOUNDED_CONTENTS: u32 = 0x405c_0008;
const BLAST_FLAGS: i32 = 0x5;
const PLAYER_SIDE: f32 = 15.0;
const AIM_HEIGHT: f32 = 24.0;

pub(crate) fn dvar(world: &dyn FireWorld, name: &str, default: f32) -> f32 {
    world
        .dvar(&name.to_ascii_lowercase())
        .or_else(|| world.dvar(name))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

fn rolling_enabled(world: &dyn FireWorld) -> bool {
    dvar(world, "grenadeRollingEnabled", 1.0) != 0.0
}

fn random(world: &mut dyn FireWorld) -> f32 {
    (f64::from(world.rand()) * f64::from(3.051_757_8e-5_f32)) as f32
}

fn irand(world: &mut dyn FireWorld, min: i32, max: i32) -> i32 {
    ((i64::from(world.rand()) * i64::from(max - min)) >> 15) as i32 + min
}

fn flrand(world: &mut dyn FireWorld, min: f32, max: f32) -> f32 {
    (f64::from(world.rand()) * (f64::from(max) - f64::from(min)) * f64::from(3.051_757_8e-5_f32)
        + f64::from(min)) as f32
}

fn normalize_turns(angle: f32) -> f32 {
    let turns = angle * 0.002_777_777_8;
    turns - (turns + 0.5).floor()
}

fn normalize_360(angle: f32) -> f32 {
    let turns = angle * 0.002_777_777_8;
    (turns - turns.floor()) * 360.0
}

fn length(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[2] * b[2] + a[1] * b[1] + a[0] * b[0]
}

fn lerp(from: [f32; 3], to: [f32; 3], fraction: f32) -> [f32; 3] {
    [
        (to[0] - from[0]) * fraction + from[0],
        (to[1] - from[1]) * fraction + from[1],
        (to[2] - from[2]) * fraction + from[2],
    ]
}

pub(crate) fn gravity(world: &dyn FireWorld) -> f32 {
    dvar(world, "bg_gravity", 800.0)
}

pub(crate) fn splash_means(def: &WeaponDefView<'_>, grenade: bool) -> usize {
    if !grenade {
        return MOD_PROJECTILE_SPLASH;
    }
    if def.def_i32(def::PROJ_EXPLOSION) == 6 {
        MOD_EXPLOSIVE
    } else {
        MOD_GRENADE_SPLASH
    }
}

/// `Weapon_ThrowGrenade` and `Weapon_GrenadeLauncher_Fire`; `time_left` is
/// the throwing player's cooked fuse (`grenadeTimeLeft`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn launch_grenade(
    owner: i32,
    weapon: u32,
    def: &WeaponDefView<'_>,
    origin: [f32; 3],
    angles: [f32; 3],
    launcher: bool,
    time_left: Option<i32>,
    world: &mut dyn FireWorld,
) -> Option<i32> {
    let (forward, _, up) = angle_vectors(angles);
    let speed = def.def_i32(def::PROJECTILE_SPEED) as f32;
    let speed_up = def.def_i32(def::PROJECTILE_SPEED_UP) as f32;
    let mut vel = if launcher {
        [
            forward[0] * speed,
            forward[1] * speed,
            forward[2] * speed + speed_up,
        ]
    } else {
        let relative_up = def.def_i32(def::PROJECTILE_SPEED_RELATIVE_UP) as f32;
        let mut v = [
            up[0] * relative_up + forward[0] * speed,
            up[1] * relative_up + forward[1] * speed,
            speed_up + up[2] * relative_up + forward[2] * speed,
        ];
        let push_fwd = def.def_i32(def::PROJECTILE_SPEED_FORWARD);
        if push_fwd != 0 {
            let flat = forward[1] * forward[1] + forward[0] * forward[0];
            let guess = f32::from_bits(0x5f37_59df_u32.wrapping_sub(flat.to_bits() >> 1));
            let inv = (1.5 - flat * 0.5 * guess * guess) * guess;
            let push = push_fwd as f32;
            v[0] += push * forward[0] * inv;
            v[1] += forward[1] * inv * push;
        }
        v
    };
    let player = world.player(owner);
    let fuse = if player.is_some() {
        def.def_i32(def::FUSE_TIME)
    } else {
        def.def_i32(def::AI_FUSE_TIME)
    };
    let time_left = player.map(|_| time_left.unwrap_or(0));
    let len = length(vel);
    let len = if -len >= 0.0 { 1.0 } else { len };
    let inv = 1.0 / len;
    let carried = player.map_or([0.0; 3], |player| {
        let pv = player.velocity;
        let along = pv[1] * vel[1] * inv + pv[2] * inv * vel[2] + pv[0] * inv * vel[0];
        [
            inv * vel[0] * along,
            vel[1] * inv * along,
            along * inv * vel[2],
        ]
    });
    // `G_FireGrenade` truncates the base launch velocity before the caller
    // adds the thrower's projected velocity, then a moving platform's.
    for c in &mut vel {
        *c = *c as i32 as f32;
    }
    let platform = platform_velocity(owner, world);
    spawn_grenade(
        owner,
        weapon,
        def,
        origin,
        vel,
        [carried, platform],
        !launcher,
        fuse,
        time_left,
        world,
    )
}

/// What a moving platform a player stands on adds to what they throw or
/// fire: its travel over the last 200 ms of level time as a direction, times
/// that travel's length × 1000 × 0.005 (the x87 product rounded once to
/// single precision). Nothing when the thrower is no player or stands on no
/// platform the match keeps a history of.
pub(crate) fn platform_velocity(owner: i32, world: &mut dyn FireWorld) -> [f32; 3] {
    if world.player(owner).is_none() {
        return [0.0; 3];
    }
    let now = world.level_time();
    let (Some(then), Some(at)) = (
        world.platform_at(owner, now - 200),
        world.platform_at(owner, now),
    ) else {
        return [0.0; 3];
    };
    let mut d = [at[0] - then[0], at[1] - then[1], at[2] - then[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let inv = 1.0 / if -len >= 0.0 { 1.0 } else { len };
    for c in &mut d {
        *c *= inv;
    }
    let scale = x87_product_f32(f64::from(len) * 1000.0, f64::from(0.005_f32));
    let velocity = [d[0] * scale, d[1] * scale, d[2] * scale];
    diag::info!(
        Sim,
        "fired from a moving platform: {:.2} units in 200 ms, adds {velocity:?}",
        len
    );
    velocity
}

/// `a × b` rounded once to single precision, as the x87 unit's exact product
/// is when it is stored: the double product, its tie broken by the error
/// the double rounding dropped.
fn x87_product_f32(a: f64, b: f64) -> f32 {
    let p = a * b;
    let error = a.mul_add(b, -p);
    let r = p as f32;
    let r64 = f64::from(r);
    if error == 0.0 || p == r64 {
        return r;
    }
    let other = if p > r64 {
        f32::from_bits(
            r.to_bits()
                .wrapping_add(if r >= 0.0 { 1 } else { -1i32 as u32 }),
        )
    } else {
        f32::from_bits(
            r.to_bits()
                .wrapping_add(if r >= 0.0 { -1i32 as u32 } else { 1 }),
        )
    };
    if (r64 + f64::from(other)) * 0.5 == p && (error > 0.0) == (f64::from(other) > r64) {
        other
    } else {
        r
    }
}

/// `G_FireGrenade` takes a player's cooked fuse, and clears it, unless the
/// grenade arms by distance and has no fuse.
pub(crate) fn takes_time_left(def: &WeaponDefView<'_>, fuse: i32) -> bool {
    def.def_i32(def::PROJECTILE_ACTIVATE_DIST) == 0 || fuse >= 1
}

/// `G_FireGrenade`; `time_left` is the throwing player's cooked fuse, `None`
/// when no player throws it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_grenade(
    owner: i32,
    weapon: u32,
    def: &WeaponDefView<'_>,
    start: [f32; 3],
    vel: [f32; 3],
    // What the caller adds to the truncated velocity, in its order: the
    // thrower's projected velocity, then a moving platform's.
    added: [[f32; 3]; 2],
    rotate: bool,
    fuse: i32,
    time_left: Option<i32>,
    world: &mut dyn FireWorld,
) -> Option<i32> {
    let level = world.level_time();
    let angles = vectoangles(vel);
    let [carried, platform] = added;
    let delta: [f32; 3] =
        std::array::from_fn(|i| platform[i] + (vel[i] as i32 as f32 + carried[i]));
    let spin = if rotate && def.def_bool(def::ROTATE) {
        let base = [normalize_360(angles[0] - 120.0), angles[1], angles[2]];
        let delta = if def.def_i32(def::ROTATE_TYPE) == 1 {
            [1500.0, 0.0, 0.0]
        } else {
            let sign = (irand(world, 0, 2) * 2 - 1) as f32;
            let pitch = (flrand(world, 340.0, 800.0) * sign) as i32 as f32;
            let sign = (irand(world, 0, 2) * 2 - 1) as f32;
            let roll = (flrand(world, 180.0, 540.0) * sign) as i32 as f32;
            [pitch, 0.0, roll]
        };
        GameTrajectory {
            kind: TrajectoryKind::Linear,
            time: level,
            duration: 0,
            base,
            delta,
        }
    } else {
        GameTrajectory {
            kind: TrajectoryKind::Stationary,
            time: level,
            duration: 0,
            base: angles,
            delta: [0.0; 3],
        }
    };
    let hidden = if world.player(owner).is_some() {
        ((length(vel) * -35.0 * 0.001_666_666_7 + 85.0) as i32).clamp(20, 50)
    } else {
        0
    };
    let mut explode_at = 0;
    if takes_time_left(def, fuse) {
        match time_left {
            Some(_) if !def.def_bool(def::TIMED_DETONATION) => {}
            Some(left) if left != 0 => explode_at = level + left,
            _ => explode_at = level + fuse,
        }
    }
    if explode_at == 0 {
        explode_at = level + DEFAULT_FUSE_MS;
    }
    explode_at = explode_at.min(level + MAX_FUSE_MS);
    let state = State {
        bounce: true,
        grenade: true,
        ground: ENTITY_NONE,
        explode_at,
        spawned_at: level,
        ..State::default()
    };
    let mut game = [0u8; GAME_MISSILE_BYTES];
    state.write(&mut game);
    world.spawn_missile(GameMissile {
        entity: ENTITY_NONE,
        owner,
        weapon,
        position: GameTrajectory {
            kind: TrajectoryKind::Gravity(gravity(world)),
            time: level,
            duration: 0,
            base: start,
            delta,
        },
        angles: spin,
        origin: start,
        visible_at: level + hidden,
        game,
    })
}

/// A grenade resting on something that is no longer under it starts falling.
pub(crate) fn start_falling(
    missile: &mut GameMissile,
    state: &mut State,
    def: &WeaponDefView<'_>,
    world: &mut dyn FireWorld,
) {
    if missile.position.kind != TrajectoryKind::Stationary || state.ground == ENTITY_WORLD {
        return;
    }
    if !matches!(def.def_i32(def::STICKINESS), 0 | 2 | 3 | 4) {
        return;
    }
    let start = missile.origin;
    let end = mad(start, state.rest_normal, -1.635);
    let trace = missile_trace(start, end, missile.owner, clipmask(def), world);
    if trace.fraction == 1.0 {
        let level = world.level_time();
        missile.position = GameTrajectory {
            kind: TrajectoryKind::Gravity(gravity(world)),
            time: level,
            duration: 0,
            base: missile.origin,
            delta: [0.0; 3],
        };
        state.ground = ENTITY_NONE;
    }
}

pub(crate) fn impact_time(trace: &BulletTrace, world: &dyn FireWorld) -> i32 {
    let level = world.level_time();
    let prev = world.previous_level_time();
    ((level - prev) as f32 * trace.fraction) as i32 + prev
}

/// `G_MissileImpact`'s bounce branch.
pub(crate) fn bounce(
    missile: &mut GameMissile,
    def: &WeaponDefView<'_>,
    state: &mut State,
    trace: &BulletTrace,
    end: [f32; 3],
    hit: Option<FireEntity>,
    world: &mut dyn FireWorld,
) {
    if !bounce_off(missile, def, state, trace, hit, world) || trace.start_solid {
        return;
    }
    let level = world.level_time();
    world.set_entity_pose(
        missile.entity,
        missile.origin,
        evaluate(&missile.angles, level),
    );
    let target = hit_id(trace);
    if state.surface == 0 && hit.is_some_and(sentient) {
        state.surface = SURF_FLESH;
    }
    let surf_name = SURFACES
        .get(usize::from(state.surface))
        .copied()
        .unwrap_or(SURFACES[0]);
    let stickiness = def.def_i32(def::STICKINESS);
    let stuck_to_ent = matches!(stickiness, 1 | 5)
        || (stickiness == 2 && target != ENTITY_WORLD && target != ENTITY_NONE);
    if stuck_to_ent && target != ENTITY_WORLD && target != ENTITY_NONE {
        world.notify(
            target,
            "grenade_stuck",
            &[NotifyArg::Entity(missile.entity)],
        );
        world.notify(
            missile.entity,
            "stationary",
            &[
                NotifyArg::Vector(end),
                NotifyArg::Vector(trace.normal),
                NotifyArg::String(surf_name),
            ],
        );
    } else {
        let hit_ent = if target == ENTITY_WORLD {
            ENTITY_NONE
        } else {
            target
        };
        world.notify(
            missile.entity,
            "grenade_bounce",
            &[
                NotifyArg::Vector(end),
                NotifyArg::Vector(trace.normal),
                NotifyArg::Entity(hit_ent),
                NotifyArg::String(surf_name),
            ],
        );
    }
    world.bounce(missile.entity, state.surface);
}

/// `G_BounceGrenade`.
fn bounce_off(
    missile: &mut GameMissile,
    def: &WeaponDefView<'_>,
    state: &mut State,
    trace: &BulletTrace,
    hit: Option<FireEntity>,
    world: &mut dyn FireWorld,
) -> bool {
    let level = world.level_time();
    let in_water = world.point_contents(missile.origin, CONTENTS_WATER) != 0;
    let n = trace.normal;
    let vel = evaluate_delta(&missile.position, impact_time(trace, world));
    let strike = dot(n, vel);
    let rolls = rolling_enabled(world) && def.def_i32(def::IS_ROLLING_GRENADE) != 0 && !state.dud;
    let k = if rolls { -strike } else { strike * -2.0 };
    let mut delta = [n[0] * k + vel[0], k * n[1] + vel[1], k * n[2] + vel[2]];
    let struck = hit_id(trace);
    let stickiness = def.def_i32(def::STICKINESS);
    let is_player = hit.is_some_and(|e| e.kind == FireEntityKind::Player);
    let mut can_rest = matches!(stickiness, 0 | 1) || !is_player;
    if stickiness != 4
        && struck != ENTITY_WORLD
        && struck != ENTITY_NONE
        && state.surface == SURF_FLESH
    {
        can_rest = false;
    }
    if can_rest && (matches!(stickiness, 1 | 2) || n[2] > REST_SLOPE) {
        state.ground = struck;
    }
    let rolling = surface_bounce(
        &mut delta,
        strike,
        state.surface,
        missile.weapon,
        def,
        n,
        rolls,
        state.dud,
        world,
    );
    if let Some((speed, dir)) = rolling {
        roll(missile, state, def, n, &mut delta, speed, dir, world);
    }
    missile.position.delta = delta;
    let speed = length(delta);
    if can_rest
        && (matches!(stickiness, 1 | 2)
            || (speed < 1.0 || n[2] > REST_SLOPE)
                && (matches!(stickiness, 3 | 4)
                    || speed < dvar(world, "grenadeRestThreshold", 20.0)))
    {
        missile.position = GameTrajectory {
            kind: TrajectoryKind::Stationary,
            time: 0,
            duration: 0,
            base: missile.origin,
            delta: [0.0; 3],
        };
        let angles = match stickiness {
            2 | 4 => stick_angles(missile, trace, true, world),
            3 => stick_angles(missile, trace, false, world),
            _ => land_angles(missile, def, state, trace, true, false, world),
        };
        missile.angles = GameTrajectory {
            kind: TrajectoryKind::Stationary,
            time: 0,
            duration: 0,
            base: angles,
            delta: [0.0; 3],
        };
        state.rest_normal = n;
        if !def.def_bool(def::TIMED_DETONATION) {
            state.explode_at = 0;
        }
        world.set_entity_pose(missile.entity, missile.origin, angles);
        world.notify(missile.entity, "stationary", &[]);
        if def.def_bool(def::EXPLODE_WHEN_STATIONARY) {
            state.explode_at = level + 1;
        }
        return true;
    }
    let o = missile.origin;
    let target = [
        n[0] * 0.1 + o[0],
        n[1] * 0.1 + o[1],
        (n[2] * 0.1).min(0.0) + o[2],
    ];
    let t = missile_trace(o, target, missile.owner, clipmask(def), world);
    missile.origin = lerp(o, target, t.fraction);
    missile.position.base = missile.origin;
    missile.position.time = level;
    if rolling.is_none() && dot(delta, n) > ROLL_STRIKE {
        let (rest, by_type) = match def.def_i32(def::ROTATE_TYPE) {
            1 => (false, true),
            2 => (true, false),
            _ => (false, false),
        };
        missile.angles.base = land_angles(missile, def, state, trace, rest, by_type, world);
        missile.angles.time = level;
    }
    let change = [delta[0] - vel[0], delta[1] - vel[1], delta[2] - vel[2]];
    !in_water && length(change) > BOUNCE_HEARD
}

/// The surface's bounce coefficients and rolling friction.
#[allow(clippy::too_many_arguments)]
fn surface_bounce(
    delta: &mut [f32; 3],
    strike: f32,
    surface: u8,
    weapon: u32,
    def: &WeaponDefView<'_>,
    n: [f32; 3],
    rolls: bool,
    dud: bool,
    world: &mut dyn FireWorld,
) -> Option<(f32, [f32; 3])> {
    if !(length(*delta) > 0.0 && strike <= 0.0) {
        return None;
    }
    let coefficient = |world: &dyn FireWorld, parallel: bool| {
        world.weapon(weapon).and_then(|w| {
            let table = if parallel {
                w.parallel_bounce.as_deref()
            } else {
                w.perpendicular_bounce.as_deref()
            };
            table.and_then(|t| t.get(usize::from(surface)).copied())
        })
    };
    let perpendicular = coefficient(world, false).unwrap_or(0.25);
    if rolls {
        let speed = length(*delta);
        let inv = 1.0 / if -speed >= 0.0 { 1.0 } else { speed };
        let dir = [delta[0] * inv, inv * delta[1], inv * delta[2]];
        let friction = if speed <= dvar(world, "grenadeFrictionMaxThresh", 100.0) {
            dvar(world, "grenadeFrictionLow", 0.01)
        } else {
            dvar(world, "grenadeFrictionHigh", 0.4)
        };
        let keep = 1.0 - friction;
        for c in delta.iter_mut() {
            *c *= keep;
        }
        if strike.abs() < ROLL_STRIKE {
            return Some((speed, dir));
        }
        let max = dvar(world, "grenadeBounceRestitutionMax", 0.3);
        let restitution = if max - perpendicular >= 0.0 {
            max
        } else {
            perpendicular
        };
        let tilt = 1.0 - (1.0 - perpendicular) * n[2];
        let restitution = if restitution - tilt >= 0.0 {
            tilt
        } else {
            restitution
        };
        let k = -(restitution * strike);
        delta[0] += n[0] * k;
        delta[1] += n[1] * k;
        delta[2] += k * n[2];
        return None;
    }
    let mut along = coefficient(world, true).unwrap_or(0.5);
    if def.def_bool(def::KEEP_ROLLING) && !dud && length(*delta) < KEEP_ROLLING_SPEED {
        along *= KEEP_ROLLING_BOOST + 1.0;
    }
    let d = dot(*delta, n) * (perpendicular - along);
    let v = *delta;
    delta[0] = n[0] * d + v[0] * along;
    delta[1] = v[1] * along + d * n[1];
    delta[2] = v[2] * along + n[2] * d;
    None
}

#[allow(clippy::too_many_arguments)]
fn roll(
    missile: &mut GameMissile,
    state: &mut State,
    def: &WeaponDefView<'_>,
    n: [f32; 3],
    delta: &mut [f32; 3],
    speed: f32,
    dir: [f32; 3],
    world: &mut dyn FireWorld,
) {
    if random(world) < dvar(world, "grenadeBumpFreq", 0.3) {
        let mut bump = dvar(world, "grenadeBumpMag", 0.4) * speed;
        let max = dvar(world, "grenadeBumpMax", 100.0);
        if bump - max >= 0.0 {
            bump = max;
        }
        if gravity(world) * 0.05 < bump {
            delta[0] += n[0] * bump;
            delta[1] += bump * n[1];
            delta[2] += bump * n[2];
        }
    }
    if dir[2] < ROLL_STRIKE && n[2] > REST_SLOPE {
        let old = state.wobble;
        state.wobble = dvar(world, "grenadeWobbleFreq", 0.08) * speed * 0.314_159 + old;
        while TWO_PI < state.wobble {
            state.wobble -= TWO_PI;
        }
        let wobble = (f64::from(state.wobble).sin() - f64::from(old).sin()) as f32;
        if speed > 0.01 {
            let scale = (dvar(world, "grenadeWobbleFwdMag", 10.0) * wobble + speed) / speed;
            for c in delta.iter_mut() {
                *c *= scale;
            }
        }
        let side = [
            dir[1] * n[2] - n[1] * dir[2],
            n[0] * dir[2] - dir[0] * n[2],
            dir[0] * n[1] - n[0] * dir[1],
        ];
        let damp = 1.0 - dvar(world, "grenadeWobbleSideDamp", 0.01) * speed;
        if -damp < 0.0 && damp > 0.0 {
            let k = dvar(world, "grenadeWobbleSideMag", 1.0) * damp * wobble;
            let o = missile.origin;
            let target = [side[0] * k + o[0], side[1] * k + o[1], side[2] * k + o[2]];
            let t = missile_trace(o, target, missile.owner, clipmask(def), world);
            missile.origin = lerp(o, target, t.fraction);
        }
        if state.curve == 0.0 {
            state.curve = random(world) * 2.0 - 1.0;
        }
        let k = dvar(world, "grenadeCurveMax", 4.0) * state.curve;
        delta[0] += k * side[0];
        delta[1] += side[1] * k;
        delta[2] += side[2] * k;
    }
    let circumference = ROLL_RADIUS * TWO_PI;
    missile.angles.base[1] = vectoangles(*delta)[1];
    missile.angles.base[2] = 90.0;
    missile.angles.delta = [length(*delta) / circumference * 360.0, 0.0, 0.0];
}

/// Landing angles for `stickiness` 2, 3 and 4.
fn stick_angles(
    missile: &GameMissile,
    trace: &BulletTrace,
    slope_pitch: bool,
    world: &dyn FireWorld,
) -> [f32; 3] {
    let time = impact_time(trace, world);
    let mut angles = evaluate(&missile.angles, time);
    let n = trace.normal;
    if slope_pitch && n[2] > 0.1 {
        let (sin, cos) = f64::from(angles[1] * DEG2RAD).sin_cos();
        let slope = (f64::from(n[0]) * cos + f64::from(n[1]) * sin) / f64::from(n[2]);
        angles[0] = (slope.atan() as f32) * 57.295_78;
        angles[2] = 0.0;
    }
    let (_, _, up) = angle_vectors(angles);
    let cos_roll = f64::from(dot(n, up)).clamp(-1.0, 1.0);
    angles[2] = (cos_roll.acos() as f32) * 180.0 * 0.318_309_87;
    angles
}

/// The angles a grenade lands at on a surface.
fn land_angles(
    missile: &mut GameMissile,
    def: &WeaponDefView<'_>,
    state: &State,
    trace: &BulletTrace,
    rest: bool,
    by_type: bool,
    world: &mut dyn FireWorld,
) -> [f32; 3] {
    let time = impact_time(trace, world);
    let mut angles = evaluate(&missile.angles, time);
    if rolling_enabled(world) && def.def_bool(def::KEEP_ROLLING) && !state.dud {
        return angles;
    }
    let n = trace.normal;
    if n[2] > 0.1 {
        let (sin, cos) = f64::from(angles[1] * DEG2RAD).sin_cos();
        let slope = (f64::from(n[0]) * cos + f64::from(n[1]) * sin) / f64::from(n[2]);
        let slope = (slope.atan() as f32) * 57.295_78;
        let tilt = normalize_turns(slope - angles[0]) * 360.0;
        let off = tilt.abs();
        if !rest || by_type {
            missile.angles.base = angles;
            missile.angles.time = time;
            if off >= 80.0 {
                if !by_type {
                    let r = random(world);
                    missile.angles.delta[0] *= r * 0.3 + 0.85;
                }
            } else if !by_type {
                let r = random(world);
                missile.angles.delta[0] *= -(r * 0.3 + 0.85);
            } else {
                missile.angles.delta[0] *= -1.0;
            }
        }
        let pitch = normalize_turns(angles[0]) * 360.0;
        angles[0] = pitch;
        if !rest && off >= 45.0 {
            angles[0] = if off < 80.0 {
                normalize_360(tilt * 0.25 + pitch)
            } else {
                normalize_360(pitch)
            };
            return angles;
        }
        let slope = if pitch.abs() > 90.0 {
            slope + 180.0
        } else {
            slope
        };
        angles[0] = normalize_360(slope);
        if !rest {
            return angles;
        }
    } else if !rest {
        if !by_type {
            let r = world.rand();
            missile.angles.delta[0] =
                normalize_360(((r & 0x7f) - 0x3f) as f32 + missile.angles.delta[0]);
        }
        return angles;
    }
    let rotate_type = def.def_i32(def::ROTATE_TYPE);
    if rotate_type == 2 || rest && !by_type && rotate_type == 1 {
        angles[2] = 90.0;
    }
    angles
}

/// `G_ExplodeMissile`.
pub(crate) fn explode(
    missile: &mut GameMissile,
    def: &WeaponDefView<'_>,
    state: &mut State,
    world: &mut dyn FireWorld,
) -> MissileRun {
    let level = world.level_time();
    if def.def_i32(def::IS_VALUABLE) != 0 && def.def_i32(def::IS_ROLLING_GRENADE) != 0 {
        state.explode_at = 0;
        return MissileRun::Flying;
    }
    if (def.def_bool(def::EXPLODE_ON_GROUND) && state.ground == ENTITY_NONE) || state.dud {
        if level - state.spawned_at <= GROUND_WAIT_MS {
            state.explode_at = level + GROUND_RETRY;
            return MissileRun::Flying;
        }
        return MissileRun::Gone;
    }
    let at = evaluate(&missile.position, level);
    let at = [
        at[0] as i32 as f32,
        at[1] as i32 as f32,
        at[2] as i32 as f32,
    ];
    if !in_world(at, world.map_center()) {
        return MissileRun::Gone;
    }
    missile.origin = at;
    missile.position = GameTrajectory {
        kind: TrajectoryKind::Stationary,
        time: 0,
        duration: 0,
        base: at,
        delta: [0.0; 3],
    };
    let in_water = world.point_contents(at, CONTENTS_WATER) != 0;
    // Water when in water, mud (14) if the last surface was water but the
    // explosion is out of water, else the last surface struck.
    let surf_idx = if in_water {
        20
    } else if state.surface == 20 {
        14
    } else {
        state.surface
    };
    let surf_name = SURFACES
        .get(usize::from(surf_idx))
        .copied()
        .unwrap_or(SURFACES[0]);
    world.set_entity_pose(missile.entity, at, evaluate(&missile.angles, level));
    world.notify(
        missile.entity,
        "explode",
        &[NotifyArg::Vector(at), NotifyArg::String(surf_name)],
    );
    let mask = clipmask(def);
    let (from, to) = if matches!(def.def_i32(def::STICKINESS), 1 | 2) && state.ground != ENTITY_NONE
    {
        let r = state.rest_normal;
        (mad(at, r, 4.0), mad(at, r, -EXPLOSION_FLOOR))
    } else {
        (at, [at[0], at[1], at[2] - EXPLOSION_FLOOR])
    };
    let floor = world.bullet_trace(from, to, missile.entity, mask);
    let normal = if def.def_bool(def::PROJ_EXPLOSION_EFFECT_FORCE_NORMAL_UP) {
        [0.0, 0.0, 1.0]
    } else if floor.fraction >= 1.0 {
        state.rest_normal
    } else {
        floor.normal
    };
    world.explosion(&MissileExplosion {
        missile: missile.entity,
        owner: missile.owner,
        weapon: missile.weapon,
        origin: at,
        normal,
        surface_type: surface_type(&floor) as u8,
    });
    if def.def_i32(def::EXPLOSION_INNER_DAMAGE) != 0 {
        let cone_angle = def.def_f32(def::DAMAGE_CONE_ANGLE);
        let cone = (cone_angle - 180.0 < 0.0).then(|| f64::from(cone_angle * DEG2RAD).cos() as f32);
        let (forward, _, _) = angle_vectors(evaluate(&missile.angles, level));
        let means = splash_means(def, state.grenade);
        radius_damage(
            &RadiusDamage {
                origin: at,
                inflictor: missile.entity,
                attacker: missile.owner,
                inner: def.def_i32(def::EXPLOSION_INNER_DAMAGE) as f32,
                outer: def.def_i32(def::EXPLOSION_OUTER_DAMAGE) as f32,
                radius: def.def_i32(def::EXPLOSION_RADIUS) as f32,
                cone: cone.map(|cos| (cos, forward)),
                ignore: missile.entity,
                means: MEANS[means],
                weapon: missile.weapon,
                script: false,
            },
            world,
        );
    }
    MissileRun::Gone
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[2] * v[2] + v[1] * v[1] + v[0] * v[0]).sqrt();
    let len = if -len >= 0.0 { 1.0 } else { len };
    let inv = 1.0 / len;
    [v[0] * inv, v[1] * inv, v[2] * inv]
}

fn actor_bounds(origin: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    const MINS: [f32; 3] = [-15.0, -15.0, 0.0];
    const MAXS: [f32; 3] = [15.0, 15.0, 72.0];
    (
        core::array::from_fn(|i| origin[i] + MINS[i] - 1.0),
        core::array::from_fn(|i| origin[i] + MAXS[i] + 1.0),
    )
}

fn linked(mut entity: FireEntity) -> FireEntity {
    if entity.kind == FireEntityKind::Actor {
        entity.bounds = Some(actor_bounds(entity.origin));
    }
    entity
}

fn distance_sq(entity: &FireEntity, origin: [f32; 3]) -> f32 {
    let d = match entity.bounds {
        None => sub(entity.origin, origin),
        Some((mins, maxs)) => core::array::from_fn(|i| {
            if mins[i] > origin[i] {
                mins[i] - origin[i]
            } else if origin[i] > maxs[i] {
                origin[i] - maxs[i]
            } else {
                0.0
            }
        }),
    };
    d[0] * d[0] + d[2] * d[2] + d[1] * d[1]
}

fn sight_points(entity: &FireEntity, origin: [f32; 3], eye: [f32; 3]) -> [[f32; 3]; 5] {
    let at = entity.origin;
    if sentient(*entity) {
        let dx = origin[0] - at[0];
        let dy = origin[1] - at[1];
        let len = (dx * dx + dy * dy).sqrt();
        let len = if -len >= 0.0 { 1.0 } else { len };
        let inv = 1.0 / len;
        let nx = inv * dx;
        let px = -(inv * dy);
        let half = (eye[2] - at[2]) * 0.5;
        let base = [
            (eye[0] + at[0]) * 0.5,
            (at[1] + eye[1]) * 0.5,
            (at[2] + eye[2]) * 0.5,
        ];
        let point = |side: f32, up: f32| [px * side + base[0], nx * side + base[1], base[2] + up];
        return [
            base,
            point(PLAYER_SIDE, half),
            point(PLAYER_SIDE, -half),
            point(-PLAYER_SIDE, half),
            point(-PLAYER_SIDE, -half),
        ];
    }
    [at; 5]
}

fn visibility(
    entity: &FireEntity,
    target: i32,
    blast: &RadiusDamage,
    eye: [f32; 3],
    world: &mut dyn FireWorld,
) -> f32 {
    let d = sub(blast.origin, entity.origin);
    if entity.contents & BOUNDED_CONTENTS == 0
        && blast.radius * blast.radius <= d[0] * d[0] + d[2] * d[2] + d[1] * d[1]
    {
        return 0.0;
    }
    let mut seen = 0usize;
    for point in sight_points(entity, blast.origin, eye) {
        if let Some((cos, dir)) = blast.cone {
            let v = normalize(sub(point, blast.origin));
            if cos > dir[2] * v[2] + dir[1] * v[1] + dir[0] * v[0] {
                continue;
            }
        }
        if world.sight_clear(blast.origin, point, [target, blast.inflictor], SIGHT_MASK) {
            if entity.kind != FireEntityKind::Player {
                return 1.0;
            }
            seen += 1;
        }
    }
    match seen {
        0 => 0.0,
        1..=3 => seen as f32 * 0.333_333_34,
        _ => 1.0,
    }
}

/// `G_RadiusDamage`.
pub(crate) fn radius_damage(blast: &RadiusDamage, world: &mut dyn FireWorld) {
    let radius = blast.radius.max(1.0);
    let reach = radius * 1.414_213_5;
    let o = blast.origin;
    let mins = [o[0] - reach, o[1] - reach, o[2] - reach];
    let maxs = [o[0] + reach, o[1] + reach, o[2] + reach];
    let blast = RadiusDamage { radius, ..*blast };
    let skip_players = world.players_ignore_radius_damage();
    for target in world.entities_in_box(mins, maxs) {
        if target == blast.ignore {
            continue;
        }
        let Some(entity) = world.entity(target).map(linked) else {
            continue;
        };
        if !entity.takes_damage || (entity.kind == FireEntityKind::Player && skip_players) {
            continue;
        }
        let d2 = distance_sq(&entity, blast.origin);
        if d2 >= radius * radius {
            continue;
        }
        let eye = match entity.kind {
            FireEntityKind::Player => world.player(target).map_or(entity.origin, |p| {
                [p.origin[0], p.origin[1], p.origin[2] + p.view_height]
            }),
            FireEntityKind::Actor => {
                entity
                    .eye
                    .unwrap_or([entity.origin[0], entity.origin[1], entity.origin[2] + 64.0])
            }
            FireEntityKind::Other => entity.origin,
        };
        let seen = visibility(&entity, target, &blast, eye, world);
        if seen <= 0.0 {
            continue;
        }
        let dist = d2.sqrt();
        let amount =
            (((1.0 - dist / radius) * (blast.inner - blast.outer) + blast.outer) * seen) as i32;
        if amount <= 0 {
            continue;
        }
        let dir = [
            entity.origin[0] - blast.origin[0],
            entity.origin[1] - blast.origin[1],
            entity.origin[2] - blast.origin[2] + AIM_HEIGHT,
        ];
        world.damage(&FireDamage {
            target,
            inflictor: blast.inflictor,
            attacker: blast.attacker,
            dir,
            point: blast.origin,
            amount,
            flags: BLAST_FLAGS,
            means: blast.means,
            weapon: blast.weapon,
            hit_location: HIT_LOCATIONS[0],
            location: 0,
            bone: None,
            time_offset: 0,
        });
    }
}
