//! Black Ops 2's client gumps: the zones a map leaves some of its models
//! to, which the client scripts load into the local client's slots
//! (`loadgump`) and are told of when each load is done (`gump_loaded`).

use super::super::{int, string};
use super::game::LOCAL_CLIENT;
use crate::script::host::client_scripts::with_server;
use crate::script::{NativeRegistry, Value};
use bevy_ecs::prelude::{Resource, World};

/// The local client's gump loads: the engine's done-load event, which the
/// client's next frame consumes.
#[derive(Resource, Default)]
struct GumpLoads {
    done: bool,
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use crate::script::Namespace::Function;

    // loadgump(name, slot): a non-empty name goes into the slot. The engine
    // flushes the slot's gump and loads the zone, unless the slot or another
    // slot already holds that gump (the slots then share it); either way its
    // load is done after the call.
    registry.register(Function, "loadgump", |world, _, args| {
        let name = string(args, 0)?;
        if name.is_empty() {
            return Ok(Value::Undefined);
        }
        let slot = int(args, 1)?;
        let at = usize::try_from(slot)
            .ok()
            .filter(|&at| at < crate::GUMP_SLOTS)
            .ok_or_else(|| {
                format!(
                    "t6.csc.gump.slot: the engine has no gump slot {slot}, and reads past its slots"
                )
            })?;
        with_server(world, |server| {
            let mut frame = crate::frame::FrameWorld::from_world(server);
            let client_id = crate::ClientId(LOCAL_CLIENT as u32);
            if frame.client_meta(client_id).is_some() {
                frame
                    .client_meta_mut(client_id)
                    .view_effects
                    .client_screen
                    .gumps[at] = Some(name);
            }
        });
        world.get_resource_or_insert_with(GumpLoads::default).done = true;
        Ok(Value::Undefined)
    });
}

/// The engine's once-a-frame check of its done-load event (an auto-reset
/// event, so loads done since the last check are told once): the client's
/// level is notified `gump_loaded` with 1.
pub(super) fn frame(client: &mut World, now: i64) {
    let Some(mut loads) = client.get_resource_mut::<GumpLoads>() else {
        return;
    };
    if !std::mem::take(&mut loads.done) {
        return;
    }
    crate::script::runtime::notify_now(
        client,
        Value::level(),
        "gump_loaded",
        vec![Value::Int(1)],
        now,
    );
}
