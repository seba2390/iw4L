//! Black Ops multiplayer's equipment builtins: `SpawnTimedFX` (Willy Pete's
//! smoke and Nova gas clouds), `TestSpawnPoint` and `IsTouchingVolume` (the
//! tactical insertion).

use super::super::args::{int, optional, string, vector};
use crate::frame::FrameWorld;
use crate::script::Namespace::{Function, Method};
use crate::script::{NativeRegistry, Value};
use bevy_ecs::prelude::World;
use game_api::{Unknown, unknown};

/// How long a timed effect's entity lasts when the script names no time,
/// in seconds.
const TIMED_FX_SECONDS: i32 = 10;
/// The box `TestSpawnPoint` fits a player in, and what it must not start in.
const SPAWN_MINS: [f32; 3] = [-15.0, -15.0, 0.0];
const SPAWN_MAXS: [f32; 3] = [15.0, 15.0, 70.0];
const SPAWN_MASK: u32 = 0x0081_0011;

fn timed_fx_unknown() -> &'static Unknown {
    unknown!(
        "t5.mp.timed_fx",
        "a SpawnTimedFX effect plays once where it was spawned; Black Ops keeps its entity for the given time, so a client that only then comes to see it starts the effect",
        "Black Ops' timed effect entity, replayed for clients that see it late"
    )
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Function, "spawntimedfx", spawn_timed_fx);
    registry.register(Function, "testspawnpoint", |world, _, args| {
        let at = vector(args, 0)?;
        let t = super::engine::trace(world, at, at, SPAWN_MINS, SPAWN_MAXS, SPAWN_MASK);
        Ok(Value::Int((t.startsolid == 0 && t.allsolid == 0).into()))
    });
    registry.register(Method, "istouchingvolume", |world, receiver, args| {
        let own = super::engine::entity_id(world, receiver)?;
        let origin = vector(args, 0)?;
        let (lo, hi) = (vector(args, 1)?, vector(args, 2)?);
        let mut mins: [f32; 3] = core::array::from_fn(|i| origin[i] + lo[i]);
        let mut maxs: [f32; 3] = core::array::from_fn(|i| origin[i] + hi[i]);
        // Grown in height to at least its width, as Black Ops' touch boxes
        // are.
        let height = maxs[2] - mins[2];
        let x = maxs[0] - mins[0];
        let y = maxs[1] - mins[1];
        let width = if x - y < 0.0 { y } else { x };
        if height < width {
            let grow = (width - height) * 0.5;
            mins[2] -= grow;
            maxs[2] += grow;
        }
        let hit = super::super::triggers::box_touches(world, own, mins, maxs);
        Ok(Value::Int(hit.into()))
    });
}

/// `SpawnTimedFX(weapon, origin, [dir], [seconds])`: the weapon's explosion
/// effect played at `origin` (snapped to whole units) facing `dir` (up when
/// none), its entity kept `seconds` (10 when none).
fn spawn_timed_fx(world: &mut World, _: &Value, args: &[Value]) -> Result<Value, String> {
    let name = string(args, 0)?;
    let origin = vector(args, 1)?;
    let mut dir = optional(args, 2, vector)?.unwrap_or([0.0, 0.0, 1.0]);
    let _seconds = optional(args, 3, int)?.unwrap_or(TIMED_FX_SECONDS);
    let mut frame = FrameWorld::from_world(world);
    let weapon = crate::script_player::weapon_named(&frame, &name)?;
    let effect = frame
        .game_weapon(weapon)
        .and_then(|w| w.explosion_effect.clone())
        .ok_or_else(|| format!("weapon '{name}' has no explosion effect"))?;
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    let len = if -len >= 0.0 { 1.0 } else { len };
    dir = dir.map(|v| v / len);
    let at = origin.map(|v| v as i32 as f32);
    let index = frame.effect_name_index(&effect);
    frame.report_game_gap(timed_fx_unknown());
    super::engine::play_fx_at(world, index, at, dir, [0.0; 3]);
    Ok(Value::Undefined)
}
