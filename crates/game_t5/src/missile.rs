//! Black Ops' missiles: a projectile weapon's rocket, moved every frame
//! along its trajectory and traced against what it may hit, and its
//! impact: the direct hit, the explosion and its blast.

use fastfile_t5::weapon_def::WeaponDefView;
use game_api::fire::{
    AiEvent, AiLineEvent, AiPointEvent, BulletHitType, BulletTrace, FireWorld, GAME_MISSILE_BYTES,
    GameMissile, GameTrajectory, MissileExplosion, MissileRun, NotifyArg, RadiusDamage,
    TrajectoryKind,
};
use game_api::movement::{ENTITY_NONE, ENTITY_WORLD};
use game_api::{Unknown, unknown};

use crate::fire::{DEG2RAD, Hit, MEANS, damage, dvar, hit_id};

/// What a missile stops at.
pub(crate) const MISSILE_MASK: u32 = 0x0280_e091;
const CONTENTS_WATER: u32 = 0x20;
/// Contents a missile flies through as if they were open sky.
const CONTENTS_MISSILE_PASS: u32 = 0x800;
const SURF_SKY: u32 = 0x4;
const SURF_WATER: i32 = 20;
const SURF_GLASS: i32 = 9;
/// A missile's direct hit and its blast.
const MOD_PROJECTILE: usize = 5;
const MOD_PROJECTILE_SPLASH: usize = 6;
const MOD_IMPACT: usize = 16;
/// `weaponType`: a projectile weapon.
const WEAPTYPE_PROJECTILE: i32 = 2;
/// `projExplosionType` grenade: the projectile leaves its grenade weapon.
const EXPLOSION_GRENADE: i32 = 9;
/// The surface flags of a strike on flesh, alone.
const SURF_FLESH: u32 = 7 << 20;
/// How fast and how head on a strike crumples a bouncing projectile.
const CRUMPLE_SPEED: f32 = 500.0;
const CRUMPLE_COS: f32 = 0.707;

fn missile_unknown() -> &'static Unknown {
    unknown!(
        "t5.weapons.missile",
        "a missile that accelerates, curves, destabilises, is guided, bounces or sticks flies straight instead",
        "Black Ops' missile acceleration, curvature, destabilisation, guidance, bounce and stick rules"
    )
}

fn bolt_unknown() -> &'static Unknown {
    unknown!(
        "t5.weapons.bolt",
        "a bolt without a grenade (the ballistic knife) is not drawn stuck in what it struck",
        "Black Ops' client's stuck bolt model for its impact event"
    )
}

fn missile_water_unknown() -> &'static Unknown {
    unknown!(
        "t5.weapons.missile_water",
        "a missile entering water makes no splash, and an explosion under water is not told apart",
        "Black Ops' missile splash and its water height test in the runtime"
    )
}

fn missile_glass_unknown() -> &'static Unknown {
    unknown!(
        "t5.weapons.missile_glass",
        "a missile does not break glass it flies into",
        "Black Ops' missile glass damage in the runtime"
    )
}

/// What the game keeps per missile beyond the shared fields.
#[derive(Clone, Copy, Default)]
pub(crate) struct State {
    /// How far it has flown, for its arming distance.
    pub(crate) travel: f32,
    /// It bounces off what it strikes.
    pub(crate) bounce: bool,
    /// A grenade: it explodes on its fuse.
    pub(crate) grenade: bool,
    /// Fired by a grenade launcher: it never destabilises.
    pub(crate) launched: bool,
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
    /// When the grenade last broadcast its AI ping.
    pub(crate) last_ping_at: i32,
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
            launched: game[4] & 4 != 0,
            surface: game[5],
            ground: i(8),
            explode_at: i(12),
            wobble: f(16),
            curve: f(20),
            rest_normal: [f(24), f(28), f(32)],
            spawned_at: i(36),
            last_ping_at: i(40),
        }
    }

    pub(crate) fn write(self, game: &mut [u8; GAME_MISSILE_BYTES]) {
        game[0..4].copy_from_slice(&self.travel.to_le_bytes());
        game[4] =
            u8::from(self.bounce) | u8::from(self.grenade) << 1 | u8::from(self.launched) << 2;
        game[5] = self.surface;
        game[8..12].copy_from_slice(&self.ground.to_le_bytes());
        game[12..16].copy_from_slice(&self.explode_at.to_le_bytes());
        game[16..20].copy_from_slice(&self.wobble.to_le_bytes());
        game[20..24].copy_from_slice(&self.curve.to_le_bytes());
        for (i, c) in self.rest_normal.iter().enumerate() {
            game[24 + i * 4..28 + i * 4].copy_from_slice(&c.to_le_bytes());
        }
        game[36..40].copy_from_slice(&self.spawned_at.to_le_bytes());
        game[40..44].copy_from_slice(&self.last_ping_at.to_le_bytes());
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
    let t = (time - tr.time) as f32 * 0.00100000005;
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
            let t = (time - tr.time) as f32 * 0.00100000005;
            [tr.delta[0], tr.delta[1], tr.delta[2] - gravity * t]
        }
    }
}

/// The angles a direction points at.
pub(crate) fn vectoangles(v: [f32; 3]) -> [f32; 3] {
    let yaw = if v[1] == 0.0 && v[0] == 0.0 {
        0.0
    } else {
        movement_t5::math::vectoyaw(v)
    };
    [movement_t5::math::vectopitch(v), yaw, 0.0]
}

/// The server's random point in the unit circle (not seeded by the shot).
fn random_in_circle(world: &mut dyn FireWorld) -> (f32, f32) {
    let r = |world: &mut dyn FireWorld| f64::from(world.rand()) * f64::from(3.05175781e-05f32);
    let angle = (r(world) * 360.0) as f32;
    let radius = r(world);
    let radians = f64::from(angle) * f64::from(DEG2RAD);
    let (sin, cos) = radians.sin_cos();
    ((cos as f32 * radius as f32), (radius as f32 * sin as f32))
}

/// A projectile weapon's shot: a rocket along the spread, and the shooter
/// pushed back by any weapon not firing fully automatic.
#[allow(clippy::too_many_arguments)]
pub(crate) fn fire_rocket(
    attacker: i32,
    weapon: u32,
    def: &WeaponDefView<'_>,
    spread: f32,
    forward: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
    muzzle: [f32; 3],
    world: &mut dyn FireWorld,
    gaps: &mut Vec<&'static Unknown>,
) {
    let aim = (f64::from(spread * DEG2RAD).tan() as f32) * 16.0;
    let (x, y) = random_in_circle(world);
    let (x, y) = (x * aim, y * aim);
    let dir = [
        up[0] * y + right[0] * x + forward[0] * 16.0,
        up[1] * y + right[1] * x + forward[1] * 16.0,
        up[2] * y + right[2] * x + forward[2] * 16.0,
    ];
    let len = (dir[2] * dir[2] + dir[1] * dir[1] + dir[0] * dir[0]).sqrt();
    let len = if -len >= 0.0 { 1.0 } else { len };
    let inv = 1.0 / len;
    let dir = [dir[0] * inv, dir[1] * inv, dir[2] * inv];
    spawn_rocket(attacker, weapon, def, muzzle, dir, [0.0; 3], world, gaps);
    if def.fire_type() != 0 {
        let z = (-forward[2]).min(0.0);
        world.push_player(attacker, [-forward[0] * 64.0, -forward[1] * 64.0, z * 64.0]);
    }
}

/// A rocket leaving the muzzle: straight along its direction at the
/// weapon's speed, drawn once clear of the shooter.
#[allow(clippy::too_many_arguments)]
fn spawn_rocket(
    owner: i32,
    weapon: u32,
    def: &WeaponDefView<'_>,
    start: [f32; 3],
    dir: [f32; 3],
    gun_velocity: [f32; 3],
    world: &mut dyn FireWorld,
    gaps: &mut Vec<&'static Unknown>,
) {
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    let len = if -len >= 0.0 { 1.0 } else { len };
    let inv = 1.0 / len;
    let dir = [dir[0] * inv, dir[1] * inv, dir[2] * inv];
    if def.guided_missile_type() != 0
        || def.time_to_accelerate() > 0.0
        || def.projectile_curvature() > 0.0
    {
        gaps.push(missile_unknown());
    }
    let level = world.level_time();
    let speed = def.projectile_speed() as f32;
    let hidden = ((speed * -35.0 * 0.00166666671 + 85.0) as i32).clamp(20, 50);
    let delta = [
        ((dir[0] * speed) + gun_velocity[0]) as i32 as f32,
        ((speed * dir[1]) + gun_velocity[1]) as i32 as f32,
        ((def.projectile_speed_up() as f32 + speed * dir[2]) + gun_velocity[2]) as i32 as f32,
    ];
    let angles = vectoangles(dir);
    let mut game = [0u8; GAME_MISSILE_BYTES];
    State {
        bounce: def.force_bounce(),
        ground: ENTITY_NONE,
        spawned_at: level,
        ..State::default()
    }
    .write(&mut game);
    world.spawn_missile(GameMissile {
        entity: ENTITY_NONE,
        owner,
        weapon,
        position: GameTrajectory {
            kind: TrajectoryKind::Linear,
            time: level,
            duration: 0,
            base: start,
            delta,
        },
        angles: GameTrajectory {
            kind: TrajectoryKind::Stationary,
            time: level,
            duration: 0,
            base: angles,
            delta: [0.0; 3],
        },
        origin: start,
        visible_at: level + hidden,
        game,
    });
}

/// One frame of a missile: along its trajectory, traced against what its
/// mask names (water too once clear of the muzzle), into its impact.
pub(crate) fn run_missile(
    missile: &mut GameMissile,
    world: &mut dyn FireWorld,
) -> (MissileRun, Vec<&'static Unknown>) {
    let mut gaps = Vec::new();
    let Some((def_bytes, variant_bytes)) = world
        .weapon(missile.weapon)
        .map(|w| (w.def.clone(), w.variant.clone()))
    else {
        gaps.push(crate::movement::weapon_unknown());
        return (MissileRun::Gone, gaps);
    };
    let Some(def) = WeaponDefView::new(&def_bytes, &variant_bytes) else {
        gaps.push(crate::movement::weapon_unknown());
        return (MissileRun::Gone, gaps);
    };
    let mut state = State::read(&missile.game);
    let level = world.level_time();
    if state.grenade {
        if def.offhand_class() != 0 && level - state.last_ping_at > 250 {
            world.ai_event(AiEvent::Point {
                originator: missile.entity,
                kind: AiPointEvent::GrenadePing {
                    no_ping: def.no_ping(),
                },
                teams: Some(0xff),
                origin: missile.origin,
            });
            state.last_ping_at = level;
        }
        crate::grenade::start_falling(missile, &mut state, &def, world);
    } else {
        world.ai_event(AiEvent::Point {
            originator: missile.entity,
            kind: AiPointEvent::ProjectilePing {
                no_ping: def.no_ping(),
            },
            teams: Some(0xff),
            origin: missile.origin,
        });
    }
    let run = fly(missile, &def, &mut state, level, world, &mut gaps);
    state.write(&mut missile.game);
    if run != MissileRun::Flying {
        return (run, gaps);
    }
    if state.explode_at != 0 && state.explode_at <= level {
        let run = crate::grenade::explode(missile, &def, &mut state, world, &mut gaps);
        state.write(&mut missile.game);
        return (run, gaps);
    }
    (run, gaps)
}

/// The missile's move this frame, into what it strikes.
fn fly(
    missile: &mut GameMissile,
    def: &WeaponDefView<'_>,
    state: &mut State,
    level: i32,
    world: &mut dyn FireWorld,
    gaps: &mut Vec<&'static Unknown>,
) -> MissileRun {
    let old = missile.origin;
    let new = evaluate(&missile.position, level);
    let d = [new[0] - old[0], new[1] - old[1], new[2] - old[2]];
    let len = (d[2] * d[2] + d[1] * d[1] + d[0] * d[0]).sqrt();
    if len < 0.00100000005 {
        return MissileRun::Flying;
    }
    if def.weapon_type() == WEAPTYPE_PROJECTILE && def.proj_explosion_type() == 9 {
        gaps.push(missile_unknown());
    }
    let clipmask = clipmask(def);
    let mask = if len <= 20.0 || world.point_contents(old, CONTENTS_WATER) != 0 {
        clipmask
    } else {
        clipmask | CONTENTS_WATER
    };
    let mut trace = missile_trace(def, old, new, missile.owner, mask, world);
    if surface_type(&trace) == SURF_WATER {
        gaps.push(missile_water_unknown());
        trace = missile_trace(def, old, new, missile.owner, clipmask, world);
    }
    if surface_type(&trace) == SURF_GLASS {
        gaps.push(missile_glass_unknown());
    }
    let f = trace.fraction;
    let end = [
        (new[0] - old[0]) * f + old[0],
        (new[1] - old[1]) * f + old[1],
        (new[2] - old[2]) * f + old[2],
    ];
    missile.origin = end;
    if def.projectile_activate_dist() > 0 {
        let m = [end[0] - old[0], end[1] - old[1], end[2] - old[2]];
        state.travel += (m[2] * m[2] + m[1] * m[1] + m[0] * m[0]).sqrt();
    }
    if f == 1.0 {
        if def.weapon_type() == WEAPTYPE_PROJECTILE
            && !state.launched
            && def.destabilize_distance() > 0
            && def.destabilization_rate_time() != 0.0
        {
            gaps.push(missile_unknown());
        }
    } else {
        if trace.surface_flags & SURF_SKY != 0 || trace.contents & CONTENTS_MISSILE_PASS != 0 {
            missile.origin = new;
            return MissileRun::Flying;
        }
        let inv = 1.0 / if -len >= 0.0 { 1.0 } else { len };
        let dir = [d[0] * inv, d[1] * inv, d[2] * inv];
        if let Some(run) = impact(missile, def, &trace, dir, end, state, world, gaps) {
            return run;
        }
    }
    if !in_world(missile.origin, world.map_center()) {
        return MissileRun::Gone;
    }
    MissileRun::Flying
}

/// What a missile of this weapon stops at: plantables stop at more, and a
/// grenade that sticks to the ground keeping its yaw passes bodies.
pub(crate) fn clipmask(def: &WeaponDefView<'_>) -> u32 {
    let mask = if def.plantable() {
        MISSILE_MASK | 0x0001_0000
    } else {
        MISSILE_MASK
    };
    if def.stickiness() == 4 {
        mask & !CONTENTS_BODY
    } else {
        mask
    }
}

/// The contents of a player's or an actor's body.
const CONTENTS_BODY: u32 = 0x0200_0000;

/// A missile's trace: a point, or for a plantable a box
/// `missilePlantableSize` wide and twice as tall from the point up. From
/// inside a solid it reports the way it came.
pub(crate) fn missile_trace(
    def: &WeaponDefView<'_>,
    start: [f32; 3],
    end: [f32; 3],
    owner: i32,
    mask: u32,
    world: &mut dyn FireWorld,
) -> BulletTrace {
    let mut trace = if def.plantable() {
        let r = dvar(world, "missilePlantableSize", 5.0f32);
        world.box_trace(start, end, [-r, -r, 0.0], [r, r, r * 2.0], owner, mask)
    } else {
        world.bullet_trace(start, end, owner, mask)
    };
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

/// A strike that sets off a bouncing projectile (a launched grenade) at
/// once: on flesh, or, unless the weapon never crumples, at 500 units a
/// second or more within 45 degrees of head on.
fn crumples(
    missile: &GameMissile,
    def: &WeaponDefView<'_>,
    trace: &BulletTrace,
    world: &dyn FireWorld,
) -> bool {
    if def.weapon_type() != WEAPTYPE_PROJECTILE {
        return false;
    }
    if trace.surface_flags == SURF_FLESH {
        return true;
    }
    if def.no_crumple_missile() {
        return false;
    }
    let previous = world.previous_level_time();
    let at = ((world.level_time() - previous) as f32 * trace.fraction) as i32 + previous;
    let v = evaluate_delta(&missile.position, at);
    let len = ((v[1] * v[1] + v[0] * v[0]) + v[2] * v[2]).sqrt();
    if CRUMPLE_SPEED > len {
        return false;
    }
    let inv = -1.0 / len;
    let n = trace.normal;
    let head_on = (n[1] * (v[1] * inv) + n[2] * (inv * v[2])) + n[0] * (inv * v[0]);
    head_on > CRUMPLE_COS
}

/// A missile striking: its direct hit, its explosion and the blast; `None`
/// when it flies on (glass).
#[allow(clippy::too_many_arguments)]
fn impact(
    missile: &mut GameMissile,
    def: &WeaponDefView<'_>,
    trace: &BulletTrace,
    dir: [f32; 3],
    end: [f32; 3],
    state: &mut State,
    world: &mut dyn FireWorld,
    gaps: &mut Vec<&'static Unknown>,
) -> Option<MissileRun> {
    if matches!(trace.hit, BulletHitType::Glass(_)) {
        gaps.push(missile_glass_unknown());
        return None;
    }
    state.surface = surface_type(trace) as u8;
    let target = hit_id(trace);
    let level = world.level_time();
    let unarmed =
        def.projectile_activate_dist() > 0 && state.travel <= def.projectile_activate_dist() as f32;
    let explodes = def.proj_impact_explode() && !unarmed;
    let means = if explodes { MOD_PROJECTILE } else { MOD_IMPACT };
    if missile.owner != ENTITY_NONE
        && let Some(owner) = world.entity(missile.owner)
    {
        world.ai_event(AiEvent::Line {
            originator: missile.owner,
            hit: ENTITY_NONE,
            kind: AiLineEvent::ProjectileImpact,
            teams: None,
            start: owner.origin,
            end,
        });
    }
    let amount = def.damage();
    let hit = world.entity(target);
    if let Some(entity) = hit.filter(|e| e.takes_damage)
        && amount > 0
    {
        let mut velocity = evaluate_delta(&missile.position, level);
        if velocity[2] * velocity[2] + velocity[1] * velocity[1] + velocity[0] * velocity[0]
            < 0.00100000005
        {
            velocity = [0.0, 0.0, 1.0];
        }
        damage(
            gaps,
            missile.owner,
            missile.entity,
            entity,
            Hit {
                target,
                dir: velocity,
                point: missile.origin,
                amount,
                flags: 0,
                means,
                weapon: missile.weapon,
                location: trace.hit_location,
                bone: trace.bone,
            },
            world,
        );
    }
    if !explodes {
        if unarmed {
            // A dud: it falls dead where it strikes.
            gaps.push(missile_unknown());
            return Some(MissileRun::Gone);
        }
        // A projectile that neither explodes on impact nor bounces ends
        // where it strikes, by its explosion type.
        if state.bounce && !def.proj_impact_explode() && !crumples(missile, def, trace, world) {
            crate::grenade::bounce(missile, def, state, trace, end, world, gaps);
            return None;
        }
    }
    if amount != 0 {
        world.damage_triggers(missile.owner, missile.origin, end, amount, MEANS[means]);
    }
    let normal = if def.proj_explosion_effect_force_normal_up() {
        [0.0, 0.0, 1.0]
    } else {
        trace.normal
    };
    gaps.push(missile_water_unknown());
    if def.proj_explosion_type() == EXPLOSION_GRENADE {
        // A bolt: its impact event plays the surface's bolt impact where it
        // struck, and it leaves its grenade there. Without a grenade the
        // client also leaves the bolt's model stuck in what it hit, which
        // is not drawn.
        world.explosion(&MissileExplosion {
            missile: missile.entity,
            owner: missile.owner,
            weapon: missile.weapon,
            origin: end,
            normal,
            surface_type: surface_type(trace) as u8,
        });
        let grenade = world
            .weapon(missile.weapon)
            .map_or(0, |weapon| weapon.grenade_weapon);
        if grenade != 0 {
            let on = match trace.hit {
                BulletHitType::Entity(entity) => entity,
                _ => ENTITY_WORLD,
            };
            crate::grenade::leave_grenade(
                missile.owner,
                grenade,
                end,
                dir,
                crate::grenade::Stuck {
                    on,
                    normal: trace.normal,
                },
                world,
                gaps,
            );
        } else if !def.retrievable() && !def.die_on_respawn() {
            gaps.push(bolt_unknown());
        }
    } else {
        world.explosion(&MissileExplosion {
            missile: missile.entity,
            owner: missile.owner,
            weapon: missile.weapon,
            origin: end,
            normal,
            surface_type: surface_type(trace) as u8,
        });
    }
    // A retrievable bolt (the ballistic knife) stays as a weapon object:
    // its scripts hear where it came to rest and what it struck.
    let retrievable = def.retrievable() || def.die_on_respawn();
    if retrievable {
        let prey = match trace.hit {
            BulletHitType::Entity(entity) if entity != ENTITY_WORLD => entity,
            _ => ENTITY_NONE,
        };
        let bone = match (prey, trace.bone) {
            (ENTITY_NONE, _) => NotifyArg::String(""),
            (entity, Some(bone)) => NotifyArg::Bone { entity, bone },
            (entity, None) => match world.entity(entity) {
                Some(e)
                    if matches!(
                        e.kind,
                        game_api::fire::FireEntityKind::Player
                            | game_api::fire::FireEntityKind::Actor
                    ) =>
                {
                    NotifyArg::String("j_knee_ri")
                }
                _ => NotifyArg::String(""),
            },
        };
        world.notify(
            missile.entity,
            "stationary",
            &[
                NotifyArg::Vector(end),
                NotifyArg::Vector(trace.normal),
                NotifyArg::Vector(vectoangles(dir)),
                NotifyArg::Entity(missile.owner),
                if prey == ENTITY_NONE {
                    NotifyArg::Undefined
                } else {
                    NotifyArg::Entity(prey)
                },
                bone,
            ],
        );
    }
    if def.proj_impact_explode() && def.explosion_inner_damage() != 0 {
        let cone = (def.damage_cone_angle() - 180.0 < 0.0)
            .then(|| ((def.damage_cone_angle() * DEG2RAD) as f64).cos() as f32);
        // The entity struck directly is spared the blast, unless it has its
        // own blast rule.
        let ignore = match hit {
            Some(entity) if entity.blast_handler => ENTITY_WORLD,
            _ => target,
        };
        gaps.extend(crate::radius::radius_damage(
            &RadiusDamage {
                origin: end,
                inflictor: missile.entity,
                attacker: missile.owner,
                inner: def.explosion_inner_damage() as f32,
                outer: def.explosion_outer_damage() as f32,
                radius: def.explosion_radius() as f32,
                cone: cone.map(|cos| (cos, dir)),
                ignore,
                means: MEANS[MOD_PROJECTILE_SPLASH],
                weapon: missile.weapon,
                script: false,
            },
            world,
        ));
        world.notify(
            missile.owner,
            "projectile_impact",
            &[
                NotifyArg::Weapon(missile.weapon),
                NotifyArg::Vector(end),
                NotifyArg::Float(def.explosion_radius() as f32),
                NotifyArg::Entity(missile.entity),
            ],
        );
    }
    if def.proj_explosion_type() == 2 {
        gaps.push(missile_unknown());
    }
    missile.origin = end;
    Some(if retrievable {
        MissileRun::Stationary
    } else {
        MissileRun::Gone
    })
}

/// Resets a missile's fuse (`resetmissiledetonationtime`): `after_ms` from
/// `level_time` when given, else its weapon's player or AI fuse time when the
/// weapon uses timed detonation.
pub(crate) fn reset_detonation_time(
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
    let Some(w) = world.weapon(missile.weapon) else {
        return;
    };
    let Some(def) = WeaponDefView::new(&w.def, &w.variant) else {
        return;
    };
    if def.timed_detonation() {
        let fuse = if world.player(missile.owner).is_some() {
            def.fuse_time()
        } else {
            def.ai_fuse_time()
        };
        state.explode_at = level + fuse;
        state.write(&mut missile.game);
    }
}
