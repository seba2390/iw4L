//! Black Ops multiplayer's `setActionSlot`: what each dpad slot does — a
//! weapon to switch to (a killstreak's, the equipment), the alternate mode
//! or night vision. The client switches to a slot's weapon when its button
//! is pressed.

use super::super::args::{int, string};
use crate::frame::FrameWorld;
use crate::script::Namespace::Method;
use crate::script::{NativeRegistry, RoundScript, Value};
use crate::world::ClientId;
use bevy_ecs::prelude::World;

const SLOTS: i32 = 4;
const WEAPON: i32 = 1;
const ALT_MODE: i32 = 2;
const NIGHT_VISION: i32 = 3;

fn set_action_slot(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let id = super::engine::entity_id(world, receiver).map_err(|_| "not an entity".to_owned())?;
    let runtime = world.resource::<RoundScript>();
    let client = runtime.player_client(id).ok_or_else(|| {
        let number = runtime.entities.get(&id).map_or(-1, |e| e.number);
        format!("entity {number} is not a player")
    })?;
    let slot = int(args, 0)?;
    if !(1..=SLOTS).contains(&slot) {
        return Err(format!(
            "Invalid slot ({slot}) given, expecting 1 - {SLOTS}\n"
        ));
    }
    let slot = (slot - 1) as usize;
    let kind = string(args, 1)?;
    // An alternate mode or night vision slot keeps the weapon a weapon slot
    // named before.
    let (kind, weapon) = if kind.eq_ignore_ascii_case("weapon") {
        let name = string(args, 2)?;
        let weapon = crate::script_player::weapon_named(&FrameWorld::from_world(world), &name)
            .ok()
            .filter(|&w| w != 0)
            .ok_or_else(|| format!("Unknown weapon name \"{name}\".\n"))?;
        (WEAPON, Some(weapon as i32))
    } else if kind.eq_ignore_ascii_case("altmode") {
        (ALT_MODE, None)
    } else if kind.eq_ignore_ascii_case("nightvision") {
        (NIGHT_VISION, None)
    } else if kind.is_empty() {
        (0, None)
    } else {
        return Err(
            "Invalid option: expected \"weapon\", \"altweapon\", or \"nightvision\".\n".into(),
        );
    };
    if let Some(ps) = FrameWorld::from_world(world).player_mut(ClientId(client)) {
        ps.action_slot_type[slot] = kind;
        if let Some(weapon) = weapon {
            ps.action_slot_param[slot] = weapon;
        }
    }
    Ok(Value::Undefined)
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Method, "setactionslot", set_action_slot);
}
