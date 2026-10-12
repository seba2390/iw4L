//! Black Ops 2's entity geometry builtins: tags on an entity's model, its
//! local frame, and path-node lookups.

use super::super::engine::{collider_entity, entity_id, entity_trace, keyed_array, trace_ignore};
use super::{node_string_field, string, vector};
use crate::bullet_collision::{ColliderId, TraceOutcome};
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

/// The game's surface names, by the surface type a trace reports.
const SURFACES: [&str; 32] = [
    "default",
    "bark",
    "brick",
    "carpet",
    "cloth",
    "concrete",
    "dirt",
    "flesh",
    "foliage",
    "glass",
    "grass",
    "gravel",
    "ice",
    "metal",
    "mud",
    "paper",
    "plaster",
    "rock",
    "sand",
    "snow",
    "water",
    "wood",
    "asphalt",
    "ceramic",
    "plastic",
    "rubber",
    "cushion",
    "fruit",
    "paintedmetal",
    "player",
    "tallgrass",
    "riotshield",
];
/// A ground trace's contents, with characters or without.
const GROUND_MASK: u32 = 0x0280_ecb3;
const GROUND_MASK_NO_CHARACTERS: u32 = 0x0080_6cb3;
/// The contents the fifth and sixth arguments leave out.
const GROUND_WATER: u32 = 0x20;
const GROUND_SIXTH: u32 = 0x10;

fn surface(collider: ColliderId) -> &'static str {
    let flags = match collider {
        ColliderId::World { surface_flags, .. }
        | ColliderId::EntityDObjBone { surface_flags, .. }
        | ColliderId::EntityLinkedBrush { surface_flags, .. } => surface_flags,
        ColliderId::Player { .. } => return "flesh",
    };
    SURFACES
        .get(((flags >> 20) & 0x3f) as usize)
        .copied()
        .unwrap_or("default")
}

/// `groundtrace(start, end, characters, [ignore], [no water], [sixth])`:
/// fraction, position, entity, normal and surface type.
fn ground_trace(world: &mut World, _: &Value, args: &[Value]) -> Result<Value, String> {
    let (start, end) = (vector(args, 0)?, vector(args, 1)?);
    let mut mask = if super::int(args, 2)? != 0 {
        GROUND_MASK
    } else {
        GROUND_MASK_NO_CHARACTERS
    };
    if args.len() == 5 && super::int(args, 4)? != 0 {
        mask &= !GROUND_WATER;
    }
    if args.len() == 6 && super::int(args, 5)? != 0 {
        mask &= !GROUND_SIXTH;
    }
    let ignore = trace_ignore(world, args.get(3));
    let (fraction, normal, collider) = match entity_trace(world, start, end, mask, ignore) {
        TraceOutcome::Hit {
            fraction,
            normal,
            collider,
            ..
        } => (fraction, normal, Some(collider)),
        TraceOutcome::StartSolid { collider, .. } => (0.0, [0.0; 3], collider),
        _ => (1.0, [0.0; 3], None),
    };
    let position: [f32; 3] = std::array::from_fn(|i| start[i] + (end[i] - start[i]) * fraction);
    let entity = collider.map_or(Value::Undefined, |c| collider_entity(world, c));
    let (normal, surface) = match collider {
        Some(collider) if fraction < 1.0 => (normal, surface(collider)),
        _ => {
            let d: [f32; 3] = std::array::from_fn(|i| end[i] - start[i]);
            let length = dot(d, d).sqrt();
            let d = if length > 0.0 {
                d.map(|c| c / length)
            } else {
                d
            };
            (d, "none")
        }
    };
    keyed_array(
        world,
        vec![
            ("fraction", Value::Float(fraction)),
            ("position", Value::Vector(position)),
            ("entity", entity),
            ("normal", Value::Vector(normal)),
            ("surfacetype", Value::string(surface)),
        ],
    )
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    registry.register(Function, "groundtrace", ground_trace);
    registry.register(Method, "getlinkedent", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        Ok(world
            .resource::<RoundScript>()
            .entities
            .get(&id)
            .and_then(|entity| entity.linked_to.as_ref())
            .map_or(Value::Undefined, |link| Value::Object(link.parent)))
    });
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
    registry.register(Method, "setcandamage", |world, receiver, args| {
        let id = entity_id(world, receiver)?;
        let on = super::int(args, 0)?;
        world
            .resource_mut::<RoundScript>()
            .set_object_field(id, "takedamage", Value::Int(on));
        Ok(Value::Undefined)
    });
    // Actors, vehicles and players do not link things to themselves.
    registry.register(Method, "enablelinkto", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        let runtime = world.resource::<RoundScript>();
        let player = runtime.player_client(id).is_some();
        let refused = player
            || runtime.entities.get(&id).is_some_and(|entity| {
                matches!(
                    entity.kind,
                    super::super::super::entities::EntityKind::Actor
                        | super::super::super::entities::EntityKind::Vehicle
                )
            });
        if refused {
            let classname = text_field(world, id, "classname");
            let kind = if player { "player" } else { "actor/vehicle" };
            return Err(format!(
                "entity (classname: '{classname}', type: '{kind}') does not currently support enableLinkTo"
            ));
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "dontinterpolate", |world, receiver, _| {
        entity_id(world, receiver)?;
        Err(
            "t6.entities.interpolation: the client's interpolation of an entity is not switched"
                .into(),
        )
    });
    // The nearest node of any type within 192 units a body can reach.
    registry.register(Function, "getnearestnode", |world, _, args| {
        let at = vector(args, 0)?;
        match super::movement::nearest_node_within(world, 0, at, &[super::movement::NODE_NEAR]) {
            Some(index) => super::super::super::actor_nav::node_value(world, index),
            None => Ok(Value::Undefined),
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
