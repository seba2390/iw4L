//! The hellhound's (`zombie_dog` species) exposed think, as Black Ops runs it
//! for an actor with an enemy outside Dead Ops Arcade: it aims at where the
//! enemy will be a quarter second from now, claims one of four attack spots
//! around it (its code goal), and bites once it is close and on its spot's
//! side. Its path, to the node nearest the enemy, is the brain think's.

use bevy_ecs::prelude::World;

use crate::frame::FrameWorld;
use crate::script::{RoundScript, Value};

/// How far ahead a moving enemy's position is predicted, in seconds.
const PREDICT_SECONDS: f32 = 0.25;
/// Added to `meleeAttackDist` for the bite's reach, once more for a moving enemy.
const REACH_PAD: f32 = 15.0;
/// A player some other attacker is mauling is aimed at this much higher.
const SYNCED_MELEE_LIFT: f32 = 64.0;
/// Farther above or below its target than this, the dog is not close.
const CLOSE_HEIGHT: f32 = 80.0;
/// The four attack spots lie along these directions from the target.
const SPOT_DIRECTIONS: [[f32; 2]; 4] = [[1.0, 0.0], [0.0, 1.0], [-1.0, 0.0], [0.0, -1.0]];
/// The dog's box for the attack-spot traces.
const DOG_MINS: [f32; 3] = [-15.0, -15.0, 18.0];
const DOG_MAXS: [f32; 3] = [15.0, 15.0, 72.0];
/// How far below an attack spot the ground may lie.
const SPOT_DROP: f32 = 90.0;
/// A spot stands this far above the ground found under it.
const SPOT_LIFT: f32 = 1.0;
/// The dog bites only from within 45 degrees of its spot's direction.
const SPOT_FACING_DOT: f32 = 0.707;
/// The actor clip mask.
const MASK_ACTOR_SOLID: u32 = 0x0282_0011;

/// An enemy's four attack spots, each held by the dog that claimed it.
pub(crate) type AttackSpots = [Option<u64>; 4];

/// The hellhound's bite test against `enemy`: whether it should bite now,
/// and the attack spot it claimed, which becomes its code goal.
pub(crate) fn think(world: &mut World, actor: u64, enemy: u64) -> (bool, Option<[f32; 3]>) {
    let Some(dog) = vector(world, actor, "origin") else {
        return (false, None);
    };
    let (target, reach) = predicted_target(world, actor, enemy);
    let height = dog[2] - target[2];
    let close = height * height <= CLOSE_HEIGHT * CLOSE_HEIGHT
        && horizontal_sq(dog, target) <= reach * reach;
    let spot = claim_spot(world, actor, enemy, dog, target);
    let bite = match spot {
        Some(spot) => close && spot_facing(spot, target, dog) > SPOT_FACING_DOT,
        None => close && clear(world, dog, target),
    };
    (bite, spot)
}

/// Where the dog aims, and how far its bite reaches.
fn predicted_target(world: &mut World, actor: u64, enemy: u64) -> ([f32; 3], f32) {
    let mut target = vector(world, enemy, "origin").unwrap_or([0.0; 3]);
    let mut reach = float_field(world, actor, "meleeattackdist") + REACH_PAD;
    let Some(client) = world.resource::<RoundScript>().player_client(enemy) else {
        return (target, reach);
    };
    let velocity = FrameWorld::from_world(world)
        .player(crate::ClientId(client))
        .map_or([0.0; 3], |ps| ps.velocity);
    if velocity[0] * velocity[0] + velocity[1] * velocity[1] > 1.0 {
        for (axis, speed) in velocity.iter().enumerate() {
            target[axis] += speed * PREDICT_SECONDS;
        }
        reach += REACH_PAD;
    }
    if !matches!(
        super::players::entity_field(world, enemy, "syncedmeleetarget"),
        Value::Undefined
    ) {
        target[2] += SYNCED_MELEE_LIFT;
    }
    (target, reach)
}

/// Gives up the dog's spot and claims the free spot around `target` best
/// facing it whose box is clear, reaches the target and stands on ground.
/// A spot another dog holds is taken only by a dog nearer to it.
fn claim_spot(
    world: &mut World,
    actor: u64,
    enemy: u64,
    dog: [f32; 3],
    target: [f32; 3],
) -> Option<[f32; 3]> {
    let away = normalized_2d([dog[0] - target[0], dog[1] - target[1]]);
    let score = SPOT_DIRECTIONS.map(|direction| dot_2d(direction, away));
    // Highest facing first, by insertion; a later spot goes before an
    // earlier one that faces equally well.
    let mut order: Vec<usize> = Vec::with_capacity(4);
    for index in 0..4 {
        let at = order
            .iter()
            .position(|&sorted| score[sorted] <= score[index])
            .unwrap_or(order.len());
        order.insert(at, index);
    }
    let mut spots = world
        .resource::<RoundScript>()
        .dog_spots
        .get(&enemy)
        .copied()
        .unwrap_or([None; 4]);
    for held in &mut spots {
        if *held == Some(actor) {
            *held = None;
        }
    }
    let distance = float_field(world, actor, "meleeattackdist");
    let mut claimed = None;
    for index in order {
        let direction = SPOT_DIRECTIONS[index];
        let mut spot = [
            target[0] + direction[0] * distance,
            target[1] + direction[1] * distance,
            target[2],
        ];
        if let Some(holder) = spots[index]
            && let Some(at) = vector(world, holder, "origin")
            && horizontal_sq(dog, spot) >= horizontal_sq(at, spot)
        {
            continue;
        }
        let frame = FrameWorld::from_world(world);
        let inside = frame.trace_clip(spot, spot, DOG_MINS, DOG_MAXS, MASK_ACTOR_SOLID);
        let toward = if inside.startsolid == 0 && inside.allsolid == 0 {
            frame.trace_clip(spot, target, DOG_MINS, DOG_MAXS, MASK_ACTOR_SOLID)
        } else {
            inside
        };
        if toward.fraction != 1.0 || toward.startsolid != 0 || toward.allsolid != 0 {
            continue;
        }
        let below = [spot[0], spot[1], spot[2] - SPOT_DROP];
        let ground = frame.trace_clip(spot, below, DOG_MINS, DOG_MAXS, MASK_ACTOR_SOLID);
        if ground.fraction >= 1.0 || ground.startsolid != 0 || ground.allsolid != 0 {
            continue;
        }
        spot[2] = spot[2] - SPOT_DROP * ground.fraction + SPOT_LIFT;
        spots[index] = Some(actor);
        claimed = Some(spot);
        break;
    }
    world
        .resource_mut::<RoundScript>()
        .dog_spots
        .insert(enemy, spots);
    claimed
}

/// How squarely the dog stands on its spot's side of the target.
fn spot_facing(spot: [f32; 3], target: [f32; 3], dog: [f32; 3]) -> f32 {
    dot_2d(
        normalized_2d([spot[0] - target[0], spot[1] - target[1]]),
        normalized_2d([dog[0] - target[0], dog[1] - target[1]]),
    )
}

/// Nothing solid lies between the dog and its target, for the dog's box.
fn clear(world: &mut World, dog: [f32; 3], target: [f32; 3]) -> bool {
    let trace =
        FrameWorld::from_world(world).trace_clip(dog, target, DOG_MINS, DOG_MAXS, MASK_ACTOR_SOLID);
    trace.fraction >= 1.0 && trace.allsolid == 0 && trace.startsolid == 0
}

/// Lets a dog's claims go when it is removed.
pub(crate) fn release(runtime: &mut RoundScript, actor: u64) {
    for spots in runtime.dog_spots.values_mut() {
        for held in spots.iter_mut() {
            if *held == Some(actor) {
                *held = None;
            }
        }
    }
}

fn normalized_2d(v: [f32; 2]) -> [f32; 2] {
    let length = (v[0] * v[0] + v[1] * v[1]).sqrt();
    let scale = if -length >= 0.0 { 1.0 } else { 1.0 / length };
    [v[0] * scale, v[1] * scale]
}

fn dot_2d(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

fn horizontal_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]) * (a[0] - b[0]) + (a[1] - b[1]) * (a[1] - b[1])
}

fn vector(world: &mut World, id: u64, name: &str) -> Option<[f32; 3]> {
    match super::players::entity_field(world, id, name) {
        Value::Vector(v) => Some(v),
        _ => None,
    }
}

fn float_field(world: &mut World, id: u64, name: &str) -> f32 {
    match world.resource_mut::<RoundScript>().object_field(id, name) {
        Value::Float(f) => f,
        Value::Int(n) => n as f32,
        _ => 0.0,
    }
}
