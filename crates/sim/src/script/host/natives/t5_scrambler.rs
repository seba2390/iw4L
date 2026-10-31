//! Black Ops multiplayer's client scrambler state, which its client
//! scripts (`_scrambler.csc`) keep: how much the nearest enemy and friendly
//! Jammers jam, whether an enemy counter-spy plane is up, and the local
//! player's own Jammers for the compass. The compass hides enemies while an
//! enemy counter-spy plane is up and fades them by the enemy jamming.
//! `getOwner` and `getLocalPlayerTeam` serve the scripts' friend-or-foe
//! test.

use super::super::args::{float, int};
use crate::script::Namespace::{Function, Method};
use crate::script::client::ClientOutput;
use crate::script::{NativeRegistry, Value};
use bevy_ecs::prelude::World;

/// The local player's Jammers the compass shows.
const FRIENDLY_SLOTS: usize = 16;

/// A friendly Jammer: where it is, when it was added (the oldest slot is
/// taken next) and the scripts' handle for it (`0`: an empty slot).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct FriendlyScrambler {
    pub xy: [f32; 2],
    pub added: u32,
    pub handle: i32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ClientScrambler {
    pub global: i32,
    pub enemy_amount: f32,
    pub friendly_amount: f32,
    pub friendlies: [FriendlyScrambler; FRIENDLY_SLOTS],
    serial: u32,
}

impl ClientScrambler {
    /// The compass's view of it: the enemy jamming, whether an enemy
    /// counter-spy plane is up, and where the friendly Jammers are.
    pub(crate) fn screen(&self) -> (f32, bool, Vec<[f32; 2]>) {
        let friendlies = self
            .friendlies
            .iter()
            .filter(|slot| slot.handle != 0)
            .map(|slot| slot.xy)
            .collect();
        (self.enemy_amount, self.global != 0, friendlies)
    }
}

fn scrambler(world: &mut World) -> &mut ClientScrambler {
    &mut world.resource_mut::<ClientOutput>().into_inner().scrambler
}

fn amount(args: &[Value]) -> Result<f32, String> {
    let value = float(args, 0)?;
    if !(0.0..=1.0).contains(&value) {
        return Err("float value must be between 0 and 1".into());
    }
    Ok(value)
}

fn local_client(args: &[Value], index: usize) -> Result<(), String> {
    let local = int(args, index)?;
    if local != 0 {
        return Err(format!(
            "Trying to get a local client index for a client '{local}' that is not a local client."
        ));
    }
    Ok(())
}

pub(crate) fn register_client(registry: &mut NativeRegistry) {
    // `ent getOwner(localClientNum)`: the entity its entity state names as
    // its owner.
    registry.register(Method, "getowner", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        local_client(args, 0)?;
        let Some(owner) = world
            .get_resource::<crate::script::client::ClientOwners>()
            .and_then(|owners| owners.0.get(&id).copied())
        else {
            return Ok(Value::Undefined);
        };
        let runtime = world.resource::<crate::script::RoundScript>();
        Ok(runtime
            .entities
            .iter()
            .find(|(_, e)| e.number == owner)
            .map_or(Value::Undefined, |(id, _)| Value::Object(*id)))
    });
    // `GetLocalPlayerTeam(localClientNum)`: the local player's team, as
    // `free`, `axis`, `allies` or `spectator`.
    registry.register(Function, "getlocalplayerteam", |world, _, args| {
        local_client(args, 0)?;
        let Some(player) = world
            .resource::<crate::script::client::ClientView>()
            .locals
            .first()
            .map(|local| local.object)
        else {
            return Ok(Value::Undefined);
        };
        let mut runtime = world.resource_mut::<crate::script::RoundScript>();
        Ok(match runtime.object_field(player, "team") {
            Value::String(team) if matches!(&*team, "axis" | "allies" | "spectator") => {
                Value::String(team)
            }
            _ => Value::string("free"),
        })
    });
    registry.register(Method, "setenemyglobalscrambler", |world, _, args| {
        scrambler(world).global = int(args, 0)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setenemyscrambleramount", |world, _, args| {
        scrambler(world).enemy_amount = amount(args)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "getenemyscrambleramount", |world, _, _| {
        Ok(Value::Float(scrambler(world).enemy_amount))
    });
    registry.register(Method, "setfriendlyscrambleramount", |world, _, args| {
        scrambler(world).friendly_amount = amount(args)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "getfriendlyscrambleramount", |world, _, _| {
        Ok(Value::Float(scrambler(world).friendly_amount))
    });
    registry.register(Method, "addfriendlyscrambler", |world, _, args| {
        let xy = [float(args, 0)?, float(args, 1)?];
        let handle = int(args, 2)?;
        let state = scrambler(world);
        state.serial += 1;
        let added = state.serial;
        let slot = state
            .friendlies
            .iter_mut()
            .min_by_key(|slot| slot.added)
            .unwrap();
        *slot = FriendlyScrambler { xy, added, handle };
        Ok(Value::Undefined)
    });
    registry.register(Method, "removefriendlyscrambler", |world, _, args| {
        let handle = int(args, 0)?;
        if let Some(slot) = scrambler(world)
            .friendlies
            .iter_mut()
            .find(|slot| slot.handle == handle)
        {
            slot.added = 0;
            slot.handle = 0;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "removeallfriendlyscramblers", |world, _, _| {
        for slot in &mut scrambler(world).friendlies {
            slot.added = 0;
            slot.handle = 0;
        }
        Ok(Value::Undefined)
    });
    // The nearest enemy Jammer: nothing ported reads it yet (ledger
    // `t5.csc.nearest_enemy_scrambler`).
    registry.register(Method, "setnearestenemyscrambler", |world, _, args| {
        let entity = args.first().ok_or("not an entity")?;
        super::engine::entity_id(world, entity)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "clearnearestenemyscrambler", |_, _, _| {
        Ok(Value::Undefined)
    });
}
