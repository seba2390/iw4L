use super::super::arrays::new_array;
use super::super::entities::EntityKind;
use crate::script::{Namespace, NativeRegistry, Runtime, Value};
use bevy_ecs::prelude::World;

pub(crate) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};
    registry.register(Function, "getplayers", |world, _, args| {
        let team = optional_team(args)?;
        let players = players(world)
            .into_iter()
            .filter(|&id| team.as_deref().is_none_or(|team| on_team(world, id, team)))
            .map(Value::Object)
            .collect();
        new_array(world, players)
    });
    // No actors are simulated yet, so every query for living AI is empty.
    registry.register(Function, "getaiarray", |world, _, args| {
        optional_team(args)?;
        new_array(world, Vec::new())
    });
    registry.register(Function, "getaispeciesarray", |world, _, _| {
        new_array(world, Vec::new())
    });
    registry.register(Function, "getspawnerarray", |world, _, args| {
        no_args(args)?;
        let spawners = spawners(world).into_iter().map(Value::Object).collect();
        new_array(world, spawners)
    });
    registry.register(Function, "getdifficulty", |_, _, args| {
        no_args(args)?;
        Ok(Value::string("medium"))
    });
    registry.register(Function, "issaverecentlyloaded", |_, _, args| {
        no_args(args)?;
        Ok(Value::Int(0))
    });
    // The host keeps no AI budget, water simulation or exploder ids of its own.
    registry.register(Function, "setailimit", |_, _, _| Ok(Value::Undefined));
    registry.register(Function, "watersimenable", |_, _, _| Ok(Value::Undefined));
    registry.register(Method, "setexploderid", |_, _, _| Ok(Value::Undefined));
    registry.register(Function, "getnumconnectedplayers", |world, _, args| {
        no_args(args)?;
        Ok(Value::Int(players(world).len() as i32))
    });
    // Every client the session admits is connected by the time it is a player,
    // so the expected count is the connected count; scripts wait while it is zero.
    registry.register(Function, "getnumexpectedplayers", |world, _, args| {
        no_args(args)?;
        Ok(Value::Int(players(world).len() as i32))
    });
}

fn optional_team(args: &[Value]) -> Result<Option<String>, String> {
    match args {
        [] | [Value::Undefined] => Ok(None),
        [Value::String(team)] if team.as_bytes() == b"all" => Ok(None),
        [Value::String(team)] => Ok(Some(team.to_string())),
        _ => Err("takes an optional team name".into()),
    }
}

fn on_team(world: &mut World, id: u64, team: &str) -> bool {
    match world.resource_mut::<Runtime>().object_field(id, "team") {
        Value::String(own) => own.as_bytes() == team.as_bytes(),
        _ => true,
    }
}

/// Map actors flagged as spawners, in entity order.
fn spawners(world: &mut World) -> Vec<u64> {
    let candidates: Vec<u64> = world
        .resource::<Runtime>()
        .entities
        .iter()
        .filter(|(_, entity)| entity.classname.starts_with("actor_"))
        .map(|(id, _)| *id)
        .collect();
    let mut runtime = world.resource_mut::<Runtime>();
    candidates
        .into_iter()
        .filter(|&id| matches!(runtime.object_field(id, "spawnflags"), Value::Int(flags) if flags & 1 != 0))
        .collect()
}

fn no_args(args: &[Value]) -> Result<(), String> {
    if args.is_empty() {
        Ok(())
    } else {
        Err("takes no arguments".into())
    }
}

/// Player entities in client order.
fn players(world: &mut World) -> Vec<u64> {
    let mut players: Vec<(i32, u64)> = world
        .resource::<Runtime>()
        .entities
        .iter()
        .filter(|(_, entity)| entity.kind == EntityKind::Player)
        .map(|(id, entity)| (entity.number, *id))
        .collect();
    players.sort_unstable();
    players.into_iter().map(|(_, id)| id).collect()
}
