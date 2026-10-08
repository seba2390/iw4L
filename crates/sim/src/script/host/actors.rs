use super::entities::EntityKind;
use super::natives::engine::entity_id;
use crate::script::runtime::{raise, run_now};
use crate::script::{Namespace, NativeRegistry, Runtime, Value};
use bevy_ecs::prelude::World;

/// The spawner keys an actor keeps as its own; the rest describe the spawner.
const SPAWNER_ONLY_FIELDS: [&str; 3] = ["count", "spawnflags", "export"];

pub(crate) fn register(registry: &mut NativeRegistry) {
    use Namespace::Method;
    registry.register(Method, "codespawnerforcespawn", |world, receiver, _| {
        spawn_from(world, receiver)
    });
    registry.register(Method, "dospawn", |world, receiver, _| {
        spawn_from(world, receiver)
    });
}

/// Spawns an actor from a map spawner: it takes the spawner's place and keys,
/// runs its aitype script, and the spawner is told `spawned`.
fn spawn_from(world: &mut World, spawner: &Value) -> Result<Value, String> {
    let spawner_id = entity_id(world, spawner)?;
    let (classname, count) = {
        let mut runtime = world.resource_mut::<Runtime>();
        let classname = runtime
            .entities
            .get(&spawner_id)
            .map(|entity| entity.classname.clone())
            .ok_or("spawner is gone")?;
        (classname, runtime.object_field(spawner_id, "count"))
    };
    let Some(aitype) = classname.strip_prefix("actor_") else {
        return Err(format!("{classname} is not an actor spawner"));
    };
    if matches!(count, Value::Int(n) if n <= 0) {
        return Ok(Value::Undefined);
    }
    let (origin, angles) = {
        let mut runtime = world.resource_mut::<Runtime>();
        let pose = |runtime: &mut Runtime, name| match runtime.object_field(spawner_id, name) {
            Value::Vector(v) => v,
            _ => [0.0; 3],
        };
        (pose(&mut runtime, "origin"), pose(&mut runtime, "angles"))
    };

    let presence = super::presence::spawn_presence(world, origin)?;
    let number = crate::frame::FrameWorld::from_world(world).gentity_number(presence);
    let id = {
        let mut runtime = world.resource_mut::<Runtime>();
        let id = runtime.create_entity(EntityKind::Actor, &classname)?;
        let skipped: Vec<u32> = SPAWNER_ONLY_FIELDS
            .iter()
            .map(|name| runtime.symbol(name))
            .collect();
        let inherited: Vec<(u32, Value)> = runtime
            .objects
            .get(&spawner_id)
            .into_iter()
            .flatten()
            .filter(|(symbol, _)| !skipped.contains(symbol))
            .map(|(symbol, value)| (*symbol, value.clone()))
            .collect();
        if let Some(fields) = runtime.objects.get_mut(&id) {
            fields.extend(inherited);
        }
        runtime.set_object_field(id, "origin", Value::Vector(origin));
        runtime.set_object_field(id, "angles", Value::Vector(angles));
        runtime.set_object_field(id, "code_classname", Value::string("actor"));
        if let Value::Int(n) = count {
            runtime.set_object_field(spawner_id, "count", Value::Int(n - 1));
        }
        let entity = runtime.entities.get_mut(&id).ok_or("actor is gone")?;
        entity.presence = Some(presence);
        if let Some(number) = number {
            entity.number = number;
        }
        id
    };

    let now = super::players::now_ms(world);
    let main = format!("aitype/{aitype}::main");
    run_now(world, &main, Value::Object(id), Vec::new(), now)
        .map_err(|fault| format!("{main}: {fault:?}"))?;
    let tree = match world.resource_mut::<Runtime>().object_field(id, "type") {
        Value::String(kind) if kind.as_bytes() == b"zombie_dog" => "zombie_dog",
        _ => "generic_human",
    };
    let _ = super::actor_anims::attach(world, id, tree);
    super::actor_brain::begin(world, id)?;
    raise(world, spawner.clone(), "spawned", vec![Value::Object(id)]);
    Ok(Value::Object(id))
}
