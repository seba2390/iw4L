//! Black Ops 2's dynamic paths: a `script_model` or `script_brushmodel`
//! spawned with spawnflag 1 (DYNAMICPATH) cuts the path links that cross it
//! when the level starts, and `disconnectpaths` / `connectpaths` cut and
//! rejoin them later (a door that opens, debris that is cleared). What IW4L
//! does not do of it is listed under `t6.paths.dynamic` in
//! `docs/fidelity/t6.md`.

use super::super::super::actor_nav::node_value;
use super::super::super::dynamic_paths::{self, LinkHull};
use crate::frame::FrameWorld;
use crate::script::{Namespace, NativeRegistry, Value};
use bevy_ecs::prelude::World;

/// `getnodesinradius`'s height window and most nodes.
const NODES_HEIGHT: f32 = 512.0;
const NODES_MAX: usize = 256;
/// The body a link is tested with: 15 each way, from 18 to 48 above the
/// node, traced against the entity's collision with the engine's mask.
const HULL: LinkHull = LinkHull {
    low: 18.0,
    high: 48.0,
    sweep_mask: Some(0x282_0011),
};

/// `getnodesinradius(origin, max, min, [height], [type], [count])`: the
/// path nodes within `max` of the origin (and not within `min`), at most
/// `height` above or below it (512), of the node type named ("Cover": any
/// cover node), at most `count` of them (1 to 256), nearest first when
/// `sorted`.
fn nodes_in_radius(world: &mut World, args: &[Value], sorted: bool) -> Result<Value, String> {
    let number = |value: Option<&Value>| match value {
        Some(Value::Int(n)) => Some(*n as f32),
        Some(Value::Float(f)) => Some(*f),
        _ => None,
    };
    let Some(Value::Vector(origin)) = args.first() else {
        return Err("getnodesinradius expects an origin".into());
    };
    let origin = *origin;
    let max = number(args.get(1)).ok_or("getnodesinradius expects a radius")?;
    let min = number(args.get(2)).ok_or("getnodesinradius expects a minimum radius")?;
    let height = number(args.get(3)).unwrap_or(NODES_HEIGHT);
    let kind = match args.get(4) {
        Some(Value::String(kind)) => Some(kind.to_string()),
        _ => None,
    };
    let count = match args.get(5) {
        Some(Value::Int(n)) => (*n).clamp(1, NODES_MAX as i32) as usize,
        _ => NODES_MAX,
    };
    let Some(paths) = FrameWorld::from_world(world).actor_paths() else {
        return super::super::super::arrays::new_array(world, Vec::new());
    };
    let mut found: Vec<(f32, u16)> = paths
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, node)| {
            kind.as_deref().is_none_or(|kind| {
                let name = node
                    .fields
                    .as_ref()
                    .map_or("", |fields| fields.type_name.as_str());
                if kind.eq_ignore_ascii_case("cover") {
                    name.to_ascii_lowercase().starts_with("cover")
                } else {
                    name.eq_ignore_ascii_case(kind)
                }
            })
        })
        .filter_map(|(index, node)| {
            let d: f32 = (0..3).map(|i| (node.origin[i] - origin[i]).powi(2)).sum();
            ((node.origin[2] - origin[2]).abs() <= height && d <= max * max && d >= min * min)
                .then_some((d, index as u16))
        })
        .collect();
    if sorted {
        found.sort_by(|a, b| a.0.total_cmp(&b.0));
    }
    found.truncate(count);
    let mut values = Vec::with_capacity(found.len());
    for (_, index) in found {
        values.push(node_value(world, index)?);
    }
    super::super::super::arrays::new_array(world, values)
}

/// The box a node is dropped to the floor with (15 each way, 30 high), from
/// 1 above it to 256 below, and the room it needs where it lands (48 high).
const DROP_MINS: [f32; 3] = [-15.0, -15.0, 0.0];
const DROP_MAXS: [f32; 3] = [15.0, 15.0, 30.0];
const ROOM_MAXS: [f32; 3] = [15.0, 15.0, 48.0];
const DROP_ABOVE: f32 = 1.0;
const DROP_BELOW: f32 = 256.0;
const DROP_MASK: u32 = 0x82_0011;
/// The spawnflag of a node that rides a mover, and the one the game sets
/// on such a node whose drop met the world rather than its mover.
const PARENTED: u32 = 0x100;
const SUSPENDED: u32 = 0x1_0000;

fn dvar_text(world: &World, name: &str) -> String {
    world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get(name)
        .cloned()
        .or_else(|| super::dvars::lookup(name).map(|spec| spec.default.to_owned()))
        .unwrap_or_else(String::new)
}

/// Why a node became a bad node as the level loaded.
#[derive(Clone, Copy)]
enum Unusable {
    InSolid,
    Floating,
    NoParent,
}

/// Where a parented node's drop landed: on the mover it names, on the
/// world (or nothing), or on some other entity.
enum Landing {
    Named,
    World,
    Other,
}

/// What the drop pass sees of the level's entities: which collision rows
/// are dynamic paths (the game makes them not solid while the free nodes
/// drop), and the entities a parented node may name, by targetname.
struct DropScene {
    dynamic: std::collections::BTreeSet<crate::ScriptModelId>,
    movers: Vec<(String, Option<crate::ScriptModelId>, bool)>,
}

impl DropScene {
    fn of(world: &mut World) -> Self {
        let ids: Vec<u64> = {
            let runtime = world.resource::<crate::script::RoundScript>();
            let mut ids: Vec<(i32, u64)> = runtime
                .entities
                .iter()
                .filter(|(id, entity)| {
                    runtime.live(id)
                        && matches!(
                            entity.kind,
                            super::super::super::entities::EntityKind::Map
                                | super::super::super::entities::EntityKind::Spawned
                                | super::super::super::entities::EntityKind::Vehicle
                        )
                })
                .map(|(id, entity)| (entity.number, *id))
                .collect();
            ids.sort_unstable();
            ids.into_iter().map(|(_, id)| id).collect()
        };
        let mut dynamic = std::collections::BTreeSet::new();
        let mut movers = Vec::new();
        for id in ids {
            let dynamic_path = dynamic_paths::dynamic_path(world, id);
            let targetname = match world
                .resource_mut::<crate::script::RoundScript>()
                .object_field(id, "targetname")
            {
                Value::String(name) => name.to_string(),
                _ => String::new(),
            };
            let runtime = world.resource::<crate::script::RoundScript>();
            let entity = &runtime.entities[&id];
            let vehicle = entity.kind == super::super::super::entities::EntityKind::Vehicle
                || entity.classname.starts_with("script_vehicle");
            // A vehicle is a dynamic path as it spawns.
            if (vehicle || dynamic_path)
                && let Some(presence) = entity.presence
            {
                dynamic.insert(presence);
            }
            // A script mover or a vehicle is what a node may ride.
            let rides = vehicle
                || matches!(
                    &*entity.classname,
                    "script_model" | "script_brushmodel" | "script_origin"
                );
            if !targetname.is_empty() {
                movers.push((targetname, entity.presence, rides));
            }
        }
        Self { dynamic, movers }
    }
}

/// What a drop sweep met: how far it went, and whether it started in
/// solid.
struct Sweep {
    fraction: f32,
    solid: bool,
}

fn sweep(
    world: &mut World,
    start: [f32; 3],
    end: [f32; 3],
    maxs: [f32; 3],
    skip: &dyn Fn(crate::ScriptModelId) -> bool,
) -> Sweep {
    let trace = super::super::super::presence::settled(world).trace_world_hull_except(
        start,
        end,
        DROP_MINS,
        maxs,
        DROP_MASK,
        &|owner| owner.script_model().is_some_and(skip),
    );
    Sweep {
        fraction: trace.fraction,
        solid: trace.startsolid != 0 || trace.allsolid != 0,
    }
}

/// Drops one node to the floor: the box swept down from 1 above it to 256
/// below must start clear and land, and the room where it lands must be
/// clear. The landing point, or why the node is unusable.
fn drop_node(
    world: &mut World,
    at: [f32; 3],
    skip: &dyn Fn(crate::ScriptModelId) -> bool,
) -> Result<([f32; 3], f32), Unusable> {
    let start = [at[0], at[1], at[2] + DROP_ABOVE];
    let end = [at[0], at[1], at[2] - DROP_BELOW];
    let down = sweep(world, start, end, DROP_MAXS, skip);
    if down.solid {
        return Err(Unusable::InSolid);
    }
    if down.fraction == 1.0 {
        return Err(Unusable::Floating);
    }
    let landed = std::array::from_fn(|i| start[i] + (end[i] - start[i]) * down.fraction);
    if sweep(world, landed, landed, ROOM_MAXS, skip).solid {
        return Err(Unusable::InSolid);
    }
    Ok((landed, down.fraction))
}

/// Where a parented node's drop landed: the same drop without the movers
/// it names (and then without any entity) tells which collision it met.
fn landing(
    world: &mut World,
    at: [f32; 3],
    down: f32,
    named: &std::collections::BTreeSet<crate::ScriptModelId>,
) -> Landing {
    let start = [at[0], at[1], at[2] + DROP_ABOVE];
    let end = [at[0], at[1], at[2] - DROP_BELOW];
    let past_named = sweep(world, start, end, DROP_MAXS, &|id| named.contains(&id));
    if past_named.fraction > down {
        return Landing::Named;
    }
    let past_all = sweep(world, start, end, DROP_MAXS, &|_| true);
    if past_all.fraction > down {
        Landing::Other
    } else {
        Landing::World
    }
}

/// The level's path nodes dropped to the floor as the game loads it, before
/// any script runs: every node that does not ride a mover first, with the
/// dynamic paths (and vehicles) not solid, then those that do, against
/// everything. A node that starts in solid, finds no floor within 256, or
/// lands without room becomes a bad node. A node that rides a mover (while
/// `enable_moving_paths` is 1) must name it by `target` (a traversal's
/// begin node through the end node it targets): landing on that mover
/// moves it there; landing on the world leaves it where it stands,
/// suspended, as long as some mover has that name; anything else makes it
/// a bad node. Once per level.
fn drop_nodes(world: &mut World) {
    let Some(paths) = FrameWorld::from_world(world).actor_paths() else {
        return;
    };
    if FrameWorld::from_world(world).paths_dropped() {
        return;
    }
    let moving = super::atoi(&dvar_text(world, "enable_moving_paths")) == 1;
    let scene = DropScene::of(world);
    let mut nodes = paths.nodes().to_vec();
    let spawnflags =
        |node: &crate::script::NavNode| node.fields.as_ref().map_or(0, |f| f.spawnflags);
    let mut bad = [0usize; 3];
    let mut moved = 0usize;
    let mut farthest = 0.0f32;
    let mut riders = 0usize;
    for pass_parented in [false, true] {
        for index in 0..nodes.len() {
            let parented = spawnflags(&nodes[index]) & PARENTED != 0;
            if parented != pass_parented {
                continue;
            }
            let at = nodes[index].origin;
            let dynamic = &scene.dynamic;
            let skip_dynamic = |id: crate::ScriptModelId| dynamic.contains(&id);
            let skip: &dyn Fn(crate::ScriptModelId) -> bool =
                if parented { &|_| false } else { &skip_dynamic };
            let outcome = drop_node(world, at, skip).and_then(|(landed, down)| {
                if !(moving && parented) {
                    return Ok((landed, false));
                }
                riders += 1;
                let mut target = nodes[index].target.clone();
                if nodes[index].kind == crate::script::NavNodeKind::NegotiationBegin
                    && let Some(end) = nodes.iter().find(|node| {
                        node.kind == crate::script::NavNodeKind::NegotiationEnd
                            && spawnflags(node) & PARENTED != 0
                            && node.targetname == target
                    })
                {
                    target = end.target.clone();
                }
                if target.is_empty() {
                    return Err(Unusable::NoParent);
                }
                let named: std::collections::BTreeSet<crate::ScriptModelId> = scene
                    .movers
                    .iter()
                    .filter(|(name, _, _)| *name == target)
                    .filter_map(|(_, presence, _)| *presence)
                    .collect();
                let rides = |name: &str| {
                    scene
                        .movers
                        .iter()
                        .find(|(mover, _, _)| mover == name)
                        .is_some_and(|(_, _, rides)| *rides)
                };
                match landing(world, at, down, &named) {
                    Landing::Named if rides(&target) => Ok((landed, false)),
                    Landing::World if rides(&target) => Ok((at, true)),
                    _ => Err(Unusable::NoParent),
                }
            });
            let node = &mut nodes[index];
            match outcome {
                Ok((landed, suspended)) => {
                    if landed != node.origin {
                        moved += 1;
                        farthest = farthest.max((node.origin[2] - landed[2]).abs());
                    }
                    node.origin = landed;
                    if suspended && let Some(fields) = node.fields.as_mut() {
                        fields.spawnflags |= SUSPENDED;
                    }
                }
                Err(why) => {
                    bad[why as usize] += 1;
                    diag::debug!(
                        Sim,
                        "paths: node {index} at {:?} is {}",
                        node.origin,
                        match why {
                            Unusable::InSolid => "in solid",
                            Unusable::Floating => "floating",
                            Unusable::NoParent => "without its mover",
                        }
                    );
                    node.kind = crate::script::NavNodeKind::Bad;
                    if let Some(fields) = node.fields.as_mut() {
                        fields.type_name = "BAD NODE".into();
                    }
                }
            }
        }
    }
    diag::info!(
        Sim,
        "paths: {} nodes dropped to the floor ({moved} moved, at most {farthest:.1} down or up, {riders} ride movers); bad: {} in solid, {} floating, {} without their mover",
        nodes.len(),
        bad[0],
        bad[1],
        bad[2]
    );
    FrameWorld::from_world(world).set_dropped_paths(crate::script::ActorPaths::with_nodes(nodes));
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    registry.register(Function, "getnodesinradius", |world, _, args| {
        nodes_in_radius(world, args, false)
    });
    registry.register(Function, "getnodesinradiussorted", |world, _, args| {
        nodes_in_radius(world, args, true)
    });

    registry.register(Method, "disconnectpaths", |world, receiver, _| {
        let id = super::super::engine::entity_id(world, receiver)?;
        if !dynamic_paths::dynamic_path(world, id) {
            return dynamic_paths::refuse(world, id, "disconnect paths");
        }
        dynamic_paths::disconnect(world, id, HULL);
        Ok(Value::Undefined)
    });
    registry.register(Method, "connectpaths", |world, receiver, _| {
        let id = super::super::engine::entity_id(world, receiver)?;
        if !dynamic_paths::dynamic_path(world, id) {
            return dynamic_paths::refuse(world, id, "connect paths");
        }
        dynamic_paths::connect(world, id);
        Ok(Value::Undefined)
    });
    registry.on_tick(|world| {
        drop_nodes(world);
        dynamic_paths::cut_level(world, HULL);
    });
}
