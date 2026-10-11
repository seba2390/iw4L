//! Black Ops 2's actor movement: the goal scripts give an actor, the path
//! planned to it over the map's path nodes, the animscript the engine runs
//! for it (stop, move or combat), and the walk its clips' root motion
//! drives along the path.

use super::super::super::entities::EntityKind;
use super::super::engine::{entity_id, trace};
use super::{anim, vector};
use crate::frame::FrameWorld;
use crate::script::runtime::{raise, run_now};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

/// The goal an actor starts with: where it spawned, 4 across and 80 high.
const GOAL_RADIUS: f32 = 4.0;
const GOAL_HEIGHT: f32 = 80.0;
/// How far the engine looks for the node nearest a position, then further.
const NODE_NEAR: f32 = 192.0;
const NODE_FAR: f32 = 512.0;
const ACTOR_MASK: u32 = 0x0282_0011;
/// IW4L's walking rules (`t6.actors.path_following`).
const POINT_REACHED: f32 = 16.0;
const TURN_RATE: f32 = 360.0;
const STEP_HEIGHT: f32 = 18.0;
const GRAVITY: f32 = 800.0;
const ANIMSCRIPTS: &str = "maps/mp/animscripts";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Script {
    Stop,
    Move,
    Combat,
}

impl Script {
    fn name(self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::Move => "move",
            Self::Combat => "combat",
        }
    }
}

#[derive(Clone, Debug, Default)]
struct Movement {
    goal: Option<[f32; 3]>,
    goal_sent: bool,
    /// The path to the goal; planned again when the goal changes.
    planned: bool,
    path: Vec<[f32; 3]>,
    script: Option<Script>,
    falling: f32,
}

#[derive(Resource, Default)]
struct Movements(BTreeMap<u64, Movement>);

fn movement(world: &mut World, id: u64) -> &mut Movement {
    world
        .get_resource_or_insert_with(Movements::default)
        .into_inner()
        .0
        .entry(id)
        .or_default()
}

fn actor(world: &World, receiver: &Value) -> Result<u64, String> {
    match world.resource::<RoundScript>().entity(receiver) {
        Some((id, entity)) if entity.kind == EntityKind::Actor => Ok(id),
        _ => Err("not an actor".into()),
    }
}

fn number(world: &mut World, id: u64, name: &str, default: f32) -> f32 {
    match world.resource_mut::<RoundScript>().object_field(id, name) {
        Value::Int(n) => n as f32,
        Value::Float(f) => f,
        _ => default,
    }
}

fn origin_of(world: &mut World, id: u64) -> [f32; 3] {
    match world.resource_mut::<RoundScript>().object_field(id, "origin") {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    }
}

fn alive(world: &mut World, id: u64) -> bool {
    number(world, id, "health", 0.0) > 0.0
}

fn flat_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// The goal's test: within its radius across and its height up or down.
fn in_goal(world: &mut World, id: u64, at: [f32; 3], goal: [f32; 3]) -> bool {
    let radius = number(world, id, "goalradius", GOAL_RADIUS);
    let height = number(world, id, "goalheight", GOAL_HEIGHT);
    (at[2] - goal[2]).abs() <= height && flat_distance(at, goal) <= radius
}

fn hull(world: &World, id: u64) -> ([f32; 3], [f32; 3]) {
    let (radius, _, top) = world
        .get_resource::<super::actor::Actors>()
        .and_then(|actors| actors.0.get(&id))
        .map_or((15.0, 0.0, 72.0), |actor| actor.hull);
    ([-radius, -radius, STEP_HEIGHT], [radius, radius, top.max(STEP_HEIGHT + 1.0)])
}

fn clear(world: &mut World, id: u64, from: [f32; 3], to: [f32; 3]) -> bool {
    let (mins, maxs) = hull(world, id);
    trace(world, from, to, mins, maxs, ACTOR_MASK).fraction >= 1.0
}

/// The node nearest `at` within 192 units, else within 512, that a body
/// can walk to from there.
fn nearest_node(world: &mut World, id: u64, at: [f32; 3]) -> Option<u16> {
    let paths = FrameWorld::from_world(world).actor_paths()?;
    for radius in [NODE_NEAR, NODE_FAR] {
        let mut near: Vec<(f32, u16)> = paths
            .nodes()
            .iter()
            .enumerate()
            .filter(|(_, node)| !node.links.is_empty())
            .map(|(index, node)| {
                let d: f32 = (0..3).map(|i| (node.origin[i] - at[i]).powi(2)).sum();
                (d, index as u16)
            })
            .filter(|(d, _)| *d <= radius * radius)
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, index) in near {
            if clear(world, id, at, paths.nodes()[index as usize].origin) {
                return Some(index);
            }
        }
    }
    None
}

/// A straight walk when nothing is in the way, else the path over the
/// nodes; none when no route joins.
fn plan(world: &mut World, id: u64, from: [f32; 3], goal: [f32; 3]) -> Option<Vec<[f32; 3]>> {
    if clear(world, id, from, goal) {
        return Some(vec![goal]);
    }
    let paths = FrameWorld::from_world(world).actor_paths()?;
    let start = nearest_node(world, id, from)?;
    let end = nearest_node(world, id, goal)?;
    let route = paths.route(start, end)?;
    let mut points: Vec<[f32; 3]> = route
        .iter()
        .map(|&node| paths.nodes()[node as usize].origin)
        .collect();
    points.push(goal);
    Some(points)
}

/// Starts an animscript's main as the engine switches scripts; the one it
/// leaves is told `killanimscript` (`t6.actors.state_machine`).
fn set_script(world: &mut World, id: u64, script: Script) -> Result<(), String> {
    if movement(world, id).script == Some(script) {
        return Ok(());
    }
    movement(world, id).script = Some(script);
    anim::reset_orient(world, id);
    let prefix = if matches!(
        world.resource_mut::<RoundScript>().object_field(id, "isdog"),
        Value::Int(1)
    ) {
        "zm_dog_"
    } else {
        "zm_"
    };
    raise(world, Value::Object(id), "killanimscript", Vec::new());
    let main = format!("{ANIMSCRIPTS}/{prefix}{}::main", script.name());
    let now = super::super::super::players::now_ms(world);
    run_now(world, &main, Value::Object(id), Vec::new(), now)
        .map_err(|fault| format!("{main}: {fault:?}"))
}

/// The enemy an actor fights: its favorite enemy, while it lives and the
/// actor does not ignore everyone (`t6.actors.enemy`).
fn enemy(world: &mut World, id: u64) -> Option<Value> {
    let mut runtime = world.resource_mut::<RoundScript>();
    let ignore = matches!(runtime.object_field(id, "ignoreall"), Value::Int(n) if n != 0);
    let favorite = runtime.object_field(id, "favoriteenemy");
    if ignore || !matches!(favorite, Value::Object(_)) {
        return None;
    }
    drop(runtime);
    let other = entity_id(world, &favorite).ok()?;
    alive(world, other).then_some(favorite)
}

fn turn(yaw: f32, toward: f32, step: f32) -> f32 {
    let delta = (toward - yaw + 540.0).rem_euclid(360.0) - 180.0;
    yaw + delta.clamp(-step, step)
}

fn set_yaw(world: &mut World, id: u64, toward: f32) {
    let mut runtime = world.resource_mut::<RoundScript>();
    let angles = match runtime.object_field(id, "angles") {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    };
    let seconds = crate::MATCH_TICK_MS as f32 / 1000.0;
    let yaw = turn(angles[1], toward, TURN_RATE * seconds);
    runtime.set_object_field(id, "angles", Value::Vector([angles[0], yaw, angles[2]]));
}

/// Walks the actor along its path as far as its clip's root moved this
/// frame, then onto the floor.
fn walk(world: &mut World, id: u64, from: [f32; 3]) {
    let anim = anim::get(world, id);
    let moves = !matches!(
        anim.mode,
        "none" | "zonly_physics" | "angle deltas" | "nophysics" | "point relative"
    );
    let mut left = if moves {
        (anim.step[0].powi(2) + anim.step[1].powi(2)).sqrt()
    } else {
        0.0
    };
    let mut at = from;
    let mut heading = None;
    let path = movement(world, id).path.clone();
    let mut passed = 0;
    while let Some(&point) = path.get(passed) {
        let gap = flat_distance(at, point);
        if gap > 0.01 {
            heading = Some((point[1] - at[1]).atan2(point[0] - at[0]).to_degrees());
        }
        let last = passed + 1 == path.len();
        if !last && gap <= POINT_REACHED {
            passed += 1;
            continue;
        }
        let along = left.min(gap);
        if gap > 0.0 {
            at[0] += (point[0] - at[0]) / gap * along;
            at[1] += (point[1] - at[1]) / gap * along;
        }
        left -= along;
        if along < gap || last {
            break;
        }
        passed += 1;
    }
    movement(world, id).path.drain(..passed);
    let lifted = [from[0], from[1], from[2] + 1.0];
    let target = [at[0], at[1], from[2] + 1.0];
    let (mins, maxs) = hull(world, id);
    let hit = trace(world, lifted, target, mins, maxs, ACTOR_MASK);
    let mut landed: [f32; 3] =
        std::array::from_fn(|i| lifted[i] + (target[i] - lifted[i]) * hit.fraction);
    let seconds = crate::MATCH_TICK_MS as f32 / 1000.0;
    let fall = movement(world, id).falling;
    let below = [landed[0], landed[1], landed[2] - STEP_HEIGHT - fall * seconds];
    let feet = ([mins[0], mins[1], 0.0], [maxs[0], maxs[1], maxs[2] - STEP_HEIGHT]);
    let ground = trace(world, landed, below, feet.0, feet.1, ACTOR_MASK);
    landed[2] += (below[2] - landed[2]) * ground.fraction;
    movement(world, id).falling = if ground.fraction < 1.0 {
        0.0
    } else {
        fall + GRAVITY * seconds
    };
    world
        .resource_mut::<RoundScript>()
        .set_object_field(id, "origin", Value::Vector(landed));
    if let Some(yaw) = heading
        && matches!(anim.orient, "face default" | "face motion" | "face enemy or motion")
    {
        set_yaw(world, id, yaw);
    }
}

/// Faces what the actor's orient mode names when it is not walking.
fn face(world: &mut World, id: u64, at: [f32; 3], enemy: Option<&Value>) {
    let anim = anim::get(world, id);
    let toward = match anim.orient {
        "face angle" | "face direction" | "face point" | "face current" => anim.orient_yaw,
        "face enemy" | "face enemy or motion" | "face default" => enemy.and_then(|enemy| {
            let other = entity_id(world, enemy).ok()?;
            let there = origin_of(world, other);
            Some((there[1] - at[1]).atan2(there[0] - at[0]).to_degrees())
        }),
        _ => None,
    };
    if let Some(yaw) = toward {
        set_yaw(world, id, yaw);
    }
}

fn think_one(world: &mut World, id: u64) -> Result<(), String> {
    if !alive(world, id) || anim::scripted(world, id) {
        return Ok(());
    }
    let at = origin_of(world, id);
    let goal = *movement(world, id).goal.get_or_insert(at);
    if !movement(world, id).planned {
        movement(world, id).planned = true;
        let path = if in_goal(world, id, at, goal) {
            Some(Vec::new())
        } else {
            plan(world, id, at, goal)
        };
        match path {
            Some(path) => movement(world, id).path = path,
            None => {
                movement(world, id).path.clear();
                raise(world, Value::Object(id), "bad_path", Vec::new());
            }
        }
    }
    let enemy = enemy(world, id);
    world.resource_mut::<RoundScript>().set_object_field(
        id,
        "enemy",
        enemy.clone().unwrap_or(Value::Undefined),
    );
    let standing = if enemy.is_some() {
        Script::Combat
    } else {
        Script::Stop
    };
    let script = if movement(world, id).path.is_empty() {
        standing
    } else {
        Script::Move
    };
    set_script(world, id, script)?;
    if script == Script::Move {
        walk(world, id, at);
    } else {
        face(world, id, at, enemy.as_ref());
    }
    let now = origin_of(world, id);
    if in_goal(world, id, now, goal) && !movement(world, id).goal_sent {
        let state = movement(world, id);
        state.goal_sent = true;
        state.path.clear();
        raise(world, Value::Object(id), "goal", Vec::new());
    }
    Ok(())
}

/// Every server frame: each living actor's path, animscript and walk.
fn think(world: &mut World) {
    let actors: Vec<u64> = world
        .resource::<RoundScript>()
        .entities
        .iter()
        .filter(|(_, entity)| entity.kind == EntityKind::Actor)
        .map(|(id, _)| *id)
        .collect();
    if let Some(mut movements) = world.get_resource_mut::<Movements>() {
        movements.0.retain(|id, _| actors.contains(id));
    }
    for id in actors {
        if let Err(fault) = think_one(world, id) {
            diag::warn!(Sim, "actor {id}: {fault}");
        }
    }
}

fn set_goal(world: &mut World, id: u64, goal: [f32; 3]) {
    let state = movement(world, id);
    if state.goal == Some(goal) {
        return;
    }
    state.goal = Some(goal);
    state.goal_sent = false;
    state.planned = false;
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::Method;

    registry.on_tick(think);
    registry.register(Method, "setgoalpos", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let goal = vector(args, 0)?;
        if args.len() == 2 {
            vector(args, 1)?;
        }
        set_goal(world, id, goal);
        Ok(Value::Undefined)
    });
    registry.register(Method, "setgoalnode", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let node = args.first().ok_or("setgoalnode expects a node")?;
        let goal = match node {
            Value::Object(object) => match world.resource_mut::<RoundScript>().object_field(*object, "origin") {
                Value::Vector(v) => v,
                _ => return Err("not a node".into()),
            },
            _ => return Err("not a node".into()),
        };
        set_goal(world, id, goal);
        Ok(Value::Undefined)
    });
    registry.register(Method, "isingoal", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let point = vector(args, 0)?;
        let at = origin_of(world, id);
        let goal = *movement(world, id).goal.get_or_insert(at);
        Ok(Value::Int(in_goal(world, id, point, goal).into()))
    });
}
