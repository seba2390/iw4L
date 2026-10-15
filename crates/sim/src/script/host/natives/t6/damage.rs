//! Black Ops 2's damage: `G_Damage` for players and actors, `dodamage`,
//! `finishactordamage`, `finishplayerdamage`, and what an actor does when
//! it is hurt or killed.

use super::super::super::entities::EntityKind;
use super::super::engine::entity_id;
use super::movement::{self, Script, actor, field};
use super::{float, int, string, vector};
use crate::frame::FrameWorld;
use crate::script::runtime::{raise, run_now};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;

/// The game's hit locations, by its numbers.
pub(super) const HIT_LOCATIONS: [&str; 21] = [
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
pub(super) const RIOTSHIELD: usize = 20;
/// `dodamage`'s hit location when it names none.
const DEFAULT_HIT_LOCATION: usize = 2;
/// The game's means of death, by its numbers.
pub(super) const MEANS_OF_DEATH: [&str; 21] = [
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
pub(super) const MOD_MELEE: usize = 7;
/// Weapon types whose own hit-location scales apply: bullet, grenade and
/// projectile.
const LOCATION_SCALED_TYPES: [i32; 3] = [0, 1, 2];
const LOCATION_TABLE: &str = "info/mp_lochit_dmgtable";
const LOCATION_TABLE_HEADER: &str = "LOCDMGTABLE";
/// A dead actor's health goes no lower.
const HEALTH_FLOOR: i32 = -999;
const CALLBACKS: &str = "maps/mp/gametypes_zm/_callbacksetup";

/// One `G_Damage`: who, from where, how much and how.
pub(super) struct Hit {
    pub inflictor: Value,
    pub attacker: Value,
    pub dir: Option<[f32; 3]>,
    pub point: Option<[f32; 3]>,
    pub damage: i32,
    pub dflags: i32,
    pub means: usize,
    /// None when the hit names no weapon: the inflictor's, else the
    /// attacker's, is used.
    pub weapon: Option<String>,
    pub hit_location: usize,
}

fn defined(value: &Value) -> Option<&Value> {
    (!matches!(value, Value::Undefined)).then_some(value)
}

fn number(world: &mut World, id: u64, name: &str, default: f32) -> f32 {
    match field(world, id, name) {
        Value::Int(n) => n as f32,
        Value::Float(f) => f,
        _ => default,
    }
}

pub(super) fn is_actor(world: &World, value: &Value) -> bool {
    world
        .resource::<RoundScript>()
        .entity(value)
        .is_some_and(|(_, entity)| entity.kind == EntityKind::Actor)
}

/// The weapon an entity holds: a player's current weapon, an actor's own.
fn held_weapon(world: &mut World, value: &Value) -> Option<String> {
    let id = entity_id(world, value).ok()?;
    if let Some(client) = world.resource::<RoundScript>().player_client(id) {
        let frame = FrameWorld::from_world(world);
        let weapon = frame.player(crate::ClientId(client))?.weapon;
        return Some(frame.weapon_script_name(weapon).to_owned());
    }
    match field(world, id, "weapon") {
        Value::String(name) if !name.is_empty() => Some(name.to_string()),
        _ => None,
    }
}

fn weapon_of(world: &mut World, hit: &Hit) -> String {
    hit.weapon
        .clone()
        .or_else(|| defined(&hit.inflictor).and_then(|v| held_weapon(world, &v.clone())))
        .or_else(|| defined(&hit.attacker).and_then(|v| held_weapon(world, &v.clone())))
        .unwrap_or_else(|| "none".to_owned())
}

/// The global scales by hit location: 1, the riot shield 0, then what the
/// zone's `info/mp_lochit_dmgtable` sets.
fn global_scales(world: &World) -> [f32; 21] {
    let mut scales = [1.0; 21];
    scales[RIOTSHIELD] = 0.0;
    let text = world
        .resource::<RoundScript>()
        .restart
        .as_ref()
        .and_then(|plan| plan.raw_files.get(LOCATION_TABLE).cloned());
    if let Some(text) = text {
        let mut words = text.trim().split('\\');
        if words.next() == Some(LOCATION_TABLE_HEADER) {
            while let (Some(key), Some(value)) = (words.next(), words.next()) {
                if let Some(index) = HIT_LOCATIONS
                    .iter()
                    .position(|name| name.eq_ignore_ascii_case(key))
                    && let Ok(value) = value.trim().parse::<f32>()
                {
                    scales[index] = value;
                }
            }
        }
    }
    scales
}

/// A hit's damage at its location: the weapon's own scale for bullets,
/// grenades and projectiles, else the global one; never below 1.
fn scaled(world: &World, hit: &Hit, weapon: &str) -> i32 {
    // Weapon 0 (`none`) has no scales of its own.
    let own = (!weapon.eq_ignore_ascii_case("none"))
        .then(|| super::weapon::zone_weapon(world, weapon))
        .flatten()
        .filter(|w| LOCATION_SCALED_TYPES.contains(&w.weap_type))
        .and_then(|w| w.location_damage.get(hit.hit_location).copied());
    let scale = own.unwrap_or_else(|| global_scales(world)[hit.hit_location.min(RIOTSHIELD)]);
    ((hit.damage as f32 * scale) as i32).max(1)
}

fn callback_args(hit: &Hit, damage: i32, weapon: &str) -> Vec<Value> {
    vec![
        hit.inflictor.clone(),
        hit.attacker.clone(),
        Value::Int(damage),
        Value::Int(hit.dflags),
        Value::string(MEANS_OF_DEATH[hit.means]),
        Value::string(weapon),
        hit.point.map_or(Value::Undefined, Value::Vector),
        hit.dir.map_or(Value::Undefined, Value::Vector),
        Value::string(HIT_LOCATIONS[hit.hit_location.min(RIOTSHIELD)]),
        Value::Int(0),
        Value::Int(0),
    ]
}

fn callback(
    world: &mut World,
    name: &str,
    receiver: Value,
    args: Vec<Value>,
) -> Result<(), String> {
    let entry = format!("{CALLBACKS}::{name}");
    let now = super::super::super::players::now_ms(world);
    run_now(world, &entry, receiver, args, now).map_err(|fault| format!("{entry}: {fault:?}"))
}

/// `G_Damage`: a player's or an actor's own branch; other entities' damage
/// is not done (`t6.damage.entities`).
pub(super) fn damage(world: &mut World, target: &Value, hit: Hit) -> Result<(), String> {
    let id = entity_id(world, target)?;
    let takes_damage = !matches!(field(world, id, "takedamage"), Value::Int(0));
    if world.resource::<RoundScript>().player_client(id).is_some() {
        let playing = matches!(
            field(world, id, "sessionstate"),
            Value::String(state) if state.as_ref() == "playing"
        );
        let alive = number(world, id, "health", 0.0) > 0.0;
        // The script's invulnerability, or the `god` cheat.
        let god = world
            .resource::<RoundScript>()
            .player_client(id)
            .is_some_and(|client| {
                super::player::is_invulnerable(world, client)
                    || crate::script_player::god_mode(
                        &FrameWorld::from_world(world),
                        crate::ClientId(client),
                    )
            });
        if !takes_damage
            || god
            || hit.damage < 1
            || hit.hit_location == RIOTSHIELD
            || !playing
            || !alive
        {
            return Ok(());
        }
        let weapon = weapon_of(world, &hit);
        let damage = if hit.means == MOD_MELEE {
            hit.damage.max(1)
        } else {
            scaled(world, &hit, &weapon)
        };
        let args = callback_args(&hit, damage, &weapon);
        return callback(world, "codecallback_playerdamage", target.clone(), args);
    }
    if is_actor(world, target) {
        let alive = number(world, id, "health", 0.0) > 0.0;
        if !takes_damage || hit.damage < 1 || !alive {
            return Ok(());
        }
        let weapon = weapon_of(world, &hit);
        let damage = if hit.means == MOD_MELEE {
            hit.damage.max(1)
        } else {
            scaled(world, &hit, &weapon)
        };
        let args = callback_args(&hit, damage, &weapon);
        return callback(world, "codecallback_actordamage", target.clone(), args);
    }
    Err(format!(
        "t6.damage.entities: {} damage to an entity that is neither a player nor an actor",
        hit.damage
    ))
}

fn means_arg(args: &[Value], index: usize) -> Result<usize, String> {
    let name = string(args, index)?;
    MEANS_OF_DEATH
        .iter()
        .position(|mod_name| *mod_name == name)
        .ok_or_else(|| format!("Unknown means of death \"{name}\"\n"))
}

fn hit_location_arg(args: &[Value], index: usize) -> Result<usize, String> {
    let name = string(args, index)?;
    Ok(HIT_LOCATIONS
        .iter()
        .position(|location| *location == name)
        .unwrap_or(0))
}

fn entity_arg(world: &World, args: &[Value], index: usize) -> Value {
    match args.get(index) {
        Some(value) if world.resource::<RoundScript>().entity(value).is_some() => value.clone(),
        Some(value)
            if world
                .resource::<RoundScript>()
                .player_client_of(value)
                .is_some() =>
        {
            value.clone()
        }
        _ => Value::Undefined,
    }
}

/// `dodamage(amount, origin, [attacker], [inflictor], [hitloc], [mod],
/// [dflags], [weapon])`: from the origin toward the entity's centre.
fn do_damage(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let amount = float(args, 0)?;
    let origin = vector(args, 1)?;
    if origin.iter().any(|c| !c.is_finite()) {
        return Err(format!(
            "Source Damage vector is invalid : {} {} {}",
            origin[0], origin[1], origin[2]
        ));
    }
    let attacker = entity_arg(world, args, 2);
    let inflictor = entity_arg(world, args, 3);
    let hit_location = if args.len() > 4 {
        hit_location_arg(args, 4)?
    } else {
        DEFAULT_HIT_LOCATION
    };
    let means = if args.len() > 5 {
        means_arg(args, 5)?
    } else {
        0
    };
    let dflags = if args.len() > 6 { int(args, 6)? } else { 0 };
    let weapon = if args.len() > 7 {
        Some(string(args, 7)?)
    } else {
        None
    };
    let id = entity_id(world, receiver)?;
    let centre = movement::origin_of(world, id);
    let delta: [f32; 3] = std::array::from_fn(|i| centre[i] - origin[i]);
    let length = delta.iter().map(|c| c * c).sum::<f32>().sqrt();
    let dir = (length != 0.0).then(|| delta.map(|c| c / length));
    damage(
        world,
        receiver,
        Hit {
            inflictor,
            attacker,
            dir,
            point: Some(origin),
            damage: amount as i32,
            dflags,
            means,
            weapon,
            hit_location,
        },
    )?;
    Ok(Value::Undefined)
}

/// The yaw the damage came from, relative to the actor's facing.
fn damage_yaw(dir: [f32; 3], yaw: f32) -> i32 {
    let from = dir[1].atan2(dir[0]).to_degrees();
    let turns = (from - yaw) / 360.0;
    ((turns - (turns + 0.5).floor()) * 360.0) as i32
}

/// What the engine records on an actor it hurts or kills.
fn record(
    world: &mut World,
    id: u64,
    damage: i32,
    dir: [f32; 3],
    means: usize,
    location: usize,
    weapon: &str,
) {
    let yaw = match field(world, id, "angles") {
        Value::Vector(v) => v[1],
        _ => 0.0,
    };
    let mut runtime = world.resource_mut::<RoundScript>();
    runtime.set_object_field(id, "damagetaken", Value::Int(damage));
    runtime.set_object_field(id, "damagedir", Value::Vector(dir));
    runtime.set_object_field(id, "damageyaw", Value::Int(damage_yaw(dir, yaw)));
    runtime.set_object_field(id, "damagelocation", Value::string(HIT_LOCATIONS[location]));
    runtime.set_object_field(id, "damagemod", Value::string(MEANS_OF_DEATH[means]));
    runtime.set_object_field(id, "damageweapon", Value::string(weapon));
}

/// `finishactordamage(inflictor, attacker, damage, dflags, mod, weapon,
/// point, dir, hitloc, timeoffset, boneindex)`.
fn finish_actor_damage(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let id = actor(world, receiver)?;
    let damage = int(args, 2)?;
    if damage < 1 {
        return Ok(Value::Undefined);
    }
    // Without an inflictor or attacker the engine takes the world entity.
    let inflictor = super::super::super::players::damage_entity(world, args.first());
    let attacker = super::super::super::players::damage_entity(world, args.get(1));
    let dflags = int(args, 3)?;
    let means = means_arg(args, 4)?;
    let weapon = string(args, 5)?;
    let point = vector(args, 6).ok();
    let dir = vector(args, 7).ok().map(|d| {
        let length = d.iter().map(|c| c * c).sum::<f32>().sqrt();
        if length > 0.0 {
            d.map(|c| c / length)
        } else {
            d
        }
    });
    let location = hit_location_arg(args, 8)?;
    let offset = int(args, 9)?;
    int(args, 10)?;
    let health = number(world, id, "health", 0.0) as i32;
    diag::debug!(Sim, "actor {id}: {damage} damage of {health} health");
    if health <= 0 {
        return Err("Trying to do damage to a actor that is already dead".into());
    }
    let left = health - damage;
    world.resource_mut::<RoundScript>().set_object_field(
        id,
        "health",
        Value::Int(left.max(HEALTH_FLOOR)),
    );
    let zero = [0.0; 3];
    raise(
        world,
        receiver.clone(),
        "damage",
        vec![
            Value::Int(damage),
            attacker.clone(),
            Value::Vector(dir.unwrap_or(zero)),
            Value::Vector(point.unwrap_or(zero)),
            Value::string(MEANS_OF_DEATH[means]),
            Value::string(""),
            Value::string(""),
            Value::string(""),
            Value::string(&weapon),
            Value::Int(dflags),
            attacker.clone(),
        ],
    );
    let dir = dir.unwrap_or(zero);
    if left < 1 {
        // The dead actor's body is a corpse: the engine gives it the corpse
        // contents, which bullets and blows pass through.
        if let Some(entity) = world.resource_mut::<RoundScript>().entities.get_mut(&id) {
            entity.contents = CORPSE_CONTENTS;
        }
        raise(world, receiver.clone(), "death", vec![attacker.clone()]);
        let recorded = defined(&attacker)
            .and_then(|a| held_weapon(world, &a.clone()))
            .unwrap_or_else(|| weapon.clone());
        record(world, id, damage, dir, means, location, &recorded);
        callback(
            world,
            "codecallback_actorkilled",
            receiver.clone(),
            vec![
                inflictor,
                attacker.clone(),
                Value::Int(damage),
                Value::string(MEANS_OF_DEATH[means]),
                Value::string(&weapon),
                Value::Vector(dir),
                Value::string(HIT_LOCATIONS[location]),
                Value::Int(offset),
            ],
        )?;
        // After the callback the engine's actor killed sets the sentient's
        // attacker (which the death animscript reads) and clears the
        // targetname.
        {
            let mut runtime = world.resource_mut::<RoundScript>();
            runtime.set_object_field(id, "attacker", attacker);
            runtime.set_object_field(id, "targetname", Value::Undefined);
        }
        movement::set_script(world, id, Script::Death)?;
        return Ok(Value::Undefined);
    }
    let weapon_name = weapon.clone();
    record(world, id, damage, dir, means, location, &weapon_name);
    let allows = matches!(field(world, id, "allowpain"), Value::Int(n) if n != 0);
    let blocking = matches!(field(world, id, "blockingpain"), Value::Int(n) if n != 0);
    let minimum = number(world, id, "minpaindamage", 0.0) as i32;
    if allows && !blocking && minimum < damage {
        movement::pain(world, id)?;
    }
    Ok(Value::Undefined)
}

/// `finishplayerdamage(inflictor, attacker, damage, dflags, mod, weapon,
/// point, dir, hitloc, timeoffset, boneindex)`, as the engine's handler: the
/// damage timer, the health, the `damage` notify; a player whose health runs
/// out goes down and the last stand callback runs, and one it leaves at no
/// health dies. What the handler also does for the client is listed under
/// `t6.match.last_stand`.
fn finish_player_damage(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let client = super::super::player::player(world, receiver)?;
    let id = crate::ClientId(client);
    let damage = int(args, 2)?;
    if damage < 1 || FrameWorld::from_world(world).client_meta(id).is_none() {
        return Ok(Value::Undefined);
    }
    // Without an inflictor or attacker the engine takes the world entity.
    let inflictor = super::super::super::players::damage_entity(world, args.first());
    let attacker = super::super::super::players::damage_entity(world, args.get(1));
    let dflags = int(args, 3)?;
    let means = means_arg(args, 4)?;
    let weapon = string(args, 5)?;
    let point = vector(args, 6).ok();
    let dir = vector(args, 7).ok().map(|d| {
        let length = d.iter().map(|c| c * c).sum::<f32>().sqrt();
        if length > 0.0 {
            d.map(|c| c / length)
        } else {
            d
        }
    });
    let location = hit_location_arg(args, 8)?;
    if location == RIOTSHIELD {
        return Ok(Value::Undefined);
    }
    let offset = int(args, 9)?;
    int(args, 10)?;
    let health = {
        let mut frame = FrameWorld::from_world(world);
        if frame
            .client_meta(id)
            .is_some_and(|meta| meta.lifecycle == crate::ClientLifecycle::Dead)
        {
            diag::warn!(Sim, "Trying to do damage to a client that is already dead");
            return Ok(Value::Undefined);
        }
        if frame.player(id).is_none() {
            return Ok(Value::Undefined);
        }
        // The damage timer, which slows the player, is the movement's.
        crate::game_move::damage_timer(&mut frame, id, Some(damage));
        let ps = frame.player_mut(id).expect("checked above");
        ps.health -= damage;
        ps.health
    };
    let zero = [0.0; 3];
    raise(
        world,
        receiver.clone(),
        "damage",
        vec![
            Value::Int(damage),
            attacker.clone(),
            Value::Vector(dir.unwrap_or(zero)),
            Value::Vector(point.unwrap_or(zero)),
            Value::string(MEANS_OF_DEATH[means]),
            Value::string(""),
            Value::string(""),
            Value::string(""),
            Value::string(&weapon),
            Value::Int(dflags),
            attacker.clone(),
        ],
    );
    if health >= 1 {
        return Ok(Value::Undefined);
    }
    // The handler lets a player go down while any playing client is not
    // down, so the victim, who is not, always may.
    if !super::laststand::is_down(world, client) {
        super::laststand::state(world, client).down = true;
        super::laststand::enter_last_stand_move(world, id);
        callback(
            world,
            "codecallback_playerlaststand",
            receiver.clone(),
            vec![
                inflictor,
                attacker,
                Value::Int(damage),
                Value::string(MEANS_OF_DEATH[means]),
                Value::string(&weapon),
                Value::Vector(dir.unwrap_or(zero)),
                Value::string(HIT_LOCATIONS[location]),
                Value::Int(offset),
                Value::Int(0),
            ],
        )?;
    }
    let left = FrameWorld::from_world(world)
        .player(id)
        .map_or(0, |ps| ps.health);
    if left < 1 {
        return Err(format!(
            "t6.damage.player_die: player {client} is left at {left} health and the player's die handler is not run"
        ));
    }
    Ok(Value::Undefined)
}

/// The contents the engine's corpse think gives a dead body.
const CORPSE_CONTENTS: i32 = 0x0400_0000;

/// A shot or blast IW4L's weapon rules landed on one of the game's actors:
/// the game's own `G_Damage`, with the rules' damage, means and weapon, at
/// the struck bone's hit location (its model's `partClassification`).
fn entity_hit(
    world: &mut World,
    object: u64,
    hit: &super::super::super::entity_damage::EntityHit,
) -> bool {
    let target = Value::Object(object);
    let is_player = world
        .resource::<RoundScript>()
        .player_client(object)
        .is_some();
    if !is_player && !is_actor(world, &target) {
        return false;
    }
    let attacker = match hit.attacker {
        Some(client) => super::super::super::players::player_object(world, client.0),
        None => Value::Undefined,
    };
    let weapon = crate::script_player::weapon_name(&FrameWorld::from_world(world), hit.weapon);
    let hit_location = match (hit.part, hit.hit_location) {
        (Some(part), _) if usize::from(part) < HIT_LOCATIONS.len() => usize::from(part),
        (_, Some(name)) => HIT_LOCATIONS.iter().position(|n| *n == name).unwrap_or(0),
        _ => 0,
    };
    let result = damage(
        world,
        &target,
        Hit {
            inflictor: attacker.clone(),
            attacker,
            dir: Some(hit.dir),
            point: Some(hit.point),
            damage: hit.amount,
            dflags: hit.flags,
            means: MEANS_OF_DEATH
                .iter()
                .position(|name| *name == hit.means)
                .unwrap_or(0),
            weapon: Some(weapon),
            hit_location,
        },
    );
    if let Err(fault) = result {
        diag::warn!(Sim, "actor {object}: {fault}");
    }
    true
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::Method;

    registry.on_entity_hit(entity_hit);
    registry.register(Method, "dodamage", do_damage);
    registry.register(Method, "finishactordamage", finish_actor_damage);
    registry.register(Method, "finishplayerdamage", finish_player_damage);
    registry.register(Method, "startragdoll", |world, receiver, _| {
        entity_id(world, receiver)?;
        Err("t6.actors.ragdoll: ragdoll physics are not simulated; the body stays where its death clip leaves it".into())
    });
}
