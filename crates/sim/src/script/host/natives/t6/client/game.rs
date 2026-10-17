//! Black Ops 2's client game as its client scripts see it: the entities of
//! the local client's snapshot, its client fields and the callbacks the
//! client game runs when they change, and the local client itself.
//!
//! IW4L's local client reads the server's state each authority tick where
//! Black Ops 2's reads its last snapshot (`t6.csc.snapshot`).

use super::super::super::super::entities::EntityKind;
use super::super::{
    ClientExploders, ClientFields, ClientSystemStates, ClientSystems, SCRIPT_MOVERS,
};
use crate::script::host::client_scripts::ClientCallbacks;
use crate::script::{RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

/// The local client IW4L runs the client scripts of.
pub(crate) const LOCAL_CLIENT: i32 = 0;

/// A client field the client scripts registered (`registerclientfield`).
#[derive(Clone, Debug)]
#[expect(dead_code, reason = "IW4L's one local client is the host")]
pub(crate) struct FieldDef {
    pub(crate) pool: String,
    pub(crate) name: String,
    pub(crate) version: i32,
    pub(crate) bits: i32,
    pub(crate) float: bool,
    /// The function the client game runs when the field changes.
    pub(crate) callback: Value,
    /// Only the host's local client runs the callback.
    pub(crate) host_only: bool,
    /// The callback runs for a new entity or snapshot even when the value is 0.
    pub(crate) zero_callback: bool,
    pub(crate) min: f32,
    pub(crate) max: f32,
    /// The field's changes go to the client's menus (`setupclientfieldcodecallbacks`).
    pub(crate) code: bool,
}

/// A field change the client game hands its menus (LUI) for a code
/// callback: the field, the entity number, old and new values, new entity
/// and initial snapshot.
#[derive(Clone, Debug)]
pub(crate) struct MenuFieldEvent {
    pub(crate) field: usize,
    pub(crate) entity: Option<i32>,
    pub(crate) old: Value,
    pub(crate) new: Value,
    pub(crate) new_entity: bool,
    pub(crate) initial: bool,
}

/// An entity of the local client's snapshot.
#[derive(Clone, Debug)]
pub(crate) struct ClientEntity {
    /// The client's script object for it.
    pub(crate) object: u64,
    /// The engine's entity type (`self.type`).
    pub(crate) etype: i32,
    /// Its client fields as last delivered, by field.
    values: BTreeMap<usize, Value>,
    /// The client game ran its spawned callback.
    spawned: bool,
}

#[derive(Resource, Default)]
pub(crate) struct ClientGame {
    pub(crate) fields: Vec<FieldDef>,
    /// The snapshot's entities by entity number.
    pub(crate) entities: BTreeMap<i32, ClientEntity>,
    world_values: BTreeMap<usize, Value>,
    player_values: BTreeMap<usize, Value>,
    /// The client has processed a snapshot (`clienthassnapshot`).
    pub(crate) has_snapshot: bool,
    /// The local client's player entity number.
    pub(crate) local_player: Option<i32>,
    /// The server time of the last snapshot.
    pub(crate) server_time: i32,
    /// The client game ran `localclientconnect`.
    connected: bool,
    /// Registration is over: the client game finalised its fields.
    pub(crate) fields_final: bool,
    /// Entities whose fields call back again as new (`processclientfieldsasifnew`).
    pub(crate) renew: Vec<i32>,
    /// Field changes for the menus, oldest first.
    pub(crate) menu_events: Vec<MenuFieldEvent>,
    /// `allowroundanimation(allow)` events for the LUI root.
    pub(crate) allow_round_animation: Vec<i32>,
    /// `usealternatereviveicon(use_alternate)` (`waypoint_revive_zm` vs `waypoint_revive`).
    pub(crate) alternate_revive_icon: bool,
    /// `setalphafadeforname(alpha)` per player entity number (`0..18`).
    pub(crate) name_fade_alpha: BTreeMap<i32, f32>,
    /// The client systems' states as last delivered, by system.
    states: BTreeMap<usize, String>,
    /// The client's own entities drawn through the server's presentation:
    /// client object, then the server object that draws it.
    drawn: BTreeMap<u64, u64>,
}

impl ClientGame {
    /// The snapshot's value of a field: the world's, an entity's, or the
    /// local player's own.
    pub(crate) fn value(&self, set: &str, owner: Option<i32>, index: usize) -> Option<Value> {
        match (set, owner) {
            ("world", _) => self.world_values.get(&index).cloned(),
            ("toplayer", _) => self.player_values.get(&index).cloned(),
            (_, Some(number)) => self.entities.get(&number)?.values.get(&index).cloned(),
            _ => None,
        }
    }
}

/// What the client sees of one server entity.
struct SnapshotEntity {
    number: i32,
    etype: i32,
    owner: u64,
    fields: Vec<(&'static str, Value)>,
}

/// The engine's entity type names (`self.type`), by type.
pub(crate) fn type_name(etype: i32) -> &'static str {
    match etype {
        0 => "general",
        1 => "player",
        2 => "player_corpse",
        4 => "missile",
        6 => "scriptmover",
        11 => "turret",
        12 => "helicopter",
        13 => "plane",
        14 => "vehicle",
        16 => "actor",
        20 => "zbarrier",
        _ => "NA",
    }
}

/// The client field set of an entity type.
pub(crate) fn set_of_type(etype: i32) -> Option<&'static str> {
    Some(match etype {
        1 => "allplayers",
        4 => "missile",
        6 => "scriptmover",
        11 => "turret",
        12 => "helicopter",
        13 => "plane",
        14 => "vehicle",
        16 => "actor",
        20 => "zbarrier",
        _ => return None,
    })
}

/// The server's entities the local client's snapshot carries, with their
/// type: players, corpses, missiles, script movers, vehicles, actors and
/// barriers. Triggers, spawners, nodes and other server-only entities are not
/// sent.
fn snapshot(server: &mut World) -> Vec<SnapshotEntity> {
    let listed: Vec<(u64, i32, i32)> = server
        .resource::<RoundScript>()
        .entities
        .iter()
        .filter_map(|(&id, entity)| {
            let etype = match entity.kind {
                EntityKind::Player => 1,
                EntityKind::Corpse { .. } => 2,
                EntityKind::Missile(_) => 4,
                EntityKind::Vehicle => 14,
                EntityKind::Actor => 16,
                EntityKind::ActorCorpse => 18,
                EntityKind::Map | EntityKind::Spawned
                    if entity.classname.starts_with("zbarrier") =>
                {
                    20
                }
                EntityKind::Map | EntityKind::Spawned
                    if SCRIPT_MOVERS.contains(&&*entity.classname) =>
                {
                    6
                }
                _ => return None,
            };
            (entity.number >= 0).then_some((id, entity.number, etype))
        })
        .collect();
    listed
        .into_iter()
        .map(|(id, number, etype)| {
            let mut fields = Vec::new();
            for name in ["origin", "angles", "model", "targetname", "team", "name"] {
                let value = crate::script::host::players::entity_field(server, id, name);
                fields.push((name, value));
            }
            fields.push(("type", Value::string(type_name(etype))));
            fields.push((
                "weapon",
                Value::string(&snapshot_weapon(server, id, number, etype)),
            ));
            SnapshotEntity {
                number,
                etype,
                owner: id,
                fields,
            }
        })
        .collect()
}

/// The weapon an entity's state carries, by name, as the client's `weapon`
/// field reads it (`"none"` for weapon 0): a missile's weapon, a player's
/// current weapon.
fn snapshot_weapon(server: &mut World, id: u64, number: i32, etype: i32) -> String {
    let client = server.resource::<RoundScript>().player_client(id);
    let frame = crate::frame::FrameWorld::from_world(server);
    let weapon = match etype {
        1 => client
            .and_then(|client| frame.player(crate::ClientId(client)))
            .map_or(0, |ps| ps.weapon),
        4 => frame.projectile_by_number(number).map_or(0, |p| p.weapon),
        _ => 0,
    };
    crate::script_player::weapon_name(&frame, weapon)
}

/// The values the server set on a pool's fields for one owner, by the
/// client's field index.
fn server_values(
    server: &World,
    game: &ClientGame,
    pool: &str,
    owner: u64,
) -> BTreeMap<usize, Value> {
    let Some(set) = server.get_resource::<ClientFields>() else {
        return BTreeMap::new();
    };
    let mut values = BTreeMap::new();
    let first = (pool.to_owned(), owner, 0);
    let last = (pool.to_owned(), owner, usize::MAX);
    for ((_, _, index), value) in set.1.range(first..=last) {
        let Some(name) = set.0.get(*index).map(|field| &field.name) else {
            continue;
        };
        if let Some(client) = game
            .fields
            .iter()
            .position(|field| field.pool == pool && field.name == *name)
        {
            let def = &game.fields[client];
            let quantized = match (value, def.float) {
                (Value::Float(v), true) => {
                    let bits = def.bits.clamp(1, 32) as u32;
                    let mask = if bits == 32 {
                        u32::MAX
                    } else {
                        (1u32 << bits) - 1
                    };
                    let steps = mask as i32 as f32;
                    let max_step = steps as i32;
                    let span = def.max - def.min;
                    let raw = if span.abs() > f32::EPSILON {
                        (((steps / span) * (*v - def.min) + 0.5) as i32)
                            .min(max_step)
                            .max(0) as u32
                    } else {
                        0
                    };
                    let decoded = if mask > 0 {
                        (f64::from(def.min)
                            + ((f64::from(def.max) - f64::from(def.min)) / f64::from(mask as i32))
                                * f64::from(raw)) as f32
                    } else {
                        def.min
                    };
                    Value::Float(decoded)
                }
                (Value::Int(v), false) => {
                    let bits = def.bits.clamp(1, 32) as u32;
                    let mask = if bits == 32 {
                        u32::MAX
                    } else {
                        (1u32 << bits) - 1
                    };
                    let clamped = (*v).max(0) as u32 & mask;
                    Value::Int(clamped as i32)
                }
                (other, _) => other.clone(),
            };
            values.insert(client, quantized);
        }
    }
    values
}

fn zero(field: &FieldDef) -> Value {
    if field.float {
        Value::Float(0.0)
    } else {
        Value::Int(0)
    }
}

fn is_zero(value: &Value) -> bool {
    matches!(value, Value::Int(0)) || matches!(value, Value::Float(f) if *f == 0.0)
}

/// A callback the client game runs: on what, which function, with what.
struct Pending {
    receiver: Value,
    callback: Value,
    args: Vec<Value>,
}

/// The callbacks a pool's fields raise, as the client game queues them: a
/// new entity or the first snapshot calls back each field set (or each with
/// `zero_callback`), its old value the previous one; a later snapshot calls
/// back each field that changed.
fn field_callbacks(
    game: &ClientGame,
    pool: &str,
    receiver: &Value,
    old: &BTreeMap<usize, Value>,
    new: &BTreeMap<usize, Value>,
    new_entity: bool,
    initial: bool,
    (entity, out, menu): (Option<i32>, &mut Vec<Pending>, &mut Vec<MenuFieldEvent>),
) {
    for (index, field) in game.fields.iter().enumerate() {
        if field.pool != pool || (field.callback == Value::Undefined && !field.code) {
            continue;
        }
        let new_value = new.get(&index).cloned().unwrap_or_else(|| zero(field));
        let old_value = if initial {
            new_value.clone()
        } else {
            old.get(&index).cloned().unwrap_or_else(|| zero(field))
        };
        let call = if new_entity || initial {
            field.zero_callback || !is_zero(&new_value)
        } else {
            old_value != new_value
        };
        if call && field.code {
            menu.push(MenuFieldEvent {
                field: index,
                entity,
                old: old_value,
                new: new_value,
                new_entity,
                initial,
            });
        } else if call {
            out.push(Pending {
                receiver: receiver.clone(),
                callback: field.callback.clone(),
                args: vec![
                    Value::Int(LOCAL_CLIENT),
                    old_value,
                    new_value,
                    Value::Int(new_entity.into()),
                    Value::Int(initial.into()),
                    Value::string(&field.name),
                    Value::Int(0),
                ],
            });
        }
    }
}

/// The client game's frame: the snapshot's entities and client fields, then
/// the callbacks they raise, in the client game's order.
pub(crate) fn sync(client: &mut World) {
    crate::script::host::client_scripts::with_both(client, sync_with);
}

fn sync_with(server: &mut World, client: &mut World) {
    let now = i64::from(client.resource::<crate::step::StepRequest>().tick.0)
        * i64::from(crate::MATCH_TICK_MS);
    if !client.resource::<RoundScript>().started {
        return;
    }
    let mut game = client
        .remove_resource::<ClientGame>()
        .unwrap_or_else(ClientGame::default);
    let mut pending = Vec::new();
    let mut menu = Vec::new();
    let renew = std::mem::take(&mut game.renew);
    // The client game finalised its fields as it started.
    game.fields_final = true;
    let initial = !game.has_snapshot;
    game.server_time = now as i32;
    // The world's fields, then the local player's own.
    let world_new = server_values(server, &game, "world", 0);
    field_callbacks(
        &game,
        "world",
        &Value::level(),
        &game.world_values,
        &world_new,
        false,
        initial,
        (None, &mut pending, &mut menu),
    );
    game.world_values = world_new;
    let entities = snapshot(server);
    let mut seen = std::collections::BTreeSet::new();
    let mut spawned = Vec::new();
    for entity in &entities {
        seen.insert(entity.number);
        let new_entity = !game.entities.contains_key(&entity.number);
        if new_entity {
            let map_fields: Vec<(String, Value)> = {
                let server_runtime = server.resource::<RoundScript>();
                server_runtime
                    .entities
                    .get(&entity.owner)
                    .filter(|record| record.kind == EntityKind::Map)
                    .and_then(|_| server_runtime.objects.get(&entity.owner))
                    .map_or_else(Vec::new, |fields| {
                        fields
                            .iter()
                            .filter_map(|(&id, value)| {
                                let name = server_runtime
                                    .program
                                    .as_ref()
                                    .and_then(|program| program.symbols.get(id as usize).cloned())
                                    .or_else(|| {
                                        server_runtime
                                            .dynamic_symbols
                                            .iter()
                                            .find(|(_, v)| **v == id)
                                            .map(|(k, _)| k.clone())
                                    })?;
                                Some((name.to_string(), value.clone()))
                            })
                            .collect()
                    })
            };
            let object = {
                let mut runtime = client.resource_mut::<RoundScript>();
                let Ok(object) =
                    runtime.create_entity(EntityKind::Spawned, type_name(entity.etype))
                else {
                    continue;
                };
                if let Some(made) = runtime.entities.get_mut(&object) {
                    made.number = entity.number;
                }
                for (name, value) in map_fields {
                    runtime.set_object_field(object, &name, value);
                }
                object
            };
            game.entities.insert(
                entity.number,
                ClientEntity {
                    object,
                    etype: entity.etype,
                    values: BTreeMap::new(),
                    spawned: false,
                },
            );
        }
        let record = &game.entities[&entity.number];
        let object = record.object;
        {
            let mut runtime = client.resource_mut::<RoundScript>();
            for (name, value) in &entity.fields {
                runtime.set_object_field(object, name, value.clone());
            }
        }
        if let Some(pool) = set_of_type(entity.etype) {
            let new = server_values(server, &game, pool, entity.owner);
            field_callbacks(
                &game,
                pool,
                &Value::Object(object),
                &record.values,
                &new,
                new_entity || renew.contains(&entity.number),
                initial && !new_entity,
                (Some(entity.number), &mut pending, &mut menu),
            );
            game.entities.get_mut(&entity.number).unwrap().values = new;
        }
        if entity.etype == 1 && entity.number == LOCAL_CLIENT {
            game.local_player = Some(entity.number);
            let new = server_values(server, &game, "toplayer", entity.number as u64);
            field_callbacks(
                &game,
                "toplayer",
                &Value::Object(object),
                &game.player_values,
                &new,
                new_entity,
                initial && !new_entity,
                (Some(entity.number), &mut pending, &mut menu),
            );
            game.player_values = new;
        }
        if !game.entities[&entity.number].spawned {
            game.entities.get_mut(&entity.number).unwrap().spawned = true;
            let aitype = (entity.etype == 16)
                .then(|| {
                    server
                        .resource::<RoundScript>()
                        .entities
                        .get(&entity.owner)?
                        .classname
                        .strip_prefix("actor_")
                        .map(str::to_owned)
                })
                .flatten();
            spawned.push((object, entity.etype, aitype));
        }
    }
    // The client systems' states the server set for every client or for
    // this one, the latter winning, as their config strings change.
    let mut changed_states = Vec::new();
    if let Some(states) = server.get_resource::<ClientSystemStates>() {
        let names = server
            .get_resource::<ClientSystems>()
            .map_or_else(Vec::new, |systems| systems.0.clone());
        let mut current: BTreeMap<usize, String> = BTreeMap::new();
        for ((client, index), state) in &states.0 {
            match client {
                None => {
                    current.entry(*index).or_insert_with(|| state.clone());
                }
                Some(client) if *client == LOCAL_CLIENT as u32 => {
                    current.insert(*index, state.clone());
                }
                Some(_) => {}
            }
        }
        for (index, state) in &current {
            if game.states.get(index) != Some(state)
                && let Some(name) = names.get(*index)
            {
                changed_states.push((name.clone(), state.replace('!', " ")));
            }
        }
        game.states = current;
    }
    let exploders = server
        .get_resource_mut::<ClientExploders>()
        .map_or_else(Vec::new, |mut exploders| std::mem::take(&mut exploders.0));
    let gib_events: Vec<(u64, u32)> = server
        .get_resource_mut::<super::super::extras::GibEvents>()
        .map_or_else(Vec::new, |mut gibs| std::mem::take(&mut gibs.0))
        .into_iter()
        .filter_map(|(number, flags)| Some((game.entities.get(&number)?.object, flags)))
        .collect();
    let local_player_object = game
        .local_player
        .and_then(|number| game.entities.get(&number).map(|e| e.object));
    let messages = server
        .get_resource_mut::<RoundScript>()
        .map_or_else(Vec::new, |mut runtime| {
            std::mem::take(&mut runtime.client_messages)
        });
    let gone: Vec<i32> = game
        .entities
        .keys()
        .filter(|number| !seen.contains(number))
        .copied()
        .collect();
    let mut shutdowns = Vec::new();
    for number in gone {
        if let Some(entity) = game.entities.remove(&number) {
            shutdowns.push(entity.object);
        }
    }
    game.has_snapshot = true;
    game.menu_events.extend(menu);
    let connect = !game.connected;
    game.connected = true;
    client.insert_resource(game);
    let callbacks = client
        .get_resource::<ClientCallbacks>()
        .cloned()
        .unwrap_or_else(ClientCallbacks::default);
    if connect && let Some(name) = &callbacks.local_client_connect {
        run(
            client,
            name,
            Value::level(),
            vec![Value::Int(LOCAL_CLIENT)],
            now,
        );
    }
    for (system, state) in changed_states {
        if let Some(name) = &callbacks.state_change {
            run(
                client,
                name,
                Value::level(),
                vec![
                    Value::Int(LOCAL_CLIENT),
                    Value::string(&system),
                    Value::string(&state),
                ],
                now,
            );
        }
    }
    for number in exploders {
        let callback = if number > 0 {
            &callbacks.activate_exploder
        } else {
            &callbacks.deactivate_exploder
        };
        if let Some(name) = callback {
            run(
                client,
                name,
                Value::level(),
                vec![Value::Int(number.abs())],
                now,
            );
        }
    }
    for call in pending {
        let Value::Function(function) = call.callback else {
            continue;
        };
        let result = crate::script::runtime::start_function_now(
            client,
            function as usize,
            call.receiver,
            call.args,
            now,
        );
        if let Err(fault) = result {
            diag::warn!(Sim, "csc: client field callback: {fault}");
        }
    }
    for (object, etype, aitype) in spawned {
        if let Some(aitype) = aitype {
            let main = format!("aitype/clientscripts/{aitype}::main");
            if client
                .resource::<RoundScript>()
                .program
                .as_ref()
                .is_some_and(|program| program.names.contains_key(&main))
            {
                run(client, &main, Value::Object(object), Vec::new(), now);
            }
        }
        let callback = if etype == 1 {
            &callbacks.player_spawned
        } else {
            &callbacks.entity_spawned
        };
        if let Some(name) = callback {
            run(
                client,
                name,
                Value::Object(object),
                vec![Value::Int(LOCAL_CLIENT)],
                now,
            );
        }
    }
    if let Some(name) = &callbacks.gib_event {
        for (object, flags) in gib_events {
            let pieces: Vec<Value> = (0..8)
                .filter(|&bit| (flags & (1 << bit)) != 0)
                .map(Value::Int)
                .collect();
            let Ok(locations) = crate::script::host::arrays::new_array(client, pieces) else {
                continue;
            };
            let kind = match flags >> 8 {
                1 => "freeze",
                2 => "up",
                _ => "default",
            };
            run(
                client,
                name,
                Value::Object(object),
                vec![Value::Int(LOCAL_CLIENT), Value::string(kind), locations],
                now,
            );
        }
    }
    if let Some(player) = local_player_object {
        for message in messages {
            let crate::script::client::ClientMessage::JumpLand(event) = message else {
                continue;
            };
            match event {
                game_api::movement::JumpLand::Jump(surface) => {
                    if let Some(name) = &callbacks.player_jump {
                        run(
                            client,
                            name,
                            Value::level(),
                            vec![
                                Value::Int(LOCAL_CLIENT),
                                Value::Object(player),
                                Value::string(surface),
                                Value::Int(1),
                                Value::Int(0),
                                Value::Int(0),
                            ],
                            now,
                        );
                    }
                }
                game_api::movement::JumpLand::Land(surface, hurt) => {
                    if let Some(name) = &callbacks.player_land {
                        run(
                            client,
                            name,
                            Value::level(),
                            vec![
                                Value::Int(LOCAL_CLIENT),
                                Value::Object(player),
                                Value::string(surface),
                                Value::Int(1),
                                Value::Int(0),
                                Value::Int(hurt.into()),
                                Value::Int(0),
                            ],
                            now,
                        );
                    }
                }
                game_api::movement::JumpLand::Foliage => {
                    if let Some(name) = &callbacks.player_foliage {
                        run(
                            client,
                            name,
                            Value::level(),
                            vec![
                                Value::Int(LOCAL_CLIENT),
                                Value::Object(player),
                                Value::Int(1),
                                Value::Int(0),
                            ],
                            now,
                        );
                    }
                }
            }
        }
    }
    for object in shutdowns {
        crate::script::runtime::notify_now(
            client,
            Value::Object(object),
            "entityshutdown",
            Vec::new(),
            now,
        );
        if let Some(name) = &callbacks.entity_shutdown {
            run(
                client,
                name,
                Value::level(),
                vec![Value::Int(LOCAL_CLIENT), Value::Object(object)],
                now,
            );
        }
        client.resource_mut::<RoundScript>().request_delete(object);
    }
    super::ambient::update(client, now);
}

fn run(client: &mut World, name: &str, receiver: Value, args: Vec<Value>, now: i64) {
    if let Err(fault) = crate::script::runtime::run_now(client, name, receiver, args, now) {
        diag::warn!(Sim, "csc: {name}: {fault}");
    }
}

/// A local client number argument, as the client handlers read it: an int
/// below the engine's one local client, else its error.
pub(crate) fn local_client(args: &[Value], index: usize) -> Result<i32, String> {
    let number = crate::script::host::args::int(args, index)?;
    if !(0..1).contains(&number) {
        return Err(format!(
            "Trying to get a local client index for a client '{number}' that is not a local client."
        ));
    }
    Ok(number)
}

/// The server object that draws one of the client's own entities.
pub(crate) fn drawn_by(client: &World, object: u64) -> Option<u64> {
    client
        .get_resource::<ClientGame>()?
        .drawn
        .get(&object)
        .copied()
}

/// The snapshot entity a script value is, with its entity number.
pub(crate) fn client_entity(client: &World, value: &Value) -> Option<(i32, ClientEntity)> {
    let Value::Object(id) = value else {
        return None;
    };
    let game = client.get_resource::<ClientGame>()?;
    game.entities
        .iter()
        .find(|(_, entity)| entity.object == *id)
        .map(|(number, entity)| (*number, entity.clone()))
}

/// The server's object for a snapshot entity number.
pub(crate) fn server_object(server: &World, number: i32) -> Option<u64> {
    server
        .resource::<RoundScript>()
        .entities
        .iter()
        .find(|(_, entity)| entity.number == number && entity.kind != EntityKind::HudElem)
        .map(|(id, _)| *id)
}

/// The client's own entities with a model (made by its `spawn`, not of the
/// snapshot) are drawn for the local client alone through the server's
/// presentation (`t6.csc.client_entities.presence`): a server entity the
/// server's scripts never name stands for each.
pub(crate) fn present(client: &mut World) {
    crate::script::host::client_scripts::with_both(client, present_with);
}

/// The class the server's stand-ins take, which no server script spawns.
const STAND_IN: &str = "csc_model";

fn present_with(server: &mut World, client: &mut World) {
    let Some(mut game) = client.remove_resource::<ClientGame>() else {
        return;
    };
    let snapshot: std::collections::BTreeSet<u64> =
        game.entities.values().map(|entity| entity.object).collect();
    let mut wanted = Vec::new();
    {
        let mut runtime = client.resource_mut::<RoundScript>();
        let own: Vec<(u64, bool)> = runtime
            .entities
            .iter()
            .filter(|(id, entity)| {
                !snapshot.contains(id) && entity.kind == EntityKind::Spawned && entity.number >= 0
            })
            .map(|(id, entity)| (*id, entity.hidden))
            .collect();
        for (id, hidden) in own {
            let Value::String(model) = runtime.object_field(id, "model") else {
                continue;
            };
            if model.is_empty() {
                continue;
            }
            let origin = runtime.object_field(id, "origin");
            let angles = runtime.object_field(id, "angles");
            wanted.push((id, model.to_string(), origin, angles, hidden));
        }
    }
    let live: std::collections::BTreeSet<u64> = wanted.iter().map(|w| w.0).collect();
    let gone: Vec<u64> = game
        .drawn
        .keys()
        .filter(|id| !live.contains(id))
        .copied()
        .collect();
    for id in gone {
        if let Some(stand_in) = game.drawn.remove(&id) {
            server
                .resource_mut::<RoundScript>()
                .request_delete(stand_in);
        }
    }
    for (id, model, origin, angles, hidden) in wanted {
        let stand_in = match game.drawn.get(&id) {
            Some(&stand_in) => stand_in,
            None => {
                let origin_at = match origin {
                    Value::Vector(v) => v,
                    _ => [0.0; 3],
                };
                let Ok(presence) = crate::script::host::presence::spawn_presence(server, origin_at)
                else {
                    continue;
                };
                let mut runtime = server.resource_mut::<RoundScript>();
                let Ok(stand_in) = runtime.create_entity(EntityKind::Spawned, STAND_IN) else {
                    continue;
                };
                if let Some(entity) = runtime.entities.get_mut(&stand_in) {
                    entity.presence = Some(presence);
                    entity.solid = false;
                }
                game.drawn.insert(id, stand_in);
                stand_in
            }
        };
        let mut runtime = server.resource_mut::<RoundScript>();
        runtime.set_object_field(stand_in, "origin", origin);
        runtime.set_object_field(stand_in, "angles", angles);
        runtime.set_object_field(stand_in, "model", Value::string(&model));
        if let Some(entity) = runtime.entities.get_mut(&stand_in) {
            // Shown to the local client alone.
            entity.hidden = true;
            entity.shown_to = if hidden { 0 } else { 1 << LOCAL_CLIENT };
        }
    }
    let menu_events = std::mem::take(&mut game.menu_events);
    let allow_round_animation = std::mem::take(&mut game.allow_round_animation);
    if !menu_events.is_empty() || !allow_round_animation.is_empty() {
        let fields = &game.fields;
        if let Some(mut frame) = crate::frame::FrameWorld::try_from_world(server) {
            let meta = frame.client_meta_mut(crate::world::ClientId(LOCAL_CLIENT as u32));
            for event in menu_events {
                let Some(def) = fields.get(event.field) else {
                    continue;
                };
                let as_f32 = |v: &Value| match v {
                    Value::Int(n) => *n as f32,
                    Value::Float(f) => *f,
                    _ => 0.0,
                };
                meta.push_lui_client_field(game_api::ui::ClientFieldEvent {
                    name: def.name.clone(),
                    old: as_f32(&event.old),
                    new: as_f32(&event.new),
                    initial: event.initial,
                    new_entity: event.new_entity,
                    entity: event.entity,
                });
            }
            for allow in allow_round_animation {
                meta.push_lui_notify(game_api::ui::LuiNotify {
                    name: "allow_round_animation".to_owned(),
                    data: vec![allow],
                });
            }
        }
    }
    client.insert_resource(game);
}
