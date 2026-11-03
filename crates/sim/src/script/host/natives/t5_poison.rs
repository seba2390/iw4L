//! Black Ops' poison overlay, as its client keeps it: the server's
//! `startPoisoning` / `stopPoisoning` (Nova gas, `_tabun.gsc`) set the
//! player's poisoned word, and each client frame the client moves its
//! poison amount toward 1 while the player is poisoned (0 otherwise) at two
//! thirds per second, as Black Ops 2 does; Tactical Mask
//! (`specialty_gas_mask`) holds the target at 0. With
//! `r_poisonFX_debug_enable` the renderer draws `r_poisonFX_debug_amount`
//! instead. The renderer reads the `r_poisonFX_*` dvars the scripts set on
//! the client (`SetClientDvar`) as it draws; the client's screen draws it
//! (`game_t5::overlay`).

use bevy_ecs::prelude::World;
use game_api::post::PoisonOverlay;

use super::t6::poison_overlay;

/// The perk whose holder the client never poisons.
const GAS_MASK: &str = "specialty_gas_mask";

/// The poison render dvars with their registered defaults.
const DEFAULTS: [(&str, f32); 10] = [
    ("r_poisonfx_debug_enable", 0.0),
    ("r_poisonfx_debug_amount", 0.0),
    ("r_poisonfx_pulse", 1.1),
    ("r_poisonfx_warpx", 0.2),
    ("r_poisonfx_warpy", -0.15),
    ("r_poisonfx_dvisiona", 0.0),
    ("r_poisonfx_dvisionx", 0.0),
    ("r_poisonfx_dvisiony", 0.0),
    ("r_poisonfx_blurmin", 0.0),
    ("r_poisonfx_blurmax", 4.0),
];

/// A poison render dvar as the client holds it: what the scripts set on
/// it, else the registered default.
fn dvar(world: &World, client: u32, name: &str) -> f32 {
    let set = world
        .resource::<crate::script::RoundScript>()
        .players
        .get(&client)
        .and_then(|player| player.dvars.get(name).cloned());
    match set {
        Some(text) => super::iw4::atof(&text) as f32,
        None => DEFAULTS
            .iter()
            .find(|(known, _)| *known == name)
            .map_or(0.0, |(_, default)| *default),
    }
}

/// The client's frame for player `client`.
///
/// t5.mp.poison_gates: the client also takes the client scripts' own
/// poisoned word (no client script sets it), and holds the target at 0
/// under one player-state flag and during a killcam, and both the target
/// and the amount at 0 in two more client states; which states those are
/// is not read, so they do not gate it here.
pub(crate) fn frame(world: &mut World, client: u32) {
    let masked = super::t5_perks::has_perk(world, client, GAS_MASK);
    let amount = poison_overlay::fade(world, client, masked);
    let amount = if dvar(world, client, "r_poisonfx_debug_enable") != 0.0 {
        dvar(world, client, "r_poisonfx_debug_amount")
    } else {
        amount
    };
    let poison = (amount > 0.0).then(|| PoisonOverlay {
        amount,
        pulse: dvar(world, client, "r_poisonfx_pulse"),
        dvision_x: dvar(world, client, "r_poisonfx_dvisionx"),
        warp: [
            dvar(world, client, "r_poisonfx_warpx"),
            dvar(world, client, "r_poisonfx_warpy"),
        ],
        dvision_a: dvar(world, client, "r_poisonfx_dvisiona"),
        dvision_y: dvar(world, client, "r_poisonfx_dvisiony"),
        blur_min: dvar(world, client, "r_poisonfx_blurmin"),
        blur_max: dvar(world, client, "r_poisonfx_blurmax"),
    });
    poison_overlay::show(world, client, poison);
}
