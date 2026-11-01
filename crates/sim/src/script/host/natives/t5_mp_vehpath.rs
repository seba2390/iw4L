//! Black Ops multiplayer's vehicle paths as BlackOpsMP.exe's handlers keep
//! them: the map's `info_vehicle_node` chain with each node's speed and look
//! ahead filled in along it (`attachpath`), and the frame that walks a vehicle
//! down the chain toward a point ahead of it (`startpath`).

use crate::script::{RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

const MPH: f32 = 17.6;
/// A path's steps are a frame long.
const FRAME_S: f32 = 0.05;
/// Two segments turning more sharply than this share no bisecting plane.
const SHARP_TURN: f32 = 0.866;
/// How far past a segment's middle its fallback measure may advance.
const NEARLY_DONE: f32 = 0.9;
/// How fast a path's angles settle, a second.
const PITCH_ROLL_RATE: f32 = 6.0;
const YAW_RATE: f32 = 4.0;

#[derive(Clone, Debug, Default)]
struct Node {
    object: u64,
    name: String,
    target: String,
    origin: [f32; 3],
    angles: [f32; 3],
    /// Inches a second, and seconds ahead; negative until filled in.
    speed: f32,
    lookahead: f32,
    rotates: bool,
    speed_set: bool,
    lookahead_set: bool,
    next: Option<usize>,
    prev: Option<usize>,
    length: f32,
    dir: [f32; 3],
}

/// Where a vehicle is on its path: BlackOpsMP.exe's path position.
#[derive(Clone, Copy, Debug, Default)]
struct Track {
    node: usize,
    prev: usize,
    stopped: bool,
    /// How far along the node's segment.
    along: f32,
    speed: f32,
    lookahead: f32,
    rotate: f32,
    origin: [f32; 3],
    angles: [f32; 3],
}

/// A vehicle's run down its path.
#[derive(Clone, Copy, Debug, Default)]
struct PathRun {
    track: Track,
    running: bool,
    /// The share of a step this frame has gone past the track.
    fraction: f32,
    speed: f32,
    /// `setspeed` and `resumespeed`: 0 the path's speed, 1 the script's, 2
    /// back to the path's.
    mode: u8,
    wanted_speed: f32,
    speed_rate: f32,
    previous_angles: [f32; 3],
}

#[derive(Resource, Default)]
pub(crate) struct VehiclePaths {
    nodes: Option<Vec<Node>>,
    runs: BTreeMap<u64, PathRun>,
}

fn field_f32(runtime: &mut RoundScript, id: u64, name: &str) -> Option<f32> {
    match runtime.object_field(id, name) {
        Value::Int(value) => Some(value as f32),
        Value::Float(value) => Some(value),
        Value::String(text) => text.to_string().parse().ok(),
        _ => None,
    }
}

fn field_text(runtime: &mut RoundScript, id: u64, name: &str) -> String {
    match runtime.object_field(id, name) {
        Value::String(text) => text.to_string(),
        _ => String::new(),
    }
}

fn field_vector(runtime: &mut RoundScript, id: u64, name: &str) -> [f32; 3] {
    match runtime.object_field(id, name) {
        Value::Vector(value) => value,
        _ => [0.0; 3],
    }
}

fn length(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}

fn unit(v: [f32; 3]) -> [f32; 3] {
    let len = length(v);
    if len > 0.0 { v.map(|c| c / len) } else { v }
}

fn wrap_180(angle: f32) -> f32 {
    let wrapped = angle.rem_euclid(360.0);
    if wrapped > 180.0 {
        wrapped - 360.0
    } else {
        wrapped
    }
}

fn angles_of(dir: [f32; 3]) -> [f32; 3] {
    if dir[0] == 0.0 && dir[1] == 0.0 {
        return [if dir[2] > 0.0 { -90.0 } else { 90.0 }, 0.0, 0.0];
    }
    let flat = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
    [
        wrap_180(-dir[2].atan2(flat).to_degrees()),
        wrap_180(dir[1].atan2(dir[0]).to_degrees()),
        0.0,
    ]
}

/// Fills in the map's nodes: each one's target, length and direction, and the
/// speed and look ahead it did not set from those of the nodes around it.
fn build_nodes(world: &mut World) -> Vec<Node> {
    let mut runtime = world.resource_mut::<RoundScript>();
    let ids: Vec<(u64, bool)> = runtime
        .entities
        .iter()
        .filter(|(_, entity)| entity.classname.starts_with("info_vehicle_node"))
        .map(|(id, entity)| (*id, entity.classname.contains("rotate")))
        .collect();
    let mut nodes: Vec<Node> = ids
        .into_iter()
        .map(|(object, rotates)| {
            let speed = field_f32(&mut runtime, object, "speed");
            let lookahead = field_f32(&mut runtime, object, "lookahead");
            Node {
                object,
                name: field_text(&mut runtime, object, "targetname"),
                target: field_text(&mut runtime, object, "target"),
                origin: field_vector(&mut runtime, object, "origin"),
                angles: field_vector(&mut runtime, object, "angles"),
                speed: speed.filter(|s| *s >= 0.0).map_or(-1.0, |s| s * MPH),
                lookahead: lookahead.filter(|l| *l >= 0.0).unwrap_or(-1.0),
                rotates,
                speed_set: speed.is_some_and(|s| s >= 0.0),
                lookahead_set: lookahead.is_some_and(|l| l >= 0.0),
                ..Node::default()
            }
        })
        .collect();
    let count = nodes.len();
    for index in 0..count {
        let target = nodes[index].target.clone();
        if target.is_empty() {
            continue;
        }
        if let Some(next) = (0..count).find(|&other| other != index && nodes[other].name == target)
        {
            nodes[index].next = Some(next);
            nodes[next].prev = Some(index);
        }
    }
    for index in 0..count {
        if let Some(next) = nodes[index].next {
            let delta = sub(nodes[next].origin, nodes[index].origin);
            nodes[index].length = length(delta);
            nodes[index].dir = unit(delta);
            if !nodes[index].rotates {
                nodes[index].angles = angles_of(nodes[index].dir);
            }
        }
    }
    let speeds: Vec<f32> = (0..count)
        .map(|index| fill_along(&nodes, index, |node| node.speed, |node| node.speed_set))
        .collect();
    let lookaheads: Vec<f32> = (0..count)
        .map(|index| {
            fill_along(
                &nodes,
                index,
                |node| node.lookahead,
                |node| node.lookahead_set,
            )
        })
        .collect();
    for index in 0..count {
        nodes[index].speed = speeds[index];
        nodes[index].lookahead = lookaheads[index];
        if nodes[index].speed <= 0.0 || nodes[index].lookahead <= 0.0 {
            nodes[index].next = None;
        }
        if nodes[index].next.is_none() {
            if nodes[index].speed <= 0.0 {
                nodes[index].speed = 1.0;
            }
            if nodes[index].lookahead <= 0.0 {
                nodes[index].lookahead = 1.0;
            }
        }
    }
    nodes
}

/// A node's unset value, blended by distance between the nearest set values
/// behind and ahead of it along the chain.
fn fill_along(
    nodes: &[Node],
    index: usize,
    value: impl Fn(&Node) -> f32,
    set: impl Fn(&Node) -> bool,
) -> f32 {
    if set(&nodes[index]) {
        return value(&nodes[index]);
    }
    let mut behind = None;
    let mut at = nodes[index].prev;
    let mut distance = 0.0;
    let mut steps = 0;
    while let Some(node) = at {
        distance += nodes[node].length;
        if set(&nodes[node]) {
            behind = Some((value(&nodes[node]), distance));
            break;
        }
        at = nodes[node].prev;
        steps += 1;
        if node == index || steps > nodes.len() {
            break;
        }
    }
    let mut ahead = None;
    let mut at = Some(index);
    let mut distance = 0.0;
    let mut steps = 0;
    while let Some(node) = at {
        if set(&nodes[node]) {
            ahead = Some((value(&nodes[node]), distance));
            break;
        }
        distance += nodes[node].length;
        at = nodes[node].next;
        steps += 1;
        if at == Some(index) || steps > nodes.len() {
            break;
        }
    }
    match (behind, ahead) {
        (Some((before, to_before)), Some((after, to_after))) => {
            let total = to_before + to_after;
            if total > 0.0 {
                before + (after - before) * (to_before / total)
            } else {
                before
            }
        }
        (Some((value, _)), None) | (None, Some((value, _))) => value,
        (None, None) => -1.0,
    }
}

/// The point the track looks at: this far ahead down the chain.
fn lookahead_point(nodes: &[Node], track: &Track) -> [f32; 3] {
    let mut node = track.node;
    let mut distance = nodes[node].length * track.along + track.lookahead * track.speed;
    for _ in 0..nodes.len() {
        let Some(next) = nodes[node].next.filter(|_| nodes[node].length != 0.0) else {
            distance = 0.0;
            break;
        };
        if distance < nodes[node].length {
            break;
        }
        distance -= nodes[node].length;
        node = next;
    }
    std::array::from_fn(|i| nodes[node].dir[i] * distance + nodes[node].origin[i])
}

/// Which segment a place lies on, by the planes that bisect its joints.
fn find_place(nodes: &[Node], track: &mut Track) {
    let (mut node, mut prev) = (track.node, track.prev);
    let mut along = 0.0;
    for _ in 0..nodes.len() {
        let here = &nodes[node];
        let Some(next) = here.next.filter(|_| here.length != 0.0) else {
            along = 0.0;
            break;
        };
        let end_plane = unit(std::array::from_fn(|i| here.dir[i] + nodes[next].dir[i]));
        let start_plane = if -SHARP_TURN <= dot(nodes[prev].dir, here.dir) {
            unit(std::array::from_fn(|i| nodes[prev].dir[i] + here.dir[i]))
        } else {
            here.dir
        };
        let from_end = sub(nodes[next].origin, track.origin);
        let from_start = sub(track.origin, here.origin);
        let to_end = dot(from_end, end_plane);
        if to_end < 0.0 {
            prev = node;
            node = next;
            continue;
        }
        let past_start = dot(from_start, start_plane);
        if 0.0 <= past_start {
            along = past_start / (to_end + past_start);
            break;
        }
        if dot(here.dir, from_start) < 0.0 {
            along = 0.0;
            break;
        }
        let measure = 1.0 - length(from_end) / here.length;
        if measure <= 0.0 {
            along = 0.0;
            break;
        }
        along = measure.min(1.0);
        if along > NEARLY_DONE {
            prev = node;
            node = next;
            continue;
        }
        break;
    }
    track.stopped = nodes[node].next.is_none();
    track.node = node;
    track.prev = prev;
    track.along = along;
    let mix = |a: f32, b: f32| a + (b - a) * along;
    let next = nodes[node].next.unwrap_or(node);
    track.speed = mix(nodes[node].speed, nodes[next].speed);
    track.lookahead = mix(nodes[node].lookahead, nodes[next].lookahead);
    let rotate = |index: usize| if nodes[index].rotates { 1.0 } else { 0.0 };
    track.rotate = mix(rotate(node), rotate(next));
}

/// A step of the track: a frame's move toward its look ahead point.
fn step(nodes: &[Node], track: &mut Track) {
    if track.stopped {
        return;
    }
    let delta = sub(lookahead_point(nodes, track), track.origin);
    let distance = length(delta);
    if distance <= 0.0 {
        track.stopped = true;
        return;
    }
    let dir = delta.map(|c| c / distance);
    track.angles = angles_of(dir);
    let moved = track.speed * FRAME_S;
    for (place, d) in track.origin.iter_mut().zip(dir) {
        *place += d * moved;
    }
    find_place(nodes, track);
    if nodes[track.node].rotates {
        let next = nodes[track.node].next.unwrap_or(track.node);
        let along = track.along;
        for i in 0..3 {
            track.angles[i] = wrap_180(
                nodes[track.node].angles[i]
                    + wrap_180(nodes[next].angles[i] - nodes[track.node].angles[i]) * along,
            );
        }
    }
}

fn paths(world: &mut World) -> &mut VehiclePaths {
    world
        .get_resource_or_insert_with(VehiclePaths::default)
        .into_inner()
}

/// `vehicle attachpath(node)`: the vehicle stands on the node.
pub(crate) fn attach(world: &mut World, vehicle: u64, node: u64) -> Result<(), String> {
    if paths(world).nodes.is_none() {
        let nodes = build_nodes(world);
        paths(world).nodes = Some(nodes);
    }
    let table = paths(world).nodes.take().unwrap_or(Vec::new());
    let index = table
        .iter()
        .position(|candidate| candidate.object == node)
        .ok_or("attachpath: not a vehicle node");
    let run = index.map(|index| {
        let here = &table[index];
        PathRun {
            track: Track {
                node: index,
                prev: index,
                speed: here.speed,
                lookahead: here.lookahead,
                rotate: f32::from(u8::from(here.rotates)),
                origin: here.origin,
                angles: here.angles,
                ..Track::default()
            },
            previous_angles: here.angles,
            ..PathRun::default()
        }
    });
    paths(world).nodes = Some(table);
    paths(world).runs.insert(vehicle, run?);
    Ok(())
}

/// `vehicle startpath()`.
pub(crate) fn start(world: &mut World, vehicle: u64) -> Result<(), String> {
    match paths(world).runs.get_mut(&vehicle) {
        Some(run) => {
            run.running = true;
            Ok(())
        }
        None => Err("Can't start path on a vehicle that hasn't been attached".into()),
    }
}

/// `setspeed` on a vehicle on its path: what it speeds to, and how fast.
pub(crate) fn set_path_speed(world: &mut World, vehicle: u64, speed: f32, rate: f32) {
    if let Some(run) = paths(world).runs.get_mut(&vehicle) {
        run.mode = 1;
        run.wanted_speed = speed;
        run.speed_rate = rate;
    }
}

/// `resumespeed` on a vehicle on its path.
pub(crate) fn resume_path_speed(world: &mut World, vehicle: u64, rate: f32) {
    if let Some(run) = paths(world).runs.get_mut(&vehicle) {
        run.mode = 2;
        run.speed_rate = rate;
    }
}

/// What a path's frame says of a vehicle: where it stands, and the nodes it
/// reached.
pub(crate) struct PathFrame {
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub speed: f32,
    pub reached: Vec<u64>,
    pub ended: bool,
}

pub(crate) fn is_running(world: &World, vehicle: u64) -> bool {
    world
        .get_resource::<VehiclePaths>()
        .and_then(|paths| paths.runs.get(&vehicle))
        .is_some_and(|run| run.running)
}

/// A frame of a vehicle on its path.
pub(crate) fn frame(world: &mut World, vehicle: u64) -> Option<PathFrame> {
    let table = paths(world).nodes.take()?;
    let result = run_frame(paths(world), &table, vehicle);
    paths(world).nodes = Some(table);
    result
}

fn run_frame(paths: &mut VehiclePaths, nodes: &[Node], vehicle: u64) -> Option<PathFrame> {
    let run = paths.runs.get_mut(&vehicle).filter(|run| run.running)?;
    let previous_speed = run.speed;
    match run.mode {
        0 => run.speed = run.track.speed,
        mode => {
            let goal = if mode == 2 {
                run.track.speed
            } else {
                run.wanted_speed
            };
            let change = run.speed_rate * FRAME_S;
            run.speed = if goal <= previous_speed {
                (previous_speed - change).max(goal)
            } else {
                (previous_speed + change).min(goal)
            };
            if mode == 2 && run.speed == goal {
                run.mode = 0;
            }
        }
    }
    run.fraction = if 0.0 < run.track.speed {
        run.fraction + run.speed / run.track.speed
    } else {
        0.0
    };
    let mut reached = Vec::new();
    let mut ahead = run.track;
    while 1.0 <= run.fraction {
        let before = ahead.node;
        step(nodes, &mut ahead);
        run.fraction -= 1.0;
        if before != ahead.node {
            reached.push(nodes[ahead.node].object);
        }
    }
    run.track = ahead;
    if 0.0 < run.fraction {
        step(nodes, &mut ahead);
    }
    let share = run.fraction;
    let origin = std::array::from_fn(|i| {
        run.track.origin[i] + (ahead.origin[i] - run.track.origin[i]) * share
    });
    let wanted: [f32; 3] = std::array::from_fn(|i| {
        run.track.angles[i] + wrap_180(ahead.angles[i] - run.track.angles[i]) * share
    });
    let rates = [PITCH_ROLL_RATE, YAW_RATE, PITCH_ROLL_RATE];
    let angles: [f32; 3] = std::array::from_fn(|i| {
        let left = wrap_180(wanted[i] - run.previous_angles[i]);
        let turn = left * rates[i] * FRAME_S;
        if 0.05 < left.abs() && turn.abs() <= left.abs() {
            wrap_180(run.previous_angles[i] + turn)
        } else {
            wanted[i]
        }
    });
    run.previous_angles = angles;
    let ended = ahead.stopped && run.track.stopped;
    if ended {
        reached.push(nodes[run.track.node].object);
        run.running = false;
    }
    Some(PathFrame {
        origin,
        angles,
        speed: run.speed,
        reached,
        ended,
    })
}
