//! Black Ops 2's screen overlays: its backend's overlay pass, which draws
//! one exclusive overlay over the frame by precedence (the burning one
//! first, then a revive one, then the electric shock, then the poison),
//! each a fullscreen draw of its post material reading the frame as drawn
//! so far; then its filter pass, which draws the client scripts' filters.

use game_api::post::{
    FlameOverlay, OverlayConstant, OverlayFrame, OverlayPass, OverlayRules, PoisonOverlay,
    ScreenFilter, ScreenOverlays,
};
use game_api::unknown;

const BURN: &str = "flamethrowerfx_color_distort_overlay_bloom";
const SHOCK: &str = "zombie_electric_shock_overlay";
const POISON: &str = "poison";
const MATERIALS: [&str; 3] = [BURN, SHOCK, POISON];
/// The filter materials the installed maps' client scripts map and enable:
/// Buried's Vulture Aid stink overlay (`_zm_perk_vulture`). Every other
/// filter their scripts map is enabled only by maps or modes that are not
/// installed (Turned, Mob of the Dead's afterlife, the gas mask).
const FILTER_MATERIALS: [&str; 1] = ["generic_filter_zombie_perk_vulture"];

pub(crate) const OVERLAYS: OverlayRules = OverlayRules {
    materials: &MATERIALS,
    filter_materials: &FILTER_MATERIALS,
    plan,
    gaps: &[
        unknown!(
            "t6.csc.overlay.revive",
            "the revive overlay that outranks the electric shock: what turns it on (the backend's revive flag and amount) was not found, so the shock always draws when the burn does not",
            "the client code that fills the refdef's revive overlay fields"
        ),
        unknown!(
            "t6.csc.overlay.poison_blur",
            "the poison's blur, which the backend's blur pass draws with the script blur: it is not drawn",
            "Black Ops 2's own blur pass"
        ),
    ],
};

/// The frame's overlays: the exclusive one, then the filters.
fn plan(frame: &OverlayFrame<'_>) -> Vec<OverlayPass> {
    let mut passes: Vec<OverlayPass> = exclusive(frame).into_iter().collect();
    passes.extend(
        frame
            .screen
            .filters
            .iter()
            .map(|filter| filter_pass(filter, frame.now_ms)),
    );
    passes
}

/// The overlay the frame draws, if any: the burn when flame overlays are
/// allowed and it plays, else the shock on the same terms, else the poison
/// while its amount is over 0.
fn exclusive(frame: &OverlayFrame<'_>) -> Option<OverlayPass> {
    let screen = frame.screen;
    let playing = |overlay: Option<FlameOverlay>| {
        overlay.filter(|o| screen.flame_allowed && plays(o, frame.now_ms))
    };
    let (material, overlay) = match (playing(screen.burn), playing(screen.shock)) {
        (Some(burn), _) => (BURN, burn),
        (None, Some(shock)) => (SHOCK, shock),
        (None, None) => {
            return screen
                .poison
                .filter(|poison| poison.amount > 0.0)
                .map(|poison| OverlayPass {
                    material: POISON.to_owned(),
                    constants: poison_rows(&poison, frame.now_ms, frame.viewport),
                });
        }
    };
    let atlas = frame
        .atlases
        .iter()
        .find(|(name, _)| *name == material)
        .map_or([0, 0], |(_, atlas)| *atlas);
    Some(OverlayPass {
        material: material.to_owned(),
        constants: flame_rows(&overlay, atlas, screen, frame.now_ms),
    })
}

/// A filter's draw: its first pass's material over the full view, with the
/// pass's sixteen constants in the four script vector rows and the target's
/// size (width, height, 0, 0); the frame's game time row holds as the
/// backend set it.
fn filter_pass(filter: &ScreenFilter, now_ms: i32) -> OverlayPass {
    let mut constants = vec![
        (OverlayConstant::GameTime, game_time(now_ms)),
        (OverlayConstant::RenderTargetSize, [0.0; 4]),
    ];
    let (rows, _) = filter.constants.as_chunks::<4>();
    constants.extend(
        rows.iter()
            .zip(0u8..)
            .map(|(row, at)| (OverlayConstant::ScriptVector(at), *row)),
    );
    constants.push((OverlayConstant::ViewportDimensions, [0.0, 0.0, 1.0, 1.0]));
    OverlayPass {
        material: filter.material.clone(),
        constants,
    }
}

/// The client keeps a flame overlay until its fade has run longer than its
/// fade time; one that has not ended has no fade yet.
fn plays(overlay: &FlameOverlay, now_ms: i32) -> bool {
    overlay.fade_ms <= 0 || now_ms.wrapping_sub(overlay.fade_start_ms) <= overlay.fade_ms
}

/// The rows a flame-style pass sets: its atlas (columns, rows, frames) and
/// frame rate, its fade, its distortion and magnitude, and the full view.
fn flame_rows(
    overlay: &FlameOverlay,
    [rows, columns]: [u8; 2],
    screen: &ScreenOverlays,
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
            [fade(overlay, screen, now_ms), 0.0, 0.0, 0.0],
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

/// The rows the poison pass sets from its amount, the frame's time in
/// seconds and the scene viewport: two pairs of sine warps (one weighted by
/// the pulse's smoothstep, one by its rest) and their reciprocal scales,
/// then the double vision's horizontal shift (a slow cosine, two sines that
/// play every other quarter and half second, and `r_poisonFX_dvisionX`)
/// over the viewport's width, either side of the centre.
fn poison_rows(
    poison: &PoisonOverlay,
    now_ms: i32,
    [_, _, width, height]: [f32; 4],
) -> Vec<(OverlayConstant, [f32; 4])> {
    use std::f32::consts::{FRAC_PI_2, PI};
    let t = now_ms as f32 * 0.001;
    let pulse = poison.pulse - 1.0;
    let pulse = if 0.0 > pulse { 0.0 } else { pulse };
    let s = (pulse * -2.0 + 3.0) * (pulse * pulse);
    let rest = 1.0 - s;
    let amount = poison.amount;
    let x =
        ((t * 0.2 * PI * 2.0).sin() * 0.2 * s + (t * 0.1 * PI * 2.0).sin() * 0.2 * rest) * amount;
    let y = ((t * 0.236_686_39 * PI * 2.0 + FRAC_PI_2).sin() * -0.15 * s
        + (t * 0.118_343_197 * PI * 2.0 + FRAC_PI_2).sin() * -0.15 * rest)
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
    let slow = (t * 0.083_333_336 * PI * 2.0).cos() * 1.25;
    let shift = (slow + (rest * quarter + half * s + poison.dvision_x)) / width * amount;
    // The backend's vertical shift is the sine of 0 times 0.
    let lift = 0.0 / height * amount;
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

/// The fade's progress, 0 to 1 (0 before it ends); a flame scaler other
/// than 1 replaces it with 1 less the scaler.
fn fade(overlay: &FlameOverlay, screen: &ScreenOverlays, now_ms: i32) -> f32 {
    if screen.flame_scaler != 1.0 {
        return 1.0 - screen.flame_scaler;
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
