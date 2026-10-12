//! Black Ops 2's entity geometry builtins: tags on an entity's model, its
//! local frame, and path-node lookups.

use super::super::engine::entity_id;
use super::{node_string_field, string, vector};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;

fn text_field(world: &mut World, id: u64, name: &str) -> String {
    match world.resource_mut::<RoundScript>().object_field(id, name) {
        Value::String(text) => text.to_string(),
        _ => String::new(),
    }
}

fn vector_field(world: &mut World, id: u64, name: &str) -> [f32; 3] {
    match world.resource_mut::<RoundScript>().object_field(id, name) {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    }
}

/// A tag's pose on the entity's model, with the engine's errors for an
/// entity without a model and a tag its model lacks.
fn tag(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<super::super::super::presence::TagPose, String> {
    let id = entity_id(world, receiver)?;
    let name = string(args, 0)?;
    let model = text_field(world, id, "model");
    if model.is_empty() {
        let classname = text_field(world, id, "classname");
        return Err(format!(
            "entity has no model defined (classname '{classname}')"
        ));
    }
    match super::super::super::presence::tag_pose(world, id, &name) {
        Some(Some(pose)) => Ok(pose),
        Some(None) => Err(format!(
            "tag '{name}' does not exist in model '{model}' (or any attached submodels)"
        )),
        None => Err(format!(
            "t6.entities.tag_pose: model '{model}' of this entity is not posed on the server"
        )),
    }
}

/// The entity's axis: forward, left, up.
fn axis(world: &mut World, id: u64) -> [[f32; 3]; 3] {
    math_iw4::angles_to_axis(vector_field(world, id, "angles"))
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// A path node's index, from its script object.
fn node_index(world: &World, value: &Value) -> Option<u16> {
    let Value::Object(id) = value else {
        return None;
    };
    world
        .resource::<RoundScript>()
        .path_node_objects
        .iter()
        .find(|(_, object)| *object == id)
        .map(|(index, _)| *index)
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    registry.register(Method, "gettagorigin", |world, receiver, args| {
        Ok(Value::Vector(tag(world, receiver, args)?.0))
    });
    registry.register(Method, "gettagangles", |world, receiver, args| {
        Ok(Value::Vector(math_iw4::axis_to_angles(
            tag(world, receiver, args)?.1,
        )))
    });
    registry.register(Method, "worldtolocalcoords", |world, receiver, args| {
        let id = entity_id(world, receiver)?;
        let point = vector(args, 0)?;
        let origin = vector_field(world, id, "origin");
        let delta: [f32; 3] = std::array::from_fn(|i| point[i] - origin[i]);
        let [forward, left, up] = axis(world, id);
        Ok(Value::Vector([
            dot(delta, forward),
            dot(delta, left),
            dot(delta, up),
        ]))
    });
    registry.register(Method, "localtoworldcoords", |world, receiver, args| {
        let id = entity_id(world, receiver)?;
        let local = vector(args, 0)?;
        let origin = vector_field(world, id, "origin");
        let axis = axis(world, id);
        Ok(Value::Vector(std::array::from_fn(|i| {
            origin[i] + axis[0][i] * local[0] + axis[1][i] * local[1] + axis[2][i] * local[2]
        })))
    });
    // The one node whose string field holds the value; none when no node does.
    registry.register(Function, "getnode", |world, _, args| {
        let value = string(args, 0)?;
        let key = string(args, 1)?;
        let field = node_string_field(&key)?;
        let nodes = super::super::super::actor_nav::nodes_where(world, |node| {
            let text = field(node);
            !text.is_empty() && *text == *value
        })?;
        match nodes.as_slice() {
            [] => Ok(Value::Undefined),
            [node] => Ok(node.clone()),
            _ => Err("getnode used with more than one node".into()),
        }
    });
    registry.register(Function, "nodesarelinked", |world, _, args| {
        let from =
            node_index(world, args.first().unwrap_or(&Value::Undefined)).ok_or("not a pathnode")?;
        let to =
            node_index(world, args.get(1).unwrap_or(&Value::Undefined)).ok_or("not a pathnode")?;
        let linked = crate::frame::FrameWorld::from_world(world)
            .actor_paths()
            .and_then(|paths| {
                paths
                    .nodes()
                    .get(usize::from(from))
                    .map(|node| node.links.iter().any(|&(link, _, _)| link == to))
            })
            .unwrap_or(false);
        Ok(Value::Int(linked.into()))
    });
}
