//! Black Ops 2's rumbles: a name takes the next of the engine's rumble
//! slots the first time any builtin names it, and plays through IW4L's
//! rumble events.

use super::super::engine::entity_id;
use super::{string, vector};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;

/// The engine's rumble slots end here; slot 0 is no rumble.
const RUMBLE_SLOTS: i32 = 48;
/// The first rumble's configstring, which the overflow error names.
const RUMBLE_CONFIGSTRING: i32 = 0x3f2;

/// A rumble's slot, taken on first use; an empty name has none.
fn index(world: &mut World, name: &str) -> Result<i32, String> {
    if name.is_empty() {
        return Ok(0);
    }
    let mut script = world.resource_mut::<crate::script::MatchScript>();
    if let Some(&index) = script.precached.get(&("rumble", name.to_owned())) {
        return Ok(index);
    }
    let index = 1 + script
        .precached
        .keys()
        .filter(|(kind, _)| *kind == "rumble")
        .count() as i32;
    if index >= RUMBLE_SLOTS {
        return Err(format!(
            "G_FindConfigstringIndex: overflow ({RUMBLE_CONFIGSTRING}): '{name}'"
        ));
    }
    script.precached.insert(("rumble", name.to_owned()), index);
    Ok(index)
}

/// The named rumble's slot, which must exist.
fn named(world: &mut World, args: &[Value]) -> Result<i32, String> {
    let name = string(args, 0)?;
    match index(world, &name)? {
        0 => Err(format!("unknown rumble name '{name}'")),
        index => Ok(index),
    }
}

/// A rumble on the receiving entity.
fn on_entity(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    stop: bool,
) -> Result<Value, String> {
    let id = entity_id(world, receiver)?;
    let index = named(world, args)?;
    super::super::super::rumble::on_entity(world, id, index, stop);
    Ok(Value::Undefined)
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    registry.register(Function, "precacherumble", |world, _, args| {
        if !world.resource::<RoundScript>().loading {
            return Err(
                "precacheRumble must be called before any wait statements in the level script\n"
                    .into(),
            );
        }
        let name = string(args, 0)?;
        index(world, &name)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "playrumbleonentity", |world, receiver, args| {
        on_entity(world, receiver, args, false)
    });
    registry.register(Method, "stoprumble", |world, receiver, args| {
        on_entity(world, receiver, args, true)
    });
    registry.register(Function, "playrumbleonposition", |world, _, args| {
        let index = named(world, args)?;
        let origin = vector(args, 1)?;
        super::super::super::rumble::at_position(world, origin, index);
        Ok(Value::Undefined)
    });
    // gsc_stopallrumbles: a broadcast event that stops every rumble.
    registry.register(Function, "stopallrumbles", |world, _, _| {
        super::super::super::rumble::stop_all(world);
        Ok(Value::Undefined)
    });
}
