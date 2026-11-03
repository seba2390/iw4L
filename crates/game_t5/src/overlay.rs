//! Black Ops' screen overlays: its backend's overlay pass, which draws one
//! exclusive overlay over the frame by precedence (the burning one first,
//! then the electric shock, then the transporter, each while flame overlays
//! are allowed, then the poison while its amount is over 0), each a
//! fullscreen draw of its post material reading the frame as drawn so far;
//! then its blur pass.

use game_api::post::{
    BlurRules, FlameOverlay, OverlayConstant, OverlayFeedback, OverlayFrame, OverlayPass,
    OverlayRules, PoisonOverlay,
};
use game_api::unknown;

const BURN: &str = "flamethrowerfx_color_distort_overlay_bloom";
const SHOCK: &str = "zombie_electric_shock_overlay";
const TRANSPORTER: &str = "zombie_transporter_overlay";
const POISON: &str = "poison";
const BLEND: &str = "feedbackblend";
const MATERIALS: [&str; 13] = [
    BURN,
    SHOCK,
    TRANSPORTER,
    POISON,
    BLEND,
    game_api::blur::MATERIALS[0],
    game_api::blur::MATERIALS[1],
    game_api::blur::MATERIALS[2],
    game_api::blur::MATERIALS[3],
    game_api::blur::MATERIALS[4],
    game_api::blur::MATERIALS[5],
    game_api::blur::MATERIALS[6],
    game_api::blur::MATERIALS[7],
];
/// What the blur pass's smallest radius is, times the screen's height: a
/// smaller blur draws at that radius, faded in by its share of it.
const BLUR_FADE_HEIGHT: f32 = 1440.0;
const WHITE: [u8; 4] = [255; 4];

pub(crate) const OVERLAYS: OverlayRules = OverlayRules {
    materials: &MATERIALS,
    filter_materials: &[],
    plan,
    blur: Some(BlurRules {
        shift: 2,
        plan: game_api::blur::plan,
    }),
    gaps: &[unknown!(
        "t5.render.overlay_rest",
        "the overlay pass's other draws: water sheeting (between the burn and the shock) and the two after the poison",
        "Black Ops' overlay pass beyond the flame-style overlays and the blur"
    )],
};

/// The frame's overlays: the exclusive one, then the blur.
fn plan(frame: &OverlayFrame<'_>) -> Vec<OverlayPass> {
    let mut passes: Vec<OverlayPass> = exclusive(frame).into_iter().collect();
    passes.extend(blur_pass(frame));
    passes
}

/// The blur pass: the frame blurred by the root of the sum of the squares of
/// `r_blur` and the refdef's blur (the menus' and the script's as a root of
/// squares), then blended back over it; a radius under its smallest draws at
/// the smallest, faded by its share of it.
fn blur_pass(frame: &OverlayFrame<'_>) -> Option<OverlayPass> {
    // The poison's blur takes the script's place while it plays.
    let script_blur = frame
        .screen
        .poison
        .filter(|poison| poison.amount > 0.0)
        .map_or(frame.script_blur, |poison| {
            poison_blur(&poison, frame.now_ms)
        });
    let refdef = (frame.menu_blur * frame.menu_blur + script_blur * script_blur).sqrt();
    if !(0.0 < frame.r_blur || 0.0 < refdef) {
        return None;
    }
    let mut radius = (frame.r_blur * frame.r_blur + refdef * refdef).sqrt();
    let smallest = BLUR_FADE_HEIGHT / frame.viewport[3];
    let mut color = WHITE;
    if smallest > radius {
        let share = radius / smallest * 255.0;
        color[3] = (f64::from(share) + game_api::blur::NUDGE).round_ties_even() as i32 as u8;
        radius = smallest;
    }
    Some(OverlayPass {
        material: BLEND.to_owned(),
        constants: Vec::new(),
        blur: Some(radius),
        feedback: OverlayFeedback::Blurred,
        color,
    })
}

/// The frame's exclusive overlay: the burn, else the shock, else the
/// transporter, each while flame overlays are allowed and it plays, else the
/// poison while its amount is over 0.
fn exclusive(frame: &OverlayFrame<'_>) -> Option<OverlayPass> {
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
        (None, None, None) => {
            return screen
                .poison
                .filter(|poison| poison.amount > 0.0)
                .map(|poison| OverlayPass {
                    material: POISON.to_owned(),
                    constants: poison_rows(&poison, frame.now_ms, frame.viewport),
                    blur: None,
                    feedback: OverlayFeedback::Frame,
                    color: WHITE,
                });
        }
    };
    let atlas = frame
        .atlases
        .iter()
        .find(|(name, _)| *name == material)
        .map_or([0, 0], |(_, atlas)| *atlas);
    let scaler = scaled.then_some(screen.flame_scaler);
    Some(OverlayPass {
        material: material.to_owned(),
        constants: flame_rows(&overlay, atlas, scaler, frame.now_ms),
        blur: None,
        feedback: OverlayFeedback::Frame,
        color: WHITE,
    })
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

/// The pulse's smoothstep: `r_poisonFX_pulse` past 1, as `(3 - 2p)p²`.
fn pulse_weight(poison: &PoisonOverlay) -> f32 {
    let pulse = poison.pulse - 1.0;
    let pulse = if 0.0 > pulse { 0.0 } else { pulse };
    (pulse * -2.0 + 3.0) * pulse * pulse
}

/// The poison's blur: between `r_poisonFX_blurMin` and `_blurMax` along two
/// sines of the frame's time (the faster weighted by the pulse's smoothstep,
/// the slower by its rest), times the amount.
fn poison_blur(poison: &PoisonOverlay, now_ms: i32) -> f32 {
    let t = now_ms as f32 * 0.001;
    let s = pulse_weight(poison);
    let range = poison.blur_max - poison.blur_min;
    let slow =
        (((t * 0.166_666_67 * 7.853_982).sin() + 1.0) * 0.5 * range + poison.blur_min) * (1.0 - s);
    let fast = (((t * 0.333_333_34 * 7.853_982).sin() + 1.0) * 0.5 * range + poison.blur_min) * s;
    (slow + fast) * poison.amount
}

/// The rows the poison pass sets from its amount, the frame's time in
/// seconds and the scene viewport: the warp, two sines each way scaled by
/// `r_poisonFX_warpX` / `_warpY` (the faster weighted by the pulse's
/// smoothstep, the slower by its rest), with their reciprocal scales; then
/// the double vision's shift either side of the centre: across, a slow
/// cosine, two sines that play every other quarter and half second, and
/// `r_poisonFX_dvisionX` along the cosine of `_dvisionA`, over the
/// viewport's width; down, `_dvisionY` along its sine, over the height.
fn poison_rows(
    poison: &PoisonOverlay,
    now_ms: i32,
    [_, _, width, height]: [f32; 4],
) -> Vec<(OverlayConstant, [f32; 4])> {
    use std::f32::consts::{FRAC_PI_2, PI, TAU};
    let t = now_ms as f32 * 0.001;
    let s = pulse_weight(poison);
    let rest = 1.0 - s;
    let amount = poison.amount;
    let [warp_x, warp_y] = poison.warp;
    let x = ((t * 0.2 * TAU).sin() * warp_x * s + (t * 0.1 * TAU).sin() * warp_x * rest) * amount;
    let y = ((t * 0.118_343_197 * TAU + FRAC_PI_2).sin() * warp_y * rest
        + (t * 0.236_686_39 * TAU + FRAC_PI_2).sin() * warp_y * s)
        * amount;
    let warp = [x, y, 1.0 / (1.0 - x) * 0.5, 1.0 / (1.0 - y) * 0.5];
    let every_other = |at: f32, scale: f32| {
        if (at as i32) & 1 == 0 {
            (at * PI).sin() * scale
        } else {
            0.0
        }
    };
    let quarter = every_other(t * 0.25, 3.25);
    let half = every_other(t * 0.5, 3.75);
    let angle = poison.dvision_a * 0.017_453_292;
    let slow = (t * 0.083_333_336 * TAU).cos() * 1.25;
    let shift =
        (slow + rest * quarter + half * s + angle.cos() * poison.dvision_x) / width * amount;
    let lift = angle.sin() * poison.dvision_y / height * amount;
    vec![
        (OverlayConstant::GameTime, game_time(now_ms)),
        (OverlayConstant::PostFxControl(0), warp),
        (
            OverlayConstant::PostFxControl(1),
            [shift + 0.5, lift + 0.5, 0.5 - shift, 0.5 - lift],
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
