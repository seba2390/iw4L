//! Black Ops 2's server side of a player's weapon events: what its client
//! event handler does with them. A fired bullet weapon takes the weapon's
//! spread, lays its pellets along a seeded spread, traces each through what
//! it may penetrate and damages what it hits; grenades are thrown or
//! launched, rockets fired and melee blows struck.

use fastfile_t6::weapon::{WeaponDefView, def, inventory_type, variant, weap_class};
use game_api::fire::{
    BulletHitType, BulletImpact, BulletTrace, FireDamage, FireEntity, FireEntityKind, FireWorld,
    NotifyArg, RadiusDamage, WeaponEvent,
};
use game_api::movement::{ENTITY_NONE, ENTITY_WORLD, MoveSignal};
use game_api::{Unknown, unknown};
use movement_t6::events::event as ev;
use movement_t6::state::{
    ENTITYNUM_NONE, e_flags, e_flags2, pm_flags, pm_type, weap_flags, weapon_state as ws,
};
use movement_t6::weapon::{fire_type, weapon_type};
use movement_t6::{PlayerState, perks};

use crate::missile::{DEG2RAD, HIT_LOCATIONS, MEANS, angle_vectors, mad};

/// What a bullet stops at.
const BULLET_MASK: u32 = 0x0280_e833;
/// What a melee blow stops at.
const MELEE_MASK: u32 = 0x0280_e893;

mod surf {
    pub const SKY: u32 = 0x4;
    pub const NOIMPACT: u32 = 0x10;
    pub const NOPENETRATE: u32 = 0x100;
    /// The surface a player's or actor's body counts as when the trace has none.
    pub const FLESH: u32 = 7 << 20;
    pub const RIOTSHIELD: u32 = 31 << 20;
    pub const WATER: i32 = 20;
}

/// Damage flags.
mod dflags {
    pub const RADIUS: i32 = 0x1;
    pub const NO_ARMOR: i32 = 0x2;
    /// A bullet's damage after it went through a surface.
    pub const PENETRATED: i32 = 0x8;
}

/// The impact flag of the side a bullet leaves a surface by.
const IMPACT_EXIT: u8 = 0x4;

const MOD_PISTOL_BULLET: usize = 1;
const MOD_RIFLE_BULLET: usize = 2;
const MOD_PROJECTILE_SPLASH: usize = 6;
const MOD_MELEE: usize = 7;
const MOD_BAYONET: usize = 8;
const MOD_FALLING: usize = 12;
const MOD_SUICIDE: usize = 13;

const HITLOC_RIOTSHIELD: i32 = 20;

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
/// How far behind the eye a melee blow's traces start.
const MELEE_BACKSTEP: f32 = 10.0;
/// The most surfaces a bullet that does not penetrate passes through.
const BULLET_EXTENDED_SEGMENTS: i32 = 12;
/// The surfaces a charge shot's bullet goes through.
const CHARGE_SHOT_PENETRATIONS: i32 = 5;
/// How far a bullet steps on through what it hits.
const PENETRATE_STEP: f32 = 0.135;
const EXIT_STEP: f32 = 0.01;
/// The depth of what a bullet always goes through (sky, water).
const OPEN_DEPTH: f32 = 100.0;
/// The eye heights hip spread blends between: prone to crouched, then
/// crouched to standing.
const PRONE_EYE: f32 = 11.0;
const CROUCH_EYE: f32 = 40.0;
const PRONE_TO_CROUCH: f32 = 0.034_482_758;
/// A command's antilag time is no more than this far behind the match.
const ANTILAG_MS: i32 = 1000;
/// A dropped grenade's speed along each axis, at most.
const DROP_SPEED: f32 = 160.0;
const DROP_HEIGHT: f32 = 40.0;
/// The damage of a grenade gone off in its thrower's hand.
const SUICIDE_DAMAGE: i32 = 100_000;
/// The eye is never lower than this above the feet.
const EYE_FLOOR: f32 = 8.0;
/// A melee blow's push lifts its direction by this.
const MELEE_LIFT: f32 = 0.25;
/// `waterlevel` from which a player swims.
const SWIM_WATER_LEVEL: i32 = 4;

fn gun_angles_unknown() -> &'static Unknown {
    unknown!(
        "t6.weapons.gun_angles",
        "Black Ops 2 aims a shot along the gun angles its client sends with each command; the runtime's client sends none, so shots go along the view the previous command left",
        "the client's gun angle computation and the command fields that carry it"
    )
}

fn mounted_unknown() -> &'static Unknown {
    unknown!(
        "t6.weapons.mounted_fire",
        "a shot or melee blow from a player on a turret, in a vehicle or linked to an entity's view is not fired",
        "Black Ops 2's turret, vehicle and linked-view fire for players and the entity tags they fire from"
    )
}

fn antilag_unknown() -> &'static Unknown {
    unknown!(
        "t6.weapons.antilag",
        "a Black Ops 2 shot or blow is traced against players and actors where the runtime's lag compensation puts them, not where Black Ops 2 moves them back to the command's time, and its damage carries no time offset",
        "the runtime's player and actor positions at a given match time and the damage time offset"
    )
}

fn hit_priority_unknown() -> &'static Unknown {
    unknown!(
        "t6.weapons.hit_priority",
        "which body part a bullet or blow hits where parts overlap is the runtime's choice, not Black Ops 2's rifle or pistol part priority",
        "Black Ops 2's hit location priority lists in the runtime's locational trace"
    )
}

fn ai_bullet_events_unknown() -> &'static Unknown {
    unknown!(
        "t6.ai.bullet_events",
        "actors are not told of bullets passing near or hitting by them",
        "Black Ops 2's AI event broadcast for bullets and the actors' reaction to it"
    )
}

fn ai_sound_events_unknown() -> &'static Unknown {
    unknown!(
        "t6.ai.sound_events",
        "actors do not hear a player's footsteps, jumps and landings",
        "Black Ops 2's AI sound event broadcast, and the client flag it checks besides stance and perks"
    )
}

fn glass_unknown() -> &'static Unknown {
    unknown!(
        "t6.weapons.glass",
        "a bullet does not damage breakable glass with Black Ops 2's rules",
        "Black Ops 2's glass pieces and their bullet damage in the runtime"
    )
}

fn dynents_unknown() -> &'static Unknown {
    unknown!(
        "t6.weapons.dynents",
        "bullets and melee blows do not push or break dynamic entities",
        "dynamic entities in the runtime's trace and Black Ops 2's damage to them"
    )
}

fn flesh_impacts_unknown() -> &'static Unknown {
    unknown!(
        "t6.weapons.flesh_impacts",
        "a bullet's hit on a player or actor shows no impact; Black Ops 2's client draws those from its own trace of the shot",
        "Black Ops 2's client bullet trace and its body impact effects"
    )
}

fn riotshield_unknown() -> &'static Unknown {
    unknown!(
        "t6.weapons.riotshield_hits",
        "a bullet or blow on a player's riot shield neither damages the shield nor shows its impact",
        "Black Ops 2's riot shield hit, its damage and its impact effect"
    )
}

fn vehicle_hits_unknown() -> &'static Unknown {
    unknown!(
        "t6.weapons.vehicle_hits",
        "a bullet hitting an entity that may be a vehicle is not given Black Ops 2's vehicle hit rules",
        "which entities are vehicles in the runtime and Black Ops 2's vehicle bullet hit"
    )
}

fn corpse_unknown() -> &'static Unknown {
    unknown!(
        "t6.weapons.corpse_surface",
        "a bullet hitting a dead player's or actor's body takes the surface the trace gives, not flesh",
        "which entities are corpses in the runtime"
    )
}

fn pass_through_unknown() -> &'static Unknown {
    unknown!(
        "t6.weapons.client_pass_through",
        "a player with Black Ops 2's per-client pass-through flag does not shoot through the entities that let them pass",
        "the entities' per-client pass masks in the runtime"
    )
}

fn penetration_table_unknown() -> &'static Unknown {
    unknown!(
        "t6.weapons.penetration_table",
        "a bullet's penetration depth is taken as nothing: the match has no penetration table",
        "Black Ops 2's bullet penetration table"
    )
}

fn melee_events_unknown() -> &'static Unknown {
    unknown!(
        "t6.weapons.melee_events",
        "a melee blow's hit and miss events (sound, blood, the victim's rumble) are not sent",
        "Black Ops 2's melee events in the runtime's event stream"
    )
}

fn deploy_unknown() -> &'static Unknown {
    unknown!(
        "t6.weapons.deploy",
        "a deployable weapon does not place or break down its turret",
        "Black Ops 2's deployed turret spawn and break down"
    )
}

fn grenade_suicide_unknown() -> &'static Unknown {
    unknown!(
        "t6.weapons.grenade_suicide",
        "with grenade suicide on (never on zombies, whose scripts turn it off before any player can cook), a grenade held past its fuse does not kill its thrower outright, credited to whoever threw it back",
        "Black Ops 2's player death, which that rule calls without the damage rules"
    )
}

fn gas_unknown() -> &'static Unknown {
    unknown!(
        "t6.weapons.gas",
        "a gas weapon's flames are not fired (multiplayer: no zombies weapon is of the gas type or class)",
        "Black Ops 2's flame system"
    )
}

fn rocket_unknown() -> &'static Unknown {
    unknown!(
        "t6.weapons.rocket_launch",
        "a rocket of a weapon that accelerates, wanders at random, is guided (other than a ballistic knife's fall) or destabilizes flies straight",
        "Black Ops 2's accelerating, wandering, guided and destabilizing rocket flight"
    )
}

fn missile_antilag_unknown() -> &'static Unknown {
    unknown!(
        "t6.weapons.missile_antilag",
        "a player's missile starts at the match's time; Black Ops 2 starts it at the command's time and runs it forward to catch up",
        "running a new missile ahead by a time offset in the runtime"
    )
}

fn throwback_unknown() -> &'static Unknown {
    unknown!(
        "t6.weapons.throwback",
        "a thrown grenade cannot be picked up and thrown back",
        "thrown grenades as usable entities in the use search, the pick up on switching offhands, and the throw credited to the grenade's owner"
    )
}

fn actor_state_unknown() -> &'static Unknown {
    unknown!(
        "t6.damage.actor_state",
        "an actor takes damage without the actor field Black Ops 2 checks first",
        "what that actor field is and its value in the runtime"
    )
}

fn destructible_unknown() -> &'static Unknown {
    unknown!(
        "t6.damage.destructibles",
        "a destructible's pieces do not take a bullet's or blow's damage with Black Ops 2's rules",
        "Black Ops 2's destructible piece damage"
    )
}

fn location_table_unknown() -> &'static Unknown {
    unknown!(
        "t6.damage.location_table",
        "damage at a hit location is not scaled: the weapon or the match has no hit location table",
        "the weapon's and Black Ops 2's hit location damage tables"
    )
}

/// The weapon events of a player's movement and what the server does with
/// them; the weapon events every client hears are sent on.
pub(crate) fn server_events(
    ps: &PlayerState,
    was: &PlayerState,
    signals: &[MoveSignal],
    zombies: bool,
    world: &mut dyn FireWorld,
) -> Vec<&'static Unknown> {
    let mut gaps = Vec::new();
    for signal in signals {
        let &MoveSignal::Event { event, parm } = signal else {
            continue;
        };
        if let Some(sent) = weapon_event(event) {
            world.weapon_event(ps.client_num, ps.weapon, sent);
        }
        let mut shot = Shot {
            ps,
            was,
            zombies,
            gaps: &mut gaps,
        };
        shot.client_event(event, parm, world);
    }
    gaps.sort_by_key(|gap| gap.id);
    gaps.dedup_by_key(|gap| gap.id);
    gaps
}

/// The weapon events of a player's movement every client hears.
fn weapon_event(event: i32) -> Option<WeaponEvent> {
    Some(match event {
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
    })
}

/// The movement events actors hear: footsteps, jumps and landings.
fn ai_sound_event(event: i32) -> bool {
    (ev::FOOTSTEP_SPRINT..=ev::FOOTSTEP_WALK).contains(&event)
        || (ev::JUMP..ev::FALL_DAMAGE).contains(&event)
}

/// The command's time a shot is fired at, no more than a second behind
/// the match.
fn antilag_time(command_time: i32, world: &dyn FireWorld) -> i32 {
    let level = world.level_time();
    if level - command_time > ANTILAG_MS {
        level - ANTILAG_MS
    } else {
        command_time
    }
}

fn dvar_raw(world: &dyn FireWorld, name: &str) -> Option<String> {
    world
        .dvar(&name.to_ascii_lowercase())
        .or_else(|| world.dvar(name))
}

fn dvar_f32(world: &dyn FireWorld, name: &str, default: f32) -> f32 {
    dvar_raw(world, name)
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

fn dvar_i32(world: &dyn FireWorld, name: &str, default: i32) -> i32 {
    dvar_raw(world, name)
        .and_then(|v| {
            let v = v.trim();
            v.parse()
                .ok()
                .or_else(|| v.parse::<f32>().ok().map(|f| f as i32))
        })
        .unwrap_or(default)
}

fn dvar_bool(world: &dyn FireWorld, name: &str, default: bool) -> bool {
    dvar_i32(world, name, i32::from(default)) != 0
}

fn dvar_vec2(world: &dyn FireWorld, name: &str, default: [f32; 2]) -> [f32; 2] {
    dvar_raw(world, name)
        .and_then(|v| {
            let mut parts = v.split_whitespace().map(|p| p.parse::<f32>().ok());
            Some([parts.next()??, parts.next()??])
        })
        .unwrap_or(default)
}

fn length(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
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

fn distance_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = sub(a, b);
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
}

fn lerp(from: [f32; 3], to: [f32; 3], fraction: f32) -> [f32; 3] {
    [
        (to[0] - from[0]) * fraction + from[0],
        (to[1] - from[1]) * fraction + from[1],
        (to[2] - from[2]) * fraction + from[2],
    ]
}

/// The entity number a trace hit: the world for glass, none for nothing.
fn hit_id(trace: &BulletTrace) -> i32 {
    match trace.hit {
        BulletHitType::Entity(n) => n,
        BulletHitType::Glass(_) => ENTITY_WORLD,
        BulletHitType::None => ENTITY_NONE,
    }
}

fn sentient(entity: FireEntity) -> bool {
    matches!(entity.kind, FireEntityKind::Player | FireEntityKind::Actor)
}

/// The shot's seed: the shot's time with its low byte spread over the rest.
fn scramble(time: i32) -> u32 {
    let mut seed = time as u32;
    let low = seed & 0xff;
    seed ^= low << 8;
    low << 24 ^ seed ^ low << 16
}

/// The seeded random draw, 0 to 1.
fn seeded_random(seed: &mut u32) -> f64 {
    *seed = seed.wrapping_mul(0x0003_43fd).wrapping_add(0x0026_9ec3);
    f64::from(*seed >> 17) * 3.051_757_8e-5
}

/// A seeded point in the unit circle; the seed moves on six draws.
fn seeded_in_circle(seed: &mut u32) -> (f32, f32) {
    let angle = (seeded_random(seed) * 360.0) as f32;
    for _ in 0..4 {
        *seed = seed.wrapping_mul(0x0003_43fd).wrapping_add(0x0026_9ec3);
    }
    let radius = seeded_random(seed) as f32;
    let radians = angle * DEG2RAD;
    let (sin, cos) = f64::from(radians).sin_cos();
    (cos as f32 * radius, radius * sin as f32)
}

/// The server's random draw from -1 to 1.
fn crandom(world: &mut dyn FireWorld) -> f64 {
    let r = f64::from(world.rand()) * 3.051_757_8e-5;
    r + r - 1.0
}

/// A weapon's definition and variant bytes.
type WeaponBytes = (Box<[u8]>, Box<[u8]>);

/// Where a player's shot starts and the axes and angles it is aimed by.
struct WeaponParms {
    forward: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
    muzzle: [f32; 3],
    angles: [f32; 3],
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

    fn sky(&self) -> bool {
        self.trace.surface_flags & surf::SKY != 0
    }
}

/// What a shot or blow does to one entity, before the server's damage
/// rules.
struct Hit {
    target: i32,
    dir: [f32; 3],
    point: [f32; 3],
    amount: i32,
    flags: i32,
    means: usize,
    weapon: u32,
    location: i32,
    bone: Option<u16>,
}

struct Shot<'a> {
    /// The player after the command that raised the event.
    ps: &'a PlayerState,
    /// The player before it.
    was: &'a PlayerState,
    zombies: bool,
    gaps: &'a mut Vec<&'static Unknown>,
}

impl Shot<'_> {
    fn attacker(&self) -> i32 {
        self.ps.client_num
    }

    fn client_event(&mut self, event: i32, parm: i32, world: &mut dyn FireWorld) {
        let me = self.attacker();
        let parm_weapon = u32::try_from(parm).unwrap_or(0);
        match event {
            ev::STANCE_FORCE_CROUCH => world.notify(me, "force crouch", &[]),
            ev::RELOAD_START_NOTIFY => world.notify(me, "reload_start", &[]),
            ev::RELOAD_ADDAMMO => world.notify(me, "reload", &[]),
            ev::WEAPON_SWITCH_STARTED => world.notify(
                me,
                "weapon_switch_started",
                &[NotifyArg::Weapon(parm_weapon)],
            ),
            ev::PULLBACK_WEAPON | ev::PREP_OFFHAND => {
                world.notify(me, "grenade_pullback", &[NotifyArg::Weapon(parm_weapon)]);
            }
            ev::FIRE_WEAPON
            | ev::FIRE_WEAPON_LASTSHOT
            | ev::FIRE_WEAPON_LEFT
            | ev::FIRE_WEAPON_LASTSHOT_LEFT
            | ev::FIRE_WEAPON_MOUNTED_TURRET
            | ev::FIRE_BARREL_1
            | ev::FIRE_BARREL_2 => self.fire_weapon(parm, world),
            ev::FIRE_MELEE => self.melee(world),
            ev::WEAPON_FINISH_DEPLOYING | ev::WEAPON_BREAKING_DOWN => {
                self.gaps.push(deploy_unknown());
            }
            ev::USE_OFFHAND => self.throw_offhand(world),
            // Switching offhands with the use hint on a thrown grenade picks
            // it up to throw back; nothing else is picked up this way.
            ev::SWITCH_OFFHAND
                if self.ps.cursor_hint_ent_index != 0
                    && self.ps.cursor_hint_ent_index != ENTITYNUM_NONE =>
            {
                self.gaps.push(throwback_unknown());
            }
            ev::SWITCH_OFFHAND => {}
            ev::PREP_RIOTSHIELD => world.notify(me, "raise_riotshield", &[]),
            ev::DEPLOY_RIOTSHIELD => world.notify(me, "deploy_riotshield", &[NotifyArg::Int(1)]),
            ev::LOWER_RIOTSHIELD => {
                let shield = self
                    .weapon_bytes(self.ps.weapon, world)
                    .is_some_and(|(d, v)| {
                        WeaponDefView::new(&d, &v)
                            .is_some_and(|d| d.weapon_type() == weapon_type::RIOTSHIELD)
                    });
                if shield {
                    world.notify(me, "deploy_riotshield", &[NotifyArg::Int(0)]);
                }
            }
            ev::GRENADE_DROP => self.drop_grenade(world),
            ev::GRENADE_SUICIDE => self.grenade_suicide(parm_weapon, world),
            ev::DETONATE => world.notify(me, "detonate", &[]),
            e if (ev::FALL_DAMAGE..ev::FALL_DAMAGE + 32).contains(&e) => {
                if self.ps.pm_flags & pm_flags::SLIDING == 0 {
                    let frac = if parm < 100 { parm as f32 * 0.01 } else { 1.1 };
                    if frac != 0.0
                        && let Some(entity) = world.entity(me)
                        && entity.takes_damage
                    {
                        let amount = (entity.max_health as f32 * frac) as i32;
                        world.damage(&FireDamage {
                            target: me,
                            inflictor: ENTITY_NONE,
                            attacker: ENTITY_NONE,
                            dir: [0.0; 3],
                            point: [0.0; 3],
                            amount,
                            flags: 0,
                            means: MEANS[MOD_FALLING],
                            weapon: 0,
                            location: 0,
                            hit_location: HIT_LOCATIONS[0],
                            bone: None,
                        });
                    }
                }
            }
            e if ai_sound_event(e)
                && self.ps.pm_flags & pm_flags::DUCKED == 0
                && !self.has_perk(perks::QUIETER, world) =>
            {
                self.gaps.push(ai_sound_events_unknown());
            }
            _ => {}
        }
    }

    /// A weapon's definition and variant bytes, copied out of the match.
    fn weapon_bytes(&mut self, weapon: u32, world: &dyn FireWorld) -> Option<WeaponBytes> {
        let w = world.weapon(weapon)?;
        WeaponDefView::new(&w.def, &w.variant)?;
        Some((w.def.clone(), w.variant.clone()))
    }

    /// The held weapon or its bayonet strikes the blow itself.
    fn melee_rule(&mut self, weapon: u32, world: &dyn FireWorld) -> bool {
        self.weapon_bytes(weapon, world).is_some_and(|(d, v)| {
            WeaponDefView::new(&d, &v).is_some_and(|d| d.bayonet() || d.use_as_melee())
        })
    }

    /// The weapon the player has in use: the offhand while one is thrown,
    /// the melee weapon in a blow with a weapon that does not strike it.
    fn weapon_in_use(&mut self, world: &dyn FireWorld) -> u32 {
        let ps = self.ps;
        let s = ps.weaponstate;
        if ps.weap_flags & weap_flags::OFFHAND_ACTIVE != 0 && ws::is_offhand(s) {
            return ps.offhand_index;
        }
        if ws::is_melee(s) && !self.melee_rule(ps.weapon, world) && s != ws::MELEE_END {
            return ps.melee_weapon;
        }
        ps.weapon
    }

    /// The player has the perk, or the weapon in use grants it.
    fn has_perk(&mut self, perk: usize, world: &dyn FireWorld) -> bool {
        if perks::has(self.ps.perks, perk) {
            return true;
        }
        let weapon = self.weapon_in_use(world);
        self.weapon_bytes(weapon, world).is_some_and(|(d, v)| {
            WeaponDefView::new(&d, &v).is_some_and(|d| perks::has(d.variant_perks(), perk))
        })
    }

    /// The weapon a fired event shoots: a charge shot's level steps it
    /// along its alternate weapons.
    fn shot_weapon(&mut self, parm: i32, mut weapon: u32, world: &dyn FireWorld) -> u32 {
        let charge = world.weapon(weapon).is_some_and(|w| {
            w.alt_weapon != 0
                && WeaponDefView::new(&w.def, &w.variant)
                    .is_some_and(|d| d.fire_type() == fire_type::CHARGE_SHOT)
        });
        if !charge {
            return weapon;
        }
        for _ in 1..parm {
            let Some(w) = world.weapon(weapon) else {
                break;
            };
            let alt_mode = WeaponDefView::new(&w.def, &w.variant)
                .is_some_and(|d| d.inventory_type() == inventory_type::ALTMODE);
            if !alt_mode {
                weapon = w.alt_weapon;
            }
        }
        weapon
    }

    /// A fired weapon: tells scripts, takes the weapon's spread and fires
    /// the shot its weapon type makes.
    fn fire_weapon(&mut self, parm: i32, world: &mut dyn FireWorld) {
        let ps = self.ps;
        if ps.e_flags & e_flags::TURRET != 0 {
            self.gaps.push(mounted_unknown());
            return;
        }
        let weapon = self.shot_weapon(parm, ps.weapon, world);
        world.notify(
            self.attacker(),
            "weapon_fired",
            &[NotifyArg::Weapon(weapon)],
        );
        let Some((def_bytes, variant_bytes)) = self.weapon_bytes(weapon, world) else {
            return;
        };
        let Some(def) = WeaponDefView::new(&def_bytes, &variant_bytes) else {
            return;
        };
        let Some(parms) = self.weapon_parms(&def, parm, world) else {
            return;
        };
        // The server keeps the spread scale its end of frame left.
        let aim = self.was.aim_spread_scale * 0.003_921_569;
        let (min, max) = self.hip_spread(&def, world);
        let spread = if ps.weapon_pos_frac == 1.0 {
            (max - def.ads_spread()) * aim + def.ads_spread()
        } else {
            (max - min) * aim + min
        };
        match def.weapon_type() {
            weapon_type::BULLET => self.bullet_fire(weapon, &def, spread, &parms, world),
            weapon_type::GRENADE | weapon_type::MINE => {
                self.throw_grenade(weapon, &def, &parms, false, world);
            }
            weapon_type::PROJECTILE if def.weapon_class() == weap_class::GRENADE => {
                self.throw_grenade(weapon, &def, &parms, true, world);
            }
            weapon_type::PROJECTILE => self.fire_rocket(weapon, &def, spread, &parms, world),
            weapon_type::GAS => self.gaps.push(gas_unknown()),
            _ => {}
        }
    }

    /// The offhand let go: thrown from the eye along the aim.
    fn throw_offhand(&mut self, world: &mut dyn FireWorld) {
        let weapon = self.ps.offhand_index;
        let Some((def_bytes, variant_bytes)) = self.weapon_bytes(weapon, world) else {
            return;
        };
        let Some(def) = WeaponDefView::new(&def_bytes, &variant_bytes) else {
            return;
        };
        let Some(parms) = self.weapon_parms(&def, 0, world) else {
            return;
        };
        self.throw_grenade(weapon, &def, &parms, false, world);
    }

    /// The eye and the axes of the aim: the gun angles, a stacked weapon's
    /// barrel turned aside by its share of the stack's spread.
    fn weapon_parms(
        &mut self,
        def: &WeaponDefView<'_>,
        parm: i32,
        world: &mut dyn FireWorld,
    ) -> Option<WeaponParms> {
        self.gaps.push(gun_angles_unknown());
        let mut angles = [
            self.was.viewangles[0],
            self.was.viewangles[1],
            self.ps.viewangles[2],
        ];
        if def.fire_type() == fire_type::STACKED {
            let count = stack_count(def, world);
            if count > 1 {
                let mut spread = def.stack_fire_spread();
                if count == 2 {
                    spread *= if self.ps.weapon_shot_count == 2 {
                        0.5
                    } else {
                        -0.5
                    };
                } else {
                    let mut barrels = (count - 1) as f32;
                    if dvar_bool(world, "tu12_zm_stack_fire_fix", true) {
                        barrels = def.stack_fire().wrapping_sub(1) as u32 as f32;
                    }
                    spread = (spread / barrels) * (parm - 1) as f32 - spread * 0.5;
                }
                angles[1] += spread;
            }
        }
        let (forward, right, up) = angle_vectors(angles);
        let muzzle = self.view_origin(world)?;
        Some(WeaponParms {
            forward,
            right,
            up,
            muzzle,
            angles,
        })
    }

    /// The eye a player's shot or blow starts from; a mounted or linked
    /// view's is not run.
    fn view_origin(&mut self, world: &dyn FireWorld) -> Option<[f32; 3]> {
        let ps = self.ps;
        if ps.link_flags & 4 != 0
            || ps.e_flags & (e_flags::TURRET | e_flags::VEHICLE_VIEW) != 0
            || ps.e_flags2 & e_flags2::REMOTE_CONTROL != 0
        {
            self.gaps.push(mounted_unknown());
            return None;
        }
        Some(player_view_origin(ps, world.level_time(), world))
    }

    /// The hip spread at the player's eye height, and what overrides and
    /// scales it.
    fn hip_spread(&mut self, def: &WeaponDefView<'_>, world: &dyn FireWorld) -> (f32, f32) {
        let ps = self.ps;
        let (mut min, mut max);
        if ps.spread_override_state == 2 {
            min = ps.spread_override as f32;
            max = ps.spread_override as f32;
        } else {
            let height = ps.view_height_current;
            if height > CROUCH_EYE {
                let stand = dvar_i32(world, "player_standingViewHeight", 60);
                let frac = (height - CROUCH_EYE) / (stand - 40) as f32;
                min = (def.hip_spread_stand_min() - def.hip_spread_ducked_min()) * frac
                    + def.hip_spread_ducked_min();
                max = (def.hip_spread_max() - def.hip_spread_ducked_max()) * frac
                    + def.hip_spread_ducked_max();
            } else {
                let frac = (height - PRONE_EYE) * PRONE_TO_CROUCH;
                min = (def.hip_spread_ducked_min() - def.hip_spread_prone_min()) * frac
                    + def.hip_spread_prone_min();
                max = (def.hip_spread_ducked_max() - def.hip_spread_prone_max()) * frac
                    + def.hip_spread_prone_max();
            }
        }
        if def.fire_type() == fire_type::STACKED {
            let count = stack_count(def, world);
            if count > 1 {
                let decay = f64::from(def.stack_fire_accuracy_decay());
                let mut scale = decay.powf(f64::from(count) - 1.0) as f32;
                if dvar_bool(world, "tu12_zm_stack_fire_fix", true) {
                    let barrels = f64::from(def.stack_fire() as u32) - 1.0;
                    scale = decay.powf(barrels) as f32;
                }
                min *= scale;
                max *= scale;
            }
        }
        if ps.spread_override_state == 1 {
            max = ps.spread_override as f32;
        }
        let applies = if self.zombies {
            self.has_perk(perks::DEADSHOT, world) && ps.weapon_pos_frac < 1.0
        } else {
            self.has_perk(perks::BULLET_ACCURACY, world)
        };
        if applies {
            let scale = dvar_f32(world, "perk_weapSpreadMultiplier", 0.65);
            min *= scale;
            max *= scale;
        }
        (min, max)
    }

    /// A bullet weapon's shot: each pellet's end from the shot's seed, then
    /// its trace.
    fn bullet_fire(
        &mut self,
        weapon: u32,
        def: &WeaponDefView<'_>,
        spread: f32,
        parms: &WeaponParms,
        world: &mut dyn FireWorld,
    ) {
        let time = antilag_time(self.ps.command_time, world);
        self.gaps.push(antilag_unknown());
        self.gaps.push(hit_priority_unknown());
        self.gaps.push(dynents_unknown());
        let spread_class = matches!(
            def.weapon_class(),
            weap_class::SPREAD | weap_class::PISTOL_SPREAD
        );
        let mut shots = if spread_class { def.shot_count() } else { 1 };
        let range = bullet_range(def, world);
        if self.has_perk(perks::RATE_OF_FIRE, world) {
            shots *= 2;
        }
        let mut seed = scramble(time);
        let flat = perks::has(def.variant_perks(), variant::SPREAD_FLAT_BIT as usize);
        let attacker = self.attacker();
        for shot in 0..shots {
            let mut damage_multiplier = 1.0;
            let level = self.ps.charge_shot_level;
            if def.fire_type() == fire_type::CHARGE_SHOT && level as u32 > 1 {
                let per = f64::from(dvar_f32(world, "bg_chargeShotDamageIncreasePerBullet", 0.0));
                damage_multiplier = (f64::from(level.wrapping_sub(1) as u32) * per + 1.0) as f32;
            }
            let (end, dir) = bullet_end(&mut seed, spread, parms, range, shot, shots, flat);
            let mut bp = BulletParams {
                weapon_entity: attacker,
                ignore_entity: attacker,
                damage_multiplier,
                means: if def.rifle_bullet() {
                    MOD_RIFLE_BULLET
                } else {
                    MOD_PISTOL_BULLET
                },
                origin: parms.muzzle,
                start: parms.muzzle,
                end,
                dir,
            };
            if def.penetrate_type() == 0 || def.bullet_impact_explode() {
                self.fire_extended(def, weapon, &mut bp, world);
            } else {
                self.fire_penetrate(def, weapon, &mut bp, world);
            }
        }
    }

    /// One trace of the bullet and what it hit; false when it hit nothing.
    /// The results hold the trace either way.
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
        if br.trace.hit == BulletHitType::None && !br.sky() {
            return (false, br);
        }
        let id = match hit_id(&br.trace) {
            ENTITY_NONE => ENTITY_WORLD,
            id => id,
        };
        br.hit_entity = (id != ENTITY_WORLD || br.sky()).then_some(id);
        br.hit_pos = lerp(bp.start, bp.end, br.trace.fraction);
        if let Some(hit) = br.hit_entity {
            let entity = world.entity(hit);
            if br.trace.hit_location == HITLOC_RIOTSHIELD {
                br.trace.surface_flags = surf::RIOTSHIELD;
            } else if br.trace.surface_flags == 0 {
                match entity {
                    Some(e) if sentient(e) => br.trace.surface_flags = surf::FLESH,
                    Some(_) => self.gaps.push(corpse_unknown()),
                    None => {}
                }
            }
            br.ignore_hit_entity = self.ps.link_flags & 4 == 0
                && self.ps.vehicle_entity == ENTITYNUM_NONE
                && hit == self.attacker();
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

    /// The entity the bullet hit lets this player's bullets through.
    fn passes_through(&mut self, br: &BulletResults) -> bool {
        if br.hit_entity.is_some() && self.ps.other_flags & 0x20 != 0 {
            self.gaps.push(pass_through_unknown());
        }
        false
    }

    /// A bullet that does not penetrate goes on only past surfaces without
    /// impacts and through what it may pass; a rifle bullet through bodies
    /// at half damage; it may bounce off what stops it.
    fn fire_extended(
        &mut self,
        def: &WeaponDefView<'_>,
        weapon: u32,
        bp: &mut BulletParams,
        world: &mut dyn FireWorld,
    ) {
        for _ in 0..BULLET_EXTENDED_SEGMENTS {
            let (hit, br) = self.bullet_trace(bp, 0, world);
            if !hit {
                self.gaps.push(ai_bullet_events_unknown());
                return;
            }
            let (go_on, _) = self.bullet_hit(def, weapon, bp, &br, 0, true, world);
            if !go_on {
                return;
            }
            if br.trace.surface_flags & surf::NOIMPACT != 0 {
                if !advance_trace(bp, &br, PENETRATE_STEP) {
                    return;
                }
                continue;
            }
            let Some(hit) = br.hit_entity else {
                self.ricochet(def, weapon, bp, &br, world);
                return;
            };
            if self.passes_through(&br) {
                advance_trace(bp, &br, PENETRATE_STEP);
                continue;
            }
            let entity = world.entity(hit);
            if !entity.is_some_and(|e| e.takes_damage) {
                self.ricochet(def, weapon, bp, &br, world);
                return;
            }
            if !br.ignore_hit_entity {
                let through = def.rifle_bullet()
                    && entity.is_some_and(sentient)
                    && (dvar_i32(world, "scr_team_fftype", 1) != 0
                        || !same_team(self.attacker(), entity, world));
                if !through {
                    self.ricochet(def, weapon, bp, &br, world);
                    return;
                }
                bp.damage_multiplier *= 0.5;
            }
            advance_trace(bp, &br, 0.0);
        }
    }

    /// A bullet off a surface it cannot go through, on along its mirror for
    /// the rest of its range.
    fn ricochet(
        &mut self,
        def: &WeaponDefView<'_>,
        weapon: u32,
        bp: &BulletParams,
        br: &BulletResults,
        world: &mut dyn FireWorld,
    ) {
        if !dvar_bool(world, "tu13_bg_enableBulletWeaponBounce", true)
            || !def.def_bool(def::BULLET_BOUNCE)
        {
            return;
        }
        let n = br.trace.normal;
        let d = bp.dir;
        let dot = d[0] * n[0] + n[1] * d[1] + n[2] * d[2];
        if 1.0 < dot {
            return;
        }
        let k = dot * -2.0;
        let mut next = *bp;
        next.start = br.hit_pos;
        next.dir = [n[0] * k + d[0], n[1] * k + d[1], n[2] * k + d[2]];
        let travelled = sub(br.hit_pos, bp.start);
        let left = bullet_range(def, world)
            - (travelled[1] * travelled[1]
                + travelled[0] * travelled[0]
                + travelled[2] * travelled[2])
                .sqrt();
        next.end = mad(br.hit_pos, next.dir, left);
        let (hit, nbr) = self.bullet_trace(&next, 0, world);
        if !hit {
            self.gaps.push(ai_bullet_events_unknown());
            return;
        }
        self.bullet_hit(def, weapon, &next, &nbr, 0, true, world);
    }

    /// A penetrating bullet goes through surfaces as deep as the
    /// penetration table lets it, losing damage by their thickness.
    fn fire_penetrate(
        &mut self,
        def: &WeaponDefView<'_>,
        weapon: u32,
        bp: &mut BulletParams,
        world: &mut dyn FireWorld,
    ) {
        let (hit, mut br) = self.bullet_trace(bp, 0, world);
        if !hit {
            self.gaps.push(ai_bullet_events_unknown());
            return;
        }
        let (mut go_on, mut flags) = self.bullet_hit(def, weapon, bp, &br, 0, true, world);
        if !go_on || def.bullet_impact_explode() {
            return;
        }
        let charge = def.fire_type() == fire_type::CHARGE_SHOT;
        let count = if charge {
            CHARGE_SHOT_PENETRATIONS
        } else {
            dvar_i32(world, "penetrationCount", 5)
        };
        let mut scale = 0.0f32;
        if self.has_perk(perks::BULLET_PENETRATION, world) {
            scale = dvar_f32(world, "perk_bulletPenetrationMultiplier", 2.0);
        }
        let level = self.ps.charge_shot_level;
        if charge && level as u32 > 1 {
            let per = f64::from(dvar_f32(world, "bg_chargeShotPenetrationMultiplier", 1.0));
            scale = (f64::from(level as u32) * per + f64::from(scale)) as f32;
        }
        let min_fx = dvar_f32(world, "bullet_penetrationMinFxDist", 30.0);
        let pen_type = def.penetrate_type();
        for _ in 0..count {
            let mut max_depth = self.depth(pen_type, br.depth_surface_type, world);
            if br.sky() || br.surface_type() == surf::WATER || self.passes_through(&br) {
                max_depth = OPEN_DEPTH;
            }
            if 0.0 < scale {
                max_depth *= scale;
            }
            if max_depth <= 0.0 {
                return;
            }
            let last_hit_pos = br.hit_pos;
            if !advance_trace(bp, &br, PENETRATE_STEP) {
                return;
            }
            let hit;
            (hit, br) = self.bullet_trace(bp, br.depth_surface_type, world);
            // The way back: from the next hit (or the bullet's end) to just
            // past where it went in.
            let mut rev_bp = *bp;
            rev_bp.dir = [-bp.dir[0], -bp.dir[1], -bp.dir[2]];
            rev_bp.start = bp.end;
            rev_bp.end = mad(last_hit_pos, rev_bp.dir, EXIT_STEP);
            let mut rev_br = br;
            rev_br.trace.normal = [
                -br.trace.normal[0],
                -br.trace.normal[1],
                -br.trace.normal[2],
            ];
            if hit {
                advance_trace(&mut rev_bp, &rev_br, EXIT_STEP);
            }
            let rev_hit;
            (rev_hit, rev_br) = self.bullet_trace(&rev_bp, rev_br.depth_surface_type, world);
            let all_solid = (rev_hit && rev_br.trace.all_solid)
                || (br.trace.start_solid && rev_br.trace.start_solid);
            if rev_br.sky() {
                rev_br.hit_entity = Some(match hit_id(&rev_br.trace) {
                    ENTITY_NONE => ENTITY_WORLD,
                    id => id,
                });
            }
            if !rev_hit && !all_solid {
                if !hit {
                    return;
                }
                let impact = distance_sq(br.hit_pos, last_hit_pos) > min_fx * min_fx;
                (go_on, flags) =
                    self.bullet_hit(def, weapon, bp, &br, dflags::PENETRATED, impact, world);
            } else {
                let span = if all_solid {
                    sub(rev_bp.end, rev_bp.start)
                } else {
                    sub(last_hit_pos, rev_br.hit_pos)
                };
                let thickness = (span[1] * span[1] + span[0] * span[0] + span[2] * span[2])
                    .sqrt()
                    .max(1.0);
                if rev_hit {
                    let mut rev_depth = self.depth(pen_type, rev_br.depth_surface_type, world);
                    if 0.0 < scale {
                        rev_depth *= scale;
                    }
                    if rev_depth - max_depth < 0.0 {
                        max_depth = rev_depth;
                    }
                    if rev_br.sky() {
                        max_depth = OPEN_DEPTH;
                    } else if max_depth <= 0.0 {
                        return;
                    }
                }
                bp.damage_multiplier -= thickness / max_depth;
                if bp.damage_multiplier <= 0.0 {
                    return;
                }
                if !all_solid {
                    let impact = distance_sq(br.hit_pos, rev_br.hit_pos) > min_fx * min_fx;
                    if impact && (!hit || !br.sky()) {
                        self.impact(weapon, &rev_bp, &rev_br, bp.dir, flags | IMPACT_EXIT, world);
                    }
                    if !hit {
                        return;
                    }
                    (go_on, flags) =
                        self.bullet_hit(def, weapon, bp, &br, dflags::PENETRATED, impact, world);
                }
            }
            if !hit || !go_on {
                return;
            }
        }
    }

    /// How deep a bullet goes into a surface type: nothing into none.
    fn depth(&mut self, pen_type: i32, surface: i32, world: &dyn FireWorld) -> f32 {
        if surface == 0 {
            return 0.0;
        }
        world
            .penetration_depth(pen_type, surface)
            .unwrap_or_else(|| {
                self.gaps.push(penetration_table_unknown());
                0.0
            })
    }

    /// A bullet's hit: its damage at its range, damage triggers on its
    /// path, its impact, and the damage to the entity it hit. Returns
    /// whether the bullet goes on, and its impact flags (1: a head hit).
    #[allow(clippy::too_many_arguments)]
    fn bullet_hit(
        &mut self,
        def: &WeaponDefView<'_>,
        weapon: u32,
        bp: &BulletParams,
        br: &BulletResults,
        mut damage_flags: i32,
        impact: bool,
        world: &mut dyn FireWorld,
    ) -> (bool, u8) {
        let attacker = self.attacker();
        self.gaps.push(ai_bullet_events_unknown());
        let damage =
            (bullet_damage(def, bp.origin, br.hit_pos) as f32 * bp.damage_multiplier) as i32;
        world.damage_triggers(attacker, bp.start, br.hit_pos, damage, MEANS[bp.means]);
        if impact {
            self.impact(weapon, bp, br, br.trace.normal, 0, world);
        }
        if matches!(br.trace.hit, BulletHitType::Glass(_)) {
            self.gaps.push(glass_unknown());
        }
        let mut go_on = true;
        let mut flags = 0u8;
        if let Some(hit) = br.hit_entity
            && !br.ignore_hit_entity
            && let Some(entity) = world.entity(hit)
        {
            if br.trace.hit_location == HITLOC_RIOTSHIELD {
                self.gaps.push(riotshield_unknown());
                go_on = entity.kind != FireEntityKind::Player;
            } else if entity.takes_damage {
                if entity.kind == FireEntityKind::Other {
                    self.gaps.push(vehicle_hits_unknown());
                }
                if def.armor_piercing() {
                    damage_flags |= dflags::NO_ARMOR;
                }
                let location = br.trace.hit_location;
                if location == 1 || location == 2 {
                    flags |= 1;
                }
                self.damage(
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
            }
        }
        if def.bullet_impact_explode() && def.explosion_inner_damage() != 0 {
            let angle = def.damage_cone_angle();
            let cone = (angle - 180.0 < 0.0).then(|| f64::from(angle * DEG2RAD).cos() as f32);
            crate::grenade::radius_damage(
                &RadiusDamage {
                    origin: br.hit_pos,
                    inflictor: bp.weapon_entity,
                    attacker,
                    inner: def.explosion_inner_damage() as f32,
                    outer: def.explosion_outer_damage() as f32,
                    radius: def.explosion_radius() as f32,
                    cone: cone.map(|cos| (cos, bp.dir)),
                    ignore: ENTITY_NONE,
                    means: MEANS[MOD_PROJECTILE_SPLASH],
                    weapon,
                    script: false,
                },
                world,
            );
        }
        (go_on, flags)
    }

    /// The bullet's impact where Black Ops 2's server marks one: sent by
    /// it when bullets are not the client's own, else drawn by the client.
    fn impact(
        &mut self,
        weapon: u32,
        bp: &BulletParams,
        br: &BulletResults,
        normal: [f32; 3],
        flags: u8,
        world: &mut dyn FireWorld,
    ) {
        if br.ignore_hit_entity {
            return;
        }
        let body = br
            .hit_entity
            .and_then(|e| world.entity(e))
            .is_some_and(sentient);
        if br.sky() || 1.0 <= br.trace.fraction || body {
            if br.trace.hit_location == HITLOC_RIOTSHIELD {
                self.gaps.push(riotshield_unknown());
            } else if body {
                self.gaps.push(flesh_impacts_unknown());
            }
            return;
        }
        let client_side = dvar_bool(world, "sv_clientSideBullets", true);
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
            exit: flags & IMPACT_EXIT != 0,
            from_server: !client_side && flags & IMPACT_EXIT == 0,
        });
    }

    /// A thrown grenade or mine, or a launched grenade.
    fn throw_grenade(
        &mut self,
        weapon: u32,
        def: &WeaponDefView<'_>,
        parms: &WeaponParms,
        launcher: bool,
        world: &mut dyn FireWorld,
    ) {
        let me = self.attacker();
        let time_left = self.ps.grenade_time_left;
        crate::grenade::launch_grenade(
            me,
            weapon,
            def,
            parms.muzzle,
            parms.angles,
            launcher,
            Some(time_left),
            world,
        );
        self.fired_grenade(def, def.fuse_time(), world);
    }

    /// The offhand grenade dropped from a player's hands, its cooked fuse
    /// burning on.
    fn drop_grenade(&mut self, world: &mut dyn FireWorld) {
        let time_left = self.ps.grenade_time_left;
        if time_left == 0 {
            return;
        }
        let weapon = self.ps.offhand_index;
        let Some((def_bytes, variant_bytes)) = self.weapon_bytes(weapon, world) else {
            return;
        };
        let Some(def) = WeaponDefView::new(&def_bytes, &variant_bytes) else {
            return;
        };
        let vel: [f32; 3] =
            core::array::from_fn(|_| (crandom(world) * f64::from(DROP_SPEED)) as f32);
        let mut origin = self.ps.origin;
        origin[2] += DROP_HEIGHT;
        crate::grenade::spawn_grenade(
            self.attacker(),
            weapon,
            &def,
            origin,
            vel,
            [[0.0; 3]; 2],
            true,
            time_left,
            Some(time_left),
            world,
        );
        self.fired_grenade(&def, time_left, world);
    }

    /// A grenade held past its fuse. With grenade suicide off (as the
    /// zombies scripts set it) it falls at the thrower's feet, already gone
    /// off, and its thrower takes a blow no one survives unless Flak Jacket
    /// spares them.
    fn grenade_suicide(&mut self, weapon: u32, world: &mut dyn FireWorld) {
        if !world.grenade_suicide_disabled() {
            self.gaps.push(grenade_suicide_unknown());
            return;
        }
        let me = self.attacker();
        let offhand = self.ps.offhand_index;
        if let Some((def_bytes, variant_bytes)) = self.weapon_bytes(offhand, world)
            && let Some(def) = WeaponDefView::new(&def_bytes, &variant_bytes)
        {
            let time_left = self.ps.grenade_time_left;
            let mut origin = self.ps.origin;
            origin[2] += DROP_HEIGHT;
            crate::grenade::spawn_grenade(
                me,
                offhand,
                &def,
                origin,
                [0.0; 3],
                [[0.0; 3]; 2],
                false,
                time_left,
                Some(time_left),
                world,
            );
            self.fired_grenade(&def, time_left, world);
        }
        if self.has_perk(perks::FLAK_JACKET, world) {
            return;
        }
        world.damage(&FireDamage {
            target: me,
            inflictor: me,
            attacker: me,
            dir: [0.0; 3],
            point: [0.0; 3],
            amount: SUICIDE_DAMAGE,
            flags: 0,
            means: MEANS[MOD_SUICIDE],
            weapon,
            location: 0,
            hit_location: HIT_LOCATIONS[0],
            bone: None,
        });
    }

    /// What follows a player's grenade into the world: the cooked fuse it
    /// took is cleared and scripts are told.
    fn fired_grenade(&mut self, def: &WeaponDefView<'_>, fuse: i32, world: &mut dyn FireWorld) {
        let me = self.attacker();
        self.gaps.push(missile_antilag_unknown());
        if self.ps.throw_back_grenade_owner != ENTITYNUM_NONE {
            self.gaps.push(throwback_unknown());
        }
        if crate::grenade::takes_time_left(def, fuse) && self.ps.grenade_time_left != 0 {
            clear_grenade_time(me, world);
        }
        // `grenade_fire` / `grenade_launcher_fire` are raised as the runtime
        // adopts the grenade's script entity.
    }

    /// A projectile weapon's rocket, its knockback on the shooter.
    fn fire_rocket(
        &mut self,
        weapon: u32,
        def: &WeaponDefView<'_>,
        spread: f32,
        parms: &WeaponParms,
        world: &mut dyn FireWorld,
    ) {
        // A straight or falling rocket is Black Ops 2's own; the others are
        // not run.
        if def.projectile_acceleration() > 0.0
            || def.projectile_random_wander() > 0.0
            || !matches!(def.guided_missile_type(), 0 | 4)
            || def.guided_missile_type() == 0 && def.destabilization_rate_time() != 0.0
        {
            self.gaps.push(rocket_unknown());
        }
        let knockback = def.fire_type() != fire_type::FULL_AUTO
            && (!dvar_bool(world, "tu11_useMissileKnockbackHeldWeaponsOnly", true)
                || weapon == self.ps.weapon);
        let me = self.attacker();
        crate::missile::launch_rocket(
            me,
            weapon,
            def,
            parms.muzzle,
            parms.angles,
            spread,
            [0.0; 3],
            knockback,
            world,
        );
        self.gaps.push(missile_antilag_unknown());
        // `missile_fire` is raised as the runtime adopts the rocket's script entity.
    }

    /// A melee blow: the weapon that strikes (the held one when it is a
    /// bayonet or used as a melee weapon, else the melee weapon), its reach,
    /// and its damage to what its traces reach first.
    fn melee(&mut self, world: &mut dyn FireWorld) {
        let ps = self.ps;
        if ps.e_flags & e_flags::TURRET != 0 {
            self.gaps.push(mounted_unknown());
            return;
        }
        let held = ps.weapon;
        let weapon = if self.melee_rule(held, world) {
            held
        } else {
            ps.melee_weapon
        };
        let me = self.attacker();
        world.notify(me, "weapon_melee", &[NotifyArg::Weapon(weapon)]);
        let Some((def_bytes, variant_bytes)) = self.weapon_bytes(weapon, world) else {
            return;
        };
        let Some(def) = WeaponDefView::new(&def_bytes, &variant_bytes) else {
            return;
        };
        let Some(eye) = self.view_origin(world) else {
            return;
        };
        let (forward, right, up) = angle_vectors(ps.viewangles);
        let mut range = if def.bayonet() {
            dvar_f32(world, "player_bayonetRange", 85.0)
        } else {
            dvar_f32(world, "player_meleeRange", 55.0)
        };
        let (width, height) = if self.has_perk(perks::NONAME, world) {
            range = dvar_f32(world, "player_zombieMeleeRange", 85.0);
            (
                dvar_f32(world, "player_zombieMeleeWidth", 30.0),
                dvar_f32(world, "player_zombieMeleeHeight", 20.0),
            )
        } else {
            (
                dvar_f32(world, "player_meleeWidth", 10.0),
                dvar_f32(world, "player_meleeHeight", 10.0),
            )
        };
        self.gaps.push(antilag_unknown());
        self.gaps.push(hit_priority_unknown());
        self.gaps.push(dynents_unknown());
        let damage = def.melee_damage();
        let parms = WeaponParms {
            forward,
            right,
            up,
            muzzle: eye,
            angles: ps.viewangles,
        };
        let Some((trace, point)) = self.melee_trace(&parms, damage, range, width, height, world)
        else {
            return;
        };
        self.gaps.push(melee_events_unknown());
        let amount = damage + world.rand() % 5;
        let dir = [forward[0], forward[1], forward[2] + MELEE_LIFT];
        let target = hit_id(&trace);
        if target == ENTITY_WORLD {
            return;
        }
        let Some(entity) = world.entity(target).filter(|e| e.takes_damage) else {
            return;
        };
        let mut location = trace.hit_location;
        if location == HITLOC_RIOTSHIELD {
            location = 0;
            if entity.kind == FireEntityKind::Player {
                self.gaps.push(riotshield_unknown());
                if def.weapon_type() != weapon_type::RIOTSHIELD {
                    return;
                }
            }
        }
        let held_bayonet = self
            .weapon_bytes(held, world)
            .is_some_and(|(d, v)| WeaponDefView::new(&d, &v).is_some_and(|d| d.bayonet()));
        self.damage(
            me,
            entity,
            Hit {
                target,
                dir,
                point,
                amount,
                flags: 0,
                means: if held_bayonet { MOD_BAYONET } else { MOD_MELEE },
                weapon: held,
                location,
                bone: trace.bone,
            },
            world,
        );
    }

    /// The blow's traces from just behind the eye out to its reach, across
    /// its width and height: the first that reaches a body, else the
    /// nearest that hits anything. Damage triggers on the centre line.
    fn melee_trace(
        &mut self,
        parms: &WeaponParms,
        damage: i32,
        range: f32,
        width: f32,
        height: f32,
        world: &mut dyn FireWorld,
    ) -> Option<(BulletTrace, [f32; 3])> {
        let count = if 0.0 < width || 0.0 < height { 9 } else { 1 };
        let attacker = self.attacker();
        let start = mad(parms.muzzle, parms.forward, -MELEE_BACKSTEP);
        let mut best: Option<(BulletTrace, [f32; 3])> = None;
        let mut best_fraction = 1.0;
        for (i, &(across, along)) in MELEE_OFFSETS[..count].iter().enumerate() {
            let mut end = mad(parms.muzzle, parms.forward, range);
            if count > 1 {
                let w = width * across;
                let h = along * height;
                end = [
                    parms.up[0] * h + parms.right[0] * w + end[0],
                    parms.up[1] * h + parms.right[1] * w + end[1],
                    parms.up[2] * h + parms.right[2] * w + end[2],
                ];
            }
            let trace = world.bullet_trace(start, end, attacker, MELEE_MASK);
            let point = lerp(start, end, trace.fraction);
            if i == 0 {
                world.damage_triggers(attacker, parms.muzzle, point, damage, MEANS[MOD_MELEE]);
            }
            if trace.fraction == 1.0 || trace.surface_flags & surf::NOIMPACT != 0 {
                continue;
            }
            if world.entity(hit_id(&trace)).is_some_and(sentient) {
                return Some((trace, point));
            }
            if trace.fraction <= best_fraction {
                best_fraction = trace.fraction;
                best = Some((trace, point));
            }
        }
        best.filter(|_| best_fraction < 1.0)
    }

    /// The server's damage to what a shot or blow reaches: a player's and
    /// an actor's hit location scale, then the entity's damage callback.
    fn damage(&mut self, attacker: i32, entity: FireEntity, hit: Hit, world: &mut dyn FireWorld) {
        if hit.location == HITLOC_RIOTSHIELD || !entity.takes_damage {
            return;
        }
        let mut amount = hit.amount;
        let mut dir = hit.dir;
        match entity.kind {
            FireEntityKind::Player => {
                if amount < 1 {
                    return;
                }
                let Some(mut player) = world.player(hit.target) else {
                    return;
                };
                let (victim, _) = crate::movement::unpack(&mut player);
                if victim.pm_type == pm_type::DEAD || victim.other_flags & 1 != 0 {
                    return;
                }
                // The grace after going down is the match's own G_Damage
                // (`finishplayerdamage` keeps its time).
                if hit.means != MOD_MELEE {
                    amount = self.located(amount, hit.location, hit.weapon, world);
                }
            }
            FireEntityKind::Actor => {
                if amount < 1 {
                    return;
                }
                self.gaps.push(actor_state_unknown());
                if hit.means != MOD_MELEE {
                    amount = self.located(amount, hit.location, hit.weapon, world);
                }
            }
            FireEntityKind::Other => {
                if entity.blast_handler && hit.flags & dflags::RADIUS == 0 {
                    self.gaps.push(destructible_unknown());
                }
                dir = normalized(dir);
            }
        }
        amount = amount.max(1);
        world.damage(&FireDamage {
            target: hit.target,
            inflictor: attacker,
            attacker,
            dir,
            point: hit.point,
            amount,
            flags: hit.flags,
            means: MEANS[hit.means],
            weapon: hit.weapon,
            hit_location: usize::try_from(hit.location)
                .ok()
                .and_then(|i| HIT_LOCATIONS.get(i))
                .copied()
                .unwrap_or("none"),
            location: hit.location,
            bone: hit.bone,
        });
    }

    /// Damage scaled at its hit location: by the weapon's own table for
    /// bullets, grenades and projectiles, else by the game's.
    fn located(&mut self, amount: i32, location: i32, weapon: u32, world: &dyn FireWorld) -> i32 {
        let at = usize::try_from(location).unwrap_or(0);
        let own = (weapon != 0)
            .then(|| world.weapon(weapon))
            .flatten()
            .filter(|w| {
                WeaponDefView::new(&w.def, &w.variant).is_some_and(|d| {
                    matches!(
                        d.weapon_type(),
                        weapon_type::BULLET | weapon_type::PROJECTILE | weapon_type::GRENADE
                    )
                })
            });
        let scale = match own {
            Some(w) => w
                .location_damage
                .as_deref()
                .and_then(|t| t.get(at).copied()),
            None => world.hit_location_multiplier(location),
        };
        let Some(scale) = scale else {
            self.gaps.push(location_table_unknown());
            return amount;
        };
        (f64::from(amount) * f64::from(scale)) as i32
    }
}

/// Two players on one team.
fn same_team(attacker: i32, victim: Option<FireEntity>, world: &mut dyn FireWorld) -> bool {
    let Some(victim) = victim.filter(|v| v.kind == FireEntityKind::Player) else {
        return false;
    };
    let Some(mine) = world
        .entity(attacker)
        .filter(|a| a.kind == FireEntityKind::Player)
    else {
        return false;
    };
    mine.team.is_some() && mine.team == victim.team
}

/// The server clears a player's cooked fuse once the grenade took it.
fn clear_grenade_time(entity: i32, world: &mut dyn FireWorld) {
    let Some(mut player) = world.player(entity) else {
        return;
    };
    let (mut ps, mut last) = crate::movement::unpack(&mut player);
    ps.grenade_time_left = 0;
    crate::movement::pack(&mut player, &mut ps, &mut last);
    world.store_player(entity, &player);
}

/// A stacked weapon's barrels: its clip, scaled.
fn stack_count(def: &WeaponDefView<'_>, world: &dyn FireWorld) -> i32 {
    let clip = def.clip_size();
    if clip <= 1 {
        return clip;
    }
    let scale = dvar_f32(world, "player_clipSizeMultiplier", 1.0);
    let scaled = f64::from(clip as f32 * scale) + 9.313_226e-10;
    (scaled.round_ties_even() as i32).max(1)
}

/// How far a bullet reaches: a spread weapon's pellets to its last damage
/// range, other bullets `bulletrange`.
fn bullet_range(def: &WeaponDefView<'_>, world: &dyn FireWorld) -> f32 {
    if matches!(
        def.weapon_class(),
        weap_class::SPREAD | weap_class::PISTOL_SPREAD
    ) {
        def.min_damage_range()
    } else {
        dvar_f32(world, "bulletrange", 8192.0)
    }
}

/// A pellet's end and direction: its seeded point in the spread circle at
/// the end of its range; pellets laid on a level line when the weapon says
/// so.
fn bullet_end(
    seed: &mut u32,
    spread: f32,
    parms: &WeaponParms,
    range: f32,
    shot: i32,
    shots: i32,
    flat: bool,
) -> ([f32; 3], [f32; 3]) {
    let aim = (f64::from(spread * DEG2RAD).tan() as f32) * range;
    let (mut x, mut y) = seeded_in_circle(seed);
    if flat {
        y = 0.0;
        x = if shot == 0 || shots < 2 {
            0.0
        } else {
            let side = if shot % 2 == 0 { 1.0 } else { -1.0 };
            ((spread * 0.5) / (shots >> 1) as f32) * (((shot - 1) >> 1) + 1) as f32 * side
        };
    }
    let x = x * aim;
    let y = y * aim;
    let mut end = mad(parms.muzzle, parms.forward, range);
    end = mad(end, parms.right, x);
    end = mad(end, parms.up, y);
    (end, normalized(sub(end, parms.muzzle)))
}

/// A bullet's damage at the distance from its muzzle, along the weapon's
/// damage steps.
fn bullet_damage(def: &WeaponDefView<'_>, origin: [f32; 3], hit_pos: [f32; 3]) -> i32 {
    let range = |i: u32| def.def_f32(def::DAMAGE_RANGE + 4 * i);
    let damage = |i: u32| def.def_i32(def::DAMAGE + 4 * i);
    let d = sub(hit_pos, origin);
    let dist_sq = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    let near = range(0);
    if dist_sq <= near * near {
        return damage(0);
    }
    let far = range(5);
    if dist_sq > far * far {
        return damage(5);
    }
    let mut low = 0;
    for high in 1..6 {
        let r = range(high);
        if r < 0.001 {
            continue;
        }
        if r * r <= dist_sq {
            low = high;
            continue;
        }
        let dist = (d[1] * d[1] + d[0] * d[0] + d[2] * d[2]).sqrt();
        let base = range(low);
        let frac = (dist - base) / (range(high) - base);
        return (damage(low) as f32 * (1.0 - frac) + damage(high) as f32 * frac) as i32;
    }
    damage(0)
}

/// The next trace starts where this one hit, moved on through world
/// surfaces by `dist` along the surface's normal.
fn advance_trace(bp: &mut BulletParams, br: &BulletResults, dist: f32) -> bool {
    let id = match hit_id(&br.trace) {
        ENTITY_NONE if br.sky() => ENTITY_WORLD,
        id => id,
    };
    bp.ignore_entity = id;
    if id == ENTITY_WORLD && 0.0 < dist {
        let n = br.trace.normal;
        let dot = -(n[1] * bp.dir[1] + bp.dir[0] * n[0] + n[2] * bp.dir[2]);
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

/// The amplitude of a player's view bob along one axis (0 across, 1 up)
/// for the stance and aim it is in, at a speed.
fn bob_amplitude(ps: &PlayerState, axis: usize, speed: f32, world: &dyn FireWorld) -> f32 {
    let amp = |name: &str, default: [f32; 2]| dvar_vec2(world, name, default)[axis];
    let frac = ps.weapon_pos_frac;
    let amplitude = if ps.water_level >= SWIM_WATER_LEVEL {
        amp("bg_viewBobAmplitudeSwimming", [3.0, 2.0])
    } else if ps.pm_flags & pm_flags::DIVING != 0 {
        amp("bg_viewBobAmplitudeDtp", [0.002, 0.002]) * speed
    } else if ps.view_height_target == movement_t6::state::view_height::PRONE {
        amp("bg_viewBobAmplitudeProne", [0.08, 0.04]) * speed
    } else if ps.view_height_target == movement_t6::state::view_height::CROUCH {
        let hip = amp("bg_viewBobAmplitudeDucked", [0.0075, 0.0075]);
        let ads = amp("bg_viewBobAmplitudeDuckedAds", [0.0075, 0.0075]);
        ((1.0 - frac) * hip + ads * frac) * speed
    } else if ps.pm_flags & pm_flags::SPRINTING != 0 {
        amp("bg_viewBobAmplitudeSprinting", [0.02, 0.014]) * speed
    } else {
        let hip = amp("bg_viewBobAmplitudeStanding", [0.007, 0.007]);
        let ads = amp("bg_viewBobAmplitudeStandingAds", [0.007, 0.007]);
        ((1.0 - frac) * hip + ads * frac) * speed
    };
    let max = dvar_f32(world, "bg_bobMax", 8.0);
    if max < amplitude { max } else { amplitude }
}

/// A player's eye: origin and eye height, the view bob, and lean.
fn player_view_origin(ps: &PlayerState, level: i32, world: &dyn FireWorld) -> [f32; 3] {
    let mut eye = ps.origin;
    eye[2] += ps.view_height_current;
    let pi = f64::from(core::f32::consts::PI);
    let half = f64::from(ps.bob_cycle as u32 & 0xff) * f64::from(0.003_921_569_f32) * pi;
    let cycle = (half + half + f64::from(core::f32::consts::TAU)) as f32;
    let speed = if ps.pm_flags & pm_flags::LADDER == 0 {
        (ps.velocity[0] * ps.velocity[0] + ps.velocity[1] * ps.velocity[1]).sqrt()
    } else if level - ps.jump_time < 500 {
        0.0
    } else {
        ps.velocity[2]
    };
    let c = f64::from(cycle);
    let quarter = f64::from(core::f32::consts::FRAC_PI_2);
    let up = ((c + c).sin() + (4.0 * c + quarter).sin() * 0.2)
        * 0.75
        * f64::from(bob_amplitude(ps, 1, speed, world));
    eye[2] = (up + f64::from(eye[2])) as f32;
    let side = (c.sin() * f64::from(bob_amplitude(ps, 0, speed, world))) as f32;
    let (_, right, _) = angle_vectors(ps.viewangles);
    eye = mad(eye, right, side);
    if ps.e_flags & e_flags::VEHICLE_VIEW == 0 && ps.leanf != 0.0 {
        let lean = (2.0 - ps.leanf.abs()) * ps.leanf;
        let (_, right, _) = angle_vectors([0.0, ps.viewangles[1], lean * 16.0]);
        eye = mad(eye, right, lean * 20.0);
    }
    let floor = ps.origin[2] + EYE_FLOOR;
    if eye[2] < floor {
        eye[2] = floor;
    }
    eye
}
