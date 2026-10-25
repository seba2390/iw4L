//! Black Ops' grenades: thrown or fired along the throw with the thrower's
//! own speed added, falling under gravity, bouncing off each surface by its
//! own coefficients (rolling, bumping and wobbling when they roll), coming
//! to rest, and exploding when their fuse runs out.

use fastfile_t5::weapon_def::WeaponDefView;
use game_api::fire::{
    BulletTrace, FireEntityKind, FireWorld, GAME_MISSILE_BYTES, GameMissile, GameTrajectory,
    MissileExplosion, MissileRun, NotifyArg, RadiusDamage, TrajectoryKind,
};
use game_api::movement::{ENTITY_NONE, ENTITY_WORLD};
use game_api::{Unknown, unknown};

use crate::fire::{DEG2RAD, MEANS, hit_id, sentient};
use crate::missile::{
    LinkTag, State, clipmask, evaluate, evaluate_delta, mad, missile_trace, surface_type,
    vectoangles,
};

const MOD_GRENADE_SPLASH: usize = 4;
/// `stickiness`: stick to the ground, and to the ground keeping the yaw.
const STICK_GROUND: i32 = 3;
const STICK_GROUND_KEEP_YAW: i32 = 4;
/// Radians to degrees, as Black Ops' landing angles scale them.
const RAD2DEG: f32 = 57.2957764;
/// A grenade rests on what faces up more than this.
const REST_SLOPE: f32 = 0.699999988;
/// The fuse a grenade without one is given, and the longest any has.
const DEFAULT_FUSE_MS: i32 = 30_000;
const MAX_FUSE_MS: i32 = 60_000;
/// A grenade waiting for the ground to explode on gives up after this long,
/// trying again this soon.
const GROUND_WAIT_MS: i32 = 60_000;
const GROUND_RETRY: i32 = 50;
/// A keepRolling grenade slower than this keeps more of its speed along a
/// surface.
const KEEP_ROLLING_SPEED: f32 = 30.0;
const KEEP_ROLLING_BOOST: f32 = 0.5;
/// A rolling grenade's radius: it turns over once per circumference.
const ROLL_RADIUS: f32 = 5.0;
const TWO_PI: f32 = 6.28318024;
/// A strike shallower than this lets a rolling grenade roll on; a bounce
/// faster off the surface than this turns its spin over.
const ROLL_STRIKE: f32 = 25.0;
/// How much a bounce must change its velocity to be heard.
const BOUNCE_HEARD: f32 = 100.0;
const CONTENTS_WATER: u32 = 0x20;
const SURF_FLESH: u8 = 7;
/// How far below a grenade its explosion looks for the surface it lies on.
const EXPLOSION_FLOOR: f32 = 16.0;
/// Surface names, by surface type, as scripts get them.
pub(crate) const SURFACE_NAMES: [&str; 31] = [
    "default",
    "bark",
    "brick",
    "carpet",
    "cloth",
    "concrete",
    "dirt",
    "flesh",
    "foliage",
    "glass",
    "grass",
    "gravel",
    "ice",
    "metal",
    "mud",
    "paper",
    "plaster",
    "rock",
    "sand",
    "snow",
    "water",
    "wood",
    "asphalt",
    "ceramic",
    "plastic",
    "rubber",
    "cushion",
    "fruit",
    "paintedmetal",
    "player",
    "tallgrass",
];

fn grenade_unknown() -> &'static Unknown {
    unknown!(
        "t5.weapons.grenade",
        "a thrown grenade that sticks to entities, a smoke, flash, gas or molotov grenade, a grenade landing on another missile or a mover, or one exploding in water, does not do what Black Ops does with it",
        "Black Ops' thrown grenades sticking to entities, explosion tags, smoke, flash, gas and molotov effects, grenades riding movers and water explosions"
    )
}

fn dvar(world: &dyn FireWorld, name: &str, default: f32) -> f32 {
    world
        .dvar(name)
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

fn rolling_enabled(world: &dyn FireWorld) -> bool {
    dvar(world, "grenadeRollingEnabled", 1.0) != 0.0
}

/// The server's random draws: a fraction in [0, 1), an integer in
/// `min..max`, a float in `min..max`.
fn random(world: &mut dyn FireWorld) -> f32 {
    (f64::from(world.rand()) * f64::from(3.05175781e-05f32)) as f32
}

fn irand(world: &mut dyn FireWorld, min: i32, max: i32) -> i32 {
    ((i64::from(world.rand()) * i64::from(max - min)) >> 15) as i32 + min
}

fn flrand(world: &mut dyn FireWorld, min: f32, max: f32) -> f32 {
    (f64::from(world.rand()) * (f64::from(max) - f64::from(min)) * f64::from(3.05175781e-05f32)
        + f64::from(min)) as f32
}

/// `angle` in turns with the nearest whole turn taken off.
fn normalize_turns(angle: f32) -> f32 {
    let turns = angle * 0.00277777785;
    turns - (turns + 0.5).floor()
}

/// `angle` in [0, 360).
fn normalize_360(angle: f32) -> f32 {
    let turns = angle * 0.00277777785;
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

/// The gravity a player's grenade falls by.
fn gravity(world: &dyn FireWorld) -> f32 {
    dvar(world, "bg_gravity", 800.0)
}

/// A grenade thrown or fired from `muzzle`: the weapon's speeds along the
/// aim, up the aim, straight up and along the ground, and the thrower's own
/// speed along the throw.
#[allow(clippy::too_many_arguments)]
pub(crate) fn fire_grenade(
    owner: i32,
    weapon: u32,
    def: &WeaponDefView<'_>,
    forward: [f32; 3],
    up: [f32; 3],
    muzzle: [f32; 3],
    world: &mut dyn FireWorld,
    gaps: &mut Vec<&'static Unknown>,
) {
    let speed = def.projectile_speed() as f32;
    let relative_up = def.projectile_speed_relative_up() as f32;
    let mut vel = [
        up[0] * relative_up + forward[0] * speed,
        up[1] * relative_up + forward[1] * speed,
        def.projectile_speed_up() as f32 + up[2] * relative_up + forward[2] * speed,
    ];
    if def.projectile_speed_forward() != 0 {
        // The game's fast inverse square root of the aim's flat length.
        let flat = forward[1] * forward[1] + forward[0] * forward[0];
        let guess = f32::from_bits(0x5f37_59df_u32.wrapping_sub(flat.to_bits() >> 1));
        let inv = (1.5 - flat * 0.5 * guess * guess) * guess;
        let push = def.projectile_speed_forward() as f32;
        vel[0] += push * forward[0] * inv;
        vel[1] += forward[1] * inv * push;
    }
    let player = world.player(owner);
    let fuse = if player.is_some() {
        def.fuse_time()
    } else {
        def.ai_fuse_time()
    };
    // The thrower's speed along the throw, on top of the whole units the
    // grenade leaves with.
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
    spawn_grenade(
        owner, weapon, def, muzzle, vel, carried, true, false, None, None, fuse, world, gaps,
    );
}

/// A grenade launcher's shot by a player: the grenade leaves along the aim
/// at the weapon's projectile speed, its upward speed added, without spin,
/// on the weapon's fuse, carrying the shooter's speed along its flight; it
/// never destabilises. An actor's shot (aimed to land by its enemy) is not
/// run.
pub(crate) fn launch_grenade(
    owner: i32,
    weapon: u32,
    def: &WeaponDefView<'_>,
    forward: [f32; 3],
    muzzle: [f32; 3],
    world: &mut dyn FireWorld,
    gaps: &mut Vec<&'static Unknown>,
) {
    let Some(player) = world.player(owner) else {
        gaps.push(crate::fire::fire_unknown());
        return;
    };
    let speed = def.projectile_speed() as f32;
    let vel = [
        forward[0] * speed,
        forward[1] * speed,
        forward[2] * speed + def.projectile_speed_up() as f32,
    ];
    let len = ((vel[1] * vel[1] + vel[0] * vel[0]) + vel[2] * vel[2]).sqrt();
    let len = if -len >= 0.0 { 1.0 } else { len };
    let inv = 1.0 / len;
    let dir = [inv * vel[0], vel[1] * inv, inv * vel[2]];
    let pv = player.velocity;
    let along = (pv[1] * dir[1] + pv[2] * dir[2]) + pv[0] * dir[0];
    let carried = [dir[0] * along, dir[1] * along, dir[2] * along];
    spawn_grenade(
        owner,
        weapon,
        def,
        muzzle,
        vel,
        carried,
        false,
        true,
        None,
        None,
        def.fuse_time(),
        world,
        gaps,
    );
}

/// What a grenade left by a bolt sticks in: the entity struck (the world
/// for none) and the surface's normal.
#[derive(Clone, Copy)]
pub(crate) struct Stuck {
    pub(crate) on: i32,
    pub(crate) normal: [f32; 3],
    /// The bone struck on the entity's model.
    pub(crate) bone: Option<u16>,
}

/// A bolt's grenade (its weapon's `grenadeWeapon`) left where the bolt
/// struck: stuck there, its fuse the grenade weapon's, linked to the entity
/// struck (a player's or actor's bone struck, an actor's right knee when
/// that bone cannot be linked, else the entity itself).
pub(crate) fn leave_grenade(
    owner: i32,
    weapon: u32,
    start: [f32; 3],
    dir: [f32; 3],
    stuck: Stuck,
    world: &mut dyn FireWorld,
    gaps: &mut Vec<&'static Unknown>,
) {
    let Some((def_bytes, variant_bytes)) = world
        .weapon(weapon)
        .map(|w| (w.def.clone(), w.variant.clone()))
    else {
        gaps.push(crate::movement::weapon_unknown());
        return;
    };
    let Some(def) = WeaponDefView::new(&def_bytes, &variant_bytes) else {
        gaps.push(crate::movement::weapon_unknown());
        return;
    };
    let link = (stuck.on != ENTITY_WORLD)
        .then(|| link_to(world, stuck, start))
        .flatten();
    let fuse = def.fuse_time();
    spawn_grenade(
        owner,
        weapon,
        &def,
        start,
        dir,
        [0.0; 3],
        false,
        false,
        Some(stuck),
        link,
        fuse,
        world,
        gaps,
    );
}

/// What a grenade stuck at `point` links to on the entity struck, and where
/// it sits in that tag's frame.
fn link_to(
    world: &mut dyn FireWorld,
    stuck: Stuck,
    point: [f32; 3],
) -> Option<(LinkTag, [f32; 3])> {
    let kind = world.entity(stuck.on)?.kind;
    let mut tag = LinkTag::Origin;
    if matches!(kind, FireEntityKind::Player | FireEntityKind::Actor) {
        if let Some(bone) = stuck.bone
            && link_pose(world, stuck.on, LinkTag::Bone(bone)).is_some()
        {
            tag = LinkTag::Bone(bone);
        } else if kind == FireEntityKind::Actor
            && link_pose(world, stuck.on, LinkTag::KneeRight).is_some()
        {
            tag = LinkTag::KneeRight;
        }
    }
    let (origin, axis) = link_pose(world, stuck.on, tag)?;
    let d = [
        point[0] - origin[0],
        point[1] - origin[1],
        point[2] - origin[2],
    ];
    let offset = axis.map(|row| row[0] * d[0] + row[1] * d[1] + row[2] * d[2]);
    Some((tag, offset))
}

/// The world position and axis (forward, left, up) of what a grenade is
/// linked to on an entity.
fn link_pose(
    world: &mut dyn FireWorld,
    entity: i32,
    tag: LinkTag,
) -> Option<([f32; 3], [[f32; 3]; 3])> {
    match tag {
        LinkTag::None => None,
        LinkTag::Origin => {
            let entity = world.entity(entity)?;
            let (forward, right, up) = movement_t5::math::angle_vectors(entity.angles);
            Some((entity.origin, [forward, right.map(|v| -v), up]))
        }
        LinkTag::KneeRight => world.tag_pose(entity, "j_knee_ri"),
        LinkTag::Bone(bone) => {
            let name = world.bone_tag(entity, bone)?;
            world.tag_pose(entity, &name)
        }
    }
}

/// A grenade linked to the entity it rests on moves with that tag; one
/// whose entity or tag is gone is let go to fall. Answers whether it rides.
pub(crate) fn follow_link(
    missile: &mut GameMissile,
    state: &mut State,
    world: &mut dyn FireWorld,
) -> bool {
    if state.link == LinkTag::None || state.ground == ENTITY_NONE {
        return false;
    }
    let Some((origin, axis)) = link_pose(world, state.ground, state.link) else {
        state.link = LinkTag::None;
        return false;
    };
    let o = state.link_offset;
    let at = std::array::from_fn(|i| {
        origin[i] + axis[0][i] * o[0] + axis[1][i] * o[1] + axis[2][i] * o[2]
    });
    missile.origin = at;
    missile.position = GameTrajectory {
        kind: TrajectoryKind::Stationary,
        time: 0,
        duration: 0,
        base: at,
        delta: [0.0; 3],
    };
    true
}

/// A grenade put in the match: falling from `start` at `vel` (in whole
/// units, then `carried` on top), spinning when its weapon rotates, its fuse
/// lit, and the thrower told.
#[allow(clippy::too_many_arguments)]
fn spawn_grenade(
    owner: i32,
    weapon: u32,
    def: &WeaponDefView<'_>,
    start: [f32; 3],
    vel: [f32; 3],
    carried: [f32; 3],
    rotate: bool,
    launched: bool,
    stuck: Option<Stuck>,
    link: Option<(LinkTag, [f32; 3])>,
    fuse: i32,
    world: &mut dyn FireWorld,
    gaps: &mut Vec<&'static Unknown>,
) -> Option<i32> {
    if stuck.is_none() && sticks_to_entities(def) {
        gaps.push(grenade_unknown());
    }
    let level = world.level_time();
    let angles = vectoangles(vel);
    let delta = [
        vel[0] as i32 as f32 + carried[0],
        vel[1] as i32 as f32 + carried[1],
        vel[2] as i32 as f32 + carried[2],
    ];
    let spin = if rotate && def.rotate() {
        let base = [normalize_360(angles[0] - 120.0), angles[1], angles[2]];
        let delta = if def.rotate_type() == 1 {
            [1500.0, 0.0, 0.0]
        } else {
            let sign = (irand(world, 0, 2) * 2 - 1) as f32;
            let pitch = flrand(world, 340.0, 800.0) * sign;
            let sign = (irand(world, 0, 2) * 2 - 1) as f32;
            let roll = flrand(world, 180.0, 540.0) * sign;
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
    let hidden = ((length(vel) * -35.0 * 0.00166666671 + 85.0) as i32).clamp(20, 50);
    let state = State {
        bounce: true,
        grenade: true,
        launched,
        ground: stuck.map_or(ENTITY_NONE, |stuck| stuck.on),
        rest_normal: stuck.map_or([0.0; 3], |stuck| stuck.normal),
        link: link.map_or(LinkTag::None, |(tag, _)| tag),
        link_offset: link.map_or([0.0; 3], |(_, offset)| offset),
        explode_at: light_fuse(owner, def, fuse, level, world),
        spawned_at: level,
        ..State::default()
    };
    let mut game = [0u8; GAME_MISSILE_BYTES];
    state.write(&mut game);
    let entity = world.spawn_missile(GameMissile {
        entity: ENTITY_NONE,
        owner,
        weapon,
        position: match stuck {
            Some(_) => GameTrajectory {
                kind: TrajectoryKind::Stationary,
                time: 0,
                duration: 0,
                base: start,
                delta: [0.0; 3],
            },
            None => GameTrajectory {
                kind: TrajectoryKind::Gravity(gravity(world)),
                time: level,
                duration: 0,
                base: start,
                delta,
            },
        },
        angles: spin,
        origin: start,
        visible_at: level + hidden,
        game,
    })?;
    let notify = if def.projectile_activate_dist() == 0 {
        "grenade_fire"
    } else {
        "grenade_launcher_fire"
    };
    world.notify(
        owner,
        notify,
        &[NotifyArg::Entity(entity), NotifyArg::Weapon(weapon)],
    );
    Some(entity)
}

/// When the fuse runs out: a cooked grenade keeps what was left of its
/// fuse in the thrower's hand, which empties.
fn light_fuse(
    owner: i32,
    def: &WeaponDefView<'_>,
    fuse: i32,
    level: i32,
    world: &mut dyn FireWorld,
) -> i32 {
    let mut at = 0;
    if def.projectile_activate_dist() == 0 || fuse > 0 {
        match world.player(owner) {
            Some(mut player) => {
                let (mut ps, mut last) = crate::movement::unpack(&mut player);
                if !def.timed_detonation() {
                    ps.grenade_time_left = 0;
                } else if ps.grenade_time_left != 0 {
                    at = level + ps.grenade_time_left;
                    ps.grenade_time_left = 0;
                } else {
                    at = level + fuse;
                }
                crate::movement::pack(&mut player, &mut ps, &mut last);
                world.store_player(owner, &player);
            }
            None => at = level + fuse,
        }
    }
    if at == 0 {
        at = level + DEFAULT_FUSE_MS;
    }
    at.min(level + MAX_FUSE_MS)
}

/// A grenade resting on something that is no longer under it starts
/// falling.
pub(crate) fn start_falling(
    missile: &mut GameMissile,
    state: &mut State,
    def: &WeaponDefView<'_>,
    world: &mut dyn FireWorld,
) {
    if missile.position.kind != TrajectoryKind::Stationary || state.ground == ENTITY_WORLD {
        return;
    }
    let start = missile.origin;
    let end = mad(start, state.rest_normal, -1.63499999);
    let trace = missile_trace(def, start, end, missile.owner, clipmask(def), world);
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

/// The time within this frame the trace reached its fraction at.
fn impact_time(trace: &BulletTrace, world: &dyn FireWorld) -> i32 {
    let level = world.level_time();
    let prev = world.previous_level_time();
    ((level - prev) as f32 * trace.fraction) as i32 + prev
}

/// A grenade striking a surface without exploding: it bounces or comes to
/// rest, and scripts and clients hear of it when it came to rest or its
/// velocity changed enough.
pub(crate) fn bounce(
    missile: &mut GameMissile,
    def: &WeaponDefView<'_>,
    state: &mut State,
    trace: &BulletTrace,
    end: [f32; 3],
    world: &mut dyn FireWorld,
    gaps: &mut Vec<&'static Unknown>,
) {
    if !bounce_off(missile, def, state, trace, world, gaps) || trace.start_solid {
        return;
    }
    if matches!(def.stickiness(), 1 | 5) {
        gaps.push(grenade_unknown());
    }
    world.notify(
        missile.entity,
        "grenade_bounce",
        &[NotifyArg::Vector(end), NotifyArg::Vector(trace.normal)],
    );
    // A body struck without a surface of its own sounds as flesh.
    let struck = world.entity(hit_id(trace));
    if state.surface == 0 && struck.is_some_and(sentient) {
        state.surface = SURF_FLESH;
    }
    world.bounce(missile.entity, state.surface);
}

/// The bounce itself: the velocity at the strike reflected (or laid along
/// the surface for a rolling grenade) and scaled by the surface's
/// coefficients, bumped, wobbled and curved while rolling; at rest on a
/// slope flat enough when slow enough, else a little off the surface with
/// its spin turned. True when it came to rest or its velocity changed
/// enough to be heard.
fn bounce_off(
    missile: &mut GameMissile,
    def: &WeaponDefView<'_>,
    state: &mut State,
    trace: &BulletTrace,
    world: &mut dyn FireWorld,
    gaps: &mut Vec<&'static Unknown>,
) -> bool {
    let level = world.level_time();
    let in_water = world.point_contents(missile.origin, CONTENTS_WATER) != 0;
    let n = trace.normal;
    let vel = evaluate_delta(&missile.position, impact_time(trace, world));
    let strike = dot(n, vel);
    let rolls = rolling_enabled(world) && def.is_rolling_grenade() != 0;
    let k = if rolls { -strike } else { strike * -2.0 };
    let mut delta = [n[0] * k + vel[0], k * n[1] + vel[1], k * n[2] + vel[2]];
    let struck = hit_id(trace);
    if sticks_to_entities(def) {
        gaps.push(grenade_unknown());
    }
    if matches!(def.stickiness(), 1 | 2) || n[2] > REST_SLOPE {
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
        world,
        gaps,
    );
    if let Some((speed, dir)) = rolling {
        roll(missile, state, def, n, &mut delta, speed, dir, world);
    }
    missile.position.delta = delta;
    if matches!(def.stickiness(), 1 | 2)
        || n[2] > REST_SLOPE
            && (matches!(def.stickiness(), 3 | 4)
                || length(delta) < dvar(world, "grenadeRestThreshold", 20.0))
    {
        // At rest where it struck.
        missile.position = GameTrajectory {
            kind: TrajectoryKind::Stationary,
            time: 0,
            duration: 0,
            base: missile.origin,
            delta: [0.0; 3],
        };
        let angles = match def.stickiness() {
            STICK_GROUND | STICK_GROUND_KEEP_YAW => ground_angles(missile, def, trace, world),
            _ => land_angles(missile, def, trace, true, false, world),
        };
        missile.angles = GameTrajectory {
            kind: TrajectoryKind::Stationary,
            time: 0,
            duration: 0,
            base: angles,
            delta: [0.0; 3],
        };
        state.rest_normal = n;
        if !def.timed_detonation() {
            state.explode_at = 0;
        }
        world.notify(missile.entity, "stationary", &[]);
        return true;
    }
    // A little off the surface (never up from it), on from there.
    let o = missile.origin;
    let target = [
        n[0] * 0.100000001 + o[0],
        n[1] * 0.100000001 + o[1],
        (n[2] * 0.100000001).min(0.0) + o[2],
    ];
    let t = missile_trace(def, o, target, missile.owner, clipmask(def), world);
    missile.origin = lerp(o, target, t.fraction);
    missile.position.base = missile.origin;
    missile.position.time = level;
    if rolling.is_none() && dot(delta, n) > ROLL_STRIKE {
        let (rest, by_type) = match def.rotate_type() {
            1 => (false, true),
            2 => (true, false),
            _ => (false, false),
        };
        missile.angles.base = land_angles(missile, def, trace, rest, by_type, world);
        missile.angles.time = level;
    }
    let change = [delta[0] - vel[0], delta[1] - vel[1], delta[2] - vel[2]];
    !in_water && length(change) > BOUNCE_HEARD
}

/// The surface's own bounce: a rolling grenade's friction, rolling on when
/// it strikes shallowly or else its restitution off the surface; any other
/// grenade's coefficients along and across the surface. The speed and
/// direction it rolls on at, when it rolls.
#[allow(clippy::too_many_arguments)]
fn surface_bounce(
    delta: &mut [f32; 3],
    strike: f32,
    surface: u8,
    weapon: u32,
    def: &WeaponDefView<'_>,
    n: [f32; 3],
    rolls: bool,
    world: &mut dyn FireWorld,
    gaps: &mut Vec<&'static Unknown>,
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
    let Some(perpendicular) = coefficient(world, false) else {
        gaps.push(crate::movement::weapon_unknown());
        return None;
    };
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
    let Some(mut along) = coefficient(world, true) else {
        gaps.push(crate::movement::weapon_unknown());
        return None;
    };
    if def.keep_rolling() && length(*delta) < KEEP_ROLLING_SPEED {
        along *= KEEP_ROLLING_BOOST + 1.0;
    }
    let d = dot(*delta, n) * (perpendicular - along);
    let v = *delta;
    delta[0] = n[0] * d + v[0] * along;
    delta[1] = v[1] * along + d * n[1];
    delta[2] = v[2] * along + n[2] * d;
    None
}

/// A rolling grenade's bump off the surface, wobble and curve along it, and
/// its turning over as it rolls.
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
        if gravity(world) * 0.0500000007 < bump {
            delta[0] += n[0] * bump;
            delta[1] += bump * n[1];
            delta[2] += bump * n[2];
        }
    }
    if dir[2] < ROLL_STRIKE && n[2] > REST_SLOPE {
        let old = state.wobble;
        state.wobble = dvar(world, "grenadeWobbleFreq", 0.08) * speed * 0.314159006 + old;
        while TWO_PI < state.wobble {
            state.wobble -= TWO_PI;
        }
        let wobble = (f64::from(state.wobble).sin() - f64::from(old).sin()) as f32;
        if speed > 0.00999999978 {
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
            let t = missile_trace(def, o, target, missile.owner, clipmask(def), world);
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
    // Turning over as it rolls: once per circumference, on its side.
    let circumference = ROLL_RADIUS * TWO_PI;
    missile.angles.base[1] = vectoangles(*delta)[1];
    missile.angles.base[2] = 90.0;
    missile.angles.delta = [length(*delta) / circumference * 360.0, 0.0, 0.0];
}

/// The angles a grenade lands at on a surface: its pitch laid along the
/// slope it faces (or turned toward it on a hard strike), its spin slowed
/// and maybe reversed, its roll by its rotation type.
fn land_angles(
    missile: &mut GameMissile,
    def: &WeaponDefView<'_>,
    trace: &BulletTrace,
    rest: bool,
    by_type: bool,
    world: &mut dyn FireWorld,
) -> [f32; 3] {
    let time = impact_time(trace, world);
    let mut angles = evaluate(&missile.angles, time);
    if rolling_enabled(world) && def.keep_rolling() {
        return angles;
    }
    let n = trace.normal;
    if n[2] > 0.100000001 {
        let (sin, cos) = f64::from(angles[1] * DEG2RAD).sin_cos();
        let slope = (f64::from(n[0]) * cos + f64::from(n[1]) * sin) / f64::from(n[2]);
        let slope = (slope.atan() as f32) * 57.2957802;
        let tilt = normalize_turns(slope - angles[0]) * 360.0;
        let off = tilt.abs();
        if !rest || by_type {
            missile.angles.base = angles;
            missile.angles.time = time;
            if off >= 80.0 {
                if !by_type {
                    let r = random(world);
                    missile.angles.delta[0] *= r * 0.300000012 + 0.850000024;
                }
            } else if !by_type {
                let r = random(world);
                missile.angles.delta[0] *= -(r * 0.300000012 + 0.850000024);
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
    if def.rotate_type() == 2 || rest && !by_type && def.rotate_type() == 1 {
        angles[2] = 90.0;
    }
    angles
}

/// A grenade's fuse running out: it explodes where its trajectory has it,
/// tells scripts where and on what it last struck, shows its explosion on
/// the surface below and does its blast.
pub(crate) fn explode(
    missile: &mut GameMissile,
    def: &WeaponDefView<'_>,
    state: &mut State,
    world: &mut dyn FireWorld,
    gaps: &mut Vec<&'static Unknown>,
) -> MissileRun {
    let level = world.level_time();
    if def.explode_on_ground() && state.ground == ENTITY_NONE {
        if level - state.spawned_at <= GROUND_WAIT_MS {
            state.explode_at = GROUND_RETRY;
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
    if !crate::missile::in_world(at, world.map_center()) {
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
    if world.point_contents(at, CONTENTS_WATER) != 0 || !matches!(def.proj_explosion_type(), 0 | 6)
    {
        gaps.push(grenade_unknown());
    }
    let name = SURFACE_NAMES
        .get(usize::from(state.surface))
        .copied()
        .unwrap_or(SURFACE_NAMES[0]);
    world.notify(
        missile.entity,
        "explode",
        &[NotifyArg::Vector(at), NotifyArg::String(name)],
    );
    // The surface it lies on: across the surface it stuck to, else
    // straight down.
    let mask = clipmask(def);
    let (from, to) = if matches!(def.stickiness(), 1 | 2) && state.ground != ENTITY_NONE {
        let r = state.rest_normal;
        (mad(at, r, 4.0), mad(at, r, -EXPLOSION_FLOOR))
    } else {
        (at, [at[0], at[1], at[2] - EXPLOSION_FLOOR])
    };
    let floor = world.bullet_trace(from, to, missile.entity, mask);
    let normal = if def.proj_explosion_effect_force_normal_up() {
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
    if def.explosion_inner_damage() != 0 {
        let cone = (def.damage_cone_angle() - 180.0 < 0.0)
            .then(|| f64::from(def.damage_cone_angle() * DEG2RAD).cos() as f32);
        let (forward, _, _) = movement_t5::math::angle_vectors(evaluate(&missile.angles, level));
        gaps.extend(crate::radius::radius_damage(
            &RadiusDamage {
                origin: at,
                inflictor: missile.entity,
                attacker: missile.owner,
                inner: def.explosion_inner_damage() as f32,
                outer: def.explosion_outer_damage() as f32,
                radius: def.explosion_radius() as f32,
                cone: cone.map(|cos| (cos, forward)),
                ignore: missile.entity,
                means: MEANS[MOD_GRENADE_SPLASH],
                weapon: missile.weapon,
                script: false,
            },
            world,
        ));
    }
    MissileRun::Gone
}

/// The angles a grenade that sticks to the ground rests at: keeping its
/// yaw it pitches along the slope under it, or (sticking to the ground
/// alone) it lays its forward along the surface; either way it rolls to lie
/// on the surface.
fn ground_angles(
    missile: &GameMissile,
    def: &WeaponDefView<'_>,
    trace: &BulletTrace,
    world: &mut dyn FireWorld,
) -> [f32; 3] {
    let mut angles = evaluate(&missile.angles, impact_time(trace, world));
    let n = trace.normal;
    if def.stickiness() == STICK_GROUND_KEEP_YAW {
        angles[0] = slope_pitch(angles[1], n);
        angles[2] = 0.0;
    } else {
        let (f, _, _) = movement_t5::math::angle_vectors(angles);
        let d = -((n[0] * f[0] + n[2] * f[2]) + n[1] * f[1]);
        angles = vectoangles([n[0] * d + f[0], n[1] * d + f[1], n[2] * d + f[2]]);
    }
    let (_, right, up) = movement_t5::math::angle_vectors(angles);
    let along_right = (n[0] * right[0] + n[1] * right[1]) + n[2] * right[2];
    let along_up = (n[0] * up[0] + n[1] * up[1]) + n[2] * up[2];
    angles[2] = f64::from(along_right).atan2(f64::from(along_up)) as f32 * RAD2DEG;
    angles
}

/// The pitch of a surface along a yaw; straight down off a wall.
fn slope_pitch(yaw: f32, n: [f32; 3]) -> f32 {
    let (sin, cos) = f64::from(yaw * DEG2RAD).sin_cos();
    if n[2] == 0.0 {
        return 270.0;
    }
    let along = (f64::from(n[1]) * sin + f64::from(n[0]) * cos) / f64::from(n[2]);
    (along.atan() * f64::from(RAD2DEG)) as f32
}

/// The grenade sticks to what it strikes, entities among them (stickiness
/// 1, 2 and 5), which the runtime does not run.
fn sticks_to_entities(def: &WeaponDefView<'_>) -> bool {
    matches!(def.stickiness(), 1 | 2 | 5)
}

/// A grenade let go where its thrower stands (the thrower held it too
/// long): 40 units up from its origin, still, on the fuse it has left.
pub(crate) fn drop_grenade(
    owner: i32,
    weapon: u32,
    def: &WeaponDefView<'_>,
    origin: [f32; 3],
    fuse: i32,
    world: &mut dyn FireWorld,
    gaps: &mut Vec<&'static Unknown>,
) {
    let start = [origin[0], origin[1], origin[2] + 40.0];
    spawn_grenade(
        owner, weapon, def, start, [0.0; 3], [0.0; 3], false, false, None, None, fuse, world, gaps,
    );
}
