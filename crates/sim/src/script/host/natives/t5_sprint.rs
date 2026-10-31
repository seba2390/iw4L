//! Black Ops multiplayer's sprint builtins: the scripts give each body
//! type its sprint length and recharge pause (`_armor.gsc`).

use super::super::args::float;
use crate::script::Namespace::Method;
use crate::script::{NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;

/// The longest duration either builtin takes, in seconds.
const MAX_SECONDS: f32 = 16383.0;

/// `player setSprintDuration(seconds)` or `setSprintCooldown(seconds)`:
/// the player state keeps it in ms.
fn set(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    cooldown: bool,
) -> Result<Value, String> {
    let builtin = if cooldown {
        "setsprintcooldown"
    } else {
        "setsprintduration"
    };
    let id = super::engine::entity_id(world, receiver).map_err(|_| "not an entity".to_owned())?;
    let runtime = world.resource::<RoundScript>();
    let client = runtime.player_client(id).ok_or_else(|| {
        let number = runtime.entities.get(&id).map_or(-1, |e| e.number);
        format!("entity {number} is not a player")
    })?;
    if args.len() != 1 {
        return Err(format!("USAGE: <player> {builtin}( <duration> )\n"));
    }
    let seconds = float(args, 0)?;
    if seconds < 0.0 {
        return Err(format!("{builtin}: duration must be >= 0"));
    }
    if MAX_SECONDS <= seconds {
        return Err(format!(
            "{builtin}: duration must be < {}",
            MAX_SECONDS as i32
        ));
    }
    let ms = (seconds * 1000.0) as i32;
    let (duration, cooldown) = if cooldown {
        (None, Some(ms))
    } else {
        (Some(ms), None)
    };
    crate::game_move::set_sprint_times(
        &mut crate::frame::FrameWorld::from_world(world),
        crate::world::ClientId(client),
        duration,
        cooldown,
    );
    Ok(Value::Undefined)
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Method, "setsprintduration", |world, receiver, args| {
        set(world, receiver, args, false)
    });
    registry.register(Method, "setsprintcooldown", |world, receiver, args| {
        set(world, receiver, args, true)
    });
}
