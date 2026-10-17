//! Black Ops 2's client ambient rooms and packages: the rooms the client
//! scripts describe (`setambientpackage`), the client triggers that name
//! them, and the engine's per-frame pass that finds the room and package the
//! local player stands in and tells the scripts when they change
//! (`soundsetambientstate`).

use super::super::{array_id, int, string};
use super::game::ClientGame;
use super::sound::{MixCommand, mix_command, mix_names, sound_hash};
use crate::script::host::client_scripts::ClientCallbacks;
use crate::script::{ArrayKey, Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};

/// The rooms the engine holds.
const ROOMS: usize = 32;
/// The triggers (and their priorities) a room lists.
const ROOM_TRIGGERS: usize = 16;
/// The context slots a room holds.
const ROOM_CONTEXTS: usize = 16;
/// The entities whose ambient state the engine follows.
const STATES: usize = 64;

/// The client triggers the client spawned from the map, each with its
/// spawnflags.
#[derive(Resource, Default)]
pub(crate) struct ClientTriggers(pub(crate) Vec<(u64, i32)>);

/// A room (or package) the client scripts described.
#[derive(Clone)]
struct Room {
    has_room: bool,
    has_package: bool,
    default_room: bool,
    name: String,
    hash: u32,
    /// The entity numbers of the triggers that name it.
    triggers: [u16; ROOM_TRIGGERS],
    /// Each trigger's priority: a higher one wins.
    priorities: [i8; ROOM_TRIGGERS],
    /// Its contexts (type and value hashes), by the driver's context type.
    contexts: [(u32, u32); ROOM_CONTEXTS],
}

/// What the engine follows of one entity: the room and package it touches
/// this frame, and the room it was last told of.
#[derive(Clone, Copy)]
struct State {
    room: i32,
    last_room: i32,
    package: i32,
    room_slot: usize,
    package_slot: usize,
    trigger: i32,
    entity: i32,
    active: bool,
    changed: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            room: -1,
            last_room: -2,
            package: -1,
            room_slot: 0,
            package_slot: 0,
            trigger: -1,
            entity: -1,
            active: false,
            changed: false,
        }
    }
}

/// The engine's ambient rooms and the states it follows.
#[derive(Resource)]
pub(crate) struct AmbientRooms {
    rooms: Vec<Room>,
    default_room: Option<usize>,
    /// The room and package forced on every state (`forceambientroom`,
    /// `forceambientpackage`), by name hash; 0 is none.
    forced_room: u32,
    forced_package: u32,
    states: [State; STATES],
    /// The triggers marked for actors (`setambienttrigger`).
    actor_triggers: Vec<u64>,
}

impl Default for AmbientRooms {
    fn default() -> Self {
        Self {
            rooms: Vec::new(),
            default_room: None,
            forced_room: 0,
            forced_package: 0,
            states: [State::default(); STATES],
            actor_triggers: Vec::new(),
        }
    }
}

impl AmbientRooms {
    /// The best room and package among those that list the state's trigger:
    /// a higher priority replaces the one held, a tie keeps it.
    fn pick(&mut self, at: usize) {
        let state = &mut self.states[at];
        for (index, room) in self.rooms.iter().enumerate() {
            for slot in 0..ROOM_TRIGGERS {
                if state.trigger != i32::from(room.triggers[slot]) {
                    continue;
                }
                let priority = room.priorities[slot];
                if room.has_package
                    && (state.package < 0
                        || self.rooms[state.package as usize].priorities[state.package_slot]
                            < priority)
                {
                    state.package = index as i32;
                    state.package_slot = slot;
                }
                if room.has_room
                    && (state.room < 0
                        || self.rooms[state.room as usize].priorities[state.room_slot] < priority)
                {
                    state.room = index as i32;
                    state.room_slot = slot;
                }
            }
        }
    }

    /// The forced room and package replace the state's; a room other than
    /// the last told is a change.
    fn force(&mut self, at: usize) {
        let state = &mut self.states[at];
        if self.forced_room != 0
            && let Some(index) = self.rooms.iter().position(|r| r.hash == self.forced_room)
        {
            if state.room != index as i32 {
                state.changed = true;
            }
            state.room = index as i32;
        }
        if self.forced_package != 0
            && let Some(index) = self
                .rooms
                .iter()
                .enumerate()
                .position(|(i, r)| r.hash == self.forced_package && state.package != i as i32)
        {
            state.changed = true;
            state.package = index as i32;
        }
        if state.room >= 0 && state.room != state.last_room {
            state.changed = true;
        }
    }
}

/// An array argument's integer elements by index from 0, at most 16.
fn int_elements(world: &World, args: &[Value], index: usize) -> Result<Vec<i32>, String> {
    let id = array_id(args, index)?;
    let runtime = world.resource::<RoundScript>();
    let Some(array) = runtime.arrays.get(&id) else {
        return Ok(Vec::new());
    };
    Ok((0..array.len().min(ROOM_TRIGGERS) as i32)
        .filter_map(|i| match array.get(&ArrayKey::Integer(i)) {
            Some(Value::Int(n)) => Some(*n),
            _ => None,
        })
        .collect())
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};
    // setambientpackage(hasroom, haspackage, _, defaultroom, room,
    // triggerentnums, [triggerpriorities]): describes a room (or package)
    // and the triggers that name it; the third parameter is not read. The
    // first 16 integer entries count; a priority of 99 is -1.
    registry.register(Function, "setambientpackage", |world, _, args| {
        let has_room = int(args, 0)? == 1;
        let has_package = int(args, 1)? == 1;
        let default_room = int(args, 3)? == 1;
        let name = string(args, 4)?.to_string();
        let mut triggers = [0u16; ROOM_TRIGGERS];
        for (slot, number) in int_elements(world, args, 5)?.into_iter().enumerate() {
            triggers[slot] = number as u16;
        }
        let mut priorities = [0i8; ROOM_TRIGGERS];
        if args.len() >= 7 {
            for (slot, priority) in int_elements(world, args, 6)?.into_iter().enumerate() {
                priorities[slot] = if priority == 99 { -1 } else { priority as i8 };
            }
        }
        let hash = sound_hash(&name);
        let mut ambient = world.get_resource_or_insert_with(AmbientRooms::default);
        let index = match ambient.rooms.iter().position(|room| room.hash == hash) {
            Some(index) => index,
            None if ambient.rooms.len() < ROOMS => {
                ambient.rooms.push(Room {
                    has_room,
                    has_package,
                    default_room,
                    name: String::new(),
                    hash,
                    triggers,
                    priorities,
                    contexts: [(0, 0); ROOM_CONTEXTS],
                });
                ambient.rooms.len() - 1
            }
            None => return Ok(Value::Undefined),
        };
        let room = &mut ambient.rooms[index];
        room.has_room = has_room;
        room.has_package = has_package;
        room.default_room = default_room;
        room.name = name;
        room.triggers = triggers;
        room.priorities = priorities;
        if default_room {
            ambient.default_room = Some(index);
        }
        Ok(Value::Undefined)
    });
    // setambientroomcontextengine(room, context, value, [flag]): sets a
    // context on the room, or with the flag on every default room, for a
    // context type the driver has.
    registry.register(Function, "setambientroomcontextengine", |world, _, args| {
        let room = sound_hash(&string(args, 0)?);
        let kind = string(args, 1)?;
        let value = string(args, 2)?;
        let flag = args.len() >= 4 && int(args, 3)? == 1;
        let Some(names) = mix_names(world) else {
            return Ok(Value::Undefined);
        };
        let kind = sound_hash(&kind);
        let value = sound_hash(&value);
        let slot = names.contexts.iter().position(|(k, _)| *k == kind);
        let mut ambient = world.get_resource_or_insert_with(AmbientRooms::default);
        for target in ambient.rooms.iter_mut() {
            if ((flag && target.default_room) || target.hash == room)
                && let Some(slot) = slot.filter(|&slot| slot < ROOM_CONTEXTS)
            {
                target.contexts[slot] = (kind, value);
            }
        }
        Ok(Value::Undefined)
    });
    // forceambientroom(force, _, [room]) / forceambientpackage(force, _,
    // [package]): forces a room or package on every state, or with force
    // other than 1 ends the forcing; fewer than two parameters do nothing.
    registry.register(Function, "forceambientroom", |world, _, args| {
        if let Some(hash) = forced(args)? {
            world
                .get_resource_or_insert_with(AmbientRooms::default)
                .forced_room = hash;
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "forceambientpackage", |world, _, args| {
        if let Some(hash) = forced(args)? {
            world
                .get_resource_or_insert_with(AmbientRooms::default)
                .forced_package = hash;
        }
        Ok(Value::Undefined)
    });
    // trigger setambienttrigger(): lets actors touch the trigger for their
    // ambient state.
    registry.register(Method, "setambienttrigger", |world, receiver, _| {
        let id = world
            .resource::<RoundScript>()
            .entity(receiver)
            .map(|(id, _)| id)
            .ok_or("not an entity")?;
        let mut ambient = world.get_resource_or_insert_with(AmbientRooms::default);
        if !ambient.actor_triggers.contains(&id) {
            ambient.actor_triggers.push(id);
        }
        Ok(Value::Undefined)
    });
}

/// The name hash `forceambientroom` and `forceambientpackage` set: none
/// with fewer than two parameters, 0 unless forcing.
fn forced(args: &[Value]) -> Result<Option<u32>, String> {
    if args.len() <= 1 {
        return Ok(None);
    }
    let force = int(args, 0)? == 1;
    int(args, 1)?;
    let hash = if args.len() > 2 {
        sound_hash(&string(args, 2)?)
    } else {
        0
    };
    Ok(Some(if force { hash } else { 0 }))
}

/// The ambient triggers the local player touches this frame, by entity
/// number.
fn touched_triggers(client: &mut World, player_number: i32) -> Vec<i32> {
    super::touch::touched(client, player_number)
        .into_iter()
        .filter(|&(object, _)| {
            client
                .resource_mut::<RoundScript>()
                .object_field(object, "targetname")
                == Value::string("ambient_package")
        })
        .map(|(_, number)| number)
        .collect()
}

/// The engine's ambient pass for a client frame: each ambient trigger the
/// local player touches picks its room and package; a change of room for
/// the listener calls `soundsetambientstate` and sets the room's contexts
/// on the player's sound entity.
// t6.csc.sound.ambient: only the local player is followed and is always the
// listener; actors' ambient state (`soundsetaiambientstate`) is not run, and
// the touches are this frame's, where the engine's are the frame before's.
pub(crate) fn update(client: &mut World, now: i64) {
    let Some(player_number) = client
        .get_resource::<ClientGame>()
        .and_then(|game| game.local_player)
    else {
        return;
    };
    if !client.contains_resource::<AmbientRooms>() {
        return;
    }
    let touched = touched_triggers(client, player_number);
    let player_object = client
        .get_resource::<ClientGame>()
        .and_then(|game| game.entities.get(&player_number).map(|e| e.object));
    let callback = client
        .get_resource::<ClientCallbacks>()
        .and_then(|c| c.sound_set_ambient_state.clone());
    let mut calls = Vec::new();
    let mut contexts = Vec::new();
    {
        let mut ambient = client.resource_mut::<AmbientRooms>();
        for trigger in touched {
            let at = match ambient
                .states
                .iter()
                .position(|s| s.active && s.entity == player_number)
            {
                Some(at) => at,
                None => match ambient.states.iter().position(|s| !s.active) {
                    Some(at) => {
                        ambient.states[at].active = true;
                        at
                    }
                    None => continue,
                },
            };
            ambient.states[at].entity = player_number;
            ambient.states[at].trigger = trigger;
            ambient.pick(at);
        }
        for at in 0..STATES {
            if !ambient.states[at].active {
                continue;
            }
            ambient.force(at);
            let state = ambient.states[at];
            if state.changed && state.room >= 0 {
                let listener = state.entity == player_number;
                let room = ambient.rooms[state.room as usize].clone();
                let package = (state.package >= 0)
                    .then(|| ambient.rooms[state.package as usize].name.clone());
                if listener && state.room != state.last_room {
                    calls.push((room.name.clone(), package));
                }
                contexts.extend(
                    room.contexts
                        .iter()
                        .filter(|(kind, _)| *kind != 0)
                        .map(|&(kind, value)| (state.entity, kind, value)),
                );
                // The engine then clears the state but for its entity, and
                // keeps the room as the last one, told or not.
                ambient.states[at] = State {
                    last_room: state.room,
                    entity: state.entity,
                    active: true,
                    ..State::default()
                };
            }
            ambient.states[at].room = -1;
            ambient.states[at].package = -1;
        }
    }
    let names = mix_names(client);
    for (entity, kind, value) in contexts {
        let known = names.as_ref().is_some_and(|names| {
            names
                .contexts
                .iter()
                .any(|(k, values)| *k == kind && (value == 0 || values.contains(&value)))
        });
        if known {
            mix_command(
                client,
                MixCommand::EntityContext {
                    entity,
                    kind,
                    value,
                },
            );
        }
    }
    let (Some(callback), Some(player)) = (callback, player_object) else {
        return;
    };
    for (room, package) in calls {
        if crate::sound_trace::enabled() {
            diag::info!(
                Sim,
                "sound trace: ambient room={room} package={}",
                package.as_deref().unwrap_or("")
            );
        }
        let has_package = package.is_some();
        let args = vec![
            Value::string(&room),
            Value::string(package.as_deref().unwrap_or("")),
            Value::Object(player),
            if has_package {
                Value::Object(player)
            } else {
                Value::Undefined
            },
            Value::Int(0),
        ];
        if let Err(fault) =
            crate::script::runtime::run_now(client, &callback, Value::level(), args, now)
        {
            diag::warn!(Sim, "csc: {callback}: {fault}");
        }
    }
}
