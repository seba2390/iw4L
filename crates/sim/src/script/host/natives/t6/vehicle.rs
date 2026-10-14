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
/// `sighttracepassed`'s masks, without and with bodies.
const SIGHT_MASK: u32 = 0x0080_1803;
const SIGHT_MASK_CHARACTERS: u32 = 0x0280_9803;
/// `bullettracepassed`'s masks, without and with bodies.
const BULLET_MASK: u32 = 0x0080_6033;
const BULLET_MASK_CHARACTERS: u32 = 0x0280_e033;
/// `depthinwater` looks for the surface this far above and below, by the
/// water contents.
const WATER_REACH: f32 = 200.0;
const CONTENTS_WATER: u32 = 0x20;
/// How far below a platform's box a rider's feet may be (his origin rests
/// on its floor brushes).
const RIDER_SLACK: f32 = 2.0;
/// Entity numbers below this are entities (the world's and none above).
const ENTITYNUM_NONE: i32 = 1023;
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
    /// `setvehmaxspeed`.
    max: BTreeMap<u64, f32>,
    /// Moving platforms (`setmovingplatformenabled`).
    platforms: std::collections::BTreeSet<u64>,
    /// Actors that do not push players (`pushplayer(0)`).
    no_push: std::collections::BTreeSet<u64>,
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

/// Whether a player stands on a platform: on something (not in the air)
/// inside the platform model's bounds, turned by its yaw
/// (`t6.vehicles.platform`).
fn riding(world: &mut World, platform: u64, client: u32, at: [f32; 3]) -> bool {
    let Some(model) = text(world, platform, "model") else {
        return false;
    };
    let center = origin(world, platform);
    let yaw = match field(world, platform, "angles") {
        Value::Vector(v) => v[1],
        _ => 0.0,
    };
    let frame = crate::frame::FrameWorld::from_world(world);
    if crate::script_player::ground_entity(&frame, crate::ClientId(client))
        .is_none_or(|n| n == ENTITYNUM_NONE)
    {
        return false;
    }
    let Some((mid, half)) = frame
        .model_capability(&model)
        .flatten()
        .and_then(|c| c.bounds)
    else {
        return false;
    };
    let (sin, cos) = (-yaw).to_radians().sin_cos();
    let rel = [at[0] - center[0], at[1] - center[1], at[2] - center[2]];
    let local = [
        rel[0] * cos - rel[1] * sin,
        rel[0] * sin + rel[1] * cos,
        rel[2],
    ];
    (0..2).all(|i| (local[i] - mid[i]).abs() <= half[i])
        && local[2] >= mid[2] - half[2] - RIDER_SLACK
        && local[2] <= mid[2] + half[2]
}

/// The platform a player rides, if any.
fn platform_of(world: &mut World, client: u32) -> Option<u64> {
    let at = crate::frame::FrameWorld::from_world(world)
        .player(crate::ClientId(client))?
        .origin;
    let platforms: Vec<u64> = world
        .get_resource::<PathVehicles>()?
        .platforms
        .iter()
        .copied()
        .collect();
    platforms
        .into_iter()
        .find(|&platform| riding(world, platform, client, at))
}

/// The players standing on a moving platform ride it: moved with it and
/// turned about its origin as it turns.
fn carry_riders(world: &mut World, id: u64, from: [f32; 3], to: [f32; 3], turn: f32) {
    let clients: Vec<u32> = world
        .resource::<RoundScript>()
        .players
        .keys()
        .copied()
        .collect();
    // Who stood on it where it was, before it moved.
    let riders: Vec<u32> = clients
        .into_iter()
        .filter(|&client| {
            let Some(at) = crate::frame::FrameWorld::from_world(world)
                .player(crate::ClientId(client))
                .map(|ps| ps.origin)
            else {
                return false;
            };
            let moved_origin = origin(world, id);
            world
                .resource_mut::<RoundScript>()
                .set_object_field(id, "origin", Value::Vector(from));
            let on = riding(world, id, client, at);
            world.resource_mut::<RoundScript>().set_object_field(
                id,
                "origin",
                Value::Vector(moved_origin),
            );
            on
        })
        .collect();
    let clients = riders;
    let turn = (turn + 540.0).rem_euclid(360.0) - 180.0;
    let (sin, cos) = turn.to_radians().sin_cos();
    let mut frame = crate::frame::FrameWorld::from_world(world);
    for client in clients {
        let player = crate::ClientId(client);
        let Some(at) = frame.player(player).map(|ps| ps.origin) else {
            continue;
        };
        // Its place about the platform's origin, turned, then moved along.
        let rel = [at[0] - from[0], at[1] - from[1]];
        let turned = [rel[0] * cos - rel[1] * sin, rel[0] * sin + rel[1] * cos];
        let delta = [
            to[0] + turned[0] - at[0],
            to[1] + turned[1] - at[1],
            to[2] - from[2],
        ];
        crate::script_player::carry(&mut frame, player, delta, turn);
    }
}

/// The script object of an entity number in use.
fn object_of_number(world: &World, number: i32) -> Option<u64> {
    world
        .resource::<RoundScript>()
        .entities
        .iter()
        .find(|(id, entity)| entity.number == number && world.resource::<RoundScript>().live(id))
        .map(|(id, _)| *id)
}

/// A `moveto` along one world axis by `distance`.
fn move_axis(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    axis: usize,
) -> Result<Value, String> {
    let id = super::super::engine::entity_id(world, receiver)?;
    let distance = match args.first() {
        Some(Value::Int(n)) => *n as f32,
        Some(Value::Float(f)) => *f,
        _ => return Err("expected a distance".into()),
    };
    let mut to = origin(world, id);
    to[axis] += distance;
    let moveto = NativeRegistry::engine_services()
        .get(Namespace::Method, "moveto")
        .ok_or("moveto: no engine move")?;
    let mut forwarded = vec![Value::Vector(to)];
    forwarded.extend(args.iter().skip(1).cloned());
    moveto(world, receiver, &forwarded)
}

/// Hides or shows a part (tag) of the entity's model.
fn part(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    hidden: bool,
) -> Result<Value, String> {
    let id = super::super::engine::entity_id(world, receiver)?;
    let Some(Value::String(tag)) = args.first().cloned() else {
        return Err("hidepart expects a part name".into());
    };
    if text(world, id, "model").is_none() {
        return Err("entity has no model".into());
    }
    if let Some(None) = super::super::super::presence::tag_pose(world, id, &tag) {
        return Err(match args.get(1) {
            Some(Value::String(model)) => {
                format!("cannot find part '{tag}' in entity model '{model}'")
            }
            _ => format!("cannot find part '{tag}' in entity model"),
        });
    }
    if let Some(entity) = world.resource_mut::<RoundScript>().entities.get_mut(&id) {
        entity.part_ops.push((tag.to_string().into(), hidden));
    }
    Ok(Value::Undefined)
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
            let speed = vehicles
                .held
                .get(&id)
                .or_else(|| vehicles.speed.get(&id))
                .copied()
                .unwrap_or(DEFAULT_MPH * MPH);
            vehicles.max.get(&id).map_or(speed, |max| speed.min(*max))
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
            let was_yaw = match field(world, id, "angles") {
                Value::Vector(v) => v[1],
                _ => 0.0,
            };
            // Faces the way it drives; a drop straight down keeps its facing.
            let yaw = if flat[0].abs() + flat[1].abs() > MIN_STEP {
                flat[1].atan2(flat[0]).to_degrees()
            } else {
                was_yaw
            };
            let mut runtime = world.resource_mut::<RoundScript>();
            runtime.set_object_field(id, "origin", Value::Vector(at));
            runtime.set_object_field(id, "angles", Value::Vector([0.0, yaw, 0.0]));
            if world.resource::<PathVehicles>().platforms.contains(&id) {
                carry_riders(world, id, was, at, yaw - was_yaw);
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

/// The map's vehicles have been given a posed model on the server.
#[derive(Resource)]
struct VehiclesPresented;

/// When the level starts: each map vehicle with a model gets a posed model
/// on the server, as script models have, so its tags can be read and things
/// linked to them.
fn present_vehicles(world: &mut World) {
    if world.get_resource::<VehiclesPresented>().is_some() {
        return;
    }
    world.insert_resource(VehiclesPresented);
    let ids: Vec<u64> = world
        .resource::<RoundScript>()
        .entities
        .iter()
        .filter(|(_, entity)| {
            entity.kind == EntityKind::Map
                && entity.presence.is_none()
                && entity.classname.starts_with("script_vehicle")
        })
        .map(|(id, _)| *id)
        .collect();
    for id in ids {
        if text(world, id, "model").is_none() {
            continue;
        }
        let at = origin(world, id);
        let Ok(presence) = super::super::super::presence::spawn_presence(world, at) else {
            continue;
        };
        let number = crate::frame::FrameWorld::from_world(world).gentity_number(presence);
        if let Some(entity) = world.resource_mut::<RoundScript>().entities.get_mut(&id) {
            entity.presence = Some(presence);
            if let Some(number) = number {
                entity.number = number;
            }
        }
    }
}

/// The clip a script model or vehicle plays, by entity.
#[derive(Resource, Default)]
struct EntityClips(BTreeMap<u64, std::sync::Arc<str>>);

fn animation(value: &Value) -> Result<std::sync::Arc<str>, String> {
    match value {
        Value::Animation { name, .. } => Ok(name.clone()),
        other => Err(format!("{} is not an animation", super::type_name(other))),
    }
}

fn not_actor(world: &mut World, receiver: &Value) -> Result<u64, String> {
    let id = super::super::engine::entity_id(world, receiver)?;
    if world.resource::<RoundScript>().entities[&id].kind == EntityKind::Actor {
        return Err("t6.anim.entity_tree: an actor's animation tree is its ASD's".into());
    }
    Ok(id)
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    registry.on_tick(present_vehicles);
    // gscm_movex / movey / movez(distance, time, [accel], [decel]): a move
    // along one world axis, as `moveto`.
    registry.register(Method, "movex", |world, receiver, args| {
        move_axis(world, receiver, args, 0)
    });
    registry.register(Method, "movey", |world, receiver, args| {
        move_axis(world, receiver, args, 1)
    });
    registry.register(Method, "movez", |world, receiver, args| {
        move_axis(world, receiver, args, 2)
    });
    // gsc_sighttracepassed(start, end, hit_characters, [ignore]): a sight
    // trace (solid, glass and clip, and bodies when asked) that hits
    // nothing.
    registry.register(Function, "sighttracepassed", |world, _, args| {
        let (Some(Value::Vector(start)), Some(Value::Vector(end))) = (args.first(), args.get(1))
        else {
            return Err("sighttracepassed expects two points".into());
        };
        let characters = !matches!(args.get(2), Some(Value::Int(0)));
        let mask = if characters {
            SIGHT_MASK_CHARACTERS
        } else {
            SIGHT_MASK
        };
        let ignore = super::super::engine::trace_ignore(world, args.get(3));
        let passed = matches!(
            super::super::engine::entity_trace(world, *start, *end, mask, ignore),
            crate::bullet_collision::TraceOutcome::Miss { .. }
        );
        Ok(Value::Int(passed.into()))
    });
    // gsc_bullettracepassed(start, end, hit_characters, [ignore],
    // [ignore2], [check]): a bullet trace that hits nothing; the extra
    // check the sixth argument asks for is not made.
    registry.register(Function, "bullettracepassed", |world, _, args| {
        let (Some(Value::Vector(start)), Some(Value::Vector(end))) = (args.first(), args.get(1))
        else {
            return Err("bullettracepassed expects two points".into());
        };
        let characters = !matches!(args.get(2), Some(Value::Int(0)));
        let mask = if characters {
            BULLET_MASK_CHARACTERS
        } else {
            BULLET_MASK
        };
        let mut ignore = super::super::engine::trace_ignore(world, args.get(3));
        let second = super::super::engine::trace_ignore(world, args.get(4));
        ignore.other_client = ignore.other_client.or(second.client);
        ignore.model = ignore.model.or(second.model);
        let passed = matches!(
            super::super::engine::entity_trace(world, *start, *end, mask, ignore),
            crate::bullet_collision::TraceOutcome::Miss { .. }
        );
        Ok(Value::Int(passed.into()))
    });
    // gscm_depthinwater: how deep the entity's origin is under water (the
    // surface found from 200 above to 200 below), 0 out of it.
    registry.register(Method, "depthinwater", |world, receiver, _| {
        let id = super::super::engine::entity_id(world, receiver)?;
        let at = match super::super::super::players::entity_field(world, id, "origin") {
            Value::Vector(v) => v,
            _ => [0.0; 3],
        };
        let start = [at[0], at[1], at[2] + WATER_REACH];
        let end = [at[0], at[1], at[2] - WATER_REACH];
        let hit =
            super::super::engine::trace(world, start, end, [0.0; 3], [0.0; 3], CONTENTS_WATER);
        let depth = if hit.fraction < 1.0 {
            start[2] + (end[2] - start[2]) * hit.fraction - at[2]
        } else {
            0.0
        };
        Ok(Value::Float(depth.max(0.0)))
    });
    // gscm_ghostindemo: hidden in demo recordings; IW4L records none.
    registry.register(Method, "ghostindemo", |world, receiver, _| {
        super::super::engine::entity_id(world, receiver)?;
        Ok(Value::Undefined)
    });
    // gscm_pushplayer(on): whether the actor pushes players aside
    // (`t6.actors.push_player`: kept, not applied).
    registry.register(Method, "pushplayer", |world, receiver, args| {
        let id = super::super::engine::entity_id(world, receiver)?;
        if world.resource::<RoundScript>().entities[&id].kind != EntityKind::Actor {
            return Err("not an actor".into());
        }
        let on = !matches!(args.first(), Some(Value::Int(0)));
        let mut vehicles = world.get_resource_or_insert_with(PathVehicles::default);
        if on {
            vehicles.no_push.remove(&id);
        } else {
            vehicles.no_push.insert(id);
        }
        Ok(Value::Undefined)
    });
    // gscm_getgroundent: what a player or resting missile stands on.
    registry.register(Method, "getgroundent", |world, receiver, _| {
        if *receiver == Value::Object(0) {
            return Ok(Value::Undefined);
        }
        let id = super::super::engine::entity_id(world, receiver)?;
        let Some(client) = world.resource::<RoundScript>().player_client(id) else {
            let (is_missile, number) = {
                let entity = &world.resource::<RoundScript>().entities[&id];
                (matches!(entity.kind, EntityKind::Missile(_)), entity.number)
            };
            if is_missile
                && let Some(ground) = crate::game_fire::missile_ground_entity(
                    &crate::frame::FrameWorld::from_world(world),
                    number,
                )
            {
                if ground == game_api::movement::ENTITY_WORLD {
                    return Ok(Value::Object(0));
                }
                if ground != game_api::movement::ENTITY_NONE {
                    return Ok(
                        object_of_number(world, ground).map_or(Value::Undefined, Value::Object)
                    );
                }
            }
            return Ok(Value::Undefined);
        };
        if let Some(platform) = platform_of(world, client) {
            return Ok(Value::Object(platform));
        }
        let ground = crate::script_player::ground_entity(
            &crate::frame::FrameWorld::from_world(world),
            crate::ClientId(client),
        );
        if ground == Some(ENTITYNUM_NONE - 1) {
            return Ok(Value::Object(0));
        }
        Ok(ground
            .filter(|&n| (0..ENTITYNUM_NONE).contains(&n))
            .and_then(|n| object_of_number(world, n))
            .map_or(Value::Undefined, Value::Object))
    });
    // gscm_getmoverent: the moving platform a player rides: the one he
    // stands on (`t6.vehicles.platform`).
    registry.register(Method, "getmoverent", |world, receiver, _| {
        let id = super::super::engine::entity_id(world, receiver)?;
        let Some(client) = world.resource::<RoundScript>().player_client(id) else {
            return Err("GetMoverEnt can only be called on a player.".into());
        };
        Ok(platform_of(world, client).map_or(Value::Undefined, Value::Object))
    });
    // gscm_hidepart / showpart(tag, [model]): a part of the entity's model,
    // with the engine's errors.
    registry.register(Method, "hidepart", |world, receiver, args| {
        part(world, receiver, args, true)
    });
    registry.register(Method, "showpart", |world, receiver, args| {
        part(world, receiver, args, false)
    });
    // gscm_useanimtree: the entity animates from the tree; one clip at a
    // time plays here (`t6.anim.entity_tree`).
    registry.register(Method, "useanimtree", |world, receiver, _| {
        not_actor(world, receiver)?;
        Ok(Value::Undefined)
    });
    // gscm_setanim(anim, [weight], [blend], [rate]): a clip with weight
    // plays; at weight 0 it stops.
    registry.register(Method, "setanim", |world, receiver, args| {
        let id = not_actor(world, receiver)?;
        let clip = animation(args.first().ok_or("setanim expects an animation")?)?;
        let weight = match args.get(1) {
            Some(Value::Int(n)) => *n as f32,
            Some(Value::Float(f)) => *f,
            _ => 1.0,
        };
        let mut clips = world.get_resource_or_insert_with(EntityClips::default);
        let op = if weight > 0.0 {
            clips.0.insert(id, clip.clone());
            Some(Some(clip))
        } else if clips.0.get(&id) == Some(&clip) {
            clips.0.remove(&id);
            Some(None)
        } else {
            None
        };
        if let Some(op) = op
            && let Some(entity) = world.resource_mut::<RoundScript>().entities.get_mut(&id)
        {
            entity.anim_op = Some(op);
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "clearanim", |world, receiver, args| {
        let id = not_actor(world, receiver)?;
        let clip = animation(args.first().ok_or("clearanim expects an animation")?)?;
        let mut clips = world.get_resource_or_insert_with(EntityClips::default);
        if clips.0.get(&id) == Some(&clip) {
            clips.0.remove(&id);
            if let Some(entity) = world.resource_mut::<RoundScript>().entities.get_mut(&id) {
                entity.anim_op = Some(None);
            }
        }
        Ok(Value::Undefined)
    });
    // A vehicle's top speed (miles per hour) caps its path speed.
    registry.register(Method, "setvehmaxspeed", |world, receiver, args| {
        let id = vehicle(world, receiver)?;
        let mph = match args.first() {
            Some(Value::Int(n)) => *n as f32,
            Some(Value::Float(f)) => *f,
            _ => return Err("setvehmaxspeed expects a speed".into()),
        };
        world
            .get_resource_or_insert_with(PathVehicles::default)
            .max
            .insert(id, mph * MPH);
        Ok(Value::Undefined)
    });
    // Players standing on it ride it (`t6.vehicles.platform`).
    registry.register(
        Method,
        "setmovingplatformenabled",
        |world, receiver, args| {
            let id = super::super::engine::entity_id(world, receiver)?;
            let on = !matches!(args.first(), Some(Value::Int(0)));
            let mut vehicles = world.get_resource_or_insert_with(PathVehicles::default);
            if on {
                vehicles.platforms.insert(id);
            } else {
                vehicles.platforms.remove(&id);
            }
            Ok(Value::Undefined)
        },
    );

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
    registry.register(Method, "cancelaimove", |world, receiver, _| {
        vehicle(world, receiver)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "pathvariableoffset", |world, receiver, args| {
        vehicle(world, receiver)?;
        super::vector(args, 0)?;
        super::float(args, 1)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setneargoalnotifydist", |world, receiver, args| {
        vehicle(world, receiver)?;
        super::float(args, 0)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setvehgoalpos", |world, receiver, args| {
        let id = vehicle(world, receiver)?;
        let goal = super::vector(args, 0)?;
        if args.len() >= 2 {
            super::int(args, 1)?;
        }
        if args.len() >= 3 {
            super::int(args, 2)?;
        }
        if args.len() >= 4 {
            super::int(args, 3)?;
        }
        world
            .resource_mut::<RoundScript>()
            .set_object_field(id, "origin", Value::Vector(goal));
        raise(world, Value::Object(id), "goal", Vec::new());
        Ok(Value::Undefined)
    });
    registry.register(Method, "clearvehgoalpos", |world, receiver, _| {
        vehicle(world, receiver)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setvehvelocity", |world, receiver, args| {
        vehicle(world, receiver)?;
        super::vector(args, 0)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "makevehicleunusable", |world, receiver, _| {
        vehicle(world, receiver)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setphysacceleration", |world, receiver, args| {
        vehicle(world, receiver)?;
        super::vector(args, 0)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "getangularvelocity", |world, receiver, _| {
        vehicle(world, receiver)?;
        Ok(Value::Vector([0.0; 3]))
    });
    registry.register(Method, "setangularvelocity", |world, receiver, args| {
        vehicle(world, receiver)?;
        super::vector(args, 0)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "getoccupantseat", |world, receiver, args| {
        vehicle(world, receiver)?;
        super::super::engine::entity_id(world, super::arg(args, 0)?)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "isvehicle", |world, receiver, _| {
        let id = super::super::engine::entity_id(world, receiver)?;
        let runtime = world.resource::<RoundScript>();
        let is = runtime.entities[&id].kind == EntityKind::Vehicle
            || runtime.entities[&id]
                .classname
                .starts_with("script_vehicle");
        Ok(Value::Int(is.into()))
    });
    registry.register(Function, "isvehicle", |world, _, args| {
        let Some(Value::Object(id)) = args.first() else {
            return Ok(Value::Int(0));
        };
        let runtime = world.resource::<RoundScript>();
        let is = runtime.entities.get(id).is_some_and(|e| {
            e.kind == EntityKind::Vehicle || e.classname.starts_with("script_vehicle")
        });
        Ok(Value::Int(is.into()))
    });
    registry.register(Method, "getvehicleoccupied", |world, receiver, _| {
        super::super::engine::entity_id(world, receiver)?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "spawnvehicle", |world, _, args| {
        let model = super::string(args, 0)?;
        let targetname = super::string(args, 1)?;
        let _veh_type = super::string(args, 2)?;
        let at = super::vector(args, 3)?;
        let angles = super::vector(args, 4)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        let id = runtime.create_entity(EntityKind::Vehicle, "script_vehicle")?;
        runtime.set_object_field(id, "model", Value::String(model.into()));
        runtime.set_object_field(id, "targetname", Value::String(targetname.into()));
        runtime.set_object_field(id, "origin", Value::Vector(at));
        runtime.set_object_field(id, "angles", Value::Vector(angles));
        Ok(Value::Object(id))
    });
    registry.on_tick(drive);
}
