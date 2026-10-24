//! Black Ops' camera shake: each local client keeps four earthquakes, and
//! the strongest one in range shakes all three view angles with a sine on
//! each, after every view mode's angles and before the view's axis.

use game_api::shake::{Earthquake, ShakeRules, ShakeSlot, ShakeState};
use game_api::unknown;

pub(crate) const RULES: ShakeRules = ShakeRules {
    start,
    angles,
    gaps: &[
        unknown!(
            "t5.view.shake_entity_term",
            "the camera shake holds the entity pass's own shake strength at 0: Black Ops clears it before its entities run each frame and takes it in place of the earthquakes' when it is stronger",
            "what in the client's entity pass sets that strength (executable)"
        ),
        unknown!(
            "t5.view.shake_random",
            "the shake's phase draws from its own copy of the C runtime's rand: Black Ops draws it from the one every client system shares",
            "nothing a player can see; the shared draw order (executable)"
        ),
    ],
};

/// Earthquakes a local client keeps at once.
const SLOTS: usize = 4;
/// Client milliseconds to the shake's time: 1/600.
const TIME_SCALE: f32 = f32::from_bits(0x3ada_740e);
/// Each angle's wave (radians per shake time unit: 8π, 15π, 12π) and size
/// in degrees.
const WAVES: [(f32, f32); 3] = [
    (f32::from_bits(0x41c9_0fdb), 18.0),
    (f32::from_bits(0x423c_7edd), 16.0),
    (f32::from_bits(0x4216_cbe4), 10.0),
];

fn slots(state: &mut ShakeState) -> &mut [ShakeSlot] {
    if state.slots.len() != SLOTS {
        state.slots = vec![ShakeSlot::default(); SLOTS];
        state.random = 1;
    }
    &mut state.slots
}

/// An earthquake starts when the client receives it, in a free slot; with
/// none free it takes the place of the weakest one out of its range, and
/// with none out of range it is dropped.
fn start(state: &mut ShakeState, mut quake: Earthquake, now_ms: i32) {
    let slots = slots(state);
    let free = slots.iter().position(|slot| {
        now_ms < slot.quake.start_ms
            || slot.quake.start_ms as f32 + slot.quake.duration_ms as f32 <= now_ms as f32
    });
    let slot = free.or_else(|| {
        let mut weakest = None;
        let mut strength = 0.0;
        for (index, slot) in slots.iter().enumerate() {
            if slot.strength < strength {
                weakest = Some(index);
                strength = slot.strength;
            }
        }
        weakest
    });
    let Some(slot) = slot else {
        return;
    };
    quake.start_ms = now_ms;
    slots[slot] = ShakeSlot {
        quake,
        strength: 0.0,
        fade: 0.0,
    };
}

fn angles(state: &mut ShakeState, eye: [f32; 3], now_ms: i32) -> [f32; 3] {
    let time = now_ms as f32 * TIME_SCALE;
    let mut best = 0.0f32;
    let mut best_fade = 0.0f32;
    for slot in slots(state) {
        let quake = slot.quake;
        let elapsed = now_ms.wrapping_sub(quake.start_ms);
        let duration = quake.duration_ms as f32;
        if elapsed < 0 || elapsed as f32 >= duration {
            continue;
        }
        let [dx, dy, dz] = std::array::from_fn(|i| eye[i] - quake.origin[i]);
        let distance = (dz * dz + dy * dy + dx * dx).sqrt();
        let reach = 1.0 - distance / quake.radius;
        let fade = (1.0 - elapsed as f32 / duration) * quake.scale;
        if fade <= 0.0 {
            continue;
        }
        let strength = if reach < 0.0 {
            reach / fade
        } else {
            fade * reach
        };
        slot.strength = strength;
        slot.fade = fade;
        if strength > best {
            best = strength;
            best_fade = fade;
        }
    }
    if best <= 0.0 {
        state.phase = crandom(&mut state.random) * std::f32::consts::PI;
        return [0.0; 3];
    }
    let amplitude = if best > 1.0 { 1.0 } else { best };
    let phase = state.phase;
    WAVES.map(|(rate, size)| {
        let wave = f64::from(time * rate + phase).sin() as f32;
        wave * best_fade * amplitude * size
    })
}

/// The C runtime's rand, from -1 to just under 1.
fn crandom(holdrand: &mut u32) -> f32 {
    *holdrand = holdrand.wrapping_mul(214_013).wrapping_add(2_531_011);
    let rand = ((*holdrand >> 16) & 0x7fff) as f32;
    rand * (1.0 / 32768.0) * 2.0 - 1.0
}
