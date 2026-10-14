//! Black Ops' client VM natives (its client scripts, CSC), read from its
//! client function and method tables. A PC client has one local client, 0.

use super::super::args::{arg, float, int, optional, string, vector};
use super::super::arrays::new_array;
use super::super::entities::EntityKind;
use crate::script::client::{
    ClientEffect, ClientEffectTable, ClientOutput, ClientSound, ClientTagEffect, ClientTagEvent,
    ClientView, FakeEnts, LoopSource,
};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;

pub(crate) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};
    // `playSound(local, alias, origin, volume)`: a sound the client plays at
    // a point (at none without one), answering its id.
    registry.register(Function, "playsound", |world, _, args| {
        local_index(args)?;
        let alias = string(args, 1)?;
        let origin = optional(args, 2, vector)?.unwrap_or([0.0; 3]);
        Ok(play(world, alias, origin))
    });
    // `ent playSound(local, alias, origin, volume)`: at the entity's origin
    // without one.
    registry.register(Method, "playsound", |world, receiver, args| {
        local_index(args)?;
        let id = super::engine::entity_id(world, receiver)?;
        let alias = string(args, 1)?;
        let origin = match args.get(2) {
            Some(_) if args.len() == 3 => vector(args, 2)?,
            _ => match super::super::players::entity_field(world, id, "origin") {
                Value::Vector(origin) => origin,
                _ => [0.0; 3],
            },
        };
        Ok(play(world, alias, origin))
    });
    // Every local client is active; there is only client 0.
    registry.register(Function, "getlocalplayers", |world, _, _| {
        let players = locals(world).into_iter().map(Value::Object).collect();
        new_array(world, players)
    });
    registry.register(Function, "getlocalplayer", |world, _, args| {
        let local = local_index(args)?;
        Ok(locals(world)
            .get(local)
            .map_or(Value::Undefined, |&object| Value::Object(object)))
    });
    registry.register(Function, "clienthassnapshot", |world, _, args| {
        let local = local_index(args)?;
        let view = world.resource::<ClientView>();
        Ok(Value::Int(
            view.locals
                .get(local)
                .is_some_and(|client| client.has_snapshot)
                .into(),
        ))
    });
    registry.register(Function, "getmaxlocalclients", |_, _, _| Ok(Value::Int(1)));
    registry.register(Function, "localclientactive", |_, _, _| Ok(Value::Int(1)));
    registry.register(Function, "issplitscreen", |_, _, _| Ok(Value::Int(0)));
    // The local client's host flag: set on the listen server's client.
    registry.register(Function, "issplitscreenhost", |world, _, args| {
        let _ = int(args, 0)?;
        Ok(Value::Int(world.resource::<ClientView>().host.into()))
    });
    registry.register(Method, "issplitscreenhost", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        let is_local = locals(world).contains(&id);
        Ok(Value::Int(
            (is_local && world.resource::<ClientView>().host).into(),
        ))
    });
    // The client's entities a key names (`getent(local, value, key)`).
    registry.register(Function, "getent", |world, _, args| {
        if args.len() != 3 {
            return Err("illegal call to getent()\n".into());
        }
        local_index(args)?;
        let ids = super::engine::matching_entities(world, &args[1..])?;
        super::engine::single(ids, "getent")
    });
    registry.register(Function, "getentarray", |world, _, args| {
        local_index(args)?;
        let ids = if args.len() == 1 {
            let runtime = world.resource::<RoundScript>();
            let world_ent = runtime.engine.world;
            runtime
                .entities
                .iter()
                .filter(|(id, e)| e.kind != EntityKind::HudElem && Some(**id) != world_ent)
                .map(|(id, _)| *id)
                .collect()
        } else {
            super::engine::matching_entities(world, &args[1..])?
        };
        super::engine::objects(world, ids)
    });
    registry.register(Method, "getentitynumber", |world, receiver, _| {
        let (_, ent) = entity(world, receiver)?;
        Ok(Value::Int(ent.number))
    });
    registry.register(Method, "getentnum", |world, receiver, _| {
        let (_, ent) = entity(world, receiver)?;
        Ok(Value::Int(ent.number))
    });
    registry.register(Function, "getentnum", |world, _, args| {
        local_index(args)?;
        let (_, ent) = entity(world, super::super::args::arg(args, 1)?)?;
        Ok(Value::Int(ent.number))
    });
    registry.register(Method, "getorigin", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        let origin = match super::super::players::entity_field(world, id, "origin") {
            Value::Vector(origin) => origin,
            _ => [0.0; 3],
        };
        Ok(Value::Vector(origin))
    });
    registry.register(Method, "isplayer", |world, receiver, _| {
        let (_, ent) = entity(world, receiver)?;
        Ok(Value::Int((ent.kind == EntityKind::Player).into()))
    });
    registry.register(Method, "islocalplayer", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        Ok(Value::Int(locals(world).contains(&id).into()))
    });
    registry.register(Method, "getlocalclientnumber", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        let index = locals(world)
            .into_iter()
            .position(|local| local == id)
            .ok_or_else(|| "You can only call GetLocalClientNumber on players.".to_owned())?;
        Ok(Value::Int(index as i32))
    });
    registry.register(Method, "isspectating", |world, receiver, _| {
        super::engine::entity_id(world, receiver)?;
        Ok(Value::Int(0))
    });
    registry.register(Method, "isai", |world, receiver, _| {
        let (_, ent) = entity(world, receiver)?;
        Ok(Value::Int((ent.kind == EntityKind::Actor).into()))
    });
    registry.register(Method, "isalive", |world, receiver, _| {
        is_alive(world, receiver)
    });
    registry.register(Function, "isalive", |world, _, args| {
        let ent = super::super::args::arg(args, 0)?;
        if !matches!(ent, Value::Object(_)) {
            return Err("IsAlive param isn't an entity.".into());
        }
        is_alive(world, ent)
    });
    registry.register(Method, "hasdobj", |world, receiver, args| {
        if args.len() != 1 {
            return Err("HasDObj: Must be passed the local client number".into());
        }
        local_index(args)?;
        let (id, is_player) = {
            let (id, ent) = entity(world, receiver)?;
            (id, ent.kind == EntityKind::Player)
        };
        let has_model = matches!(
            super::super::players::entity_field(world, id, "model"),
            Value::String(s) if !s.is_empty()
        );
        Ok(Value::Int((is_player || has_model).into()))
    });
    registry.register(Method, "clientflag", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        let bit = int(args, 0)?;
        if !(0..16).contains(&bit) {
            return Err(format!("ClientFlag: Index {bit} out of range (0 - 15)\n"));
        }
        let flags = match super::super::players::entity_field(world, id, "serverflags") {
            Value::Int(flags) => flags,
            _ => 0,
        };
        Ok(Value::Int((flags >> bit) & 1))
    });
    registry.register(Method, "haseyes", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        let eyes = match super::super::players::entity_field(world, id, "haseyes") {
            Value::Int(eyes) => eyes != 0,
            _ => false,
        };
        Ok(Value::Int(eyes.into()))
    });
    // The client's real time: its frames' time.
    registry.register(Function, "getrealtime", |world, _, _| {
        let tick = world.resource::<crate::step::StepRequest>().tick;
        Ok(Value::Int(
            (u64::from(tick.0) * u64::from(crate::MATCH_TICK_MS)) as i32,
        ))
    });
    registry.register(Function, "spawnfakeent", |world, _, args| {
        local_index(args)?;
        let mut fakes = world.get_resource_or_insert_with(FakeEnts::default);
        let handle = fakes.next;
        fakes.next += 1;
        fakes.origins.insert(handle, [0.0; 3]);
        Ok(Value::Int(handle))
    });
    registry.register(Function, "setfakeentorg", |world, _, args| {
        local_index(args)?;
        let handle = int(args, 1)?;
        let origin = vector(args, 2)?;
        if let Some(at) = world
            .get_resource_or_insert_with(FakeEnts::default)
            .origins
            .get_mut(&handle)
        {
            *at = origin;
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "deletefakeent", |world, _, args| {
        local_index(args)?;
        let handle = int(args, 1)?;
        world
            .get_resource_or_insert_with(FakeEnts::default)
            .origins
            .remove(&handle);
        Ok(Value::Undefined)
    });
    // `spawn(local, origin, classname)`: an entity of the client's own.
    registry.register(Function, "spawn", |world, _, args| {
        local_index(args)?;
        let origin = vector(args, 1)?;
        let classname = optional(args, 2, string)?.unwrap_or_else(String::new);
        let mut runtime = world.resource_mut::<RoundScript>();
        let id = runtime.create_entity(EntityKind::Spawned, &classname)?;
        runtime.set_object_field(id, "origin", Value::Vector(origin));
        let ent_type = if matches!(classname.as_str(), "script_model" | "script_origin") {
            "scriptmover"
        } else {
            "general"
        };
        runtime.set_object_field(id, "type", Value::string(ent_type));
        runtime.set_object_field(id, "model", Value::string(""));
        runtime.set_object_field(id, "serverflags", Value::Int(0));
        runtime.set_object_field(id, "haseyes", Value::Int(0));
        Ok(Value::Object(id))
    });
    // `playLoopSound(local, fakeent, alias, fade)`: a loop at a fake
    // entity; `stopLoopSound(local, fakeent, fade)` ends it.
    registry.register(Function, "playloopsound", |world, _, args| {
        local_index(args)?;
        let handle = int(args, 1)?;
        let alias = string(args, 2)?;
        loop_fade(args, 3, "playloopsound")?;
        world
            .resource_mut::<ClientOutput>()
            .start_loop(LoopSource::Fake(handle), alias);
        Ok(Value::Undefined)
    });
    registry.register(Function, "stoploopsound", |world, _, args| {
        local_index(args)?;
        let handle = int(args, 1)?;
        loop_fade(args, 2, "stoploopsound")?;
        world
            .resource_mut::<ClientOutput>()
            .loops
            .remove(&LoopSource::Fake(handle));
        Ok(Value::Undefined)
    });
    // `ent playLoopSound(alias, fade)`, `ent stopLoopSound(fade)`.
    registry.register(Method, "playloopsound", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        let alias = string(args, 0)?;
        loop_fade(args, 1, "playloopsound")?;
        world
            .resource_mut::<ClientOutput>()
            .start_loop(LoopSource::Entity(id), alias);
        Ok(Value::Undefined)
    });
    registry.register(Method, "stoploopsound", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        loop_fade(args, 0, "stoploopsound")?;
        world
            .resource_mut::<ClientOutput>()
            .loops
            .remove(&LoopSource::Entity(id));
        Ok(Value::Undefined)
    });
    // `soundLoopEmitter(alias, origin)`: a loop at a point, until
    // `soundStopLoopEmitter(alias, origin)`.
    registry.register(Function, "soundloopemitter", |world, _, args| {
        let source = emitter(args, "soundloopemitter")?;
        let LoopSource::Emitter(alias, _) = &source else {
            unreachable!()
        };
        let alias = alias.clone();
        world
            .resource_mut::<ClientOutput>()
            .start_loop(source, alias);
        Ok(Value::Undefined)
    });
    registry.register(Function, "soundstoploopemitter", |world, _, args| {
        let source = emitter(args, "soundstoploopemitter")?;
        world.resource_mut::<ClientOutput>().loops.remove(&source);
        Ok(Value::Undefined)
    });
    // Whether the entity is under water: the runtime takes Black Ops'
    // players as dry (`t5.movement.water`).
    registry.register(Method, "underwater", |world, receiver, _| {
        super::engine::entity_id(world, receiver)?;
        Ok(Value::Int(0))
    });
    register_effects(registry);
    // `ent delete()`: a client-only entity notifies `death` and
    // `entityshutdown`, then is freed.
    registry.register(Method, "delete", |world, receiver, _| {
        let (id, number) = {
            let (id, ent) = entity(world, receiver)?;
            (id, ent.number)
        };
        if number < CLIENT_ENTITY_BASE {
            return Err("delete: the client can only delete its own entities".into());
        }
        crate::script::runtime::raise(world, Value::Object(id), "death", Vec::new());
        crate::script::runtime::raise(world, Value::Object(id), "entityshutdown", Vec::new());
        let mut output = world.resource_mut::<ClientOutput>();
        output.fx_entities.remove(&id);
        output.loops.remove(&LoopSource::Entity(id));
        world.resource_mut::<RoundScript>().request_delete(id);
        Ok(Value::Undefined)
    });
    registry.register(Function, "ispc", |_, _, _| Ok(Value::Int(1)));
    registry.register(Function, "isps3", |_, _, _| Ok(Value::Int(0)));
    registry.register(Function, "isxenon", |_, _, _| Ok(Value::Int(0)));
}

/// The first client-only entity number; lower ones are the server's
/// entities in the client's snapshot.
const CLIENT_ENTITY_BASE: i32 = 0x400;

/// The client's effect slots: its effect configstrings 1 to 195.
const EFFECT_IDS: std::ops::RangeInclusive<i32> = 1..=0xc3;

fn register_effects(registry: &mut NativeRegistry) {
    use Namespace::Function;
    // `loadFx(name)`: the index the server registered the effect at, looked
    // up without case.
    registry.register(Function, "loadfx", |world, _, args| {
        let name = string(args, 0)?;
        if name
            .get(..3)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("fx/"))
        {
            return Err("effect name should start after the 'fx' folder.".into());
        }
        let table = world.resource::<ClientEffectTable>();
        match table.indices.get(&name.to_ascii_lowercase()) {
            Some(&index) => Ok(Value::Int(index)),
            None => Err(format!(
                "loadedfx could not find effect {name} - ensure that effect is precached in the server script, and included in appropriate CSVs.\n"
            )),
        }
    });
    // `playFx(local, id, origin, [forward], [up], [begin])`: an effect at a
    // point, answering its handle. Its optional last argument, in seconds,
    // is the time it begins at, not a delay from now.
    registry.register(Function, "playfx", |world, _, args| {
        if !(3..=6).contains(&args.len()) {
            return Err("Incorrect number of parameters".into());
        }
        local_index(args)?;
        let now = now_ms(world);
        let id = effect_id(args, 1)?;
        let origin = vector(args, 2)?;
        let (axis, begin) = match args.len() {
            3 => (math_iw4::angles_to_axis(DEFAULT_EFFECT_ANGLES), now),
            4 => {
                let forward = direction(world, args, 3, id, "forward")?;
                let angles = math_iw4::vect_to_angles(forward).map(math_iw4::angle_normalize_360);
                (math_iw4::angles_to_axis(angles), now)
            }
            _ => {
                let forward = direction(world, args, 3, id, "forward")?;
                let up = direction(world, args, 4, id, "up")?;
                let begin = if args.len() > 5 {
                    seconds_to_msec(float(args, 5)?)
                } else {
                    now
                };
                (forward_up_axis(forward, up)?, begin)
            }
        };
        Ok(Value::Int(play_effect(world, id, origin, axis, begin)))
    });
    // `playFxOnTag(local, id, entity, tag)`: an effect bolted to the
    // entity's tag, answering its handle.
    registry.register(Function, "playfxontag", |world, _, args| {
        if args.len() != 4 {
            return Err("Incorrect number of parameters for playfxontag".into());
        }
        local_index(args)?;
        let raw_id = int(args, 1)?;
        let (entity, _) = entity(world, arg(args, 2)?)?;
        let id = effect_id(args, 1).map(|_| raw_id)?;
        let tag = string(args, 3)?;
        let now = now_ms(world);
        let (name, life) = effect_name_life(world, id);
        let mut output = world.resource_mut::<ClientOutput>();
        let handle = output.effect_handle();
        if let Some(name) = name {
            let effect = ClientTagEffect {
                name,
                entity,
                tag,
                end_ms: now.saturating_add(life),
            };
            output.tag_events.push(ClientTagEvent {
                effect: effect.clone(),
                stop: false,
            });
            output.tag_effects.insert(handle, effect);
        }
        Ok(Value::Int(handle))
    });
    // `spawnFx(local, id, origin, delay, [forward], [up])`: an effect
    // entity, which plays once `triggerFx` starts it; the delay is read
    // and unused.
    registry.register(Function, "spawnfx", |world, _, args| {
        if !(4..=6).contains(&args.len()) {
            return Err("Incorrect number of parameters".into());
        }
        local_index(args)?;
        let id = effect_id(args, 1)?;
        let origin = vector(args, 2)?;
        float(args, 3)?;
        let axis = match args.len() {
            4 => math_iw4::angles_to_axis(DEFAULT_EFFECT_ANGLES),
            5 => {
                let forward = direction(world, args, 4, id, "forward")?;
                let angles = math_iw4::vect_to_angles(forward).map(math_iw4::angle_normalize_360);
                math_iw4::angles_to_axis(angles)
            }
            _ => {
                let forward = direction(world, args, 4, id, "forward")?;
                let up = direction(world, args, 5, id, "up")?;
                forward_up_axis(forward, up)?
            }
        };
        let angles = math_iw4::axis_to_angles(axis);
        let (name, _) = effect_name_life(world, id);
        let mut runtime = world.resource_mut::<RoundScript>();
        let Ok(entity) = runtime.create_entity(EntityKind::Spawned, "") else {
            return Ok(Value::Undefined);
        };
        runtime.set_object_field(entity, "origin", Value::Vector(origin));
        runtime.set_object_field(entity, "angles", Value::Vector(angles));
        runtime.set_object_field(entity, "type", Value::string("NA"));
        runtime.set_object_field(entity, "model", Value::string(""));
        runtime.set_object_field(entity, "serverflags", Value::Int(0));
        runtime.set_object_field(entity, "haseyes", Value::Int(0));
        world.resource_mut::<ClientOutput>().fx_entities.insert(
            entity,
            ClientEffect {
                name: name.unwrap_or_else(String::new),
                origin,
                forward: axis[0],
                up: axis[2],
                begin_ms: None,
                end_ms: 0,
            },
        );
        Ok(Value::Object(entity))
    });
    // `triggerFx(entity, [begin])`: starts an effect entity now, or at the
    // time it names in seconds.
    registry.register(Function, "triggerfx", |world, _, args| {
        if args.is_empty() || args.len() >= 3 {
            return Err("Incorrect number of parameters".into());
        }
        let (entity, _) = entity(world, arg(args, 0)?)?;
        let now = now_ms(world);
        let begin = if args.len() == 2 {
            seconds_to_msec(float(args, 1)?)
        } else {
            now
        };
        let life = match world.resource::<ClientOutput>().fx_entities.get(&entity) {
            Some(effect) => world.resource::<ClientEffectTable>().life(&effect.name),
            None => return Err("entity wasn't created with 'newFx'".into()),
        };
        if let Some(effect) = world
            .resource_mut::<ClientOutput>()
            .fx_entities
            .get_mut(&entity)
        {
            effect.begin_ms = Some(begin);
            effect.end_ms = begin.saturating_add(life).max(now);
        }
        Ok(Value::Undefined)
    });
    // `stopFx(local, handle)` and `deleteFx(local, handle, [immediate])`
    // kill the effect a handle names.
    registry.register(Function, "stopfx", |world, _, args| {
        if args.len() != 2 {
            return Err("Incorrect number of parameters for IsFXPlaying".into());
        }
        local_index(args)?;
        let handle = int(args, 1)?;
        world.resource_mut::<ClientOutput>().kill_effect(handle);
        Ok(Value::Undefined)
    });
    registry.register(Function, "deletefx", |world, _, args| {
        if !(2..=3).contains(&args.len()) {
            return Err("Incorrect number of parameters for DeleteFX".into());
        }
        local_index(args)?;
        let handle = int(args, 1)?;
        if args.len() > 2 {
            int(args, 2)?;
        }
        world.resource_mut::<ClientOutput>().kill_effect(handle);
        Ok(Value::Undefined)
    });
    // `isFxPlaying(local, handle)`: whether the effect is still alive.
    registry.register(Function, "isfxplaying", |world, _, args| {
        if args.len() != 2 {
            return Err("Incorrect number of parameters for IsFXPlaying".into());
        }
        local_index(args)?;
        let handle = int(args, 1)?;
        let now = now_ms(world);
        let alive = world
            .resource_mut::<ClientOutput>()
            .effect_alive(handle, now);
        Ok(Value::Int(alive.into()))
    });
}

/// An effect's orientation when the script gives none: straight up.
const DEFAULT_EFFECT_ANGLES: [f32; 3] = [270.0, 0.0, 0.0];

/// The client's time now: the level time of its snapshot.
fn now_ms(world: &World) -> i32 {
    crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick)
}

/// Seconds as the client rounds them to msec.
fn seconds_to_msec(seconds: f32) -> i32 {
    (f64::from(seconds * 1000.0) + 9.313_225_746_154_785e-10).round_ties_even() as i32
}

fn effect_id(args: &[Value], at: usize) -> Result<i32, String> {
    let id = int(args, at)?;
    if !EFFECT_IDS.contains(&id) {
        return Err(format!("CScr_PlayFX: invalid effect id {id}"));
    }
    Ok(id)
}

/// The effect registered at an index, and how long it lives.
fn effect_name_life(world: &World, id: i32) -> (Option<String>, i32) {
    let table = world.resource::<ClientEffectTable>();
    let name = table.names.get(&id).cloned();
    let life = name.as_deref().map_or(0, |name| table.life(name));
    (name, life)
}

/// A direction argument, normalized; a zero one is the script's error.
fn direction(
    world: &World,
    args: &[Value],
    at: usize,
    id: i32,
    which: &str,
) -> Result<[f32; 3], String> {
    let v = vector(args, at)?;
    let length = (v[1] * v[1] + v[2] * v[2] + v[0] * v[0]).sqrt();
    let scale = 1.0 / if 0.0 <= -length { 1.0 } else { length };
    if length == 0.0 {
        let effect = world
            .resource::<ClientEffectTable>()
            .names
            .get(&id)
            .filter(|name| !name.is_empty())
            .cloned()
            .unwrap_or_else(|| "not successfully loaded".into());
        return Err(format!(
            "playFx called with (0 0 0) {which} direction (effect = {effect})\n"
        ));
    }
    Ok(v.map(|c| c * scale))
}

/// The axis of a forward and an up direction: up made square to forward.
fn forward_up_axis(forward: [f32; 3], up: [f32; 3]) -> Result<[[f32; 3]; 3], String> {
    let along = -(up[2] * forward[2] + up[1] * forward[1] + up[0] * forward[0]);
    let up = [
        forward[0] * along + up[0],
        along * forward[1] + up[1],
        forward[2] * along + up[2],
    ];
    let length = (up[2] * up[2] + up[0] * up[0] + up[1] * up[1]).sqrt();
    let scale = 1.0 / if 0.0 <= -length { 1.0 } else { length };
    if length == 0.0 {
        return Err(
            "forward and up vectors are the same direction or exact opposite directions".into(),
        );
    }
    let up = up.map(|c| c * scale);
    let left = [
        up[1] * forward[2] - up[2] * forward[1],
        up[2] * forward[0] - forward[2] * up[0],
        forward[1] * up[0] - up[1] * forward[0],
    ];
    Ok([forward, left, up])
}

/// Plays an effect at a point from `begin`, answering its handle: it is
/// alive until its longest life has passed, and at least this frame.
fn play_effect(
    world: &mut World,
    id: i32,
    origin: [f32; 3],
    axis: [[f32; 3]; 3],
    begin: i32,
) -> i32 {
    let now = now_ms(world);
    let (name, life) = effect_name_life(world, id);
    let mut output = world.resource_mut::<ClientOutput>();
    let handle = output.effect_handle();
    if let Some(name) = name {
        output.effects.insert(
            handle,
            ClientEffect {
                name,
                origin,
                forward: axis[0],
                up: axis[2],
                begin_ms: Some(begin),
                end_ms: begin.saturating_add(life).max(now),
            },
        );
    }
    handle
}

fn play(world: &mut World, alias: String, origin: [f32; 3]) -> Value {
    let mut output = world.resource_mut::<ClientOutput>();
    let id = output.next_sound;
    output.next_sound += 1;
    output.sounds.push(ClientSound { alias, origin });
    Value::Int(id)
}

/// A loop's fade: 0 to 32 seconds (the fade itself is not applied yet).
fn loop_fade(args: &[Value], at: usize, native: &str) -> Result<(), String> {
    if let Some(fade) = optional(args, at, super::super::args::float)?
        && !(0.0..=32.0).contains(&fade)
    {
        return Err(format!(
            "{native}: invalid fade value {fade:.6}. it must be between 0 and 32 seconds."
        ));
    }
    Ok(())
}

/// A point emitter's alias and origin, which must be its two arguments.
fn emitter(args: &[Value], native: &str) -> Result<LoopSource, String> {
    if args.len() != 2 {
        return Err(format!(
            "{native} has {} parameters.  There should be two.",
            args.len()
        ));
    }
    let alias = string(args, 0)?;
    let origin = vector(args, 1)?;
    Ok(LoopSource::Emitter(alias, origin.map(f32::to_bits)))
}

/// The local client a native names: Black Ops refuses any but 0.
fn local_index(args: &[Value]) -> Result<usize, String> {
    let local = int(args, 0)?;
    if local != 0 {
        return Err(format!(
            "Trying to get a local client index for a client '{local}' that is not a local client."
        ));
    }
    Ok(0)
}

fn locals(world: &World) -> Vec<u64> {
    world
        .resource::<ClientView>()
        .locals
        .iter()
        .map(|client| client.object)
        .collect()
}

fn entity<'a>(
    world: &'a World,
    receiver: &Value,
) -> Result<(u64, &'a super::super::entities::ScriptEntity), String> {
    world
        .resource::<RoundScript>()
        .entity(receiver)
        .ok_or_else(|| "not an entity".into())
}

fn is_alive(world: &mut World, receiver: &Value) -> Result<Value, String> {
    let id = super::engine::entity_id(world, receiver)?;
    let alive = matches!(
        super::super::players::entity_field(world, id, "type"),
        Value::String(kind) if matches!(&*kind, "player" | "actor" | "vehicle" | "plane")
    );
    if !alive {
        return Err("IsAlive called on the wrong kind of entity".into());
    }
    Ok(Value::Int(1))
}
