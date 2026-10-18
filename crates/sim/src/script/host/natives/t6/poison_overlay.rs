//! Black Ops 2's poison overlay, as a client keeps it: the server's
//! `startpoisoning` / `stoppoisoning` set the player's poisoned word, and
//! each client frame the client moves its poison amount toward 1 while the
//! player is poisoned (0 otherwise) at two thirds per second; the renderer
//! takes that amount, or 1 while `r_poisonFX_debug_enable` is set, with the
//! `r_poisonFX_pulse` and `r_poisonFX_dvisionX` it reads as it draws. The
//! client's screen draws it (`game_t6::overlay`).

use bevy_ecs::prelude::{Resource, World};
use game_api::post::PoisonOverlay;
use std::collections::BTreeMap;

/// The perk whose holder the client never poisons.
const PROTECTION_PERK: &str = "specialty_proximityprotection";
/// The amount's rate toward its target, per msec.
const RATE_PER_MS: f32 = 0.001 * (2.0 / 3.0);

/// One client's poison: the player's poisoned word, and the amount its
/// client moves and the time of its last frame.
#[derive(Clone, Copy, Default)]
struct State {
    poisoned: bool,
    amount: f32,
    last_ms: Option<i32>,
}

#[derive(Resource, Default)]
struct PoisonOverlays(BTreeMap<u32, State>);

/// The server's `startpoisoning` / `stoppoisoning`: the player's poisoned
/// word.
pub(crate) fn set_poisoned(world: &mut World, client: u32, poisoned: bool) {
    world
        .get_resource_or_insert_with(PoisonOverlays::default)
        .0
        .entry(client)
        .or_default()
        .poisoned = poisoned;
}

/// The server's `ispoisoned`: the player's poisoned word.
pub(crate) fn poisoned(world: &World, client: u32) -> bool {
    world
        .get_resource::<PoisonOverlays>()
        .and_then(|overlays| overlays.0.get(&client))
        .is_some_and(|state| state.poisoned)
}

fn dvar_text(world: &World, name: &str) -> String {
    world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get(name)
        .cloned()
        .or_else(|| super::dvars::lookup(name).map(|spec| spec.default.to_owned()))
        .unwrap_or_else(String::new)
}

/// A client's frame: the amount moves toward 1 while the player is
/// poisoned and does not hold the protection perk, else toward 0, by the
/// frame's time at the rate, stopping at the target.
///
/// t6.csc.overlay.poison_gates: the client also holds the target at 0 on a
/// player-state flag, and the amount at 0 on two more and while a demo's
/// free or follow camera plays; what those player-state bits mean was not
/// read, so they do not gate it here.
pub(crate) fn frame(world: &mut World, client: u32, dvars: &PoisonDvars) {
    let now = crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick);
    let protected = world
        .resource::<crate::script::RoundScript>()
        .players
        .get(&client)
        .is_some_and(|player| player.perks.contains(PROTECTION_PERK));
    let mut overlays = world.get_resource_or_insert_with(PoisonOverlays::default);
    let state = overlays.0.entry(client).or_default();
    let frame_ms = state
        .last_ms
        .map_or(0, |last| now.wrapping_sub(last).max(0));
    state.last_ms = Some(now);
    let target = if state.poisoned && !protected {
        1.0
    } else {
        0.0
    };
    if state.amount != target {
        let step = frame_ms as f32 * RATE_PER_MS;
        let short = if target <= state.amount {
            state.amount -= step;
            target < state.amount
        } else {
            state.amount += step;
            state.amount < target
        };
        if !short && state.amount != target {
            state.amount = target;
        }
    }
    let amount = state.amount;
    publish(world, client, amount, dvars);
}

/// The poison render dvars, as the client holds them.
pub(crate) struct PoisonDvars {
    debug_enable: bool,
    pulse: f32,
    dvision_x: f32,
    blur_min: f32,
    blur_max: f32,
}

impl PoisonDvars {
    pub(crate) fn read(client: &World) -> Self {
        Self {
            debug_enable: super::atoi(&dvar_text(client, "r_poisonfx_debug_enable")) != 0,
            pulse: super::atof(&dvar_text(client, "r_poisonfx_pulse")),
            dvision_x: super::atof(&dvar_text(client, "r_poisonfx_dvisionx")),
            blur_min: super::atof(&dvar_text(client, "r_poisonfx_blurmin")),
            blur_max: super::atof(&dvar_text(client, "r_poisonfx_blurmax")),
        }
    }
}

/// The client's screen takes the amount the renderer draws, with the poison
/// render dvars it reads: none while that amount is not over 0.
fn publish(world: &mut World, client: u32, amount: f32, dvars: &PoisonDvars) {
    let amount = if dvars.debug_enable { 1.0 } else { amount };
    let poison = (amount > 0.0).then_some(PoisonOverlay {
        amount,
        pulse: dvars.pulse,
        dvision_x: dvars.dvision_x,
        blur_min: dvars.blur_min,
        blur_max: dvars.blur_max,
    });
    let mut frame = crate::frame::FrameWorld::from_world(world);
    let id = crate::ClientId(client);
    if frame.client_meta(id).is_some() {
        frame
            .client_meta_mut(id)
            .view_effects
            .client_screen
            .overlays
            .poison = poison;
    }
}
