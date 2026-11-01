//! Black Ops' napalm builtins: `spawnNapalmGroundFlame` makes an entity a
//! ground fire effect for a while, and `startTanning` / `stopBurning` set
//! and clear an entity's burnt flag.

use super::super::args::{int, string, vector};
use crate::script::Namespace::Method;
use crate::script::{NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};

/// How long a ground flame burns without a time given, in seconds.
const FLAME_SECONDS: i32 = 10;

/// The ground flames and the level time each is freed at.
#[derive(Resource, Debug, Default)]
pub(crate) struct GroundFlames(std::collections::BTreeMap<u64, i32>);

fn now_ms(world: &World) -> i32 {
    crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick)
}

/// `ent spawnNapalmGroundFlame(origin, fx, angles, [seconds])`: the entity
/// moves to the origin (whole units) and turns to the angles, plays the
/// effect, and is freed after the time (10 seconds).
fn spawn_ground_flame(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let id = super::engine::entity_id(world, receiver)?;
    let origin = vector(args, 0)?.map(|v| v as i32 as f32);
    let name = string(args, 1)?.to_string();
    let angles = vector(args, 2)?;
    let seconds = if args.len() > 3 {
        int(args, 3)?
    } else {
        FLAME_SECONDS
    };
    let (forward, _, up) = math_iw4::angle_vectors(angles);
    let now = now_ms(world);
    let mut runtime = world.resource_mut::<RoundScript>();
    runtime.set_object_field(id, "origin", Value::Vector(origin));
    runtime.set_object_field(id, "angles", Value::Vector(angles));
    runtime.engine.effects.insert(
        id,
        super::super::entities::PersistentFx {
            name,
            origin,
            forward,
            up,
            start_ms: Some(now),
            repeat_ms: 0,
            cull_distance: 0.0,
        },
    );
    world
        .get_resource_or_insert_with(GroundFlames::default)
        .0
        .insert(id, now + seconds * 1000);
    Ok(Value::Undefined)
}

/// Frees the ground flames whose time is up.
pub(crate) fn advance(world: &mut World) {
    let now = now_ms(world);
    let Some(mut flames) = world.get_resource_mut::<GroundFlames>() else {
        return;
    };
    let expired: Vec<u64> = flames
        .0
        .iter()
        .filter(|(_, at)| **at <= now)
        .map(|(id, _)| *id)
        .collect();
    for id in &expired {
        flames.0.remove(id);
    }
    for id in expired {
        let mut runtime = world.resource_mut::<RoundScript>();
        if !runtime.live(&id) {
            continue;
        }
        runtime.engine.effects.remove(&id);
        runtime.request_delete(id);
        crate::script::runtime::raise(world, Value::Object(id), "death", Vec::new());
    }
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Method, "spawnnapalmgroundflame", spawn_ground_flame);
    // Any entity: a player's flag is in its player state.
    registry.register(Method, "starttanning", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        super::t5::t5(world).tanned.insert(id);
        Ok(Value::Undefined)
    });
    registry.register(Method, "stopburning", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        let client = world.resource::<RoundScript>().player_client(id);
        let state = super::t5::t5(world);
        state.tanned.remove(&id);
        if let Some(client) = client {
            state.statuses.remove(&(client, "burning"));
        }
        Ok(Value::Undefined)
    });
}
