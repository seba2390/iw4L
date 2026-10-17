//! Black Ops 2's client touch pass for the local client's player: each
//! client trigger the player's box touches is notified `touch` with the
//! player and the player `touch` with the trigger at once, and the touch is
//! queued; the client's next frame notifies each queued trigger `trigger`
//! with the player, while both are still there.

use super::ambient::ClientTriggers;
use super::game::ClientGame;
use crate::script::host::client_scripts::with_server;
use crate::script::{RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};

/// A trigger's spawnflag that keeps players out of it.
const NOT_PLAYER: i32 = 8;

/// The touches the last pass queued: trigger, then the entity touching it.
#[derive(Resource, Default)]
struct TouchQueue(Vec<(u64, u64)>);

/// The player's touch box: 15 units out from its origin and 70 up.
fn player_box(origin: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    (
        [origin[0] - 15.0, origin[1] - 15.0, origin[2]],
        [origin[0] + 15.0, origin[1] + 15.0, origin[2] + 70.0],
    )
}

/// The client triggers a player touches this frame: object and entity
/// number.
pub(super) fn touched(client: &mut World, player_number: i32) -> Vec<(u64, i32)> {
    let Some(origin) = with_server(client, |server| {
        crate::frame::FrameWorld::from_world(server)
            .player(crate::ClientId(player_number as u32))
            .map(|ps| ps.origin)
    })
    .flatten() else {
        return Vec::new();
    };
    let (mins, maxs) = player_box(origin);
    let triggers = client
        .get_resource::<ClientTriggers>()
        .map_or_else(Vec::new, |t| t.0.clone());
    let mut touched = Vec::new();
    for (object, flags) in triggers {
        if flags & NOT_PLAYER != 0 {
            continue;
        }
        let mut runtime = client.resource_mut::<RoundScript>();
        if !runtime.live(&object) {
            continue;
        }
        let Some(number) = runtime.entities.get(&object).map(|e| e.number) else {
            continue;
        };
        let at = match runtime.object_field(object, "origin") {
            Value::Vector(v) => v,
            _ => [0.0; 3],
        };
        let model = match runtime.object_field(object, "model") {
            Value::String(text) => text.to_string(),
            _ => String::new(),
        };
        let brush = model.strip_prefix('*').and_then(|n| n.parse::<u32>().ok());
        let trigger_model = model.strip_prefix('?').and_then(|n| n.parse::<u32>().ok());
        let touches = with_server(client, |server| {
            crate::script::host::triggers::map_volume_touches(
                &crate::frame::FrameWorld::from_world(server),
                at,
                brush,
                trigger_model,
                mins,
                maxs,
            )
        })
        .unwrap_or(false);
        if touches {
            touched.push((object, number));
        }
    }
    touched
}

/// The client frame's touches: the queued `trigger` notifies of the frame
/// before, then the local player's touch pass.
// t6.csc.touch.others: only the local player's touches are taken; the
// engine's pass also runs for the snapshot's other players, its actors (by
// team) and its vehicles.
pub(super) fn frame(client: &mut World, now: i64) {
    let queued = client
        .get_resource_mut::<TouchQueue>()
        .map(|mut queue| std::mem::take(&mut queue.0))
        .unwrap_or_else(Vec::new);
    for (trigger, other) in queued {
        let live = {
            let runtime = client.resource::<RoundScript>();
            runtime.live(&trigger) && runtime.live(&other)
        };
        if live {
            crate::script::runtime::notify_now(
                client,
                Value::Object(trigger),
                "trigger",
                vec![Value::Object(other)],
                now,
            );
        }
    }
    let Some((number, player)) = client.get_resource::<ClientGame>().and_then(|game| {
        let number = game.local_player?;
        Some((number, game.entities.get(&number)?.object))
    }) else {
        return;
    };
    for (trigger, _) in touched(client, number) {
        crate::script::runtime::notify_now(
            client,
            Value::Object(trigger),
            "touch",
            vec![Value::Object(player)],
            now,
        );
        crate::script::runtime::notify_now(
            client,
            Value::Object(player),
            "touch",
            vec![Value::Object(trigger)],
            now,
        );
        client
            .get_resource_or_insert_with(TouchQueue::default)
            .0
            .push((trigger, player));
    }
}
