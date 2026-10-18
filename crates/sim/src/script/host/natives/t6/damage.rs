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
const MOD_IMPACT: usize = 16;
const MOD_HEAD_SHOT: usize = 9;
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
    /// The game's own fire already scaled the damage at its hit location.
    pub located: bool,
    /// How far behind the match the hit was settled, in milliseconds.
    pub time_offset: i32,
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

/// The game's scale at a hit location, when the zones carry its table.
pub(crate) fn zone_location_scale(world: &World, location: usize) -> Option<f32> {
    world
        .resource::<RoundScript>()
        .restart
        .as_ref()?
        .raw_files
        .get(LOCATION_TABLE)?;
    global_scales(world).get(location).copied()
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
        Value::Int(hit.time_offset),
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

/// A projectile that revives on impact (the upgraded ballistic knives),
/// struck on a downed player by a player who is not down: the engine tells
/// the downed player who revived them and does no damage.
fn remote_revive(world: &mut World, target: &Value, client: u32, hit: &Hit) -> bool {
    if hit.means != MOD_IMPACT || !super::laststand::is_down(world, client) {
        return false;
    }
    let Some(reviver) = world
        .resource::<RoundScript>()
        .player_client_of(&hit.attacker)
    else {
        return false;
    };
    if super::laststand::is_down(world, reviver) {
        return false;
    }
    let revives = hit
        .weapon
        .as_deref()
        .and_then(|name| super::weapon::zone_weapon(world, name))
        .is_some_and(|weapon| weapon.revives_on_impact);
    if revives {
        raise(
            world,
            target.clone(),
            "remote_revive",
            vec![hit.attacker.clone()],
        );
    }
    revives
}

/// `G_Damage`: a player's, an actor's or another entity's own branch.
pub(super) fn damage(world: &mut World, target: &Value, hit: Hit) -> Result<(), String> {
    let id = entity_id(world, target)?;
    if let Some(client) = world.resource::<RoundScript>().player_client(id)
        && remote_revive(world, target, client, &hit)
    {
        return Ok(());
    }
    let takes_damage = !matches!(field(world, id, "takedamage"), Value::Int(0));
    if let Some(client) = world.resource::<RoundScript>().player_client(id) {
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
        // Half a second after going down, only a blow hurts.
        let now = super::super::super::players::now_ms(world);
        if hit.means != MOD_MELEE && super::laststand::in_grace(world, client, now) {
            return Ok(());
        }
        let weapon = weapon_of(world, &hit);
        let damage = if hit.means == MOD_MELEE || hit.located {
            hit.damage.max(1)
        } else {
            scaled(world, &hit, &weapon)
        };
        let args = callback_args(&hit, damage, &weapon);
        return callback(world, "codecallback_playerdamage", target.clone(), args);
    }
    if is_actor(world, target) {
        let alive = number(world, id, "health", 0.0) > 0.0;
        if !takes_damage || hit.damage < 1 || !alive || hit.hit_location == RIOTSHIELD {
            return Ok(());
        }
        let weapon = weapon_of(world, &hit);
        let damage = if hit.means == MOD_MELEE || hit.located {
            hit.damage.max(1)
        } else {
            scaled(world, &hit, &weapon)
        };
        let args = callback_args(&hit, damage, &weapon);
        return callback(world, "codecallback_actordamage", target.clone(), args);
    }
    other_damage(world, target, id, hit, None)
}

/// `G_Damage` for an entity that is neither a player nor an actor. A
/// vehicle takes damage only while it can, the damage is above 0 and it has
/// health left; what it then does is not run (`t6.damage.vehicles`). Any
/// other entity takes it while its `takedamage` is set: its health drops
/// (a `trigger_lookat`'s does not), it is notified `damage`, and at no
/// health (no lower than -999) `death` with the attacker. A hit at the riot
/// shield's location does nothing. `struck` is the struck bone's tag and the
/// entity's model, which the notify names. The entities scripts make (models, brush models, origins) have
/// no pain or death handler of their own.
fn other_damage(
    world: &mut World,
    target: &Value,
    id: u64,
    hit: Hit,
    struck: Option<(String, String)>,
) -> Result<(), String> {
    let takes_damage = matches!(field(world, id, "takedamage"), Value::Int(n) if n != 0);
    let vehicle = world
        .resource::<RoundScript>()
        .entities
        .get(&id)
        .is_some_and(|entity| entity.kind == EntityKind::Vehicle);
    if vehicle {
        let health = number(world, id, "health", 0.0);
        if !takes_damage || hit.damage < 1 || health <= 0.0 {
            return Ok(());
        }
        return Err(format!(
            "t6.damage.vehicles: {} damage to a vehicle",
            hit.damage
        ));
    }
    if hit.hit_location == RIOTSHIELD || !takes_damage {
        return Ok(());
    }
    let attacker = super::super::super::players::damage_entity(world, Some(&hit.attacker));
    let weapon = weapon_of(world, &hit);
    let damage = hit.damage.max(1);
    let lookat = matches!(
        field(world, id, "classname"),
        Value::String(name) if name.as_ref() == "trigger_lookat"
    );
    let mut health = number(world, id, "health", 0.0) as i32;
    if !lookat {
        health -= damage;
    }
    let zero = [0.0; 3];
    raise(
        world,
        target.clone(),
        "damage",
        vec![
            Value::Int(damage),
            attacker.clone(),
            Value::Vector(hit.dir.unwrap_or(zero)),
            Value::Vector(hit.point.unwrap_or(zero)),
            Value::string(MEANS_OF_DEATH[hit.means]),
            Value::string(struck.as_ref().map_or("", |(tag, _)| tag.as_str())),
            Value::string(struck.as_ref().map_or("", |(_, model)| model.as_str())),
            Value::string(""),
            Value::string(&weapon),
            Value::Int(hit.dflags),
            attacker.clone(),
        ],
    );
    if health < 1 {
        health = health.max(HEALTH_FLOOR);
        raise(world, target.clone(), "death", vec![attacker]);
    }
    if !lookat {
        world
            .resource_mut::<RoundScript>()
            .set_object_field(id, "health", Value::Int(health));
    }
    Ok(())
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
    let mut attacker = entity_arg(world, args, 2);
    let inflictor = entity_arg(world, args, 3);
    if matches!(attacker, Value::Undefined)
        && let Value::Object(inf_id) = inflictor
        && let Some(owner) = world
            .get_resource::<super::extras::OwnerTeams>()
            .and_then(|ot| ot.0.get(&inf_id))
            .and_then(|(owner, _)| *owner)
    {
        attacker = Value::Object(owner);
    }
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
            located: false,
            time_offset: 0,
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
/// point, dir, hitloc, timeoffset, boneindex)`, as the engine's handler:
/// what the game's movement keeps of the hit (`game_move::hurt`), the
/// health, the `damage` notify; a player whose health runs out goes down and
/// the last stand callback runs, and one it leaves at no health dies. What
/// the handler also does for the client is listed under
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
    let weapon_index = super::weapon::weapon_index(world, &weapon)? as u32;
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
    let (damage, health) = {
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
        // A gunner's splash scale, the damage timer that slows the player,
        // and the hit's direction and kind are the movement's.
        let damage = crate::game_move::hurt(&mut frame, id, damage, means, weapon_index, dir)
            .unwrap_or(damage);
        let ps = frame.player_mut(id).expect("checked above");
        ps.health -= damage;
        (damage, ps.health)
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
        let now = super::super::super::players::now_ms(world);
        let down = super::laststand::state(world, client);
        down.down = true;
        down.grace_until_ms = now + LAST_STAND_GRACE_MS;
        super::laststand::enter_last_stand_move(world, id);
        callback(
            world,
            "codecallback_playerlaststand",
            receiver.clone(),
            vec![
                inflictor.clone(),
                attacker.clone(),
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
    if left >= 1 {
        return Ok(Value::Undefined);
    }
    // A dying player's health is -999 unless it lies between that and 0.
    if !(-999..0).contains(&left)
        && let Some(ps) = FrameWorld::from_world(world).player_mut(id)
    {
        ps.health = -999;
    }
    player_die(
        world,
        receiver,
        Death {
            inflictor,
            attacker,
            damage,
            means,
            weapon,
            weapon_index,
            dir,
            location,
            offset,
        },
    )?;
    Ok(Value::Undefined)
}

/// What a client going down is spared for: only a blow hurts them this
/// long after.
const LAST_STAND_GRACE_MS: i64 = 500;

/// The killing hit, as the engine's player death takes it.
struct Death {
    inflictor: Value,
    attacker: Value,
    damage: i32,
    means: usize,
    weapon: String,
    weapon_index: u32,
    dir: Option<[f32; 3]>,
    location: usize,
    offset: i32,
}

/// The engine's player death: what the player's own state keeps
/// (`game_move::die`), the `flared_or_tabuned_death` and `death` notifies,
/// the scripts' killed callback, and the body that takes damage still.
fn player_die(world: &mut World, receiver: &Value, death: Death) -> Result<(), String> {
    let client = super::super::player::player(world, receiver)?;
    let id = crate::ClientId(client);
    let refused = {
        let mut frame = FrameWorld::from_world(world);
        crate::game_move::die(&mut frame, id, death.means, death.weapon_index)
    };
    if let Some(Err(move_type)) = refused {
        diag::warn!(
            Sim,
            "Aborting player_die(). Client {client} has move type {move_type}"
        );
        return Ok(());
    }
    diag::info!(
        Sim,
        "player {client} dies: {} {} at {}",
        MEANS_OF_DEATH[death.means],
        death.weapon,
        HIT_LOCATIONS[death.location]
    );
    // No zombies script sets a vision lerp ratio, so no player dies flared.
    let poisoned = super::poison_overlay::poisoned(world, client);
    raise(
        world,
        receiver.clone(),
        "flared_or_tabuned_death",
        vec![
            death.attacker.clone(),
            Value::Int(0),
            Value::Int(i32::from(poisoned)),
        ],
    );
    raise(
        world,
        receiver.clone(),
        "death",
        vec![
            death.attacker.clone(),
            Value::Int(i32::from(death.means == MOD_HEAD_SHOT)),
            Value::string(&death.weapon),
        ],
    );
    {
        let tick = world.resource::<crate::step::StepRequest>().tick;
        let mut frame = FrameWorld::from_world(world);
        let attacker = frame
            .ecs()
            .resource::<RoundScript>()
            .player_client_of(&death.attacker)
            .map(crate::ClientId);
        crate::script_player::script_death(&mut frame, tick, id, attacker);
    }
    callback(
        world,
        "codecallback_playerkilled",
        receiver.clone(),
        vec![
            death.inflictor,
            death.attacker,
            Value::Int(death.damage),
            Value::string(MEANS_OF_DEATH[death.means]),
            Value::string(&death.weapon),
            Value::Vector(death.dir.unwrap_or([0.0; 3])),
            Value::string(HIT_LOCATIONS[death.location]),
            Value::Int(death.offset),
            // The death animation's length: the player animation script is
            // not played (`t6.player.anim_events`).
            Value::Int(0),
        ],
    )?;
    // The body still takes damage; its contents are a corpse's.
    let id = entity_id(world, receiver)?;
    world
        .resource_mut::<RoundScript>()
        .set_object_field(id, "takedamage", Value::Int(1));
    Ok(())
}

/// The contents the engine's corpse think gives a dead body.
const CORPSE_CONTENTS: i32 = 0x0400_0000;

/// A shot or blast the game's own weapon rules landed on a player or one of
/// the game's actors: the rest of the game's `G_Damage` (the rules scaled
/// the damage at its hit location already), with the rules' damage, means
/// and weapon, at the struck bone's hit location (its model's
/// `partClassification`).
/// The name of the bone a shot struck on a script entity's model.
fn struck_tag(
    world: &mut World,
    model: crate::ScriptModelId,
    bone: Option<usize>,
) -> Option<String> {
    let frame = FrameWorld::from_world(world);
    frame
        .entity_collision_capabilities()
        .iter()
        .find(|row| row.owner.script_model() == Some(model))
        .and_then(|row| row.dobj.as_ref())
        .and_then(|dobj| dobj.bone_name(bone?))
        .map(str::to_owned)
}

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
    let other = !is_player && !is_actor(world, &target);
    let kind = world
        .resource::<RoundScript>()
        .entities
        .get(&object)
        .map(|entity| entity.kind.clone());
    // Origins' giant robots keep IW4L's own rules (`t6_zombies`).
    let robot = super::super::super::t6_zombies::is_robot(world, object);
    if other && (robot || !matches!(kind, Some(EntityKind::Map | EntityKind::Spawned))) {
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
    let (hit_bone, hit_model) = (hit.bone, hit.target);
    let hit = Hit {
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
        // The game's own fire scaled it at the hit location already.
        located: true,
        time_offset: hit.time_offset,
    };
    let result = if other {
        let tag = struck_tag(world, hit_model, hit_bone);
        let model = match field(world, object, "model") {
            Value::String(model) => model.to_string(),
            _ => String::new(),
        };
        other_damage(world, &target, object, hit, tag.map(|tag| (tag, model)))
    } else {
        damage(world, &target, hit)
    };
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
