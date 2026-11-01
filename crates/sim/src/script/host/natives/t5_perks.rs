//! Black Ops multiplayer's perks: one bit per perk, in the order of its
//! perk table, on the server's player and the client's player info.

use super::super::args::string;
use crate::script::Namespace::Method;
use crate::script::{NativeRegistry, Value};
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

const PERKS: [&str; 52] = [
    "specialty_armorpiercing",
    "specialty_armorvest",
    "specialty_bulletaccuracy",
    "specialty_bulletdamage",
    "specialty_bulletflinch",
    "specialty_bulletpenetration",
    "specialty_copycat",
    "specialty_delayexplosive",
    "specialty_detectexplosive",
    "specialty_disarmexplosive",
    "specialty_explosivedamage",
    "specialty_extraammo",
    "specialty_extramoney",
    "specialty_fallheight",
    "specialty_fastads",
    "specialty_fastinteract",
    "specialty_fastmantle",
    "specialty_fastmeleerecovery",
    "specialty_fastreload",
    "specialty_fastweaponswitch",
    "specialty_finalstand",
    "specialty_fireproof",
    "specialty_flakjacket",
    "specialty_gambler",
    "specialty_gas_mask",
    "specialty_gpsjammer",
    "specialty_grenadepulldeath",
    "specialty_healthregen",
    "specialty_holdbreath",
    "specialty_killstreak",
    "specialty_longersprint",
    "specialty_loudenemies",
    "specialty_movefaster",
    "specialty_nomotionsensor",
    "specialty_noname",
    "specialty_nottargetedbyai",
    "specialty_pin_back",
    "specialty_pistoldeath",
    "specialty_quieter",
    "specialty_reconnaissance",
    "specialty_rof",
    "specialty_scavenger",
    "specialty_shades",
    "specialty_showenemyequipment",
    "specialty_stunprotection",
    "specialty_shellshock",
    "specialty_sprintrecovery",
    "specialty_showonradar",
    "specialty_twoattach",
    "specialty_twogrenades",
    "specialty_twoprimaries",
    "specialty_unlimitedsprint",
];

/// The perks the multiplayer executable's own code tests whose tests do
/// not run here yet; its movement, weapon and damage ones (`bulletaccuracy`,
/// `bulletpenetration`, `fallheight`, `fastmantle`, `fastmeleerecovery`,
/// `fastreload`, `fastweaponswitch`, `longersprint`, `pistoldeath`,
/// `shellshock`, `sprintrecovery`, `unlimitedsprint`) run in the game's
/// movement, fire and damage rules and in `shellshock`.
const ENGINE_PERKS: [&str; 13] = [
    "specialty_bulletflinch",
    "specialty_fastads",
    "specialty_gas_mask",
    "specialty_grenadepulldeath",
    "specialty_loudenemies",
    "specialty_noname",
    "specialty_nottargetedbyai",
    "specialty_pin_back",
    "specialty_quieter",
    "specialty_reconnaissance",
    "specialty_scavenger",
    "specialty_showenemyequipment",
    "specialty_twogrenades",
];

/// Each player's perk bits, by client number. The client's player info
/// takes a copy every frame.
#[derive(Resource, Clone, Debug, Default)]
pub(crate) struct PlayerPerks(pub(crate) BTreeMap<u32, u64>);

impl PlayerPerks {
    fn of(&self, client: u32) -> u64 {
        self.0.get(&client).copied().unwrap_or(0)
    }
}

/// A player's perk bits.
pub(crate) fn perk_bits(world: &World, client: u32) -> u64 {
    world
        .get_resource::<PlayerPerks>()
        .map_or(0, |perks| perks.of(client))
}

fn bit(name: &str) -> Result<u64, String> {
    PERKS
        .iter()
        .position(|perk| perk.eq_ignore_ascii_case(name))
        .map(|index| 1 << index)
        .ok_or_else(|| format!("Unknown perk: {name}\n"))
}

fn player(world: &World, receiver: &Value) -> Result<u32, String> {
    let id = super::engine::entity_id(world, receiver)?;
    let runtime = world.resource::<crate::script::RoundScript>();
    runtime.player_client(id).ok_or_else(|| {
        let number = runtime.entities.get(&id).map_or(-1, |e| e.number);
        format!("entity {number} is not a player")
    })
}

fn perks(world: &mut World) -> &mut PlayerPerks {
    world
        .get_resource_or_insert_with(PlayerPerks::default)
        .into_inner()
}

/// The player state's perk words take the scripts' bits, as Black Ops
/// keeps one mask the movement reads.
fn sync(world: &mut World, client: u32) {
    let mask = perks(world).of(client);
    crate::game_move::set_perk_mask(
        &mut crate::frame::FrameWorld::from_world(world),
        crate::world::ClientId(client),
        mask,
    );
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Method, "setperk", |world, receiver, args| {
        let client = player(world, receiver)?;
        let name = string(args, 0)?;
        let bit = bit(&name)?;
        *perks(world).0.entry(client).or_default() |= bit;
        sync(world, client);
        if let Some(perk) = ENGINE_PERKS.iter().find(|p| p.eq_ignore_ascii_case(&name)) {
            diag::warn!(
                Sim,
                "t5.mp.perk_effects: {perk}'s effect in Black Ops' own movement and weapon code is not applied"
            );
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "unsetperk", |world, receiver, args| {
        let client = player(world, receiver)?;
        let bit = bit(&string(args, 0)?)?;
        *perks(world).0.entry(client).or_default() &= !bit;
        sync(world, client);
        Ok(Value::Undefined)
    });
    registry.register(Method, "hasperk", |world, receiver, args| {
        let client = player(world, receiver)?;
        let bit = bit(&string(args, 0)?)?;
        Ok(Value::Int((perks(world).of(client) & bit != 0).into()))
    });
    registry.register(Method, "clearperks", |world, receiver, _| {
        let client = player(world, receiver)?;
        perks(world).0.remove(&client);
        sync(world, client);
        Ok(Value::Undefined)
    });
    // Black Ops multiplayer's shell shock tests `specialty_shellshock`.
    registry.register(Method, "shellshock", |world, receiver, args| {
        super::t5::shellshock(world, receiver, args, true)
    });
    // In the order of the perk table.
    registry.register(Method, "getperks", |world, receiver, _| {
        let client = player(world, receiver)?;
        let held = perks(world).of(client);
        let names = PERKS
            .iter()
            .enumerate()
            .filter(|(index, _)| held & (1 << index) != 0)
            .map(|(_, name)| Value::string(name))
            .collect();
        super::super::arrays::new_array(world, names)
    });
}

/// The client's `player hasPerk(perk)`, for any player entity.
pub(crate) fn register_client(registry: &mut NativeRegistry) {
    registry.register(Method, "hasperk", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        let number = world
            .resource::<crate::script::RoundScript>()
            .entities
            .get(&id)
            .map_or(-1, |e| e.number);
        if !(0..32).contains(&number) {
            return Err("hasperk() can only be called on players".into());
        }
        let bit = bit(&string(args, 0)?)?;
        let held = world
            .get_resource::<PlayerPerks>()
            .map_or(0, |perks| perks.of(number as u32));
        Ok(Value::Int((held & bit != 0).into()))
    });
}
