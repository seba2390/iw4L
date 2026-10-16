//! Black Ops shellshock files (`shock/<name>.shock`), read the way its
//! precache reads them: every line sets one of the game's 32 `bg_shock_*`
//! dvars, each first reset to its default, and the shock's parameters are
//! then taken from the dvars.

use game_api::{ShockFile, ShockLook, ShockScreen, Unknown, unknown};

use crate::menu_expression::{atof, atoi};

/// The shock a precached name without a file of its own plays (its precache
/// prints "couldn't open" and loads this one instead).
pub(crate) const FALLBACK: &str = "default";

/// A `bg_shock_*` dvar: its type, and a float's default and domain.
#[derive(Clone, Copy)]
enum Dvar {
    /// `[blurred, flashed, none]`, by index.
    ScreenType,
    Float {
        default: f32,
        min: f32,
        max: f32,
    },
    Bool,
    Text,
}

/// The dvars a shock file must name, in the game's order.
const DVARS: [(&str, Dvar); 32] = [
    ("bg_shock_screenType", Dvar::ScreenType),
    ("bg_shock_screenBlurBlendTime", float(0.4, 0.001, 10.0)),
    (
        "bg_shock_screenBlurBlendFadeTime",
        float(1.0, 0.001, 1000.0),
    ),
    ("bg_shock_screenFlashWhiteFadeTime", float(0.0, 0.0, 1000.0)),
    ("bg_shock_screenFlashShotFadeTime", float(0.0, 0.0, 1000.0)),
    ("bg_shock_viewKickPeriod", float(0.75, 0.001, 1000.0)),
    ("bg_shock_viewKickRadius", float(0.05, 0.0, 1.0)),
    ("bg_shock_viewKickFadeTime", float(3.0, 0.001, 1000.0)),
    ("bg_shock_soundLoop", Dvar::Text),
    ("bg_shock_soundLoopSilent", Dvar::Text),
    ("bg_shock_soundEnd", Dvar::Text),
    ("bg_shock_soundEndAbort", Dvar::Text),
    ("bg_shock_sound", Dvar::Bool),
    ("bg_shock_soundFadeInTime", float(0.25, 0.001, 1000.0)),
    ("bg_shock_soundFadeOutTime", float(2.5, 0.0, 1000.0)),
    ("bg_shock_soundLoopFadeTime", float(1.5, 0.0, 1000.0)),
    ("bg_shock_soundLoopEndDelay", float(-3.0, -10.0, 1000.0)),
    ("bg_shock_soundRoomType", Dvar::Text),
    ("bg_shock_soundDryLevel", float(1.0, 0.0, 1.0)),
    ("bg_shock_soundWetLevel", float(0.5, 0.0, 1.0)),
    ("bg_shock_soundModEndDelay", float(2.0, -1000.0, 1000.0)),
    ("bg_shock_soundSnapshot", Dvar::Text),
    ("bg_shock_lookControl", Dvar::Bool),
    (
        "bg_shock_lookControl_maxpitchspeed",
        float(90.0, 0.0, f32::MAX),
    ),
    (
        "bg_shock_lookControl_maxyawspeed",
        float(90.0, 0.0, f32::MAX),
    ),
    (
        "bg_shock_lookControl_mousesensitivityscale",
        float(0.5, 0.0, 2.0),
    ),
    ("bg_shock_lookControl_fadeTime", float(2.0, 0.001, 1000.0)),
    ("bg_shock_movement", float(1.0, 0.0, 1.0)),
    ("bg_shock_animation", Dvar::Bool),
    ("bg_shock_visionset_name", Dvar::Text),
    ("bg_shock_visionset_inTime", float(0.0, 0.0, 30.0)),
    ("bg_shock_visionset_outTime", float(0.0, 0.0, 30.0)),
];

const fn float(default: f32, min: f32, max: f32) -> Dvar {
    Dvar::Float { default, min, max }
}

/// A dvar's value after the file set it.
#[derive(Clone, Copy)]
enum Value<'a> {
    Int(i32),
    Float(f32),
    Bool(bool),
    Text(&'a str),
}

const KICK: &Unknown = unknown!(
    "t5.vision.shellshock_kick",
    "a Black Ops shellshock does not kick the view",
    "its view kick: the noise table and how the kick turns the view axis"
);
const SOUND: &Unknown = unknown!(
    "t5.audio.shellshock",
    "a Black Ops shellshock plays no sound: no loop, end sound, reverb room or snapshot",
    "its shellshock sound rules, with the client sound snapshots and reverb"
);
const VISIONSET: &Unknown = unknown!(
    "t5.vision.shellshock_visionset",
    "a Black Ops shellshock's vision set is not applied",
    "the shellshock's vision channel set from the client's shellshock update"
);

/// Reads a shock file. A file that leaves a dvar out fails, as its precache
/// does.
pub(crate) fn parse(text: &str) -> Result<ShockFile, String> {
    let mut set: [Option<Value>; 32] = [None; 32];
    for line in text.lines() {
        let line = line.trim();
        let Some((name, value)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let Some(index) = DVARS
            .iter()
            .position(|(dvar, _)| dvar.eq_ignore_ascii_case(name))
        else {
            continue;
        };
        let value = value.trim().trim_matches('"');
        set[index] = Some(match DVARS[index].1 {
            // A name outside the list resets the enum.
            Dvar::ScreenType => Value::Int(
                ["blurred", "flashed", "none"]
                    .iter()
                    .position(|v| v.eq_ignore_ascii_case(value))
                    .map_or(0, |i| i as i32),
            ),
            // A value outside the domain leaves the reset default.
            Dvar::Float { default, min, max } => {
                let v = atof(value);
                Value::Float(if (min..=max).contains(&v) { v } else { default })
            }
            Dvar::Bool => Value::Bool(atoi(value) != 0),
            Dvar::Text => Value::Text(value),
        });
    }
    let missing: Vec<&str> = DVARS
        .iter()
        .zip(&set)
        .filter(|(_, value)| value.is_none())
        .map(|((name, _), _)| *name)
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "the following dvars were not specified: {}",
            missing.join(", ")
        ));
    }
    let get = |name: &str| {
        let index = DVARS
            .iter()
            .position(|(dvar, _)| *dvar == name)
            .expect("a shock dvar");
        set[index].expect("every dvar is set")
    };
    let number = |name: &str| match get(name) {
        Value::Float(v) => v,
        _ => unreachable!("{name} is a float dvar"),
    };
    let flag = |name: &str| match get(name) {
        Value::Bool(v) => v,
        _ => unreachable!("{name} is a bool dvar"),
    };
    let ms = |name: &str| milliseconds(number(name));
    let screen = match get("bg_shock_screenType") {
        Value::Int(1) => ShockScreen::Flashed,
        Value::Int(2) => ShockScreen::None,
        _ => ShockScreen::Blurred,
    };
    let mut unrun = Vec::new();
    if number("bg_shock_viewKickRadius") != 0.0 {
        unrun.push(KICK);
    }
    if flag("bg_shock_sound") {
        unrun.push(SOUND);
    }
    if matches!(get("bg_shock_visionset_name"), Value::Text(name) if !name.is_empty()) {
        unrun.push(VISIONSET);
    }
    Ok(ShockFile {
        screen,
        blur_blend_ms: ms("bg_shock_screenBlurBlendTime"),
        blur_fade_ms: ms("bg_shock_screenBlurBlendFadeTime"),
        white_fade_ms: ms("bg_shock_screenFlashWhiteFadeTime"),
        shot_fade_ms: ms("bg_shock_screenFlashShotFadeTime"),
        look: ShockLook {
            affect: flag("bg_shock_lookControl"),
            fade_ms: ms("bg_shock_lookControl_fadeTime"),
            mouse_sensitivity: number("bg_shock_lookControl_mousesensitivityscale"),
            max_pitch_speed: number("bg_shock_lookControl_maxpitchspeed"),
            max_yaw_speed: number("bg_shock_lookControl_maxyawspeed"),
            reciprocal_fade: true,
        },
        movement_scale: number("bg_shock_movement"),
        animation: flag("bg_shock_animation"),
        unrun,
    })
}

/// Seconds to milliseconds as the game rounds them: `seconds * 1000` in
/// single precision, plus 2^-30, rounded to the nearest integer.
pub(crate) fn milliseconds(seconds: f32) -> i32 {
    let bias = f64::from(2.0f32.powi(-30));
    let rounded = (f64::from(seconds * 1000.0) + bias).round_ties_even();
    if rounded.is_finite() && rounded.abs() < 2_147_483_648.0 {
        rounded as i32
    } else {
        i32::MIN
    }
}
