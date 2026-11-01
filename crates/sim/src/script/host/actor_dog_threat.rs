//! Black Ops multiplayer's actor senses and threat, which its dogs run: each
//! frame it checks its sight of the live sentients of the teams it fights,
//! then takes as its enemy the one worth most (the threat score of
//! BlackOpsMP.exe's threat update). A dog never targets an actor,
//! its owner, or anyone `ignoreme`; `specialty_nottargetedbyai` hides a player
//! only with `perk_dogsAttackGhost` off.

use bevy_ecs::prelude::World;

use super::actor_brain;
use super::entities::EntityKind;
use crate::script::runtime::raise;
use crate::script::{RoundScript, Value};

/// A known sentient is forgotten after this long.
const KNOWN_MS: i64 = 10_000;
const VISIBLE_SCORE: i32 = 1000;
const NOTICED_SCORE: i32 = 500;
const REMEMBERED_SCORE: i32 = 250;
/// Added for a visible player, more below this share of its health.
const VISIBLE_PLAYER_SCORE: i32 = 500;
const HURT_PLAYER_SCORE: i32 = 1000;
const HURT_SHARE: f64 = 0.8;
const CLOSE_DIST: f32 = 2500.0;
const CLOSE_SCALE: f32 = 0.0008;
/// Each other attacker on a target, down to `ATTACKERS_FLOOR`.
const ATTACKER_SCORE: i32 = -150;
const ATTACKERS_FLOOR: i32 = -1000;
/// A target in a synced melee with another attacker.
const MELEE_ELSEWHERE_SCORE: i32 = -10_000;

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn int_field(world: &mut World, id: u64, name: &str) -> i32 {
    match super::players::entity_field(world, id, name) {
        Value::Int(n) => n,
        Value::Float(f) => f as i32,
        _ => 0,
    }
}

/// The players the actor may target.
fn candidates(world: &mut World, actor: u64) -> Vec<u64> {
    let ghosts_too = super::actor_nav::dvar_f32(world, "perk_dogsAttackGhost", 1.0) != 0.0;
    let (team, owner, players) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let team = runtime.actor_team(actor);
        let owner = runtime.object_field(actor, "script_owner");
        let players: Vec<u64> = runtime
            .entities
            .iter()
            .filter(|(_, entity)| entity.kind == EntityKind::Player)
            .map(|(&id, _)| id)
            .collect();
        (team, owner, players)
    };
    let free = matches!(&team, Value::String(team) if &**team == "free");
    players
        .into_iter()
        .filter(|&id| {
            if !world.resource::<RoundScript>().live(&id)
                || int_field(world, id, "health") <= 0
                || int_field(world, id, "ignoreme") != 0
                || matches!(owner, Value::Object(owner) if owner == id)
            {
                return false;
            }
            let their = super::players::entity_field(world, id, "team");
            let enemies = match (&team, &their) {
                _ if free => true,
                (Value::String(ours), Value::String(theirs)) => {
                    (&**ours == "axis" && &**theirs == "allies")
                        || (&**ours == "allies" && &**theirs == "axis")
                }
                _ => false,
            };
            enemies
                && (ghosts_too
                    || world
                        .resource::<RoundScript>()
                        .player_client(id)
                        .is_none_or(|client| {
                            !super::natives::t5_perks::has_perk(
                                world,
                                client,
                                "specialty_nottargetedbyai",
                            )
                        }))
        })
        .collect()
}

fn attackers(runtime: &mut RoundScript, target: u64, actor: u64) -> i32 {
    let actors: Vec<u64> = runtime
        .entities
        .iter()
        .filter(|(_, entity)| entity.kind == EntityKind::Actor)
        .map(|(&id, _)| id)
        .collect();
    actors
        .into_iter()
        .filter(|&id| {
            id != actor
                && matches!(runtime.object_field(id, "enemy"), Value::Object(e) if e == target)
        })
        .count() as i32
}

/// The target's worth, of those terms of the executable's score a dog's
/// target reads; `t5.mp.dog_threat` lists the rest.
fn score(world: &mut World, actor: u64, target: u64, current: bool) -> i32 {
    let now = super::players::now_ms(world);
    let (visible, known, known_at) =
        actor_brain::known(world, actor, target).unwrap_or((false, None, [0.0; 3]));
    let recent = known.is_some_and(|at| now - at < KNOWN_MS);
    let health = int_field(world, target, "health");
    let max_health = int_field(world, target, "maxhealth");
    let bias = int_field(world, target, "threatbias");
    let melee = matches!(
        super::players::entity_field(world, target, "syncedmeleetarget"),
        Value::Object(other) if other != actor
    );
    let mut runtime = world.resource_mut::<RoundScript>();
    let origin = match runtime.object_field(actor, "origin") {
        Value::Vector(at) => at,
        _ => [0.0; 3],
    };
    let dist = distance(origin, known_at);
    let others = attackers(&mut runtime, target, actor);
    let mut score = bias;
    if visible {
        score += VISIBLE_SCORE;
    } else if recent {
        score += NOTICED_SCORE;
    } else if current {
        score += REMEMBERED_SCORE;
    }
    if visible {
        score += if f64::from(health) < f64::from(max_health) * HURT_SHARE {
            HURT_PLAYER_SCORE
        } else {
            VISIBLE_PLAYER_SCORE
        };
    }
    if dist < CLOSE_DIST {
        score += (CLOSE_SCALE * (CLOSE_DIST - dist) * (CLOSE_DIST - dist)) as i32;
    }
    score += (others * ATTACKER_SCORE).max(ATTACKERS_FLOOR);
    if melee {
        score += MELEE_ELSEWHERE_SCORE;
    }
    score
}

/// The frame's senses and the enemy they choose; a change of enemy notifies
/// `enemy`, which ends the dog's patrol.
pub(crate) fn update(world: &mut World, actor: u64) {
    if matches!(
        world
            .resource_mut::<RoundScript>()
            .object_field(actor, "ignoreall"),
        Value::Int(n) if n != 0
    ) {
        set_enemy(world, actor, None);
        return;
    }
    let targets = candidates(world, actor);
    let now = super::players::now_ms(world);
    let current = actor_brain::enemy(world, actor);
    let mut best: Option<(u64, i32, bool)> = None;
    for target in targets {
        let seen = actor_brain::can_see_sentient(world, actor, target, 0);
        if seen {
            actor_brain::know_sentient(world, actor, target);
        }
        let known = actor_brain::known(world, actor, target).and_then(|(_, known, _)| known);
        let Some(known) = known else {
            continue;
        };
        let recent = now - known < KNOWN_MS;
        let is_current = current == Some(target);
        if !recent && !is_current {
            continue;
        }
        if best.is_some_and(|(_, _, best_recent)| best_recent && !recent) {
            continue;
        }
        let value = score(world, actor, target, is_current);
        let better = best.is_none_or(|(_, best_value, best_recent)| {
            best_value < value || (!best_recent && recent) || is_current
        });
        if better {
            // The current enemy keeps its place against all but a sighting
            // of a target it does not see.
            let value = if is_current { i32::MAX - 1 } else { value };
            best = Some((target, value, recent));
        }
    }
    set_enemy(world, actor, best.map(|(target, ..)| target));
}

fn set_enemy(world: &mut World, actor: u64, enemy: Option<u64>) {
    if actor_brain::enemy(world, actor) == enemy {
        return;
    }
    world.resource_mut::<RoundScript>().set_object_field(
        actor,
        "enemy",
        enemy.map_or(Value::Undefined, Value::Object),
    );
    if enemy.is_some() {
        raise(world, Value::Object(actor), "enemy", Vec::new());
    }
}
