//! The game's mix snapshots (`setGroupSnapshot`, `setAmbientSnapshot`,
//! `setHealthSnapshot`, `setLevelFadeSnapshot`): each slot holds a snapshot
//! at a weight; every frame each slot's volume for a snapshot group eases
//! toward `1 - (1 - volume) x weight` at one over the snapshot's fade-in
//! time per second (at once when it has none), and a sound plays at the
//! lowest of the slots' volumes for its alias's snapshot group.

use std::collections::HashMap;
use std::sync::Arc;

use asset_core::AssetNamespace;
use bevy::prelude::*;
use frame::ScopeApp;

use crate::media::LiveGain;

/// The slots scripts set; slot 0 (the snapshots sounds carry while they
/// play) is not run.
const SLOTS: usize = 11;
const GROUPS: usize = 64;
/// A fade-in shorter than this takes effect at once.
const FADE_MIN: f32 = 0.00001;

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
    let mut volume = [1.0f32; GROUPS];
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
    let now = runtime.audio_frame();
    let frames = (f64::from(dt) * f64::from(crate::render_core::SAMPLE_RATE)) as u64;
    for (group, volume) in volume.iter().enumerate().take(groups) {
        if let Some(gain) = mix.gains.get(&(namespace, group as u8)) {
            gain.fade(now, *volume, frames);
        }
    }
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
