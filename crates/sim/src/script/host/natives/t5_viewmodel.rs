//! Black Ops' builtins for the player's viewmodel and its rumble.

use crate::frame::FrameWorld;
use crate::script::Namespace::{Function, Method};
use crate::script::{NativeRegistry, Value};
use crate::world::ClientId;
use bevy_ecs::prelude::World;

pub(crate) fn register(registry: &mut NativeRegistry) {
    // `player HideViewModel()` / `ShowViewModel()`: a reload in progress
    // stops its sound and both hands go back to ready (hide only), then the
    // server tells the player's client, which stops drawing (or draws
    // again) its viewmodel; the viewmodel goes on animating.
    registry.register(Method, "hideviewmodel", |world, receiver, _| {
        hide_viewmodel(world, receiver, true)
    });
    registry.register(Method, "showviewmodel", |world, receiver, _| {
        hide_viewmodel(world, receiver, false)
    });
    // Black Ops on PC has no rumble: every rumble builtin is one empty
    // function, which reads none of its arguments.
    for name in [
        "playrumbleonentity",
        "playrumblelooponentity",
        "stoprumble",
    ] {
        registry.register(Method, name, |_, _, _| Ok(Value::Undefined));
    }
    for name in [
        "playrumbleonposition",
        "stopallrumbles",
        "precacherumble",
    ] {
        registry.register(Function, name, |_, _, _| Ok(Value::Undefined));
    }
}

fn hide_viewmodel(world: &mut World, receiver: &Value, hide: bool) -> Result<Value, String> {
    let id = ClientId(super::t5::player_id(world, receiver)?);
    crate::game_move::hide_viewmodel(&mut FrameWorld::from_world(world), id, hide);
    Ok(Value::Undefined)
}

/// The client's: the same empty rumble function, and `precacheRumble`
/// answering 0.
pub(crate) fn register_client(registry: &mut NativeRegistry) {
    for name in [
        "playrumbleonentity",
        "playrumblelooponentity",
        "stoprumble",
    ] {
        registry.register(Method, name, |_, _, _| Ok(Value::Undefined));
    }
    registry.register(Function, "playrumbleonposition", |_, _, _| {
        Ok(Value::Undefined)
    });
    registry.register(Function, "precacherumble", |_, _, _| Ok(Value::Int(0)));
}
