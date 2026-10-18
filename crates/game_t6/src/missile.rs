//! Black Ops 2's missiles: thrown and launched grenades, mines and rockets,
//! moved every frame along their trajectories, traced against what they may
//! hit, bounced or stuck by `G_BounceGrenade` and `G_MissileImpact`, and
//! detonated by `G_ExplodeMissile`.

use fastfile_t6::weapon::{WeaponDefView, def, weap_type};
use game_api::Unknown;
use game_api::fire::{
    BulletHitType, BulletTrace, FireDamage, FireEntity, FireEntityKind, FireWorld,
    GAME_MISSILE_BYTES, GameMissile, GameTrajectory, MissileExplosion, MissilePhysics, MissileRun,
    NotifyArg, RadiusDamage, TrajectoryKind,
};
use game_api::movement::{ENTITY_NONE, ENTITY_WORLD};

use crate::grenade;

/// What a missile stops at; plantable weapons add their own contents.
const MISSILE_MASK: u32 = 0x0280_e893;
const PLANTABLE_MASK: u32 = 0x0281_e893;
const PLANTABLE_UP_MASK: u32 = 0x0280_ec93;
const STICKINESS_FLOOR_CLEAR: u32 = 0x0200_0000;
const CONTENTS_WATER: u32 = 0x20;
const CONTENTS_MISSILE_PASS: u32 = 0x800;
const SURF_SKY: u32 = 0x4;
const SURF_FLESH: u8 = 7;
const SURF_RIOTSHIELD: u8 = 31;
const HITLOC_RIOTSHIELD: i32 = 20;

pub(crate) const DEG2RAD: f32 = 0.017_453_292;

/// The game's means of death, by number.
pub(crate) const MEANS: [&str; 21] = [
    "MOD_UNKNOWN",
    "MOD_PISTOL_BULLET",
    "MOD_RIFLE_BULLET",
    "MOD_GRENADE",
    "MOD_GRENADE_SPLASH",
    "MOD_PROJECTILE",
    "MOD_PROJECTILE_SPLASH",
    "MOD_MELEE",
    "MOD_BAYONET",
    "MOD_HEAD_SHOT",
    "MOD_CRUSH",
    "MOD_TELEFRAG",
    "MOD_FALLING",
    "MOD_SUICIDE",
    "MOD_TRIGGER_HURT",
    "MOD_EXPLOSIVE",
    "MOD_IMPACT",
    "MOD_BURNED",
    "MOD_HIT_BY_OBJECT",
    "MOD_DROWN",
    "MOD_GAS",
];
pub(crate) const MOD_GRENADE: usize = 3;
pub(crate) const MOD_GRENADE_SPLASH: usize = 4;
pub(crate) const MOD_PROJECTILE: usize = 5;
pub(crate) const MOD_PROJECTILE_SPLASH: usize = 6;
pub(crate) const MOD_EXPLOSIVE: usize = 15;
pub(crate) const MOD_IMPACT: usize = 16;

/// Hit locations, by number.
pub(crate) const HIT_LOCATIONS: [&str; 21] = [
    "none",
    "helmet",
    "head",
    "neck",
    "torso_upper",
    "torso_mid",
    "torso_lower",
    "right_arm_upper",
    "left_arm_upper",
    "right_arm_lower",
    "left_arm_lower",
    "right_hand",
    "left_hand",
    "right_leg_upper",
    "left_leg_upper",
    "right_leg_lower",
    "left_leg_lower",
    "right_foot",
    "left_foot",
    "gun",
    "riotshield",
];

/// Surface names, by surface type, as scripts get them.
pub(crate) const SURFACES: [&str; 32] = [
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
    "riotshield",
];

/// What the game keeps per missile beyond the shared fields.
#[derive(Clone, Copy, Default)]
pub(crate) struct State {
    /// How far it has flown, for its arming distance.
    pub(crate) travel: f32,
    /// It bounces off what it strikes.
    pub(crate) bounce: bool,
    /// A grenade: it explodes on its fuse.
    pub(crate) grenade: bool,
    /// Made a dud by `makegrenadedud`.
    pub(crate) dud: bool,
    /// Queued for detonation on the next missile think (`detonate`).
    pub(crate) deferred_detonate: bool,
    /// What it rests on; `ENTITY_NONE` while flying.
    pub(crate) ground: i32,
    /// When its fuse runs out; 0 for no fuse.
    pub(crate) explode_at: i32,
    /// A rolling grenade's wobble phase and sideways curve.
    pub(crate) wobble: f32,
    pub(crate) curve: f32,
    /// The surface type it last struck.
    pub(crate) surface: u8,
    /// The surface it came to rest on.
    pub(crate) rest_normal: [f32; 3],
    pub(crate) spawned_at: i32,
}

impl State {
    pub(crate) fn read(game: &[u8; GAME_MISSILE_BYTES]) -> Self {
        let f =
            |at: usize| f32::from_le_bytes([game[at], game[at + 1], game[at + 2], game[at + 3]]);
        let i =
            |at: usize| i32::from_le_bytes([game[at], game[at + 1], game[at + 2], game[at + 3]]);
        Self {
            travel: f(0),
            bounce: game[4] & 1 != 0,
            grenade: game[4] & 2 != 0,
            dud: game[4] & 4 != 0,
            deferred_detonate: game[4] & 8 != 0,
            surface: game[5],
            ground: i(8),
            explode_at: i(12),
            wobble: f(16),
            curve: f(20),
            rest_normal: [f(24), f(28), f(32)],
            spawned_at: i(36),
        }
    }

    pub(crate) fn write(self, game: &mut [u8; GAME_MISSILE_BYTES]) {
        game[0..4].copy_from_slice(&self.travel.to_le_bytes());
        game[4] = u8::from(self.bounce)
            | (u8::from(self.grenade) << 1)
            | (u8::from(self.dud) << 2)
            | (u8::from(self.deferred_detonate) << 3);
        game[5] = self.surface;
        game[8..12].copy_from_slice(&self.ground.to_le_bytes());
        game[12..16].copy_from_slice(&self.explode_at.to_le_bytes());
        game[16..20].copy_from_slice(&self.wobble.to_le_bytes());
        game[20..24].copy_from_slice(&self.curve.to_le_bytes());
        for (i, c) in self.rest_normal.iter().enumerate() {
            game[24 + i * 4..28 + i * 4].copy_from_slice(&c.to_le_bytes());
        }
        game[36..40].copy_from_slice(&self.spawned_at.to_le_bytes());
    }
}

pub(crate) fn mad(base: [f32; 3], v: [f32; 3], scale: f32) -> [f32; 3] {
    [
        v[0] * scale + base[0],
        v[1] * scale + base[1],
        v[2] * scale + base[2],
    ]
}

/// The trajectory's position at `time`.
pub(crate) fn evaluate(tr: &GameTrajectory, time: i32) -> [f32; 3] {
    let t = (time - tr.time) as f32 * 0.001_000_000_05;
    match tr.kind {
        TrajectoryKind::Stationary => tr.base,
        TrajectoryKind::Linear => mad(tr.base, tr.delta, t),
        TrajectoryKind::Gravity(gravity) => {
            let mut at = mad(tr.base, tr.delta, t);
            at[2] -= gravity * t * t * 0.5;
            at
        }
    }
}

/// The trajectory's velocity at `time`.
pub(crate) fn evaluate_delta(tr: &GameTrajectory, time: i32) -> [f32; 3] {
    match tr.kind {
        TrajectoryKind::Stationary => [0.0; 3],
        TrajectoryKind::Linear => tr.delta,
        TrajectoryKind::Gravity(gravity) => {
            let t = (time - tr.time) as f32 * 0.001_000_000_05;
            [tr.delta[0], tr.delta[1], tr.delta[2] - gravity * t]
        }
    }
}

pub(crate) fn angle_vectors(angles: [f32; 3]) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let (sp, cp) = f64::from(angles[0] * DEG2RAD).sin_cos();
    let (sy, cy) = f64::from(angles[1] * DEG2RAD).sin_cos();
    let (sr, cr) = f64::from(angles[2] * DEG2RAD).sin_cos();
    let (sp, cp, sy, cy, sr, cr) = (
        sp as f32, cp as f32, sy as f32, cy as f32, sr as f32, cr as f32,
    );
    (
        [cp * cy, cp * sy, -sp],
        [-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp],
        [cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp],
    )
}

/// The angles a direction points at.
pub(crate) fn vectoangles(v: [f32; 3]) -> [f32; 3] {
    if v[1] == 0.0 && v[0] == 0.0 {
        return [if -v[2] < 0.0 { 270.0 } else { 90.0 }, 0.0, 0.0];
    }
    let wrap = |a: f32| if a < 0.0 { a + 360.0 } else { a };
    let yaw = wrap(f64::from(v[1]).atan2(f64::from(v[0])) as f32 * 57.295_78);
    let flat = (v[0] * v[0] + v[1] * v[1]).sqrt();
    let pitch = wrap(f64::from(-v[2]).atan2(f64::from(flat)) as f32 * 57.295_78);
    [pitch, yaw, 0.0]
}

pub(crate) fn hit_id(trace: &BulletTrace) -> i32 {
    match trace.hit {
        BulletHitType::Entity(id) => id,
        BulletHitType::None | BulletHitType::Glass(_) => ENTITY_NONE,
    }
}

pub(crate) fn sentient(entity: FireEntity) -> bool {
    matches!(entity.kind, FireEntityKind::Player | FireEntityKind::Actor)
}

/// What a missile of this weapon stops at.
pub(crate) fn clipmask(def: &WeaponDefView<'_>) -> u32 {
    if !def.def_bool(def::PLANTABLE) {
        return MISSILE_MASK;
    }
    let mut mask = if def.def_i32(def::PROJECTILE_SPEED_UP) > 0 {
        PLANTABLE_UP_MASK
    } else {
        PLANTABLE_MASK
    };
    if def.def_i32(def::STICKINESS) == 4 {
        mask &= !STICKINESS_FLOOR_CLEAR;
    }
    mask
}

/// A missile's trace; from inside a solid, it reports the way it came.
pub(crate) fn missile_trace(
    start: [f32; 3],
    end: [f32; 3],
    owner: i32,
    mask: u32,
    world: &mut dyn FireWorld,
) -> BulletTrace {
    let mut trace = world.bullet_trace(start, end, owner, mask);
    if trace.start_solid {
        trace.fraction = 0.0;
        let v = [start[0] - end[0], start[1] - end[1], start[2] - end[2]];
        let len = (v[0] * v[0] + v[2] * v[2] + v[1] * v[1]).sqrt();
        let len = if -len >= 0.0 { 1.0 } else { len };
        let inv = 1.0 / len;
        trace.normal = [inv * v[0], inv * v[1], inv * v[2]];
    }
    trace
}

pub(crate) fn surface_type(trace: &BulletTrace) -> i32 {
    (trace.surface_flags >> 20) as i32 & 0x3f
}

/// Within the bounds a missile may fly from the map's centre.
pub(crate) fn in_world(at: [f32; 3], center: [f32; 3]) -> bool {
    let reach_z = (1 << 16) as f32;
    let reach = (1 << 17) as f32;
    let (x, y, z) = (at[0] as i32, at[1] as i32, at[2] as i32);
    z <= (center[2] + reach_z) as i32
        && (center[2] - reach_z) as i32 <= z
        && x <= (center[0] + reach) as i32
        && (center[0] - reach) as i32 <= x
        && y <= (center[1] + reach) as i32
        && (center[1] - reach) as i32 <= y
}

/// The server's random point in the unit circle.
fn random_in_circle(world: &mut dyn FireWorld) -> (f32, f32) {
    let r = |world: &mut dyn FireWorld| f64::from(world.rand()) * f64::from(3.051_757_8e-5_f32);
    let angle = (r(world) * 360.0) as f32;
    let radius = r(world);
    let radians = f64::from(angle) * f64::from(DEG2RAD);
    let (sin, cos) = radians.sin_cos();
    (cos as f32 * radius as f32, radius as f32 * sin as f32)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn launch_rocket(
    owner: i32,
    weapon: u32,
    def: &WeaponDefView<'_>,
    origin: [f32; 3],
    angles: [f32; 3],
    spread: f32,
    gun_velocity: [f32; 3],
    knockback: bool,
    world: &mut dyn FireWorld,
) -> Option<i32> {
    let (forward, right, up) = angle_vectors(angles);
    let aim = (f64::from(spread * DEG2RAD).tan() as f32) * 16.0;
    let (x, y) = random_in_circle(world);
    let (x, y) = (x * aim, y * aim);
    let dir = [
        up[0] * y + right[0] * x + forward[0] * 16.0,
        up[1] * y + right[1] * x + forward[1] * 16.0,
        up[2] * y + right[2] * x + forward[2] * 16.0,
    ];
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    let len = if -len >= 0.0 { 1.0 } else { len };
    let inv = 1.0 / len;
    let dir = [dir[0] * inv, dir[1] * inv, dir[2] * inv];
    let level = world.level_time();
    // A player's rocket starts at their client's time.
    let offset = grenade::launch_offset(owner, world);
    let speed = def.def_i32(def::PROJECTILE_SPEED) as f32;
    let hidden = ((speed * -35.0 * 0.001_666_666_7 + 85.0) as i32).clamp(20, 50);
    // A player's moving platform adds to the gun's velocity, but not for a
    // weapon of guided missile type 6.
    let gun_velocity = if def.guided_missile_type() != 6 {
        let platform = crate::grenade::platform_velocity(owner, world);
        std::array::from_fn(|i| platform[i] + gun_velocity[i])
    } else {
        gun_velocity
    };
    let delta = [
        ((dir[0] * speed) + gun_velocity[0]) as i32 as f32,
        ((speed * dir[1]) + gun_velocity[1]) as i32 as f32,
        ((def.def_i32(def::PROJECTILE_SPEED_UP) as f32 + speed * dir[2]) + gun_velocity[2]) as i32
            as f32,
    ];
    let angles = vectoangles(dir);
    let mut game = [0u8; GAME_MISSILE_BYTES];
    State {
        bounce: false,
        grenade: false,
        ground: ENTITY_NONE,
        spawned_at: level,
        ..State::default()
    }
    .write(&mut game);
    // A ballistic knife (guided missile type 4) falls as a grenade does.
    let kind = if def.guided_missile_type() == 4 {
        TrajectoryKind::Gravity(crate::grenade::gravity(world))
    } else {
        TrajectoryKind::Linear
    };
    let ent = world.spawn_missile(GameMissile {
        entity: ENTITY_NONE,
        owner,
        weapon,
        position: GameTrajectory {
            kind,
            time: level + offset,
            duration: 0,
            base: origin,
            delta,
        },
        angles: GameTrajectory {
            kind: TrajectoryKind::Stationary,
            time: level,
            duration: 0,
            base: angles,
            delta: [0.0; 3],
        },
        origin,
        visible_at: level + offset + hidden,
        game,
    });
    if knockback {
        let z = (-forward[2]).min(0.0);
        world.push_player(owner, [-forward[0] * 64.0, -forward[1] * 64.0, z * 64.0]);
    }
    grenade::catch_up(ent, offset, world);
    ent
}

/// One frame of a missile (`G_RunMissile`).
pub(crate) fn run_missile(
    missile: &mut GameMissile,
    world: &mut dyn FireWorld,
) -> (MissileRun, Vec<&'static Unknown>) {
    let gaps = Vec::new();
    let Some((def_bytes, variant_bytes)) = world
        .weapon(missile.weapon)
        .map(|w| (w.def.clone(), w.variant.clone()))
    else {
        return (MissileRun::Gone, gaps);
    };
    let Some(def) = WeaponDefView::new(&def_bytes, &variant_bytes) else {
        return (MissileRun::Gone, gaps);
    };
    let mut state = State::read(&missile.game);
    let level = world.level_time();
    if state.deferred_detonate {
        let run = grenade::explode(missile, &def, &mut state, world);
        state.write(&mut missile.game);
        return (run, gaps);
    }
    if state.grenade {
        grenade::start_falling(missile, &mut state, &def, world);
    }
    if def.def_i32(def::IS_ROLLING_GRENADE) != 0 && missile.position.kind == TrajectoryKind::Linear
    {
        missile.position.kind = TrajectoryKind::Gravity(grenade::gravity(world));
        missile.position.base = missile.origin;
    }
    let run = fly(missile, &def, &mut state, level, world);
    state.write(&mut missile.game);
    if run != MissileRun::Flying {
        return (run, gaps);
    }
    if state.deferred_detonate || (state.explode_at != 0 && state.explode_at <= level) {
        let run = grenade::explode(missile, &def, &mut state, world);
        state.write(&mut missile.game);
        return (run, gaps);
    }
    (run, gaps)
}

fn fly(
    missile: &mut GameMissile,
    def: &WeaponDefView<'_>,
    state: &mut State,
    level: i32,
    world: &mut dyn FireWorld,
) -> MissileRun {
    let old = missile.origin;
    let new = evaluate(&missile.position, level);
    let d = [new[0] - old[0], new[1] - old[1], new[2] - old[2]];
    let len = (d[2] * d[2] + d[1] * d[1] + d[0] * d[0]).sqrt();
    if len < 0.001_000_000_05 {
        return MissileRun::Flying;
    }
    let mask = clipmask(def);
    let trace_mask = if len <= 20.0 || world.point_contents(old, CONTENTS_WATER) != 0 {
        mask
    } else {
        mask | CONTENTS_WATER
    };
    let mut trace = missile_trace(old, new, missile.owner, trace_mask, world);
    if surface_type(&trace) == 20 {
        trace = missile_trace(old, new, missile.owner, mask, world);
    }
    let f = trace.fraction;
    let end = [
        (new[0] - old[0]) * f + old[0],
        (new[1] - old[1]) * f + old[1],
        (new[2] - old[2]) * f + old[2],
    ];
    missile.origin = end;
    if def.def_i32(def::PROJECTILE_ACTIVATE_DIST) > 0 {
        let m = [end[0] - old[0], end[1] - old[1], end[2] - old[2]];
        state.travel += (m[2] * m[2] + m[1] * m[1] + m[0] * m[0]).sqrt();
    }
    if f < 1.0 {
        if trace.surface_flags & SURF_SKY != 0 || trace.contents & CONTENTS_MISSILE_PASS != 0 {
            missile.origin = new;
            return MissileRun::Flying;
        }
        let inv = 1.0 / if -len >= 0.0 { 1.0 } else { len };
        let dir = [d[0] * inv, d[1] * inv, d[2] * inv];
        if let Some(run) = impact(missile, def, &trace, dir, old, end, state, world) {
            return run;
        }
        if def.def_i32(def::IS_ROLLING_GRENADE) != 0
            && trace.normal[2] > 0.7
            && missile.position.kind != TrajectoryKind::Stationary
        {
            missile.position.kind = TrajectoryKind::Linear;
            missile.position.base = missile.origin;
        }
    }
    if !in_world(missile.origin, world.map_center()) {
        if def.def_i32(def::IS_VALUABLE) != 0 {
            missile.origin = old;
            return MissileRun::Flying;
        }
        return MissileRun::Gone;
    }
    MissileRun::Flying
}

/// Whether a non-exploding projectile crumples on a steep, fast impact
/// instead of bouncing.
fn crumples(
    missile: &GameMissile,
    def: &WeaponDefView<'_>,
    state: &State,
    trace: &BulletTrace,
    world: &dyn FireWorld,
) -> bool {
    if state.dud || def.def_i32(def::WEAP_TYPE) != weap_type::PROJECTILE {
        return false;
    }
    if surface_type(trace) == usize::from(SURF_FLESH) as i32 {
        return true;
    }
    if def.def_bool(def::NO_CRUMPLE_MISSILE) {
        return false;
    }
    let vel = evaluate_delta(&missile.position, grenade::impact_time(trace, world));
    let speed = (vel[0] * vel[0] + vel[1] * vel[1] + vel[2] * vel[2]).sqrt();
    speed >= 500.0
        && -(trace.normal[0] * vel[0] + trace.normal[1] * vel[1] + trace.normal[2] * vel[2]) / speed
            > 0.707
}

/// `G_MissileImpact`.
#[allow(clippy::too_many_arguments)]
fn impact(
    missile: &mut GameMissile,
    def: &WeaponDefView<'_>,
    trace: &BulletTrace,
    dir: [f32; 3],
    old: [f32; 3],
    end: [f32; 3],
    state: &mut State,
    world: &mut dyn FireWorld,
) -> Option<MissileRun> {
    if matches!(trace.hit, BulletHitType::Glass(_)) {
        return None;
    }
    state.surface = surface_type(trace) as u8;
    let target = hit_id(trace);
    let level = world.level_time();
    let activate = def.def_i32(def::PROJECTILE_ACTIVATE_DIST);
    let unarmed = activate > 0 && state.travel <= activate as f32;
    let explodes = def.def_bool(def::PROJ_IMPACT_EXPLODE) && !unarmed;
    let means = if explodes {
        if state.grenade {
            MOD_GRENADE
        } else {
            MOD_PROJECTILE
        }
    } else {
        MOD_IMPACT
    };
    let hit = world.entity(target);
    let mut hit_location = if !explodes && !unarmed && def.def_i32(def::PROJ_EXPLOSION) != 9 {
        0
    } else {
        trace.hit_location
    };
    let amount = def.def_i32(def::DAMAGE);
    if hit.is_some_and(|e| e.takes_damage) && amount > 0 {
        let mut velocity = evaluate_delta(&missile.position, level);
        let speed_sq =
            velocity[0] * velocity[0] + velocity[1] * velocity[1] + velocity[2] * velocity[2];
        if speed_sq < 0.001 {
            velocity = [0.0, 0.0, 1.0];
        }
        let speed = speed_sq.sqrt();
        let weap_ty = def.def_i32(def::WEAP_TYPE);
        let deals = !explodes
            || (!matches!(weap_ty, weap_type::GRENADE | weap_type::MINE)
                || def.def_i32(def::PROJ_EXPLOSION) == 7
                || speed > grenade::dvar(world, "g_minGrenadeDamageSpeed", 400.0));
        if deals && hit_location != HITLOC_RIOTSHIELD {
            let flags = if explodes && def.def_bool(def::ARMOR_PIERCING) {
                2
            } else {
                0
            };
            let loc_idx = usize::try_from(hit_location)
                .ok()
                .filter(|&i| i < HIT_LOCATIONS.len())
                .unwrap_or(0);
            world.damage(&FireDamage {
                target,
                inflictor: missile.entity,
                attacker: missile.owner,
                dir: velocity,
                point: missile.origin,
                amount,
                flags,
                means: MEANS[means],
                weapon: missile.weapon,
                hit_location: HIT_LOCATIONS[loc_idx],
                location: hit_location,
                bone: trace.bone,
                time_offset: 0,
            });
        }
    } else {
        hit_location = trace.hit_location;
    }

    if state.bounce && !explodes && !crumples(missile, def, state, trace, world) {
        if state.surface == 0 && hit.is_some_and(sentient) {
            state.surface = if hit_location == HITLOC_RIOTSHIELD {
                SURF_RIOTSHIELD
            } else {
                SURF_FLESH
            };
        }
        grenade::bounce(missile, def, state, trace, end, hit, world);
        if unarmed && missile.position.kind == TrajectoryKind::Stationary {
            return Some(MissileRun::Gone);
        }
        return None;
    }

    if amount != 0 {
        world.damage_triggers(missile.owner, old, end, amount, MEANS[means]);
    }
    let in_water = world.point_contents(end, CONTENTS_WATER) != 0;
    state.surface = if in_water {
        20
    } else {
        surface_type(trace) as u8
    };
    world.set_entity_pose(missile.entity, end, evaluate(&missile.angles, level));

    // Retrievable or die-on-respawn projectile (the Ballistic Knife):
    // 6-argument `stationary`.
    if !explodes && (def.def_bool(def::RETRIEVABLE) || def.def_bool(def::WATCHER)) {
        let angles = vectoangles(dir);
        let prey = if hit.is_some_and(sentient) {
            target
        } else {
            ENTITY_NONE
        };
        let bone = if prey != ENTITY_NONE { "j_knee_ri" } else { "" };
        world.notify(
            missile.entity,
            "stationary",
            &[
                NotifyArg::Vector(end),
                NotifyArg::Vector(trace.normal),
                NotifyArg::Vector(angles),
                NotifyArg::Entity(missile.owner),
                NotifyArg::Entity(prey),
                NotifyArg::String(bone),
            ],
        );
        return Some(MissileRun::Gone);
    }

    let normal = if def.def_bool(def::PROJ_EXPLOSION_EFFECT_FORCE_NORMAL_UP) {
        [0.0, 0.0, 1.0]
    } else {
        trace.normal
    };
    world.explosion(&MissileExplosion {
        missile: missile.entity,
        owner: missile.owner,
        weapon: missile.weapon,
        origin: end,
        normal,
        surface_type: state.surface,
    });
    if explodes && def.def_i32(def::EXPLOSION_INNER_DAMAGE) != 0 {
        let cone_angle = def.def_f32(def::DAMAGE_CONE_ANGLE);
        let cone = (cone_angle - 180.0 < 0.0).then(|| f64::from(cone_angle * DEG2RAD).cos() as f32);
        let ignore = match hit {
            Some(entity) if entity.blast_handler => ENTITY_WORLD,
            _ => target,
        };
        let splash_means = grenade::splash_means(def, state.grenade);
        grenade::radius_damage(
            &RadiusDamage {
                origin: end,
                inflictor: missile.entity,
                attacker: missile.owner,
                inner: def.def_i32(def::EXPLOSION_INNER_DAMAGE) as f32,
                outer: def.def_i32(def::EXPLOSION_OUTER_DAMAGE) as f32,
                radius: def.def_i32(def::EXPLOSION_RADIUS) as f32,
                cone: cone.map(|cos| (cos, dir)),
                ignore,
                means: MEANS[splash_means],
                weapon: missile.weapon,
                script: false,
            },
            world,
        );
        world.notify(
            missile.owner,
            "projectile_impact",
            &[
                NotifyArg::Weapon(missile.weapon),
                NotifyArg::Vector(end),
                NotifyArg::Int(def.def_i32(def::EXPLOSION_RADIUS)),
                NotifyArg::Entity(missile.entity),
            ],
        );
    }
    missile.origin = end;
    Some(MissileRun::Gone)
}

pub struct T6Missiles;

pub static MISSILES: T6Missiles = T6Missiles;

impl MissilePhysics for T6Missiles {
    fn launch_grenade(
        &self,
        owner: i32,
        weapon: u32,
        origin: [f32; 3],
        angles: [f32; 3],
        launcher: bool,
        remaining_fuse: Option<i32>,
        world: &mut dyn FireWorld,
    ) -> (Option<i32>, Vec<&'static Unknown>) {
        let Some((def_bytes, variant_bytes)) = world
            .weapon(weapon)
            .map(|w| (w.def.clone(), w.variant.clone()))
        else {
            return (None, Vec::new());
        };
        let Some(def) = WeaponDefView::new(&def_bytes, &variant_bytes) else {
            return (None, Vec::new());
        };
        let ent = grenade::launch_grenade(
            owner,
            weapon,
            &def,
            origin,
            angles,
            launcher,
            remaining_fuse,
            world,
        );
        (ent, Vec::new())
    }

    fn spawn_grenade(
        &self,
        owner: i32,
        weapon: u32,
        origin: [f32; 3],
        velocity: [f32; 3],
        fuse: i32,
        world: &mut dyn FireWorld,
    ) -> (Option<i32>, Vec<&'static Unknown>) {
        let Some((def_bytes, variant_bytes)) = world
            .weapon(weapon)
            .map(|w| (w.def.clone(), w.variant.clone()))
        else {
            return (None, Vec::new());
        };
        let Some(def) = WeaponDefView::new(&def_bytes, &variant_bytes) else {
            return (None, Vec::new());
        };
        let ent = grenade::spawn_grenade(
            owner,
            weapon,
            &def,
            origin,
            velocity,
            [[0.0; 3]; 2],
            true,
            fuse,
            None,
            world,
        );
        (ent, Vec::new())
    }

    fn launch_rocket(
        &self,
        owner: i32,
        weapon: u32,
        origin: [f32; 3],
        angles: [f32; 3],
        spread: f32,
        velocity: [f32; 3],
        world: &mut dyn FireWorld,
    ) -> (Option<i32>, Vec<&'static Unknown>) {
        let Some((def_bytes, variant_bytes)) = world
            .weapon(weapon)
            .map(|w| (w.def.clone(), w.variant.clone()))
        else {
            return (None, Vec::new());
        };
        let Some(def) = WeaponDefView::new(&def_bytes, &variant_bytes) else {
            return (None, Vec::new());
        };
        let knockback = def.def_i32(def::FIRE_TYPE) != 0;
        let ent = launch_rocket(
            owner, weapon, &def, origin, angles, spread, velocity, knockback, world,
        );
        (ent, Vec::new())
    }

    fn run(
        &self,
        missile: &mut GameMissile,
        world: &mut dyn FireWorld,
    ) -> (MissileRun, Vec<&'static Unknown>) {
        run_missile(missile, world)
    }

    fn detonate(
        &self,
        missile: &mut GameMissile,
        attacker: Option<i32>,
        world: &mut dyn FireWorld,
    ) -> (MissileRun, Vec<&'static Unknown>) {
        if let Some(attacker) = attacker {
            missile.owner = attacker;
        }
        let mut state = State::read(&missile.game);
        if grenade::dvar(world, "tu_deferScriptMissileDetonation", 1.0) != 0.0 {
            state.deferred_detonate = true;
            state.write(&mut missile.game);
            return (MissileRun::Flying, Vec::new());
        }
        let Some((def_bytes, variant_bytes)) = world
            .weapon(missile.weapon)
            .map(|w| (w.def.clone(), w.variant.clone()))
        else {
            return (MissileRun::Gone, Vec::new());
        };
        let Some(def) = WeaponDefView::new(&def_bytes, &variant_bytes) else {
            return (MissileRun::Gone, Vec::new());
        };
        let run = grenade::explode(missile, &def, &mut state, world);
        state.write(&mut missile.game);
        (run, Vec::new())
    }

    fn make_dud(&self, missile: &mut GameMissile) {
        let mut state = State::read(&missile.game);
        state.dud = true;
        state.write(&mut missile.game);
    }

    fn reset_detonation_time(
        &self,
        missile: &mut GameMissile,
        after_ms: Option<i32>,
        world: &dyn FireWorld,
    ) {
        let level = world.level_time();
        let mut state = State::read(&missile.game);
        if let Some(ms) = after_ms {
            state.explode_at = level + ms;
            state.write(&mut missile.game);
            return;
        }
        let Some(weapon) = world.weapon(missile.weapon) else {
            return;
        };
        let Some(def) = WeaponDefView::new(&weapon.def, &weapon.variant) else {
            return;
        };
        if def.def_bool(def::TIMED_DETONATION) {
            let fuse = if world.player(missile.owner).is_some() {
                def.def_i32(def::FUSE_TIME)
            } else {
                def.def_i32(def::AI_FUSE_TIME)
            };
            state.explode_at = level + fuse;
            state.write(&mut missile.game);
        }
    }

    fn ground_entity(&self, missile: &GameMissile) -> i32 {
        State::read(&missile.game).ground
    }
}
