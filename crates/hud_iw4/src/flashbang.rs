extern crate alloc;

use alloc::borrow::ToOwned;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;

pub const SCREEN_BLEND_BLURRED: i32 = 0;

pub const SCREEN_BLEND_FLASHED: i32 = 1;

pub const SCREEN_BLEND_NONE: i32 = 2;

#[must_use]
pub fn is_flashbanged(cg_time: i32, start_time: i32, duration: i32, screen_type: i32) -> i32 {
    let remaining = start_time.wrapping_sub(cg_time).wrapping_add(duration);
    if remaining < 1 || screen_type != SCREEN_BLEND_FLASHED {
        0
    } else {
        remaining
    }
}

const FLASH_FADE_HALF: f32 = 0.5;

const FLASH_FADE_PI: f32 = 3.141592741012573;

#[must_use]
pub fn shellshock_flash_fade_sin_cos(percent: f32) -> f32 {
    let s = libm::sinf((percent - FLASH_FADE_HALF) * FLASH_FADE_PI);
    (s + 1.0) * FLASH_FADE_HALF
}

#[must_use]
pub fn shellshock_flash_blend(
    remaining_ms: i32,
    white_fade_ms: i32,
    shot_fade_ms: i32,
) -> Option<(f32, f32)> {
    if remaining_ms < 1 {
        return None;
    }
    let dt = remaining_ms as f32;
    let white_lin = if white_fade_ms <= 0 || (white_fade_ms as f32) <= dt {
        1.0
    } else {
        dt / white_fade_ms as f32
    };
    let shot_lin = if shot_fade_ms <= 0 || (shot_fade_ms as f32) <= dt {
        1.0
    } else {
        dt / shot_fade_ms as f32
    };
    Some((
        shellshock_flash_fade_sin_cos(white_lin),
        shellshock_flash_fade_sin_cos(shot_lin),
    ))
}

#[must_use]
pub fn shellshock_blend_time(
    now_ms: i32,
    start_ms: i32,
    duration_ms: i32,
    fade_ms: i32,
    blend_ms: i32,
) -> i32 {
    let remaining = duration_ms.wrapping_sub(now_ms).wrapping_add(start_ms);
    if start_ms == 0 || duration_ms <= 0 || remaining <= 0 {
        return 0;
    }
    if remaining < fade_ms {
        libm::floorf(remaining as f32 / fade_ms as f32 * blend_ms as f32 + 0.5) as i32
    } else {
        blend_ms
    }
}

#[must_use]
pub fn shellshock_screen_alpha(now_ms: i32, saved_ms: i32, blend_ms: i32) -> Option<u8> {
    let age = now_ms.wrapping_sub(saved_ms);
    if blend_ms <= 0 || age < 0 || age >= blend_ms {
        return None;
    }
    let alpha = (libm::pow(
        0.009999999776482582,
        f64::from(age as f32 / blend_ms as f32),
    ) as f32)
        .min(0.99);
    Some(libm::floorf(libm::fmaf(alpha, 255.0, 0.5)) as u8)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShellshockLookParms {
    pub affect: bool,

    pub fade_ms: i32,

    pub mouse_sensitivity: f32,

    pub max_pitch_speed: f32,

    pub max_yaw_speed: f32,

    /// While fading, the max speeds are multiplied by the fade's reciprocal
    /// instead of divided by the fade.
    pub reciprocal_fade: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShellshockLookState {
    pub sensitivity: f32,

    pub max_pitch_speed: f32,

    pub max_yaw_speed: f32,
}

const LOOK_ENDED: ShellshockLookState = ShellshockLookState {
    sensitivity: 1.0,
    max_pitch_speed: 0.0,
    max_yaw_speed: 0.0,
};

#[must_use]
pub fn shellshock_remaining_ms(cg_time: i32, start_time: i32, duration: i32) -> i32 {
    if start_time == 0 {
        return 0;
    }
    let elapsed = cg_time.wrapping_sub(start_time);
    if elapsed < 0 {
        return 0;
    }
    let remaining = duration.wrapping_sub(elapsed);
    if remaining < 1 { 0 } else { remaining }
}

#[must_use]
pub fn update_shellshock_look_control(
    cg_time: i32,
    start_time: i32,
    duration: i32,
    parms: ShellshockLookParms,
) -> ShellshockLookState {
    let elapsed = cg_time.wrapping_sub(start_time);
    if start_time == 0 || elapsed < 0 || !parms.affect {
        return LOOK_ENDED;
    }
    let remaining = duration.wrapping_sub(elapsed);
    if remaining < parms.fade_ms {
        if remaining < 1 {
            return LOOK_ENDED;
        }
        let fade = remaining as f32 / parms.fade_ms as f32;
        if fade == 1.0 {
            return ShellshockLookState {
                sensitivity: parms.mouse_sensitivity,
                max_pitch_speed: parms.max_pitch_speed,
                max_yaw_speed: parms.max_yaw_speed,
            };
        }
        if parms.reciprocal_fade {
            let inverse = 1.0 / fade;
            return ShellshockLookState {
                sensitivity: (parms.mouse_sensitivity - 1.0) * fade + 1.0,
                max_pitch_speed: parms.max_pitch_speed * inverse,
                max_yaw_speed: parms.max_yaw_speed * inverse,
            };
        }
        ShellshockLookState {
            sensitivity: fade * (parms.mouse_sensitivity - 1.0) + 1.0,
            max_pitch_speed: parms.max_pitch_speed / fade,
            max_yaw_speed: parms.max_yaw_speed / fade,
        }
    } else {
        ShellshockLookState {
            sensitivity: parms.mouse_sensitivity,
            max_pitch_speed: parms.max_pitch_speed,
            max_yaw_speed: parms.max_yaw_speed,
        }
    }
}

/// The shock view kick's noise: 128 pitch and yaw pairs, the first three
/// repeated so the cubic read past the last one wraps.
const SHELLSHOCK_KICK_NOISE: [[f32; 2]; 131] = [
    [-0.563551, -0.00443],
    [-0.282062, -0.757933],
    [0.413047, 0.244246],
    [0.527895, -0.723897],
    [-0.329777, 0.6698],
    [-0.394248, -0.76309],
    [0.126207, 0.497769],
    [0.004986, -0.01413],
    [0.559913, 0.112825],
    [-0.333089, -0.573283],
    [0.335404, -0.107176],
    [-0.56906, -0.213141],
    [-0.166676, 0.785084],
    [0.299592, 0.037593],
    [-0.516867, 0.510759],
    [0.138009, 0.034823],
    [-0.156167, 0.829048],
    [-0.999458, 0.020317],
    [0.300029, 0.252944],
    [0.030215, -0.295732],
    [-0.917362, -0.050711],
    [0.044177, -0.269289],
    [0.588424, 0.362577],
    [-0.379913, 0.619214],
    [0.204432, -0.019423],
    [0.018499, 0.468079],
    [0.916187, -0.247878],
    [0.003799, 0.10821],
    [0.057363, 0.60624],
    [0.324595, 0.158733],
    [-0.130529, -0.183388],
    [0.715672, -0.363858],
    [0.984258, 0.106096],
    [-0.003313, 0.345535],
    [-0.320351, -0.573936],
    [0.063455, -0.003239],
    [-0.570173, -0.759313],
    [0.106456, 0.283726],
    [-0.668163, 0.142388],
    [-0.501119, -0.720006],
    [-0.253281, 0.524032],
    [-0.064084, -0.165943],
    [-0.194672, 0.43355],
    [-0.2818, -0.417744],
    [0.045786, 0.402986],
    [0.105064, -0.558937],
    [0.312244, 0.688318],
    [-0.263294, -0.256811],
    [0.659186, 0.070672],
    [0.093625, -0.046812],
    [-0.87502, 0.288509],
    [0.329359, 0.105941],
    [-0.181309, 0.259865],
    [0.261597, -0.07407],
    [-0.296082, 0.031858],
    [0.038584, 0.565947],
    [-0.253445, -0.717865],
    [-0.211836, 0.336521],
    [0.890123, 0.00495],
    [-0.979825, -0.17079],
    [0.045346, 0.02224],
    [-0.345796, 0.522712],
    [0.108525, 0.165424],
    [-0.572796, -0.473399],
    [0.368605, -0.865844],
    [0.075571, -0.327703],
    [-0.466353, -0.565594],
    [-0.358837, 0.610302],
    [0.603884, 0.440023],
    [0.002465, -0.144449],
    [-0.294915, 0.79997],
    [-0.028347, -0.112071],
    [-0.009472, 0.686061],
    [0.07115, 0.01991],
    [0.96269, 0.024926],
    [0.309208, 0.871549],
    [-0.123782, -0.312301],
    [-0.433055, -0.895981],
    [0.962495, -0.263777],
    [-0.51146, -0.359478],
    [-0.044013, 0.02021],
    [-0.10934, -0.76123],
    [0.171003, -0.107461],
    [0.418912, 0.435294],
    [0.44494, -0.139643],
    [0.518574, 0.365965],
    [-0.506997, 0.655597],
    [0.510525, 0.508961],
    [-0.296173, -0.675837],
    [0.851332, 0.307192],
    [-0.008474, -0.188744],
    [0.552703, 0.427086],
    [0.080334, -0.002805],
    [0.035656, 0.610991],
    [0.770593, 0.398874],
    [-0.522137, 0.324362],
    [0.006045, 0.042788],
    [0.482456, 0.848994],
    [0.226058, -0.522367],
    [-0.674606, -0.547814],
    [-0.441998, 0.59884],
    [-0.183957, -0.270234],
    [0.51885, 0.634946],
    [0.430386, 0.125257],
    [-0.185496, -0.264459],
    [0.02369, 0.312978],
    [-0.444287, 0.849928],
    [0.291978, -0.897679],
    [-0.045826, -0.047128],
    [-0.114246, 0.511975],
    [0.738133, 0.607667],
    [-0.786889, -0.384057],
    [0.182993, 0.265086],
    [-0.39945, -0.309031],
    [-0.482895, 0.265662],
    [0.059671, 0.09776],
    [0.793174, -0.015972],
    [0.201658, 0.492445],
    [-0.707371, -0.02619],
    [-0.320882, 0.37228],
    [0.572813, -0.537255],
    [0.337619, 0.116293],
    [-0.606537, 0.173373],
    [-0.166593, -0.335112],
    [-0.583993, 0.182916],
    [-0.573519, -0.623348],
    [-0.392707, 0.449474],
    [0.151474, 0.840401],
    [-0.563551, -0.00443],
    [-0.282062, -0.757933],
    [0.413047, 0.244246],
];

/// A shock's view kick this frame, as pitch and yaw offsets of the view's
/// forward axis: none once the shock is over, faded with a smoothstep over
/// its last `fade_ms`, and read from the noise table with a cubic between
/// four points, the walk starting at a point picked by the duration.
#[must_use]
pub fn shellshock_view_kick(kick: ShellshockKick, elapsed: i32, duration: i32) -> [f32; 2] {
    let remaining = duration.wrapping_sub(elapsed);
    if remaining < 1 {
        return [0.0; 2];
    }
    let fade = if remaining < kick.fade_ms {
        remaining as f32 / kick.fade_ms as f32
    } else {
        1.0
    };
    let scale = (3.0 - fade * 2.0) * fade * fade * kick.radius;
    let walk = elapsed as f32 * kick.rate;
    // The x87 store rounds to nearest even after taking just under a half.
    let step = libm::rint(f64::from(walk) - KICK_FLOOR_BIAS);
    let step = if step.is_finite() && step.abs() < 2_147_483_648.0 {
        step as i32
    } else {
        i32::MIN
    };
    let index = (duration.wrapping_mul(61).wrapping_add(step) & 0x7f) as usize;
    let frac = walk - step as f32;
    let mut out = [0.0; 2];
    for (axis, out) in out.iter_mut().enumerate() {
        let p0 = SHELLSHOCK_KICK_NOISE[index][axis];
        let p1 = SHELLSHOCK_KICK_NOISE[index + 1][axis];
        let p2 = SHELLSHOCK_KICK_NOISE[index + 2][axis];
        let p3 = SHELLSHOCK_KICK_NOISE[index + 3][axis];
        let a = p3 - p2 + p1 - p0;
        *out = ((((p0 - p1 - a) + a * frac) * frac + (p2 - p0)) * frac + p1) * scale;
    }
    out
}

/// Just under a half (0.5 - 2^-30), as a double.
const KICK_FLOOR_BIAS: f64 = 0.4999999990686774;

/// Turns a view axis (forward, left, up) by a shock's view kick: the kick
/// is a forward axis tilted by it, completed to a frame with the world's up
/// and applied ahead of the view axis. With `ordered_sums` the lengths are
/// summed (x² + 1) + y² and (l1² + l0²) + l2², and each product element
/// (a0·b0 + a1·b1) + a2·b2, as Black Ops 2's client sums them.
#[must_use]
pub fn shellshock_perturb_axis(
    kick: [f32; 2],
    axis: [[f32; 3]; 3],
    ordered_sums: bool,
) -> [[f32; 3]; 3] {
    let [kx, ky] = kick;
    if kx == 0.0 && ky == 0.0 {
        return axis;
    }
    let inverse = |length: f32| 1.0 / if 0.0 <= -length { 1.0 } else { length };
    let length = if ordered_sums {
        kx * kx + 1.0 + ky * ky
    } else {
        ky * ky + kx * kx + 1.0
    };
    let scale = inverse(libm::sqrtf(length));
    let f = [scale, scale * kx, scale * ky];
    let left = [
        f[2] * 0.0 - f[1],
        f[0] - f[2] * 0.0,
        f[1] * 0.0 - f[0] * 0.0,
    ];
    let length = if ordered_sums {
        left[1] * left[1] + left[0] * left[0] + left[2] * left[2]
    } else {
        left[2] * left[2] + left[1] * left[1] + left[0] * left[0]
    };
    let scale = inverse(libm::sqrtf(length));
    let l = [left[0] * scale, left[1] * scale, left[2] * scale];
    let u = [
        l[2] * f[1] - l[1] * f[2],
        l[0] * f[2] - l[2] * f[0],
        l[1] * f[0] - l[0] * f[1],
    ];
    let (a, b) = ([f, l, u], axis);
    if ordered_sums {
        return core::array::from_fn(|i| {
            core::array::from_fn(|j| a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j])
        });
    }
    // Each element sums its products in the order the game does.
    [
        [
            a[0][0] * b[0][0] + a[0][1] * b[1][0] + a[0][2] * b[2][0],
            a[0][1] * b[1][1] + a[0][2] * b[2][1] + a[0][0] * b[0][1],
            a[0][1] * b[1][2] + a[0][2] * b[2][2] + a[0][0] * b[0][2],
        ],
        [
            a[1][1] * b[1][0] + a[1][0] * b[0][0] + a[1][2] * b[2][0],
            a[1][2] * b[2][1] + a[1][1] * b[1][1] + a[1][0] * b[0][1],
            a[1][2] * b[2][2] + a[1][1] * b[1][2] + a[1][0] * b[0][2],
        ],
        [
            a[2][0] * b[0][0] + a[2][2] * b[2][0] + a[2][1] * b[1][0],
            a[2][2] * b[2][1] + a[2][1] * b[1][1] + a[2][0] * b[0][1],
            a[2][2] * b[2][2] + a[2][1] * b[1][2] + a[2][0] * b[0][2],
        ],
    ]
}

/// A shock's view kick, for shock files that carry one: the kick fades
/// over its last `fade_ms`, walks the noise table `rate` steps per msec and
/// is `radius` at full strength.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShellshockKick {
    pub fade_ms: i32,
    pub rate: f32,
    pub radius: f32,
    /// The view axis is turned with every sum in index order
    /// (`shellshock_perturb_axis`).
    pub ordered_sums: bool,
}

/// A shock's own sound, for shock files that time it themselves: a loop on
/// the shocked player faded in over `fade_in_ms`, a reverb room and a mix
/// snapshot held while it plays, and an end sound once the shock is within
/// `-mod_end_delay_ms` of its end, the loop and room then fading out over
/// `fade_out_ms`.
#[derive(Clone, Debug, PartialEq)]
pub struct ShellshockTimedSound {
    pub loop_alias: String,
    pub end_alias: String,
    pub fade_in_ms: i32,
    pub fade_out_ms: i32,
    pub mod_end_delay_ms: i32,
    pub room: String,
    pub dry: f32,
    pub wet: f32,
    pub snapshot: String,
}

/// The vision a shock blends the naked vision to over `in_ms` while it
/// lasts, blending back over `out_ms` after.
#[derive(Clone, Debug, PartialEq)]
pub struct ShellshockVision {
    pub name: String,
    pub in_ms: i32,
    pub out_ms: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShellshockSoundParms {
    pub affect: bool,

    pub channel_volumes: Option<BTreeMap<String, f32>>,

    pub loop_alias: String,

    pub end_alias: String,

    pub abort_alias: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShockParams {
    pub screen_type: i32,

    pub blur_blend_ms: i32,

    pub blur_fade_ms: i32,

    pub white_fade_ms: i32,

    pub shot_fade_ms: i32,

    pub look: ShellshockLookParms,

    pub sound: ShellshockSoundParms,

    pub movement: bool,

    /// The fraction of their speed a shocked player moves at, for movement
    /// that scales by the shock (1 in this format, whose `movement` flag
    /// applies a fixed scale).
    pub movement_scale: f32,

    /// The shocked player's torso plays the shellshock animation and the
    /// weapon's fire animation is skipped.
    pub animation: bool,

    pub kick: Option<ShellshockKick>,

    pub timed_sound: Option<ShellshockTimedSound>,

    pub vision: Option<ShellshockVision>,
}

impl ShockParams {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut values = BTreeMap::new();
        for line in text.lines() {
            let line = line.trim();
            let Some((key, value)) = line.split_once(char::is_whitespace) else {
                continue;
            };
            let value = value.trim().trim_matches('"');
            values.insert(key.to_ascii_lowercase(), value.to_owned());
        }
        let text = |key: &str| {
            values
                .get(&format!("bg_shock_{}", key.to_ascii_lowercase()))
                .cloned()
                .ok_or_else(|| format!("bg_shock_{key} is missing"))
        };
        let number = |key: &str| -> Result<f32, String> {
            let value = text(key)?;
            value
                .parse::<f32>()
                .map_err(|_| format!("bg_shock_{key} \"{value}\" is not a number"))
        };
        let ms = |key: &str| number(key).map(|seconds| libm::roundf(seconds * 1000.0) as i32);
        let blur_ms = |key: &str| -> Result<i32, String> {
            let seconds = number(key)?;
            if !seconds.is_finite() {
                return Err(format!("bg_shock_{key} must be finite"));
            }
            Ok(libm::floorf(seconds.max(0.001) * 1000.0 + 0.5) as i32)
        };
        let flag = |key: &str| number(key).map(|value| value != 0.0);
        let screen_type = match text("screenType")?.to_ascii_lowercase().as_str() {
            "blurred" => SCREEN_BLEND_BLURRED,
            "flashed" => SCREEN_BLEND_FLASHED,
            "none" => SCREEN_BLEND_NONE,
            other => {
                return Err(format!(
                    "bg_shock_screenType \"{other}\" is not blurred, flashed or none"
                ));
            }
        };
        let mut channel_volumes = BTreeMap::new();
        for (key, value) in &values {
            let Some(channel) = key.strip_prefix("bg_shock_volume_") else {
                continue;
            };
            if channel.is_empty() || channel.len() > 64 || channel_volumes.len() >= 64 {
                return Err("invalid shellshock channel volume name or count".into());
            }
            let gain = value
                .parse::<f32>()
                .map_err(|_| format!("{key} \"{value}\" is not a number"))?;
            if !gain.is_finite() {
                return Err(format!("{key} must be finite"));
            }
            channel_volumes.insert(channel.to_owned(), gain.clamp(0.0, 1.0));
        }
        Ok(Self {
            screen_type,
            blur_blend_ms: blur_ms("screenBlurBlendTime")?,
            blur_fade_ms: blur_ms("screenBlurBlendFadeTime")?,
            white_fade_ms: ms("screenFlashWhiteFadeTime")?,
            shot_fade_ms: ms("screenFlashShotFadeTime")?,
            look: ShellshockLookParms {
                affect: flag("lookControl")?,
                fade_ms: ms("lookControl_fadeTime")?,
                mouse_sensitivity: number("lookControl_mousesensitivityscale")?,
                max_pitch_speed: number("lookControl_maxpitchspeed")?,
                max_yaw_speed: number("lookControl_maxyawspeed")?,
                reciprocal_fade: false,
            },
            sound: ShellshockSoundParms {
                channel_volumes: Some(channel_volumes),
                affect: flag("sound")?,
                loop_alias: text("soundLoop")?,
                end_alias: text("soundEnd")?,
                abort_alias: text("soundEndAbort")?,
            },
            movement: flag("movement")?,
            movement_scale: 1.0,
            animation: false,
            kick: None,
            timed_sound: None,
            vision: None,
        })
    }
}
