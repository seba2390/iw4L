//! The game's mix snapshots (`setGroupSnapshot`, `setAmbientSnapshot`,
//! `setHealthSnapshot`, `setLevelFadeSnapshot`): each slot holds a snapshot
//! at a weight; every frame each slot's volume for a snapshot group eases
//! toward `1 - (1 - volume) x weight` at one over the snapshot's fade-in
//! time per second (at once when it has none), and a sound plays at the
//! lowest of the slots' volumes for its alias's snapshot group.
//!
//! Slot 0 is the snapshots sounds carry while they play: each playing
//! sound's snapshot pulls each group toward its volume by a weight that its
//! fade-in and fade-out curves (and, for a sound that applies it by
//! distance, the default curve over its distance) give, the lowest per
//! group kept; the slot eases toward that at thirty per second.

use std::collections::HashMap;
use std::sync::Arc;

use asset_core::AssetNamespace;
use bevy::prelude::*;
use frame::ScopeApp;

use crate::media::LiveGain;

/// The slots scripts set; slot 0 holds the snapshots sounds carry while
/// they play.
const SLOTS: usize = 11;
const GROUPS: usize = 64;
/// A fade-in shorter than this takes effect at once.
const FADE_MIN: f32 = 0.00001;
/// How fast slot 0 eases toward its playing sounds' volumes, per second.
const VOICE_RATE: f32 = 30.0;

#[derive(Clone, Copy)]
struct Slot {
    /// The snapshot's name hash; `None` is the default snapshot.
    hash: Option<u32>,
    weight: f32,
    current: [f32; GROUPS],
}

impl Default for Slot {
    fn default() -> Self {
        Self {
            hash: None,
            weight: 1.0,
            current: [1.0; GROUPS],
        }
    }
}

#[derive(Resource)]
pub(crate) struct SnapshotMix {
    epoch: u64,
    slots: [Slot; SLOTS],
    /// The slots are at the default snapshot's volumes: the first update
    /// starts them there instead of easing in.
    settled: bool,
    gains: Arc<HashMap<(AssetNamespace, u8), LiveGain>>,
    pending: std::collections::VecDeque<(u8, String, f32)>,
    /// Each sound context's current value (name hashes), shared with the
    /// cue resolver that picks alias variants by them.
    contexts: Arc<std::sync::RwLock<HashMap<u32, u32>>>,
    /// The contexts have their starting values for this match.
    contexts_ready: bool,
    pending_contexts: std::collections::VecDeque<(String, String)>,
    /// A game whose banks carry sound ducks (Black Ops 2) runs them instead
    /// of the slots.
    ducks: crate::duck_mix::DuckMix,
}

impl Default for SnapshotMix {
    fn default() -> Self {
        Self {
            epoch: 0,
            slots: [Slot::default(); SLOTS],
            settled: false,
            gains: Arc::new(
                AssetNamespace::ALL
                    .into_iter()
                    .flat_map(|namespace| {
                        (0..GROUPS as u8)
                            .map(move |group| ((namespace, group), LiveGain::default()))
                    })
                    .collect(),
            ),
            pending: Default::default(),
            contexts: Default::default(),
            contexts_ready: false,
            pending_contexts: Default::default(),
            ducks: Default::default(),
        }
    }
}

impl SnapshotMix {
    pub(crate) fn bindings(&self) -> Arc<HashMap<(AssetNamespace, u8), LiveGain>> {
        self.gains.clone()
    }

    pub(crate) fn contexts(&self) -> Arc<std::sync::RwLock<HashMap<u32, u32>>> {
        self.contexts.clone()
    }

    /// Puts a snapshot in a slot from the client's own code, as a script's
    /// call would.
    pub(crate) fn set_slot(&mut self, slot: u8, name: String, value: f32) {
        self.pending.push_back((slot, name, value));
    }

    pub(crate) fn reset_epoch(&mut self, epoch: u64) {
        if self.epoch != epoch {
            self.epoch = epoch;
            self.slots = [Slot::default(); SLOTS];
            self.settled = false;
            self.pending.clear();
            self.pending_contexts.clear();
            self.contexts_ready = false;
            self.ducks = Default::default();
            if let Ok(mut contexts) = self.contexts.write() {
                contexts.clear();
            }
            for gain in self.gains.values() {
                gain.set(1.0);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_snapshot_mix(
    mut mix: ResMut<SnapshotMix>,
    mut events: MessageReader<net::SvcScriptAudio>,
    bank: Option<Res<crate::SoundBank>>,
    namespace: Option<Res<crate::ambient::SoundBankNamespace>>,
    local: Option<Res<net::LocalPresentClient>>,
    epoch: Res<frame::ScopeEpoch<frame::MatchScope>>,
    runtime: Res<crate::AudioRuntime>,
    time: Res<Time>,
) {
    mix.reset_epoch(epoch.0);
    let local = local.map(|local| local.0);
    for event in events.read() {
        match &event.0 {
            sim::ScriptAudioCommand::ClientSnapshot {
                client,
                slot,
                name,
                value,
            } if Some(*client) == local => {
                mix.pending.push_back((*slot, name.clone(), *value));
            }
            sim::ScriptAudioCommand::ClientSoundContext {
                client,
                context,
                value,
            } if Some(*client) == local => {
                mix.pending_contexts
                    .push_back((context.clone(), value.clone()));
            }
            _ => {}
        }
    }
    let (Some(bank), Some(namespace)) = (bank, namespace) else {
        return;
    };
    let namespace = namespace.namespace;
    update_contexts(&mut mix, bank.0.sound_contexts(namespace));
    if let Some(ducks) = bank.0.sound_ducks(namespace) {
        while let Some((slot, name, value)) = mix.pending.pop_front() {
            let name = if name.is_empty() { "default" } else { &name };
            let hash = asset_transport::sab::snd_hash_name(name);
            mix.ducks.set(ducks, usize::from(slot), hash, value);
        }
        let dt = time.delta_secs();
        let gains = mix.ducks.update(ducks, dt);
        let now = runtime.audio_frame();
        let frames = (f64::from(dt) * f64::from(crate::render_core::SAMPLE_RATE)) as u64;
        for (group, volume) in gains.iter().enumerate() {
            if let Some(gain) = mix.gains.get(&(namespace, group as u8)) {
                gain.fade(now, *volume, frames);
            }
        }
        return;
    }
    let Some(snapshots) = bank.0.mix_snapshots(namespace) else {
        return;
    };
    let default_hash = asset_transport::sab::snd_hash_name("default");
    let Some(default) = snapshots.find(default_hash) else {
        return;
    };
    while let Some((slot, name, value)) = mix.pending.pop_front() {
        let name = if name.is_empty() { "default" } else { &name };
        let hash = asset_transport::sab::snd_hash_name(name);
        let hash = if snapshots.find(hash).is_some() {
            Some(hash)
        } else {
            diag::warn!(Audio, "audio: could not find group snapshot {name}");
            None
        };
        let slot = &mut mix.slots[usize::from(slot)];
        slot.hash = hash;
        slot.weight = value;
    }
    let groups = snapshots.groups.unwrap_or(GROUPS).min(GROUPS);
    let dt = time.delta_secs();
    let settle = !mix.settled;
    mix.settled = true;
    let now = runtime.audio_frame();
    let listener = runtime.listener();
    let mut volume = [1.0f32; GROUPS];
    let voices = voice_volumes(
        &runtime.snapshot_voices(),
        snapshots,
        |hash| bank.0.curve_knots_by_hash(namespace, hash),
        listener.map(|listener| listener.origin_inches),
        now,
    );
    for ((current, lowest), target) in mix.slots[0]
        .current
        .iter_mut()
        .zip(volume.iter_mut())
        .zip(voices)
        .take(groups)
    {
        if settle {
            *current = target;
        } else {
            let diff = target - *current;
            *current += (VOICE_RATE * dt).min(diff.abs()).copysign(diff);
        }
        *lowest = current.min(*lowest);
    }
    for slot in mix.slots.iter_mut().skip(1) {
        let snapshot = slot
            .hash
            .and_then(|hash| snapshots.find(hash))
            .unwrap_or(default);
        let rate = if snapshot.fade_in - FADE_MIN < 0.0 {
            0.0
        } else {
            1.0 / snapshot.fade_in
        };
        for ((current, lowest), authored) in slot
            .current
            .iter_mut()
            .zip(volume.iter_mut())
            .zip(snapshot.volumes)
            .take(groups)
        {
            let target = 1.0 - (1.0 - authored) * slot.weight;
            if settle || !(0.0 < rate) {
                *current = target;
            } else {
                let diff = target - *current;
                let step = (rate * dt).min(diff.abs());
                *current += step.copysign(diff);
            }
            if *current < *lowest {
                *lowest = *current;
            }
        }
    }
    let frames = (f64::from(dt) * f64::from(crate::render_core::SAMPLE_RATE)) as u64;
    for (group, volume) in volume.iter().enumerate().take(groups) {
        if let Some(gain) = mix.gains.get(&(namespace, group as u8)) {
            gain.fade(now, *volume, frames);
        }
    }
}

/// Each group's lowest volume over the playing sounds' snapshots.
fn voice_volumes<'a>(
    voices: &[crate::runtime::SnapshotVoice],
    snapshots: &asset_audio::MixSnapshots,
    curve: impl Fn(u32) -> Option<&'a [(f32, f32)]>,
    listener: Option<[f32; 3]>,
    now: u64,
) -> [f32; GROUPS] {
    let default_curve = asset_transport::sab::snd_hash_name("default");
    let eval = |hash: u32, x: f32| curve(hash).map_or(1.0, |knots| curve_value(knots, x));
    let mut volume = [1.0f32; GROUPS];
    for voice in voices {
        let Some(snapshot) = snapshots.find(voice.policy.hash) else {
            continue;
        };
        let elapsed =
            (now as f64 - voice.start_frame as f64) / f64::from(crate::render_core::SAMPLE_RATE);
        let (elapsed, length) = (elapsed as f32, voice.length_s);
        if elapsed < 0.0 || !(voice.looping || snapshot.fade_in + snapshot.fade_out <= length) {
            continue;
        }
        let mut weight = if elapsed < snapshot.fade_in {
            eval(snapshot.fade_in_curve, elapsed / snapshot.fade_in)
        } else if !voice.looping && 0.0 < snapshot.fade_out && length - snapshot.fade_out < elapsed
        {
            eval(
                snapshot.fade_out_curve,
                1.0 - (length - elapsed) / snapshot.fade_out,
            )
        } else {
            1.0
        };
        if 1.0 <= snapshot.distance && voice.policy.by_distance {
            let distance = match (listener, voice.origin_inches) {
                (Some(listener), Some(origin)) => crate::distance_inches(listener, origin),
                _ => 0.0,
            };
            weight = weight.min(eval(default_curve, distance / snapshot.distance));
        }
        for (lowest, authored) in volume.iter_mut().zip(snapshot.volumes) {
            *lowest = lowest.min(authored * weight + (1.0 - weight));
        }
    }
    volume
}

/// A sound curve at `x` (clamped to one; at or below zero, its value at
/// zero): straight lines between its knots, flat past its ends.
fn curve_value(knots: &[(f32, f32)], x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    let Some(&(first_x, first_y)) = knots.first() else {
        return 1.0;
    };
    if x <= first_x {
        return first_y;
    }
    for pair in knots.windows(2) {
        let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
        if x < x1 {
            let t = if x1 - x0 > 0.0 {
                (x - x0) / (x1 - x0)
            } else {
                1.0
            };
            return y0 * (1.0 - t) + y1 * t;
        }
    }
    knots.last().map_or(1.0, |knot| knot.1)
}

/// The sound system starts the match's contexts at their starting values;
/// a script's context must be one the sound driver declares, and its value
/// one the context may take (or empty, unsetting it).
fn update_contexts(mix: &mut SnapshotMix, defs: Option<&asset_audio::SoundContexts>) {
    let Some(defs) = defs else {
        return;
    };
    let Ok(mut contexts) = mix.contexts.write() else {
        return;
    };
    if !mix.contexts_ready {
        mix.contexts_ready = true;
        for &(ty, value) in &defs.initial {
            if defs.defs.iter().any(|def| def.ty == ty) {
                contexts.insert(ty, value);
            }
        }
    }
    let hash = asset_transport::sab::snd_hash_name;
    while let Some((context, value)) = mix.pending_contexts.pop_front() {
        let ty = hash(&context);
        let Some(def) = defs.defs.iter().find(|def| def.ty == ty) else {
            diag::warn!(Audio, "audio: unknown sound context {context}");
            continue;
        };
        let value_hash = hash(&value);
        if value_hash != 0 && !def.values.contains(&value_hash) {
            diag::warn!(Audio, "audio: unknown sound context {value}");
            continue;
        }
        contexts.insert(ty, value_hash);
    }
}

pub(crate) fn register(app: &mut App) {
    app.scoped::<SnapshotMix>(frame::MatchScope::Live)
        .add_systems(
            Update,
            update_snapshot_mix
                .in_set(frame::InMatch)
                .in_set(net::ClientSet::Effects),
        );
}
