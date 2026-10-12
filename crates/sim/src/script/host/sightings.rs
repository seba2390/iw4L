//! What a test player may read of the script world: the living actors and a
//! few integer fields, without touching the running scripts.

use crate::ClientId;
use crate::script::RoundScript;
use crate::script::Value;
use bevy_ecs::prelude::World;

/// A living actor: where it stands, its health, and the script model that
/// carries its body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActorSighting {
    pub origin: [f32; 3],
    pub health: i32,
    pub body: Option<crate::ScriptModelId>,
}

/// Whose field a test player reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptOwner {
    Level,
    Player(ClientId),
}

fn field(runtime: &RoundScript, id: u64, name: &str) -> Option<Value> {
    let symbol = runtime
        .program
        .as_ref()?
        .symbol_ids
        .get(name)
        .copied()
        .or_else(|| runtime.dynamic_symbols.get(name).copied())?;
    runtime.objects.get(&id)?.get(&symbol).cloned()
}

fn int(value: Option<Value>) -> Option<i32> {
    match value? {
        Value::Int(n) => Some(n),
        Value::Float(f) => Some(f as i32),
        _ => None,
    }
}

pub(crate) fn living_actors(world: &World) -> Vec<ActorSighting> {
    let Some(runtime) = world.get_resource::<RoundScript>() else {
        return Vec::new();
    };
    runtime
        .entities
        .iter()
        .filter(|(id, entity)| {
            entity.kind == super::entities::EntityKind::Actor && runtime.live(id)
        })
        .filter_map(|(&id, entity)| {
            let health = int(field(runtime, id, "health"))?;
            let Some(Value::Vector(origin)) = field(runtime, id, "origin") else {
                return None;
            };
            (health > 0).then_some(ActorSighting {
                origin,
                health,
                body: entity.presence,
            })
        })
        .collect()
}

pub(crate) fn script_int(world: &World, owner: ScriptOwner, name: &str) -> Option<i32> {
    let runtime = world.get_resource::<RoundScript>()?;
    let id = match owner {
        ScriptOwner::Level => 0,
        ScriptOwner::Player(client) => super::entity_damage::player_object_id(world, client.0)?,
    };
    int(field(runtime, id, name))
}
