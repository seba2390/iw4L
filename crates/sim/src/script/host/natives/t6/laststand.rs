//! Black Ops 2's last stand and revive, as the executable's handlers read
//! and write the player: `finishplayerdamage` (in `damage.rs`) puts a player
//! whose health runs out down and calls the scripts' last stand callback;
//! `reviveplayer`, `undolaststand`, `startrevive`, `stoprevive`,
//! `setlaststandprevweap`, the weapon cycling switches and
//! `fakedamagefrom` are here. The client fields IW4L does not keep are
//! listed under `t6.match.last_stand` in `docs/fidelity/t6.md`.

use super::super::player::player;
use super::{float, string, vector};
use crate::ClientId;
use crate::frame::FrameWorld;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

/// What the engine keeps on a client for last stand.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Down {
    /// The client is down (set by `finishplayerdamage`, cleared by
    /// `reviveplayer` and `undolaststand`).
    pub down: bool,
    /// Someone is reviving the client (`startrevive` / `stoprevive`).
    pub being_revived: bool,
    /// The weapon `setlaststandprevweap` named; `reviveplayer` clears it.
    pub prev_weapon: u32,
    /// `disableweaponcycling`, kept for the weapons that will honour it.
    pub cycling_disabled: bool,
}

#[derive(Resource, Default)]
struct Downs(BTreeMap<u32, Down>);

pub(super) fn state(world: &mut World, client: u32) -> &mut Down {
    world
        .get_resource_or_insert_with(Downs::default)
        .into_inner()
        .0
        .entry(client)
        .or_default()
}

pub(super) fn is_down(world: &World, client: u32) -> bool {
    world
        .get_resource::<Downs>()
        .and_then(|downs| downs.0.get(&client))
        .is_some_and(|down| down.down)
}

/// The lent movement's own last stand, which a downed player moves by
/// (`t6.movement.player`).
pub(super) fn lend_last_stand_move(world: &mut World, id: ClientId) {
    crate::script_player::lend_last_stand_move(&mut FrameWorld::from_world(world), id);
}

fn client(world: &World, receiver: &Value) -> Result<u32, String> {
    player(world, receiver)
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    // Back up: the damage timer and the last stand fields cleared, health
    // back to the maximum.
    registry.register(Method, "reviveplayer", |world, receiver, _| {
        let client = client(world, receiver)?;
        *state(world, client) = Down {
            cycling_disabled: state(world, client).cycling_disabled,
            ..Down::default()
        };
        let id = ClientId(client);
        let mut frame = FrameWorld::from_world(world);
        crate::script_player::undo_last_stand(&mut frame, id);
        if let Some(ps) = frame.player_mut(id) {
            ps.damage_timer = 0;
            ps.damage_duration = 0;
            ps.health = ps.max_health;
        }
        Ok(Value::Undefined)
    });
    // Out of last stand, health left as it is.
    registry.register(Method, "undolaststand", |world, receiver, _| {
        let client = client(world, receiver)?;
        let down = state(world, client);
        down.down = false;
        down.being_revived = false;
        crate::script_player::undo_last_stand(&mut FrameWorld::from_world(world), ClientId(client));
        Ok(Value::Undefined)
    });
    registry.register(Method, "startrevive", |world, receiver, _| {
        let client = client(world, receiver)?;
        state(world, client).being_revived = true;
        Ok(Value::Undefined)
    });
    registry.register(Method, "stoprevive", |world, receiver, _| {
        let client = client(world, receiver)?;
        state(world, client).being_revived = false;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setlaststandprevweap", |world, receiver, args| {
        let client = client(world, receiver)?;
        let name = string(args, 0)?;
        let weapon = super::weapon::weapon_index(world, &name)?;
        state(world, client).prev_weapon = weapon as u32;
        Ok(Value::Undefined)
    });
    registry.register(Method, "disableweaponcycling", |world, receiver, _| {
        let client = client(world, receiver)?;
        state(world, client).cycling_disabled = true;
        Ok(Value::Undefined)
    });
    registry.register(Method, "enableweaponcycling", |world, receiver, _| {
        let client = client(world, receiver)?;
        state(world, client).cycling_disabled = false;
        Ok(Value::Undefined)
    });
    // The hit indicator from a direction: its yaw and pitch in 256ths of a
    // turn, a new damage event, a count of 50.
    registry.register(Method, "fakedamagefrom", |world, receiver, args| {
        let id = ClientId(client(world, receiver)?);
        let dir = vector(args, 0)?;
        let length = dir.iter().map(|c| c * c).sum::<f32>().sqrt();
        let length = if -length >= 0.0 { 1.0 } else { length };
        let dir = dir.map(|c| c / length);
        let angles = vectoangles(dir);
        let mut frame = FrameWorld::from_world(world);
        if let Some(ps) = frame.player_mut(id) {
            ps.damage_yaw = (angles[1] * (1.0 / 360.0) * 256.0) as i32 as u32;
            ps.damage_pitch = (angles[0] * (1.0 / 360.0) * 256.0) as i32 as u32;
            ps.damage_event = ps.damage_event.wrapping_add(1);
            ps.damage_count = FAKE_DAMAGE_COUNT;
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "visionsetlaststand", |world, _, args| {
        let duration_ms = match args.len() {
            1 => 1000,
            2 => (float(args, 1)? * 1000.0 + 9.313_226e-10).round() as i32,
            _ => return Ok(Value::Undefined),
        };
        let name = string(args, 0)?;
        let set_ms = world
            .get_resource::<crate::step::StepRequest>()
            .map_or(0, |r| crate::level_time_ms(r.tick));
        world.resource_mut::<RoundScript>().engine.last_stand_vision = Some(crate::VisionChange {
            name,
            duration_ms,
            set_ms,
        });
        Ok(Value::Undefined)
    });
    registry.register(Method, "docowardswayanims", |world, receiver, _| {
        client(world, receiver).map_err(|_| "USAGE: Must be called on a client\n".to_owned())?;
        Err("t6.player.anim_events: the player's coward sway animation event is not played, so its length is not known".into())
    });
}

/// The engine's angles of a direction, in degrees from 0 to 360; straight
/// up or down has pitch 270 or 90 and yaw 0.
fn vectoangles(v: [f32; 3]) -> [f32; 3] {
    if v[0] == 0.0 && v[1] == 0.0 {
        return [if -v[2] < 0.0 { 270.0 } else { 90.0 }, 0.0, 0.0];
    }
    let wrap = |degrees: f32| {
        if degrees < 0.0 {
            degrees + 360.0
        } else {
            degrees
        }
    };
    let yaw = wrap(v[1].atan2(v[0]) * 180.0 * std::f32::consts::FRAC_1_PI);
    let flat = (v[0] * v[0] + v[1] * v[1]).sqrt();
    let pitch = wrap(v[2].atan2(flat) * -180.0 * std::f32::consts::FRAC_1_PI);
    [pitch, yaw, 0.0]
}

/// The damage count `fakedamagefrom` gives the hit indicator.
const FAKE_DAMAGE_COUNT: i32 = 50;
