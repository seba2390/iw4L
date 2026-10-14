//! Black Ops' client VM natives (its client scripts, CSC), read from its
//! client function and method tables. A PC client has one local client, 0.

use super::super::args::{int, optional, string, vector};
use super::super::arrays::new_array;
use super::super::entities::EntityKind;
use crate::script::client::{ClientOutput, ClientSound, ClientView, FakeEnts, LoopSource};
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
    registry.register(Function, "ispc", |_, _, _| Ok(Value::Int(1)));
    registry.register(Function, "isps3", |_, _, _| Ok(Value::Int(0)));
    registry.register(Function, "isxenon", |_, _, _| Ok(Value::Int(0)));
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
