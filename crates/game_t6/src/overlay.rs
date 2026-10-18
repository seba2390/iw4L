//! Black Ops 2's screen overlays: its backend's overlay pass, which draws
//! one exclusive overlay over the frame by precedence (the burning one
//! first, then a revive one, then the electric shock, then the poison),
//! each a fullscreen draw of its post material reading the frame as drawn
//! so far; then its filter pass, which draws the client scripts' filters;
//! then its blur pass.

use game_api::post::{
    BlurRules, FlameOverlay, OverlayConstant, OverlayFeedback, OverlayFrame, OverlayPass,
    OverlayRules, PoisonOverlay, ReviveFx, ScreenFilter, ScreenOverlays,
};
use game_api::unknown;

const BURN: &str = "flamethrowerfx_color_distort_overlay_bloom";
const SHOCK: &str = "zombie_electric_shock_overlay";
const POISON: &str = "poison";
const REVIVE: &str = "revivefx";
const BLEND: &str = "feedbackblend";
const MATERIALS: [&str; 13] = [
    BURN,
    SHOCK,
    POISON,
    REVIVE,
    BLEND,
    crate::blur::MATERIALS[0],
    crate::blur::MATERIALS[1],
    crate::blur::MATERIALS[2],
    crate::blur::MATERIALS[3],
    crate::blur::MATERIALS[4],
    crate::blur::MATERIALS[5],
    crate::blur::MATERIALS[6],
    crate::blur::MATERIALS[7],
];
/// The revive pass's blur radius.
const REVIVE_BLUR: f32 = 6.4;
/// What the blur pass's smallest radius is, times the screen's height: a
/// smaller blur draws at that radius, faded in by its share of it.
const BLUR_FADE_HEIGHT: f32 = 2880.0;
const WHITE: [u8; 4] = [255; 4];
/// The filter materials the installed maps' client scripts map and enable:
/// Buried's Vulture Aid stink overlay (`_zm_perk_vulture`). Every other
/// filter their scripts map is enabled only by maps or modes that are not
/// installed (Turned, Mob of the Dead's afterlife, the gas mask).
const FILTER_MATERIALS: [&str; 1] = ["generic_filter_zombie_perk_vulture"];

pub(crate) const OVERLAYS: OverlayRules = OverlayRules {
    materials: &MATERIALS,
    filter_materials: &FILTER_MATERIALS,
    plan,
    blur: Some(BlurRules {
        shift: 2,
        plan: crate::blur::plan,
    }),
    gaps: &[unknown!(
        "t6.csc.vision.revive_blend",
        "while a vision channel blends from one vision to another, the revive settings blend by the vision field table's rules (switches at the halfway point for some blend modes, floats and vectors along the mode's curve); the overlay takes them from the vision the frame draws with",
        "Black Ops 2's vision channel blend"
    )],
};

/// The frame's overlays: the exclusive one, then the filters, then the
/// blur.
fn plan(frame: &OverlayFrame<'_>) -> Vec<OverlayPass> {
    let mut passes: Vec<OverlayPass> = exclusive(frame).into_iter().collect();
    passes.extend(
        frame
            .screen
            .filters
            .iter()
            .map(|filter| filter_pass(filter, frame.now_ms)),
    );
    passes.extend(blur_pass(frame));
    passes
}

/// The blur pass: the frame blurred by the root of the sum of the squares of
/// `r_blur` and the refdef's blur (the poison's while it plays, else the
/// script blur), then blended back over it; a radius under its smallest
/// draws at the smallest, faded by its share of it.
fn blur_pass(frame: &OverlayFrame<'_>) -> Option<OverlayPass> {
    let refdef = frame
        .screen
        .poison
        .filter(|poison| poison.amount > 0.0)
        .map_or(frame.script_blur, |poison| {
            poison_blur(&poison, frame.now_ms)
        });
    if !(0.0 < frame.r_blur || 0.0 < refdef) {
        return None;
    }
    let mut radius = (frame.r_blur * frame.r_blur + refdef * refdef).sqrt();
    let smallest = BLUR_FADE_HEIGHT / frame.viewport[3];
    let mut color = WHITE;
    if smallest > radius {
        let share = radius / smallest * 255.0;
        color[3] = (f64::from(share) + crate::blur::NUDGE).round_ties_even() as i32 as u8;
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

/// The poison's blur, which takes the refdef's blur while it plays: between
/// `blur_min` and `blur_max` along two sines of the frame's time (one weighted
/// by the pulse's smoothstep, one by its rest), times the amount.
fn poison_blur(poison: &PoisonOverlay, now_ms: i32) -> f32 {
    use std::f32::consts::PI;
    let t = now_ms as f32 * 0.001;
    let pulse = poison.pulse - 1.0;
    let pulse = if 0.0 > pulse { 0.0 } else { pulse };
    let s = (pulse * -2.0 + 3.0) * pulse * pulse;
    let range = poison.blur_max - poison.blur_min;
    let slow =
        (((t * 0.166_666_67 * PI * 2.5).sin() + 1.0) * 0.5 * range + poison.blur_min) * (1.0 - s);
    let fast = (((t * 0.333_333_34 * PI * 2.5).sin() + 1.0) * 0.5 * range + poison.blur_min) * s;
    (slow + fast) * poison.amount
}

/// The revive pass: the frame blurred, then drawn through the vision's edge
/// colour matrix and mask over the frame.
fn revive_pass(revive: &ReviveFx) -> OverlayPass {
    let rows = crate::revive::rows(revive);
    let mut constants: Vec<_> = rows
        .iter()
        .zip(0u8..)
        .map(|(row, at)| (OverlayConstant::PostFxControl(at), *row))
        .collect();
    constants.push((OverlayConstant::ViewportDimensions, [0.0, 0.0, 1.0, 1.0]));
    OverlayPass {
        material: REVIVE.to_owned(),
        constants,
        blur: Some(REVIVE_BLUR),
        feedback: OverlayFeedback::Frame,
        color: WHITE,
    }
}

/// The overlay the frame draws, if any: the burn when flame overlays are
/// allowed and it plays, else the revive while `r_reviveFX_debug` sets its
/// settings or the vision enables it with an amount over 0, else the shock
/// on the burn's terms, else the poison while its amount is over 0.
fn exclusive(frame: &OverlayFrame<'_>) -> Option<OverlayPass> {
    let screen = frame.screen;
    let playing = |overlay: Option<FlameOverlay>| {
        overlay.filter(|o| screen.flame_allowed && plays(o, frame.now_ms))
    };
    let revive = screen.revive_debug.or_else(|| {
        frame
            .revive
            .filter(|revive| revive.enable && revive.edge_amount > 0.0)
    });
    let (material, overlay) = match (playing(screen.burn), revive, playing(screen.shock)) {
        (Some(burn), ..) => (BURN, burn),
        (None, Some(revive), _) => return Some(revive_pass(&revive)),
        (None, None, Some(shock)) => (SHOCK, shock),
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
    Some(OverlayPass {
        material: material.to_owned(),
        constants: flame_rows(&overlay, atlas, screen, frame.now_ms),
        blur: None,
        feedback: OverlayFeedback::Frame,
        color: WHITE,
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
        blur: None,
        feedback: OverlayFeedback::Frame,
        color: WHITE,
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
