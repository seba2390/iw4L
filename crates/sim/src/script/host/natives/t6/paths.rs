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
    registry.on_tick(|world| dynamic_paths::cut_level(world, HULL));
}
