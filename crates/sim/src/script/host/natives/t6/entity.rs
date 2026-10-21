//! Black Ops 2's entity geometry builtins: tags on an entity's model, its
//! local frame, and path-node lookups.

use super::super::super::actor_nav::LinkEdits;
use super::super::engine::{collider_entity, entity_id, entity_trace, keyed_array, trace_ignore};
use super::{node_string_field, string, vector};
use crate::bullet_collision::{ColliderId, TraceOutcome};
use crate::script::{Namespace, NativeRegistry, NavNodeKind, RoundScript, Value};
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
    if let Some(client) = world.resource::<RoundScript>().player_client(id) {
        let frame = crate::frame::FrameWorld::from_world(world);
        if let Some(ps) = frame.player(crate::ClientId(client)) {
            if name.eq_ignore_ascii_case("tag_origin") {
                return Ok((
                    ps.origin,
                    math_iw4::angles_to_axis([0.0, ps.viewangles[1], 0.0]),
                ));
            }
            if name.eq_ignore_ascii_case("tag_flash")
                || name.eq_ignore_ascii_case("tag_weapon")
                || name.eq_ignore_ascii_case("tag_weapon_right")
            {
                let eye = [
                    ps.origin[0],
                    ps.origin[1],
                    ps.origin[2] + ps.view_height_current,
                ];
                return Ok((eye, math_iw4::angles_to_axis(ps.viewangles)));
            }
        }
    }
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
        None => bind_pose_tag(world, id, &model, &name).ok_or_else(|| {
            format!(
                "t6.entities.tag_pose: model '{model}' of this entity is not posed on the server"
            )
        }),
    }
}

/// A tag of the model in its bind pose at the entity's origin and angles,
/// for an entity whose model is not posed on the server yet (an actor in
/// the frame it spawns): `t6.entities.tag_pose`.
fn bind_pose_tag(
    world: &mut World,
    id: u64,
    model: &str,
    tag: &str,
) -> Option<super::super::super::presence::TagPose> {
    let origin = vector_field(world, id, "origin");
    let angles = vector_field(world, id, "angles");
    let capability = crate::frame::FrameWorld::from_world(world)
        .model_capability(model)
        .flatten()?;
    let dobj = crate::AuthorityDObjState::at_pose(model, Some(capability), origin, angles);
    let matrix = dobj.tag_world_matrix(tag)?;
    let axis = |v: glam::Vec4| v.truncate().normalize_or_zero().to_array();
    Some((
        matrix.w_axis.truncate().to_array(),
        [
            axis(matrix.x_axis),
            axis(matrix.y_axis),
            axis(matrix.z_axis),
        ],
    ))
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

/// The two path nodes a link builtin names.
fn node_pair(world: &World, args: &[Value]) -> Result<(u16, u16), String> {
    let node = |index: usize| {
        node_index(world, args.get(index).unwrap_or(&Value::Undefined))
            .ok_or_else(|| String::from("not a pathnode"))
    };
    Ok((node(0)?, node(1)?))
}

/// `linknodes`/`unlinknodes`: one direction only; a negotiation node is
/// left as it is.
fn edit_link(world: &mut World, args: &[Value], link: bool) -> Result<Value, String> {
    let (from, to) = node_pair(world, args)?;
    let Some(paths) = crate::frame::FrameWorld::from_world(world).actor_paths() else {
        return Ok(Value::Undefined);
    };
    let begin = |node: u16| paths.nodes()[usize::from(node)].kind == NavNodeKind::NegotiationBegin;
    if begin(from) || begin(to) {
        return Ok(Value::Undefined);
    }
    let mut edits = world.get_resource_or_insert_with(LinkEdits::default);
    if link {
        edits.link(&paths, from, to);
    } else {
        edits.unlink(from, to);
    }
    Ok(Value::Undefined)
}

/// The game's surface names, by the surface type a trace reports.
pub(super) const SURFACES: [&str; 32] = [
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

fn physics_trace(world: &mut World, _: &Value, args: &[Value]) -> Result<Value, String> {
    let (start, end) = (vector(args, 0)?, vector(args, 1)?);
    let mins = if args.len() >= 3 {
        vector(args, 2)?
    } else {
        [0.0; 3]
    };
    let maxs = if args.len() >= 4 {
        vector(args, 3)?
    } else {
        [0.0; 3]
    };
    let ignore = trace_ignore(world, args.get(4));
    let flags = if args.len() >= 6 {
        super::int(args, 5)?
    } else {
        1
    };
    let mut mask = 0u32;
    if flags & 1 != 0 {
        mask |= 0x0280_ec93;
    }
    if flags & 2 != 0 {
        mask |= 0x211;
    }
    if flags & 4 != 0 {
        mask |= 0x20;
    }
    if flags & 8 != 0 {
        mask |= 0x3_76c0;
    }
    if flags == 0x10 {
        mask = 0x200;
    }
    let (fraction, normal, collider) = if mins == [0.0; 3] && maxs == [0.0; 3] {
        match entity_trace(world, start, end, mask, ignore) {
            TraceOutcome::Hit {
                fraction,
                normal,
                collider,
                ..
            } => (fraction, normal, Some(collider)),
            TraceOutcome::StartSolid { collider, .. } => (0.0, [0.0; 3], collider),
            _ => (1.0, [0.0; 3], None),
        }
    } else {
        let hit = super::super::engine::trace(world, start, end, mins, maxs, mask);
        let fraction = if hit.startsolid != 0 {
            0.0
        } else {
            hit.fraction.clamp(0.0, 1.0)
        };
        (fraction, hit.normal, None)
    };
    let position: [f32; 3] = std::array::from_fn(|i| start[i] + (end[i] - start[i]) * fraction);
    let entity = collider.map_or(Value::Undefined, |c| collider_entity(world, c));
    let (normal, surface) = match collider {
        Some(collider) if fraction < 1.0 => (normal, surface(collider)),
        _ if fraction < 1.0 => (normal, "default"),
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
    registry.register(Function, "physicstrace", physics_trace);
    registry.register(Method, "getshootatpos", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        if let Some(client) = world.resource::<RoundScript>().player_client(id)
            && let Some(eye) = crate::script_player::eye(
                &crate::frame::FrameWorld::from_world(world),
                crate::ClientId(client),
            )
        {
            return Ok(Value::Vector(eye));
        }
        let origin = vector_field(world, id, "origin");
        let is_actor = world
            .resource::<RoundScript>()
            .entities
            .get(&id)
            .is_some_and(|e| e.kind == super::super::super::entities::EntityKind::Actor);
        Ok(Value::Vector([
            origin[0],
            origin[1],
            origin[2] + if is_actor { 56.0 } else { 0.0 },
        ]))
    });
    registry.register(Method, "setplayercollision", |world, receiver, args| {
        entity_id(world, receiver)?;
        super::int(args, 0)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setforcenocull", |world, receiver, _| {
        entity_id(world, receiver)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setscriptmoverflag", |world, receiver, args| {
        entity_id(world, receiver)?;
        super::int(args, 0)?;
        Ok(Value::Undefined)
    });
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
    // gscm_dontinterpolate flips the entity's teleport flag, a player's in
    // his player state, a drawn entity's in its entity state.
    registry.register(Method, "dontinterpolate", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        if let Some(client) = world.resource::<RoundScript>().player_client(id) {
            crate::script_player::snap(
                &mut crate::frame::FrameWorld::from_world(world),
                crate::ClientId(client),
            );
        } else if let Some(presence) = world
            .resource::<RoundScript>()
            .entities
            .get(&id)
            .and_then(|entity| entity.presence)
        {
            crate::script_player::snap_presence(
                &mut crate::frame::FrameWorld::from_world(world),
                presence,
            );
        }
        Ok(Value::Undefined)
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
        let (from, to) = node_pair(world, args)?;
        let paths = crate::frame::FrameWorld::from_world(world).actor_paths();
        let edits = world.get_resource_or_insert_with(LinkEdits::default);
        let linked =
            paths.is_some_and(|paths| paths.links(from, &edits).any(|(link, _, _)| link == to));
        Ok(Value::Int(linked.into()))
    });
    registry.register(Function, "linknodes", |world, _, args| {
        edit_link(world, args, true)
    });
    registry.register(Function, "unlinknodes", |world, _, args| {
        edit_link(world, args, false)
    });
}
