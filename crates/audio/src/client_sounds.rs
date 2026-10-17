//! The local client's own sounds: the sounds its client scripts play by
//! playback id (`playSound` on its client VM), which they stop and fade by
//! that id, and whose status (playing, how far, how long) goes back to the
//! client VM each frame, as a client game's sound thread publishes its
//! playback status for its scripts to read.

use std::collections::BTreeMap;

use bevy::prelude::*;
use frame::ScopeApp;

use crate::aliases::namespace_alias;
use crate::cue::CueHandle;
use crate::cue_execution::CueTrigger;
use crate::render_core::SAMPLE_RATE;
use crate::{SoundBank, SoundClass};

struct Voice {
    handle: CueHandle,
    /// The volume it was last sent toward.
    volume: f32,
    /// How fast its volume moves, per second; zero moves it at once.
    rate: f32,
}

#[derive(Resource, Default)]
pub(crate) struct ClientSounds {
    voices: BTreeMap<i32, Voice>,
    /// Sounds that could not start, reported once as not playing.
    refused: Vec<i32>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_client_sounds(
    epoch: Res<frame::ScopeEpoch<frame::MatchScope>>,
    runtime: Res<crate::AudioRuntime>,
    mut events: MessageReader<net::SvcScriptAudio>,
    bank: Option<Res<SoundBank>>,
    family: Option<Res<crate::ambient::SoundBankNamespace>>,
    local: Res<net::LocalPresentClient>,
    mut sounds: ResMut<ClientSounds>,
    mut feedback: ResMut<crate::clip_store::CueFeedback>,
    authority: Option<ResMut<net::AuthorityWorld>>,
) {
    let frame = runtime.audio_frame();
    for event in events.read() {
        match &event.0 {
            sim::ScriptAudioCommand::ClientSound {
                client,
                id,
                alias,
                origin,
                volume,
                fade_in_ms,
            } if *client == local.0 => {
                let (Some(bank), Some(family)) = (bank.as_deref(), family.as_deref()) else {
                    sounds.refused.push(*id);
                    continue;
                };
                let (namespace, name) = namespace_alias(alias, family.namespace);
                let handle = runtime.trigger_cue(CueTrigger {
                    event: None,
                    bank: bank.0.clone(),
                    namespace,
                    alias: name.into(),
                    bound: None,
                    origin_inches: *origin,
                    emitter: None,
                    follow_emitter: false,
                    class: SoundClass::World,
                    epoch: epoch.0,
                    pitch_scale: 1.0,
                    volume_scale: 1.0,
                    fallbacks: Vec::new(),
                });
                if *fade_in_ms > 0 {
                    handle.fade_to(frame, 0.0, 0);
                    let frames = i64::from(*fade_in_ms) * i64::from(SAMPLE_RATE) / 1000;
                    handle.fade_to(frame, *volume, frames as u64);
                } else if *volume != 1.0 {
                    handle.fade_to(frame, *volume, 0);
                }
                feedback.push(CueHandle(handle.0.clone()));
                if let Some(old) = sounds.voices.insert(
                    *id,
                    Voice {
                        handle,
                        volume: *volume,
                        rate: 0.0,
                    },
                ) {
                    old.handle.release(frame, 0);
                }
            }
            sim::ScriptAudioCommand::ClientSoundStop { client, id } if *client == local.0 => {
                if let Some(voice) = sounds.voices.get(id) {
                    voice.handle.release(frame, 0);
                }
            }
            sim::ScriptAudioCommand::ClientSoundVolume { client, id, volume }
                if *client == local.0 =>
            {
                if let Some(voice) = sounds.voices.get_mut(id) {
                    let frames = if voice.rate > 0.0 {
                        ((voice.volume - volume).abs() / voice.rate * SAMPLE_RATE as f32) as u64
                    } else {
                        0
                    };
                    voice.handle.fade_to(frame, *volume, frames);
                    voice.volume = *volume;
                }
            }
            sim::ScriptAudioCommand::ClientSoundVolumeRate { client, id, rate }
                if *client == local.0 =>
            {
                if let Some(voice) = sounds.voices.get_mut(id) {
                    voice.rate = *rate;
                }
            }
            _ => {}
        }
    }
    let mut statuses: Vec<sim::ClientSoundStatus> = sounds
        .refused
        .drain(..)
        .map(|id| sim::ClientSoundStatus {
            id,
            playing: false,
            playback_ms: 0,
            length_ms: 0,
        })
        .collect();
    sounds.voices.retain(|id, voice| {
        let playing = voice.handle.active();
        let (playback_ms, length_ms) = voice
            .handle
            .0
            .playback
            .get()
            .map_or((0, 0), |instance| instance.playback_ms());
        statuses.push(sim::ClientSoundStatus {
            id: *id,
            playing,
            playback_ms,
            length_ms,
        });
        playing
    });
    if let Some(mut authority) = authority {
        authority.0.report_client_sounds(statuses);
    }
}

pub(crate) fn register(app: &mut App) {
    app.scoped::<ClientSounds>(frame::MatchScope::Live)
        .add_systems(
            Update,
            update_client_sounds
                .in_set(frame::InMatch)
                .in_set(net::ClientSet::Effects),
        );
}
