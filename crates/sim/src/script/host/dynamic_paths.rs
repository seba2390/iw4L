//! Dynamic paths: a `script_model` or `script_brushmodel` spawned with
//! spawnflag 1 (DYNAMICPATH) cuts the path links that cross it when the level
//! starts, and `disconnectpaths` / `connectpaths` cut and rejoin them later
//! (a door that opens, debris that is cleared). The games differ only in the
//! body a link is tested with, which each passes as a [`LinkHull`].

use bevy_ecs::prelude::World;

use super::actor_nav::{LinkEdits, NavNodeKind};
use super::entities::EntityKind;
use crate::frame::FrameWorld;
use crate::script::{RoundScript, Value};

/// The classes whose spawn marks them as dynamic paths when spawnflag 1 is
/// set.
const DYNAMIC_PATH_CLASSES: [&str; 2] = ["script_model", "script_brushmodel"];
const DYNAMIC_PATH_SPAWNFLAG: i32 = 1;
/// A node this far outside an entity's bounds keeps its links.
const NODE_REACH: f32 = 266.0;
/// The link body's half width.
const HULL_HALF: f32 = 15.0;

/// The body a link is tested with: 15 each way, from `low` to `high` above
/// the node. With a `sweep_mask` the body is swept along the link against a
/// brush model's own brushes of those contents; without one (and for a
/// model) against the entity's bounds.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LinkHull {
    pub(crate) low: f32,
    pub(crate) high: f32,
    pub(crate) sweep_mask: Option<u32>,
}

fn field(world: &mut World, id: u64, name: &str) -> Value {
    world.resource_mut::<RoundScript>().object_field(id, name)
}

fn vector(value: Value) -> [f32; 3] {
    match value {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    }
}

/// Spawned as a dynamic path: one of the two classes with spawnflag 1.
pub(crate) fn dynamic_path(world: &mut World, id: u64) -> bool {
    let class_ok = world
        .resource::<RoundScript>()
        .entities
        .get(&id)
        .is_some_and(|entity| DYNAMIC_PATH_CLASSES.contains(&&*entity.classname));
    class_ok
        && matches!(field(world, id, "spawnflags"), Value::Int(flags) if flags & DYNAMIC_PATH_SPAWNFLAG != 0)
}

/// A turned box: origin, axes (forward, left, up), mins, maxs.
type Obstacle = ([f32; 3], [[f32; 3]; 3], [f32; 3], [f32; 3]);

/// The box the entity's collision is tested as: its brush model's bounds,
/// else its model's, turned by its angles: (origin, axes, mins, maxs).
fn obstacle(world: &mut World, id: u64) -> Option<Obstacle> {
    let origin = vector(field(world, id, "origin"));
    let angles = vector(field(world, id, "angles"));
    let brush = world.resource::<RoundScript>().entities.get(&id)?.brush;
    let model = match field(world, id, "model") {
        Value::String(name) => name.to_string(),
        _ => String::new(),
    };
    let frame = FrameWorld::from_world(world);
    let (mins, maxs) = if let Some(n) = brush {
        let model = frame.clip_cmodels().models.get(n as usize)?;
        (model.mins, model.maxs)
    } else {
        let (mid, half) = frame.model_capability(&model).flatten()?.bounds?;
        (
            std::array::from_fn(|i| mid[i] - half[i]),
            std::array::from_fn(|i| mid[i] + half[i]),
        )
    };
    Some((origin, axes(angles), mins, maxs))
}

/// The axes of `angles` (pitch, yaw, roll in degrees): forward, left, up.
fn axes(angles: [f32; 3]) -> [[f32; 3]; 3] {
    let [p, y, r] = angles.map(f32::to_radians);
    let (sp, cp) = p.sin_cos();
    let (sy, cy) = y.sin_cos();
    let (sr, cr) = r.sin_cos();
    let forward = [cp * cy, cp * sy, -sp];
    let right = [-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp];
    let up = [cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp];
    [forward, right.map(|c| -c), up]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Whether the hull moved from `a` to `b` (node origins) crosses the
/// turned box.
fn crosses((origin, axis, mins, maxs): Obstacle, hull: LinkHull, a: [f32; 3], b: [f32; 3]) -> bool {
    let local = |p: [f32; 3]| {
        let d = std::array::from_fn(|i| p[i] - origin[i]);
        [dot(d, axis[0]), dot(d, axis[1]), dot(d, axis[2])]
    };
    let (la, lb) = (local(a), local(b));
    // The upright hull's reach along the box's flat axes.
    let flat = HULL_HALF * (axis[0][0].abs() + axis[0][1].abs());
    let lo = [mins[0] - flat, mins[1] - flat, mins[2] - hull.high];
    let hi = [maxs[0] + flat, maxs[1] + flat, maxs[2] - hull.low];
    let (mut enter, mut leave) = (0.0f32, 1.0f32);
    for i in 0..3 {
        let d = lb[i] - la[i];
        if d.abs() < 1e-6 {
            if la[i] < lo[i] || la[i] > hi[i] {
                return false;
            }
            continue;
        }
        let (t0, t1) = ((lo[i] - la[i]) / d, (hi[i] - la[i]) / d);
        enter = enter.max(t0.min(t1));
        leave = leave.min(t0.max(t1));
        if enter > leave {
            return false;
        }
    }
    true
}

/// Cuts the links that cross the entity (after rejoining those it cut
/// before): every link of each node within its bounds and 266 more, and of
/// every negotiation node.
pub(crate) fn disconnect(world: &mut World, id: u64, hull: LinkHull) {
    connect(world, id);
    let Some(paths) = FrameWorld::from_world(world).actor_paths() else {
        return;
    };
    let Some(box_) = obstacle(world, id) else {
        return;
    };
    let (origin, axis, mins, maxs) = box_;
    let brush = world
        .resource::<RoundScript>()
        .entities
        .get(&id)
        .and_then(|entity| entity.brush);
    let angles = vector(field(world, id, "angles"));
    let content = FrameWorld::from_world(world).content();
    let hull_mins = [-HULL_HALF, -HULL_HALF, hull.low];
    let hull_maxs = [HULL_HALF, HULL_HALF, hull.high];
    let blocks = |a: [f32; 3], b: [f32; 3]| match (hull.sweep_mask, brush) {
        (Some(mask), Some(model)) => crate::world::brush_model_sweep_hits(
            &content, model, origin, angles, a, b, hull_mins, hull_maxs, mask,
        )
        .unwrap_or(false),
        _ => crosses(box_, hull, a, b),
    };
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for corner in 0..8 {
        let p = [
            if corner & 1 == 0 { mins[0] } else { maxs[0] },
            if corner & 2 == 0 { mins[1] } else { maxs[1] },
            if corner & 4 == 0 { mins[2] } else { maxs[2] },
        ];
        for i in 0..3 {
            let w = origin[i] + axis[0][i] * p[0] + axis[1][i] * p[1] + axis[2][i] * p[2];
            lo[i] = lo[i].min(w);
            hi[i] = hi[i].max(w);
        }
    }
    let mut cut = Vec::new();
    {
        let edits = world.get_resource_or_insert_with(LinkEdits::default);
        for (index, node) in paths.nodes().iter().enumerate() {
            let near = (0..3).all(|i| {
                lo[i] - NODE_REACH <= node.origin[i] && node.origin[i] <= hi[i] + NODE_REACH
            });
            let negotiation = matches!(
                node.kind,
                NavNodeKind::NegotiationBegin | NavNodeKind::NegotiationEnd
            );
            if !near && !negotiation {
                continue;
            }
            let from = index as u16;
            for (to, _, _) in paths.links(from, &edits) {
                let target = paths.nodes()[usize::from(to)].origin;
                if blocks(node.origin, target) {
                    cut.push((from, to));
                }
            }
        }
    }
    world
        .get_resource_or_insert_with(LinkEdits::default)
        .cut(id, cut);
}

/// Rejoins the links the entity cut.
pub(crate) fn connect(world: &mut World, id: u64) {
    world
        .get_resource_or_insert_with(LinkEdits::default)
        .rejoin(id);
}

/// When the level starts: each dynamic path cuts its links. Once per level;
/// a restart drops the link edits and so cuts again.
pub(crate) fn cut_level(world: &mut World, hull: LinkHull) {
    if world
        .get_resource::<LinkEdits>()
        .is_some_and(LinkEdits::level_cut)
        || FrameWorld::from_world(world).actor_paths().is_none()
    {
        return;
    }
    world
        .get_resource_or_insert_with(LinkEdits::default)
        .mark_level_cut();
    let ids: Vec<u64> = world
        .resource::<RoundScript>()
        .entities
        .iter()
        .filter(|(_, entity)| matches!(entity.kind, EntityKind::Map | EntityKind::Spawned))
        .map(|(id, _)| *id)
        .collect();
    let mut entities = 0;
    for id in ids {
        if world.resource::<RoundScript>().live(&id) && dynamic_path(world, id) {
            disconnect(world, id, hull);
            entities += 1;
        }
    }
    let links = world
        .get_resource_or_insert_with(LinkEdits::default)
        .cut_count();
    diag::info!(
        Sim,
        "paths: {entities} dynamic path entities cut {links} links as the level starts"
    );
}

/// The engine's refusal of an entity that is not a dynamic path.
pub(crate) fn refuse(world: &mut World, id: u64, verb: &str) -> Result<Value, String> {
    let classname = world.resource::<RoundScript>().entities[&id]
        .classname
        .to_string();
    if classname == "script_brushmodel" {
        return Err(format!(
            "script_brushmodel must have DYNAMICPATH set to {verb}"
        ));
    }
    Err(if verb == "disconnect paths" {
        format!("entity of type '{classname}' cannot disconnect paths.\n \n ")
    } else {
        format!("entity of type '{classname}' cannot connect paths \n\n")
    })
}
