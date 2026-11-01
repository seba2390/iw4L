//! Black Ops multiplayer's class builtins outside its stats: the profile's
//! loadout, the custom game mode's class table and the pregame class.

use super::super::args::{int, string};
use crate::script::Namespace::{Function, Method};
use crate::script::{NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;

/// The loadout slots: the custom game mode class table's item names, one
/// byte each, and the stats' class members, in Black Ops' slot order.
pub(crate) const ITEMS: [&str; 39] = [
    "primary",
    "primaryattachment",
    "primaryattachmenttop",
    "primaryattachmentbottom",
    "primaryattachmenttrigger",
    "primaryattachmentmuzzle",
    "primarycamo",
    "primaryreticle",
    "primaryreticlecolor",
    "primarylens",
    "primaryemblem",
    "primarytag",
    "secondary",
    "secondaryattachment",
    "secondaryattachmenttop",
    "secondaryattachmentbottom",
    "secondaryattachmenttrigger",
    "secondaryattachmentmuzzle",
    "secondarycamo",
    "secondaryreticle",
    "secondaryreticlecolor",
    "secondarylens",
    "secondaryemblem",
    "secondarytag",
    "specialty1",
    "specialty2",
    "specialty3",
    "classbonus",
    "head",
    "body",
    "primarygrenade",
    "specialgrenade",
    "equipment",
    "facepaintpattern",
    "facepaintcolor",
    "killstreak1",
    "killstreak2",
    "killstreak3",
    "deathstreak",
];

/// The custom game mode class table's modifiers, one int each.
const MODIFIERS: [&str; 9] = [
    "active",
    "team",
    "health",
    "healthRegeneration",
    "healthVampirism",
    "movementSpeed",
    "movementSprintSpeed",
    "damage",
    "damageExplosive",
];

const CLASSES: u32 = 10;
const USAGE: &str = "GetCustomClassLoadoutItem usage: <classnum>, <itemname>";

/// A player entity's client number; `refusal` words the error for an
/// entity (by its number) that is not a player.
fn player(
    world: &World,
    receiver: &Value,
    refusal: impl FnOnce(i32) -> String,
) -> Result<u32, String> {
    let id = super::engine::entity_id(world, receiver).map_err(|_| "not an entity".to_owned())?;
    let runtime = world.resource::<RoundScript>();
    runtime
        .player_client(id)
        .ok_or_else(|| refusal(runtime.entities.get(&id).map_or(-1, |e| e.number)))
}

/// The custom game mode's class table: nothing fills it here (no custom
/// game settings reach the host), so it reads as the game's zeroed table.
fn class_number(args: &[Value]) -> Result<(u32, String), String> {
    if args.len() != 2 {
        return Err(USAGE.into());
    }
    let name = string(args, 1)?;
    Ok((int(args, 0)? as u32, name))
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    // The profile's own loadouts are console split-screen's; the PC game
    // answers 0 for every slot.
    registry.register(
        Method,
        "getloadoutitemfromprofile",
        |world, receiver, args| {
            player(world, receiver, |_| {
                "getloadoutitemfromprofile: entity must be a player entity".into()
            })?;
            string(args, 0)?;
            if args.len() > 1 {
                string(args, 1)?;
            }
            Ok(Value::Int(0))
        },
    );
    registry.register(Function, "getcustomclassloadoutitem", |_, _, args| {
        let (class, name) = class_number(args)?;
        if class >= CLASSES {
            return Ok(Value::Int(0));
        }
        if !ITEMS.iter().any(|item| item.eq_ignore_ascii_case(&name)) {
            diag::warn!(Sim, "Unknown custom class item name {name}");
            return Ok(Value::Int(0xff));
        }
        Ok(Value::Int(0))
    });
    registry.register(Function, "getcustomclassmodifier", |_, _, args| {
        let (class, name) = class_number(args)?;
        if class >= CLASSES {
            return Ok(Value::Int(0));
        }
        if !MODIFIERS.iter().any(|m| m.eq_ignore_ascii_case(&name)) {
            diag::warn!(Sim, "Unknown custom class modifier {name}");
            return Ok(Value::Int(-1));
        }
        Ok(Value::Int(0))
    });
    // The pregame lobby's class; no pregame lobby runs, so it is empty.
    registry.register(Method, "getpregameclass", |world, receiver, _| {
        player(world, receiver, |number| {
            format!("only valid on players; called on entity {number}")
        })?;
        Ok(Value::Undefined)
    });
}
