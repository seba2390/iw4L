//! Black Ops 2's sound ducks (the snapshots its client scripts set with
//! `setGroupSnapshot` and its kin), as its sound thread runs them: a duck
//! set in a slot comes in over its fade-in after its start delay and goes
//! over its fade-out once replaced or once its length has passed; each duck
//! group's volume is the lowest any duck in force asks of it, eased toward
//! at ten a second (a tenth at most per update), and a voice plays at the
//! cube of its duck group's.

use asset_audio::SoundDuck;

/// The engine's epsilon for durations, distances and values.
const EPSILON: f32 = 1.528_79e-5;
/// Ducks in force at once, and slots scripts set them in.
const ENTRIES: usize = 128;
const SLOTS: usize = 32;
/// How fast a group's volume eases, per second, and its most per update.
const EASE_RATE: f32 = 10.0;
const EASE_MAX: f32 = 0.1;
pub(crate) const GROUPS: usize = 64;

#[derive(Clone, Copy)]
struct Entry {
    slot: Option<usize>,
    start_ms: i64,
    /// When it began going (0: in force).
    stop_ms: i64,
    hash: u32,
    duck: usize,
    value: f32,
    amount: f32,
}

pub(crate) struct DuckMix {
    entries: Vec<Option<Entry>>,
    slots: [Option<usize>; SLOTS],
    current: [f32; GROUPS],
    now_ms: i64,
    /// The gains last traced (`IW4L_SOUND_TRACE`).
    traced: [f32; GROUPS],
}

fn tracing() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("IW4L_SOUND_TRACE").is_ok_and(|v| v != "0" && !v.is_empty()))
}

impl Default for DuckMix {
    fn default() -> Self {
        Self {
            entries: vec![None; ENTRIES],
            slots: [None; SLOTS],
            current: [1.0; GROUPS],
            now_ms: 0,
            traced: [1.0; GROUPS],
        }
    }
}

impl DuckMix {
    /// A duck put in a slot at a value (the engine's sound command 0x14).
    pub(crate) fn set(&mut self, ducks: &[SoundDuck], slot: usize, hash: u32, value: f32) {
        let default = asset_transport::sab::snd_hash_name("default");
        let Some(index) = ducks.iter().position(|duck| duck.hash == hash) else {
            return;
        };
        let duck = &ducks[index];
        if EPSILON < duck.distance {
            diag::warn!(
                Audio,
                "audio: sound duck {hash:08x} cannot have a distance if it is not associated with an alias"
            );
            return;
        }
        let Some(&held) = self.slots.get(slot) else {
            return;
        };
        if held.is_none() && hash == default {
            return;
        }
        if let Some(entry) = held.and_then(|e| self.entries[e].as_mut()) {
            if entry.hash == hash && EPSILON < value {
                entry.value = value;
                return;
            }
            if duck.length < EPSILON {
                entry.stop_ms = self.now_ms;
                self.slots[slot] = None;
            }
        }
        if hash == default || value < EPSILON {
            return;
        }
        let Some(free) = self.entries.iter().position(Option::is_none) else {
            diag::warn!(Audio, "audio: no free sound ducks");
            return;
        };
        let timed = EPSILON <= duck.length;
        if tracing() {
            diag::info!(
                Audio,
                "sound trace: duck {hash:08x} slot={slot} value={value} fade_in={} fade_out={} length={}",
                duck.fade_in,
                duck.fade_out,
                duck.length
            );
        }
        self.entries[free] = Some(Entry {
            slot: (!timed).then_some(slot),
            start_ms: self.now_ms,
            stop_ms: if timed {
                self.now_ms + (duck.length * 1000.0) as i64
            } else {
                0
            },
            hash,
            duck: index,
            value,
            amount: 0.0,
        });
        if !timed {
            self.slots[slot] = Some(free);
        }
    }

    /// One update `dt` seconds on: each duck's weight, then the groups'
    /// volumes eased toward the lowest the ducks ask; returns each group's
    /// gain.
    pub(crate) fn update(&mut self, ducks: &[SoundDuck], dt: f32) -> [f32; GROUPS] {
        self.now_ms += (dt * 1000.0) as i64;
        let now = self.now_ms;
        for index in 0..self.entries.len() {
            let Some(entry) = self.entries[index] else {
                continue;
            };
            let duck = &ducks[entry.duck];
            let t = (now - entry.start_ms) as f32 * 0.001 - duck.start_delay;
            let gone = (now - entry.stop_ms) as f32 * 0.001;
            let weight = if entry.stop_ms == 0 || gone <= 0.0 {
                if t < 0.0 {
                    0.0
                } else if t < duck.fade_in {
                    t / duck.fade_in
                } else {
                    1.0
                }
            } else if t < 0.0 || duck.fade_out < gone {
                if entry
                    .slot
                    .is_some_and(|slot| self.slots[slot] == Some(index))
                {
                    self.slots[entry.slot.unwrap_or(0)] = None;
                }
                self.entries[index] = None;
                continue;
            } else if duck.fade_in <= t {
                1.0 - gone / duck.fade_out
            } else {
                (1.0 - ((1.0 - t / duck.fade_in) * duck.fade_out + gone) / duck.fade_out).max(0.0)
            };
            if let Some(entry) = self.entries[index].as_mut() {
                entry.amount = entry.value * weight.min(1.0);
            }
        }
        let groups = ducks
            .iter()
            .map(|duck| duck.attenuation.len())
            .max()
            .unwrap_or(0)
            .min(GROUPS);
        let mut target = [1.0f32; GROUPS];
        for entry in self.entries.iter().flatten() {
            let duck = &ducks[entry.duck];
            for (lowest, attenuation) in target.iter_mut().zip(&duck.attenuation).take(groups) {
                *lowest = lowest.min(1.0 - entry.amount * (1.0 - attenuation));
            }
        }
        let step = (dt * EASE_RATE).min(EASE_MAX);
        let mut gains = [1.0f32; GROUPS];
        for ((current, target), gain) in self
            .current
            .iter_mut()
            .zip(target)
            .zip(gains.iter_mut())
            .take(groups)
        {
            *current += (target - *current).clamp(-step, step);
            *gain = current.powi(3);
        }
        if tracing()
            && gains
                .iter()
                .zip(&self.traced)
                .any(|(gain, traced)| (gain - traced).abs() > 0.05)
        {
            self.traced = gains;
            let ducked: Vec<String> = gains
                .iter()
                .enumerate()
                .take(groups)
                .filter(|(_, gain)| **gain < 0.999)
                .map(|(group, gain)| format!("{group}:{gain:.2}"))
                .collect();
            diag::info!(Audio, "sound trace: duck groups {}", ducked.join(" "));
        }
        gains
    }
}
