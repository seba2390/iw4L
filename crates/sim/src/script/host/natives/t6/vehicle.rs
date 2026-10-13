//! Black Ops 2's path vehicles: `attachpath(node)` puts a vehicle on its
//! chain of `info_vehicle_node`s (each node's `target` names the next) at
//! the first node, and `startpath()` drives it along them, node to node at
//! each node's `speed` (miles per hour), notifying `reached_node` (with the
//! node) as it passes each and `reached_end_node` at the last. Nuketown's
//! perk machines ride `perk_arrival_vehicle` down from the sky this way;
//! TranZit's bus drives its road. What IW4L does not do of the engine's
//! path following is listed under `t6.vehicles.path` in
//! `docs/fidelity/t6.md`.

use super::super::super::entities::EntityKind;
use crate::script::runtime::raise;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

/// Units a second in a mile an hour.
const MPH: f32 = 17.6;
/// The speed a vehicle drives at when no node it has passed sets one
/// (`t6.vehicles.path`).
const DEFAULT_MPH: f32 = 30.0;
/// The least travel still driven in a tick.
const MIN_STEP: f32 = 1e-3;
/// The most nodes a vehicle passes in one tick.
const MAX_HOPS: usize = 64;

#[derive(Clone, Copy, Debug)]
struct Drive {
    from: u64,
    to: u64,
    /// How far from `from` to `to` (0..1).
    t: f32,
}

#[derive(Resource, Default)]
struct PathVehicles {
    attached: BTreeMap<u64, u64>,
    driving: BTreeMap<u64, Drive>,
    /// Units a second each vehicle drives at.
    speed: BTreeMap<u64, f32>,
    /// A speed the script set (`setspeed`), over the nodes'.
    held: BTreeMap<u64, f32>,
}

fn field(world: &mut World, id: u64, name: &str) -> Value {
    world.resource_mut::<RoundScript>().object_field(id, name)
}

fn origin(world: &mut World, id: u64) -> [f32; 3] {
    match field(world, id, "origin") {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    }
}

fn text(world: &mut World, id: u64, name: &str) -> Option<String> {
    match field(world, id, name) {
        Value::String(text) if !text.is_empty() => Some(text.to_string()),
        _ => None,
    }
}

fn is_node(world: &World, id: u64) -> bool {
    world
        .resource::<RoundScript>()
        .entities
        .get(&id)
        .is_some_and(|entity| entity.classname.starts_with("info_vehicle_node"))
}

/// The node a node's `target` names.
fn next_node(world: &mut World, node: u64) -> Option<u64> {
    let target = text(world, node, "target")?;
    let ids: Vec<u64> = world
        .resource::<RoundScript>()
        .entities
        .iter()
        .filter(|(_, entity)| entity.classname.starts_with("info_vehicle_node"))
        .map(|(id, _)| *id)
        .collect();
    ids.into_iter()
        .find(|&id| text(world, id, "targetname").as_deref() == Some(target.as_str()))
}

/// A node's `speed` in units a second, when it sets one.
fn node_speed(world: &mut World, node: u64) -> Option<f32> {
    match field(world, node, "speed") {
        Value::Int(n) => Some(n as f32 * MPH),
        Value::Float(f) => Some(f * MPH),
        Value::String(text) => text.trim().parse::<f32>().ok().map(|mph| mph * MPH),
        _ => None,
    }
}

fn vehicle(world: &mut World, receiver: &Value) -> Result<u64, String> {
    let id = super::super::engine::entity_id(world, receiver).map_err(|_| "not an entity")?;
    if world.resource::<RoundScript>().entities[&id].kind != EntityKind::Vehicle
        && !world.resource::<RoundScript>().entities[&id]
            .classname
            .starts_with("script_vehicle")
    {
        return Err("not a vehicle".into());
    }
    Ok(id)
}

fn node(world: &mut World, value: &Value) -> Result<u64, String> {
    let id = super::super::engine::entity_id(world, value)?;
    if !is_node(world, id) {
        return Err("not a vehicle node".into());
    }
    Ok(id)
}

/// Units a second a vehicle drives at now; 0 off its path.
fn current_speed(world: &mut World, id: u64) -> f32 {
    let vehicles = world.get_resource_or_insert_with(PathVehicles::default);
    if !vehicles.driving.contains_key(&id) {
        return 0.0;
    }
    vehicles
        .held
        .get(&id)
        .or_else(|| vehicles.speed.get(&id))
        .copied()
        .unwrap_or(DEFAULT_MPH * MPH)
}

/// Each tick: the vehicles on their paths drive on.
fn drive(world: &mut World) {
    let seconds = crate::MATCH_TICK_MS as f32 / 1000.0;
    let Some(vehicles) = world.get_resource::<PathVehicles>() else {
        return;
    };
    let ids: Vec<u64> = vehicles.driving.keys().copied().collect();
    for id in ids {
        if !world.resource::<RoundScript>().live(&id) {
            let mut vehicles = world.resource_mut::<PathVehicles>();
            vehicles.driving.remove(&id);
            continue;
        }
        let Some(mut path) = world.resource::<PathVehicles>().driving.get(&id).copied() else {
            continue;
        };
        let speed = {
            let vehicles = world.resource::<PathVehicles>();
            vehicles
                .held
                .get(&id)
                .or_else(|| vehicles.speed.get(&id))
                .copied()
                .unwrap_or(DEFAULT_MPH * MPH)
        };
        let mut step = speed * seconds;
        let mut at = origin(world, id);
        let mut reached = Vec::new();
        let mut ended = false;
        for _ in 0..MAX_HOPS {
            let (a, b) = (origin(world, path.from), origin(world, path.to));
            let length = (0..3).map(|i| (b[i] - a[i]).powi(2)).sum::<f32>().sqrt();
            let left = (1.0 - path.t) * length;
            if length > MIN_STEP && step < left {
                path.t += step / length;
                at = std::array::from_fn(|i| a[i] + (b[i] - a[i]) * path.t);
                break;
            }
            step -= left.max(0.0);
            at = b;
            reached.push(path.to);
            if let Some(mph) = node_speed(world, path.to) {
                world.resource_mut::<PathVehicles>().speed.insert(id, mph);
            }
            match next_node(world, path.to) {
                Some(next) => {
                    path = Drive {
                        from: path.to,
                        to: next,
                        t: 0.0,
                    };
                }
                None => {
                    ended = true;
                    break;
                }
            }
            if step <= MIN_STEP {
                break;
            }
        }
        let was = origin(world, id);
        if (0..3).any(|i| (at[i] - was[i]).abs() > MIN_STEP) {
            let flat = [at[0] - was[0], at[1] - was[1]];
            let mut runtime = world.resource_mut::<RoundScript>();
            runtime.set_object_field(id, "origin", Value::Vector(at));
            // Faces the way it drives; a drop straight down keeps its facing.
            if flat[0].abs() + flat[1].abs() > MIN_STEP {
                let yaw = flat[1].atan2(flat[0]).to_degrees();
                runtime.set_object_field(id, "angles", Value::Vector([0.0, yaw, 0.0]));
            }
        }
        {
            let mut vehicles = world.resource_mut::<PathVehicles>();
            if ended {
                vehicles.driving.remove(&id);
            } else {
                vehicles.driving.insert(id, path);
            }
        }
        for node in reached {
            raise(
                world,
                Value::Object(id),
                "reached_node",
                vec![Value::Object(node)],
            );
        }
        if ended {
            raise(world, Value::Object(id), "reached_end_node", Vec::new());
        }
    }
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    // The map's vehicle nodes by key, as `getent` finds entities.
    let shared = NativeRegistry::engine_services();
    for name in ["getvehiclenode", "getvehiclenodearray"] {
        if let Some(native) = shared.get(Function, name) {
            registry.register(Function, name, native);
        }
    }
    registry.register(Method, "attachpath", |world, receiver, args| {
        let id = vehicle(world, receiver)?;
        let start = node(world, args.first().ok_or("attachpath expects a node")?)?;
        let at = origin(world, start);
        world
            .resource_mut::<RoundScript>()
            .set_object_field(id, "origin", Value::Vector(at));
        let mut vehicles = world.get_resource_or_insert_with(PathVehicles::default);
        vehicles.attached.insert(id, start);
        vehicles.driving.remove(&id);
        Ok(Value::Undefined)
    });
    registry.register(Method, "startpath", |world, receiver, args| {
        let id = vehicle(world, receiver)?;
        let start = match args.first() {
            Some(value) => node(world, value)?,
            None => *world
                .get_resource_or_insert_with(PathVehicles::default)
                .attached
                .get(&id)
                .ok_or("Can't start path on a vehicle that hasn't been attached")?,
        };
        if let Some(speed) = node_speed(world, start) {
            world
                .get_resource_or_insert_with(PathVehicles::default)
                .speed
                .insert(id, speed);
        }
        match next_node(world, start) {
            Some(next) => {
                world
                    .get_resource_or_insert_with(PathVehicles::default)
                    .driving
                    .insert(
                        id,
                        Drive {
                            from: start,
                            to: next,
                            t: 0.0,
                        },
                    );
            }
            None => raise(world, Value::Object(id), "reached_end_node", Vec::new()),
        }
        Ok(Value::Undefined)
    });
    // `setspeed(mph, ...)` and `setspeedimmediate(mph, ...)`: the script's
    // speed over the nodes' (`resumespeed` hands it back).
    for name in ["setspeed", "setspeedimmediate"] {
        registry.register(Method, name, |world, receiver, args| {
            let id = vehicle(world, receiver)?;
            let mph = match args.first() {
                Some(Value::Int(n)) => *n as f32,
                Some(Value::Float(f)) => *f,
                _ => return Err("setspeed expects a speed".into()),
            };
            world
                .get_resource_or_insert_with(PathVehicles::default)
                .held
                .insert(id, mph * MPH);
            Ok(Value::Undefined)
        });
    }
    registry.register(Method, "resumespeed", |world, receiver, _| {
        let id = vehicle(world, receiver)?;
        world
            .get_resource_or_insert_with(PathVehicles::default)
            .held
            .remove(&id);
        Ok(Value::Undefined)
    });
    registry.register(Method, "getspeed", |world, receiver, _| {
        let id = vehicle(world, receiver)?;
        Ok(Value::Float(current_speed(world, id)))
    });
    registry.register(Method, "getspeedmph", |world, receiver, _| {
        let id = vehicle(world, receiver)?;
        Ok(Value::Float(current_speed(world, id) / MPH))
    });
    registry.on_tick(drive);
}
