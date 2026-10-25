//! The game's crosshair scan at the end of each server frame: a line from
//! each player's eye along the view, and what it meets, for the game's
//! movement rules to mark in the player's state.

use std::cell::RefCell;

use bevy_ecs::prelude::World;
use game_api::movement::{CrosshairHit, CrosshairHitKind, CrosshairScan};

use super::entities::EntityKind;
use super::natives::engine::{TraceIgnore, collider_entity, entity_trace};
use crate::bullet_collision::{ColliderId, TraceOutcome};
use crate::frame::FrameWorld;
use crate::script::{RoundScript, Value};

pub(crate) fn scan_players(world: &mut World) {
    let request = world.resource::<crate::step::StepRequest>();
    if !request.reason.advances_authority_world() {
        return;
    }
    let level_time = crate::level_time_ms(request.tick);
    let (movement, content, ids) = {
        let frame = FrameWorld::from_world(world);
        let movement = match frame.bootstrap_ref().mode.map(|mode| mode.movement) {
            Some(game_api::Rule::Known(game_api::MovementRules::Game(movement))) => movement,
            _ => return,
        };
        (movement, frame.content(), frame.client_ids_sorted())
    };
    let dvars = world.resource::<crate::script::MatchScript>().dvars.clone();
    let dvar = |name: &str| {
        dvars
            .get(&name.to_ascii_lowercase())
            .or_else(|| dvars.get(name))
            .cloned()
    };
    for id in ids {
        let Some(ps) = FrameWorld::from_world(world).player(id).copied() else {
            continue;
        };
        let mut player = ps.move_player(id.0 as i32, false);
        {
            let cell = RefCell::new(&mut *world);
            let trace = |start, end, mask| hit(&mut cell.borrow_mut(), id, start, end, mask);
            let weapon = |weapon: u32| content.game_weapon(weapon);
            movement.crosshair_scan(
                &mut player,
                &CrosshairScan {
                    trace: &trace,
                    weapon: &weapon,
                    dvar: &dvar,
                    level_time,
                },
            );
        }
        if let Some(ps) = FrameWorld::from_world(world).player_mut(id) {
            ps.store_move_player(&player);
        }
    }
}

/// What the line meets: an actor, a player or another entity, with its
/// origin and team.
fn hit(
    world: &mut World,
    client: crate::ClientId,
    start: [f32; 3],
    end: [f32; 3],
    mask: u32,
) -> Option<(CrosshairHit, f32)> {
    let ignore = TraceIgnore {
        client: Some(client),
        ..TraceIgnore::default()
    };
    let TraceOutcome::Hit {
        fraction, collider, ..
    } = entity_trace(world, start, end, mask, ignore)
    else {
        return None;
    };
    if let ColliderId::World { .. } = collider {
        return None;
    }
    let target = collider_entity(world, collider);
    let own = world
        .resource::<RoundScript>()
        .players
        .get(&client.0)
        .map(|slot| slot.object);
    let (id, number, kind) = {
        let runtime = world.resource::<RoundScript>();
        let (id, entity) = runtime.entity(&target)?;
        (id, entity.number, entity.kind.clone())
    };
    let own_team = own.map(|own| team(world, own));
    let mut allied = |id: u64| {
        let theirs = team(world, id);
        own_team
            .as_deref()
            .is_some_and(|own| !own.is_empty() && own == theirs.as_str())
    };
    let kind = match kind {
        EntityKind::Actor => CrosshairHitKind::Actor { allied: allied(id) },
        EntityKind::Player => CrosshairHitKind::Player { allied: allied(id) },
        _ => CrosshairHitKind::Other,
    };
    let origin = match collider {
        ColliderId::Player { client, .. } => FrameWorld::from_world(world)
            .player(client)
            .map_or([0.0; 3], |ps| ps.origin),
        _ => match world
            .resource_mut::<RoundScript>()
            .object_field(id, "origin")
        {
            Value::Vector(origin) => origin,
            _ => [0.0; 3],
        },
    };
    Some((
        CrosshairHit {
            entity: number,
            origin,
            kind,
        },
        fraction,
    ))
}

fn team(world: &mut World, object: u64) -> String {
    match world
        .resource_mut::<RoundScript>()
        .object_field(object, "team")
    {
        Value::String(team) => team.to_string(),
        _ => String::new(),
    }
}
