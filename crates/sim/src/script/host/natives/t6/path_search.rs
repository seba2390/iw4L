//! Black Ops 2's path search: A* over the path nodes' links from the node
//! nearest the actor to the node nearest its goal, each step costing the
//! link's length and, the first time the search reaches a node, that node's
//! own cost: what other actors' paths hold of it as their next traversal,
//! a random share, a chokepoint cost, and the overlap cost again when the
//! actor stands within the node's use distance.

use std::collections::BTreeMap;

use super::super::super::actor_nav::{ActorPaths, LinkEdits};
use crate::script::Value;
use bevy_ecs::prelude::World;

/// Within this of its goal node (squared) an actor's search takes no random
/// share.
const RANDOM_REACH_SQ: f32 = 262_144.0;
/// The random share's step per percent the random draw leaves.
const RANDOM_STEP: f32 = 5.12;
/// The node spawnflag marking a chokepoint.
const CHOKEPOINT: u32 = 0x1000;

fn dvar_text(world: &World, name: &str) -> String {
    world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get(name)
        .cloned()
        .or_else(|| super::dvars::lookup(name).map(|spec| spec.default.to_owned()))
        .unwrap_or_else(String::new)
}

fn int_field(world: &mut World, id: u64, name: &str) -> i32 {
    match world
        .resource_mut::<crate::script::RoundScript>()
        .object_field(id, name)
    {
        Value::Int(n) => n,
        Value::Float(f) => f as i32,
        _ => 0,
    }
}

/// What one search adds at the nodes it reaches: four random shares and
/// four chokepoint costs taken in turn, and the cost of each actor a node
/// holds.
struct NodeCosts {
    random: [f32; 4],
    chokepoint: [f32; 4],
    overlap: f32,
    turn: usize,
}

impl NodeCosts {
    /// The random shares are drawn when the actor is more than 512 from its
    /// goal node, each a draw modulo its `pathrandompercent` in steps of
    /// 5.12; with `usechokepoints`, three of the four turns (one drawn
    /// out, of a random mask) cost `ai_pathChokePointCost`.
    fn draw(world: &mut World, id: u64, from: [f32; 3], goal_node: [f32; 3]) -> Self {
        let far: f32 = (0..3).map(|i| (goal_node[i] - from[i]).powi(2)).sum();
        let percent = if far > RANDOM_REACH_SQ {
            int_field(world, id, "pathrandompercent")
        } else {
            0
        };
        let mut random = [0.0; 4];
        if percent != 0 {
            for share in &mut random {
                let draw = (super::next_rand(world) >> 17) as i32;
                *share = (draw % percent) as f32 * RANDOM_STEP;
            }
        }
        let mut chokepoint = [0.0; 4];
        if int_field(world, id, "usechokepoints") != 0 {
            let mask = super::next_rand(world) >> 17;
            let out = (super::next_rand(world) >> 17) & 3;
            let mask = mask & !(1 << out);
            let cost = super::atof(&dvar_text(world, "ai_pathChokePointCost"));
            for (turn, slot) in chokepoint.iter_mut().enumerate() {
                if mask & (1 << turn) != 0 {
                    *slot = cost;
                }
            }
        }
        Self {
            random,
            chokepoint,
            overlap: super::atof(&dvar_text(world, "ai_pathNegotiationOverlapCost")),
            turn: 0,
        }
    }
}

/// A node the search has reached: the cost to it, its distance to the goal,
/// its own cost, and the node it was reached from.
#[derive(Clone, Copy)]
struct Reached {
    cost: f32,
    estimate: f32,
    own: f32,
    from: Option<u16>,
}

/// The node sequence from `start` to `goal` the engine's search finds for
/// actor `id` standing at `from` and heading for `goal_at`; `holding` is how
/// many actors' paths hold each node as their next traversal.
pub(super) fn route(
    world: &mut World,
    id: u64,
    paths: &ActorPaths,
    edits: &LinkEdits,
    holding: &BTreeMap<u16, u32>,
    (start, goal): (u16, u16),
    (from, goal_at): ([f32; 3], [f32; 3]),
) -> Option<Vec<u16>> {
    let nodes = paths.nodes();
    let mut costs = NodeCosts::draw(world, id, from, nodes.get(usize::from(goal))?.origin);
    let distance = |a: [f32; 3], b: [f32; 3]| -> f32 {
        (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f32>().sqrt()
    };
    let mut reached: BTreeMap<u16, Reached> = BTreeMap::new();
    reached.insert(
        start,
        Reached {
            cost: 0.0,
            estimate: 0.0,
            own: 0.0,
            from: None,
        },
    );
    // Open nodes by cost and estimate, a node going ahead of those that
    // tie with it, as the engine inserts them.
    let mut open: Vec<u16> = vec![start];
    while !open.is_empty() {
        let node = open.remove(0);
        if node == goal {
            let mut route = vec![goal];
            let mut cursor = goal;
            while let Some(previous) = reached[&cursor].from {
                route.push(previous);
                cursor = previous;
            }
            route.reverse();
            return Some(route);
        }
        let at = reached[&node].cost;
        for (next, length, _) in paths.links(node, edits) {
            let target = &nodes[usize::from(next)];
            let total = match reached.get(&next) {
                Some(known) => {
                    let total = at + length + known.own;
                    if known.cost <= total {
                        continue;
                    }
                    open.retain(|&n| n != next);
                    total
                }
                None => {
                    let turn = costs.turn;
                    costs.turn = (turn + 1) % 4;
                    let held = holding.get(&next).copied().unwrap_or(0);
                    let mut own = held as f32 * costs.overlap + costs.random[turn];
                    let fields = target.fields.as_ref();
                    let use_sq = fields.map_or(0.0, |f| f.min_use_dist_sq);
                    let near: f32 = (0..3).map(|i| (from[i] - target.origin[i]).powi(2)).sum();
                    if use_sq > 1.0 && near < use_sq {
                        own += costs.overlap;
                    }
                    if fields.is_some_and(|f| f.spawnflags & CHOKEPOINT != 0) {
                        own += costs.chokepoint[turn];
                    }
                    reached.insert(
                        next,
                        Reached {
                            cost: 0.0,
                            estimate: distance(goal_at, target.origin),
                            own,
                            from: None,
                        },
                    );
                    at + length + own
                }
            };
            let entry = reached.get_mut(&next)?;
            entry.cost = total;
            entry.from = Some(node);
            let rank = total + entry.estimate;
            let place = open
                .iter()
                .position(|n| {
                    let other = &reached[n];
                    other.cost + other.estimate >= rank
                })
                .unwrap_or(open.len());
            open.insert(place, next);
        }
    }
    None
}
