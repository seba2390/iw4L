//! Black Ops' client triggers: the map's `trigger_multiple`,
//! `trigger_radius` and `trigger_once` with spawnflag `0x80` live on the
//! client alone. Each client frame the local player and the actors test
//! their box against them: a touch notifies `touch` on both, and the
//! trigger's `trigger` notify waits in a queue that lets each toucher set off
//! one trigger a frame. A one-shot trigger runs `codescripts/delete::main`.

use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

use super::super::host::entities::EntityKind;
use super::super::{RoundScript, Value};
use super::ClientScripts;

/// What a client trigger answers to: its contents, by the spawnflags that
/// name its touchers.
const CONTENTS_PLAYER: u32 = 0x4000_0000;
const CONTENTS_AXIS: u32 = 0x0004_0000;
const CONTENTS_ALLIES: u32 = 0x0008_0000;
const CONTENTS_NEUTRAL: u32 = 0x0010_0000;
const CONTENTS_VEHICLE: u32 = 0x0000_0008;

/// A player's or an actor's box against client triggers.
const TOUCHER_MINS: [f32; 3] = [-15.0, -15.0, 0.0];
const TOUCHER_MAXS: [f32; 3] = [15.0, 15.0, 70.0];

/// The `trigger` notifies the queue holds at most; past it a touch notifies
/// at once.
const QUEUE_CAPACITY: usize = 0x100;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Shape {
    /// A brush model (`*N`) in the world.
    Brush(crate::script::host::triggers::ModelVolume),
    /// An upright cylinder from its origin.
    Cylinder { radius: f32, height: f32 },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ClientTrigger {
    contents: u32,
    /// Runs `codescripts/delete::main` when touched.
    once: bool,
    pub(crate) shape: Shape,
}

/// The client's own triggers, by client entity.
#[derive(Resource, Default)]
pub(crate) struct ClientTriggers(pub(crate) BTreeMap<u64, ClientTrigger>);

/// A client trigger's contents from its spawnflags: players unless 8, axis
/// 1, allies 2, neutral 4 and vehicles `0x10`.
fn contents(spawnflags: i32) -> u32 {
    let mut contents = 0;
    if spawnflags & 8 == 0 {
        contents = CONTENTS_PLAYER;
    }
    if spawnflags & 1 != 0 {
        contents |= CONTENTS_AXIS;
    }
    if spawnflags & 2 != 0 {
        contents |= CONTENTS_ALLIES;
    }
    if spawnflags & 4 != 0 {
        contents |= CONTENTS_NEUTRAL;
    }
    if spawnflags & 0x10 != 0 {
        contents |= CONTENTS_VEHICLE;
    }
    contents
}

/// The client's triggers among its map entities, as its spawn makes them:
/// a `trigger_once`, spawnflag `0x40` or a `wait` of 0 or less makes one
/// fire once.
pub(super) fn spawn(server: &World, world: &mut World) {
    /// A map trigger: its entity, classname, brush model and cylinder.
    type Candidate = (u64, std::sync::Arc<str>, Option<u32>, Option<(f32, f32)>);
    let state = server.get::<crate::world::SimState>(crate::frame::state_entity(server));
    let mut runtime = world.resource_mut::<RoundScript>();
    let candidates: Vec<Candidate> = runtime
        .entities
        .iter()
        .filter(|(_, e)| e.client_only && e.kind == EntityKind::Map)
        .filter(|(_, e)| {
            matches!(
                &*e.classname,
                "trigger_multiple" | "trigger_radius" | "trigger_once"
            )
        })
        .map(|(id, e)| (*id, e.classname.clone(), e.brush, e.cylinder))
        .collect();
    let mut triggers = BTreeMap::new();
    for (id, classname, brush, cylinder) in candidates {
        let spawnflags = match runtime.object_field(id, "spawnflags") {
            Value::Int(n) => n,
            _ => 0,
        };
        let wait = match runtime.object_field(id, "wait") {
            Value::Float(n) => Some(n),
            Value::Int(n) => Some(n as f32),
            _ => None,
        };
        let shape = match (&*classname, cylinder, brush) {
            ("trigger_radius", Some((radius, height)), _) => Shape::Cylinder { radius, height },
            (_, _, Some(brush)) => {
                // Black Ops kills a trigger whose brush model is invalid.
                let Some(volume) = state
                    .and_then(|state| crate::script::host::triggers::model_volume(state, brush))
                else {
                    continue;
                };
                Shape::Brush(volume)
            }
            _ => continue,
        };
        let once = classname.as_ref() == "trigger_once"
            || spawnflags & 0x40 != 0
            || (classname.as_ref() != "trigger_once" && wait.is_some_and(|wait| wait <= 0.0));
        triggers.insert(
            id,
            ClientTrigger {
                contents: contents(spawnflags),
                once,
                shape,
            },
        );
    }
    world.insert_resource(ClientTriggers(triggers));
}

/// Grows a box upward and downward alike until it is as tall as it is wide.
fn cubed(mut mins: [f32; 3], mut maxs: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let height = maxs[2] - mins[2];
    let mut width = maxs[0] - mins[0];
    if width - (maxs[1] - mins[1]) < 0.0 {
        width = maxs[1] - mins[1];
    }
    if height < width {
        let grow = (width - height) * 0.5;
        mins[2] -= grow;
        maxs[2] += grow;
    }
    (mins, maxs)
}

/// The box a player or an actor touches client triggers with.
pub(super) fn toucher_box(origin: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    cubed(
        std::array::from_fn(|i| origin[i] + TOUCHER_MINS[i]),
        std::array::from_fn(|i| origin[i] + TOUCHER_MAXS[i]),
    )
}

/// Whether the box `mins..maxs` touches a client trigger at `origin`: a
/// brush model the box starts inside, or the cylinder's height and its
/// radius grown by the box's half width.
pub(crate) fn touches(shape: &Shape, origin: [f32; 3], mins: [f32; 3], maxs: [f32; 3]) -> bool {
    match *shape {
        Shape::Brush(ref volume) => volume.touches(origin, mins, maxs),
        Shape::Cylinder { radius, height } => {
            if !(origin[2] < maxs[2] && mins[2] < height + origin[2]) {
                return false;
            }
            let x = (mins[0] + maxs[0]) * 0.5;
            let y = (mins[1] + maxs[1]) * 0.5 - origin[1];
            let reach = (maxs[0] - x) + radius;
            let x = x - origin[0];
            y * y + x * x < reach * reach
        }
    }
}

fn origin_of(runtime: &mut RoundScript, id: u64) -> [f32; 3] {
    match runtime.object_field(id, "origin") {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    }
}

/// The contents an entity touches client triggers as: a player's, or an
/// actor's by its team; none for anything else.
fn toucher_contents(kind: &EntityKind, team: Option<&str>) -> Option<u32> {
    match kind {
        EntityKind::Player => Some(CONTENTS_PLAYER),
        EntityKind::Actor => match team? {
            "axis" => Some(CONTENTS_AXIS),
            "allies" => Some(CONTENTS_ALLIES),
            "neutral" => Some(CONTENTS_NEUTRAL),
            _ => None,
        },
        _ => None,
    }
}

/// Tests the local player and the actors against the client's triggers,
/// then lets the queued `trigger` notifies go.
pub(super) fn run(client: &mut ClientScripts, now: i64) {
    let triggers: Vec<(u64, ClientTrigger)> = client
        .world
        .resource::<ClientTriggers>()
        .0
        .iter()
        .map(|(id, t)| (*id, t.clone()))
        .collect();
    if triggers.is_empty() {
        return;
    }
    let local = client.world.resource::<super::ClientView>().locals[0].object;
    let touchers: Vec<(u64, u32, [f32; 3])> = {
        let mut runtime = client.world.resource_mut::<RoundScript>();
        let ids: Vec<(u64, EntityKind)> = runtime
            .entities
            .iter()
            .filter(|(id, e)| **id == local || e.kind == EntityKind::Actor)
            .map(|(id, e)| (*id, e.kind.clone()))
            .collect();
        ids.into_iter()
            .filter_map(|(id, kind)| {
                let team = match runtime.object_field(id, "team") {
                    Value::String(team) => Some(team),
                    _ => None,
                };
                let contents = toucher_contents(&kind, team.as_deref())?;
                Some((id, contents, origin_of(&mut runtime, id)))
            })
            .collect()
    };
    for (toucher, mask, origin) in touchers {
        let (mins, maxs) = toucher_box(origin);
        for (trigger, shape) in &triggers {
            let trigger = *trigger;
            if shape.contents & mask == 0 {
                continue;
            }
            let at = {
                let mut runtime = client.world.resource_mut::<RoundScript>();
                if !runtime.entities.contains_key(&trigger) {
                    continue;
                }
                origin_of(&mut runtime, trigger)
            };
            if !touches(&shape.shape, at, mins, maxs) {
                continue;
            }
            for (on, other) in [(trigger, toucher), (toucher, trigger)] {
                crate::script::runtime::notify_now(
                    &mut client.world,
                    Value::Object(on),
                    "touch",
                    vec![Value::Object(other)],
                    now,
                );
            }
            touched(client, toucher, trigger, shape.once, now);
        }
    }
    flush(client, now);
}

/// `ent isTouching(other)`: of two client entities, the client trigger
/// (or brush model) tests the other's box. A player's or an actor's box is
/// its toucher box; any other entity's would be its model's bounds, which the
/// client VM does not read, so it touches nothing.
pub(crate) fn is_touching(world: &mut World, a: u64, b: u64) -> Result<bool, String> {
    let solid = |world: &mut World, id: u64| {
        world.resource::<ClientTriggers>().0.contains_key(&id)
            || matches!(
                world.resource_mut::<RoundScript>().object_field(id, "model"),
                Value::String(model) if model.starts_with('*')
            )
    };
    let (trigger, other) = if solid(world, a) {
        if solid(world, b) {
            return Err("istouching cannot be called on 2 brush/cylinder entities".into());
        }
        (a, b)
    } else {
        (b, a)
    };
    let kind = world
        .resource::<RoundScript>()
        .entities
        .get(&other)
        .map(|e| e.kind.clone());
    if !matches!(kind, Some(EntityKind::Player | EntityKind::Actor)) {
        return Ok(false);
    }
    let Some(shape) = world
        .resource::<ClientTriggers>()
        .0
        .get(&trigger)
        .map(|t| t.shape.clone())
    else {
        return Ok(false);
    };
    let mut runtime = world.resource_mut::<RoundScript>();
    let (mins, maxs) = toucher_box(origin_of(&mut runtime, other));
    let at = origin_of(&mut runtime, trigger);
    Ok(touches(&shape, at, mins, maxs))
}

/// A touched trigger notifies `trigger` from the queue; a one-shot one
/// runs `codescripts/delete::main` on itself.
fn touched(client: &mut ClientScripts, toucher: u64, trigger: u64, once: bool, now: i64) {
    if !client
        .world
        .resource::<RoundScript>()
        .entities
        .contains_key(&trigger)
    {
        return;
    }
    if client.trigger_queue.len() == QUEUE_CAPACITY {
        crate::script::runtime::notify_now(
            &mut client.world,
            Value::Object(toucher),
            "trigger",
            vec![Value::Object(trigger)],
            now,
        );
    } else {
        client.trigger_queue.push((toucher, trigger));
    }
    if once && let Some(delete) = client.callbacks.map(|c| c.trigger_delete) {
        super::start_callback(
            &mut client.world,
            delete,
            Value::Object(trigger),
            Vec::new(),
        );
    }
}

/// Each queued toucher sets off one trigger a frame; a pair whose toucher
/// or trigger is gone leaves the queue unheard.
fn flush(client: &mut ClientScripts, now: i64) {
    let mut served = std::collections::BTreeSet::new();
    let queue = std::mem::take(&mut client.trigger_queue);
    for (toucher, trigger) in queue {
        let alive = {
            let runtime = client.world.resource::<RoundScript>();
            runtime.entities.contains_key(&toucher) && runtime.entities.contains_key(&trigger)
        };
        if !alive {
            continue;
        }
        if !served.insert(toucher) {
            client.trigger_queue.push((toucher, trigger));
            continue;
        }
        crate::script::runtime::notify_now(
            &mut client.world,
            Value::Object(trigger),
            "trigger",
            vec![Value::Object(toucher)],
            now,
        );
    }
}
