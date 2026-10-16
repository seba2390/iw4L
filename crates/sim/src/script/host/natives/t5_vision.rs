//! Black Ops' vision and fog builtins. The server's set a client's naked and
//! last-stand visions and its exponential or volumetric fog, as its config
//! strings would; the client scripts' go straight to the local client's
//! vision channels and fog state.

use super::super::args::{float, int, string};
use super::player::player;
use crate::frame::FrameWorld;
use crate::script::Namespace::{Function, Method};
use crate::script::client::{ClientOutput, ClientView};
use crate::script::{NativeRegistry, RoundScript, Value};
use crate::world::ClientId;
use crate::{ScriptFog, ScriptFogParams, ScriptSunFog, ScriptVolFog, VisionCall, VisionChange};
use bevy_ecs::prelude::World;

/// A transition time when a call gives none.
const DEFAULT_MS: i32 = 1000;

const EXP_FOG_USAGE: &str = "Incorrect number of parameters\nUSAGE: setExpFog(<startDist>, <halfwayDist>, <red>, <green>, <blue>, <transition time>)\n";
const SERVER_VOL_FOG_USAGE: &str = "Incorrect number of parameters\nUSAGE: setVolFog(<startDist>, <halfwayDist>, <halfwayHeight>, <baseHeight>, <red>, <green>, <blue>, <transition time>)\nOR:    SetVolFog(<startDist>, <halfwayDist>, <halfwayHeight>, <baseHeight>, <red>, <green>, <blue>, <fogColorScale>, <sunFogRed>, <sunFogGreen>, <sunFogBlue>, <sunFogDirX>, <sunFogDirY>, <sunFogDirZ>, <sunFogStartAng>, <sunFogEndAng>, <transition time>)\n";
const CLIENT_VOL_FOG_USAGE: &str = "Incorrect number of parameters\nUSAGE: setVolFog(<startDist>, <halfwayDist>, <halfwayHeight>, <baseHeight>, <red>, <green>, <blue>, <transition time>, <sun red>, <sun blue>, <sun green>, <sun dir X>, <sun dir Y>, <sun dir Z>, <sun start angle>, <sun end angle>, <max fog opacity>)\n";
const CLIENT_VOL_FOG_FOR_CLIENT_USAGE: &str = "Incorrect number of parameters\nUSAGE: setVolFog(<localClientNum>, <startDist>, <halfwayDist>, <halfwayHeight>, <baseHeight>, <red>, <green>, <blue>, <transition time>, <sun red>, <sun blue>, <sun green>, <sun dir X>, <sun dir Y>, <sun dir Z>, <sun start angle>, <sun end angle>, <max fog opacity>)\n";
const WATER_FOG_USAGE: &str = "Incorrect number of parameters\nUSAGE: SetWaterFog(<startDist>, <halfwayDist>, <halfwayHeight>, <baseHeight>, <red>, <green>, <blue>)\n";

/// The server's vision and fog builtins.
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
    registry.register(Function, "setexpfog", |world, _, args| {
        let (target, duration_ms) = server_exp_fog(args)?;
        set_fog(world, None, target, duration_ms);
        Ok(Value::Undefined)
    });
    registry.register(Method, "setexpfog", |world, receiver, args| {
        let client = client_entity(
            world,
            receiver,
            "setexponentialfog() called on an invalid client entity.\n",
        )?;
        let (target, duration_ms) = server_exp_fog(args)?;
        set_fog(world, Some(client), target, duration_ms);
        Ok(Value::Undefined)
    });
    registry.register(Function, "setvolfog", |world, _, args| {
        let (target, duration_ms) = server_vol_fog(args)?;
        set_fog(world, None, target, duration_ms);
        Ok(Value::Undefined)
    });
    registry.register(Method, "setvolfog", |world, receiver, args| {
        let client = client_entity(
            world,
            receiver,
            "setvolumetricfog() called on an invalid client entity.\n",
        )?;
        let (target, duration_ms) = server_vol_fog(args)?;
        set_fog(world, Some(client), target, duration_ms);
        Ok(Value::Undefined)
    });
}

/// The client scripts' vision and fog builtins.
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
        let (name, duration_ms) =
            client_vision(args, 2, "visionsetunderwater() called with wrong params.\n")?;
        client_call(world, VisionCall::ClientUnderwater { name, duration_ms });
        Ok(Value::Undefined)
    });
    registry.register(Function, "setvolfog", |world, _, args| {
        if !matches!(args.len(), 8 | 18) {
            return Err(CLIENT_VOL_FOG_USAGE.into());
        }
        let fog = client_vol_fog(args)?;
        world.resource_mut::<ClientOutput>().fog_call = Some(fog);
        Ok(Value::Undefined)
    });
    registry.register(Function, "setvolfogforclient", |world, _, args| {
        if !matches!(args.len(), 9 | 19) {
            return Err(CLIENT_VOL_FOG_FOR_CLIENT_USAGE.into());
        }
        let local = int(args, 0)?;
        if local == 0 {
            let fog = client_vol_fog(&args[1..])?;
            world.resource_mut::<ClientOutput>().fog_call = Some(fog);
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "setwaterfog", |world, _, args| {
        let fog = client_water_fog(args)?;
        world.resource_mut::<ClientOutput>().water_fog = Some(fog);
        Ok(Value::Undefined)
    });
}

/// Applies a fog target on the server (for `client`, or for every client
/// when `None`), starting a transition from the fog currently shown if its
/// density is non-zero.
pub(crate) fn set_fog(
    world: &mut World,
    client: Option<u32>,
    target: ScriptFogParams,
    duration_ms: i32,
) {
    let now = world
        .get_resource::<crate::step::StepRequest>()
        .map_or(0, |request| crate::level_time_ms(request.tick));
    let previous = {
        let frame = FrameWorld::from_world(world);
        let client_fog = frame
            .client_meta(ClientId(client.unwrap_or(0)))
            .and_then(|meta| meta.view_effects.fog);
        client_fog
            .or_else(|| world.resource::<RoundScript>().engine.fog)
            .map(|fog| fog.sample(now))
    };
    let duration_ms = if previous.is_some_and(|prev| prev.halfway_dist > 0.0) {
        duration_ms.max(0)
    } else {
        0
    };
    let fog = ScriptFog {
        from: previous.unwrap_or(target),
        to: target,
        start_ms: now,
        duration_ms,
    };
    if let Some(client) = client {
        let has_meta = FrameWorld::from_world(world)
            .client_meta(ClientId(client))
            .is_some();
        if has_meta {
            edit_view(world, client, |view| view.fog = Some(fog));
        } else if client == 0 {
            world.resource_mut::<RoundScript>().engine.fog = Some(fog);
        }
    } else {
        let mut frame = FrameWorld::from_world(world);
        for id in frame.client_ids_sorted() {
            frame.client_meta_mut(id).view_effects.fog = None;
        }
        world.resource_mut::<RoundScript>().engine.fog = Some(fog);
    }
}

fn client_entity(world: &World, receiver: &Value, invalid: &str) -> Result<u32, String> {
    let Value::Object(id) = receiver else {
        return Err("not an entity".into());
    };
    let runtime = world.resource::<RoundScript>();
    if !runtime.entities.contains_key(id) {
        return Err("not an entity".into());
    }
    runtime.player_client(*id).ok_or_else(|| invalid.into())
}

fn server_exp_fog(args: &[Value]) -> Result<(ScriptFogParams, i32), String> {
    if args.len() != 6 {
        return Err(EXP_FOG_USAGE.into());
    }
    let start_dist = float(args, 0)?;
    if start_dist < 0.0 {
        return Err("setExpFog: startDist must be greater or equal to 0".into());
    }
    let halfway_dist = float(args, 1)?;
    if halfway_dist <= 0.0 {
        return Err("setExpFog: halfwayDist must be greater than 0".into());
    }
    let color_rgb = [float(args, 2)?, float(args, 3)?, float(args, 4)?];
    let time = float(args, 5)?;
    if time < 0.0 {
        return Err("setExpFog: transition time must be >= 0 seconds".into());
    }
    let duration_ms = (f64::from(time * 1000.0)).round() as i32;
    Ok((
        ScriptFogParams {
            start_dist,
            halfway_dist,
            color_rgb,
            max_opacity: 1.0,
            sun: Some(ScriptSunFog {
                color_rgb: [0.5; 3],
                sun_dir: [1.0, 0.0, 0.0],
                begin_angle_deg: 0.0,
                end_angle_deg: 0.0,
                scale: 1.0,
            }),
            volumetric: Some(ScriptVolFog {
                halfway_height: 0.0,
                base_height: 0.0,
                color_scale: 1.0,
                max_density: 100.0 / halfway_dist,
            }),
        },
        duration_ms,
    ))
}

fn server_vol_fog(args: &[Value]) -> Result<(ScriptFogParams, i32), String> {
    if !matches!(args.len(), 8 | 18) {
        return Err(SERVER_VOL_FOG_USAGE.into());
    }
    let start_dist = float(args, 0)?;
    if start_dist < 0.0 {
        return Err("setExpFog: startDist must be greater or equal to 0".into());
    }
    let halfway_dist = float(args, 1)?;
    if halfway_dist <= 0.0 {
        return Err("setVolFog: halfwayDist must be greater than 0".into());
    }
    let halfway_height = float(args, 2)?;
    if halfway_height < 0.0 {
        return Err("setVolFog: halfwayHeight must be greater or equal to 0".into());
    }
    let base_height = float(args, 3)?;
    let mut color_rgb = [float(args, 4)?, float(args, 5)?, float(args, 6)?];
    let (color_scale, sun, time, max_opacity) = if args.len() == 18 {
        let color_scale = float(args, 7)?;
        let sun = ScriptSunFog {
            color_rgb: [float(args, 8)?, float(args, 9)?, float(args, 10)?],
            sun_dir: [float(args, 11)?, float(args, 12)?, float(args, 13)?],
            begin_angle_deg: float(args, 14)?,
            end_angle_deg: float(args, 15)?,
            scale: 1.0,
        };
        (color_scale, sun, float(args, 16)?, float(args, 17)?)
    } else {
        let color_scale = color_rgb[0].max(color_rgb[1]).max(color_rgb[2]);
        let inv = 1.0 / color_scale;
        color_rgb = color_rgb.map(|c| c * inv);
        let sun = ScriptSunFog {
            color_rgb: [0.5; 3],
            sun_dir: [1.0, 0.0, 0.0],
            begin_angle_deg: 0.0,
            end_angle_deg: 0.0,
            scale: 1.0,
        };
        (color_scale, sun, float(args, 7)?, 1.0)
    };
    if time < 0.0 {
        return Err("setVolFog: transition time must be >= 0 seconds".into());
    }
    let duration_ms = (f64::from(time * 1000.0)).round() as i32;
    Ok((
        ScriptFogParams {
            start_dist,
            halfway_dist,
            color_rgb,
            max_opacity,
            sun: Some(sun),
            volumetric: Some(ScriptVolFog {
                halfway_height: if halfway_height < 1.0 {
                    0.0
                } else {
                    halfway_height
                },
                base_height,
                color_scale,
                max_density: 100.0 / halfway_dist,
            }),
        },
        duration_ms,
    ))
}

fn client_vol_fog(args: &[Value]) -> Result<(ScriptFogParams, i32), String> {
    let start_dist = float(args, 0)?;
    if start_dist < 0.0 {
        return Err("setExpFog: startDist must be greater or equal to 0".into());
    }
    let halfway_dist = float(args, 1)?;
    if halfway_dist <= 0.0 {
        return Err("setVolFog: halfwayDist must be greater than 0".into());
    }
    let halfway_height = float(args, 2)?;
    if halfway_height < 0.0 {
        return Err("setVolFog: halfwayHeight must be greater or equal to 0".into());
    }
    let base_height = float(args, 3)?;
    let color_rgb = [float(args, 4)?, float(args, 5)?, float(args, 6)?];
    let (color_scale, sun, duration_ms, max_opacity) = if args.len() == 8 {
        let duration_ms = (float(args, 7)? * 1000.0) as i32;
        let sun = ScriptSunFog {
            color_rgb: [0.0; 3],
            sun_dir: [0.0; 3],
            begin_angle_deg: 0.0,
            end_angle_deg: 0.0,
            scale: 1.0,
        };
        (1.0, sun, duration_ms, 1.0)
    } else {
        let duration_ms = (float(args, 16)? * 1000.0) as i32;
        let max_opacity = float(args, 17)?;
        let end_angle_deg = float(args, 15)?;
        let begin_angle_deg = float(args, 14)?;
        let sun_dir = [float(args, 11)?, float(args, 12)?, float(args, 13)?];
        let sun_color = [float(args, 8)?, float(args, 9)?, float(args, 10)?];
        let color_scale = float(args, 7)?;
        let sun = ScriptSunFog {
            color_rgb: sun_color,
            sun_dir,
            begin_angle_deg,
            end_angle_deg,
            scale: 1.0,
        };
        (color_scale, sun, duration_ms, max_opacity)
    };
    Ok((
        ScriptFogParams {
            start_dist,
            halfway_dist,
            color_rgb,
            max_opacity,
            sun: Some(sun),
            volumetric: Some(ScriptVolFog {
                halfway_height: if halfway_height < 1.0 {
                    0.0
                } else {
                    halfway_height
                },
                base_height,
                color_scale,
                max_density: 100.0 / halfway_dist,
            }),
        },
        duration_ms.max(0),
    ))
}

fn client_water_fog(args: &[Value]) -> Result<ScriptFogParams, String> {
    if !matches!(args.len(), 7 | 17) {
        return Err(WATER_FOG_USAGE.into());
    }
    let start_dist = float(args, 0)?;
    let halfway_dist = float(args, 1)?;
    if halfway_dist <= 0.0 {
        return Err("SetWaterFog: halfwayDist must be greater than 0".into());
    }
    let halfway_height = float(args, 2)?;
    if halfway_height < 0.0 {
        return Err("SetWaterFog: halfwayHeight must be greater or equal to 0".into());
    }
    let base_height = float(args, 3)?;
    let color_rgb = [float(args, 4)?, float(args, 5)?, float(args, 6)?];
    let (color_scale, sun, max_opacity) = if args.len() == 7 {
        let sun = ScriptSunFog {
            color_rgb: [1.0; 3],
            sun_dir: [1.0, 0.0, 0.0],
            begin_angle_deg: 0.0,
            end_angle_deg: 0.0,
            scale: 1.0,
        };
        (1.0, sun, 1.0)
    } else {
        let color_scale = float(args, 7)?;
        let max_opacity = float(args, 16)?;
        let sun_color = [float(args, 8)?, float(args, 9)?, float(args, 10)?];
        let sun_dir = [float(args, 11)?, float(args, 12)?, float(args, 13)?];
        let begin_angle_deg = float(args, 14)?;
        let end_angle_deg = float(args, 15)?;
        let sun = ScriptSunFog {
            color_rgb: sun_color,
            sun_dir,
            begin_angle_deg,
            end_angle_deg,
            scale: 1.0,
        };
        (color_scale, sun, max_opacity)
    };
    Ok(ScriptFogParams {
        start_dist,
        halfway_dist,
        color_rgb,
        max_opacity,
        sun: Some(sun),
        volumetric: Some(ScriptVolFog {
            halfway_height: if halfway_height < 1.0 {
                0.0
            } else {
                halfway_height
            },
            base_height,
            color_scale,
            max_density: 1.0 / halfway_dist,
        }),
    })
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
