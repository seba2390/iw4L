//! Black Ops' screen overlays: its backend's overlay pass, which draws one
//! exclusive overlay over the frame by precedence (the burning one first,
//! then the electric shock, then the transporter), each a fullscreen draw of
//! its post material reading the frame as drawn so far, while flame overlays
//! are allowed.

use game_api::post::{
    FlameOverlay, OverlayConstant, OverlayFeedback, OverlayFrame, OverlayPass, OverlayRules,
};
use game_api::unknown;

const BURN: &str = "flamethrowerfx_color_distort_overlay_bloom";
const SHOCK: &str = "zombie_electric_shock_overlay";
const TRANSPORTER: &str = "zombie_transporter_overlay";
const MATERIALS: [&str; 3] = [BURN, SHOCK, TRANSPORTER];
const WHITE: [u8; 4] = [255; 4];

pub(crate) const OVERLAYS: OverlayRules = OverlayRules {
    materials: &MATERIALS,
    filter_materials: &[],
    plan,
    blur: None,
    gaps: &[unknown!(
        "t5.render.overlay_rest",
        "the overlay pass's other draws: water sheeting (between the burn and the shock), the three after the transporter, and the pass's own blur of the frame; the script blur draws through the film pass",
        "Black Ops' overlay pass beyond the flame-style overlays"
    )],
};

/// The frame's overlay: the burn, else the shock, else the transporter,
/// each while flame overlays are allowed and it plays.
fn plan(frame: &OverlayFrame<'_>) -> Vec<OverlayPass> {
    let screen = frame.screen;
    let playing = |overlay: Option<FlameOverlay>| {
        overlay.filter(|o| screen.flame_allowed && plays(o, frame.now_ms))
    };
    let (material, overlay, scaled) = match (
        playing(screen.burn),
        playing(screen.shock),
        playing(screen.transporter),
    ) {
        (Some(burn), ..) => (BURN, burn, true),
        (None, Some(shock), _) => (SHOCK, shock, false),
        (None, None, Some(transporter)) => (TRANSPORTER, transporter, false),
        (None, None, None) => return Vec::new(),
    };
    let atlas = frame
        .atlases
        .iter()
        .find(|(name, _)| *name == material)
        .map_or([0, 0], |(_, atlas)| *atlas);
    let scaler = scaled.then_some(screen.flame_scaler);
    vec![OverlayPass {
        material: material.to_owned(),
        constants: flame_rows(&overlay, atlas, scaler, frame.now_ms),
        blur: None,
        feedback: OverlayFeedback::Frame,
        color: WHITE,
    }]
}

/// The client keeps a flame overlay until its fade has run longer than its
/// fade time; one that has not ended has no fade yet.
fn plays(overlay: &FlameOverlay, now_ms: i32) -> bool {
    overlay.fade_ms <= 0 || now_ms.wrapping_sub(overlay.fade_start_ms) <= overlay.fade_ms
}

/// The rows a flame-style draw sets: its atlas (columns, rows, frames) and
/// frame rate, its fade, its distortion and magnitude, and the full view.
fn flame_rows(
    overlay: &FlameOverlay,
    [rows, columns]: [u8; 2],
    scaler: Option<f32>,
    now_ms: i32,
) -> Vec<(OverlayConstant, [f32; 4])> {
    let [x, y] = overlay.distortion;
    vec![
        (OverlayConstant::GameTime, game_time(now_ms)),
        (
            OverlayConstant::OverlayTexCoord,
            [
                f32::from(columns),
                f32::from(rows),
                (u32::from(rows) * u32::from(columns)) as f32,
                overlay.fps,
            ],
        ),
        (
            OverlayConstant::FadeEffect,
            [fade(overlay, scaler, now_ms), 0.0, 0.0, 0.0],
        ),
        (
            OverlayConstant::FlameDistortion,
            [
                x,
                y,
                if y <= 0.0 { 0.0 } else { 1.0 / y },
                overlay.magnitude,
            ],
        ),
        (OverlayConstant::ViewportDimensions, [0.0, 0.0, 1.0, 1.0]),
    ]
}

/// The backend's game time row from the frame's time in msec: the sine and
/// cosine of its fraction of a second's turn, the fraction, and the time in
/// seconds.
fn game_time(now_ms: i32) -> [f32; 4] {
    let t = now_ms as f32 * 0.001;
    let frac = t - t.floor();
    let angle = f64::from(frac * std::f32::consts::TAU);
    [angle.sin() as f32, angle.cos() as f32, frac, t]
}

/// The fade's progress, 0 to 1 (0 before it ends); the burn's flame scaler
/// other than 1 replaces it with 1 less the scaler.
fn fade(overlay: &FlameOverlay, scaler: Option<f32>, now_ms: i32) -> f32 {
    if let Some(scaler) = scaler
        && scaler != 1.0
    {
        return 1.0 - scaler;
    }
    if overlay.fade_ms == 0 {
        return 0.0;
    }
    let remaining = overlay
        .fade_start_ms
        .wrapping_sub(now_ms)
        .wrapping_add(overlay.fade_ms);
    let progress = 1.0 - remaining as f32 / overlay.fade_ms as f32;
    if progress <= 0.0 {
        0.0
    } else if progress - 1.0 < 0.0 {
        progress
    } else {
        1.0
    }
}
