//! Black Ops' server side of a shot: what its client event handler does with
//! a player's fire events. A bullet weapon's shot takes the weapon's spread,
//! fires its pellets along a seeded spread, traces each through what it may
//! penetrate and damages what it hits.

use fastfile_t5::weapon_def::WeaponDefView;
use game_api::fire::{
    ActorMelee, AiEvent, AiLineEvent, AiPointEvent, BulletHitType, BulletImpact, BulletPath,
    BulletTrace, FireDamage, FireEntity, FireEntityKind, FireWorld, NotifyArg, WeaponEvent,
};
use game_api::movement::{ENTITY_NONE, ENTITY_WORLD, MoveSignal};
use game_api::{Unknown, unknown};
use movement_t5::PlayerState;
use movement_t5::events::event as ev;

/// What a bullet stops at: solids, glass, bodies and the like.
const BULLET_MASK: u32 = 0x0280_e833;

mod surf {
    pub const SKY: u32 = 0x4;
    pub const NOIMPACT: u32 = 0x10;
    pub const NOPENETRATE: u32 = 0x100;
    /// The surface type a sentient's body counts as when the trace has none.
    pub const FLESH: u32 = 7 << 20;
    pub const WATER: i32 = 20;
}

/// Damage flags.
mod dflags {
    pub const RADIUS: i32 = 0x1;
    pub const NO_ARMOR: i32 = 0x2;
    /// A bullet's damage after it went through a surface.
    pub const PENETRATED: i32 = 0x8;
}

/// The means of death, by number.
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
const MOD_PISTOL_BULLET: usize = 1;
const MOD_RIFLE_BULLET: usize = 2;
const MOD_MELEE: usize = 7;
const MOD_FALLING: usize = 12;

/// Hit locations, by number.
const HIT_LOCATIONS: [&str; 19] = [
    "none",
    "helmet",
    "head",
    "neck",
    "torso_upper",
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
];

/// `weaponType`: a weapon that fires bullets.
const WEAPTYPE_BULLET: i32 = 0;
/// `weaponType`: a thrown grenade.
const WEAPTYPE_GRENADE: i32 = 1;
/// `weaponType`: a projectile weapon.
const WEAPTYPE_PROJECTILE: i32 = 2;
/// `weaponType`: a mine, thrown like a grenade.
const WEAPTYPE_MINE: i32 = 6;
/// `weaponClass` of a projectile weapon that launches grenades.
const WEAPCLASS_GRENADE_LAUNCHER: i32 = 5;
/// `weaponClass`: a spread weapon, its pellets out to its own range.
const WEAPCLASS_SPREAD: i32 = 3;
/// `fireType`: stacked barrels fired together.
const FIRETYPE_STACKED: i32 = 5;
/// `perks`: the perk that tightens hip spread in zombies (Deadshot).
const PERK_DEADSHOT: u32 = movement_t5::perks::DEADSHOT;
/// `pm_flags` of a player on a turret or in a vehicle; the server fires
/// that, not the player.
const PMF_MOUNTED_FIRE: u32 = 0x4300;
/// `pm_flags` of a player on a turret or in a vehicle, for melee.
const PMF_MOUNTED_MELEE: u32 = 0x300;
/// What a melee blow stops at.
const MELEE_MASK: u32 = 0x0280_e091;
/// A melee blow's traces, by their offsets across the blow's width and
/// height; one trace when the blow has neither.
const MELEE_OFFSETS: [(f32, f32); 9] = [
    (0.0, 0.0),
    (0.5, 0.0),
    (-0.5, 0.0),
    (0.0, 0.5),
    (0.0, -0.5),
    (1.0, 1.0),
    (-1.0, 1.0),
    (1.0, -1.0),
    (-1.0, -1.0),
];
const MOD_BAYONET: usize = 8;
/// The most surfaces a bullet that does not penetrate passes through.
const BULLET_EXTENDED_SEGMENTS: i32 = 12;
/// The eye heights hip spread blends between: prone to crouched, then
/// crouched to standing.
const PRONE_EYE: f32 = 11.0;
const CROUCH_EYE: f32 = 40.0;
pub(crate) const DEG2RAD: f32 = 0.0174532924;

pub(crate) fn fire_unknown() -> &'static Unknown {
    unknown!(
        "t5.weapons.fire",
        "Black Ops' thrown and fired grenades, projectiles, detonation and grenade suicide are not run",
        "Black Ops' server offhand throw, grenade, rocket and detonate rules from its executable"
    )
}

fn mounted_unknown() -> &'static Unknown {
    unknown!(
        "t5.weapons.mounted_fire",
        "a shot from a player in a vehicle is not fired",
        "Black Ops' vehicle fire for players"
    )
}

fn glass_unknown() -> &'static Unknown {
    unknown!(
        "t5.weapons.glass",
        "a bullet does not damage breakable glass with Black Ops' rules",
        "Black Ops' glass pieces and their bullet damage in the runtime"
    )
}

fn impact_explode_unknown() -> &'static Unknown {
    unknown!(
        "t5.weapons.impact_explode",
        "a bullet that explodes on impact does not explode",
        "Black Ops' bullet impact explosion"
    )
}

fn player_hit_unknown() -> &'static Unknown {
    unknown!(
        "t5.weapons.player_hits",
        "a bullet reaching another player neither hits nor passes them with Black Ops' rules",
        "the other player's eye position for Black Ops' hit test and its player damage path"
    )
}

fn achievement_unknown() -> &'static Unknown {
    unknown!(
        "t5.achievements",
        "Black Ops' achievements are not awarded",
        "the achievement awards and where they are reported"
    )
}

/// The fire events this module runs, and the fire events it does not.
pub(crate) fn server_events(
    ps: &PlayerState,
    aim_spread_before: f32,
    gun_angles: [f32; 2],
    signals: &[MoveSignal],
    zombies: bool,
    world: &mut dyn FireWorld,
) -> Vec<&'static Unknown> {
    let mut gaps = Vec::new();
    for signal in signals {
        let &MoveSignal::Event { event, parm } = signal else {
            continue;
        };
        if let Some((weapon, sent)) = weapon_event(ps, event, parm, world) {
            world.weapon_event(ps.client_num, weapon, sent);
        }
        match event {
            ev::FIRE_WEAPON
            | ev::FIRE_WEAPON_LASTSHOT
            | ev::FIRE_WEAPON_LEFT
            | ev::FIRE_WEAPON_LASTSHOT_LEFT => {
                let time = antilag_time(ps.command_time, world);
                let shot = Shot {
                    ps,
                    aim_spread_before,
                    gun_angles,
                    hand: u8::from(matches!(
                        event,
                        ev::FIRE_WEAPON_LEFT | ev::FIRE_WEAPON_LASTSHOT_LEFT
                    )),
                    zombies,
                    gaps: &mut gaps,
                };
                shot.fire_weapon(time, parm, world);
            }
            ev::FIRE_MELEE => {
                let shot = Shot {
                    ps,
                    aim_spread_before,
                    gun_angles,
                    hand: 0,
                    zombies,
                    gaps: &mut gaps,
                };
                shot.melee(world);
            }
            ev::USE_OFFHAND => {
                let shot = Shot {
                    ps,
                    aim_spread_before,
                    gun_angles,
                    hand: 0,
                    zombies,
                    gaps: &mut gaps,
                };
                shot.throw_offhand(parm, world);
            }
            ev::PULLBACK_WEAPON | ev::PREP_OFFHAND => {
                let weapon = u32::try_from(parm).unwrap_or(0);
                world.notify(
                    ps.client_num,
                    "grenade_pullback",
                    &[NotifyArg::Weapon(weapon)],
                );
            }
            // Black Ops' scripts hear a detonation key as the player's notify.
            ev::DETONATE => world.notify(ps.client_num, "detonate", &[]),
            ev::GRENADE_SUICIDE => grenade_suicide(ps, zombies, &mut gaps, world),
            ev::FALL_DAMAGE..=FALL_DAMAGE_LAST => {
                fall_damage(ps.client_num, parm, &mut gaps, world)
            }
            _ => {}
        }
    }
    gaps.sort_by_key(|gap| gap.id);
    gaps.dedup_by_key(|gap| gap.id);
    gaps
}

/// The weapon events of a player's movement every client hears: the ones
/// Black Ops' client event handler plays a weapon sound for.
fn weapon_event(
    ps: &PlayerState,
    event: i32,
    parm: i32,
    world: &dyn FireWorld,
) -> Option<(u32, WeaponEvent)> {
    let mut weapon = ps.weapon;
    let sent = match event {
        ev::FIRE_WEAPON => WeaponEvent::Fire {
            left: false,
            last_shot: false,
        },
        ev::FIRE_WEAPON_LASTSHOT => WeaponEvent::Fire {
            left: false,
            last_shot: true,
        },
        ev::FIRE_WEAPON_LEFT => WeaponEvent::Fire {
            left: true,
            last_shot: false,
        },
        ev::FIRE_WEAPON_LASTSHOT_LEFT => WeaponEvent::Fire {
            left: true,
            last_shot: true,
        },
        ev::NOAMMO | ev::NOAMMO_LEFT => {
            if event == ev::NOAMMO_LEFT {
                weapon = world.weapon(weapon).map_or(0, |w| w.dual_wield_weapon);
            }
            let suppressed = weapon_bytes(world, weapon).is_some_and(|(def, var)| {
                WeaponDefView::new(&def, &var).is_some_and(|d| d.suppress_ammo_reserve_display())
            });
            if suppressed {
                return None;
            }
            WeaponEvent::NoAmmo
        }
        ev::EJECT_BRASS => WeaponEvent::EjectBrass,
        ev::MELEE_SWIPE => {
            let strikes_with_held = weapon_bytes(world, weapon).is_some_and(|(def, var)| {
                WeaponDefView::new(&def, &var).is_some_and(|d| d.bayonet() || d.use_as_melee())
            });
            if !strikes_with_held {
                weapon = ps.melee_weapon;
            }
            WeaponEvent::MeleeSwipe
        }
        ev::PREP_OFFHAND => {
            weapon = u32::try_from(parm).unwrap_or(ps.offhand_index);
            WeaponEvent::Pullback
        }
        ev::RELOAD => WeaponEvent::Reload,
        ev::RELOAD_FROM_EMPTY => WeaponEvent::ReloadFromEmpty,
        ev::RELOAD_START => WeaponEvent::ReloadStart,
        ev::RELOAD_END => WeaponEvent::ReloadEnd,
        ev::RAISE_WEAPON => WeaponEvent::Raise,
        ev::FIRST_RAISE_WEAPON => WeaponEvent::FirstRaise,
        ev::PUTAWAY_WEAPON => WeaponEvent::Putaway,
        ev::WEAPON_ALT => WeaponEvent::Alt,
        ev::PULLBACK_WEAPON => WeaponEvent::Pullback,
        ev::RECHAMBER_WEAPON => WeaponEvent::Rechamber,
        _ => return None,
    };
    Some((weapon, sent))
}

/// The time a shot is fired at: the command's, no more than a second behind
/// the match (`g_antilag`); the match's own time without antilag.
fn antilag_time(command_time: i32, world: &dyn FireWorld) -> i32 {
    let level = world.level_time();
    if !dvar(world, "g_antilag", true) {
        return level;
    }
    if level - command_time > 1000 {
        level - 1000
    } else {
        command_time
    }
}

pub(crate) fn dvar<T: DvarDefault>(world: &dyn FireWorld, name: &str, default: T) -> T {
    world
        .dvar(name)
        .and_then(|value| T::parse(&value))
        .unwrap_or(default)
}

pub(crate) trait DvarDefault: Sized {
    fn parse(value: &str) -> Option<Self>;
}

impl DvarDefault for f32 {
    fn parse(value: &str) -> Option<Self> {
        value.trim().parse().ok()
    }
}

impl DvarDefault for i32 {
    fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        value
            .parse()
            .ok()
            .or_else(|| value.parse::<f32>().ok().map(|f| f as i32))
    }
}

impl DvarDefault for bool {
    fn parse(value: &str) -> Option<Self> {
        <i32 as DvarDefault>::parse(value).map(|n| n != 0)
    }
}

/// The game's spread random: a Park–Miller generator behind a shuffle
/// table the call rebuilds from the negated seed, so the draw is a function
/// of the seed alone; the seed moves on.
fn ran1(seed: &mut i32) -> f64 {
    const AM: f64 = 1.0 / 2_147_483_647.0;
    const RNMX: f64 = 0.99999988;
    fn step(x: i32) -> i32 {
        let k = x / 127_773;
        let r = x
            .wrapping_mul(16_807)
            .wrapping_sub(k.wrapping_mul(0x7fff_ffff));
        if r < 0 {
            r.wrapping_add(0x7fff_ffff)
        } else {
            r
        }
    }
    let mut idum = seed.wrapping_neg();
    let mut table = [0i32; 32];
    for j in (0..40).rev() {
        idum = step(idum);
        if j < 32 {
            table[j] = idum;
        }
    }
    *seed = step(idum);
    let pick = (table[0] / (1 << 26)) as usize;
    let value = f64::from(table[pick]) * AM;
    if RNMX - value >= 0.0 { value } else { RNMX }
}

/// A random point in the unit circle from the seed.
fn random_in_circle(mut seed: i32) -> (f32, f32) {
    let angle = (ran1(&mut seed) * 360.0) as f32;
    let radius = ran1(&mut seed) as f32;
    let radians = angle * DEG2RAD;
    let (sin, cos) = f64::from(radians).sin_cos();
    (cos as f32 * radius, sin as f32 * radius)
}

fn length(v: [f32; 3]) -> f32 {
    (v[2] * v[2] + v[0] * v[0] + v[1] * v[1]).sqrt()
}

fn normalized(v: [f32; 3]) -> [f32; 3] {
    let len = length(v);
    let len = if -len >= 0.0 { 1.0 } else { len };
    let inv = 1.0 / len;
    [v[0] * inv, v[1] * inv, v[2] * inv]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// `v * scale + base`.
fn mad(base: [f32; 3], v: [f32; 3], scale: f32) -> [f32; 3] {
    [
        v[0] * scale + base[0],
        v[1] * scale + base[1],
        v[2] * scale + base[2],
    ]
}

fn distance_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = sub(a, b);
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
}

/// Where a shot starts and the axes it is aimed by.
pub(crate) struct WeaponParms {
    pub(crate) forward: [f32; 3],
    pub(crate) right: [f32; 3],
    pub(crate) up: [f32; 3],
    pub(crate) muzzle: [f32; 3],
}

/// One bullet's state along its path.
#[derive(Clone, Copy)]
struct BulletParams {
    weapon_entity: i32,
    ignore_entity: i32,
    damage_multiplier: f32,
    means: usize,
    origin: [f32; 3],
    start: [f32; 3],
    end: [f32; 3],
    dir: [f32; 3],
    /// Enemies this bullet killed.
    kills: i32,
    /// The pellet of a spread shot, and the hand that fired it.
    pellet: u16,
    hand: u8,
}

/// What a shot or blow does to one entity, before the server's damage
/// rules.
pub(crate) struct Hit {
    pub(crate) target: i32,
    pub(crate) dir: [f32; 3],
    pub(crate) point: [f32; 3],
    pub(crate) amount: i32,
    pub(crate) flags: i32,
    pub(crate) means: usize,
    pub(crate) weapon: u32,
    pub(crate) location: i32,
    pub(crate) bone: Option<u16>,
}

/// One bullet trace and what it hit.
#[derive(Clone, Copy, Default)]
struct BulletResults {
    trace: BulletTrace,
    /// The entity hit; none for the world (except its sky).
    hit_entity: Option<i32>,
    hit_pos: [f32; 3],
    ignore_hit_entity: bool,
    depth_surface_type: i32,
}

impl BulletResults {
    fn surface_type(&self) -> i32 {
        (self.trace.surface_flags >> 20) as i32 & 0x3f
    }
}

struct Shot<'a> {
    ps: &'a PlayerState,
    aim_spread_before: f32,
    gun_angles: [f32; 2],
    /// The hand that fired: 1 the left.
    hand: u8,
    zombies: bool,
    gaps: &'a mut Vec<&'static Unknown>,
}

impl Shot<'_> {
    fn attacker(&self) -> i32 {
        self.ps.client_num
    }

    /// A fired weapon: tells scripts, takes the weapon's spread and fires
    /// the shot its weapon type makes.
    fn fire_weapon(mut self, time: i32, parm: i32, world: &mut dyn FireWorld) {
        if self.ps.pm_flags & PMF_MOUNTED_FIRE != 0 {
            self.gaps.push(mounted_unknown());
            return;
        }
        let weapon = self.ps.weapon;
        world.notify(
            self.attacker(),
            "weapon_fired",
            &[NotifyArg::Weapon(weapon)],
        );
        // The shot's own copy: the world is traced and damaged while the
        // definition is read.
        let Some((def_bytes, variant_bytes)) = world
            .weapon(weapon)
            .map(|w| (w.def.clone(), w.variant.clone()))
        else {
            self.gaps.push(crate::movement::weapon_unknown());
            return;
        };
        let Some(def) = WeaponDefView::new(&def_bytes, &variant_bytes) else {
            self.gaps.push(crate::movement::weapon_unknown());
            return;
        };
        let parms = self.weapon_parms(&def, parm, world);
        let aim_frac = self.aim_spread_before * (1.0 / 255.0f32);
        let (mut min, max) = self.spread(&def, world);
        if self.ps.weapon_pos_frac == 1.0 {
            min = def.ads_spread();
        }
        let spread = (max - min) * aim_frac + min;
        match def.weapon_type() {
            WEAPTYPE_BULLET => self.bullet_fire(&def, weapon, spread, &parms, time, world),
            WEAPTYPE_PROJECTILE if def.weapon_class() != WEAPCLASS_GRENADE_LAUNCHER => {
                crate::missile::fire_rocket(
                    self.attacker(),
                    weapon,
                    &def,
                    spread,
                    parms.forward,
                    parms.right,
                    parms.up,
                    parms.muzzle,
                    world,
                    self.gaps,
                );
            }
            WEAPTYPE_PROJECTILE => crate::grenade::launch_grenade(
                self.attacker(),
                weapon,
                &def,
                parms.forward,
                parms.muzzle,
                world,
                self.gaps,
            ),
            WEAPTYPE_GRENADE | WEAPTYPE_MINE => crate::grenade::fire_grenade(
                self.attacker(),
                weapon,
                &def,
                parms.forward,
                parms.up,
                parms.muzzle,
                world,
                self.gaps,
            ),
            _ => self.gaps.push(fire_unknown()),
        }
    }

    /// The offhand grenade let go: thrown from the eye along the aim, with
    /// no weapon_fired notify.
    fn throw_offhand(mut self, parm: i32, world: &mut dyn FireWorld) {
        let weapon = self.ps.offhand_index;
        let Some((def_bytes, variant_bytes)) = world
            .weapon(weapon)
            .map(|w| (w.def.clone(), w.variant.clone()))
        else {
            self.gaps.push(crate::movement::weapon_unknown());
            return;
        };
        let Some(def) = WeaponDefView::new(&def_bytes, &variant_bytes) else {
            self.gaps.push(crate::movement::weapon_unknown());
            return;
        };
        let parms = self.weapon_parms(&def, parm, world);
        crate::grenade::fire_grenade(
            self.attacker(),
            weapon,
            &def,
            parms.forward,
            parms.up,
            parms.muzzle,
            world,
            self.gaps,
        );
    }

    /// The player's eye and the axes of the aim.
    fn weapon_parms(
        &mut self,
        def: &WeaponDefView<'_>,
        parm: i32,
        world: &dyn FireWorld,
    ) -> WeaponParms {
        let mut angles = self.ps.viewangles;
        if self.ps.pm_flags & 0x4000 == 0 && dvar(world, "player_topDownCamMode", 0) == 0 {
            angles[0] = self.gun_angles[0];
            angles[1] = self.gun_angles[1];
        }
        if def.fire_type() == FIRETYPE_STACKED && self.ps.stack_fire_count > 1 {
            let spread = def.stack_fire_spread();
            let count = self.ps.stack_fire_count;
            let offset = if count == 2 {
                if self.ps.weapon_shot_count == 2 {
                    spread * 0.5
                } else {
                    spread * -0.5
                }
            } else {
                (parm - 1) as f32 * (spread / (count - 1) as f32) - spread * 0.5
            };
            angles[1] += offset;
        }
        let (forward, right, up) = movement_t5::math::angle_vectors(angles);
        let muzzle = mad(
            view_origin(self.ps),
            forward,
            dvar(world, "bg_gunXOffset", 0.0f32),
        );
        WeaponParms {
            forward,
            right,
            up,
            muzzle,
        }
    }

    /// The hip spread at the player's eye height, and what overrides it.
    fn spread(&self, def: &WeaponDefView<'_>, world: &dyn FireWorld) -> (f32, f32) {
        hip_spread_stacked(
            self.ps,
            [
                def.hip_spread_stand_min(),
                def.hip_spread_ducked_min(),
                def.hip_spread_prone_min(),
            ],
            [
                def.hip_spread_max(),
                def.hip_spread_ducked_max(),
                def.hip_spread_prone_max(),
            ],
            (def.fire_type() == FIRETYPE_STACKED).then(|| def.stack_fire_accuracy_decay()),
            self.zombies
                .then(|| dvar(world, "perk_weapSpreadMultiplier", 0.65f32)),
        )
    }

    /// A bullet shot, by the player.
    fn bullet_fire(
        &mut self,
        def: &WeaponDefView<'_>,
        weapon: u32,
        spread: f32,
        parms: &WeaponParms,
        time: i32,
        world: &mut dyn FireWorld,
    ) {
        let attacker = self.attacker();
        Bullets {
            attacker,
            weapon_entity: attacker,
            hand: self.hand,
            zombies: self.zombies,
            gaps: self.gaps,
        }
        .fire(def, weapon, spread, parms, time, world);
    }

    /// A melee blow: the weapon that strikes (a bayonet or a weapon used as
    /// a melee weapon, else the melee weapon), its reach, and its damage
    /// to what its traces reach first.
    fn melee(self, world: &mut dyn FireWorld) {
        if self.ps.pm_flags & PMF_MOUNTED_MELEE != 0 {
            self.gaps.push(mounted_unknown());
            return;
        }
        let held = self.ps.weapon;
        let strikes_with_held = weapon_bytes(world, held).is_some_and(|(def, variant)| {
            WeaponDefView::new(&def, &variant).is_some_and(|d| d.bayonet() || d.use_as_melee())
        });
        let weapon = if strikes_with_held {
            held
        } else {
            self.ps.melee_weapon
        };
        let Some((def_bytes, variant_bytes)) = weapon_bytes(world, weapon) else {
            self.gaps.push(crate::movement::weapon_unknown());
            return;
        };
        let Some(def) = WeaponDefView::new(&def_bytes, &variant_bytes) else {
            self.gaps.push(crate::movement::weapon_unknown());
            return;
        };
        let eye = view_origin(self.ps);
        let (forward, right, up) = movement_t5::math::angle_vectors(self.ps.viewangles);
        let range = if def.bayonet() {
            dvar(world, "player_bayonetRange", 85.0f32)
        } else {
            dvar(world, "player_meleeRange", 64.0f32)
        };
        let width = dvar(world, "player_meleeWidth", 10.0f32);
        let height = dvar(world, "player_meleeHeight", 10.0f32);
        let damage = def.melee_damage();
        let parms = WeaponParms {
            forward,
            right,
            up,
            muzzle: eye,
        };
        let attacker = self.attacker();
        let blow = Blow {
            attacker,
            client: true,
            parms: &parms,
            damage,
            range,
            width,
            height,
        };
        let Some((trace, point)) = melee_trace(&blow, world) else {
            return;
        };
        let means = if def.bayonet() {
            MOD_BAYONET
        } else {
            MOD_MELEE
        };
        if matches!(trace.hit, BulletHitType::Glass(_)) {
            self.gaps.push(glass_unknown());
            return;
        }
        let target = hit_id(&trace);
        let entity = world.entity(target);
        let knife = def.has_gun_model();
        let hit_sentient = entity.is_some_and(sentient);
        if hit_sentient {
            world.weapon_event(attacker, weapon, WeaponEvent::MeleeBlood { point });
            world.weapon_event(attacker, weapon, WeaponEvent::MeleeHit { point, knife });
        } else {
            world.weapon_event(attacker, weapon, WeaponEvent::MeleeMiss { point, knife });
        }
        let Some(entity) = entity.filter(|e| e.takes_damage) else {
            return;
        };
        let amount = damage + world.rand() % 5;
        let dir = [forward[0], forward[1], forward[2] + 0.25];
        let weapon = self.damage_weapon(world);
        self::damage(
            self.gaps,
            attacker,
            attacker,
            entity,
            Hit {
                target,
                dir,
                point,
                amount,
                flags: 0,
                means,
                weapon,
                location: trace.hit_location,
                bone: trace.bone,
            },
            world,
        );
    }

    /// The weapon a player's damage is credited to: the offhand while one
    /// is thrown, the melee weapon in a blow with a weapon that is neither
    /// a bayonet nor used as a melee weapon, else the held weapon.
    fn damage_weapon(&self, world: &dyn FireWorld) -> u32 {
        use movement_t5::weapon::state;
        let ps = self.ps;
        let s = ps.weaponstate;
        if ps.weap_flags & movement_t5::state::weap_flags::OFFHAND_ACTIVE != 0
            && state::OFFHAND_INIT < s
            && s < state::DETONATING
        {
            return ps.offhand_index;
        }
        if matches!(s, state::MELEE_INIT | state::MELEE_FIRE | state::MELEE_END) {
            let plain = weapon_bytes(world, ps.weapon).is_some_and(|(def, variant)| {
                WeaponDefView::new(&def, &variant)
                    .is_some_and(|d| !d.bayonet() && !d.use_as_melee())
            });
            if plain && s != state::MELEE_END {
                return ps.melee_weapon;
            }
        }
        ps.weapon
    }
}

/// A melee blow: who strikes, from where, how hard and how far.
struct Blow<'a> {
    attacker: i32,
    /// A player strikes; an actor otherwise.
    client: bool,
    parms: &'a WeaponParms,
    damage: i32,
    range: f32,
    width: f32,
    height: f32,
}

/// The blow's traces out to its reach, across its width and height, from
/// the eye (a player's from just behind it): a player's first trace that
/// reaches a body, else the nearest that hits anything. Damage triggers on
/// the centre line.
fn melee_trace(blow: &Blow<'_>, world: &mut dyn FireWorld) -> Option<(BulletTrace, [f32; 3])> {
    let parms = blow.parms;
    let count = if blow.width > 0.0 || blow.height > 0.0 {
        9
    } else {
        1
    };
    let start = if blow.client {
        mad(parms.muzzle, parms.forward, -10.0)
    } else {
        parms.muzzle
    };
    let mut best: Option<(BulletTrace, [f32; 3])> = None;
    for &(across, along) in &MELEE_OFFSETS[..count] {
        let mut end = mad(parms.muzzle, parms.forward, blow.range);
        if count > 1 {
            end = mad(end, parms.right, across * blow.width);
            end = mad(end, parms.up, along * blow.height);
        }
        let trace = world.bullet_trace(start, end, blow.attacker, MELEE_MASK);
        let f = trace.fraction;
        let point = [
            (end[0] - start[0]) * f + start[0],
            (end[1] - start[1]) * f + start[1],
            (end[2] - start[2]) * f + start[2],
        ];
        if across == 0.0 && along == 0.0 {
            world.damage_triggers(
                blow.attacker,
                parms.muzzle,
                point,
                blow.damage,
                MEANS[MOD_MELEE],
            );
        }
        if f == 1.0 || trace.surface_flags & surf::NOIMPACT != 0 {
            continue;
        }
        if blow.client && world.entity(hit_id(&trace)).is_some_and(sentient) {
            return Some((trace, point));
        }
        if best.as_ref().is_none_or(|(b, _)| f <= b.fraction) {
            best = Some((trace, point));
        }
    }
    best.filter(|(trace, _)| trace.fraction < 1.0)
}

/// How far an actor that is not a dog reaches; its blow has no width or
/// height.
const ACTOR_MELEE_RANGE: f32 = 64.0;
/// The most an actor in a melee charge strikes for.
const MELEE_CHARGE_DAMAGE: i32 = 80;

/// An actor's melee blow (its `melee` builtin): towards its enemy's eye
/// (turned to the script's direction when it passes one, at the same
/// height), else along the script's direction, else along its weapon's
/// flash tag or its facing. A dog's blow spans the AI melee width and
/// height; any other actor's is one line. Nothing on its own team is
/// struck. Returns what it struck.
pub(crate) fn actor_melee(
    melee: &ActorMelee,
    world: &mut dyn FireWorld,
) -> (i32, Vec<&'static Unknown>) {
    let mut gaps = Vec::new();
    let (facing, right, up) = movement_t5::math::angle_vectors(melee.angles);
    let mut forward = melee.flash_forward.unwrap_or(facing);
    let enemy_eye = melee
        .enemy
        .and_then(|(enemy, eye)| player_eye(world, enemy).or(eye));
    if let Some(at) = enemy_eye {
        let to = sub(at, melee.eye);
        let (x, y) = match melee.direction {
            Some(dir) => {
                let flat = (to[1] * to[1] + to[0] * to[0]).sqrt();
                let scale = flat / (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
                (dir[0] * scale, dir[1] * scale)
            }
            None => (to[0], to[1]),
        };
        forward = normalized_zyx([x, y, to[2]]);
    } else if let Some(dir) = melee.direction {
        forward = normalized_zyx(dir);
    }
    let attacker = world.entity(melee.attacker);
    // Black Ops' dog species (dog and `zombie_dog`).
    let dog = attacker
        .as_ref()
        .is_some_and(|e| matches!(e.ai_type, Some("dog" | "zombie_dog")));
    let (range, width, height) = if dog {
        (
            dvar(world, "ai_meleeRange", 64.0f32),
            dvar(world, "ai_meleeWidth", 20.0f32),
            dvar(world, "ai_meleeHeight", 10.0f32),
        )
    } else {
        (ACTOR_MELEE_RANGE, 0.0, 0.0)
    };
    let mut damage = if dog {
        dvar(world, "dog_MeleeDamage", 150i32)
    } else {
        dvar(world, "ai_meleeDamage", 100i32)
    };
    if melee.in_melee_charge && damage > MELEE_CHARGE_DAMAGE {
        damage = MELEE_CHARGE_DAMAGE;
    }
    let parms = WeaponParms {
        forward,
        right,
        up,
        muzzle: melee.eye,
    };
    let blow = Blow {
        attacker: melee.attacker,
        client: false,
        parms: &parms,
        damage,
        range,
        width,
        height,
    };
    let Some((trace, point)) = melee_trace(&blow, world) else {
        return (ENTITY_NONE, gaps);
    };
    if matches!(trace.hit, BulletHitType::Glass(_)) {
        gaps.push(glass_unknown());
        return (ENTITY_NONE, gaps);
    }
    let target = hit_id(&trace);
    let entity = world.entity(target);
    if let (Some(attacker), Some(hit)) = (&attacker, &entity)
        && sentient(*attacker)
        && sentient(*hit)
        && attacker.team == hit.team
    {
        return (ENTITY_NONE, gaps);
    }
    let Some(entity) = entity.filter(|e| target != ENTITY_WORLD && e.takes_damage) else {
        return (ENTITY_NONE, gaps);
    };
    let bayonet = weapon_bytes(world, melee.weapon).is_some_and(|(def, variant)| {
        WeaponDefView::new(&def, &variant).is_some_and(|d| d.bayonet())
    });
    let amount = damage + world.rand() % 5;
    let dir = [forward[0], forward[1], forward[2] + 0.25];
    self::damage(
        &mut gaps,
        melee.attacker,
        melee.attacker,
        entity,
        Hit {
            target,
            dir,
            point,
            amount,
            flags: 0,
            means: if bayonet { MOD_BAYONET } else { MOD_MELEE },
            weapon: melee.weapon,
            location: trace.hit_location,
            bone: trace.bone,
        },
        world,
    );
    (target, gaps)
}

/// A bullet weapon's shot: by its attacker, from its weapon's entity (the
/// attacker's own, or a turret's).
pub(crate) struct Bullets<'a> {
    pub(crate) attacker: i32,
    pub(crate) weapon_entity: i32,
    /// The hand that fired: 1 the left.
    pub(crate) hand: u8,
    pub(crate) zombies: bool,
    pub(crate) gaps: &'a mut Vec<&'static Unknown>,
}

impl Bullets<'_> {
    fn attacker(&self) -> i32 {
        self.attacker
    }

    /// A bullet shot: each pellet's end from its seed, then its trace.
    pub(crate) fn fire(
        &mut self,
        def: &WeaponDefView<'_>,
        weapon: u32,
        spread: f32,
        parms: &WeaponParms,
        time: i32,
        world: &mut dyn FireWorld,
    ) {
        let (shots, range) = if def.weapon_class() == WEAPCLASS_SPREAD {
            (def.shot_count(), def.min_damage_range())
        } else {
            (1, dvar(world, "bulletrange", 8192.0f32))
        };
        let weapon_entity = self.weapon_entity;
        for shot in 0..shots {
            let (end, dir) = bullet_end(time.wrapping_add(shot), spread, parms, range);
            let mut bp = BulletParams {
                weapon_entity,
                ignore_entity: weapon_entity,
                damage_multiplier: 1.0,
                means: if def.rifle_bullet() {
                    MOD_RIFLE_BULLET
                } else {
                    MOD_PISTOL_BULLET
                },
                origin: parms.muzzle,
                start: parms.muzzle,
                end,
                dir,
                kills: 0,
                pellet: shot as u16,
                hand: self.hand,
            };
            if def.penetrate_type() != 0 && !def.bullet_impact_explode() {
                self.fire_penetrate(def, weapon, &mut bp, world);
            } else {
                self.fire_extended(def, weapon, &mut bp, world);
            }
        }
    }

    /// One trace of the bullet and what it hit; false when it hit
    /// nothing. The results hold the trace either way.
    fn bullet_trace(
        &mut self,
        bp: &BulletParams,
        last_surface_type: i32,
        world: &mut dyn FireWorld,
    ) -> (bool, BulletResults) {
        let mut br = BulletResults {
            trace: world.bullet_trace(bp.start, bp.end, bp.ignore_entity, BULLET_MASK),
            ..BulletResults::default()
        };
        let sky = br.trace.surface_flags & surf::SKY != 0;
        if br.trace.hit == BulletHitType::None && !sky {
            return (false, br);
        }
        let id = hit_id(&br.trace);
        br.hit_entity = (id != ENTITY_WORLD && id != ENTITY_NONE).then_some(id);
        if sky {
            br.hit_entity = Some(id);
        }
        let fraction = br.trace.fraction;
        br.hit_pos = [
            (bp.end[0] - bp.start[0]) * fraction + bp.start[0],
            (bp.end[1] - bp.start[1]) * fraction + bp.start[1],
            (bp.end[2] - bp.start[2]) * fraction + bp.start[2],
        ];
        if let Some(hit) = br.hit_entity {
            let entity = world.entity(hit);
            if entity.is_some_and(sentient) && br.trace.surface_flags == 0 {
                br.trace.surface_flags = surf::FLESH;
            }
            br.ignore_hit_entity = self.ignore_hit_entity(hit, entity);
        }
        br.depth_surface_type = br.surface_type();
        if br.trace.surface_flags & surf::NOPENETRATE != 0 {
            br.depth_surface_type = 0;
            return (true, br);
        }
        if br.depth_surface_type == 0 && last_surface_type != 0 {
            br.depth_surface_type = last_surface_type;
        }
        (true, br)
    }

    /// A bullet passes through its shooter; another player it reaches is
    /// tested against that player's eye, which the match does not give.
    fn ignore_hit_entity(&mut self, hit: i32, entity: Option<FireEntity>) -> bool {
        if hit == self.attacker() {
            return true;
        }
        if entity.is_some_and(|e| e.kind == FireEntityKind::Player) {
            self.gaps.push(player_hit_unknown());
            return true;
        }
        false
    }

    /// A bullet that does not penetrate goes on only
    /// past surfaces without impacts, and a rifle bullet through bodies at
    /// half damage.
    fn fire_extended(
        &mut self,
        def: &WeaponDefView<'_>,
        weapon: u32,
        bp: &mut BulletParams,
        world: &mut dyn FireWorld,
    ) {
        for segment in 0..BULLET_EXTENDED_SEGMENTS {
            let (hit, br) = self.bullet_trace(bp, 0, world);
            if segment == 0 {
                path(weapon, bp, hit, &br, world);
            }
            if !hit {
                return;
            }
            self.bullet_hit(def, weapon, bp, &br, 0, true, world);
            if br.trace.surface_flags & surf::NOIMPACT != 0 {
                if !advance_trace(bp, &br, 0.135) {
                    return;
                }
                continue;
            }
            let Some(entity) = br.hit_entity.and_then(|e| world.entity(e)) else {
                return;
            };
            if !entity.takes_damage {
                return;
            }
            if !br.ignore_hit_entity {
                if !def.rifle_bullet() || !sentient(entity) {
                    return;
                }
                bp.damage_multiplier *= 0.5;
            }
            advance_trace(bp, &br, 0.0);
        }
    }

    /// A penetrating bullet goes through surfaces as deep as
    /// the penetration table lets it, losing damage by their thickness.
    fn fire_penetrate(
        &mut self,
        def: &WeaponDefView<'_>,
        weapon: u32,
        bp: &mut BulletParams,
        world: &mut dyn FireWorld,
    ) {
        let (hit, mut br) = self.bullet_trace(bp, 0, world);
        path(weapon, bp, hit, &br, world);
        if !hit {
            return;
        }
        let mut flags = self.bullet_hit(def, weapon, bp, &br, 0, true, world);
        if def.bullet_impact_explode() {
            return;
        }
        let count = dvar(world, "penetrationCount", 2i32);
        let min_fx = dvar(world, "bullet_penetrationMinFxDist", 30.0f32);
        let depth_of = |world: &dyn FireWorld, surface: i32| {
            if surface == 0 {
                0.0
            } else {
                world
                    .penetration_depth(def.penetrate_type(), surface)
                    .unwrap_or(0.0)
            }
        };
        for _ in 0..count {
            let mut max_depth = depth_of(world, br.depth_surface_type);
            if br.trace.surface_flags & surf::SKY != 0 {
                max_depth = 100.0;
            }
            if self.zombies && br.surface_type() == surf::WATER {
                max_depth = 100.0;
            } else if max_depth <= 0.0 {
                return;
            }
            let last_hit_pos = br.hit_pos;
            if !advance_trace(bp, &br, 0.135) {
                return;
            }
            let hit;
            (hit, br) = self.bullet_trace(bp, br.depth_surface_type, world);
            if let Some(entity) = br.hit_entity.and_then(|e| world.entity(e))
                && entity.kind == FireEntityKind::Player
            {
                return;
            }
            // The way back: from the next hit (or the bullet's end) to just
            // past where it went in.
            let mut rev_bp = *bp;
            rev_bp.dir = [-bp.dir[0], -bp.dir[1], -bp.dir[2]];
            rev_bp.start = bp.end;
            rev_bp.end = mad(last_hit_pos, rev_bp.dir, 0.01);
            let mut rev_br = br;
            rev_br.trace.normal = [
                -br.trace.normal[0],
                -br.trace.normal[1],
                -br.trace.normal[2],
            ];
            if hit {
                advance_trace(&mut rev_bp, &rev_br, 0.01);
            }
            let rev_hit;
            (rev_hit, rev_br) = self.bullet_trace(&rev_bp, rev_br.depth_surface_type, world);
            let all_solid = (rev_hit && rev_br.trace.all_solid)
                || (br.trace.start_solid && rev_br.trace.start_solid);
            if rev_br.trace.surface_flags & surf::SKY != 0 {
                rev_br.hit_entity = Some(hit_id(&rev_br.trace));
            }
            if !rev_hit && !all_solid {
                if !hit {
                    return;
                }
                let impact = distance_sq(br.hit_pos, last_hit_pos) > min_fx * min_fx;
                flags = self.bullet_hit(def, weapon, bp, &br, dflags::PENETRATED, impact, world);
                continue;
            }
            let span = if all_solid {
                sub(rev_bp.end, rev_bp.start)
            } else {
                sub(last_hit_pos, rev_br.hit_pos)
            };
            let thickness = length_zyx(span).max(1.0);
            let depth = if rev_hit {
                let rev_depth = depth_of(world, rev_br.depth_surface_type);
                let depth = if rev_depth - max_depth < 0.0 {
                    rev_depth
                } else {
                    max_depth
                };
                if rev_br.trace.surface_flags & surf::SKY != 0 {
                    100.0
                } else if depth <= 0.0 {
                    return;
                } else {
                    depth
                }
            } else {
                max_depth
            };
            bp.damage_multiplier -= thickness / depth;
            if bp.damage_multiplier <= 0.0 {
                return;
            }
            if !all_solid {
                let impact = distance_sq(rev_br.hit_pos, br.hit_pos) > min_fx * min_fx;
                if impact && (!hit || br.trace.surface_flags & surf::SKY == 0) {
                    self.impact(weapon, &rev_bp, &rev_br, flags | 0x4, true, bp.dir, world);
                }
                if !hit {
                    return;
                }
                flags = self.bullet_hit(def, weapon, bp, &br, dflags::PENETRATED, impact, world);
            }
            if !hit {
                return;
            }
        }
    }

    /// A bullet's hit: its damage at its range, damage triggers on its
    /// path, its impact and the damage to the entity it hit.
    /// Returns the impact's flags: 1 a head was hit, 2 the hit killed.
    #[allow(clippy::too_many_arguments)]
    fn bullet_hit(
        &mut self,
        def: &WeaponDefView<'_>,
        weapon: u32,
        bp: &mut BulletParams,
        br: &BulletResults,
        mut damage_flags: i32,
        impact: bool,
        world: &mut dyn FireWorld,
    ) -> u8 {
        let attacker = self.attacker();
        if world.entity(self.attacker).is_some_and(sentient) {
            if bp.damage_multiplier == 1.0 {
                world.ai_event(AiEvent::Point {
                    originator: self.attacker,
                    kind: if def.silenced() {
                        AiPointEvent::SilencedShot
                    } else {
                        AiPointEvent::Gunshot
                    },
                    teams: None,
                    origin: bp.origin,
                });
            }
            let d = sub(br.hit_pos, bp.origin);
            let len = (d[2] * d[2] + d[0] * d[0] + d[1] * d[1]).sqrt();
            let len = if -len >= 0.0 { 1.0 } else { len };
            let inv = 1.0 / len;
            let end = [
                inv * d[0] * 200.0 + br.hit_pos[0],
                inv * d[1] * 200.0 + br.hit_pos[1],
                d[2] * inv * 200.0 + br.hit_pos[2],
            ];
            let friendly = world
                .dvar("ai_friendlySuppression")
                .is_none_or(|v| v.trim() != "0");
            world.ai_event(AiEvent::Line {
                originator: self.attacker,
                hit: br.hit_entity.unwrap_or(ENTITY_NONE),
                kind: AiLineEvent::Bullet,
                teams: friendly.then_some(0xe),
                start: bp.origin,
                end,
            });
        }
        let damage = bullet_damage(def, bp, br);
        world.damage_triggers(attacker, bp.start, br.hit_pos, damage, MEANS[bp.means]);
        let mut out_flags = 0u8;
        if def.bullet_impact_explode() && def.explosion_inner_damage() != 0 {
            self.gaps.push(impact_explode_unknown());
        }
        if let Some(hit) = br.hit_entity
            && let Some(entity) = world.entity(hit)
            && entity.takes_damage
            && !br.ignore_hit_entity
        {
            if def.armor_piercing() {
                damage_flags |= dflags::NO_ARMOR;
            }
            let was_alive = entity.health > 0;
            let location = br.trace.hit_location;
            if location == 1 || location == 2 {
                out_flags |= 1;
            }
            let attacker = self.attacker();
            self::damage(
                self.gaps,
                attacker,
                attacker,
                entity,
                Hit {
                    target: hit,
                    dir: bp.dir,
                    point: br.hit_pos,
                    amount: damage,
                    flags: damage_flags,
                    means: bp.means,
                    weapon,
                    location,
                    bone: br.trace.bone,
                },
                world,
            );
            let alive = world.entity(hit).is_some_and(|e| e.health > 0);
            if was_alive && !alive {
                out_flags |= 2;
                let attacker_team = world.entity(attacker).and_then(|e| e.team);
                let victim_team = (entity.kind == FireEntityKind::Actor)
                    .then_some(entity.team)
                    .flatten();
                if let (Some(a), Some(v)) = (attacker_team, victim_team)
                    && a != v
                {
                    bp.kills += 1;
                    if bp.kills == 3 {
                        self.gaps.push(achievement_unknown());
                    }
                    if bp.kills >= dvar(world, "bullet_multikill_shots", 2i32) {
                        world.notify(attacker, "multikill", &[]);
                    }
                }
            }
        }
        if matches!(br.trace.hit, BulletHitType::Glass(_)) {
            self.gaps.push(glass_unknown());
        }
        if impact {
            self.impact(weapon, bp, br, out_flags, false, br.trace.normal, world);
        }
        out_flags
    }

    /// The bullet's impact event, where Black Ops makes one.
    #[allow(clippy::too_many_arguments)]
    fn impact(
        &self,
        weapon: u32,
        bp: &BulletParams,
        br: &BulletResults,
        flags: u8,
        exit: bool,
        normal: [f32; 3],
        world: &mut dyn FireWorld,
    ) {
        if br.ignore_hit_entity {
            return;
        }
        if br.trace.surface_flags & surf::SKY != 0 || br.trace.fraction >= 1.0 {
            return;
        }
        let entity = br.hit_entity.and_then(|e| world.entity(e));
        let player = entity.is_some_and(|e| e.kind == FireEntityKind::Player);
        let actor = entity.is_some_and(|e| e.kind == FireEntityKind::Actor);
        let client_side = dvar(world, "sv_clientSideBullets", true);
        world.impact(&BulletImpact {
            attacker: bp.weapon_entity,
            weapon,
            start: bp.start,
            point: br.hit_pos,
            normal,
            surface_type: br.surface_type() as u8,
            surface_flags: br.trace.surface_flags,
            entity: br.hit_entity.unwrap_or(ENTITY_NONE),
            flags,
            exit,
            from_server: player || actor || !client_side,
        });
    }
}

/// A bullet's first trace goes to the client as the path it draws the
/// bullet's beam along: Black Ops' client traces each bullet itself
/// (`sv_clientSideBullets`) and spawns its beam from that first trace, to
/// where it hit or, missing, to its end.
fn path(weapon: u32, bp: &BulletParams, hit: bool, br: &BulletResults, world: &mut dyn FireWorld) {
    world.bullet_path(&BulletPath {
        shooter: bp.weapon_entity,
        weapon,
        pellet: bp.pellet,
        hand: bp.hand,
        start: bp.start,
        end: if hit { br.hit_pos } else { bp.end },
    });
}

/// A player held a cooked grenade past its fuse. With grenade suicide on,
/// Black Ops kills the player at once. With it off, the grenade drops at the
/// player's feet on what is left of its fuse, and the player takes a killing
/// blow anyway unless in zombies with PhD Flopper.
fn grenade_suicide(
    ps: &PlayerState,
    zombies: bool,
    gaps: &mut Vec<&'static Unknown>,
    world: &mut dyn FireWorld,
) {
    let client = ps.client_num;
    let Some(entity) = world.entity(client) else {
        return;
    };
    if entity.kind != FireEntityKind::Player {
        return;
    }
    if world.grenade_suicide_disabled() {
        let weapon = ps.offhand_index;
        if let Some((def_bytes, variant_bytes)) = world
            .weapon(weapon)
            .map(|w| (w.def.clone(), w.variant.clone()))
            && let Some(def) = WeaponDefView::new(&def_bytes, &variant_bytes)
        {
            crate::grenade::drop_grenade(
                client,
                weapon,
                &def,
                ps.origin,
                ps.grenade_time_left,
                world,
                gaps,
            );
        } else {
            gaps.push(crate::movement::weapon_unknown());
        }
        if zombies && ps.perks & PERK_FLAKJACKET != 0 {
            return;
        }
    }
    damage(
        gaps,
        client,
        client,
        entity,
        Hit {
            target: client,
            dir: [0.0; 3],
            point: [0.0; 3],
            amount: SUICIDE_DAMAGE,
            flags: 0,
            means: MOD_SUICIDE,
            weapon: ps.offhand_index,
            location: 0,
            bone: None,
        },
        world,
    );
}

/// The damage grenade suicide deals, and its means of death.
const SUICIDE_DAMAGE: i32 = 100_000;
const MOD_SUICIDE: usize = 13;
/// PhD Flopper's perk bits (`specialty_flakjacket`).
const PERK_FLAKJACKET: u32 = 0x3000;

/// The server's damage to what a shot, blow or blast reaches: a player's
/// own multipliers, an actor's hit location multipliers, then the entity's
/// damage callback.
pub(crate) fn damage(
    gaps: &mut Vec<&'static Unknown>,
    attacker: i32,
    inflictor: i32,
    entity: FireEntity,
    hit: Hit,
    world: &mut dyn FireWorld,
) {
    let Hit {
        target,
        dir,
        point,
        mut amount,
        flags,
        means,
        weapon,
        location,
        bone,
    } = hit;
    if entity.kind == FireEntityKind::Player {
        if target != attacker
            && world
                .entity(attacker)
                .is_some_and(|e| e.kind == FireEntityKind::Player)
        {
            gaps.push(player_hit_unknown());
            return;
        }
        let Some(player) = world.player(target) else {
            return;
        };
        let (ps, _) = crate::movement::unpack(&mut player.clone());
        if ps.other_flags & 1 != 0 || ps.pm_type == movement_t5::state::pm_type::DEAD {
            return;
        }
        if means != MOD_FALLING {
            let scale = if flags & dflags::RADIUS != 0 {
                dvar(world, "player_radiusDamageMultiplier", 1.0f32)
            } else if means == MOD_MELEE {
                dvar(world, "player_meleeDamageMultiplier", 1.0f32)
            } else {
                dvar(world, "player_damageMultiplier", 1.0f32)
            };
            amount = (amount as f32 * scale) as i32;
        }
        amount = amount.max(1);
    }
    let attacker_team = world.entity(attacker).and_then(|e| e.team);
    if entity.kind == FireEntityKind::Actor
        && dvar_string(world, "g_gametype").as_deref() == Some("vs")
        && entity.team.is_some()
        && entity.team == attacker_team
    {
        return;
    }
    if entity.kind == FireEntityKind::Actor
        && !entity.ignore_locational_damage
        && means != MOD_MELEE
    {
        let scale = location_multiplier(location, weapon, world);
        // Black Ops caps it for dogs and hellhounds (species 1 and 3), not zombies.
        let capped = matches!(entity.ai_type, Some("dog" | "zombie_dog"));
        let scale = if capped && scale > 1.0 {
            1.0
        } else if scale == 0.0 {
            return;
        } else {
            scale
        };
        amount = (amount as f32 * scale) as i32;
        if amount < 1 {
            amount = 1;
        }
    }
    if entity.god {
        return;
    }
    amount = amount.max(1);
    // Delayed death: an actor that may not die, in a scripted animation or a
    // traversal, keeps one point of health; the rest is told
    // `damage_notdone`.
    if entity.kind == FireEntityKind::Actor
        && amount >= entity.health
        && !entity.allow_death
        && entity.delays_death
    {
        let absorbed = amount - entity.health + 1;
        amount -= absorbed;
        world.set_delayed_death(target);
        world.notify(
            target,
            "damage_notdone",
            &[
                NotifyArg::Int(absorbed),
                NotifyArg::Entity(attacker),
                NotifyArg::Vector(normalized(dir)),
                NotifyArg::Vector(point),
                NotifyArg::String(MEANS[means]),
                NotifyArg::String(""),
                NotifyArg::String(""),
                NotifyArg::String(
                    HIT_LOCATIONS
                        .get(location as usize)
                        .copied()
                        .unwrap_or("none"),
                ),
                NotifyArg::Int(flags),
            ],
        );
    }
    if flags & dflags::RADIUS == 0 && entity.demigod && entity.health <= amount {
        amount = entity.health - 1;
    }
    if amount == 0 {
        return;
    }
    world.damage(&FireDamage {
        target,
        inflictor,
        attacker,
        dir: normalized(dir),
        point,
        amount,
        flags,
        means: MEANS[means],
        weapon,
        hit_location: HIT_LOCATIONS
            .get(location as usize)
            .copied()
            .unwrap_or("none"),
        location,
        bone,
        time_offset: 0,
    });
}

/// The last fall damage event: one per surface type.
const FALL_DAMAGE_LAST: i32 = ev::FALL_DAMAGE + 30;
/// The share of health a fall of 100% or more takes.
const FALL_DAMAGE_FULL: f32 = 1.1;

/// A player's fall: the event's percent of its maximum health (110% from
/// 100 up), no attacker.
fn fall_damage(
    client: i32,
    percent: i32,
    gaps: &mut Vec<&'static Unknown>,
    world: &mut dyn FireWorld,
) {
    let Some(entity) = world.entity(client) else {
        return;
    };
    if entity.kind != FireEntityKind::Player {
        return;
    }
    let share = if percent >= 100 {
        FALL_DAMAGE_FULL
    } else {
        percent as f32 * 0.01
    };
    if share == 0.0 {
        return;
    }
    let amount = (entity.max_health as f32 * share) as i32;
    damage(
        gaps,
        ENTITY_WORLD,
        ENTITY_WORLD,
        entity,
        Hit {
            target: client,
            dir: [0.0; 3],
            point: [0.0; 3],
            amount,
            flags: 0,
            means: MOD_FALLING,
            weapon: 0,
            location: 0,
            bone: None,
        },
        world,
    );
}

/// A means of death's number, by its script name.
pub(crate) fn means_index(name: &str) -> usize {
    MEANS.iter().position(|m| *m == name).unwrap_or(0)
}

/// A player's eye: origin, eye height and lean.
pub(crate) fn view_origin(ps: &PlayerState) -> [f32; 3] {
    let mut eye = ps.origin;
    eye[2] += ps.view_height_current;
    if ps.leanf != 0.0 {
        let frac = (2.0 - ps.leanf.abs()) * ps.leanf;
        let roll = frac * 16.0;
        let (_, right, _) = movement_t5::math::angle_vectors([0.0, ps.viewangles[1], roll]);
        eye = mad(eye, right, frac * 20.0);
    }
    eye
}

/// Another player's eye, from its movement state.
pub(crate) fn player_eye(world: &dyn FireWorld, entity: i32) -> Option<[f32; 3]> {
    let player = world.player(entity)?;
    let (ps, _) = crate::movement::unpack(&mut player.clone());
    Some(view_origin(&ps))
}

/// A weapon's definition and variant bytes, copied out of the match.
fn weapon_bytes(world: &dyn FireWorld, weapon: u32) -> Option<(Box<[u8]>, Box<[u8]>)> {
    world
        .weapon(weapon)
        .map(|w| (w.def.clone(), w.variant.clone()))
}

fn dvar_string(world: &dyn FireWorld, name: &str) -> Option<String> {
    world.dvar(name)
}

pub(crate) fn sentient(entity: FireEntity) -> bool {
    matches!(entity.kind, FireEntityKind::Player | FireEntityKind::Actor)
}

fn length_zyx(v: [f32; 3]) -> f32 {
    (v[2] * v[2] + v[1] * v[1] + v[0] * v[0]).sqrt()
}

/// `v` normalized, its length summed z, y, x.
fn normalized_zyx(v: [f32; 3]) -> [f32; 3] {
    let len = length_zyx(v);
    let len = if -len >= 0.0 { 1.0 } else { len };
    let inv = 1.0 / len;
    [v[0] * inv, v[1] * inv, v[2] * inv]
}

/// The entity number a trace hit: the world for glass, none for nothing.
pub(crate) fn hit_id(trace: &BulletTrace) -> i32 {
    match trace.hit {
        BulletHitType::Entity(n) => n,
        BulletHitType::Glass(_) => ENTITY_WORLD,
        BulletHitType::None => ENTITY_NONE,
    }
}

/// The damage multiplier at a hit location: a bullet weapon's own table,
/// else the game's.
fn location_multiplier(location: i32, weapon: u32, world: &dyn FireWorld) -> f32 {
    let at = usize::try_from(location).unwrap_or(0);
    if weapon != 0
        && let Some(w) = world.weapon(weapon)
        && WeaponDefView::new(&w.def, &w.variant)
            .is_some_and(|def| def.weapon_type() == WEAPTYPE_BULLET)
    {
        return w
            .location_damage
            .as_deref()
            .and_then(|table| table.get(at).copied())
            .unwrap_or(1.0);
    }
    world.hit_location_multiplier(location).unwrap_or(1.0)
}

/// A bullet's damage: the weapon's damage falling off from its far end of
/// full damage to its near end of least damage, scaled by what the bullet
/// still carries.
fn bullet_damage(def: &WeaponDefView<'_>, bp: &BulletParams, br: &BulletResults) -> i32 {
    let base = def.damage();
    let least = def.min_damage();
    let near = def.max_damage_range();
    let far = def.min_damage_range();
    let span = far - near;
    let mut out = base;
    if base != least && span != 0.0 {
        let d = sub(br.hit_pos, bp.origin);
        let dist = (d[1] * d[1] + d[0] * d[0] + d[2] * d[2]).sqrt();
        if near <= dist {
            out = least;
            if dist < far {
                let frac = (dist - near) / span;
                let blended = ((1.0 - frac) * base as f32 + least as f32 * frac) as i32;
                return (blended as f32 * bp.damage_multiplier) as i32;
            }
        }
    }
    (out as f32 * bp.damage_multiplier) as i32
}

/// The next trace starts where this one hit, moved on
/// through world surfaces by `dist` along the surface's normal.
fn advance_trace(bp: &mut BulletParams, br: &BulletResults, dist: f32) -> bool {
    let id = hit_id(&br.trace);
    bp.ignore_entity = id;
    if id == ENTITY_WORLD && dist > 0.0 {
        let n = br.trace.normal;
        let dot = -(bp.dir[2] * n[2] + bp.dir[1] * n[1] + bp.dir[0] * n[0]);
        if dot < 0.125 {
            bp.start = mad(br.hit_pos, bp.dir, dist * 8.0);
            return false;
        }
        bp.start = mad(br.hit_pos, bp.dir, dist / dot);
        return true;
    }
    bp.start = br.hit_pos;
    true
}

/// A pellet's end and direction: its seeded point in the spread circle at
/// the end of its range.
fn bullet_end(seed: i32, spread: f32, parms: &WeaponParms, range: f32) -> ([f32; 3], [f32; 3]) {
    let aim = libm_tan(spread * DEG2RAD) * range;
    let (x, y) = random_in_circle(seed);
    let mut end = mad(parms.muzzle, parms.forward, range);
    end = mad(end, parms.right, x * aim);
    end = mad(end, parms.up, y * aim);
    (end, normalized(sub(end, parms.muzzle)))
}

fn libm_tan(x: f32) -> f32 {
    f64::from(x).tan() as f32
}

/// The hip spread at the player's eye height (stand, ducked and prone
/// minimum and maximum), and what overrides it; in zombies Deadshot scales
/// it by `deadshot_scale` while the sights are not fully up.
pub(crate) fn hip_spread(
    ps: &PlayerState,
    min: [f32; 3],
    max: [f32; 3],
    deadshot_scale: Option<f32>,
) -> (f32, f32) {
    hip_spread_stacked(ps, min, max, None, deadshot_scale)
}

fn hip_spread_stacked(
    ps: &PlayerState,
    min: [f32; 3],
    max: [f32; 3],
    stack_fire_decay: Option<f32>,
    deadshot_scale: Option<f32>,
) -> (f32, f32) {
    let (mut lo, mut hi);
    if ps.spread_override_state == 2 {
        lo = ps.spread_override as f32;
        hi = ps.spread_override as f32;
    } else {
        let height = ps.view_height_current;
        if height <= CROUCH_EYE {
            let frac = (height - PRONE_EYE) * 0.0344827585;
            lo = (min[1] - min[2]) * frac + min[2];
            hi = (max[1] - max[2]) * frac + max[2];
        } else {
            let frac = (height - CROUCH_EYE) * 0.0500000007;
            lo = (min[0] - min[1]) * frac + min[1];
            hi = (max[0] - max[1]) * frac + max[1];
        }
    }
    if let Some(decay) = stack_fire_decay
        && ps.stack_fire_count > 1
    {
        let scale = f64::from(decay).powf(f64::from(ps.stack_fire_count - 1));
        lo = (f64::from(lo) * scale) as f32;
        hi = (f64::from(hi) * scale) as f32;
    }
    if ps.spread_override_state == 1 {
        hi = ps.spread_override as f32;
    }
    if let Some(scale) = deadshot_scale
        && ps.perks & PERK_DEADSHOT != 0
        && ps.weapon_pos_frac < 1.0
    {
        lo *= scale;
        hi *= scale;
    }
    (lo, hi)
}
