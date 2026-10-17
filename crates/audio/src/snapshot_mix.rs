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
        }
    }
}

impl SnapshotMix {
    pub(crate) fn bindings(&self) -> Arc<HashMap<(AssetNamespace, u8), LiveGain>> {
        self.gains.clone()
    }

    pub(crate) fn reset_epoch(&mut self, epoch: u64) {
        if self.epoch != epoch {
            self.epoch = epoch;
            self.slots = [Slot::default(); SLOTS];
            self.settled = false;
            self.pending.clear();
            for gain in self.gains.values() {
                gain.set(1.0);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn update_snapshot_mix(
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
        if let sim::ScriptAudioCommand::ClientSnapshot {
            client,
            slot,
            name,
            value,
        } = &event.0
            && Some(*client) == local
        {
            mix.pending.push_back((*slot, name.clone(), *value));
        }
    }
    let (Some(bank), Some(namespace)) = (bank, namespace) else {
        return;
    };
    let namespace = namespace.namespace;
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
        diag::info!(
            Audio,
            "PROBE snapshot slot={slot} name={name} value={value} found={} count={} groups={:?}",
            hash.is_some(),
            snapshots.list.len(),
            snapshots.groups
        );
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
        for group in 0..groups {
            let target = 1.0 - (1.0 - snapshot.volumes[group]) * slot.weight;
            let current = &mut slot.current[group];
            if settle || !(0.0 < rate) {
                *current = target;
            } else {
                let diff = target - *current;
                let step = (rate * dt).min(diff.abs());
                *current += step.copysign(diff);
            }
            if *current < volume[group] {
                volume[group] = *current;
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

pub(crate) fn register(app: &mut App) {
    app.scoped::<SnapshotMix>(frame::MatchScope::Live)
        .add_systems(
            Update,
            update_snapshot_mix
                .in_set(frame::InMatch)
                .in_set(net::ClientSet::Effects),
        );
}
