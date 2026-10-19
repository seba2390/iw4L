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
    /// Until this level time a downed client takes no damage but a blow:
    /// half a second past going down.
    pub grace_until_ms: i64,
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

/// A downed player's move type: last stand, which `movement_t6` moves them
/// by, and what going down does in the game's own movement.
pub(super) fn enter_last_stand_move(world: &mut World, id: ClientId) {
    let mut frame = FrameWorld::from_world(world);
    crate::script_player::enter_script_last_stand(&mut frame, id);
    crate::game_move::enter_last_stand(&mut frame, id);
}

/// A downed client still in the half second after going down.
pub(super) fn in_grace(world: &World, client: u32, now_ms: i64) -> bool {
    world
        .get_resource::<Downs>()
        .and_then(|downs| downs.0.get(&client))
        .is_some_and(|down| down.down && down.grace_until_ms > now_ms)
}

fn client(world: &World, receiver: &Value) -> Result<u32, String> {
    player(world, receiver)
}

fn being_revived(world: &mut World, receiver: &Value, on: bool) -> Result<Value, String> {
    let client = client(world, receiver)?;
    state(world, client).being_revived = on;
    crate::game_move::set_being_revived(&mut FrameWorld::from_world(world), ClientId(client), on);
    Ok(Value::Undefined)
}

fn weapon_cycling(world: &mut World, receiver: &Value, allowed: bool) -> Result<Value, String> {
    let id = ClientId(client(world, receiver)?);
    crate::game_move::set_script_flag(
        &mut FrameWorld::from_world(world),
        id,
        game_api::movement::ScriptFlag::NoWeaponCycling,
        !allowed,
    );
    Ok(Value::Undefined)
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    // Back up: the damage timer and the last stand fields cleared, health
    // back to the maximum.
    registry.register(Method, "reviveplayer", |world, receiver, _| {
        let client = client(world, receiver)?;
        *state(world, client) = Down::default();
        let id = ClientId(client);
        let mut frame = FrameWorld::from_world(world);
        crate::script_player::undo_last_stand(&mut frame, id);
        crate::game_move::revive(&mut frame, id);
        crate::game_move::clear_damage_timer(&mut frame, id);
        if let Some(ps) = frame.player_mut(id) {
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
    // The revive move type (8) while someone revives, back to last stand
    // (7) when they stop.
    registry.register(Method, "startrevive", |world, receiver, _| {
        being_revived(world, receiver, true)
    });
    registry.register(Method, "stoprevive", |world, receiver, _| {
        being_revived(world, receiver, false)
    });
    registry.register(Method, "setlaststandprevweap", |world, receiver, args| {
        let client = client(world, receiver)?;
        let name = string(args, 0)?;
        let weapon = super::weapon::weapon_index(world, &name)?;
        state(world, client).prev_weapon = weapon as u32;
        Ok(Value::Undefined)
    });
    // The weapon flag that keeps an empty weapon in hand.
    registry.register(Method, "disableweaponcycling", |world, receiver, _| {
        weapon_cycling(world, receiver, false)
    });
    registry.register(Method, "enableweaponcycling", |world, receiver, _| {
        weapon_cycling(world, receiver, true)
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
            lerp: None,
        });
        Ok(Value::Undefined)
    });
    registry.register(Method, "docowardswayanims", |world, receiver, _| {
        let client = client(world, receiver).map_err(|_| "not a player".to_owned())?;
        let mut frame = FrameWorld::from_world(world);
        // `laststand_suicide` is index 36 in `ANIM_ET_NAMES`.
        let duration = crate::game_move::play_anim_event(&mut frame, ClientId(client), 36, true)
            .unwrap_or(100);
        Ok(Value::Float((duration - 100) as f32 * 0.001))
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
