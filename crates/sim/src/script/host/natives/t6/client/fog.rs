//! The client's script fog: configstring 10 (the server's `setvolfog` and
//! `setexpfog`) and the client scripts' volumetric fog functions fill the
//! client's fog slot 1 and switch between slot 0 (no fog) and slot 1; the
//! local client's draw blends to the slot (`game_t6::fog`). The active world
//! fog bank mask, which the draw's fog volumes are tested against, is kept
//! with them.

use super::super::extras::ServerFog;
use super::game::{LOCAL_CLIENT, local_client};
use crate::script::host::args::{float, int};
use crate::script::host::client_scripts::with_server;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};

/// A fog's values in the order the client keeps them: start, colour,
/// density, height density, base height, colour scale, sun colour, sun
/// direction, sun start and end angles, maximum opacity.
pub(crate) type FogValues = [f32; 17];

/// The bank mask the client game sets for a map.
const INITIAL_BANK: u16 = 1;

#[derive(Resource, Default)]
struct ClientFog {
    /// The client game has taken the gamestate.
    connected: bool,
    /// The server fog's serial last applied.
    server_serial: u32,
    /// Configstring 10's values as last parsed.
    server: FogValues,
    /// `setclientvolumetricfog`'s values.
    client: FogValues,
    /// Switches made.
    switches: u32,
}

/// The slot block a fog's values fill: the maximum density is 100 times the
/// density.
fn slot_block(v: &FogValues) -> [f32; 20] {
    let mut block = [0.0; 20];
    block[2..5].copy_from_slice(&v[1..4]);
    block[5] = v[7];
    block[6] = v[0];
    block[7] = v[4];
    block[8] = v[5];
    block[9] = v[6];
    block[10..13].copy_from_slice(&v[8..11]);
    block[13] = v[16];
    block[14..17].copy_from_slice(&v[11..14]);
    block[17] = v[14];
    block[18] = v[15];
    block[19] = v[4] * 100.0;
    block
}

/// Switches the local client's fog to `slot` (filled with `values` for slot
/// 1) at the client's time, over `transition_ms`.
fn switch(client: &mut World, slot: u8, values: Option<&FogValues>, transition_ms: i32) {
    let serial = {
        let mut fog = client.get_resource_or_insert_with(ClientFog::default);
        fog.switches = fog.switches.wrapping_add(1);
        fog.switches
    };
    let block = values.map_or([0.0; 20], slot_block);
    with_server(client, |server| {
        let start_ms = crate::level_time_ms(server.resource::<crate::step::StepRequest>().tick);
        let mut frame = crate::frame::FrameWorld::from_world(server);
        let id = crate::ClientId(LOCAL_CLIENT as u32);
        if frame.client_meta(id).is_some() {
            frame.client_meta_mut(id).view_effects.client_screen.fog =
                Some(crate::ScriptFogSwitch {
                    serial,
                    slot,
                    block,
                    start_ms,
                    transition_ms,
                });
        }
    });
}

/// Each snapshot: on the gamestate the fog switches to slot 0 at once, then
/// configstring 10 is parsed, as it is again whenever it changes: its values
/// fill slot 1 and switch to it over its transition (an empty configstring
/// switches to slot 0 at once).
pub(crate) fn sync(client: &mut World) {
    if !client.resource::<RoundScript>().started {
        return;
    }
    let (serial, server_fog) = with_server(client, |server| {
        server
            .get_resource::<ServerFog>()
            .map_or((0, None), |fog| (fog.serial, fog.values))
    })
    .unwrap_or((0, None));
    let (connect, changed) = {
        let fog = client.get_resource_or_insert_with(ClientFog::default);
        (!fog.connected, fog.server_serial != serial)
    };
    if connect {
        switch(client, 0, None, 0);
    }
    if !(connect || changed) {
        return;
    }
    {
        let mut fog = client.resource_mut::<ClientFog>();
        fog.connected = true;
        fog.server_serial = serial;
    }
    match server_fog {
        Some((values, transition_ms)) => {
            client.resource_mut::<ClientFog>().server = values;
            switch(client, 1, Some(&values), transition_ms);
        }
        None => switch(client, 0, None, 0),
    }
}

/// The local client's active world fog bank mask.
pub(crate) fn bank(client: &World) -> u16 {
    with_server(client, |server| {
        crate::frame::FrameWorld::from_world(server)
            .client_meta(crate::ClientId(LOCAL_CLIENT as u32))
            .and_then(|meta| meta.view_effects.client_screen.fog_bank)
    })
    .flatten()
    .unwrap_or(INITIAL_BANK)
}

/// Sets a client's active world fog bank mask.
pub(crate) fn set_bank(server: &mut World, client: u32, bank: u16) {
    let mut frame = crate::frame::FrameWorld::from_world(server);
    let id = crate::ClientId(client);
    if frame.client_meta(id).is_some() {
        frame
            .client_meta_mut(id)
            .view_effects
            .client_screen
            .fog_bank = Some(bank);
    }
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::Function;

    // setclientvolumetricfog(start, halfway, halfheight, base, r, g, b,
    // [scale, sun r, g, b, sun dir x, y, z, start angle, end angle,
    // transition,] max opacity): keeps the client's own fog for
    // `switchtoclientvolumetricfog`. The short form has no sun, a colour
    // scale and opacity of 1; neither form reads its transition.
    registry.register(Function, "setclientvolumetricfog", |world, _, args| {
        let start = float(args, 0)?;
        if start < 0.0 {
            return Err("setClientVolFog: startDist must be greater or equal to 0".into());
        }
        let halfway = float(args, 1)?;
        if halfway <= 0.0 {
            return Err("setClientVolFog: halfwayDist must be greater than 0".into());
        }
        let half_height = float(args, 2)?;
        if half_height < 0.0 {
            return Err("setClientVolFog: halfwayHeight must be greater or equal to 0".into());
        }
        let base = float(args, 3)?;
        let density = 1.0 / halfway;
        let height_density = if half_height < 1.0 {
            0.0
        } else {
            1.0 / half_height
        };
        let color = [float(args, 4)?, float(args, 5)?, float(args, 6)?];
        let mut values = [0.0; 17];
        values[0] = start;
        values[1..4].copy_from_slice(&color);
        values[4] = density;
        values[5] = height_density;
        values[6] = base;
        if args.len() == 8 {
            values[7] = 1.0;
            values[16] = 1.0;
        } else {
            for (i, value) in values.iter_mut().enumerate().take(16).skip(7) {
                *value = float(args, i)?;
            }
            values[16] = float(args, 17)?;
        }
        world.get_resource_or_insert_with(ClientFog::default).client = values;
        Ok(Value::Undefined)
    });

    // switchtoservervolumetricfog(localclientnum): slot 1 takes the server's
    // last fog, switched to at once.
    registry.register(Function, "switchtoservervolumetricfog", |world, _, args| {
        local_client(args, 0)?;
        let values = world.get_resource_or_insert_with(ClientFog::default).server;
        switch(world, 1, Some(&values), 0);
        Ok(Value::Undefined)
    });

    // switchtoclientvolumetricfog(localclientnum): slot 1 takes the
    // client's own fog, switched to at once.
    registry.register(Function, "switchtoclientvolumetricfog", |world, _, args| {
        if args.len() != 1 {
            return Err("Incorrect number of parameters\nUSAGE: SwitchToClientVolumetricFog(localClientNum)\n".into());
        }
        local_client(args, 0)?;
        let values = world.get_resource_or_insert_with(ClientFog::default).client;
        switch(world, 1, Some(&values), 0);
        Ok(Value::Undefined)
    });

    // setservervolumetricfogdensity(localclientnum, density, transition_ms):
    // slot 1 takes the server's last fog with this density, switched to
    // over the transition.
    registry.register(
        Function,
        "setservervolumetricfogdensity",
        |world, _, args| {
            local_client(args, 0)?;
            let density = float(args, 1)?;
            let transition_ms = int(args, 2)?;
            let mut values = world.get_resource_or_insert_with(ClientFog::default).server;
            values[4] = density;
            switch(world, 1, Some(&values), transition_ms);
            Ok(Value::Undefined)
        },
    );

    // getservervolumetricfogdensity(localclientnum): the server fog's
    // density as configstring 10 last gave it.
    registry.register(
        Function,
        "getservervolumetricfogdensity",
        |world, _, args| {
            local_client(args, 0)?;
            let values = world.get_resource_or_insert_with(ClientFog::default).server;
            Ok(Value::Float(values[4]))
        },
    );
}
