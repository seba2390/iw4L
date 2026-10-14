//! Black Ops' client VM natives (its client scripts, CSC), read from its
//! client function and method tables. A PC client has one local client, 0.

use super::super::args::{int, optional, string, vector};
use super::super::arrays::new_array;
use super::super::entities::EntityKind;
use crate::script::client::{ClientOutput, ClientSound, ClientView};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

/// The client's fake entities (`spawnfakeent`): sound emitters of its own,
/// by handle.
#[derive(Resource, Default)]
struct FakeEnts {
    next: i32,
    origins: BTreeMap<i32, [f32; 3]>,
}

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
    // The client's entities a key names (`getent(local, value, key)`).
    registry.register(Function, "getent", |world, _, args| {
        local_index(args)?;
        let ids = super::engine::matching_entities(world, &args[1..])?;
        super::engine::single(ids, "getent")
    });
    registry.register(Function, "getentarray", |world, _, args| {
        local_index(args)?;
        let ids = super::engine::matching_entities(world, &args[1..])?;
        super::engine::objects(world, ids)
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
        Ok(Value::Object(id))
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
