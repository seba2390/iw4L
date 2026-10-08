use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap};

use bevy_ecs::prelude::World;

use super::args::{arg, float, string, vector};
use super::entities::EntityKind;
use crate::bullet_collision::MASK_PLAYER_SOLID;
use crate::frame::FrameWorld;
use crate::script::runtime::raise;
use crate::script::{Namespace, NativeRegistry, Runtime, Value};

/// A path node as actors use it.
#[derive(Clone, Debug)]
pub struct NavNode {
    pub kind: NavNodeKind,
    pub origin: [f32; 3],
    pub yaw: f32,
    pub targetname: String,
    pub target: String,
    pub animscript: String,
    /// Linked node, link length, and whether crossing it is a traversal.
    pub links: Vec<(u16, f32, bool)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavNodeKind {
    Path,
    NegotiationBegin,
    NegotiationEnd,
    Other,
}

impl NavNodeKind {
    fn script_type(self) -> &'static str {
        match self {
            Self::Path => "Path",
            Self::NegotiationBegin => "Begin",
            Self::NegotiationEnd => "End",
            Self::Other => "Other",
        }
    }
}

/// The map's authored path network.
#[derive(Debug, Default)]
pub struct ActorPaths {
    nodes: Vec<NavNode>,
}

/// Nodes farther above or below a position than this do not serve it.
const NODE_STEP_HEIGHT: f32 = 72.0;

impl ActorPaths {
    pub fn new(nodes: Vec<NavNode>) -> Self {
        Self { nodes }
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    fn nearest(&self, at: [f32; 3]) -> Option<u16> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| (node.origin[2] - at[2]).abs() <= NODE_STEP_HEIGHT)
            .min_by(|(_, a), (_, b)| {
                distance_sq(a.origin, at).total_cmp(&distance_sq(b.origin, at))
            })
            .map(|(index, _)| index as u16)
    }

    /// The node sequence from the node serving `from` to the one serving `to`.
    fn plan(&self, from: [f32; 3], to: [f32; 3]) -> Option<Vec<u16>> {
        let start = self.nearest(from)?;
        let goal = self.nearest(to)?;
        let goal_origin = self.nodes[goal as usize].origin;
        let mut best: BTreeMap<u16, (f32, Option<u16>)> = BTreeMap::new();
        best.insert(start, (0.0, None));
        let mut open = BinaryHeap::new();
        open.push(Frontier {
            estimate: distance_sq(self.nodes[start as usize].origin, goal_origin).sqrt(),
            node: start,
        });
        while let Some(Frontier { node, .. }) = open.pop() {
            if node == goal {
                let mut path = vec![goal];
                let mut cursor = goal;
                while let Some(&(_, Some(previous))) = best.get(&cursor) {
                    path.push(previous);
                    cursor = previous;
                }
                path.reverse();
                return Some(path);
            }
            let cost = best[&node].0;
            for &(next, length, _) in &self.nodes[node as usize].links {
                let reached = cost + length.max(1.0);
                if best.get(&next).is_some_and(|(known, _)| *known <= reached) {
                    continue;
                }
                best.insert(next, (reached, Some(node)));
                open.push(Frontier {
                    estimate: reached
                        + distance_sq(self.nodes[next as usize].origin, goal_origin).sqrt(),
                    node: next,
                });
            }
        }
        None
    }
}

#[derive(PartialEq)]
struct Frontier {
    estimate: f32,
    node: u16,
}

impl Eq for Frontier {}

impl Ord for Frontier {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .estimate
            .total_cmp(&self.estimate)
            .then(other.node.cmp(&self.node))
    }
}

impl PartialOrd for Frontier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// How an actor's animation moves it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AnimMode {
    /// The engine walks the actor along its path at the animation's speed.
    #[default]
    Normal,
    /// The animation moves the actor; gravity keeps it on the ground.
    Gravity,
    /// The animation moves the actor horizontally; the ground sets its height.
    ZOnlyPhysics,
    /// The animation moves the actor freely.
    NoGravity,
    /// Only the animation's turn applies.
    AngleDeltas,
    None,
}

/// What an actor faces.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum Orient {
    #[default]
    Motion,
    Enemy,
    Angle(f32),
    Point([f32; 3]),
    Current,
}

#[derive(Clone, Debug)]
pub(crate) enum Goal {
    Position([f32; 3]),
    Entity(u64),
}

/// An actor's goal, path and movement modes.
#[derive(Clone, Debug, Default)]
pub(crate) struct ActorMove {
    goal: Option<Goal>,
    /// Where the path was planned to; an entity goal replans when it moves.
    planned_to: Option<[f32; 3]>,
    waypoints: Vec<[f32; 3]>,
    pub(crate) anim_mode: AnimMode,
    pub(crate) orient: Orient,
    /// Stances a script allows; none recorded means all.
    stances: Option<Vec<String>>,
    /// Direction of travel over the last tick, in degrees.
    heading: Option<f32>,
}

impl ActorMove {
    pub(crate) fn has_path(&self) -> bool {
        !self.waypoints.is_empty()
    }
}

/// A goal entity that moves farther than this is chased with a new path.
const REPLAN_DISTANCE: f32 = 64.0;
/// A waypoint closer than this is passed.
const WAYPOINT_REACHED: f32 = 16.0;
/// When a script names no radius.
const DEFAULT_GOAL_RADIUS: f32 = 32.0;
/// Degrees an actor turns per second toward what it faces.
const TURN_RATE: f32 = 360.0;
/// The actor's collision box for ground traces.
const ACTOR_MINS: [f32; 3] = [-15.0, -15.0, 0.0];
const ACTOR_MAXS: [f32; 3] = [15.0, 15.0, 18.0];
const GROUND_PROBE_UP: f32 = 48.0;
const GROUND_PROBE_DOWN: f32 = 64.0;

pub(crate) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};
    registry.register(Function, "getnode", |world, _, args| {
        let found = find_nodes(world, args)?;
        Ok(found.into_iter().next().unwrap_or(Value::Undefined))
    });
    registry.register(Function, "getnodearray", |world, _, args| {
        let found = find_nodes(world, args)?;
        super::arrays::new_array(world, found)
    });
    registry.register(Method, "setgoalpos", |world, receiver, args| {
        let goal = vector(args, 0)?;
        set_goal(world, receiver, Goal::Position(goal))
    });
    registry.register(Method, "setgoalnode", |world, receiver, args| {
        let node = arg(args, 0)?.clone();
        let Value::Object(id) = node else {
            return Err("expects a path node".into());
        };
        let origin = match world.resource_mut::<Runtime>().object_field(id, "origin") {
            Value::Vector(origin) => origin,
            _ => return Err("path node has no origin".into()),
        };
        set_goal(world, receiver, Goal::Position(origin))
    });
    registry.register(Method, "setgoalentity", |world, receiver, args| {
        let target = super::natives::engine::entity_id(world, arg(args, 0)?)?;
        set_goal(world, receiver, Goal::Entity(target))
    });
    registry.register(Method, "allowedstances", |world, receiver, args| {
        let stances = (0..args.len())
            .map(|index| string(args, index))
            .collect::<Result<Vec<_>, _>>()?;
        with_move(world, receiver, |movement| movement.stances = Some(stances))
    });
    registry.register(Method, "isstanceallowed", |world, receiver, args| {
        let stance = string(args, 0)?;
        let id = actor_id(world, receiver)?;
        let allowed = world
            .resource::<Runtime>()
            .actor_moves
            .get(&id)
            .and_then(|movement| movement.stances.as_ref())
            .is_none_or(|stances| stances.iter().any(|s| *s == stance));
        Ok(Value::Int(allowed.into()))
    });
    registry.register(Method, "getmotionangle", |world, receiver, _| {
        let id = actor_id(world, receiver)?;
        let (_, angles) = pose(&mut world.resource_mut::<Runtime>(), id);
        let heading = world
            .resource::<Runtime>()
            .actor_moves
            .get(&id)
            .and_then(|movement| movement.heading);
        Ok(Value::Float(heading.map_or(0.0, |heading| {
            math_iw4::angle_subtract(heading, angles[1])
        })))
    });
    registry.register(Method, "getclosestenemysqdist", |world, receiver, _| {
        let id = actor_id(world, receiver)?;
        let (origin, _) = pose(&mut world.resource_mut::<Runtime>(), id);
        Ok(Value::Float(
            enemy_position(world, id).map_or(f32::MAX, |enemy| distance_sq(origin, enemy)),
        ))
    });
    registry.register(Method, "animmode", |world, receiver, args| {
        let mode = match string(args, 0)?.as_str() {
            "normal" => AnimMode::Normal,
            "gravity" => AnimMode::Gravity,
            "zonly_physics" => AnimMode::ZOnlyPhysics,
            "nogravity" | "noclip" => AnimMode::NoGravity,
            "angle deltas" => AnimMode::AngleDeltas,
            "none" => AnimMode::None,
            other => return Err(format!("unknown animmode '{other}'")),
        };
        with_move(world, receiver, |movement| movement.anim_mode = mode)
    });
    registry.register(Method, "orientmode", |world, receiver, args| {
        let orient = match string(args, 0)?.as_str() {
            "face motion" | "face default" => Orient::Motion,
            "face enemy" | "face enemy or motion" => Orient::Enemy,
            "face angle" => Orient::Angle(float(args, 1)?),
            "face point" => Orient::Point(vector(args, 1)?),
            "face current" => Orient::Current,
            other => return Err(format!("unknown orientmode '{other}'")),
        };
        with_move(world, receiver, |movement| movement.orient = orient)
    });
}

fn actor_id(world: &World, receiver: &Value) -> Result<u64, String> {
    match world.resource::<Runtime>().entity(receiver) {
        Some((id, entity)) if entity.kind == EntityKind::Actor => Ok(id),
        _ => Err("receiver is not an actor".into()),
    }
}

fn with_move(
    world: &mut World,
    receiver: &Value,
    apply: impl FnOnce(&mut ActorMove),
) -> Result<Value, String> {
    let id = actor_id(world, receiver)?;
    apply(
        world
            .resource_mut::<Runtime>()
            .actor_moves
            .entry(id)
            .or_default(),
    );
    Ok(Value::Undefined)
}

fn set_goal(world: &mut World, receiver: &Value, goal: Goal) -> Result<Value, String> {
    let id = actor_id(world, receiver)?;
    let mut runtime = world.resource_mut::<Runtime>();
    let movement = runtime.actor_moves.entry(id).or_default();
    movement.goal = Some(goal);
    movement.planned_to = None;
    movement.waypoints.clear();
    Ok(Value::Undefined)
}

/// `GetNode(value, key)` and `GetNodeArray(value, key)`: nodes whose key matches.
fn find_nodes(world: &mut World, args: &[Value]) -> Result<Vec<Value>, String> {
    let value = string(args, 0)?;
    let key = string(args, 1)?;
    let Some(paths) = FrameWorld::from_world(world).actor_paths() else {
        return Ok(Vec::new());
    };
    let matches: Vec<u16> = (0..paths.nodes.len())
        .filter(|&index| {
            let node = &paths.nodes[index];
            match key.as_str() {
                "targetname" => node.targetname == value,
                "target" => node.target == value,
                "animscript" => node.animscript == value,
                _ => false,
            }
        })
        .map(|index| index as u16)
        .collect();
    matches
        .into_iter()
        .map(|index| node_object(world, &paths, index))
        .collect()
}

/// The script object standing for a path node, made on first use.
fn node_object(world: &mut World, paths: &ActorPaths, index: u16) -> Result<Value, String> {
    let mut runtime = world.resource_mut::<Runtime>();
    if let Some(&id) = runtime.path_node_objects.get(&index)
        && runtime.live(&id)
    {
        return Ok(Value::Object(id));
    }
    let node = &paths.nodes[index as usize];
    let id = runtime.new_object()?;
    runtime.set_object_field(id, "origin", Value::Vector(node.origin));
    runtime.set_object_field(id, "angles", Value::Vector([0.0, node.yaw, 0.0]));
    runtime.set_object_field(id, "type", Value::string(node.kind.script_type()));
    for (name, text) in [
        ("targetname", &node.targetname),
        ("target", &node.target),
        ("animscript", &node.animscript),
    ] {
        if !text.is_empty() {
            runtime.set_object_field(id, name, Value::string(text));
        }
    }
    runtime.path_node_objects.insert(index, id);
    Ok(Value::Object(id))
}

fn pose(runtime: &mut Runtime, id: u64) -> ([f32; 3], [f32; 3]) {
    let vector = |runtime: &mut Runtime, name| match runtime.object_field(id, name) {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    };
    (vector(runtime, "origin"), vector(runtime, "angles"))
}

/// Plans paths for new goals, walks each actor by its animation and reports
/// `goal` and `bad_path`.
pub(crate) fn locomote(world: &mut World, seconds: f32) {
    let actors: Vec<u64> = world
        .resource::<Runtime>()
        .actor_moves
        .keys()
        .copied()
        .collect();
    let paths = FrameWorld::from_world(world).actor_paths();
    for actor in actors {
        if !world.resource::<Runtime>().entities.contains_key(&actor) {
            world.resource_mut::<Runtime>().actor_moves.remove(&actor);
            continue;
        }
        let (mut origin, mut angles) = pose(&mut world.resource_mut::<Runtime>(), actor);
        let goal_radius = match world
            .resource_mut::<Runtime>()
            .object_field(actor, "goalradius")
        {
            Value::Int(n) => n as f32,
            Value::Float(f) => f,
            _ => DEFAULT_GOAL_RADIUS,
        };
        let goal_at = goal_position(world, actor);
        let mut movement = world.resource::<Runtime>().actor_moves[&actor].clone();
        let mut events: Vec<&'static str> = Vec::new();

        if let Some(goal_at) = goal_at {
            let stale = movement.planned_to.is_none_or(|planned| {
                distance_sq(planned, goal_at) > REPLAN_DISTANCE * REPLAN_DISTANCE
            });
            if stale {
                movement.planned_to = Some(goal_at);
                movement.waypoints.clear();
                if horizontal_distance(origin, goal_at) > goal_radius {
                    match paths
                        .as_deref()
                        .and_then(|paths| paths.plan(origin, goal_at))
                    {
                        Some(nodes) => {
                            let paths = paths.as_deref().unwrap();
                            movement.waypoints = nodes
                                .iter()
                                .map(|&node| paths.nodes[node as usize].origin)
                                .collect();
                            movement.waypoints.push(goal_at);
                        }
                        None => events.push("bad_path"),
                    }
                }
            }
        }

        let delta = super::actor_anims::root_delta(world, actor);
        let (local, turned) = delta.unwrap_or(([0.0; 3], 0.0));
        let (sin, cos) = angles[1].to_radians().sin_cos();
        let world_delta = [
            local[0] * cos - local[1] * sin,
            local[0] * sin + local[1] * cos,
            local[2],
        ];
        let mut heading = None;
        match movement.anim_mode {
            AnimMode::Normal => {
                while movement
                    .waypoints
                    .first()
                    .is_some_and(|&next| horizontal_distance(origin, next) < WAYPOINT_REACHED)
                    && movement.waypoints.len() > 1
                {
                    movement.waypoints.remove(0);
                }
                if let Some(&next) = movement.waypoints.first() {
                    let step = (world_delta[0].powi(2) + world_delta[1].powi(2)).sqrt();
                    let to = [next[0] - origin[0], next[1] - origin[1]];
                    let length = (to[0] * to[0] + to[1] * to[1]).sqrt();
                    if length > 0.001 {
                        let advance = step.min(length);
                        origin[0] += to[0] / length * advance;
                        origin[1] += to[1] / length * advance;
                        heading = Some(to[1].atan2(to[0]).to_degrees());
                    }
                    origin = ground(world, origin);
                }
            }
            AnimMode::Gravity | AnimMode::ZOnlyPhysics => {
                origin[0] += world_delta[0];
                origin[1] += world_delta[1];
                origin = ground(world, origin);
                angles[1] += turned;
            }
            AnimMode::NoGravity => {
                for axis in 0..3 {
                    origin[axis] += world_delta[axis];
                }
                angles[1] += turned;
            }
            AnimMode::AngleDeltas => angles[1] += turned,
            AnimMode::None => {}
        }

        movement.heading = heading;
        let facing = match &movement.orient {
            Orient::Motion => heading,
            Orient::Angle(yaw) => Some(*yaw),
            Orient::Point(point) => Some(yaw_to(origin, *point)),
            Orient::Enemy => enemy_position(world, actor)
                .map(|enemy| yaw_to(origin, enemy))
                .or(heading),
            Orient::Current => None,
        };
        if let Some(wanted) = facing {
            let turn = math_iw4::angle_subtract(wanted, angles[1]);
            let limit = TURN_RATE * seconds;
            angles[1] += turn.clamp(-limit, limit);
        }

        if let Some(goal_at) = goal_at
            && horizontal_distance(origin, goal_at) <= goal_radius
            && movement.planned_to.is_some()
            && (movement.waypoints.len() <= 1)
        {
            if movement.has_path() || !matches!(movement.goal, Some(Goal::Entity(_))) {
                events.push("goal");
            }
            movement.waypoints.clear();
            if matches!(movement.goal, Some(Goal::Position(_))) {
                movement.goal = None;
            }
        }

        {
            let mut runtime = world.resource_mut::<Runtime>();
            runtime.set_object_field(actor, "origin", Value::Vector(origin));
            runtime.set_object_field(actor, "angles", Value::Vector(angles));
            runtime.actor_moves.insert(actor, movement);
        }
        for event in events {
            raise(world, Value::Object(actor), event, Vec::new());
        }
    }
}

fn goal_position(world: &mut World, actor: u64) -> Option<[f32; 3]> {
    let goal = world
        .resource::<Runtime>()
        .actor_moves
        .get(&actor)?
        .goal
        .clone()?;
    match goal {
        Goal::Position(at) => Some(at),
        Goal::Entity(id) => as_vector(super::players::entity_field(world, id, "origin")),
    }
}

fn enemy_position(world: &mut World, actor: u64) -> Option<[f32; 3]> {
    let enemy = match world
        .resource_mut::<Runtime>()
        .object_field(actor, "favoriteenemy")
    {
        Value::Object(id) => id,
        _ => return None,
    };
    as_vector(super::players::entity_field(world, enemy, "origin"))
}

/// The floor under an actor at `at`, or `at` when there is none in reach.
fn ground(world: &mut World, at: [f32; 3]) -> [f32; 3] {
    let frame = FrameWorld::from_world(world);
    let start = [at[0], at[1], at[2] + GROUND_PROBE_UP];
    let end = [at[0], at[1], at[2] - GROUND_PROBE_DOWN];
    let trace = frame.trace_clip(start, end, ACTOR_MINS, ACTOR_MAXS, MASK_PLAYER_SOLID);
    if trace.startsolid != 0 || trace.fraction >= 1.0 {
        return at;
    }
    [
        at[0],
        at[1],
        start[2] + (end[2] - start[2]) * trace.fraction,
    ]
}

fn yaw_to(from: [f32; 3], to: [f32; 3]) -> f32 {
    (to[1] - from[1]).atan2(to[0] - from[0]).to_degrees()
}

fn horizontal_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn distance_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

pub(crate) fn melee_range_enemy(world: &mut World, actor: u64, range: f32) -> bool {
    let Some(enemy) = enemy_position(world, actor) else {
        return false;
    };
    let (origin, _) = pose(&mut world.resource_mut::<Runtime>(), actor);
    horizontal_distance(origin, enemy) <= range && (origin[2] - enemy[2]).abs() <= NODE_STEP_HEIGHT
}

fn as_vector(value: Value) -> Option<[f32; 3]> {
    match value {
        Value::Vector(v) => Some(v),
        _ => None,
    }
}
