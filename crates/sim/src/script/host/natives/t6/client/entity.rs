//! Black Ops 2's client builtins: entities, the local client and its player
//! state, movers, animations, traces, fake entities and physics.

use super::super::{atof, float, g, int, string, type_name, vector};
use super::game::{
    ClientGame, LOCAL_CLIENT, client_entity, local_client, server_object, type_name as etype_name,
};
use crate::ClientId;
use crate::bullet_collision::{ColliderId, TraceOutcome};
use crate::frame::FrameWorld;
use crate::script::host::args::arg;
use crate::script::host::arrays::new_array;
use crate::script::host::entities::{EntityKind, Link};
use crate::script::host::mechanics::{Mechanics, Motion, MotionPath};
use crate::script::host::natives::engine;
use crate::script::runtime::{raise, to_text};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;
use std::sync::Arc;

/// The engine's entity type numbers (`centity_t.nextState.eType`).
const ETYPE_GENERAL: i32 = 0;
const ETYPE_PLAYER: i32 = 1;
const ETYPE_PLAYER_CORPSE: i32 = 2;
const ETYPE_MISSILE: i32 = 4;
const ETYPE_SCRIPTMOVER: i32 = 6;
const ETYPE_HELICOPTER: i32 = 12;
const ETYPE_PLANE: i32 = 13;
const ETYPE_VEHICLE: i32 = 14;
const ETYPE_VEHICLE_CORPSE: i32 = 15;
const ETYPE_ACTOR: i32 = 16;
const ETYPE_ACTOR_CORPSE: i32 = 18;
const ETYPE_ZBARRIER: i32 = 20;

/// The server's entity numbers sit below 1024; client-spawned entities start
/// at 1024.
const CLIENT_ENTITY_BASE: i32 = 1024;
/// The fake-entity bitset holds 768 slots per local client.
const MAX_FAKE_ENTITIES: i32 = 768;
/// The client's mystery box weapon table holds 32 entries.
const MAX_ZOMBIE_BOX_WEAPONS: usize = 32;
/// `hasperk` refuses receiver entity numbers at or above the 18 client slots.
const MAX_PERK_CLIENT_NUM: i32 = 18;

/// A bullet trace's content masks on the client: with characters, without
/// characters, and the water bit the fifth parameter clears.
const BULLET_CHARACTERS_MASK: u32 = 0x0280_df33;
const BULLET_WORLD_MASK: u32 = 0x0080_6833;
const CONTENTS_WATER: u32 = 32;

/// The client entity fields whose values are strings.
const CLIENT_STRING_KEYS: [&str; 6] = ["weapon", "targetname", "type", "model", "team", "name"];
/// The client entity fields whose values are not strings.
const CLIENT_NON_STRING_KEYS: [&str; 4] = ["origin", "angles", "eflags", "eflags2"];

/// The 63 perks the engine's `BG_GetPerkIndexForName` registers.
const PERKS: [&str; 63] = [
    "specialty_additionalprimaryweapon",
    "specialty_armorpiercing",
    "specialty_armorvest",
    "specialty_bulletaccuracy",
    "specialty_bulletdamage",
    "specialty_bulletflinch",
    "specialty_bulletpenetration",
    "specialty_deadshot",
    "specialty_delayexplosive",
    "specialty_detectexplosive",
    "specialty_disarmexplosive",
    "specialty_earnmoremomentum",
    "specialty_explosivedamage",
    "specialty_extraammo",
    "specialty_fallheight",
    "specialty_fastads",
    "specialty_fastequipmentuse",
    "specialty_fastladderclimb",
    "specialty_fastmantle",
    "specialty_fastmeleerecovery",
    "specialty_fastreload",
    "specialty_fasttoss",
    "specialty_fastweaponswitch",
    "specialty_finalstand",
    "specialty_fireproof",
    "specialty_flakjacket",
    "specialty_flashprotection",
    "specialty_gpsjammer",
    "specialty_grenadepulldeath",
    "specialty_healthregen",
    "specialty_holdbreath",
    "specialty_immunecounteruav",
    "specialty_immuneemp",
    "specialty_immunemms",
    "specialty_immunenvthermal",
    "specialty_immunerangefinder",
    "specialty_killstreak",
    "specialty_longersprint",
    "specialty_loudenemies",
    "specialty_marksman",
    "specialty_movefaster",
    "specialty_nomotionsensor",
    "specialty_noname",
    "specialty_nottargetedbyairsupport",
    "specialty_nokillstreakreticle",
    "specialty_nottargettedbysentry",
    "specialty_pin_back",
    "specialty_pistoldeath",
    "specialty_proximityprotection",
    "specialty_quickrevive",
    "specialty_quieter",
    "specialty_reconnaissance",
    "specialty_rof",
    "specialty_scavenger",
    "specialty_showenemyequipment",
    "specialty_stunprotection",
    "specialty_shellshock",
    "specialty_sprintrecovery",
    "specialty_showonradar",
    "specialty_stalker",
    "specialty_twogrenades",
    "specialty_twoprimaries",
    "specialty_unlimitedsprint",
];

/// The fake entities (`spawnfakeent`) in use on the local client, with their
/// origins (`setfakeentorg`).
#[derive(Resource, Default)]
struct FakeEntities(BTreeMap<i32, [f32; 3]>);

/// A fake entity's origin, while it is in use.
pub(crate) fn fake_origin(world: &World, slot: i32) -> Option<[f32; 3]> {
    world.get_resource::<FakeEntities>()?.0.get(&slot).copied()
}

/// The animation tree `scriptmodelsuseanimtree` gave client script models,
/// and the trees `useanimtree` set on individual client entities.
#[derive(Resource, Default)]
struct ClientAnimTrees {
    script_models: Option<Arc<str>>,
    entities: BTreeMap<u64, Arc<str>>,
}

/// Weapons registered by `addzombieboxweapon` and cleared by
/// `resetzombieboxweapons`.
#[derive(Resource, Default)]
struct ZombieBoxWeapons(Vec<(String, String, bool)>);

fn with_server<R>(world: &World, f: impl FnOnce(&mut World) -> R) -> Option<R> {
    crate::script::host::client_scripts::with_server(world, f)
}

fn now_ms(world: &World) -> i64 {
    world
        .get_resource::<crate::step::StepRequest>()
        .map_or(0, |r| i64::from(r.tick.0) * i64::from(crate::MATCH_TICK_MS))
}

/// The receiving client entity: its object id, its entity number, and its
/// `eType`.
pub(super) fn receiver_entity(world: &World, receiver: &Value) -> Result<(u64, i32, i32), String> {
    let runtime = world.resource::<RoundScript>();
    let (id, entity) = match runtime.entity(receiver) {
        Some((id, entity)) if entity.kind != EntityKind::HudElem => (id, entity),
        _ => return Err("not an entity".into()),
    };
    let etype = client_entity(world, receiver).map_or_else(
        || {
            if entity.classname.as_ref() == "scriptmover" {
                ETYPE_SCRIPTMOVER
            } else {
                ETYPE_GENERAL
            }
        },
        |(_, found)| found.etype,
    );
    Ok((id, entity.number, etype))
}

/// An entity parameter as `Scr_GetEntityRef` reads it.
fn entity_arg(world: &World, args: &[Value], index: usize) -> Result<(u64, i32, i32), String> {
    let value = arg(args, index)?;
    let Value::Object(id) = value else {
        return Err(format!("type {} is not an entity", type_name(value)));
    };
    let runtime = world.resource::<RoundScript>();
    let Some(entity) = runtime.entities.get(id) else {
        let kind = if runtime.objects.contains_key(id) {
            "struct"
        } else {
            "removed entity"
        };
        return Err(format!("type {kind} is not an entity"));
    };
    if entity.kind == EntityKind::HudElem || !runtime.live(id) {
        return Err("type removed entity is not an entity".into());
    }
    receiver_entity(world, value)
}

/// Whether the receiver is the local player in the current snapshot
/// (`CG_GetLocalClientNumForEnt` != -1).
fn is_local_player(world: &World, receiver: &Value) -> bool {
    let Some((number, entity)) = client_entity(world, receiver) else {
        return false;
    };
    entity.etype == ETYPE_PLAYER
        && world
            .get_resource::<ClientGame>()
            .is_some_and(|game| game.has_snapshot && game.local_player == Some(number))
}

fn vector_field(world: &mut World, id: u64, name: &str) -> [f32; 3] {
    match world.resource_mut::<RoundScript>().object_field(id, name) {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    }
}

fn string_field(world: &mut World, id: u64, name: &str) -> String {
    match world.resource_mut::<RoundScript>().object_field(id, name) {
        Value::String(s) => s.to_string(),
        _ => String::new(),
    }
}

/// Whether the client entity has a DObj (a model or a character/vehicle body).
fn entity_has_dobj(world: &mut World, id: u64, etype: i32) -> bool {
    if matches!(
        etype,
        ETYPE_PLAYER
            | ETYPE_PLAYER_CORPSE
            | ETYPE_MISSILE
            | 11
            | ETYPE_HELICOPTER
            | ETYPE_PLANE
            | ETYPE_VEHICLE
            | ETYPE_ACTOR
            | ETYPE_ZBARRIER
    ) {
        return true;
    }
    !string_field(world, id, "model").is_empty()
}

/// Whether the client entity has an animation tree attached.
fn entity_has_animtree(world: &World, id: u64, etype: i32) -> bool {
    if matches!(
        etype,
        ETYPE_PLAYER | ETYPE_PLAYER_CORPSE | ETYPE_ACTOR | ETYPE_ACTOR_CORPSE
    ) {
        return true;
    }
    let Some(trees) = world.get_resource::<ClientAnimTrees>() else {
        return false;
    };
    trees.entities.contains_key(&id) || trees.script_models.is_some()
}

fn animtree_arg(args: &[Value], index: usize) -> Result<Arc<str>, String> {
    match arg(args, index)? {
        Value::AnimationTree(tree) => Ok(tree.clone()),
        other => Err(format!("type {} is not an animtree", type_name(other))),
    }
}

fn anim_arg(args: &[Value], index: usize) -> Result<Arc<str>, String> {
    match arg(args, index)? {
        Value::Animation { name, .. } => Ok(name.clone()),
        other => Err(format!("type {} is not an anim", type_name(other))),
    }
}

/// The local player's object in `ClientGame`, when present in the snapshot.
fn local_player_object(world: &World) -> Option<Value> {
    let game = world.get_resource::<ClientGame>()?;
    let number = game.local_player?;
    game.entities
        .get(&number)
        .map(|entity| Value::Object(entity.object))
}

/// A client object for a server entity number, when the snapshot carries it.
fn client_object_for_number(world: &World, number: i32) -> Option<Value> {
    world
        .get_resource::<ClientGame>()?
        .entities
        .get(&number)
        .map(|entity| Value::Object(entity.object))
}

/// Checks whether a weapon exists on the server.
fn has_weapon_named(world: &World, name: &str) -> bool {
    with_server(world, |server| {
        super::super::weapon::weapon_index(server, name)
            .ok()
            .is_some_and(|index| index != 0)
    })
    .unwrap_or(false)
}

/// Looks up a weapon's definition on the server.
fn zone_weapon(world: &World, name: &str) -> Option<Arc<asset_core::ZoneWeapon>> {
    with_server(world, |server| {
        let index = super::super::weapon::weapon_index(server, name).ok()?;
        (index != 0).then(|| super::super::weapon::zone_weapon(server, name))?
    })
    .flatten()
}

/// Checks whether `isalive` accepts an entity's `eType`, then answers
/// whether it lives.
fn check_alive(
    world: &World,
    id: u64,
    number: i32,
    etype: i32,
    method: bool,
) -> Result<Value, String> {
    if !matches!(
        etype,
        ETYPE_PLAYER
            | ETYPE_HELICOPTER
            | ETYPE_PLANE
            | ETYPE_VEHICLE
            | ETYPE_VEHICLE_CORPSE
            | ETYPE_ACTOR
    ) {
        return Err("IsAlive called on the wrong kind of entity".into());
    }
    if method {
        return Ok(Value::Int(1));
    }
    let alive = with_server(world, |server| {
        if etype == ETYPE_PLAYER {
            let frame = FrameWorld::from_world(server);
            let client = ClientId(number as u32);
            return frame.player(client).is_some_and(|ps| ps.health > 0)
                && frame.client_meta(client).is_some_and(|meta| {
                    meta.lifecycle == crate::match_state::ClientLifecycle::Alive
                });
        }
        let Some(server_id) = server_object(server, number) else {
            return false;
        };
        match server
            .resource_mut::<RoundScript>()
            .object_field(server_id, "health")
        {
            Value::Int(h) => h > 0,
            Value::Float(h) => h > 0.0,
            _ => true,
        }
    })
    .unwrap_or_else(|| world.resource::<RoundScript>().live(&id));
    Ok(Value::Int(alive.into()))
}

/// The client entities whose string field `key` equals `value`, in entity
/// number order.
fn matching_client_entities(world: &mut World, value: &str, key: &str) -> Vec<u64> {
    let mut pairs: Vec<(i32, u64)> = world
        .resource::<RoundScript>()
        .entities
        .iter()
        .filter(|(id, entity)| {
            entity.kind != EntityKind::HudElem && world.resource::<RoundScript>().live(id)
        })
        .map(|(id, entity)| (entity.number, *id))
        .collect();
    pairs.sort_unstable_by_key(|(number, _)| *number);
    let mut matches = Vec::new();
    let mut runtime = world.resource_mut::<RoundScript>();
    for (_, id) in pairs {
        if let Value::String(text) = runtime.object_field(id, key)
            && &*text == value
        {
            matches.push(id);
        }
    }
    matches
}

/// A tag's world origin and axis on a client entity, with the client
/// handler's error texts.
fn client_tag_pose(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<([f32; 3], [[f32; 3]; 3]), String> {
    let (id, number, etype) = receiver_entity(world, receiver)?;
    let tag = string(args, 0)?;
    let origin = vector_field(world, id, "origin");
    let angles = vector_field(world, id, "angles");
    if !entity_has_dobj(world, id, etype) {
        return Err(format!(
            "entity has no model defined ({} {} {})",
            g(origin[0]),
            g(origin[1]),
            g(origin[2])
        ));
    }
    let model = string_field(world, id, "model");
    if client_entity(world, receiver).is_some()
        && let Some(result) = with_server(world, |server| {
            let server_id = server_object(server, number)?;
            if let Some(client) = server.resource::<RoundScript>().player_client(server_id) {
                let frame = FrameWorld::from_world(server);
                if let Some(ps) = frame.player(ClientId(client)) {
                    if tag.eq_ignore_ascii_case("tag_origin") {
                        return Some(Ok((
                            ps.origin,
                            math_iw4::angles_to_axis([0.0, ps.viewangles[1], 0.0]),
                        )));
                    }
                    if tag.eq_ignore_ascii_case("tag_flash")
                        || tag.eq_ignore_ascii_case("tag_weapon")
                        || tag.eq_ignore_ascii_case("tag_weapon_right")
                    {
                        let eye = [
                            ps.origin[0],
                            ps.origin[1],
                            ps.origin[2] + ps.view_height_current,
                        ];
                        return Some(Ok((eye, math_iw4::angles_to_axis(ps.viewangles))));
                    }
                }
            }
            match crate::script::host::presence::tag_pose(server, server_id, &tag) {
                Some(Some(pose)) => Some(Ok(pose)),
                Some(None) => Some(Err(format!(
                    "tag '{tag}' does not exist in model '{model}' (or any attached submodels)"
                ))),
                None => None,
            }
        })
        .flatten()
    {
        return result;
    }
    if tag.eq_ignore_ascii_case("tag_origin") {
        return Ok((origin, math_iw4::angles_to_axis(angles)));
    }
    let posed = with_server(world, |server| {
        let capability = FrameWorld::from_world(server)
            .model_capability(&model)
            .flatten()?;
        let dobj = crate::AuthorityDObjState::at_pose(&model, Some(capability), origin, angles);
        let Some(matrix) = dobj.tag_world_matrix(&tag) else {
            return Some(Err(format!(
                "tag '{tag}' does not exist in model '{model}' (or any attached submodels)"
            )));
        };
        let axis = |v: glam::Vec4| v.truncate().normalize_or_zero().to_array();
        Some(Ok((
            matrix.w_axis.truncate().to_array(),
            [
                axis(matrix.x_axis),
                axis(matrix.y_axis),
                axis(matrix.z_axis),
            ],
        )))
    })
    .flatten();
    match posed {
        Some(result) => result,
        None => Err(format!(
            "tag '{tag}' does not exist in model '{model}' (or any attached submodels)"
        )),
    }
}

/// The mover time parameters (`total`, `accel`, `decel`) as the client mover
/// handlers check and scale them.
fn mover_times(args: &[Value]) -> Result<(f32, f32, f32), String> {
    let mut time = float(args, 1)?;
    if time <= 0.0 {
        return Err("total time must be positive".into());
    }
    if time < 0.001 {
        time = 0.001;
    }
    let accel = if args.len() > 2 {
        let a = float(args, 2)?;
        if a < 0.0 {
            return Err("accel time must be nonnegative".into());
        }
        a
    } else {
        0.0
    };
    let decel = if args.len() > 3 {
        let d = float(args, 3)?;
        if d < 0.0 {
            return Err("decel time must be nonnegative".into());
        }
        d
    } else {
        0.0
    };
    if accel + decel > time {
        let scaled = time * 1.000_000_5;
        if accel + decel > scaled {
            return Err("accel time plus decel time is greater than total time".into());
        }
        time = scaled;
    }
    Ok((time, accel, decel))
}

fn normalize_delta_180(delta: f32) -> f32 {
    let turns = delta * (1.0 / 360.0);
    (turns - (turns + 0.5).floor()) * 360.0
}

fn rotate_axis(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    axis: usize,
) -> Result<Value, String> {
    let (id, _, _) = receiver_entity(world, receiver)?;
    let delta = float(args, 0)?;
    let (time, accel, decel) = mover_times(args)?;
    let from = vector_field(world, id, "angles");
    let mut to = from;
    to[axis] += delta;
    let now = now_ms(world);
    let duration_ms = ((time * 1000.0) as i32).max(1) as i64;
    world.resource_mut::<Mechanics>().start(
        id,
        Motion {
            field: "angles",
            path: MotionPath::Linear {
                from,
                to,
                accel,
                decel,
            },
            start_ms: now,
            duration_ms,
            done: "rotatedone",
        },
    );
    Ok(Value::Undefined)
}

/// `setanimrestart`, `setflaggedanim`, `setflaggedanimrestart` and
/// `animscripted`.
fn set_anim_internal(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    flagged: bool,
    scripted: bool,
) -> Result<Value, String> {
    let (id, _, etype) = receiver_entity(world, receiver)?;
    if !entity_has_animtree(world, id, etype) {
        if !entity_has_dobj(world, id, etype) {
            return Err("Client entity has no model or animtree.".into());
        }
        return Err("SetAnim*: Entity does not have an animtree.".into());
    }
    let mut at = 0;
    let flag_name = if flagged {
        if args.len() <= at {
            return Err("Not enough parameters.".into());
        }
        let name = string(args, 0)?;
        at = 1;
        Some(name)
    } else {
        None
    };
    if args.len() <= at {
        return Err("Not enough parameters.".into());
    }
    let anim = anim_arg(args, at)?;
    at += 1;
    if matches!(
        etype,
        ETYPE_PLAYER | ETYPE_PLAYER_CORPSE | ETYPE_ACTOR | ETYPE_ACTOR_CORPSE
    ) {
        return Err("SetAnim*: Can not be called on a player or an actor.".into());
    }
    let mut weight = 1.0f32;
    if at < args.len() {
        weight = float(args, at)?;
        if weight < 0.0 {
            return Err("must set nonnegative weight".into());
        }
        at += 1;
    }
    if at < args.len() {
        let goal_time = float(args, at)?;
        if goal_time < 0.0 {
            return Err("must set nonnegative goal time".into());
        }
        at += 1;
    }
    let mut rate = 1.0f32;
    if at < args.len() {
        rate = float(args, at)?;
        if rate < 0.0 {
            return Err("must set nonnegative rate".into());
        }
        at += 1;
    }
    if at < args.len() {
        return Err("too many parameters".into());
    }
    if !entity_has_dobj(world, id, etype) {
        return Err("No model exists.".into());
    }
    if flagged && weight == 0.0 {
        return Err("cannot flag anim since it has 0 effective goal weight".into());
    }
    // t6.csc.anim.tree: client XAnimTree blending and notetrack playback are
    // not stepped on client entities; scripted/flagged completion notifies
    // are scheduled from the clip duration when loaded.
    if let Some(entity) = world.resource_mut::<RoundScript>().entities.get_mut(&id) {
        entity.anim_op = Some(Some(anim.clone()));
    }
    let notify = if scripted {
        Some("single anim".to_owned())
    } else {
        flag_name
    };
    if let Some(notify) = notify
        && rate > 0.0
    {
        let duration = with_server(world, |server| {
            FrameWorld::from_world(server)
                .anim_clip_named(&anim)
                .map(|clip| (clip.duration(), clip.looping))
        })
        .flatten();
        if let Some((seconds, false)) = duration {
            let due = now_ms(world) + ((seconds / rate * 1000.0).round() as i64).max(1);
            world.resource_mut::<RoundScript>().timed_notifies.push((
                due,
                Value::Object(id),
                notify.into(),
                vec![Value::string("end")],
            ));
        }
    }
    Ok(Value::Undefined)
}

/// `setanimknob` and `setflaggedanimknob`.
fn set_anim_knob_internal(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    flagged: bool,
) -> Result<Value, String> {
    let (id, _, etype) = receiver_entity(world, receiver)?;
    if !entity_has_animtree(world, id, etype) {
        if !entity_has_dobj(world, id, etype) {
            return Err("Client entity has no model or animtree.".into());
        }
        return Err(
            "CScr_SetAnimKnobInternal called on an entity that doesn't have an anim tree".into(),
        );
    }
    let mut at = 0;
    if flagged {
        if args.len() <= at {
            return Err("Not enough parameters.".into());
        }
        string(args, 0)?;
        at = 1;
    }
    if args.len() <= at {
        return Err("Not enough parameters.".into());
    }
    let anim = anim_arg(args, at)?;
    at += 1;
    if args.len() <= at {
        return Err("Not enough parameters.".into());
    }
    let weight = float(args, at)?;
    if weight < 0.0 {
        return Err("must set nonnegative weight".into());
    }
    at += 1;
    if args.len() <= at {
        return Err("Not enough parameters.".into());
    }
    let goal_time = float(args, at)?;
    if goal_time < 0.0 {
        return Err("must set nonnegative goal time".into());
    }
    at += 1;
    if args.len() <= at {
        return Err("Not enough parameters.".into());
    }
    let rate = float(args, at)?;
    if rate < 0.0 {
        return Err("must set nonnegative rate".into());
    }
    at += 1;
    if at < args.len() {
        return Err("too many parameters".into());
    }
    if flagged && weight == 0.0 {
        return Err("cannot flag anim since it has 0 effective goal weight".into());
    }
    // t6.csc.anim.tree: client XAnimTree knob blending is not stepped on
    // client entities.
    if let Some(entity) = world.resource_mut::<RoundScript>().entities.get_mut(&id) {
        entity.anim_op = Some(Some(anim));
    }
    Ok(Value::Undefined)
}

/// Velocity vector of a client entity, from the server's player state for a
/// player or `Mechanics` otherwise.
fn entity_velocity(world: &World, id: u64, number: i32, etype: i32) -> [f32; 3] {
    if etype == ETYPE_PLAYER
        && let Some(velocity) = with_server(world, |server| {
            FrameWorld::from_world(server)
                .player(ClientId(number as u32))
                .map(|ps| ps.velocity)
        })
        .flatten()
    {
        return velocity;
    }
    let now = now_ms(world);
    let client_vel = world
        .get_resource::<Mechanics>()
        .map_or([0.0; 3], |m| m.velocity(id, now));
    if client_vel != [0.0; 3] {
        return client_vel;
    }
    with_server(world, |server| {
        let server_id = server_object(server, number)?;
        Some(server.resource::<Mechanics>().velocity(server_id, now))
    })
    .flatten()
    .unwrap_or([0.0; 3])
}

/// Deletes a client entity, raising `death` and `entityshutdown`.
fn delete_client_entity(world: &mut World, receiver: &Value) -> Result<Value, String> {
    let (id, number, _) = receiver_entity(world, receiver)?;
    raise(world, Value::Object(id), "death", Vec::new());
    raise(world, Value::Object(id), "entityshutdown", Vec::new());
    if let Some(mut game) = world.get_resource_mut::<ClientGame>()
        && game.entities.get(&number).is_some_and(|e| e.object == id)
    {
        game.entities.remove(&number);
    }
    world.resource_mut::<RoundScript>().request_delete(id);
    Ok(Value::Undefined)
}

/// The local client's box weapons (weapon, model, dual wield), as its
/// scripts listed them.
pub(crate) fn zombie_box_weapons(server: &World) -> Vec<(String, String, bool)> {
    server
        .get_resource::<crate::script::host::client_scripts::ClientScripts>()
        .and_then(|client| client.world.get_resource::<ZombieBoxWeapons>())
        .map(|list| list.0.clone())
        .unwrap_or_else(Vec::new)
}

/// The revive icon material each team's downed players show
/// (`setteamreviveicon`), by the engine's team number; kept for the
/// client's revive icons (`t6.csc.hud.team_revive_icon`).
#[derive(bevy_ecs::prelude::Resource, Default)]
pub(crate) struct TeamReviveIcons(pub(crate) BTreeMap<usize, String>);

pub(super) fn register(registry: &mut NativeRegistry) {
    // setteamreviveicon(team, material): the team's revive icon (an illegal
    // team is the engine's team error).
    registry.register(
        Namespace::Function,
        "setteamreviveicon",
        |world, _, args| {
            let team = super::super::team(args, 0)?;
            let material = super::super::string(args, 1)?;
            world
                .get_resource_or_insert_with(TeamReviveIcons::default)
                .0
                .insert(team, material.to_string());
            Ok(Value::Undefined)
        },
    );
    use Namespace::{Function, Method};

    // addzombieboxweapon(weapon, model, is_dual_wield): adds a weapon to the
    // client's 32-slot mystery box table.
    registry.register(Function, "addzombieboxweapon", |world, _, args| {
        let weapon = string(args, 0)?;
        let model = string(args, 1)?;
        let dual_wield = int(args, 2)? == 1;
        let count = world
            .get_resource_or_insert_with(ZombieBoxWeapons::default)
            .0
            .len();
        if count >= MAX_ZOMBIE_BOX_WEAPONS {
            return Ok(Value::Undefined);
        }
        if !has_weapon_named(world, &weapon) {
            return Err(format!(
                "AddZombieBoxWeapon: Failed to find weapon {weapon}"
            ));
        }
        // t6.csc.weapons.zombie_box: IW4L keeps the client's mystery box
        // weapon list; box cycling is driven by the scripts themselves.
        world
            .get_resource_or_insert_with(ZombieBoxWeapons::default)
            .0
            .push((weapon, model, dual_wield));
        Ok(Value::Undefined)
    });

    // resetzombieboxweapons(): clears the client's mystery box weapon table.
    registry.register(Function, "resetzombieboxweapons", |world, _, _| {
        world
            .get_resource_or_insert_with(ZombieBoxWeapons::default)
            .0
            .clear();
        Ok(Value::Undefined)
    });

    // allowroundanimation(allow): hands `{ name = "allow_round_animation", allow }` to the LUI root.
    registry.register(Function, "allowroundanimation", |world, _, args| {
        let allow = int(args, 0)?;
        world
            .get_resource_or_insert_with(ClientGame::default)
            .allow_round_animation
            .push(allow);
        Ok(Value::Undefined)
    });

    // usealternatereviveicon(use_alternate): selects `waypoint_revive_zm` (non-zero)
    // or `waypoint_revive` (zero) on the client.
    registry.register(Function, "usealternatereviveicon", |world, _, args| {
        let alt = int(args, 0)? != 0;
        world
            .get_resource_or_insert_with(ClientGame::default)
            .alternate_revive_icon = alt;
        // t6.csc.hud.alternate_revive_icon: ownerdraw 199 (`revive_hints`) does
        // not yet paint the client revive icon material.
        Ok(Value::Undefined)
    });

    // clienthassnapshot(localclientnum): whether the local client has
    // received a snapshot.
    registry.register(Function, "clienthassnapshot", |world, _, args| {
        local_client(args, 0)?;
        let has = world
            .get_resource::<ClientGame>()
            .is_some_and(|game| game.has_snapshot);
        Ok(Value::Int(has.into()))
    });

    // getservertime(localclientnum): the server time of the local client's
    // snapshot.
    registry.register(Function, "getservertime", |world, _, args| {
        local_client(args, 0)?;
        let time = world
            .get_resource::<ClientGame>()
            .map_or(0, |game| game.server_time);
        Ok(Value::Int(time))
    });

    // getactivelocalclients(): the number of active local clients (1 in IW4L).
    registry.register(Function, "getactivelocalclients", |_, _, _| {
        Ok(Value::Int(1))
    });

    // localclientactive(localclientnum): whether that local client is active.
    registry.register(Function, "localclientactive", |_, _, args| {
        Ok(Value::Int((int(args, 0)? == LOCAL_CLIENT).into()))
    });

    // getlocalplayer(localclientnum): the local client's player entity.
    registry.register(Function, "getlocalplayer", |world, _, args| {
        local_client(args, 0)?;
        Ok(local_player_object(world).unwrap_or(Value::Undefined))
    });

    // getlocalplayers(): array of active local players with a valid snapshot.
    registry.register(Function, "getlocalplayers", |world, _, _| {
        let mut players = Vec::new();
        if world
            .get_resource::<ClientGame>()
            .is_some_and(|game| game.has_snapshot)
            && let Some(player) = local_player_object(world)
        {
            players.push(player);
        }
        new_array(world, players)
    });

    // getnonpredictedlocalplayer(localclientnum): the local player entity
    // from the snapshot.
    registry.register(Function, "getnonpredictedlocalplayer", |world, _, args| {
        local_client(args, 0)?;
        Ok(local_player_object(world).unwrap_or(Value::Undefined))
    });

    // ent islocalplayer(): whether the entity is a local player.
    registry.register(Method, "islocalplayer", |world, receiver, _| {
        receiver_entity(world, receiver)?;
        Ok(Value::Int(is_local_player(world, receiver).into()))
    });

    // ent getlocalclientnumber(): the local client number of a local player
    // entity.
    registry.register(Method, "getlocalclientnumber", |world, receiver, _| {
        receiver_entity(world, receiver)?;
        if !is_local_player(world, receiver) {
            return Err("You can only call GetLocalClientNumber on players.".into());
        }
        Ok(Value::Int(LOCAL_CLIENT))
    });

    // ent isplayer(): whether the entity's eType is player (1).
    registry.register(Method, "isplayer", |world, receiver, _| {
        let (_, _, etype) = receiver_entity(world, receiver)?;
        Ok(Value::Int((etype == ETYPE_PLAYER).into()))
    });

    // isalive(ent) and ent isalive(): whether a player, actor or vehicle
    // lives; other entity types are an error.
    registry.register(Function, "isalive", |world, _, args| {
        if args.is_empty() {
            return Err("parameter 1 does not exist".into());
        }
        let Ok((id, number, etype)) = entity_arg(world, args, 0) else {
            return Err("IsAlive param isn't an entity.".into());
        };
        check_alive(world, id, number, etype, false)
    });
    registry.register(Method, "isalive", |world, receiver, _| {
        let (id, number, etype) = receiver_entity(world, receiver)?;
        check_alive(world, id, number, etype, true)
    });

    // getlocalclientpos(localclientnum): the local player's predicted origin.
    registry.register(Function, "getlocalclientpos", |world, _, args| {
        local_client(args, 0)?;
        let origin = with_server(world, |server| {
            FrameWorld::from_world(server)
                .player(ClientId(LOCAL_CLIENT as u32))
                .map(|ps| ps.origin)
        })
        .flatten()
        .unwrap_or([0.0; 3]);
        Ok(Value::Vector(origin))
    });

    // getlocalclientangles(localclientnum): the local player's predicted
    // viewangles.
    registry.register(Function, "getlocalclientangles", |world, _, args| {
        local_client(args, 0)?;
        let angles = with_server(world, |server| {
            FrameWorld::from_world(server)
                .player(ClientId(LOCAL_CLIENT as u32))
                .map(|ps| ps.viewangles)
        })
        .flatten()
        .unwrap_or([0.0; 3]);
        Ok(Value::Vector(angles))
    });

    // getlocalclienteyepos(localclientnum): the local player's eye position.
    registry.register(Function, "getlocalclienteyepos", |world, _, args| {
        local_client(args, 0)?;
        let eye = with_server(world, |server| {
            FrameWorld::from_world(server)
                .player(ClientId(LOCAL_CLIENT as u32))
                .map(|ps| {
                    [
                        ps.origin[0],
                        ps.origin[1],
                        ps.origin[2] + ps.view_height_current,
                    ]
                })
        })
        .flatten()
        .unwrap_or([0.0; 3]);
        Ok(Value::Vector(eye))
    });

    // getcamposbylocalclientnum(localclientnum): the local client's camera
    // origin.
    registry.register(Function, "getcamposbylocalclientnum", |world, _, args| {
        local_client(args, 0)?;
        let cam = with_server(world, |server| {
            crate::script_player::eye(
                &FrameWorld::from_world(server),
                ClientId(LOCAL_CLIENT as u32),
            )
        })
        .flatten()
        .unwrap_or([0.0; 3]);
        Ok(Value::Vector(cam))
    });

    // ent geteyeapprox(): the snapshot's player origin plus viewHeightCurrent.
    registry.register(Method, "geteyeapprox", |world, _, _| {
        let eye = with_server(world, |server| {
            FrameWorld::from_world(server)
                .player(ClientId(LOCAL_CLIENT as u32))
                .map(|ps| {
                    [
                        ps.origin[0],
                        ps.origin[1],
                        ps.origin[2] + ps.view_height_current,
                    ]
                })
        })
        .flatten()
        .unwrap_or([0.0; 3]);
        Ok(Value::Vector(eye))
    });

    // getcurrentweapon(localclientnum): the name of the local player's
    // selected weapon.
    registry.register(Function, "getcurrentweapon", |world, _, args| {
        local_client(args, 0)?;
        let name = with_server(world, |server| {
            let frame = FrameWorld::from_world(server);
            let ps = frame.player(ClientId(LOCAL_CLIENT as u32))?;
            if ps.weapon == 0 {
                Some("none".to_owned())
            } else {
                Some(crate::script_player::weapon_name(&frame, ps.weapon).to_owned())
            }
        })
        .flatten()
        .unwrap_or_else(|| "none".to_owned());
        Ok(Value::string(&name))
    });

    // getweaponammoclip(localclientnum, weapon): rounds in the local player's
    // clip for the named weapon.
    registry.register(Function, "getweaponammoclip", |world, _, args| {
        local_client(args, 0)?;
        let name = string(args, 1)?;
        let count = with_server(world, |server| {
            let frame = FrameWorld::from_world(server);
            let weapon = crate::script_player::weapon_named(&frame, &name).ok()?;
            Some(crate::script_player::ammo_clip(
                &frame,
                ClientId(LOCAL_CLIENT as u32),
                weapon,
            ))
        })
        .flatten()
        .unwrap_or(0);
        Ok(Value::Int(count))
    });

    // isthrowinggrenade(localclientnum): whether the local player's weapon
    // state is an offhand throw.
    registry.register(Function, "isthrowinggrenade", |world, _, args| {
        local_client(args, 0)?;
        let throwing = with_server(world, |server| {
            crate::script_player::weapon_activity(
                &FrameWorld::from_world(server),
                ClientId(LOCAL_CLIENT as u32),
            )
            .throwing_grenade
        })
        .unwrap_or(false);
        Ok(Value::Int(throwing.into()))
    });

    // ismeleeing(localclientnum): whether the local player's weapon state is
    // a melee attack.
    registry.register(Function, "ismeleeing", |world, _, args| {
        local_client(args, 0)?;
        let meleeing = with_server(world, |server| {
            crate::script_player::weapon_activity(
                &FrameWorld::from_world(server),
                ClientId(LOCAL_CLIENT as u32),
            )
            .meleeing
        })
        .unwrap_or(false);
        Ok(Value::Int(meleeing.into()))
    });

    // isonturret(localclientnum): whether the local player is mounted on a
    // turret.
    registry.register(Function, "isonturret", |world, _, args| {
        local_client(args, 0)?;
        let on_turret = with_server(world, |server| {
            FrameWorld::from_world(server)
                .player(ClientId(LOCAL_CLIENT as u32))
                .is_some_and(|ps| ps.e_flags & 768 != 0)
        })
        .unwrap_or(false);
        Ok(Value::Int(on_turret.into()))
    });

    // isweaponoverheating(localclientnum, [want_heat], [weapon]): whether the
    // local player's weapon is overheating, or its heat value when
    // `want_heat` is non-zero.
    registry.register(Function, "isweaponoverheating", |world, _, args| {
        local_client(args, 0)?;
        let want_heat = if args.len() >= 2 {
            int(args, 1)? != 0
        } else {
            false
        };
        let on_turret = with_server(world, |server| {
            FrameWorld::from_world(server)
                .player(ClientId(LOCAL_CLIENT as u32))
                .is_some_and(|ps| ps.e_flags & 768 != 0)
        })
        .unwrap_or(false);
        if on_turret {
            return Err("IsWeaponOverheating() not supported for turret weapons on client".into());
        }
        if args.len() >= 3 {
            let name = string(args, 2)?;
            if !has_weapon_named(world, &name) {
                return Ok(Value::Int(0));
            }
        }
        let mut forwarded = vec![Value::Int(want_heat.into())];
        if let Some(weapon) = args.get(2) {
            forwarded.push(weapon.clone());
        }
        let result = with_server(world, |server| {
            let player = server
                .resource::<RoundScript>()
                .players
                .get(&(LOCAL_CLIENT as u32))
                .map(|slot| slot.object)?;
            let native =
                NativeRegistry::black_ops_2().get(Namespace::Method, "isweaponoverheating")?;
            native(server, &Value::Object(player), &forwarded).ok()
        })
        .flatten();
        Ok(result.unwrap_or(if want_heat {
            Value::Float(0.0)
        } else {
            Value::Int(0)
        }))
    });

    // player hasperk(localclientnum, perk): whether the player has the named
    // engine perk.
    registry.register(Method, "hasperk", |world, receiver, args| {
        let (_, number, _) = receiver_entity(world, receiver)?;
        if !(0..MAX_PERK_CLIENT_NUM).contains(&number) {
            return Err(format!(
                "hasperk() can only be called on local players {number}"
            ));
        }
        let client = int(args, 0)?;
        if client >= 1 {
            return Err(
                "player hasperk(<localClientNum>, <perk>): localClientNum out of range".into(),
            );
        }
        let perk = string(args, 1)?;
        if !PERKS.iter().any(|known| known.eq_ignore_ascii_case(&perk)) {
            return Err(format!("Unknown perk: {perk}\n"));
        }
        let has = with_server(world, |server| {
            server
                .resource::<RoundScript>()
                .players
                .get(&(number as u32))
                .is_some_and(|slot| slot.perks.iter().any(|p| p.eq_ignore_ascii_case(&perk)))
        })
        .unwrap_or(false);
        Ok(Value::Int(has.into()))
    });

    // ent isburning(): whether the entity has the burning flag set.
    // ent isburning(): a player whose burning (`setburn`) runs, as the
    // client keeps it (t6.csc.player.burning: other entities' burn flag).
    registry.register(Method, "isburning", |world, receiver, _| {
        let (_, number, etype) = receiver_entity(world, receiver)?;
        let burning = etype == ETYPE_PLAYER
            && u32::try_from(number).is_ok_and(|client| {
                crate::script::host::client_scripts::with_server(world, |server| {
                    crate::script::host::natives::t6::flame_overlay::burning(server, client)
                })
                .unwrap_or(false)
            });
        Ok(Value::Int(burning.into()))
    });

    // player isdriving(localclientnum): whether the player is driving a
    // vehicle.
    registry.register(Method, "isdriving", |world, receiver, args| {
        let (_, number, etype) = receiver_entity(world, receiver)?;
        if etype != ETYPE_PLAYER || !(0..MAX_PERK_CLIENT_NUM).contains(&number) {
            return Err("IsDriving not called on a player.".into());
        }
        local_client(args, 0)?;
        // t6.csc.vehicles.driving: IW4L has no client vehicle occupancy state.
        Ok(Value::Int(0))
    });

    // ent islocalplayerweaponviewonlylinked(): whether the local player is
    // weapon-view-only linked.
    registry.register(
        Method,
        "islocalplayerweaponviewonlylinked",
        |world, receiver, _| {
            receiver_entity(world, receiver)?;
            Ok(Value::Int(0))
        },
    );

    // ent isremotecontrolling(): whether the entity has the remote-control
    // eFlag set.
    registry.register(Method, "isremotecontrolling", |world, receiver, _| {
        receiver_entity(world, receiver)?;
        Ok(Value::Int(0))
    });

    // isspectating(localclientnum, [not_dead_spectating]): whether the local
    // client is spectating.
    registry.register(Function, "isspectating", |world, _, args| {
        local_client(args, 0)?;
        if args.len() > 1 {
            int(args, 1)?;
        }
        let spectating = with_server(world, |server| {
            FrameWorld::from_world(server)
                .client_meta(ClientId(LOCAL_CLIENT as u32))
                .is_some_and(|meta| {
                    meta.lifecycle == crate::match_state::ClientLifecycle::Spectating
                })
        })
        .unwrap_or(false);
        Ok(Value::Int(spectating.into()))
    });

    // playerbeingspectated(localclientnum): the player entity the local
    // client is viewing.
    registry.register(Function, "playerbeingspectated", |world, _, args| {
        local_client(args, 0)?;
        Ok(local_player_object(world).unwrap_or(Value::Undefined))
    });

    // ent getinkillcam(localclientnum): whether the local client is in a
    // killcam.
    registry.register(Method, "getinkillcam", |world, receiver, args| {
        receiver_entity(world, receiver)?;
        local_client(args, 0)?;
        Ok(Value::Int(0))
    });

    // ent getmovementtype(): "", "walk", "run" or "sprint" from the entity's
    // speed squared against 0, 10000 and 28900.
    registry.register(Method, "getmovementtype", |world, receiver, _| {
        let (id, number, etype) = receiver_entity(world, receiver)?;
        let v = entity_velocity(world, id, number, etype);
        let speed_sq = if etype == ETYPE_PLAYER {
            v[0] * v[0] + v[1] * v[1] + v[2] * v[2]
        } else {
            v[0] * v[0] + v[1] * v[1]
        };
        let kind = if speed_sq <= 0.0 {
            ""
        } else if speed_sq < 10_000.0 {
            "walk"
        } else if speed_sq < 28_900.0 {
            "run"
        } else {
            "sprint"
        };
        Ok(Value::string(kind))
    });

    // ent getspeed(): the entity's speed in units per second.
    registry.register(Method, "getspeed", |world, receiver, _| {
        let (id, number, etype) = receiver_entity(world, receiver)?;
        let v = entity_velocity(world, id, number, etype);
        Ok(Value::Float(
            (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt(),
        ))
    });

    // ent isgrenadedud(): whether a missile entity has been marked a dud.
    registry.register(Method, "isgrenadedud", |world, receiver, _| {
        let (_, number, etype) = receiver_entity(world, receiver)?;
        if etype != ETYPE_MISSILE {
            return Ok(Value::Int(0));
        }
        let dud = crate::script::host::client_scripts::with_server(world, |server| {
            crate::game_fire::game_missile_is_dud(
                &crate::frame::FrameWorld::from_world(server),
                number,
            )
        })
        .unwrap_or(false);
        Ok(Value::Int(dud.into()))
    });

    // spawn(localclientnum, origin, [classname]): spawns a client entity; a
    // "script_model" or "script_origin" becomes a script mover (eType 6).
    registry.register(Function, "spawn", |world, _, args| {
        local_client(args, 0)?;
        let origin = vector(args, 1)?;
        let etype = if args.len() >= 3 {
            let class = string(args, 2)?;
            if class.eq_ignore_ascii_case("script_model")
                || class.eq_ignore_ascii_case("script_origin")
            {
                ETYPE_SCRIPTMOVER
            } else {
                ETYPE_GENERAL
            }
        } else {
            ETYPE_GENERAL
        };
        let kind_name = etype_name(etype);
        let mut runtime = world.resource_mut::<RoundScript>();
        let id = runtime.create_entity(EntityKind::Spawned, kind_name)?;
        runtime.set_object_field(id, "origin", Value::Vector(origin));
        runtime.set_object_field(id, "angles", Value::Vector([0.0; 3]));
        runtime.set_object_field(id, "type", Value::string(kind_name));
        Ok(Value::Object(id))
    });

    // getent(localclientnum, value, key): the single client entity whose
    // string field `key` matches `value`.
    registry.register(Function, "getent", |world, _, args| {
        local_client(args, 0)?;
        let value = string(args, 1)?;
        let key = string(args, 2)?.to_ascii_lowercase();
        if CLIENT_NON_STRING_KEYS.contains(&key.as_str()) {
            return Err("key is not internally a string".into());
        }
        if !CLIENT_STRING_KEYS.contains(&key.as_str()) {
            return Ok(Value::Undefined);
        }
        let matches = matching_client_entities(world, &value, &key);
        match matches.as_slice() {
            [] => Ok(Value::Undefined),
            [id] => Ok(Value::Object(*id)),
            _ => Err("getent used with more than one entity".into()),
        }
    });

    // getentarray(localclientnum, [value, key]): all client entities, or
    // those whose string field `key` matches `value`.
    registry.register(Function, "getentarray", |world, _, args| {
        local_client(args, 0)?;
        if args.len() == 1 {
            let mut pairs: Vec<(i32, u64)> = world
                .resource::<RoundScript>()
                .entities
                .iter()
                .filter(|(id, entity)| {
                    entity.kind != EntityKind::HudElem && world.resource::<RoundScript>().live(id)
                })
                .map(|(id, entity)| (entity.number, *id))
                .collect();
            pairs.sort_unstable_by_key(|(number, _)| *number);
            return new_array(
                world,
                pairs.into_iter().map(|(_, id)| Value::Object(id)).collect(),
            );
        }
        let value = string(args, 1)?;
        let key = string(args, 2)?.to_ascii_lowercase();
        if !CLIENT_STRING_KEYS.contains(&key.as_str()) {
            return Ok(Value::Undefined);
        }
        let matches = matching_client_entities(world, &value, &key);
        new_array(world, matches.into_iter().map(Value::Object).collect())
    });

    // getentbynum(localclientnum, entnum): the client entity with that
    // entity number, or undefined.
    registry.register(Function, "getentbynum", |world, _, args| {
        local_client(args, 0)?;
        let number = int(args, 1)?;
        let runtime = world.resource::<RoundScript>();
        let found = runtime
            .entities
            .iter()
            .find(|(id, entity)| {
                entity.kind != EntityKind::HudElem && entity.number == number && runtime.live(id)
            })
            .map(|(id, _)| Value::Object(*id));
        Ok(found.unwrap_or(Value::Undefined))
    });

    // ent getentitynumber(): the entity's number.
    registry.register(Method, "getentitynumber", |world, receiver, _| {
        let (_, number, _) = receiver_entity(world, receiver)?;
        Ok(Value::Int(number))
    });

    // ent delete() / ent forcedelete(): retires the client entity after
    // raising `death` and `entityshutdown`.
    registry.register(Method, "delete", |world, receiver, _| {
        delete_client_entity(world, receiver)
    });
    registry.register(Method, "forcedelete", |world, receiver, _| {
        delete_client_entity(world, receiver)
    });

    // ent setmodel(model): sets a script mover's model.
    registry.register(Method, "setmodel", |world, receiver, args| {
        let (id, _, etype) = receiver_entity(world, receiver)?;
        if etype != ETYPE_SCRIPTMOVER {
            return Err("SetModel can only be called on a script mover\n".into());
        }
        let model = string(args, 0)?;
        world
            .resource_mut::<RoundScript>()
            .set_object_field(id, "model", Value::string(&model));
        Ok(Value::Undefined)
    });

    // getweaponmodel(weapon, [variant], [viewmodel]): the weapon's world
    // model (or viewmodel when the third argument is non-zero).
    registry.register(Function, "getweaponmodel", |world, _, args| {
        let name = string(args, 0)?;
        let Some(weapon) = zone_weapon(world, &name) else {
            return Ok(Value::string(""));
        };
        let mut variant = 0usize;
        if args.len() >= 2 {
            let wanted = int(args, 1)?;
            if (0..=255).contains(&wanted)
                && weapon
                    .world_models
                    .get(wanted as usize)
                    .is_some_and(|m| !m.is_empty())
            {
                variant = wanted as usize;
            }
        }
        let viewmodel = args.len() == 3 && int(args, 2)? != 0;
        let models = if viewmodel {
            &weapon.view_models
        } else {
            &weapon.world_models
        };
        let model = models.get(variant).map_or("", String::as_str);
        Ok(Value::string(model))
    });

    // ent useweaponmodel(weapon, [model], [options]): dresses a script mover
    // in the weapon's model and hide tags.
    registry.register(Method, "useweaponmodel", |world, receiver, args| {
        let (id, _, etype) = receiver_entity(world, receiver)?;
        if etype != ETYPE_SCRIPTMOVER {
            return Err("SetModel can only be called on a script mover\n".into());
        }
        let name = string(args, 0)?;
        let Some(weapon) = zone_weapon(world, &name) else {
            return Err(format!(
                "useweaponmodel called with unknown weapon name {name}\n"
            ));
        };
        let model = if args.len() >= 2 {
            string(args, 1)?
        } else {
            weapon
                .world_models
                .first()
                .cloned()
                .unwrap_or_else(String::new)
        };
        // The client handler checks the server VM's parameter count for the
        // third argument (which is zero while the client VM runs), so the
        // options parameter is never read from the client stack.
        {
            let mut runtime = world.resource_mut::<RoundScript>();
            runtime.set_object_field(id, "model", Value::string(&model));
            runtime.set_object_field(id, "weapon", Value::string(&name));
        }
        super::super::weapon::hide_weapon_tags(world, id, &name);
        Ok(Value::Undefined)
    });

    // ent useweaponhidetags(weapon): hides the weapon's hide tags on the
    // entity's DObj.
    registry.register(Method, "useweaponhidetags", |world, receiver, args| {
        let name = string(args, 0)?;
        if !has_weapon_named(world, &name) {
            return Err(format!(
                "useweaponhidetags called with unknown weapon name {name}\n"
            ));
        }
        let (id, ..) = receiver_entity(world, receiver)?;
        super::super::weapon::hide_weapon_tags(world, id, &name);
        Ok(Value::Undefined)
    });

    // ent show() / ent hide(): clears or sets the client entity's hidden
    // flag.
    registry.register(Method, "show", |world, receiver, _| {
        let (id, _, _) = receiver_entity(world, receiver)?;
        if let Some(entity) = world.resource_mut::<RoundScript>().entities.get_mut(&id) {
            entity.hidden = false;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "hide", |world, receiver, _| {
        let (id, _, _) = receiver_entity(world, receiver)?;
        if let Some(entity) = world.resource_mut::<RoundScript>().entities.get_mut(&id) {
            entity.hidden = true;
        }
        Ok(Value::Undefined)
    });

    // ent hasdobj(localclientnum): whether the client entity has a DObj.
    registry.register(Method, "hasdobj", |world, receiver, args| {
        local_client(args, 0)?;
        let (id, _, etype) = receiver_entity(world, receiver)?;
        Ok(Value::Int(entity_has_dobj(world, id, etype).into()))
    });

    // ent getowner(localclientnum): the owner entity of a snapshot entity.
    registry.register(Method, "getowner", |world, receiver, args| {
        let (_, number, _) = receiver_entity(world, receiver)?;
        local_client(args, 0)?;
        let owner_number = with_server(world, |server| {
            let server_id = server_object(server, number)?;
            let runtime = server.resource::<RoundScript>();
            let owner_id = runtime.entities.get(&server_id)?.missile_owner?;
            Some(runtime.entities.get(&owner_id)?.number)
        })
        .flatten();
        Ok(owner_number
            .and_then(|n| client_object_for_number(world, n))
            .unwrap_or(Value::Undefined))
    });

    // ent getparententity(): the entity `ent` is linked to, on the client or
    // in the snapshot.
    registry.register(Method, "getparententity", |world, receiver, _| {
        let (id, number, _) = receiver_entity(world, receiver)?;
        if let Some(parent) = world
            .resource::<RoundScript>()
            .entities
            .get(&id)
            .and_then(|e| e.linked_to.as_ref())
            .map(|link| link.parent)
            .filter(|parent| world.resource::<RoundScript>().live(parent))
        {
            return Ok(Value::Object(parent));
        }
        let parent_number = with_server(world, |server| {
            let server_id = server_object(server, number)?;
            let runtime = server.resource::<RoundScript>();
            let parent_id = runtime.entities.get(&server_id)?.linked_to.as_ref()?.parent;
            Some(runtime.entities.get(&parent_id)?.number)
        })
        .flatten();
        Ok(parent_number
            .and_then(|n| client_object_for_number(world, n))
            .unwrap_or(Value::Undefined))
    });

    // ent gettagorigin(tag) / ent gettagangles(tag): a bone's world origin
    // or angles on the entity's model.
    registry.register(Method, "gettagorigin", |world, receiver, args| {
        Ok(Value::Vector(client_tag_pose(world, receiver, args)?.0))
    });
    registry.register(Method, "gettagangles", |world, receiver, args| {
        Ok(Value::Vector(math_iw4::axis_to_angles(
            client_tag_pose(world, receiver, args)?.1,
        )))
    });

    // ent linkto(parent, [tag], [origin_offset], [angles_offset]): links a
    // client entity to `parent`.
    registry.register(Method, "linkto", |world, receiver, args| {
        let (id, number, _) = receiver_entity(world, receiver)?;
        let (parent, _, _) = entity_arg(world, args, 0)?;
        let tag = match args.len() {
            2 => Some(string(args, 1)?),
            3 => {
                vector(args, 2)?;
                Some(string(args, 1)?)
            }
            4 => {
                vector(args, 3)?;
                vector(args, 2)?;
                Some(string(args, 1)?)
            }
            _ => None,
        }
        .filter(|tag| !tag.is_empty() && !tag.eq_ignore_ascii_case("tag_origin"));
        // Only client-spawned entities (>= 1024) may be linked on the client.
        if number < CLIENT_ENTITY_BASE {
            return Err("LinkTo: Only works on fake entities\n".into());
        }
        let (pose_origin, pose_angles, local_origin, local_angles) = if let Some(ref tag_name) = tag
            && let Ok((tag_origin, tag_axis)) =
                client_tag_pose(world, &Value::Object(parent), &[Value::string(tag_name)])
        {
            (
                tag_origin,
                math_iw4::axis_to_angles(tag_axis),
                [0.0; 3],
                [0.0; 3],
            )
        } else {
            let parent_origin = vector_field(world, parent, "origin");
            let parent_angles = vector_field(world, parent, "angles");
            let child_origin = vector_field(world, id, "origin");
            let child_angles = vector_field(world, id, "angles");
            let parent_axis = math_iw4::angles_to_axis(parent_angles);
            let delta = [
                child_origin[0] - parent_origin[0],
                child_origin[1] - parent_origin[1],
                child_origin[2] - parent_origin[2],
            ];
            let local_org = std::array::from_fn(|i| {
                delta[0] * parent_axis[i][0]
                    + delta[1] * parent_axis[i][1]
                    + delta[2] * parent_axis[i][2]
            });
            let transpose = std::array::from_fn(|r| std::array::from_fn(|c| parent_axis[c][r]));
            let local_ang = math_iw4::axis_to_angles(math_iw4::matrix_multiply(
                math_iw4::angles_to_axis(child_angles),
                transpose,
            ));
            (child_origin, child_angles, local_org, local_ang)
        };
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.set_object_field(id, "origin", Value::Vector(pose_origin));
        runtime.set_object_field(id, "angles", Value::Vector(pose_angles));
        if let Some(entity) = runtime.entities.get_mut(&id) {
            entity.linked_to = Some(Link {
                parent,
                origin: local_origin,
                angles: local_angles,
                tag: tag.map(Arc::from),
                tag_offset: Some([0.0; 3]),
            });
        }
        Ok(Value::Undefined)
    });

    // ent moveto(origin, time, [accel], [decel]): moves the client entity to
    // `origin` over `time` seconds.
    registry.register(Method, "moveto", |world, receiver, args| {
        let (id, _, _) = receiver_entity(world, receiver)?;
        let to = vector(args, 0)?;
        let (time, accel, decel) = mover_times(args)?;
        let from = vector_field(world, id, "origin");
        let now = now_ms(world);
        let duration_ms = ((time * 1000.0) as i32).max(1) as i64;
        world.resource_mut::<Mechanics>().start(
            id,
            Motion {
                field: "origin",
                path: MotionPath::Linear {
                    from,
                    to,
                    accel,
                    decel,
                },
                start_ms: now,
                duration_ms,
                done: "movedone",
            },
        );
        Ok(Value::Undefined)
    });

    // ent rotateto(angles, time, [accel], [decel]): rotates the client
    // entity to `angles` over `time` seconds the short way round.
    registry.register(Method, "rotateto", |world, receiver, args| {
        let (id, _, _) = receiver_entity(world, receiver)?;
        let target = vector(args, 0)?;
        let (time, accel, decel) = mover_times(args)?;
        let from = vector_field(world, id, "angles");
        let to = std::array::from_fn(|i| from[i] + normalize_delta_180(target[i] - from[i]));
        let now = now_ms(world);
        let duration_ms = ((time * 1000.0) as i32).max(1) as i64;
        world.resource_mut::<Mechanics>().start(
            id,
            Motion {
                field: "angles",
                path: MotionPath::Linear {
                    from,
                    to,
                    accel,
                    decel,
                },
                start_ms: now,
                duration_ms,
                done: "rotatedone",
            },
        );
        Ok(Value::Undefined)
    });

    registry.register(Method, "rotatepitch", |world, receiver, args| {
        rotate_axis(world, receiver, args, 0)
    });
    registry.register(Method, "rotateyaw", |world, receiver, args| {
        rotate_axis(world, receiver, args, 1)
    });
    registry.register(Method, "rotateroll", |world, receiver, args| {
        rotate_axis(world, receiver, args, 2)
    });

    // spawnfakeent(localclientnum): allocates a fake entity slot (0..768),
    // or -1 when full.
    registry.register(Function, "spawnfakeent", |world, _, args| {
        local_client(args, 0)?;
        let fakes = &mut world.get_resource_or_insert_with(FakeEntities::default).0;
        let Some(slot) = (0..MAX_FAKE_ENTITIES).find(|slot| !fakes.contains_key(slot)) else {
            return Ok(Value::Int(-1));
        };
        fakes.insert(slot, [0.0; 3]);
        Ok(Value::Int(slot))
    });

    // deletefakeent(localclientnum, fakeent): frees a fake entity slot.
    registry.register(Function, "deletefakeent", |world, _, args| {
        local_client(args, 0)?;
        let slot = int(args, 1)?;
        if let Some(mut fakes) = world.get_resource_mut::<FakeEntities>() {
            fakes.0.remove(&slot);
        }
        Ok(Value::Undefined)
    });

    // setfakeentorg(localclientnum, fakeent, origin): sets an active fake
    // entity's origin.
    registry.register(Function, "setfakeentorg", |world, _, args| {
        local_client(args, 0)?;
        let slot = int(args, 1)?;
        let origin = vector(args, 2)?;
        let fakes = &mut world.get_resource_or_insert_with(FakeEntities::default).0;
        let Some(stored) = fakes.get_mut(&slot) else {
            return Err("Fake ent is not in use!".into());
        };
        *stored = origin;
        Ok(Value::Undefined)
    });

    // scriptmodelsuseanimtree(animtree): sets the animation tree client
    // script models share.
    registry.register(Function, "scriptmodelsuseanimtree", |world, _, args| {
        let tree = animtree_arg(args, 0)?;
        world
            .get_resource_or_insert_with(ClientAnimTrees::default)
            .script_models = Some(tree);
        Ok(Value::Undefined)
    });

    // ent useanimtree(animtree, [allow_missing_model]): attaches `animtree`
    // to a non-player, non-actor entity that has a model.
    registry.register(Method, "useanimtree", |world, receiver, args| {
        let (id, _, etype) = receiver_entity(world, receiver)?;
        if matches!(
            etype,
            ETYPE_PLAYER | ETYPE_PLAYER_CORPSE | ETYPE_ACTOR | ETYPE_ACTOR_CORPSE
        ) {
            return Err("UseAnimTree: Can not be called on a player or an actor.".into());
        }
        let tree = animtree_arg(args, 0)?;
        if !entity_has_dobj(world, id, etype) {
            if args.len() > 1 && int(args, 1)? != 0 {
                return Ok(Value::Int(0));
            }
            return Err("Cannot set an animtree into an entity that has no model.".into());
        }
        world
            .get_resource_or_insert_with(ClientAnimTrees::default)
            .entities
            .insert(id, tree);
        Ok(Value::Int(1))
    });

    // getanimlength(anim): the clip's duration in seconds.
    registry.register(Function, "getanimlength", |world, _, args| {
        let name = anim_arg(args, 0)?;
        if name.eq_ignore_ascii_case("root") {
            return Err("non-primitive animation has no concept of length".into());
        }
        let duration = with_server(world, |server| {
            FrameWorld::from_world(server)
                .anim_clip_named(&name)
                .map(|clip| clip.duration())
        })
        .flatten()
        .ok_or_else(|| {
            format!("t6.csc.anim.length: animation '{name}' is not loaded in the simulation")
        })?;
        Ok(Value::Float(duration))
    });

    // ent getentityanimrate(): the entity's animation playback rate.
    registry.register(Method, "getentityanimrate", |world, receiver, _| {
        let (_, number, _) = receiver_entity(world, receiver)?;
        let rate = with_server(world, |server| {
            let server_id = server_object(server, number)?;
            let native =
                NativeRegistry::black_ops_2().get(Namespace::Method, "getentityanimrate")?;
            match native(server, &Value::Object(server_id), &[]) {
                Ok(Value::Float(rate)) => Some(rate),
                _ => None,
            }
        })
        .flatten()
        .unwrap_or(1.0);
        Ok(Value::Float(rate))
    });

    // ent clearanim(anim, time): clears `anim` on the entity's animation
    // tree over `time` seconds.
    registry.register(Method, "clearanim", |world, receiver, args| {
        let (id, _, etype) = receiver_entity(world, receiver)?;
        if !entity_has_animtree(world, id, etype) {
            if !entity_has_dobj(world, id, etype) {
                return Err("Client entity has no model or animtree.".into());
            }
            return Err("ClearAnim called on an entity that doesn't have an anim tree".into());
        }
        anim_arg(args, 0)?;
        float(args, 1)?;
        if let Some(entity) = world.resource_mut::<RoundScript>().entities.get_mut(&id) {
            entity.anim_op = Some(None);
        }
        Ok(Value::Undefined)
    });

    // ent setanimtime(anim, time): sets the normalized playback time (0..1)
    // of a non-looping timed animation.
    registry.register(Method, "setanimtime", |world, receiver, args| {
        let (id, _, etype) = receiver_entity(world, receiver)?;
        if !entity_has_animtree(world, id, etype) {
            if !entity_has_dobj(world, id, etype) {
                return Err("Client entity has no model or animtree.".into());
            }
            return Ok(Value::Undefined);
        }
        let name = anim_arg(args, 0)?;
        let time = float(args, 1)?;
        if time < 0.0 {
            return Err("must be > 0".into());
        }
        if time > 1.0 {
            return Err("must be < 1".into());
        }
        if name.eq_ignore_ascii_case("root") {
            return Err("not a timed animation".into());
        }
        if time == 1.0 {
            let looping = with_server(world, |server| {
                FrameWorld::from_world(server)
                    .anim_clip_named(&name)
                    .is_some_and(|clip| clip.looping)
            })
            .unwrap_or(false);
            if looping {
                return Err("cannot set time 1 on looping animation".into());
            }
        }
        Ok(Value::Undefined)
    });

    registry.register(Method, "animscripted", |world, receiver, args| {
        set_anim_internal(world, receiver, args, false, true)
    });
    registry.register(Method, "setanimrestart", |world, receiver, args| {
        set_anim_internal(world, receiver, args, false, false)
    });
    registry.register(Method, "setflaggedanim", |world, receiver, args| {
        set_anim_internal(world, receiver, args, true, false)
    });
    registry.register(Method, "setflaggedanimrestart", |world, receiver, args| {
        set_anim_internal(world, receiver, args, true, false)
    });
    registry.register(Method, "setanimknob", |world, receiver, args| {
        set_anim_knob_internal(world, receiver, args, false)
    });
    registry.register(Method, "setflaggedanimknob", |world, receiver, args| {
        set_anim_knob_internal(world, receiver, args, true)
    });

    // getsurfacestrings(): the 32 surface type names in the engine's order.
    registry.register(Function, "getsurfacestrings", |world, _, _| {
        new_array(
            world,
            super::super::entity::SURFACES
                .iter()
                .map(|s| Value::string(s))
                .collect(),
        )
    });

    // bullettrace(start, end, hit_characters, ignore_ent, [no_water]): traces
    // a ray and answers fraction, position, entity, normal and surfacetype.
    registry.register(Function, "bullettrace", |world, _, args| {
        let start = vector(args, 0)?;
        let end = vector(args, 1)?;
        let mut mask = if int(args, 2)? != 0 {
            BULLET_CHARACTERS_MASK
        } else {
            BULLET_WORLD_MASK
        };
        let ignore_number = args
            .get(3)
            .and_then(|v| entity_arg(world, std::slice::from_ref(v), 0).ok())
            .map(|(_, number, _)| number);
        if args.len() == 5 && int(args, 4)? != 0 {
            mask &= !CONTENTS_WATER;
        }
        let traced = with_server(world, |server| {
            let ignore = match ignore_number.and_then(|n| server_object(server, n)) {
                Some(server_id) => engine::trace_ignore(server, Some(&Value::Object(server_id))),
                None => engine::TraceIgnore::default(),
            };
            let outcome = engine::entity_trace(server, start, end, mask, ignore);
            let (fraction, normal, collider) = match outcome {
                TraceOutcome::Hit {
                    fraction,
                    normal,
                    collider,
                    ..
                } => (fraction, normal, Some(collider)),
                TraceOutcome::StartSolid { collider, .. } => (0.0, [0.0; 3], collider),
                _ => (1.0, [0.0; 3], None),
            };
            let hit_number = collider.and_then(|c| {
                let Value::Object(server_id) = engine::collider_entity(server, c) else {
                    return None;
                };
                server
                    .resource::<RoundScript>()
                    .entities
                    .get(&server_id)
                    .map(|e| e.number)
            });
            let surface = collider.map(|c| match c {
                ColliderId::World { surface_flags, .. }
                | ColliderId::EntityDObjBone { surface_flags, .. }
                | ColliderId::EntityLinkedBrush { surface_flags, .. } => {
                    super::super::entity::SURFACES
                        .get(((surface_flags >> 20) & 63) as usize)
                        .copied()
                        .unwrap_or("default")
                }
                ColliderId::Player { .. } => "flesh",
            });
            (fraction, normal, hit_number, surface)
        });
        let (fraction, hit_normal, hit_number, hit_surface) =
            traced.unwrap_or((1.0, [0.0; 3], None, None));
        let position: [f32; 3] = std::array::from_fn(|i| start[i] + (end[i] - start[i]) * fraction);
        let entity = hit_number
            .and_then(|n| client_object_for_number(world, n))
            .unwrap_or(Value::Undefined);
        let (normal, surfacetype) = match hit_surface {
            Some(surface) if fraction < 1.0 => (hit_normal, surface),
            _ => {
                let d: [f32; 3] = std::array::from_fn(|i| end[i] - start[i]);
                let mut len = (d[1] * d[1] + d[0] * d[0] + d[2] * d[2]).sqrt();
                if -len >= 0.0 {
                    len = 1.0;
                }
                let scale = 1.0 / len;
                ([d[0] * scale, d[1] * scale, d[2] * scale], "none")
            }
        };
        let mut pairs = vec![
            ("fraction", Value::Float(fraction)),
            ("position", Value::Vector(position)),
        ];
        if !matches!(entity, Value::Undefined) {
            pairs.push(("entity", entity));
        }
        pairs.push(("normal", Value::Vector(normal)));
        pairs.push(("surfacetype", Value::string(surfacetype)));
        engine::keyed_array(world, pairs)
    });

    // ent istouching(other): whether two client entities' bounds overlap.
    registry.register(Method, "istouching", |world, receiver, args| {
        let (a_id, a_num, a_etype) = receiver_entity(world, receiver)?;
        let (b_id, b_num, b_etype) = entity_arg(world, args, 0)?;
        let a_origin = vector_field(world, a_id, "origin");
        let b_origin = vector_field(world, b_id, "origin");
        let a_model = string_field(world, a_id, "model");
        let b_model = string_field(world, b_id, "model");
        let a_half = if matches!(a_etype, ETYPE_PLAYER | ETYPE_ACTOR) {
            ([-15.0, -15.0, 0.0], [15.0, 15.0, 72.0])
        } else {
            ([-4.0, -4.0, 0.0], [4.0, 4.0, 10.0])
        };
        let b_half = if matches!(b_etype, ETYPE_PLAYER | ETYPE_ACTOR) {
            ([-15.0, -15.0, 0.0], [15.0, 15.0, 72.0])
        } else {
            ([-4.0, -4.0, 0.0], [4.0, 4.0, 10.0])
        };
        let touching = with_server(world, |server| {
            let s_a = server_object(server, a_num);
            let s_b = server_object(server, b_num);
            if let (Some(sa), Some(sb)) = (s_a, s_b) {
                return super::super::usable::spawned_box_touches(server, sa, sb)
                    .or_else(|| super::super::usable::spawned_box_touches(server, sb, sa))
                    .unwrap_or_else(|| crate::script::host::triggers::is_touching(server, sa, sb));
            }
            let frame = FrameWorld::from_world(server);
            for (vol_origin, model, other_origin, other_half) in [
                (b_origin, b_model.as_str(), a_origin, a_half),
                (a_origin, a_model.as_str(), b_origin, b_half),
            ] {
                let brush = model.strip_prefix('*').and_then(|n| n.parse::<u32>().ok());
                let trigger_model = model.strip_prefix('?').and_then(|n| n.parse::<u32>().ok());
                if (brush.is_some() || trigger_model.is_some())
                    && crate::script::host::triggers::map_volume_touches(
                        &frame,
                        vol_origin,
                        brush,
                        trigger_model,
                        std::array::from_fn(|i| other_origin[i] + other_half.0[i]),
                        std::array::from_fn(|i| other_origin[i] + other_half.1[i]),
                    )
                {
                    return true;
                }
            }
            false
        })
        .unwrap_or(false)
            || (0..3).all(|i| {
                a_origin[i] + a_half.0[i] <= b_origin[i] + b_half.1[i]
                    && a_origin[i] + a_half.1[i] >= b_origin[i] + b_half.0[i]
            });
        Ok(Value::Int(touching.into()))
    });

    // getgametypesetting(setting): reads a gametype setting from the server's
    // table.
    registry.register(Function, "getgametypesetting", |world, _, args| {
        let name = string(args, 0)?.to_ascii_lowercase();
        let value = with_server(world, |server| {
            server
                .get_resource::<crate::script::GametypeSettings>()
                .and_then(|settings| settings.0.get(&name).cloned())
        })
        .flatten();
        let Some(value) = value else {
            return Ok(Value::Undefined);
        };
        Ok(match value.parse::<i32>() {
            Ok(n) => Value::Int(n),
            Err(_) => Value::Float(atof(&value)),
        })
    });

    // iprintlnbold(text): prints a bold message on the local client.
    registry.register(Function, "iprintlnbold", |world, _, args| {
        let value = arg(args, 0)?;
        let text = match value {
            Value::LocalizedString(s) => s.to_string(),
            Value::String(s) => s.to_string(),
            other => to_text(other).unwrap_or_else(String::new),
        };
        with_server(world, |server| {
            server
                .resource_mut::<RoundScript>()
                .presented
                .insert("iprintlnbold", vec![Value::string(&text)]);
        });
        Ok(Value::Undefined)
    });

    // createdynentandlaunch(localclientnum, model, pos, angles, hitpos,
    // velocity, [fx], [ang_vel_flag]): spawns and launches a client dynamic
    // entity.
    registry.register(Function, "createdynentandlaunch", |world, _, args| {
        int(args, 0)?;
        let model = string(args, 1)?;
        if model.is_empty() {
            return Err(format!("model '{model}' not precached\n"));
        }
        let pos = vector(args, 2)?;
        let angles = vector(args, 3)?;
        let hit_pos = vector(args, 4)?;
        let force = vector(args, 5)?;
        let fx = if args.len() > 6 {
            match int(args, 6)? {
                0 => None,
                id => super::effects::effect_name(world, id),
            }
        } else {
            None
        };
        if args.len() > 7 {
            int(args, 7)?;
        }
        // The piece is spawned and launched by the runtime's dynamic entity
        // physics (t6.csc.physics.dynent).
        world
            .resource_mut::<crate::script::client::ClientOutput>()
            .spawn_dyn_ent(crate::SpawnedDynEntLaunch {
                model,
                pos,
                angles,
                hit_pos,
                force,
                fx,
            });
        Ok(Value::Undefined)
    });

    // ent launchragdoll(velocity): pushes the entity's ragdoll (building it
    // first when the entity is ragdolling), as the launch event does.
    registry.register(Method, "launchragdoll", |world, receiver, args| {
        let (_, number, _) = receiver_entity(world, receiver)?;
        let direction = vector(args, 0)?;
        crate::script::host::client_scripts::with_server(world, |server| {
            if let Some(id) = super::game::server_object(server, number) {
                super::super::ragdoll::launch(server, id, direction, None);
            }
        });
        Ok(Value::Undefined)
    });

    // physicsexplosioncylinder(localclientnum, origin, outer_radius,
    // inner_radius, magnitude): applies a client physics cylinder impulse.
    registry.register(Function, "physicsexplosioncylinder", |world, _, args| {
        let origin = vector(args, 1)?;
        local_client(args, 0)?;
        let outer = float(args, 2)?;
        let inner = float(args, 3)?;
        if inner < 0.0 {
            return Err("Radius is negative".into());
        }
        if inner > outer {
            return Err("Inner radius is outside the outer radius".into());
        }
        let magnitude = float(args, 4)?;
        crate::script::host::client_scripts::with_server(world, |server| {
            super::super::ragdoll::explosion(server, true, origin, inner, outer, None, magnitude);
        });
        Ok(Value::Undefined)
    });

    // getrope(targetname): looks up a map rope by targetname (-1 when none).
    registry.register(Function, "getrope", |world, _, args| {
        let name = string(args, 0)?;
        Ok(Value::Int(super::rope::find(world, &name)))
    });

    // ropegetposition(rope, fraction): the world point `fraction` (0..1)
    // along the rope.
    registry.register(Function, "ropegetposition", |world, _, args| {
        let rope = int(args, 0)?;
        let fraction = float(args, 1)?;
        if !(0.0..=1.0).contains(&fraction) {
            return Err("The fraction must be between 0 and 1".into());
        }
        // A rope number with no rope leaves the engine's result unset; IW4L
        // answers the origin.
        Ok(Value::Vector(
            super::rope::position(world, rope, fraction).unwrap_or([0.0; 3]),
        ))
    });

    // player usealternateaimparams() / clearalternateaimparams(): toggles
    // alternate aim parameters on a local player.
    registry.register(Method, "usealternateaimparams", |world, receiver, _| {
        receiver_entity(world, receiver)?;
        if !is_local_player(world, receiver) {
            return Err("You can only call UseAlternateAim on players.".into());
        }
        // t6.csc.player.alternate_aim: IW4L does not apply client alternate
        // aim parameters.
        Ok(Value::Undefined)
    });
    registry.register(Method, "clearalternateaimparams", |world, receiver, _| {
        receiver_entity(world, receiver)?;
        if !is_local_player(world, receiver) {
            return Err("You can only call ClearAlternateAim on players.".into());
        }
        Ok(Value::Undefined)
    });

    // player equipflashlight() / removeflashlight(): toggles the local
    // player's flashlight.
    registry.register(Method, "equipflashlight", |world, receiver, _| {
        receiver_entity(world, receiver)?;
        if !is_local_player(world, receiver) {
            return Err("You can only call EquipFlashlight on players.".into());
        }
        // t6.csc.player.flashlight: IW4L has no client player flashlight
        // light source.
        Ok(Value::Undefined)
    });
    registry.register(Method, "removeflashlight", |world, receiver, _| {
        receiver_entity(world, receiver)?;
        if !is_local_player(world, receiver) {
            return Err("You can only call RemoveFlashlight on players.".into());
        }
        Ok(Value::Undefined)
    });

    // ent setrenderinthirdpersonspectate(enable): sets whether the client
    // entity renders during third-person spectating.
    registry.register(
        Method,
        "setrenderinthirdpersonspectate",
        |world, receiver, args| {
            let (id, number, _) = receiver_entity(world, receiver)?;
            let enable = int(args, 0)? != 0;
            let mut flagged = world.get_resource_or_insert_with(ThirdPersonSpectate::default);
            if enable {
                flagged.0.insert(id, number);
            } else {
                flagged.0.remove(&id);
            }
            Ok(Value::Undefined)
        },
    );

    // ent setsonarattachmentenabled(enable): enables or disables the sonar
    // attachment overlay on the local client.
    registry.register(
        Method,
        "setsonarattachmentenabled",
        |world, receiver, args| {
            receiver_entity(world, receiver)?;
            int(args, 0)?;
            // t6.csc.weapons.sonar_attachment: IW4L has no sonar attachment
            // overlay on the client.
            Ok(Value::Undefined)
        },
    );

    // zacklodscalerigid([scale, min_scale]): with no arguments puts
    // `r_lodScaleRigid` and its domain's floor back to 1; with two, the
    // floor becomes `min_scale` (at least 0.6) and the value `scale` held
    // between the floor and the domain's top (4); other counts do nothing.
    registry.register(Function, "zacklodscalerigid", |world, _, args| {
        let (value, min) = match args.len() {
            0 => (1.0, 1.0),
            2 => {
                let floor = float(args, 1)?;
                let floor = if floor - 0.6 >= 0.0 { floor } else { 0.6 };
                let scale = float(args, 0)?;
                let mut value = if scale - 4.0 >= 0.0 { 4.0 } else { scale };
                if floor - scale >= 0.0 {
                    value = floor;
                }
                (value, floor)
            }
            _ => return Ok(Value::Undefined),
        };
        with_server(world, |server| {
            let set_ms = crate::level_time_ms(server.resource::<crate::step::StepRequest>().tick);
            let mut frame = crate::frame::FrameWorld::from_world(server);
            let client_id = crate::ClientId(super::game::LOCAL_CLIENT as u32);
            if frame.client_meta(client_id).is_some() {
                frame
                    .client_meta_mut(client_id)
                    .view_effects
                    .client_screen
                    .lod_scale_rigid = Some(crate::LodScaleRigid { set_ms, value, min });
            }
        });
        Ok(Value::Undefined)
    });

    // ent issplitscreenhost(): whether the entity is the split-screen host's
    // local player.
    registry.register(Method, "issplitscreenhost", |world, receiver, _| {
        Ok(Value::Int(is_local_player(world, receiver).into()))
    });

    // ent getcharacterindex(): the character variant index set on an actor or
    // script mover (`setcharacterindex`), 0 otherwise.
    registry.register(Method, "getcharacterindex", |world, receiver, _| {
        let (_, number, etype) = receiver_entity(world, receiver)?;
        let index = if matches!(etype, ETYPE_ACTOR | ETYPE_ACTOR_CORPSE) {
            with_server(world, |server| {
                let server_id = server_object(server, number)?;
                Some(super::super::actor::character_index(server, server_id))
            })
            .flatten()
            .unwrap_or(0)
        } else {
            0
        };
        Ok(Value::Int(i32::from(index)))
    });

    // precacheanimstatedef(ai_index, #animtree, name): registers an actor
    // type's animation state definition on the client.
    registry.register(Function, "precacheanimstatedef", |_, _, args| {
        int(args, 0)?;
        animtree_arg(args, 1)?;
        string(args, 2)?;
        Ok(Value::Undefined)
    });

    // setdemolockonvalues(ai_index, highlight_dist, height1, pitch_min1,
    // pitch_max1, height2, pitch_min2, pitch_max2): sets theater free-camera
    // lock-on parameters for an actor type.
    registry.register(Function, "setdemolockonvalues", |_, _, args| {
        let index = int(args, 0)? as u8;
        if index != 0 {
            float(args, 1)?;
            int(args, 2)?;
            float(args, 3)?;
            float(args, 4)?;
            int(args, 5)?;
            float(args, 6)?;
            float(args, 7)?;
        }
        Ok(Value::Undefined)
    });
}

/// The client entities `setrenderinthirdpersonspectate` flags, by client
/// object id: their numbers.
#[derive(Resource, Default)]
struct ThirdPersonSpectate(BTreeMap<u64, i32>);

/// The client's draw of its flagged entities, each frame: one is drawn while
/// the client renders in third person and its player state's other flags
/// hold bit 2 or bit 4; otherwise (bit 2 aside, which keeps it undrawn) only
/// an actor not linked to the local player is drawn. The client's screen
/// takes those it does not draw.
///
/// t6.cg.rendering_third_person: the client's third-person rendering state
/// reads Black Ops 2's own player state fields, which only `movement_t6`
/// keeps, so the client renders in first person here.
pub(super) fn sync_third_person_spectate(world: &mut World) {
    let flagged: Vec<(u64, i32)> = world
        .get_resource::<ThirdPersonSpectate>()
        .map(|flagged| {
            flagged
                .0
                .iter()
                .map(|(id, number)| (*id, *number))
                .collect()
        })
        .unwrap_or_else(Vec::new);
    let alive: Vec<(u64, i32)> = {
        let runtime = world.resource::<RoundScript>();
        flagged
            .into_iter()
            .filter(|(id, _)| runtime.entities.contains_key(id))
            .collect()
    };
    if let Some(mut flagged) = world.get_resource_mut::<ThirdPersonSpectate>() {
        flagged
            .0
            .retain(|id, _| alive.iter().any(|(kept, _)| kept == id));
    }
    let rendering_third_person = false;
    crate::script::host::client_scripts::with_server(world, |server| {
        let local = ClientId(LOCAL_CLIENT as u32);
        let other_flags = FrameWorld::from_world(server)
            .player(local)
            .map_or(0, |ps| ps.other_flags);
        let undrawn: Vec<u16> = alive
            .iter()
            .filter(|(_, number)| {
                let drawn = if other_flags & 2 != 0 {
                    rendering_third_person
                } else if other_flags & 4 != 0 && rendering_third_person {
                    true
                } else {
                    actor_apart_from(server, *number, local)
                };
                !drawn
            })
            .filter_map(|(_, number)| u16::try_from(*number).ok())
            .collect();
        let mut frame = FrameWorld::from_world(server);
        if frame.client_meta(local).is_some() {
            let screen = &mut frame.client_meta_mut(local).view_effects.client_screen;
            if screen.undrawn_entities != undrawn {
                diag::info!(Sim, "t6 client undrawn entities: {undrawn:?}");
                screen.undrawn_entities = undrawn;
            }
        }
    });
}

/// Whether the entity is an actor not linked to the local player.
fn actor_apart_from(server: &World, number: i32, local: ClientId) -> bool {
    let runtime = server.resource::<RoundScript>();
    let Some(entity) = server_object(server, number).and_then(|id| runtime.entities.get(&id))
    else {
        return false;
    };
    if entity.kind != EntityKind::Actor {
        return false;
    }
    let parent = entity
        .linked_to
        .as_ref()
        .and_then(|link| runtime.entities.get(&link.parent))
        .map(|parent| parent.number);
    parent != Some(local.0 as i32)
}
