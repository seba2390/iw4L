//! The game's camera from above the players on this client: its client
//! dvars turn it on, it takes the view in place of the player's eye (and
//! shows the player's body), and the mouse aims through its cursor.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use game_api::top_down::{TopDownScene, TopDownState};
use render_scene::{CameraView, SimCamera, WorldCameraPose, transform_from_iw_view};

/// The camera's state, and whether this client last showed it.
#[derive(Resource, Default)]
pub(crate) struct GameTopDown {
    state: TopDownState,
    showing: bool,
}

/// The players the camera frames: the game's camera set is its first
/// clients.
const CAMERA_PLAYERS: u32 = 4;

/// A 4:3 screen, within this much.
const FOUR_BY_THREE: f32 = 4.0 / 3.0;
const ASPECT_EPSILON: f32 = 0.01;

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_top_down(
    mode: Option<Res<game_api::ModeRules>>,
    mut presented: ResMut<net::PresentedSnapshot>,
    prediction: Res<net::ClientPredictionState>,
    local: Res<net::LocalPresentClient>,
    clock: Res<net::FrameClock>,
    settings: Res<frame::GameSettings>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut sim_cam: ResMut<SimCamera>,
    mut aim: ResMut<net::CursorAim>,
    mut top_down: ResMut<GameTopDown>,
) {
    let rules = mode.as_deref().and_then(|mode| mode.top_down);
    let dvars = presented
        .snapshot()
        .and_then(|snapshot| snapshot.meta.for_client(local.0))
        .map(|meta| meta.client_dvars.clone());
    let dvar = |name: &str| {
        dvars.as_ref().and_then(|dvars| {
            dvars
                .iter()
                .rev()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.clone())
        })
    };
    let camera = rules.and_then(|rules| (rules.camera)(&dvar).map(|camera| (rules, camera)));
    let ps = prediction
        .0
        .predicted_local()
        .or_else(|| presented.player(local.0))
        .copied();
    let (Some((rules, camera)), Some(ps)) = (camera, ps) else {
        if top_down.showing {
            top_down.showing = false;
            top_down.state = TopDownState::default();
            sim_cam.view_override = None;
            presented.set_external_camera(false);
        }
        aim.active = false;
        return;
    };
    if !top_down.showing {
        top_down.showing = true;
        for gap in rules.gaps {
            diag::warn!(World, "top-down camera: {} — needs {}", gap.what, gap.needs);
        }
    }
    let players: Vec<[f32; 3]> = (0..CAMERA_PLAYERS)
        .filter_map(|n| {
            let id = sim::ClientId(n);
            if id == local.0 {
                Some(ps.origin)
            } else {
                presented.player(id).map(|other| other.origin)
            }
        })
        .collect();
    let four_by_three = windows.iter().next().is_some_and(|window| {
        (window.width() / window.height().max(1.0) - FOUR_BY_THREE).abs() < ASPECT_EPSILON
    });
    let scene = TopDownScene {
        origin: ps.origin,
        viewangles: ps.viewangles,
        players: &players,
        frametime_ms: clock.frametime(),
        four_by_three,
    };
    let pose = (rules.view)(&camera, &scene, &mut top_down.state);
    let horizontal_fov = dvar("cg_fov")
        .and_then(|fov| fov.trim().parse::<f32>().ok())
        .filter(|fov| fov.is_finite())
        .map_or(settings.fov, |fov| {
            fov.clamp(CG_FOV_DOMAIN.0, CG_FOV_DOMAIN.1)
        });
    sim_cam.view_override = Some(CameraView {
        transform: transform_from_iw_view(WorldCameraPose {
            origin: pose.origin,
            angles: pose.angles,
        }),
        horizontal_fov,
    });
    presented.set_external_camera(true);

    aim.active = true;
    let mouse = std::mem::take(&mut aim.mouse);
    if let Some(turn) = (rules.aim)(
        &camera,
        mouse,
        clock.frametime_secs(),
        ps.viewangles,
        &mut top_down.state,
    ) {
        aim.turn[0] += turn[0];
        aim.turn[1] += turn[1];
    }
}

/// `cg_fov`'s domain.
const CG_FOV_DOMAIN: (f32, f32) = (1.0, 160.0);
