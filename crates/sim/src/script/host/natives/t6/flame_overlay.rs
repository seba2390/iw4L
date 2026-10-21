//! Black Ops 2's flame-style screen overlays, burning and electric shock, as
//! a client keeps them: a timed start (the server's `setburn` and
//! `setelectrified`, which send their client the time, or the client
//! scripts' own), the settings each takes from the `r_flameFX_*` dvars as it
//! starts, and its fade once it ends. The client's screen draws them
//! (`game_t6::overlay`).

use bevy_ecs::prelude::{Resource, World};
use game_api::post::FlameOverlay;
use std::collections::BTreeMap;

/// Which overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Flame {
    Burn,
    Shock,
}

/// One overlay of a client: its timer (start and end, 0 when it runs no
/// longer) and the block its screen draws from.
#[derive(Clone, Copy, Default)]
struct State {
    start_ms: i32,
    end_ms: i32,
    block: Option<Block>,
}

/// The settings an overlay took as it started, and its fade.
#[derive(Clone, Copy)]
struct Block {
    overlay: FlameOverlay,
    active: bool,
}

#[derive(Resource, Default)]
struct FlameOverlays(BTreeMap<u32, [State; 2]>);

fn slot(kind: Flame) -> usize {
    match kind {
        Flame::Burn => 0,
        Flame::Shock => 1,
    }
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

fn dvar_float(world: &World, name: &str) -> f32 {
    super::atof(&dvar_text(world, name))
}

/// Starts the overlay for `ms` from now: it plays at once with the current
/// magnitude, distortion scale and atlas rate and no fade.
pub(crate) fn start(world: &mut World, client: u32, kind: Flame, ms: i32) {
    let now = now_ms(world);
    let distortion = dvar_text(world, "r_flamefx_distortionscalefactor");
    let mut words = distortion.split_whitespace().map(super::atof);
    let distortion = [words.next().unwrap_or(0.0), words.next().unwrap_or(0.0)];
    let overlay = FlameOverlay {
        distortion,
        magnitude: dvar_float(world, "r_flamefx_magnitude"),
        fps: super::atoi(&dvar_text(world, "r_flamefx_fps")) as f32,
        fade_start_ms: 0,
        fade_ms: 0,
    };
    {
        let mut overlays = world.get_resource_or_insert_with(FlameOverlays::default);
        overlays.0.entry(client).or_default()[slot(kind)] = State {
            start_ms: now,
            end_ms: now.wrapping_add(ms),
            block: Some(Block {
                overlay,
                active: true,
            }),
        };
    }
    diag::info!(
        Sim,
        "t6 flame overlay: {kind:?} for client {client}, {ms} ms at {now}"
    );
    publish(world, client);
}

/// Whether a client's burning runs (its timer has not ended).
pub(crate) fn burning(world: &World, client: u32) -> bool {
    world
        .get_resource::<FlameOverlays>()
        .and_then(|overlays| overlays.0.get(&client))
        .is_some_and(|states| states[slot(Flame::Burn)].end_ms != 0)
}

/// A client's frame: an overlay past its end time stops, beginning its
/// fade (`r_flameFX_fadeDuration`) unless one already runs, and one whose
/// fade has run longer than its fade time is gone.
pub(crate) fn frame(world: &mut World, client: u32) {
    let Some(states) = world
        .get_resource::<FlameOverlays>()
        .and_then(|overlays| overlays.0.get(&client))
        .copied()
    else {
        return;
    };
    let now = now_ms(world);
    let fade_ms = (dvar_float(world, "r_flamefx_fadeduration") * 1000.0) as i32;
    let mut next = states;
    for state in &mut next {
        if state.end_ms != 0 && state.end_ms < now {
            state.start_ms = 0;
            state.end_ms = 0;
            if let Some(block) = state.block.as_mut()
                && block.overlay.fade_ms == 0
            {
                block.overlay.fade_start_ms = now;
                block.overlay.fade_ms = fade_ms;
            }
        }
        if let Some(block) = state.block.as_mut()
            && block.active
            && block.overlay.fade_ms > 0
            && now.wrapping_sub(block.overlay.fade_start_ms) > block.overlay.fade_ms
        {
            block.active = false;
        }
    }
    if let Some(mut overlays) = world.get_resource_mut::<FlameOverlays>() {
        overlays.0.insert(client, next);
    }
    publish(world, client);
}

/// The client's screen takes its overlays and the flame render dvars.
fn publish(world: &mut World, client: u32) {
    let states = world
        .get_resource::<FlameOverlays>()
        .and_then(|overlays| overlays.0.get(&client))
        .copied()
        .unwrap_or_else(<[State; 2]>::default);
    let drawn = |state: &State| {
        state
            .block
            .filter(|block| block.active)
            .map(|block| block.overlay)
    };
    let burn = drawn(&states[slot(Flame::Burn)]);
    let shock = drawn(&states[slot(Flame::Shock)]);
    let flame_allowed = super::atoi(&dvar_text(world, "r_flame_allowed")) != 0;
    let flame_scaler = dvar_float(world, "r_flamescaler");
    let mut frame = crate::frame::FrameWorld::from_world(world);
    let id = crate::ClientId(client);
    if frame.client_meta(id).is_some() {
        let screen = &mut frame
            .client_meta_mut(id)
            .view_effects
            .client_screen
            .overlays;
        screen.burn = burn;
        screen.shock = shock;
        screen.flame_allowed = flame_allowed;
        screen.flame_scaler = flame_scaler;
    }
}

fn now_ms(world: &World) -> i32 {
    crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick)
}
