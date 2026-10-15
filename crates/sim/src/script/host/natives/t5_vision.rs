//! Black Ops' vision builtins. The server's set a client's naked and
//! last-stand visions, as its config strings would; the client scripts' go
//! straight to the local client's vision channels.

use super::super::args::{float, int, string};
use super::player::player;
use crate::frame::FrameWorld;
use crate::script::Namespace::{Function, Method};
use crate::script::client::{ClientOutput, ClientView};
use crate::script::{NativeRegistry, RoundScript, Value};
use crate::world::ClientId;
use crate::{VisionCall, VisionChange};
use bevy_ecs::prelude::World;

/// A transition time when a call gives none.
const DEFAULT_MS: i32 = 1000;

/// The server's vision builtins.
pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Function, "visionsetnaked", |world, _, args| {
        let vision = server_vision(
            world,
            args,
            "USAGE: VisionSetNaked( <visionset name>, <transition time> )\n",
        )?;
        let mut frame = FrameWorld::from_world(world);
        for client in frame.client_ids_sorted() {
            frame.client_meta_mut(client).view_effects.naked_vision = None;
        }
        world.resource_mut::<RoundScript>().engine.naked_vision = Some(vision);
        Ok(Value::Undefined)
    });
    registry.register(Method, "visionsetnaked", |world, receiver, args| {
        let client = player(world, receiver)?;
        let vision = server_vision(
            world,
            args,
            "USAGE: player VisionSetNaked( <visionset name>, <transition time> )\n",
        )?;
        edit_view(world, client, |view| view.naked_vision = Some(vision));
        Ok(Value::Undefined)
    });
    registry.register(Method, "visionsetlaststand", |world, receiver, args| {
        let client = player(world, receiver)?;
        let vision = server_vision(
            world,
            args,
            "USAGE: player visionsetlaststand( <visionset name>, <transition time> )\n",
        )?;
        edit_view(world, client, |view| view.last_stand_vision = Some(vision));
        Ok(Value::Undefined)
    });
}

/// The client scripts' vision builtins.
pub(crate) fn register_client(registry: &mut NativeRegistry) {
    registry.register(Function, "visionsetnaked", |world, _, args| {
        let (name, duration_ms) =
            client_vision(args, 2, "VisionSetNaked() called with wrong params.\n")?;
        client_call(world, VisionCall::ClientNaked { name, duration_ms });
        Ok(Value::Undefined)
    });
    // The naked vision the server set, not one a client script set.
    registry.register(Function, "getvisionsetnaked", |world, _, args| {
        int(args, 0)?;
        let view = world.resource::<ClientView>();
        Ok(Value::string(&view.locals[0].player.naked_vision))
    });
    registry.register(Function, "visionsetdamage", |world, _, args| {
        local_index(args)?;
        let on = int(args, 1)? != 0;
        let name = string(args, 2)?;
        let duration_ms = match args.len() {
            3 => DEFAULT_MS,
            4 => super::t5_client::seconds_to_msec(float(args, 3)?),
            _ => return Err("VisionSetDamage() called with wrong params.\n".into()),
        };
        client_call(
            world,
            VisionCall::ClientDamage {
                on,
                name,
                duration_ms,
            },
        );
        Ok(Value::Undefined)
    });
    registry.register(Function, "visionsetunderwater", |world, _, args| {
        let (name, duration_ms) = client_vision(
            args,
            2,
            "visionsetunderwater() called with wrong params.\n",
        )?;
        client_call(world, VisionCall::ClientUnderwater { name, duration_ms });
        Ok(Value::Undefined)
    });
}

/// A server vision call's name and transition: one or two arguments, the
/// time in seconds rounded to milliseconds.
fn server_vision(world: &World, args: &[Value], usage: &str) -> Result<VisionChange, String> {
    let duration_ms = match args.len() {
        1 => DEFAULT_MS,
        2 => super::t5_client::seconds_to_msec(float(args, 1)?),
        _ => return Err(usage.into()),
    };
    Ok(VisionChange {
        name: string(args, 0)?,
        duration_ms,
        set_ms: crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick),
    })
}

/// A client vision call's name and transition: the local client, the name
/// at `name_at`, and an optional time after it.
fn client_vision(args: &[Value], name_at: usize, refusal: &str) -> Result<(String, i32), String> {
    let duration_ms = if args.len() == name_at {
        DEFAULT_MS
    } else if args.len() == name_at + 1 {
        super::t5_client::seconds_to_msec(float(args, name_at)?)
    } else {
        return Err(refusal.into());
    };
    local_index(args)?;
    Ok((string(args, name_at - 1)?, duration_ms))
}

fn local_index(args: &[Value]) -> Result<(), String> {
    let local = int(args, 0)?;
    if local != 0 {
        return Err(format!(
            "Trying to get a local client index for a client '{local}' that is not a local client."
        ));
    }
    Ok(())
}

fn client_call(world: &mut World, call: VisionCall) {
    world.resource_mut::<ClientOutput>().vision_call(call);
}

fn edit_view(world: &mut World, client: u32, edit: impl FnOnce(&mut crate::ViewEffects)) {
    let mut frame = FrameWorld::from_world(world);
    if frame.client_meta(ClientId(client)).is_some() {
        edit(&mut frame.client_meta_mut(ClientId(client)).view_effects);
    }
}
