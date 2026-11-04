//! Black Ops 2's actor movement: the goal scripts give an actor, the path
//! planned to it over the map's path nodes, the animscript the engine runs
//! for it (move, combat, or a negotiation node's traversal), and the
//! walk its clips' root motion drives along the path.

use super::super::super::actor_nav::{LinkEdits, NavNodeKind, node_value};
use super::super::super::entities::EntityKind;
use super::super::engine::{entity_id, trace};
use super::path_follow::{Follow, Lookahead, Probe};
use super::team_move::{self, Friend, Mover, Slowed, TeamMove};
use super::{anim, vector};
use crate::frame::FrameWorld;
use crate::script::runtime::{notify_now, raise, start_function_now, start_now, thread_alive};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

/// The goal an actor starts with: where it spawned, 4 across and 80 high.
const GOAL_RADIUS: f32 = 4.0;
const GOAL_HEIGHT: f32 = 80.0;
/// How far the engine looks for the node nearest a position, then further.
pub(super) const NODE_NEAR: f32 = 192.0;
const NODE_FAR: f32 = 512.0;
pub(super) const ACTOR_MASK: u32 = 0x0282_0011;
/// The code move's step: the clip's root-motion distance, scaled by the
/// lookahead's flat share to the power 0.333 (at least 0.6), and no further
/// than the lookahead.
const MIN_FLAT_SQ: f32 = 0.000_001;
const SPEED_EXPONENT: f32 = 0.333;
const MIN_SPEED_SCALE: f32 = 0.6;
/// IW4L's walking rules (`t6.actors.path_following`).
const POINT_REACHED: f32 = 16.0;
const TURN_RATE: f32 = 360.0;
const STEP_HEIGHT: f32 = 18.0;
const GRAVITY: f32 = 800.0;
const NODE_DROP: f32 = 128.0;
const SLIDES: usize = 3;
/// `maymovetopoint`'s probe: up and down from the point, the extra
/// contents it stops at, and the deepest drop it allows.
const PROBE_HEIGHT: f32 = 48.0;
const PROBE_MASK: u32 = 0xa000;
const MAX_DROP: f32 = 32.0;
const ANIMSCRIPTS: &str = "maps/mp/animscripts";
/// The engine's arrival: an actor within a quarter second of its path's end
/// (at its speed; 0.25² here) has 500 ms left to reach it, taking the share
/// of what is left that a 50 ms frame gives, and its choice runs the stop
/// script for the last 200.
const ARRIVAL_SPEED_SQ: f32 = 0.0625;
const ARRIVAL_MS: i64 = 500;
const ARRIVAL_STOP_MS: i64 = 200;
const FRAME_MS: f32 = 50.0;
/// How far around an enemy a path that would pass it is dropped for combat
/// is the actor's `pathenemyfightdist`; the wait that follows is the team
/// move's.
const PATH_DROP_WAIT_MS: i64 = 500;
/// A wait on a friend that is itself piled up on another.
const PILE_CHAIN_WAIT_MS: i64 = 50;
/// Piling up: how close another actor counts as touching, how far one holds
/// it, how far up or down one counts, and the move the actor makes when its
/// clip has none (all squared).
const PILE_TOUCH_SQ: f32 = 900.0;
const PILE_HOLD_SQ: f32 = 1_406.25;
const PILE_HEIGHT_SQ: f32 = 6_400.0;
const PILE_TOUCH_HEIGHT: f32 = 80.0;
const PILE_MIN_MOVE_SQ: f32 = 2.25;
const PILE_MOVED_SHARE: f32 = 0.25;
/// A path request counts a path ending this near its point (squared) as there.
const POINT_NEAR_SQ: f32 = 4.0;
/// How fast a species counts itself when it stands still: 190 for a zombie,
/// 300 for a dog.
const ZOMBIE_SPEED: f32 = 190.0;
const DOG_SPEED: f32 = 300.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Script {
    Move,
    Stop,
    Combat,
    /// The traversal its negotiation begin node names.
    Traverse(u16),
    Pain,
    Death,
}

/// A point of the path, and the node it is when it is one.
#[derive(Clone, Copy, Debug)]
struct Waypoint {
    at: [f32; 3],
    node: Option<u16>,
}

/// `traversemode`: whether the body falls and whether it collides.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum TraverseMode {
    Gravity,
    /// An actor's mode until a script sets one, as the engine makes it.
    #[default]
    NoGravity,
    NoClip,
}

/// A negotiation link being crossed by its traversal script.
#[derive(Clone, Copy, Debug)]
struct Traversal {
    begin: u16,
    end: u16,
    thread: Option<u64>,
}

#[derive(Clone, Debug, Default)]
struct Movement {
    goal: Option<[f32; 3]>,
    /// The path to the goal; planned again when the goal changes.
    planned: bool,
    path: Vec<Waypoint>,
    /// The path as the engine follows it, and the lookahead it keeps
    /// across paths.
    follow: Option<Follow>,
    look: Lookahead,
    script: Option<Script>,
    falling: f32,
    /// The walk made no headway on its last frame.
    stalled: bool,
    /// How fast the last walk moved it across, in units a second.
    speed: f32,
    /// When its arrival ends; held until then, a new path or its goal.
    arrival: Option<i64>,
    /// Where its last move took it and where it meant to go (a frame's), and
    /// whether it ran into a player.
    velocity: [f32; 2],
    wish: [f32; 2],
    hit_sentient: bool,
    /// The engine's move mode: 0 stop, 1 stopping, 2 walk, 3 run.
    move_mode: u8,
    /// The team move's wait, the actor it piled up on, and the friend it
    /// slows behind.
    wait_until: i64,
    pile: Option<u64>,
    slowed: Slowed,
    /// The end its path was planned to, kept while it stays in the goal.
    path_goal: Option<[f32; 3]>,
    /// `pushplayer(1)`: its moves pass through players' bodies.
    through_bodies: bool,
    traverse_mode: TraverseMode,
    traversal: Option<Traversal>,
    /// The pain script while it runs; the engine's choice waits for it.
    pain: Option<u64>,
    /// `animcustom(func, [end_func])`: the running custom thread and its
    /// optional end function.
    custom: Option<(Option<u64>, Option<usize>)>,
    /// The death animscript's thread, while it runs.
    death: Option<u64>,
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

pub(super) fn actor(world: &World, receiver: &Value) -> Result<u64, String> {
    match world.resource::<RoundScript>().entity(receiver) {
        Some((id, entity)) if entity.kind == EntityKind::Actor => Ok(id),
        _ => Err("not an actor".into()),
    }
}

/// A field as script reads it: a player's from its player state.
pub(super) fn field(world: &mut World, id: u64, name: &str) -> Value {
    if let Some(client) = world.resource::<RoundScript>().player_client(id)
        && let Some(value) = super::super::super::players::load_field(world, client, name)
    {
        return value;
    }
    world.resource_mut::<RoundScript>().object_field(id, name)
}

fn number(world: &mut World, id: u64, name: &str, default: f32) -> f32 {
    match field(world, id, name) {
        Value::Int(n) => n as f32,
        Value::Float(f) => f,
        _ => default,
    }
}

pub(super) fn origin_of(world: &mut World, id: u64) -> [f32; 3] {
    match field(world, id, "origin") {
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

/// The engine's pain state (`Actor_Pain` / `AIS_PAIN`): ignored while in
/// `AIS_SCRIPTEDANIM` or `AIS_CUSTOMANIM` (whose state table `pain` callback
/// returns `0`), otherwise starts the pain animscript and holds the actor's
/// state choice until that thread ends.
pub(super) fn pain(world: &mut World, id: u64) -> Result<(), String> {
    if anim::scripted(world, id) || in_custom(world, id) {
        return Ok(());
    }
    let thread = set_script(world, id, Script::Pain)?;
    movement(world, id).pain = thread;
    Ok(())
}

pub(super) fn enter_scripted(world: &mut World, id: u64) {
    movement(world, id).script = None;
    record_script_state(world, id, "scripted", "scripted");
}

fn record_script_state(world: &mut World, id: u64, state: &str, reason: &str) {
    let mut runtime = world.resource_mut::<RoundScript>();
    let prev = runtime.object_field(id, "scriptstate");
    if matches!(prev, Value::String(_)) {
        runtime.set_object_field(id, "lastscriptstate", prev);
    }
    runtime.set_object_field(id, "scriptstate", Value::string(state));
    runtime.set_object_field(id, "statechangereason", Value::string(reason));
}

/// The goal's test: within its radius across and its height up or down.
fn in_goal(world: &mut World, id: u64, at: [f32; 3], goal: [f32; 3]) -> bool {
    let radius = number(world, id, "goalradius", GOAL_RADIUS);
    let height = number(world, id, "goalheight", GOAL_HEIGHT);
    (at[2] - goal[2]).abs() <= height && flat_distance(at, goal) <= radius
}

pub(super) fn hull(world: &World, id: u64) -> ([f32; 3], [f32; 3]) {
    let (radius, _, top) = world
        .get_resource::<super::actor::Actors>()
        .and_then(|actors| actors.0.get(&id))
        .map_or((15.0, 0.0, 72.0), |actor| actor.hull);
    (
        [-radius, -radius, STEP_HEIGHT],
        [radius, radius, top.max(STEP_HEIGHT + 1.0)],
    )
}

fn clear(world: &mut World, id: u64, from: [f32; 3], to: [f32; 3]) -> bool {
    let (mins, maxs) = hull(world, id);
    trace(world, from, to, mins, maxs, ACTOR_MASK).fraction >= 1.0
}

/// The node nearest `at` within 192 units, else within 512, that a body
/// can walk to from there.
fn nearest_node(world: &mut World, id: u64, at: [f32; 3]) -> Option<u16> {
    nearest_node_within(world, id, at, &[NODE_NEAR, NODE_FAR])
}

/// The node nearest `at` within each radius in turn that a body (the
/// actor's, or a default one) can walk to from there.
pub(super) fn nearest_node_within(
    world: &mut World,
    id: u64,
    at: [f32; 3],
    radii: &[f32],
) -> Option<u16> {
    let paths = FrameWorld::from_world(world).actor_paths()?;
    for &radius in radii {
        let mut near: Vec<(f32, u16)> = paths
            .nodes()
            .iter()
            .enumerate()
            .filter(|(_, node)| !node.links.is_empty() && node.kind != NavNodeKind::Bad)
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
fn plan(world: &mut World, id: u64, from: [f32; 3], goal: [f32; 3]) -> Option<Vec<Waypoint>> {
    let arrival = Waypoint {
        at: goal,
        node: None,
    };
    if clear(world, id, from, goal) {
        return Some(vec![arrival]);
    }
    let paths = FrameWorld::from_world(world).actor_paths()?;
    let start = nearest_node(world, id, from)?;
    let end = nearest_node(world, id, goal)?;
    let holding = traversals_held(world);
    let edits = world
        .get_resource_or_insert_with(LinkEdits::default)
        .clone();
    let route = super::path_search::route(
        world,
        id,
        &paths,
        &edits,
        &holding,
        (start, end),
        (from, goal),
    )?;
    let mut points: Vec<Waypoint> = route
        .iter()
        .map(|&node| Waypoint {
            at: paths.nodes()[node as usize].origin,
            node: Some(node),
        })
        .collect();
    points.push(arrival);
    Some(points)
}

/// What the follower asks of the world for an actor.
struct WorldProbe<'w> {
    world: &'w mut World,
    id: u64,
    paths: Option<std::sync::Arc<crate::script::ActorPaths>>,
    edits: LinkEdits,
}

impl Probe for WorldProbe<'_> {
    /// Whether the actor's body walks from one point to the other: IW4L's
    /// hull trace (`t6.actors.path_following`).
    fn walkable(&mut self, from: [f32; 3], to: [f32; 3]) -> bool {
        clear(self.world, self.id, from, to)
    }

    fn linked(&mut self, from: u16, to: u16) -> bool {
        self.paths.as_ref().is_some_and(|paths| {
            paths
                .links(from, &self.edits)
                .any(|(next, _, _)| next == to)
        })
    }
}

fn probe(world: &mut World, id: u64) -> WorldProbe<'_> {
    let paths = FrameWorld::from_world(world).actor_paths();
    let edits = world
        .get_resource_or_insert_with(LinkEdits::default)
        .clone();
    WorldProbe {
        world,
        id,
        paths,
        edits,
    }
}

/// The follower for a planned path from `from`: a straight walk starts
/// where the actor stands, a path over the nodes at its first node.
fn follow_path(world: &mut World, id: u64, from: [f32; 3], path: &[Waypoint]) -> Follow {
    let mut route: Vec<([f32; 3], Option<u16>)> = Vec::with_capacity(path.len() + 1);
    if path.first().is_none_or(|point| point.node.is_none()) {
        route.push((from, None));
    }
    route.extend(path.iter().map(|point| (point.at, point.node)));
    // The goal is no point of its own where it stands on the last node.
    if let [.., (node_at, Some(_)), (goal, None)] = route[..]
        && node_at[0] == goal[0]
        && node_at[1] == goal[1]
    {
        route.pop();
    }
    let offset = route.len().saturating_sub(path.len()).min(1);
    let negotiation = (0..path.len())
        .find(|&index| negotiation(world, path, index).is_some())
        .map(|index| index + offset);
    let look = movement(world, id).look;
    let mut probe = probe(world, id);
    Follow::new(&route, negotiation, look, from, &mut probe)
}

/// How many actors' paths hold each traversal's begin node as the next
/// traversal they cross, as the engine counts them on the node.
fn traversals_held(world: &mut World) -> BTreeMap<u16, u32> {
    let paths: Vec<Vec<Waypoint>> = world
        .get_resource::<Movements>()
        .map(|movements| {
            movements
                .0
                .values()
                .filter(|movement| movement.traversal.is_none())
                .map(|movement| movement.path.clone())
                .collect()
        })
        .unwrap_or_else(Vec::new);
    let mut held = BTreeMap::new();
    for path in paths {
        if let Some((begin, _)) = (0..path.len()).find_map(|index| negotiation(world, &path, index))
        {
            *held.entry(begin).or_default() += 1;
        }
    }
    held
}

/// The points the actor's follower has left ahead of it, nearest first.
fn sync_path(state: &mut Movement) {
    if let Some(follow) = &state.follow {
        state.path = follow
            .ahead()
            .map(|point| Waypoint {
                at: point.origin,
                node: point.node,
            })
            .collect();
    }
}

/// The negotiation link the path crosses from its `index`th point: a begin
/// node naming a traversal, linked to the next point by a negotiation link.
fn negotiation(world: &mut World, path: &[Waypoint], index: usize) -> Option<(u16, u16)> {
    let begin = path.get(index)?.node?;
    let end = path.get(index + 1)?.node?;
    let paths = FrameWorld::from_world(world).actor_paths()?;
    let node = paths.nodes().get(usize::from(begin))?;
    let crosses = node.kind == NavNodeKind::NegotiationBegin
        && !node.animscript.is_empty()
        && node
            .links
            .iter()
            .any(|&(to, _, negotiates)| to == end && negotiates);
    crosses.then_some((begin, end))
}

/// Starts an animscript's main as the engine switches scripts; the one it
/// leaves is told `killanimscript`, and the actor's anim mode and orient
/// mode go back to `normal` and `face default`, as the engine's switch
/// leaves them. The thread while it lives on.
pub(super) fn set_script(
    world: &mut World,
    id: u64,
    script: Script,
) -> Result<Option<u64>, String> {
    if movement(world, id).script == Some(script) {
        return Ok(None);
    }
    movement(world, id).script = Some(script);
    let state_name = match script {
        Script::Move => "move",
        Script::Stop => "stop",
        Script::Combat => "combat",
        Script::Pain => "pain",
        Script::Death => "death",
        Script::Traverse(_) => "traverse",
    };
    record_script_state(world, id, state_name, state_name);
    let prefix = if matches!(
        world
            .resource_mut::<RoundScript>()
            .object_field(id, "isdog"),
        Value::Int(1)
    ) {
        "zm_dog_"
    } else {
        "zm_"
    };
    let main = match script {
        Script::Move => format!("{ANIMSCRIPTS}/{prefix}move::main"),
        Script::Stop => format!("{ANIMSCRIPTS}/{prefix}stop::main"),
        Script::Combat => format!("{ANIMSCRIPTS}/{prefix}combat::main"),
        Script::Pain => format!("{ANIMSCRIPTS}/{prefix}pain::main"),
        Script::Death => format!("{ANIMSCRIPTS}/{prefix}death::main"),
        Script::Traverse(begin) => {
            let paths = FrameWorld::from_world(world)
                .actor_paths()
                .ok_or("no path nodes")?;
            let name = &paths.nodes()[usize::from(begin)].animscript;
            format!("{ANIMSCRIPTS}/traverse/{name}::main")
        }
    };
    let at = origin_of(world, id);
    diag::debug!(Sim, "actor {id}: animscript {main} at {at:?}");
    let now = super::super::super::players::now_ms(world);
    notify_now(world, Value::Object(id), "killanimscript", Vec::new(), now);
    anim::reset_modes(world, id);
    let thread = start_now(world, &main, Value::Object(id), Vec::new(), now)
        .map_err(|fault| format!("{main}: {fault:?}"))?;
    if script == Script::Death {
        movement(world, id).death = thread;
    }
    Ok(thread)
}

/// The enemy an actor fights: its favorite enemy, while it lives and the
/// actor does not ignore everyone (`t6.actors.enemy`).
fn enemy(world: &mut World, id: u64) -> Option<Value> {
    let ignore = matches!(field(world, id, "ignoreall"), Value::Int(n) if n != 0);
    let favorite = field(world, id, "favoriteenemy");
    if ignore || !matches!(favorite, Value::Object(_)) {
        return None;
    }
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

/// Moves the body from `from` toward `to`: up a step, across until
/// something blocks, then down onto the floor or falling.
pub(super) fn step_actor(world: &mut World, id: u64, from: [f32; 3], to: [f32; 3]) -> [f32; 3] {
    step(world, id, from, to)
}

fn step(world: &mut World, id: u64, from: [f32; 3], to: [f32; 3]) -> [f32; 3] {
    let (mins, maxs) = hull(world, id);
    let (mins, maxs) = ([mins[0], mins[1], 0.0], [maxs[0], maxs[1], maxs[2]]);
    let ground = [from[0], from[1], from[2] + 0.25];
    let raised = [from[0], from[1], from[2] + STEP_HEIGHT];
    let up = trace(world, ground, raised, mins, maxs, ACTOR_MASK);
    let mut moved = lerp(ground, raised, up.fraction);
    let bodies = player_bodies(world, id);
    movement(world, id).hit_sentient = false;
    let mut motion = [to[0] - from[0], to[1] - from[1], 0.0];
    // A wall takes the part of the motion into it; the rest slides along.
    for _ in 0..SLIDES {
        let ahead = [moved[0] + motion[0], moved[1] + motion[1], moved[2]];
        let across = trace(world, moved, ahead, mins, maxs, ACTOR_MASK);
        let world_fraction = across.fraction;
        let across = crate::step::actor_move_past_bodies(
            across,
            &bodies,
            (moved, ahead),
            (mins, maxs),
            ACTOR_MASK,
        );
        if across.fraction < world_fraction {
            movement(world, id).hit_sentient = true;
        }
        moved = lerp(moved, ahead, across.fraction);
        if across.fraction >= 1.0 || across.startsolid != 0 {
            break;
        }
        let left = 1.0 - across.fraction;
        let into = motion[0] * across.normal[0] + motion[1] * across.normal[1];
        motion = [
            (motion[0] - into * across.normal[0]) * left,
            (motion[1] - into * across.normal[1]) * left,
            0.0,
        ];
    }
    let seconds = crate::MATCH_TICK_MS as f32 / 1000.0;
    let fall = movement(world, id).falling;
    let below = [moved[0], moved[1], from[2] - STEP_HEIGHT - fall * seconds];
    let down = trace(world, moved, below, mins, maxs, ACTOR_MASK);
    if down.startsolid != 0 {
        return from;
    }
    movement(world, id).falling = if down.fraction < 1.0 {
        0.0
    } else {
        fall + GRAVITY * seconds
    };
    lerp(moved, below, down.fraction)
}

/// The players' bodies an actor's move stops at, unless `pushplayer(1)` took
/// bodies out of its mask.
fn player_bodies(world: &mut World, id: u64) -> Vec<crate::step::PlayerBodyClip> {
    if movement(world, id).through_bodies {
        return Vec::new();
    }
    crate::step::alive_body_clips(&FrameWorld::from_world(world))
}

pub(super) fn lerp(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

/// The code move's backtrack: on for a hull of radius 15 or less, else as
/// `dlc3_zm_ai_lookAheadAllowBackTrack` says.
const BACKTRACK_RADIUS: f32 = 15.0;
const ALLOW_BACKTRACK: &str = "dlc3_zm_ai_lookaheadallowbacktrack";

fn backtracks(world: &World, id: u64) -> bool {
    let radius = world
        .get_resource::<super::actor::Actors>()
        .and_then(|actors| actors.0.get(&id))
        .map_or(BACKTRACK_RADIUS, |actor| actor.hull.0);
    radius <= BACKTRACK_RADIUS
        || world
            .resource::<crate::script::MatchScript>()
            .dvars
            .get(ALLOW_BACKTRACK)
            .is_some_and(|value| super::atof(value) != 0.0)
}

/// Walks the actor along its path as far as its clip's root moved this
/// frame, then onto the floor.
fn walk(world: &mut World, id: u64, from: [f32; 3]) {
    let anim = anim::get(world, id);
    let moves = !matches!(
        anim.mode,
        "none" | "zonly_physics" | "angle deltas" | "nophysics" | "point relative"
    );
    let speed = if moves {
        (anim.step[0].powi(2) + anim.step[1].powi(2)).sqrt()
    } else {
        0.0
    };
    let Some(mut follow) = movement(world, id).follow.take() else {
        return;
    };
    // The engine's code move: the lookahead first (planned again from here
    // when the link ahead was cut), then the step along it.
    let backtrack = backtracks(world, id);
    let kept = follow.update(from, (false, false, backtrack, true), &mut probe(world, id));
    if !kept {
        let goal = movement(world, id).goal;
        match goal.and_then(|goal| plan(world, id, from, goal)) {
            Some(path) => {
                follow = follow_path(world, id, from, &path);
                movement(world, id).arrival = None;
            }
            None => {
                follow.update(
                    from,
                    (false, false, backtrack, false),
                    &mut probe(world, id),
                );
            }
        }
    }
    let [dx, dy, _] = follow.look.dir;
    let flat = dx * dx + dy * dy;
    let factor = flat
        .max(MIN_FLAT_SQ)
        .powf(SPEED_EXPONENT)
        .max(MIN_SPEED_SCALE);
    let dist = (speed * factor).min(follow.look.dist);
    let at = [from[0] + dx * dist, from[1] + dy * dist, from[2]];
    let heading = (flat > 0.0).then(|| dy.atan2(dx).to_degrees());
    {
        let state = movement(world, id);
        state.look = follow.look;
        state.follow = Some(follow);
        sync_path(state);
    }
    let at = arrive(world, id, from).unwrap_or(at);
    let landed = step(world, id, from, at);
    let moved = flat_distance(from, landed);
    let state = movement(world, id);
    state.stalled = moved < 0.1;
    state.speed = moved * 1000.0 / FRAME_MS;
    state.wish = [at[0] - from[0], at[1] - from[1]];
    state.velocity = [
        (landed[0] - from[0]) * 1000.0 / FRAME_MS,
        (landed[1] - from[1]) * 1000.0 / FRAME_MS,
    ];
    {
        let look = movement(world, id).look;
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.set_object_field(id, "origin", Value::Vector(landed));
        runtime.set_object_field(id, "lookaheaddir", Value::Vector(look.dir));
        runtime.set_object_field(id, "lookaheaddist", Value::Float(look.dist));
    }
    if let Some(yaw) = heading
        && matches!(
            anim.orient,
            "face default" | "face motion" | "face enemy or motion"
        )
    {
        set_yaw(world, id, yaw);
    }
}

/// The lookahead's update for an actor that takes no code move this frame.
fn look_only(world: &mut World, id: u64, at: [f32; 3]) {
    let Some(mut follow) = movement(world, id).follow.take() else {
        return;
    };
    let backtrack = backtracks(world, id);
    let kept = follow.update(at, (false, false, backtrack, true), &mut probe(world, id));
    let state = movement(world, id);
    state.look = follow.look;
    state.follow = Some(follow);
    sync_path(state);
    if !kept {
        state.planned = false;
    }
}

fn stands_still(state: &mut Movement) {
    state.speed = 0.0;
    state.velocity = [0.0; 2];
    state.wish = [0.0; 2];
}

fn drop_path(state: &mut Movement) {
    state.path.clear();
    state.follow = None;
    state.arrival = None;
    state.path_goal = None;
    state.slowed = Slowed::default();
}

fn pile_state(world: &World, id: u64) -> Option<u64> {
    world
        .get_resource::<Movements>()
        .and_then(|movements| movements.0.get(&id))
        .and_then(|state| state.pile)
}

fn flat_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)
}

/// The living actors other than `id`.
fn others(world: &mut World, id: u64) -> Vec<u64> {
    let candidates: Vec<u64> = world
        .resource::<RoundScript>()
        .entities
        .iter()
        .filter(|(other, entity)| **other != id && entity.kind == EntityKind::Actor)
        .map(|(other, _)| *other)
        .collect();
    candidates
        .into_iter()
        .filter(|other| alive(world, *other))
        .collect()
}

/// The engine's pile-up upkeep before an actor's frame: it ends once the other
/// is no longer near, and when it has stepped clear and its path leads away.
fn update_pile(world: &mut World, id: u64, at: [f32; 3]) {
    let Some(pile) = movement(world, id).pile else {
        return;
    };
    let is_actor = world
        .resource::<RoundScript>()
        .entities
        .get(&pile)
        .is_some_and(|entity| entity.kind == EntityKind::Actor);
    if !is_actor || !alive(world, pile) {
        movement(world, id).pile = None;
        return;
    }
    let there = origin_of(world, pile);
    let dist_sq = flat_sq(at, there);
    if dist_sq >= PILE_HOLD_SQ {
        if pile_state(world, pile) == Some(id) {
            movement(world, pile).pile = None;
        }
        movement(world, id).pile = None;
        return;
    }
    if dist_sq >= PILE_TOUCH_SQ && anim::get(world, id).mode == "normal" {
        let state = movement(world, id);
        let away = (at[0] - there[0]) * state.look.dir[0] + (at[1] - there[1]) * state.look.dir[1];
        if state.follow.is_some() && away > 0.0 {
            state.pile = None;
        }
    }
}

/// When an actor waits: it piles up on an actor it touches (in combat, only
/// one that ran into a player or actor).
fn check_pile(world: &mut World, id: u64, at: [f32; 3]) {
    if movement(world, id).pile.is_some() {
        return;
    }
    let in_combat = movement(world, id).script == Some(Script::Combat);
    for other in others(world, id) {
        if in_combat && !movement(world, other).hit_sentient {
            continue;
        }
        let there = origin_of(world, other);
        if (at[2] - there[2]).powi(2) <= PILE_HEIGHT_SQ && flat_sq(at, there) <= PILE_TOUCH_SQ {
            movement(world, id).pile = Some(other);
            return;
        }
    }
}

/// The engine's move for an actor piled up on another: straight away from it
/// by its clip's root motion. It holds when that gets it no farther or brings
/// it to a third actor, which then piles up on it in turn.
fn separate(world: &mut World, id: u64, from: [f32; 3], pile: u64) {
    let there = origin_of(world, pile);
    let delta = [from[0] - there[0], from[1] - there[1]];
    let dist_sq = delta[0] * delta[0] + delta[1] * delta[1];
    let dir = if dist_sq == 0.0 {
        // The engine draws a random direction; two actors on one spot.
        let now = super::super::super::players::now_ms(world) as u64;
        let (sin, cos) = ((now.wrapping_mul(2_654_435_761).wrapping_add(id) % 360) as f32)
            .to_radians()
            .sin_cos();
        [cos, sin]
    } else {
        let dist = dist_sq.sqrt();
        [delta[0] / dist, delta[1] / dist]
    };
    let root = anim::get(world, id).step;
    let move_sq = (root[0] * root[0] + root[1] * root[1]).max(PILE_MIN_MOVE_SQ);
    let distance = move_sq.sqrt();
    let to = [
        from[0] + dir[0] * distance,
        from[1] + dir[1] * distance,
        from[2],
    ];
    let landed = step(world, id, from, to);
    let farther = dist_sq < flat_sq(landed, there);
    let moved = move_sq * PILE_MOVED_SHARE < flat_sq(landed, from);
    let free = farther && moved && !touches_another(world, id, pile, from, landed);
    let state = movement(world, id);
    state.wish = [to[0] - from[0], to[1] - from[1]];
    if free {
        state.velocity = [
            (landed[0] - from[0]) * 1000.0 / FRAME_MS,
            (landed[1] - from[1]) * 1000.0 / FRAME_MS,
        ];
        state.speed = flat_distance(from, landed) * 1000.0 / FRAME_MS;
        world
            .resource_mut::<RoundScript>()
            .set_object_field(id, "origin", Value::Vector(landed));
    } else {
        state.velocity = [0.0; 2];
        state.speed = 0.0;
        if pile_state(world, pile).is_none() {
            movement(world, pile).pile = Some(id);
        }
    }
}

/// Whether a piled-up actor's move brings it to touch a third actor (which
/// then piles up on it).
fn touches_another(world: &mut World, id: u64, pile: u64, from: [f32; 3], to: [f32; 3]) -> bool {
    let mut touches = false;
    for other in others(world, id) {
        if other == pile {
            continue;
        }
        let there = origin_of(world, other);
        if flat_sq(to, there) < PILE_TOUCH_SQ
            && PILE_TOUCH_SQ <= flat_sq(from, there)
            && (from[2] - there[2]).abs() < PILE_TOUCH_HEIGHT
        {
            if pile_state(world, other).is_none() {
                movement(world, other).pile = Some(id);
            }
            touches = true;
        }
    }
    touches
}

/// The engine's team move for an actor with a path (`natives::t6::team_move`).
fn team_step(world: &mut World, id: u64, at: [f32; 3], now: i64) -> TeamMove {
    if movement(world, id).wait_until > now {
        return TeamMove::Wait;
    }
    if let Some(pile) = movement(world, id).pile
        && pile_state(world, pile).is_some_and(|chain| chain != id)
    {
        movement(world, id).wait_until = now + PILE_CHAIN_WAIT_MS;
        return TeamMove::Wait;
    }
    let team = field(world, id, "team");
    let mut friends = Vec::new();
    for other in others(world, id) {
        if field(world, other, "team") != team {
            continue;
        }
        let there = origin_of(world, other);
        let normal = anim::get(world, other).mode == "normal";
        let number = world.resource::<RoundScript>().entities[&other].number;
        let state = movement(world, other);
        let velocity = if state.pile.is_none() {
            [
                state.wish[0] * 1000.0 / FRAME_MS,
                state.wish[1] * 1000.0 / FRAME_MS,
            ]
        } else {
            [0.0; 2]
        };
        friends.push(Friend {
            id: other,
            number,
            at: there,
            velocity,
            hit_sentient: state.hit_sentient,
            walks: state.move_mode == 2,
            runs: state.move_mode == 3,
            normal,
        });
    }
    let nododge = !matches!(field(world, id, "nododgemove"), Value::Int(0));
    let species_speed = if matches!(field(world, id, "isdog"), Value::Int(1)) {
        DOG_SPEED
    } else {
        ZOMBIE_SPEED
    };
    let number = world.resource::<RoundScript>().entities[&id].number;
    let slowed = movement(world, id).slowed;
    let slowed_at = slowed.by.map(|friend| origin_of(world, friend));
    let path = movement(world, id).path.clone();
    let negotiating = (0..path.len()).any(|index| negotiation(world, &path, index).is_some());
    let state = movement(world, id);
    let me = Mover {
        number,
        at,
        velocity: state.velocity,
        look_dir: [state.look.dir[0], state.look.dir[1]],
        look_dist: state.look.dist,
        amount: state.look.amount,
        moving: state.move_mode != 0,
        nododge,
        piled: state.pile.is_some(),
        species_speed,
        slowed,
        slowed_at,
        negotiating,
    };
    let out = team_move::decide(&me, &friends);
    state.slowed = out.slowed;
    if out.pile.is_some() {
        state.pile = out.pile;
    }
    if out.wait {
        state.wait_until = now + team_move::WAIT_MS;
    }
    if out.drop_path {
        drop_path(state);
    }
    out.result
}

/// The move mode a team move's result and the actor's pile-up and arrival
/// leave it in; the script follows from it.
fn move_mode(result: TeamMove, piled: bool, moving: bool, stop_left: Option<i64>) -> u8 {
    let piled_mode = if moving { 2 } else { 1 };
    match result {
        TeamMove::Wait => {
            if piled {
                piled_mode
            } else {
                0
            }
        }
        TeamMove::Travel => {
            if piled {
                piled_mode
            } else if stop_left.is_some_and(|left| left <= ARRIVAL_STOP_MS) {
                1
            } else {
                3
            }
        }
        TeamMove::Slow => {
            if piled {
                piled_mode
            } else if stop_left.is_some_and(|left| left < ARRIVAL_STOP_MS) {
                1
            } else {
                2
            }
        }
    }
}

/// The engine drops a path for combat when its enemy is within
/// `pathenemyfightdist` across and the goal height, in sight, and either the
/// lookahead would pass it or it stands in the actor's goal.
fn enemy_stops_path(
    world: &mut World,
    id: u64,
    at: [f32; 3],
    goal: [f32; 3],
    enemy: &Value,
) -> bool {
    let Ok(other) = entity_id(world, enemy) else {
        return false;
    };
    let there = origin_of(world, other);
    let fight = number(world, id, "pathenemyfightdist", 0.0);
    let height = number(world, id, "goalheight", GOAL_HEIGHT);
    if flat_sq(at, there) >= fight * fight || (there[2] - at[2]).abs() > height {
        return false;
    }
    if !super::combat::can_see(world, id, enemy).unwrap_or(false) {
        return false;
    }
    let look = movement(world, id).look;
    let beyond = [
        at[0] + look.dir[0] * look.dist - there[0],
        at[1] + look.dir[1] * look.dist - there[1],
    ];
    let passes = look.dir[0] * beyond[0] + look.dir[1] * beyond[1] > 0.0
        && beyond[0] * beyond[0] + beyond[1] * beyond[1] < look.dist * look.dist;
    passes || in_goal(world, id, there, goal)
}

/// The engine's arrival, after the frame's lookahead: where the actor steps
/// instead of along its lookahead while its timer runs. The timer starts when
/// the lookahead has reached the path's end, with no traversal ahead and the
/// end a quarter second away; when it runs out the actor takes all that is
/// left and its path is cleared.
fn arrive(world: &mut World, id: u64, from: [f32; 3]) -> Option<[f32; 3]> {
    let now = super::super::super::players::now_ms(world);
    let state = movement(world, id);
    let follow = state.follow.as_ref()?;
    let point = follow.next_point();
    let left = [point[0] - from[0], point[1] - from[1]];
    let end = match state.arrival {
        Some(end) => end,
        None => {
            let far =
                state.speed * state.speed * ARRIVAL_SPEED_SQ < left[0].powi(2) + left[1].powi(2);
            if !follow.arriving() || far {
                return None;
            }
            now + ARRIVAL_MS
        }
    };
    let time = end - now;
    if time < 1 {
        state.arrival = None;
        state.path.clear();
        state.follow = None;
        return Some([point[0], point[1], from[2]]);
    }
    state.arrival = Some(end);
    let frame = FRAME_MS / time as f32;
    let share = (2.0 - frame) * frame;
    Some([
        from[0] + left[0] * share,
        from[1] + left[1] * share,
        from[2],
    ])
}

/// `maymovetopoint`: the point grounded within 48 units, not starting in
/// the world, no more than 32 below the actor when drops count, and the body
/// able to walk there (`t6.actors.path_following` for the walk test).
fn may_move(world: &mut World, id: u64, from: [f32; 3], to: [f32; 3], drops: bool) -> bool {
    let top = [to[0], to[1], to[2] + PROBE_HEIGHT];
    let bottom = [to[0], to[1], to[2] - PROBE_HEIGHT];
    let floor = trace(
        world,
        top,
        bottom,
        [0.0; 3],
        [0.0; 3],
        ACTOR_MASK | PROBE_MASK,
    );
    if floor.startsolid != 0 {
        return false;
    }
    let ground = lerp(top, bottom, floor.fraction);
    if drops && from[2] - ground[2] > MAX_DROP {
        return false;
    }
    clear(world, id, from, ground)
}

/// Stands the actor on the begin node, facing along it, as the traversal
/// clip expects (`t6.actors.path_following`).
fn align(world: &mut World, id: u64, begin: u16) {
    let Some(paths) = FrameWorld::from_world(world).actor_paths() else {
        return;
    };
    let node = &paths.nodes()[usize::from(begin)];
    let at = origin_of(world, id);
    let mut runtime = world.resource_mut::<RoundScript>();
    let angles = match runtime.object_field(id, "angles") {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    };
    runtime.set_object_field(
        id,
        "origin",
        Value::Vector([node.origin[0], node.origin[1], at[2]]),
    );
    runtime.set_object_field(
        id,
        "angles",
        Value::Vector([angles[0], node.yaw, angles[2]]),
    );
}

fn embedded(world: &mut World, id: u64, at: [f32; 3]) -> bool {
    let (mins, maxs) = hull(world, id);
    let below = [at[0], at[1], at[2] - 1.0];
    trace(world, at, below, [mins[0], mins[1], 0.0], maxs, ACTOR_MASK).startsolid != 0
}

/// Stands the actor on the floor under a node (`t6.actors.path_following`).
fn settle(world: &mut World, id: u64, node: u16) {
    let Some(paths) = FrameWorld::from_world(world).actor_paths() else {
        return;
    };
    let top = paths.nodes()[usize::from(node)].origin;
    let bottom = [top[0], top[1], top[2] - NODE_DROP];
    let (mins, maxs) = hull(world, id);
    let floor = trace(
        world,
        top,
        bottom,
        [mins[0], mins[1], 0.0],
        maxs,
        ACTOR_MASK,
    );
    let at = lerp(top, bottom, floor.fraction);
    world
        .resource_mut::<RoundScript>()
        .set_object_field(id, "origin", Value::Vector(at));
}

/// Carries the actor by its traversal clip's root motion, turned to its
/// facing: through the world under `noclip`, else by the body's step.
fn traverse(world: &mut World, id: u64, from: [f32; 3]) {
    let anim = anim::get(world, id);
    let mut angles = match world
        .resource_mut::<RoundScript>()
        .object_field(id, "angles")
    {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    };
    let yaw = angles[1].to_radians();
    let (sin, cos) = yaw.sin_cos();
    let delta = [
        cos * anim.step[0] - sin * anim.step[1],
        sin * anim.step[0] + cos * anim.step[1],
        anim.step[2],
    ];
    let to: [f32; 3] = std::array::from_fn(|i| from[i] + delta[i]);
    let landed = match movement(world, id).traverse_mode {
        TraverseMode::NoClip => to,
        TraverseMode::NoGravity => {
            let (mins, maxs) = hull(world, id);
            let hit = trace(world, from, to, [mins[0], mins[1], 0.0], maxs, ACTOR_MASK);
            lerp(from, to, hit.fraction)
        }
        TraverseMode::Gravity => step(world, id, from, to),
    };
    angles[1] = (angles[1] + anim.step_yaw).rem_euclid(360.0);
    let mut runtime = world.resource_mut::<RoundScript>();
    runtime.set_object_field(id, "origin", Value::Vector(landed));
    if anim.step_yaw != 0.0 {
        runtime.set_object_field(id, "angles", Value::Vector(angles));
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

/// `Actor_UpdatePitchOrient` / `Actor_ProbeGroundPitch`: probes 10.9 units
/// ahead and behind the actor along its yaw (`[-4, -4, 0]..[4, 4, 8]` box
/// from `z + 30` down to `z - 30`, or `z - 45` for dogs) and steps the
/// actor's pitch (`angles[0]`) toward the ground slope at up to 12 deg/tick.
fn update_pitch_orient(world: &mut World, id: u64) {
    let at = origin_of(world, id);
    let angles = match world
        .resource_mut::<RoundScript>()
        .object_field(id, "angles")
    {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    };
    let is_dog = matches!(field(world, id, "isdog"), Value::Int(1));
    let drop_depth = if is_dog { 45.0 } else { 30.0 };
    let mins = [-4.0, -4.0, 0.0];
    let maxs = [4.0, 4.0, 8.0];
    let (sin, cos) = angles[1].to_radians().sin_cos();
    let start = [at[0], at[1], at[2] + 30.0];
    let mut probe = |sign: f32| -> Option<[f32; 3]> {
        let end_horiz = [
            start[0] + cos * 10.9 * sign,
            start[1] + sin * 10.9 * sign,
            start[2],
        ];
        let horiz = trace(world, start, end_horiz, mins, maxs, ACTOR_MASK);
        if horiz.fraction == 0.0 {
            return None;
        }
        let mid = lerp(start, end_horiz, horiz.fraction);
        let end_down = [mid[0], mid[1], at[2] - drop_depth];
        let down = trace(world, mid, end_down, mins, maxs, ACTOR_MASK);
        if down.startsolid != 0 || down.fraction >= 1.0 {
            Some(at)
        } else {
            Some(lerp(mid, end_down, down.fraction))
        }
    };
    let target_pitch = match (probe(1.0), probe(-1.0)) {
        (Some(front), Some(back)) if front != back => {
            let dx = front[0] - back[0];
            let dy = front[1] - back[1];
            let dz = front[2] - back[2];
            (-dz).atan2((dx * dx + dy * dy).sqrt()).to_degrees()
        }
        _ => 0.0,
    };
    let pitch = turn(angles[0], target_pitch, 12.0);
    world.resource_mut::<RoundScript>().set_object_field(
        id,
        "angles",
        Value::Vector([pitch, angles[1], angles[2]]),
    );
}

fn think_one(world: &mut World, id: u64) -> Result<(), String> {
    if !alive(world, id) || anim::scripted(world, id) {
        return Ok(());
    }
    if let Some((thread, end_func)) = movement(world, id).custom {
        if thread.is_some_and(|t| thread_alive(world, t)) {
            let at = origin_of(world, id);
            let enemy = enemy(world, id);
            if anim::get(world, id).mode != "none" {
                traverse(world, id, at);
            }
            face(world, id, at, enemy.as_ref());
            return Ok(());
        }
        movement(world, id).custom = None;
        movement(world, id).script = None;
        if let Some(end_func) = end_func {
            let now = super::super::super::players::now_ms(world);
            start_function_now(world, end_func, Value::Object(id), Vec::new(), now)
                .map_err(|fault| format!("animcustom end: {fault:?}"))?;
        }
    }
    if let Some(pain) = movement(world, id).pain {
        if thread_alive(world, pain) {
            let at = origin_of(world, id);
            let enemy = enemy(world, id);
            if anim::get(world, id).mode != "none" {
                traverse(world, id, at);
            }
            face(world, id, at, enemy.as_ref());
            return Ok(());
        }
        movement(world, id).pain = None;
        movement(world, id).script = None;
    }
    let at = origin_of(world, id);
    let enemy = enemy(world, id);
    world.resource_mut::<RoundScript>().set_object_field(
        id,
        "enemy",
        enemy.clone().unwrap_or(Value::Undefined),
    );
    if world
        .resource::<RoundScript>()
        .entities
        .get(&id)
        .is_some_and(|e| e.linked_to.is_some())
    {
        let state = movement(world, id);
        state.path.clear();
        state.follow = None;
        state.planned = false;
        state.falling = 0.0;
        return Ok(());
    }
    if let Some(traversal) = movement(world, id).traversal {
        if traversal
            .thread
            .is_some_and(|thread| thread_alive(world, thread))
        {
            // The clip holds the facing the traversal started with.
            traverse(world, id, at);
            return Ok(());
        }
        // The clip leaves the body near the end node; the walk goes on from
        // where it stands (`t6.actors.path_following`).
        if embedded(world, id, at) {
            settle(world, id, traversal.end);
        }
        let state = movement(world, id);
        state.traversal = None;
        state.falling = 0.0;
        state.planned = false;
    }
    let at = origin_of(world, id);
    let goal = *movement(world, id).goal.get_or_insert(at);
    let now_ms = super::super::super::players::now_ms(world);
    update_pile(world, id, at);
    // No new path while the team move's wait runs on an actor without one.
    let holding = movement(world, id).follow.is_none() && now_ms < movement(world, id).wait_until;
    if !movement(world, id).planned && !holding && !keeps_path(world, id, goal) {
        movement(world, id).planned = true;
        let path = if movement(world, id).follow.is_none() && in_goal(world, id, at, goal) {
            Some(Vec::new())
        } else {
            plan(world, id, at, goal)
        };
        diag::debug!(
            Sim,
            "actor {id}: goal {goal:?} from {at:?}, path {:?}",
            path.as_ref().map(Vec::len)
        );
        match path {
            Some(path) => install_path(world, id, at, goal, path),
            None => {
                drop_path(movement(world, id));
                {
                    let mut runtime = world.resource_mut::<RoundScript>();
                    runtime.set_object_field(id, "pathstartpos", Value::Undefined);
                    runtime.set_object_field(id, "pathgoalpos", Value::Undefined);
                    runtime.set_object_field(id, "lookaheadnodepos", Value::Undefined);
                }
                raise(world, Value::Object(id), "bad_path", Vec::new());
            }
        }
    }
    // A traversal starts on its begin node, or as near as the walk got.
    let stalled = movement(world, id).stalled;
    // The engine's exposed state runs combat for an actor with no path, or
    // with one while it is not safe to change its script, enemy or not;
    // with a path its move choice runs: a traversal, the stop script late in
    // an arrival (the last 200 ms when farther from the goal than `walkdist`,
    // just under that when not), or move.
    let safe = !matches!(field(world, id, "safetochangescript"), Value::Int(0));
    if movement(world, id).follow.is_some()
        && let Some(enemy) = enemy.as_ref()
        && enemy_stops_path(world, id, at, goal, enemy)
    {
        let state = movement(world, id);
        drop_path(state);
        state.planned = false;
        state.wait_until = now_ms + PATH_DROP_WAIT_MS;
    }
    let path = movement(world, id).path.clone();
    let crossing = negotiation(world, &path, 0).filter(|_| {
        let gap = flat_distance(at, path[0].at);
        safe && (gap <= 1.0 || stalled && gap <= POINT_REACHED)
    });
    let decided =
        (!path.is_empty() && safe && crossing.is_none()).then(|| team_step(world, id, at, now_ms));
    if decided == Some(TeamMove::Wait) {
        check_pile(world, id, at);
    }
    let path_left = !movement(world, id).path.is_empty();
    let state = movement(world, id);
    let piled = state.pile.is_some();
    let moving = state.velocity[0].powi(2) + state.velocity[1].powi(2) >= 1.0;
    let stop_left = state.arrival.map(|end| end - now_ms);
    let mode = decided.map_or(0, |result| move_mode(result, piled, moving, stop_left));
    state.move_mode = mode;
    let script = match crossing {
        _ if !path_left || !safe => Script::Combat,
        Some((begin, _)) => Script::Traverse(begin),
        None => match mode {
            0 => Script::Combat,
            1 => Script::Stop,
            _ => Script::Move,
        },
    };
    if script == Script::Move && movement(world, id).script != Some(Script::Move) {
        movement(world, id).arrival = None;
    }
    if !matches!(script, Script::Move | Script::Stop) {
        stands_still(movement(world, id));
    }
    if let Some((begin, end)) = crossing {
        movement(world, id).traversal = Some(Traversal {
            begin,
            end,
            thread: None,
        });
        align(world, id, begin);
    }
    let thread = set_script(world, id, script)?;
    match script {
        Script::Traverse(_) => {
            let state = movement(world, id);
            match (state.traversal.as_mut(), thread) {
                (Some(traversal), Some(thread)) => traversal.thread = Some(thread),
                _ => state.traversal = None,
            }
        }
        Script::Move | Script::Stop => match pile_state(world, id) {
            Some(pile) => {
                look_only(world, id, at);
                separate(world, id, at, pile);
            }
            None => walk(world, id, at),
        },
        Script::Combat => {
            look_only(world, id, at);
            if let Some(pile) = pile_state(world, id) {
                separate(world, id, at, pile);
            }
            face(world, id, at, enemy.as_ref());
        }
        Script::Pain | Script::Death => {}
    }
    if world
        .get_resource::<super::extras::ActorPitch>()
        .and_then(|p| p.0.get(&id))
        .is_some_and(|&(orient, allow)| orient || allow)
    {
        update_pitch_orient(world, id);
    }
    let now = origin_of(world, id);
    // The engine notifies every frame the actor is in its goal.
    if in_goal(world, id, now, goal) {
        raise(world, Value::Object(id), "goal", Vec::new());
    }
    Ok(())
}

/// The engine keeps a path through a new goal while the path's end stays in
/// the goal and no traversal lies ahead on it.
fn keeps_path(world: &mut World, id: u64, goal: [f32; 3]) -> bool {
    let state = movement(world, id);
    let Some(end) = state.path_goal.filter(|_| state.follow.is_some()) else {
        return false;
    };
    if state.follow.as_ref().is_some_and(Follow::truncated) {
        return false;
    }
    let path = state.path.clone();
    if !in_goal(world, id, end, goal)
        || (0..path.len()).any(|index| negotiation(world, &path, index).is_some())
    {
        return false;
    }
    movement(world, id).planned = true;
    true
}

/// A second's debug line: how many pairs of living actors stand closer than
/// their bodies' width, and the closest.
fn report_crowd(world: &mut World, actors: &[u64]) {
    const BODY_SQ: f32 = 900.0;
    let now = super::super::super::players::now_ms(world);
    if now % 1000 >= i64::from(crate::MATCH_TICK_MS) {
        return;
    }
    let mut spots = Vec::with_capacity(actors.len());
    for &id in actors {
        if alive(world, id) {
            spots.push(origin_of(world, id));
        }
    }
    let mut overlaps = 0;
    let mut closest = f32::MAX;
    for (index, a) in spots.iter().enumerate() {
        for b in &spots[index + 1..] {
            if (a[2] - b[2]).abs() < PILE_TOUCH_HEIGHT {
                let dist = flat_sq(*a, *b);
                closest = closest.min(dist);
                overlaps += usize::from(dist < BODY_SQ);
            }
        }
    }
    if closest < f32::MAX {
        diag::debug!(
            Sim,
            "crowd: {} actors, {overlaps} pairs within a body, closest {:.0}",
            spots.len(),
            closest.sqrt()
        );
    }
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
    super::corpse::forget_deleted(world);
    super::corpse::think(world);
    report_crowd(world, &actors);
    for id in actors {
        // The death state (`AIS_DEATH`): steps the death clip's root motion
        // while its animscript runs; once it ends the actor becomes a corpse.
        let state = movement(world, id);
        let dead = state.script == Some(Script::Death) && !alive(world, id);
        if dead {
            if movement(world, id)
                .death
                .is_some_and(|t| thread_alive(world, t))
            {
                if !anim::scripted(world, id) && anim::get(world, id).mode != "none" {
                    let at = origin_of(world, id);
                    traverse(world, id, at);
                }
            } else if let Err(fault) = super::corpse::finish_death(world, id) {
                diag::warn!(Sim, "actor {id}: {fault}");
            }
            continue;
        }
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
    state.planned = false;
}

pub(super) fn replan(world: &mut World, id: u64) {
    let state = movement(world, id);
    drop_path(state);
    state.planned = false;
}

/// The engine's request for a path to a point: none for a linked actor; a
/// path ending within 2 units of it stays, unless a traversal lies ahead or it
/// was cut short; an actor already there has none.
fn path_to_point(world: &mut World, id: u64, point: [f32; 3]) {
    let at = origin_of(world, id);
    let linked = world
        .resource::<RoundScript>()
        .entities
        .get(&id)
        .is_some_and(|entity| entity.linked_to.is_some());
    if linked {
        drop_path(movement(world, id));
        return;
    }
    let reaches = |to: [f32; 3]| {
        (to[2] - point[2]).powi(2) <= PILE_HEIGHT_SQ && flat_sq(to, point) <= POINT_NEAR_SQ
    };
    if movement(world, id).follow.is_some() {
        let state = movement(world, id);
        let end = state.path_goal;
        let cut = state.follow.as_ref().is_some_and(Follow::truncated);
        let path = state.path.clone();
        let crossing = (0..path.len()).any(|index| negotiation(world, &path, index).is_some());
        if !cut && !crossing && end.is_some_and(reaches) {
            return;
        }
        drop_path(movement(world, id));
    } else if reaches(at) {
        return;
    }
    if let Some(path) = plan(world, id, at, point) {
        install_path(world, id, at, point, path);
    }
}

/// The actor's path becomes one planned to `goal` from `at`.
fn install_path(world: &mut World, id: u64, at: [f32; 3], goal: [f32; 3], path: Vec<Waypoint>) {
    let first = path.first().map(|w| w.at);
    let follow = follow_path(world, id, at, &path);
    let state = movement(world, id);
    state.look = follow.look;
    state.follow = Some(follow);
    state.path = path;
    state.arrival = None;
    state.path_goal = (!state.path.is_empty()).then_some(goal);
    sync_path(state);
    let mut runtime = world.resource_mut::<RoundScript>();
    runtime.set_object_field(id, "pathstartpos", Value::Vector(at));
    runtime.set_object_field(id, "pathgoalpos", Value::Vector(goal));
    runtime.set_object_field(
        id,
        "lookaheadnodepos",
        first.map_or(Value::Undefined, Value::Vector),
    );
}

/// `pushplayer`: `1` takes players' bodies out of the actor's move, `0` puts
/// them back.
pub(super) fn set_through_bodies(world: &mut World, id: u64, through: bool) {
    movement(world, id).through_bodies = through;
}

pub(super) fn in_custom(world: &mut World, id: u64) -> bool {
    movement(world, id)
        .custom
        .and_then(|(thread, _)| thread)
        .is_some_and(|thread| thread_alive(world, thread))
}

pub(super) fn path_yaw(world: &mut World, id: u64) -> Option<f32> {
    let at = origin_of(world, id);
    let next = movement(world, id).path.first()?.at;
    let (dx, dy) = (next[0] - at[0], next[1] - at[1]);
    (dx * dx + dy * dy > 0.01).then(|| dy.atan2(dx).to_degrees())
}

fn func_arg(args: &[Value], index: usize) -> Result<usize, String> {
    match super::arg(args, index)? {
        Value::Function(f) => Ok(*f as usize),
        other => Err(format!(
            "type {} is not a function",
            super::type_name(other)
        )),
    }
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    // gsc_findpath(start, end, [entity], [allow_negotiation], [allow_partial]).
    registry.register(Function, "findpath", |world, _, args| {
        let start = vector(args, 0)?;
        let end = vector(args, 1)?;
        let id = match args.get(2) {
            Some(value @ Value::Object(_)) => entity_id(world, value).unwrap_or(0),
            _ => 0,
        };
        if args.len() >= 4 {
            super::int(args, 3)?;
        }
        if args.len() >= 5 {
            super::int(args, 4)?;
        }
        Ok(Value::Int(plan(world, id, start, end).is_some().into()))
    });

    // gscm_getpathlength: the remaining length of the actor's current path.
    registry.register(Method, "getpathlength", |world, receiver, _| {
        let id = actor(world, receiver)?;
        let mut at = origin_of(world, id);
        let mut length = 0.0f32;
        for point in &movement(world, id).path.clone() {
            length += (0..3)
                .map(|i| (point.at[i] - at[i]).powi(2))
                .sum::<f32>()
                .sqrt();
            at = point.at;
        }
        Ok(Value::Int(length as i32))
    });

    // gscm_calcpathlength(point): the actor's own path becomes one to the
    // point, and the answer is its number of points (0 with no path).
    registry.register(
        Namespace::Method,
        "calcpathlength",
        |world, receiver, args| {
            let id = actor(world, receiver).map_err(|_| "not an actor".to_owned())?;
            let Some(Value::Vector(point)) = args.first() else {
                return Err("calcpathlength expects a point".into());
            };
            path_to_point(world, id, *point);
            Ok(Value::Int(
                movement(world, id)
                    .follow
                    .as_ref()
                    .map_or(0, |follow| follow.len() as i32),
            ))
        },
    );
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
            Value::Object(object) => match world
                .resource_mut::<RoundScript>()
                .object_field(*object, "origin")
            {
                Value::Vector(v) => v,
                _ => return Err("not a node".into()),
            },
            _ => return Err("not a node".into()),
        };
        set_goal(world, id, goal);
        Ok(Value::Undefined)
    });
    registry.register(Method, "traversemode", |world, receiver, args| {
        let id = actor(world, receiver)?;
        movement(world, id).traverse_mode = match super::string(args, 0)?.as_str() {
            "gravity" => TraverseMode::Gravity,
            "nogravity" => TraverseMode::NoGravity,
            "noclip" => TraverseMode::NoClip,
            _ => return Err("traverseMode must be 'gravity', 'nogravity', or 'noclip'\n".into()),
        };
        Ok(Value::Undefined)
    });
    registry.register(Method, "getnegotiationstartnode", |world, receiver, _| {
        let id = actor(world, receiver)?;
        let state = movement(world, id).clone();
        let begin = match state.traversal {
            Some(traversal) => Some(traversal.begin),
            None => negotiation(world, &state.path, 0).map(|(begin, _)| begin),
        };
        begin.map_or(Ok(Value::Undefined), |begin| node_value(world, begin))
    });
    registry.register(Method, "maymovetopoint", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let point = vector(args, 0)?;
        let drops = args.len() < 2 || super::int(args, 1)? != 0;
        let from = origin_of(world, id);
        Ok(Value::Int(may_move(world, id, from, point, drops).into()))
    });
    registry.register(
        Method,
        "maymovefrompointtopoint",
        |world, receiver, args| {
            let id = actor(world, receiver)?;
            let from = vector(args, 0)?;
            let to = vector(args, 1)?;
            let drops = args.len() < 3 || super::int(args, 2)? != 0;
            let linked = world
                .resource::<RoundScript>()
                .entities
                .get(&id)
                .is_some_and(|e| e.linked_to.is_some());
            let ok = !linked && may_move(world, id, from, to, drops);
            Ok(Value::Int(ok.into()))
        },
    );
    registry.register(Method, "isingoal", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let point = vector(args, 0)?;
        let at = origin_of(world, id);
        let goal = *movement(world, id).goal.get_or_insert(at);
        Ok(Value::Int(in_goal(world, id, point, goal).into()))
    });
    // gscm_nearnode(node): within 80 vertically and 15 horizontally.
    registry.register(Method, "nearnode", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let Some(Value::Object(node_obj)) = args.first() else {
            return Err("not a pathnode".into());
        };
        let is_node = world
            .resource::<RoundScript>()
            .path_node_objects
            .values()
            .any(|obj| obj == node_obj);
        if !is_node {
            return Err("not a pathnode".into());
        }
        let node_origin = origin_of(world, *node_obj);
        let at = origin_of(world, id);
        let dz = at[2] - node_origin[2];
        let dx = node_origin[0] - at[0];
        let dy = node_origin[1] - at[1];
        let near = dz * dz <= 6400.0 && dx * dx + dy * dy <= 225.0;
        Ok(Value::Int(near.into()))
    });
    // gscm_teleport(origin, [angles]).
    registry.register(Method, "teleport", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let origin = vector(args, 0)?;
        let angles = if args.len() > 1 {
            Some(vector(args, 1)?)
        } else {
            None
        };
        let old = origin_of(world, id);
        let dist_sq: f32 = (0..3).map(|i| (old[i] - origin[i]).powi(2)).sum();
        {
            let mut runtime = world.resource_mut::<RoundScript>();
            runtime.set_object_field(id, "origin", Value::Vector(origin));
            if let Some(angles) = angles {
                runtime.set_object_field(id, "angles", Value::Vector(angles));
            }
        }
        if dist_sq > 100.0 {
            replan(world, id);
        }
        Ok(Value::Int(1))
    });
    // gscm_animcustom(func, [end_func]).
    registry.register(Method, "animcustom", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let func = func_arg(args, 0)?;
        let end_func = if args.len() >= 2 {
            Some(func_arg(args, 1)?)
        } else {
            None
        };
        let prev_end = movement(world, id).custom.take().and_then(|(_, end)| end);
        movement(world, id).script = None;
        record_script_state(world, id, "custom", "custom");
        anim::reset_orient(world, id);
        let now = super::super::super::players::now_ms(world);
        notify_now(world, receiver.clone(), "killanimscript", Vec::new(), now);
        if let Some(prev_end) = prev_end {
            start_function_now(world, prev_end, receiver.clone(), Vec::new(), now)
                .map_err(|fault| format!("animcustom end: {fault:?}"))?;
        }
        let thread = start_function_now(world, func, receiver.clone(), Vec::new(), now)
            .map_err(|fault| format!("animcustom: {fault:?}"))?;
        movement(world, id).custom = Some((thread, end_func));
        Ok(Value::Undefined)
    });
}
