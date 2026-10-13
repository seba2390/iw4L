//! Black Ops 2's `playsoundwithnotify`: a sound at the entity, and a notify
//! once the sound's length has passed.

use super::string;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;
use std::sync::Arc;

/// How long the engine waits when no length was reported for the sound.
const UNREPORTED_LENGTH_MS: i64 = 5000;

/// Each entity's notify waiting on its sound.
#[derive(Resource, Default)]
struct SoundWaits(BTreeMap<u64, Arc<str>>);

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
    if alias.is_empty() {
        return Ok(Value::Undefined);
    }
    let origin = match world
        .resource_mut::<RoundScript>()
        .object_field(id, "origin")
    {
        Value::Vector(origin) => origin.map(f32::trunc),
        _ => [0.0; 3],
    };
    super::super::engine::sound_alias_event(world, origin, &alias)?;
    let Some(notify) = notify else {
        return Ok(Value::Undefined);
    };
    // A notify still waiting is raised now, before the new one waits.
    let waiting = world
        .get_resource_or_insert_with(SoundWaits::default)
        .0
        .insert(id, Arc::clone(&notify));
    if let Some(old) = waiting {
        let pending = {
            let mut runtime = world.resource_mut::<RoundScript>();
            let before = runtime.timed_notifies.len();
            runtime.timed_notifies.retain(|(_, target, name, _)| {
                !(matches!(target, Value::Object(o) if *o == id) && *name == old)
            });
            runtime.timed_notifies.len() != before
        };
        if pending {
            crate::script::runtime::raise(world, receiver.clone(), &old, Vec::new());
        }
    }
    let at = super::super::super::players::now_ms(world) + UNREPORTED_LENGTH_MS;
    world.resource_mut::<RoundScript>().timed_notifies.push((
        at,
        receiver.clone(),
        notify,
        Vec::new(),
    ));
    Ok(Value::Undefined)
}

/// `playsoundtoplayer`: the sound at the receiver, sent to that player
/// alone.
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
    let origin = super::movement::origin_of(world, id);
    super::super::engine::sound_to_client(world, crate::ClientId(client), &alias, origin);
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
