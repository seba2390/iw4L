//! Black Ops 2's revive overlay debug, as a client keeps it: while
//! `r_reviveFX_debug` is set, the frame's revive settings are the
//! `r_reviveFX_edge*` dvars (enabled) in place of the vision's, and the
//! revive pass draws whatever their amount. The client's screen draws it
//! (`game_t6::overlay`).

use bevy_ecs::prelude::World;
use game_api::post::ReviveFx;

fn dvar_text(world: &World, name: &str) -> String {
    world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get(name)
        .cloned()
        .or_else(|| super::dvars::lookup(name).map(|spec| spec.default.to_owned()))
        .unwrap_or_else(String::new)
}

fn float(world: &World, name: &str) -> f32 {
    super::atof(&dvar_text(world, name))
}

/// A vector dvar's three components; one it lacks is 0.
fn triple(world: &World, name: &str) -> [f32; 3] {
    let text = dvar_text(world, name);
    let mut words = text.split_whitespace().map(super::atof);
    std::array::from_fn(|_| words.next().unwrap_or(0.0))
}

/// The revive settings `r_reviveFX_debug` puts in the frame, while it is
/// set.
pub(crate) fn read(client: &World) -> Option<ReviveFx> {
    (super::atoi(&dvar_text(client, "r_revivefx_debug")) != 0).then(|| ReviveFx {
        enable: true,
        edge_color_temp: float(client, "r_revivefx_edgecolortemp"),
        edge_saturation: float(client, "r_revivefx_edgesaturation"),
        edge_scale: triple(client, "r_revivefx_edgescale"),
        edge_contrast: triple(client, "r_revivefx_edgecontrast"),
        edge_offset: triple(client, "r_revivefx_edgeoffset"),
        edge_mask_adjust: float(client, "r_revivefx_edgemaskadjust"),
        edge_amount: float(client, "r_revivefx_edgeamount"),
    })
}

/// The client's screen takes the debug revive settings.
pub(crate) fn publish(world: &mut World, client: u32, revive: Option<ReviveFx>) {
    let mut frame = crate::frame::FrameWorld::from_world(world);
    let id = crate::ClientId(client);
    if frame.client_meta(id).is_some() {
        frame
            .client_meta_mut(id)
            .view_effects
            .client_screen
            .overlays
            .revive_debug = revive;
    }
}
