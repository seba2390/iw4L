use bevy::prelude::Resource;
use hud_iw4::{
    HUDELEM_SOUND_SLOTS, HudElem, TextPulseFx, TextPulseSound, flags as elem_flags, rand_with_seed,
    seh_print_strlen,
};

use crate::draw2d::TextRunFx;

#[derive(Resource)]
pub struct HudElemSoundLatch {
    last_played_time: [i32; HUDELEM_SOUND_SLOTS],
}

impl Default for HudElemSoundLatch {
    fn default() -> Self {
        Self {
            last_played_time: [0; HUDELEM_SOUND_SLOTS],
        }
    }
}

impl HudElemSoundLatch {
    fn slot(&mut self, sound_id: i32) -> Option<&mut i32> {
        let index = usize::try_from(sound_id).ok()?;
        self.last_played_time.get_mut(index)
    }
}

pub(crate) fn resolve_hud_text(
    strings: &assets::PreparedLocalizedStrings,
    raw: &str,
) -> Option<String> {
    if let Some(plain) = raw.strip_prefix(sim::HUD_STRING_PLAIN) {
        return Some(plain.to_owned());
    }
    if raw.is_empty() {
        return Some(String::new());
    }
    // `KEY␖a␖b`: a localized string whose `&&1`, `&&2` take the values.
    let mut parts = raw.split(sim::HUD_PRINT_ARG_SEPARATOR);
    let mut text = strings.0.text(parts.next()?)?.to_owned();
    for (index, value) in parts.enumerate() {
        text = text.replace(&format!("&&{}", index + 1), value);
    }
    Some(text)
}

pub(crate) fn hudelem_pulse_sound(
    elem: &HudElem,
    text: &str,
    cg_time: i32,
    latch: &mut HudElemSoundLatch,
) -> Option<&'static str> {
    if elem.fx_birth_time == 0 {
        return None;
    }
    let birth_time = elem.fx_birth_time.min(cg_time);
    let last_played_time = latch.slot(elem.sound_id)?;
    hud_iw4::play_text_fx_pulse_sounds(
        cg_time,
        seh_print_strlen(text),
        birth_time,
        elem.fx_letter_time,
        elem.fx_decay_start_time,
        last_played_time,
    )
    .map(TextPulseSound::alias)
}

pub(crate) fn hudelem_text_fx(elem: &HudElem, cg_time: i32) -> Option<TextRunFx> {
    if elem.fx_birth_time == 0 {
        return None;
    }
    Some(TextRunFx {
        decode: elem.flags & elem_flags::COD7_DECODE != 0,
        scene_time: cg_time,
        fx: TextPulseFx {
            birth_time: elem.fx_birth_time.min(cg_time),
            letter_time: elem.fx_letter_time,
            decay_start_time: elem.fx_decay_start_time,
            decay_duration: elem.fx_decay_duration,
        },
    })
}

/// Letters typing in from `birth_time`, then decaying.
pub(crate) fn pulse_text_fx(
    scene_time: i32,
    birth_time: i32,
    letter_time: i32,
    decay_start_time: i32,
    decay_duration: i32,
) -> TextRunFx {
    TextRunFx {
        decode: false,
        scene_time,
        fx: TextPulseFx {
            birth_time,
            letter_time,
            decay_start_time,
            decay_duration,
        },
    }
}

/// Black Ops' `setCOD7DecodeFX` at one frame (its text draw's decode
/// setup): the letters drawn, how many at the end still show scrambled
/// digits and their seed, or the decay's elapsed time and length.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DecodeVars {
    pub max_length: i32,
    pub scrambled: i32,
    pub seed: i32,
    pub decay: Option<(i32, i32)>,
}

/// `None` once the decay is over: nothing draws.
pub(crate) fn decode_fx_vars(text: &str, max_length: i32, fx: TextRunFx) -> Option<DecodeVars> {
    let len = seh_print_strlen(text).min(max_length);
    let elapsed = fx.scene_time - fx.fx.birth_time;
    let decay_end = fx.fx.decay_start_time.saturating_add(fx.fx.decay_duration);
    if elapsed > decay_end {
        return None;
    }
    if elapsed > len.saturating_mul(fx.fx.letter_time) && elapsed > fx.fx.decay_start_time {
        return Some(DecodeVars {
            max_length,
            scrambled: 0,
            seed: 0,
            decay: Some((elapsed - fx.fx.decay_start_time, fx.fx.decay_duration)),
        });
    }
    // The digits change every 50 ms.
    let mut seed = fx.scene_time / 50;
    rand_with_seed(&mut seed);
    rand_with_seed(&mut seed);
    let typed = if fx.fx.letter_time > 0 {
        elapsed / fx.fx.letter_time
    } else {
        len
    };
    Some(DecodeVars {
        max_length: len,
        scrambled: len - typed,
        seed,
        decay: None,
    })
}

/// A letter not decoded yet: one of Black Ops' scramble digits.
pub(crate) fn decode_digit(seed: &mut i32) -> u32 {
    const DIGITS: &[u8; 9] = b"023456789";
    let mut pick = rand_with_seed(seed);
    u32::from(DIGITS[rand_with_seed(&mut pick).rem_euclid(9) as usize])
}

/// The decay's letter alpha: the run's alpha falling linearly to nothing
/// over the decay, none under one 30 Hz tick.
pub(crate) fn decode_fade(alpha: u8, elapsed: i32, duration: i32) -> u8 {
    if ((duration as f32 * 0.001 * 30.0) as i32) < 1 {
        return 0;
    }
    let left = if elapsed <= duration {
        (1.0 - elapsed as f32 / duration as f32) * (f32::from(alpha) / 255.0)
    } else {
        1.0
    };
    (left * 255.0).round().clamp(0.0, 255.0) as u8
}
