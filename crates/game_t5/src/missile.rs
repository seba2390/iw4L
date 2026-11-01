//! Black Ops' missiles: a projectile weapon's rocket, moved every frame
//! along its trajectory and traced against what it may hit, and its
//! impact: the direct hit, the explosion and its blast.

use fastfile_t5::weapon_def::WeaponDefView;
use game_api::fire::{
    AiEvent, AiLineEvent, AiPointEvent, BulletHitType, BulletTrace, FireWorld, GAME_MISSILE_BYTES,
    GameMissile, GameTrajectory, GuidedInput, MissileExplosion, MissileRun, NotifyArg,
    RadiusDamage, TrajectoryKind,
};
use game_api::movement::{ENTITY_NONE, ENTITY_WORLD};
use game_api::{Unknown, unknown};
use movement_t5::state::weap_flags;

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
    /// What on the entity it rests on it is linked to, and where it sits in
    /// that tag's frame.
    pub(crate) link: LinkTag,
    pub(crate) link_offset: [f32; 3],
    /// How it is turned in that frame, in short angles (65536 a turn).
    pub(crate) link_angles: [i16; 3],
    /// Multiplayer: the sideways push its curvature adds to its velocity
    /// every second.
    pub(crate) curve_push: [f32; 3],
    /// Multiplayer: the entity a guided missile homes on.
    pub(crate) target: Option<i32>,
    /// A remote-guided missile: the boost and attack buttons were let go
    /// since it left, and it is boosting now.
    pub(crate) boost_released: bool,
    pub(crate) attack_released: bool,
    pub(crate) boosting: bool,
    /// When a remote-guided missile last burnt its owner's fuel.
    pub(crate) burnt_at: i32,
}

/// What a grenade is linked to on the entity it struck.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LinkTag {
    #[default]
    None,
    /// The entity's origin and angles.
    Origin,
    /// An actor's right knee, when the bone struck could not be linked.
    KneeRight,
    /// The bone struck, by its number on the entity's model.
    Bone(u16),
}

impl LinkTag {
    fn code(self) -> u16 {
        match self {
            Self::None => 0,
            Self::Origin => 1,
            Self::KneeRight => 2,
            Self::Bone(bone) => bone.saturating_add(3),
        }
    }

    fn from_code(code: u16) -> Self {
        match code {
            0 => Self::None,
            1 => Self::Origin,
            2 => Self::KneeRight,
            code => Self::Bone(code - 3),
        }
    }
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
            link: LinkTag::from_code(u16::from_le_bytes([game[44], game[45]])),
            link_offset: [f(48), f(52), f(56)],
            link_angles: [
                i16::from_le_bytes([game[46], game[47]]),
                i16::from_le_bytes([game[60], game[61]]),
                i16::from_le_bytes([game[62], game[63]]),
            ],
            curve_push: [f(64), f(68), f(72)],
            // Kept one up: 0 is none.
            target: Some(i(76) - 1).filter(|&t| t >= 0),
            boost_released: game[6] & 1 != 0,
            attack_released: game[6] & 2 != 0,
            boosting: game[6] & 4 != 0,
            burnt_at: i(80),
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
        game[44..46].copy_from_slice(&self.link.code().to_le_bytes());
        for (i, c) in self.link_offset.iter().enumerate() {
            game[48 + i * 4..52 + i * 4].copy_from_slice(&c.to_le_bytes());
        }
        let [pitch, yaw, roll] = self.link_angles.map(i16::to_le_bytes);
        game[46..48].copy_from_slice(&pitch);
        game[60..62].copy_from_slice(&yaw);
        game[62..64].copy_from_slice(&roll);
        for (i, c) in self.curve_push.iter().enumerate() {
            game[64 + i * 4..68 + i * 4].copy_from_slice(&c.to_le_bytes());
        }
        game[76..80].copy_from_slice(&self.target.map_or(0, |t| t + 1).to_le_bytes());
        game[6] = u8::from(self.boost_released)
            | u8::from(self.attack_released) << 1
            | u8::from(self.boosting) << 2;
        game[80..84].copy_from_slice(&self.burnt_at.to_le_bytes());
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

/// Black Ops' axis to angles: the forward axis's pitch and yaw, and the
/// roll that turns the left axis, unpitched and unyawed, back level.
pub(crate) fn axis_to_angles(axis: [[f32; 3]; 3]) -> [f32; 3] {
    const DEG2RAD: f32 = 0.017_453_292;
    let mut angles = vectoangles(axis[0]);
    let left = axis[1];
    let (s, c) = (f64::from(-angles[1] * DEG2RAD)).sin_cos();
    let (s, c) = (s as f32, c as f32);
    let y = left[0] * s + left[1] * c;
    let x = left[0] * c - left[1] * s;
    let (s, c) = (f64::from(-angles[0] * DEG2RAD)).sin_cos();
    let (s, c) = (s as f32, c as f32);
    let v = [left[2] * s + x * c, y, left[2] * c - x * s];
    let atan = if v[1] == 0.0 && v[0] == 0.0 {
        if -v[2] >= 0.0 { 90.0 } else { -90.0 }
    } else {
        let flat = (v[1] * v[1] + v[0] * v[0]).sqrt();
        f64::from(v[2]).atan2(f64::from(flat)) as f32 * -57.295_776
    };
    angles[2] = if y >= 0.0 {
        -atan
    } else if atan < 0.0 {
        180.0 + atan
    } else {
        -180.0 + atan
    };
    angles
}

/// The axis (forward, left, up) of `angles`.
pub(crate) fn angles_to_axis(angles: [f32; 3]) -> [[f32; 3]; 3] {
    let (forward, right, up) = movement_t5::math::angle_vectors(angles);
    [forward, right.map(|v| -v), up]
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
    let dir = spread_dir(spread, forward, right, up, world);
    if multiplayer(world) {
        let angles = world.player(attacker).map(|p| p.viewangles);
        spawn_rocket_mp(
            attacker, attacker, weapon, def, muzzle, dir, angles, None, world, gaps,
        );
    } else {
        spawn_rocket(attacker, weapon, def, muzzle, dir, [0.0; 3], world, gaps);
    }
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

/// A projectile shot's direction: off the aim by a random point within the
/// spread's circle at 16 units.
pub(crate) fn spread_dir(
    spread: f32,
    forward: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
    world: &mut dyn FireWorld,
) -> [f32; 3] {
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
    [dir[0] * inv, dir[1] * inv, dir[2] * inv]
}

/// Black Ops multiplayer: `zombiemode` is 0.
pub(crate) fn multiplayer(world: &dyn FireWorld) -> bool {
    dvar(world, "zombiemode", 0.0f32) == 0.0
}

/// BlackOpsMP.exe's missile spawn for a shot: a heat-seeking missile
/// (guided type 3) climbs 0.3 off its aim from 10 units right of its
/// shooter, a remote-guided one (type 6) starts 13 units out; a missile
/// that accelerates, curves or is guided keeps its velocity and is moved
/// by its think (a heat-seeker and the type 4 fall), else it flies straight
/// at the weapon's speed. A curving missile takes a random sideways push of
/// up to its curvature, kept in its state. Drawn once clear of a shooting
/// player; `owner` is the missile's (a SAM Turret's owner for its shot).
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_rocket_mp(
    shooter: i32,
    owner: i32,
    weapon: u32,
    def: &WeaponDefView<'_>,
    start: [f32; 3],
    dir: [f32; 3],
    owner_angles: Option<[f32; 3]>,
    target: Option<i32>,
    world: &mut dyn FireWorld,
    gaps: &mut Vec<&'static Unknown>,
) -> Option<i32> {
    let normalize = |v: [f32; 3]| {
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        let len = if -len >= 0.0 { 1.0 } else { len };
        let inv = 1.0 / len;
        [v[0] * inv, v[1] * inv, v[2] * inv]
    };
    let mut dir = normalize(dir);
    let mut start = start;
    let guided = def.guided_missile_type();
    if guided == 3 {
        dir = normalize([dir[0], dir[1], dir[2] + 0.3]);
        let (_, right, _) = movement_t5::math::angle_vectors(owner_angles.unwrap_or([0.0; 3]));
        start = mad(start, right, 10.0);
    }
    if guided == GUIDED_REMOTE {
        start = mad(start, dir, 13.0);
    }
    if !matches!(guided, 0 | GUIDED_HOMING | GUIDED_REMOTE) {
        gaps.push(missile_unknown());
    }
    let level = world.level_time();
    let speed = def.projectile_speed() as f32;
    let hidden = if world.player(shooter).is_some() {
        ((speed * -35.0 * 0.00166666671 + 85.0) as i32).clamp(20, 50)
    } else {
        0
    };
    let accelerates = def.time_to_accelerate() > 0.0;
    let delta = if accelerates {
        [0.0; 3]
    } else {
        [
            dir[0] * speed,
            speed * dir[1],
            def.projectile_speed_up() as f32 + speed * dir[2],
        ]
    }
    .map(|v| v as i32 as f32);
    let angles = vectoangles(dir);
    let mut curve_push = [0.0; 3];
    let curvature = def.projectile_curvature();
    if curvature > 0.0 {
        let (_, right, up) = movement_t5::math::angle_vectors(angles);
        let (x, y) = random_in_circle(world);
        let (x, y) = (x * curvature, y * curvature);
        curve_push = std::array::from_fn(|i| right[i] * x + up[i] * y);
    }
    let kind = match guided {
        3 | 4 => TrajectoryKind::Gravity(dvar(world, "bg_gravity", 800.0)),
        _ if accelerates || curvature > 0.0 || guided != 0 => TrajectoryKind::Stationary,
        _ => TrajectoryKind::Linear,
    };
    // It explodes when its life runs out, a minute at most.
    let life = ((def.projectile_lifetime() * -1000.0) as i32).wrapping_neg();
    let mut game = [0u8; GAME_MISSILE_BYTES];
    State {
        bounce: def.force_bounce(),
        ground: ENTITY_NONE,
        spawned_at: level,
        explode_at: (level + life).min(level + MISSILE_LIFE_CAP_MS),
        curve_push,
        target,
        burnt_at: level,
        ..State::default()
    }
    .write(&mut game);
    let entity = world.spawn_missile(GameMissile {
        entity: ENTITY_NONE,
        owner,
        weapon,
        position: GameTrajectory {
            kind,
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
    if guided == GUIDED_REMOTE
        && let Some(entity) = entity
    {
        launch_remote(owner, entity, world);
    }
    entity
}

/// A remote-guided missile leaving: its owner's camera may now follow it,
/// they may not cycle weapons, and the fuel of the weapon in hand starts
/// over.
fn launch_remote(owner: i32, entity: i32, world: &mut dyn FireWorld) {
    let Some(mut player) = world.player(owner) else {
        return;
    };
    let (mut ps, mut last) = crate::movement::unpack(&mut player);
    ps.weap_flags = (ps.weap_flags & !weap_flags::FIRING_GUIDED) | weap_flags::CYCLING_DISABLED;
    let held = ps.weapon;
    movement_t5::set_fuel(&mut ps, held, 0);
    crate::movement::pack(&mut player, &mut ps, &mut last);
    world.store_player(owner, &player);
    world.set_guided_missile(owner, Some(entity));
}

/// A remote-guided missile ending: its owner may cycle weapons again and
/// the camera has no missile to follow (their view stays where it is until
/// `unlinkguidedmissilecamera`).
fn release_remote(missile: &GameMissile, world: &mut dyn FireWorld) {
    let Some(mut player) = world.player(missile.owner) else {
        return;
    };
    let (mut ps, mut last) = crate::movement::unpack(&mut player);
    ps.weap_flags &= !weap_flags::CYCLING_DISABLED;
    crate::movement::pack(&mut player, &mut ps, &mut last);
    world.store_player(missile.owner, &player);
    world.set_guided_missile(missile.owner, None);
}

/// A remote-guided missile's fuel: the seconds it may fly are its weapon's
/// `projectileLifetime`, burnt from the owner's weapon in hand, twice as fast
/// while boosting. False when it has burnt out.
fn burn_fuel(
    missile: &GameMissile,
    def: &WeaponDefView<'_>,
    state: &mut State,
    level: i32,
    world: &mut dyn FireWorld,
) -> bool {
    let Some(mut player) = world.player(missile.owner) else {
        return true;
    };
    let (mut ps, mut last) = crate::movement::unpack(&mut player);
    let held = ps.weapon;
    if held == 0 {
        return true;
    }
    let burnt = (level - state.burnt_at) * if state.boosting { 2 } else { 1 };
    movement_t5::add_fuel(&mut ps, held, burnt);
    let life = def.projectile_lifetime() * 1000.0;
    let out = life < movement_t5::fuel(&ps, held) as f32;
    if out {
        movement_t5::set_fuel(&mut ps, held, life as i32);
    } else {
        state.burnt_at = level;
    }
    crate::movement::pack(&mut player, &mut ps, &mut last);
    world.store_player(missile.owner, &player);
    !out
}

/// `value` held within `-limit..=limit`, tested as the game does.
fn cap(value: f32, limit: f32) -> f32 {
    let upper = if value - limit < 0.0 { value } else { limit };
    if -limit - value < 0.0 { upper } else { -limit }
}

/// The guided missile type that homes on its target (the SAM Turret's).
const GUIDED_HOMING: i32 = 1;
/// The guided missile type its owner steers (the Valkyrie Rockets').
const GUIDED_REMOTE: i32 = 6;
/// The longest a missile lives.
const MISSILE_LIFE_CAP_MS: i32 = 60_000;
/// How far ahead a remote-guided missile aims at its steered angles.
const REMOTE_AIM_DISTANCE: f32 = 300.0;

/// BlackOpsMP.exe's step for a missile its think moves (one not flying
/// straight, once drawn, not of the fifth weapon type or guided type 4):
/// it speeds up along its facing to the weapon's speed over
/// `timeToAccelerate`, its curvature pushes it sideways, a homing missile
/// steers for its target, and it moves one frame along its velocity (a
/// heat-seeker falls by its trajectory). A guided missile faces along its
/// velocity once accelerated.
fn step_mp(
    missile: &mut GameMissile,
    def: &WeaponDefView<'_>,
    state: &mut State,
    level: i32,
    world: &mut dyn FireWorld,
) {
    if missile.visible_at >= level
        || missile.position.kind == TrajectoryKind::Linear
        || def.weapon_type() == 5
        || def.guided_missile_type() == 4
    {
        return;
    }
    let frame = 0.05;
    let accelerate = def.time_to_accelerate();
    if accelerate > 0.0 {
        let tr = &mut missile.position;
        let (forward, _, _) = movement_t5::math::angle_vectors(missile.angles.base);
        let along = tr.delta[1] * forward[1] + tr.delta[2] * forward[2] + tr.delta[0] * forward[0];
        let speed = def.projectile_speed() as f32;
        let add = if along < speed {
            speed / accelerate * frame
        } else {
            speed - along
        };
        for (d, f) in tr.delta.iter_mut().zip(forward) {
            *d += add * f;
        }
    }
    if def.projectile_curvature() > 0.0 {
        for i in 0..3 {
            missile.position.delta[i] += state.curve_push[i] * frame;
        }
    }
    let guided = def.guided_missile_type();
    let accelerated = accelerate - (level - missile.position.time) as f32 * 0.00100000005 <= 0.0;
    if guided == GUIDED_HOMING
        && accelerated
        && let Some(target) = state.target.and_then(|t| world.entity_origin(t))
    {
        steer(missile, def, target);
    }
    if guided == 3 {
        return;
    }
    let mut roll = 0.0;
    if guided == GUIDED_REMOTE {
        let input = remote_input(missile, world);
        roll = missile.angles.base[2];
        if accelerated {
            roll = steer_remote(missile, def, input, world);
        }
        let tr = &mut missile.position;
        for i in 0..3 {
            tr.base[i] += tr.delta[i] * frame;
        }
        boost_remote(missile, def, state, input, world);
    }
    let tr = &mut missile.position;
    for i in 0..3 {
        tr.base[i] += tr.delta[i] * frame;
    }
    if guided != 0 && accelerated {
        missile.angles.base = vectoangles(tr.delta);
        if guided == GUIDED_REMOTE {
            missile.angles.base[2] = roll;
        }
    }
}

/// BlackOpsMP.exe's steering of a remote-guided missile, by the owner's look
/// axes: past the deadzone pitch and yaw turn the point it climbs toward by
/// `missileTVGuidedTurnRate` at the axis's end, and yaw also pushes it
/// sideways by up to `maxSteeringAccel` and rolls it toward
/// `missileTVGuidedMaxRollAngle`; pitching alone rolls it back level. Inside
/// the deadzone the axes add to the aim as whole degrees and the roll stays.
/// Returns the roll.
fn steer_remote(
    missile: &mut GameMissile,
    def: &WeaponDefView<'_>,
    input: GuidedInput,
    world: &dyn FireWorld,
) -> f32 {
    const AXIS: f32 = 0.007_936_508;
    let deadzone = dvar(world, "missileTVGuidedStickDeadzone", 15i32);
    let turn_rate = dvar(world, "missileTVGuidedTurnRate", 3.0f32);
    let max_roll = dvar(world, "missileTVGuidedMaxRollAngle", 35.0f32);
    let accel = def.max_steering_accel();
    let [mut pitch, mut yaw, mut roll] = missile.angles.base;
    let (pitch_axis, yaw_axis) = (i32::from(input.pitch), i32::from(input.yaw));
    let past = |axis: i32| axis > deadzone || axis < -deadzone;
    let mut moved = false;
    if past(pitch_axis) {
        pitch += pitch_axis as f32 * AXIS * turn_rate;
        moved = true;
    }
    let (roll_goal, roll_rate, side_accel) = if past(yaw_axis) {
        let stick = yaw_axis as f32 * AXIS;
        yaw += stick * turn_rate;
        moved = true;
        (
            -(max_roll * stick),
            dvar(world, "missileTVGuidedTurningRollAccel", 1.5f32),
            -(accel * stick),
        )
    } else {
        (
            0.0,
            dvar(world, "missileTVGuidedFlatteningRollAccel", 2.0f32),
            0.0,
        )
    };
    let previous_roll = roll;
    if roll > roll_goal {
        roll += (roll_goal - roll) * roll_rate * 0.05;
    } else if roll < roll_goal {
        roll -= (roll - roll_goal) * roll_rate * 0.05;
    }
    if !moved {
        pitch += pitch_axis as f32;
        yaw += yaw_axis as f32;
        // BlackOpsMP.exe keeps the roll it had, floored at the limit, here.
        roll = previous_roll.max(-max_roll);
    }
    let (forward, _, _) = movement_t5::math::angle_vectors([pitch, yaw, roll]);
    let target = mad(missile.origin, forward, REMOTE_AIM_DISTANCE);
    let v = missile.position.delta;
    let speed_xy = (v[0] * v[0] + v[1] * v[1]).sqrt();
    let inverse = 1.0 / if 0.0 <= -speed_xy { 1.0 } else { speed_xy };
    let heading = [v[0] * inverse, v[1] * inverse];
    let side = [heading[1], -heading[0]];
    let at = missile.position.base;
    let (dx, dy, dz) = (target[0] - at[0], target[1] - at[1], target[2] - at[2]);
    let ahead = dy * heading[1] + dx * heading[0];
    let across = dy * side[1] + dx * side[0];
    let push = [
        side_accel * side[0],
        side_accel * side[1],
        climb(ahead, across, dz, speed_xy, v[2], accel),
    ];
    for (d, p) in missile.position.delta.iter_mut().zip(push) {
        *d += p * 0.05;
    }
    roll
}

fn remote_input(missile: &GameMissile, world: &mut dyn FireWorld) -> GuidedInput {
    match world.guided_input(missile.owner) {
        Some(input) => input,
        None => GuidedInput::default(),
    }
}

/// The boost: while the secondary offhand button is held (let go once since
/// launch) a remote-guided missile speeds up to `missileTVGuidedBoost` times
/// over its weapon's speed, else it falls back to its speed.
fn boost_remote(
    missile: &mut GameMissile,
    def: &WeaponDefView<'_>,
    state: &mut State,
    input: GuidedInput,
    world: &dyn FireWorld,
) {
    let (forward, _, _) = movement_t5::math::angle_vectors(missile.angles.base);
    let v = missile.position.delta;
    let along = v[1] * forward[1] + v[2] * forward[2] + v[0] * forward[0];
    let speed = def.projectile_speed() as f32;
    let add = if input.boost && state.boost_released {
        let goal = dvar(world, "missileTVGuidedBoost", 2.0f32) * speed + speed;
        state.boosting = true;
        let up = dvar(world, "missileTVGuidedBoostSpeedUp", 1000i32) as f32 * 0.05;
        if along < goal && up < goal {
            up
        } else if along < goal {
            goal - up
        } else {
            goal - along
        }
    } else {
        if !input.boost {
            state.boost_released = true;
        }
        state.boosting = false;
        if speed <= along {
            dvar(world, "missileTVGuidedBoostSpeedDown", -1000i32) as f32 * 0.05
        } else {
            speed - along
        }
    };
    for (d, f) in missile.position.delta.iter_mut().zip(forward) {
        *d += add * f;
    }
}

/// The attack button detonates a remote-guided missile, once it was let go
/// after launch.
fn attack_detonates(state: &mut State, input: GuidedInput) -> bool {
    if !input.attack {
        state.attack_released = true;
    }
    input.attack && state.attack_released
}

/// `detonate`: the missile explodes where it is now.
pub(crate) fn detonate(
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
    if def.guided_missile_type() == GUIDED_REMOTE {
        release_remote(missile, world);
    }
    state.explode_at = 0;
    let run = crate::grenade::explode(missile, &def, &mut state, world, &mut gaps);
    state.write(&mut missile.game);
    (run, gaps)
}

/// A homing missile's steering, in the horizontal plane by the turn radius
/// that would reach the target (no tighter than its speed allows at
/// `maxSteeringAccel`) and vertically toward the climb that arrives with
/// it; each capped at `maxSteeringAccel`, applied over one frame.
fn steer(missile: &mut GameMissile, def: &WeaponDefView<'_>, target: [f32; 3]) {
    let v = missile.position.delta;
    let speed_xy = (v[0] * v[0] + v[1] * v[1]).sqrt();
    let inverse = 1.0 / if 0.0 <= -speed_xy { 1.0 } else { speed_xy };
    let heading = [v[0] * inverse, v[1] * inverse];
    let side = [heading[1], -heading[0]];
    let at = missile.position.base;
    let (dx, dy, dz) = (target[0] - at[0], target[1] - at[1], target[2] - at[2]);
    let ahead = dy * heading[1] + dx * heading[0];
    let across = dy * side[1] + dx * side[0];
    let accel = def.max_steering_accel();
    let speed = def.projectile_speed() as f32;
    let radius = if across != 0.0 {
        (ahead * ahead + across * across) / (across * 2.0)
    } else {
        f32::MAX
    };
    let tightest = speed / accel * speed;
    let mut push = [0.0f32; 3];
    if ahead <= 0.0 {
        if tightest + 60.0 <= radius.abs() {
            let a = if across <= 0.0 { -accel } else { accel };
            push[0] = side[0] * a;
            push[1] = side[1] * a;
        }
    } else if across != 0.0 {
        let r = if radius.abs() < tightest {
            if radius <= 0.0 { -tightest } else { tightest }
        } else {
            radius
        };
        let a = cap(speed_xy / r * speed_xy * 2.0, accel);
        push[0] = side[0] * a;
        push[1] = side[1] * a;
    }
    push[2] = climb(ahead, across, dz, speed_xy, v[2], accel);
    for (d, p) in missile.position.delta.iter_mut().zip(push) {
        *d += p * 0.05;
    }
}

/// The vertical push that brings a guided missile to its target's height as
/// it arrives, capped at `maxSteeringAccel`.
fn climb(ahead: f32, across: f32, dz: f32, speed_xy: f32, vz: f32, accel: f32) -> f32 {
    let distance_xy = (ahead * ahead + across * across).sqrt();
    if distance_xy == 0.0 || ahead < 0.0 {
        return 0.0;
    }
    let climb = ((ahead / distance_xy * speed_xy).abs() / distance_xy * dz - vz) * 20.0;
    cap(climb, accel)
}

/// `launchbomb`: Black Ops multiplayer's missile spawn with the velocity the
/// script gives. A bullet, projectile, gas or bomb weapon only; the missile
/// falls under `bg_gravity` from `origin`, pointed along the velocity,
/// drawn once clear of a player owner. A missile that accelerates, curves,
/// destabilises or is guided (other than ballistic) is a gap.
pub(crate) fn launch_bomb(
    owner: i32,
    weapon: u32,
    origin: [f32; 3],
    velocity: [f32; 3],
    world: &mut dyn FireWorld,
) -> (Result<Option<i32>, &'static str>, Vec<&'static Unknown>) {
    let Some((def_bytes, variant_bytes)) = world
        .weapon(weapon)
        .map(|w| (w.def.clone(), w.variant.clone()))
    else {
        return (Ok(None), vec![crate::movement::weapon_unknown()]);
    };
    let Some(def) = WeaponDefView::new(&def_bytes, &variant_bytes) else {
        return (Ok(None), vec![crate::movement::weapon_unknown()]);
    };
    if !matches!(def.weapon_type(), 0 | WEAPTYPE_PROJECTILE | 4 | 5) {
        return (
            Err("LaunchBomb only support bullet, bomb, gas and projectile weapons\n"),
            Vec::new(),
        );
    }
    let mut gaps = Vec::new();
    let guided = def.guided_missile_type();
    if def.time_to_accelerate() > 0.0
        || def.projectile_curvature() > 0.0
        || !matches!(guided, 0 | 3)
    {
        gaps.push(missile_unknown());
    }
    let len =
        (velocity[0] * velocity[0] + velocity[1] * velocity[1] + velocity[2] * velocity[2]).sqrt();
    let len = if -len >= 0.0 { 1.0 } else { len };
    let inv = 1.0 / len;
    let dir = [velocity[0] * inv, velocity[1] * inv, velocity[2] * inv];
    let level = world.level_time();
    let hidden = if owner >= 0 && world.player(owner).is_some() {
        let speed = def.projectile_speed() as f32;
        ((speed * -35.0 * 0.00166666671 + 85.0) as i32).clamp(20, 50)
    } else {
        0
    };
    let mut game = [0u8; GAME_MISSILE_BYTES];
    State {
        bounce: def.force_bounce(),
        ground: ENTITY_NONE,
        spawned_at: level,
        ..State::default()
    }
    .write(&mut game);
    let entity = world.spawn_missile(GameMissile {
        entity: ENTITY_NONE,
        owner,
        weapon,
        position: GameTrajectory {
            kind: TrajectoryKind::Gravity(dvar(world, "bg_gravity", 800.0)),
            time: level,
            duration: 0,
            base: origin,
            delta: velocity.map(|v| v as i32 as f32),
        },
        angles: GameTrajectory {
            kind: TrajectoryKind::Stationary,
            time: level,
            duration: 0,
            base: vectoangles(dir),
            delta: [0.0; 3],
        },
        origin,
        visible_at: level + hidden,
        game,
    });
    (Ok(entity), gaps)
}

/// One frame of a missile: along its trajectory, traced against what its
/// mask names (water too once clear of the muzzle), into its impact.
pub(crate) fn run_missile(
    missile: &mut GameMissile,
    world: &mut dyn FireWorld,
) -> (MissileRun, Vec<&'static Unknown>) {
    let (run, gaps) = run_frame(missile, world);
    if run != MissileRun::Flying
        && multiplayer(world)
        && let Some(weapon) = world.weapon(missile.weapon)
        && WeaponDefView::new(&weapon.def, &weapon.variant)
            .is_some_and(|def| def.guided_missile_type() == GUIDED_REMOTE)
    {
        release_remote(missile, world);
    }
    (run, gaps)
}

fn run_frame(
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
    // A grenade linked to what it struck rides it and does not fly.
    if crate::grenade::follow_link(missile, &mut state, world) {
        state.write(&mut missile.game);
        if state.explode_at != 0 && state.explode_at <= level {
            let run = crate::grenade::explode(missile, &def, &mut state, world, &mut gaps);
            state.write(&mut missile.game);
            return (run, gaps);
        }
        return (MissileRun::Flying, gaps);
    }
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
    let multiplayer = multiplayer(world);
    let remote = multiplayer && def.guided_missile_type() == GUIDED_REMOTE;
    if multiplayer {
        step_mp(missile, def, state, level, world);
    }
    if remote && attack_detonates(state, remote_input(missile, world)) {
        return crate::grenade::explode(missile, def, state, world, gaps);
    }
    let new = evaluate(&missile.position, level);
    let d = [new[0] - old[0], new[1] - old[1], new[2] - old[2]];
    let len = (d[2] * d[2] + d[1] * d[1] + d[0] * d[0]).sqrt();
    if len < 0.00100000005 {
        return MissileRun::Flying;
    }
    if remote && !burn_fuel(missile, def, state, level, world) {
        return crate::grenade::explode(missile, def, state, world, gaps);
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
                    bone: trace.bone,
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
