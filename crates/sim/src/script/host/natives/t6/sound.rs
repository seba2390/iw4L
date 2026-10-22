//! Black Ops 2's `playsoundwithnotify`: a sound on the entity, and a notify
//! once the sound has played, as long as the client reports it lasts (the
//! engine's wait when no client reported it: 5000 ms).

use super::string;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;
use std::sync::Arc;

/// `playsoundwithnotify(alias, [notify])`: an alias the game has is played
/// on the entity, the clients told to report its length; with a notify, the
/// entity raises it once that length has passed since the report (5000 ms
/// when none comes), a notify still waiting being raised first. An unknown
/// alias plays nothing and sets no notify.
fn play_sound_with_notify(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let id = super::super::engine::entity_id(world, receiver)?;
    let alias = if args.is_empty() {
        String::new()
    } else {
        string(args, 0)?
    };
    let notify: Option<Arc<str>> = if args.len() > 1 {
        Some(string(args, 1)?.into())
    } else {
        None
    };
    let known = !alias.is_empty()
        && crate::frame::FrameWorld::from_world(world)
            .script_sound_exists(&alias)
            .ok_or("t6.sound.aliases: the sound alias catalog is not installed")?;
    if !known {
        return Ok(Value::Undefined);
    }
    let number = world.resource::<RoundScript>().entities[&id].number;
    let origin = super::movement::origin_of(world, id);
    super::super::engine::sound_on_entity(world, number, origin, &alias, notify.is_some());
    if let Some(notify) = notify {
        let now = super::super::super::players::now_ms(world);
        let old = world.resource_mut::<RoundScript>().sound_notifies.insert(
            number,
            crate::script::runtime::SoundNotify {
                receiver: receiver.clone(),
                notify,
                start_ms: now,
                duration_ms: -1,
            },
        );
        if let Some(old) = old {
            crate::script::runtime::raise(world, old.receiver, &old.notify, Vec::new());
        }
    }
    Ok(Value::Undefined)
}

/// `playsoundtoplayer(alias, player)`: an alias the game has plays on the
/// receiver (the event carries the entity, so the sound follows it), sent
/// to that player alone; an unknown alias plays nothing.
fn play_sound_to_player(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let target = super::super::super::args::arg(args, 1)?;
    let target = super::super::engine::entity_id(world, target)?;
    let runtime = world.resource::<RoundScript>();
    let Some(client) = runtime.player_client(target) else {
        let number = runtime.entities[&target].number;
        return Err(format!("entity {number} is not a player"));
    };
    let id = super::super::engine::entity_id(world, receiver)?;
    let alias = string(args, 0)?;
    let known = crate::frame::FrameWorld::from_world(world)
        .script_sound_exists(&alias)
        .ok_or("t6.sound.aliases: the sound alias catalog is not installed")?;
    if !known {
        return Ok(Value::Undefined);
    }
    let number = world.resource::<RoundScript>().entities[&id].number;
    let origin = super::movement::origin_of(world, id);
    super::super::engine::sound_to(
        world,
        crate::EventAudience::Client(crate::ClientId(client)),
        &alias,
        number,
        origin,
    );
    Ok(Value::Undefined)
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(
        Namespace::Method,
        "playsoundwithnotify",
        play_sound_with_notify,
    );
    registry.register(Namespace::Method, "playsoundtoplayer", play_sound_to_player);
    // gsc_soundexists: whether the alias is in the game's sound alias table.
    registry.register(Namespace::Function, "soundexists", |world, _, args| {
        let alias = string(args, 0)?;
        let exists = crate::frame::FrameWorld::from_world(world)
            .script_sound_exists(&alias)
            .ok_or("t6.sound.aliases: the sound alias catalog is not installed")?;
        Ok(Value::Int(exists.into()))
    });
    // gsc_soundgetplaybacktime: the alias's playback time as the engine
    // works it out from its banks (-1 for an alias it lacks).
    registry.register(
        Namespace::Function,
        "soundgetplaybacktime",
        |world, _, args| {
            let alias = string(args, 0)?;
            let ms = crate::frame::FrameWorld::from_world(world)
                .script_sound_playback_ms(&alias)
                .ok_or("t6.sound.aliases: the sound alias catalog is not installed")?;
            Ok(Value::Int(ms))
        },
    );
    registry.register(
        Namespace::Method,
        "playsoundasmaster",
        |world, receiver, args| {
            let id = super::super::engine::entity_id(world, receiver)?;
            let alias = if args.is_empty() {
                String::new()
            } else {
                string(args, 0)?
            };
            let known = crate::frame::FrameWorld::from_world(world)
                .script_sound_exists(&alias)
                .unwrap_or(false);
            if !alias.is_empty() && known {
                let at = super::movement::origin_of(world, id);
                super::super::engine::sound_alias_event(world, at, &alias)?;
            }
            Ok(Value::Undefined)
        },
    );
    // gscm_playsoundontag(alias, [tag]): the sound at the tag (a tag the
    // model lacks is an error), else at the entity; an unknown alias plays
    // nothing.
    registry.register(
        Namespace::Method,
        "playsoundontag",
        |world, receiver, args| {
            let id = super::super::engine::entity_id(world, receiver)?;
            let alias = if args.is_empty() {
                String::new()
            } else {
                string(args, 0)?
            };
            let mut at = match world
                .resource_mut::<RoundScript>()
                .object_field(id, "origin")
            {
                Value::Vector(v) => v,
                _ => [0.0; 3],
            };
            if args.len() > 1 {
                let tag = string(args, 1)?;
                if tag != "tag_origin" {
                    match super::super::super::presence::tag_pose(world, id, &tag) {
                        Some(Some((origin, _))) => at = origin,
                        Some(None) => {
                            let model = match world
                                .resource_mut::<RoundScript>()
                                .object_field(id, "model")
                            {
                                Value::String(model) => model.to_string(),
                                _ => String::new(),
                            };
                            return Err(format!(
                                "tag '{tag}' does not exist on entity with model '{model}'"
                            ));
                        }
                        None => {}
                    }
                }
            }
            let known = crate::frame::FrameWorld::from_world(world)
                .script_sound_exists(&alias)
                .unwrap_or(false);
            if !alias.is_empty() && known {
                super::super::engine::sound_alias_event(world, at, &alias)?;
            }
            Ok(Value::Undefined)
        },
    );
}
