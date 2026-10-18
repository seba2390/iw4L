use asset_core::AssetNamespace;
use bevy::prelude::*;
use frame::{AppScreen, LifeEnded};
use hud_iw4::shellshock_remaining_ms;
use net::{FrameClock, LocalPresentClient, PresentedSnapshot};

use crate::aliases::namespace_alias;
use crate::cue::CueHandle;
use crate::cue_execution::CueTrigger;
use crate::render_core::SAMPLE_RATE;
use crate::sources::{DesiredSource, SourceCueRequest, SourceKey};
use crate::{PlayAlias, SND_ENT_LOCAL, SoundClass, playback::SoundBank};

#[derive(Resource, Default)]
pub(crate) struct ShellshockSources {
    pub source: Option<DesiredSource>,
    context: Option<(u64, u32, u32)>,
    next_version: u64,
    was_active: bool,
}

pub(crate) fn update_shellshock_tinnitus(
    mut died: MessageReader<LifeEnded>,
    screen: Option<Res<AppScreen>>,
    cg_clock: Option<Res<FrameClock>>,
    presented: Option<Res<PresentedSnapshot>>,
    local: Option<Res<LocalPresentClient>>,
    bank: Option<Res<SoundBank>>,
    runtime: Res<crate::AudioRuntime>,
    mut sources: ResMut<ShellshockSources>,
    mut play: MessageWriter<crate::AliasCommand>,
    epoch: Res<frame::ScopeEpoch<frame::MatchScope>>,
) {
    let local_id = local.as_ref().map(|l| l.0.0);
    let life = local.as_ref().and_then(|local| {
        presented
            .as_ref()?
            .snapshot()?
            .meta
            .for_client(local.0)
            .map(|meta| meta.life_sequence.0)
    });
    let died = died
        .read()
        .any(|ev| local_id == Some(ev.client) && life == Some(ev.life));
    if !screen.is_some_and(|s| matches!(*s, AppScreen::InGame)) {
        sources.source = None;
        sources.context = None;
        sources.was_active = false;
        return;
    }
    let (Some(cg_clock), Some(presented), Some(local)) = (cg_clock, presented, local) else {
        sources.source = None;
        sources.context = None;
        sources.was_active = false;
        return;
    };
    let context = (epoch.0, local.0.0, life.unwrap_or(0));
    if sources.context != Some(context) {
        sources.source = None;
        sources.was_active = false;
        sources.context = Some(context);
    }
    let alive = presented.alive_player(local.0).is_some();
    let remaining = presented
        .player(local.0)
        .map(|ps| {
            shellshock_remaining_ms(cg_clock.time(), ps.shellshock_time, ps.shellshock_duration)
        })
        .unwrap_or(0);
    let Some(parms) = presented
        .shellshock(local.0)
        .map(|shock| shock.sound.clone())
    else {
        sources.source = None;
        sources.was_active = false;
        return;
    };
    let want = remaining > 0 && parms.affect && alive && !died;
    if want {
        let same = sources
            .source
            .as_ref()
            .is_some_and(|source| source.cue.alias == parms.loop_alias);
        if !same {
            sources.source = None;
            if let Some(bank) = bank
                && let Some(cue) = runtime.source_cue(SourceCueRequest {
                    bank: bank.0.clone(),
                    namespace: AssetNamespace::Iw4,
                    alias: parms.loop_alias.clone(),
                    emitter: Some(local.0.0),
                    scope: crate::backend::AudioScope::Match,
                    epoch: epoch.0,
                    group: None,
                })
            {
                sources.next_version = sources
                    .next_version
                    .checked_add(1)
                    .expect("source version exhausted");
                sources.source = Some(DesiredSource {
                    key: SourceKey {
                        scope: crate::backend::AudioScope::Match,
                        epoch: epoch.0,
                        object: u64::from(local.0.0),
                        slot: 5,
                    },
                    version: sources.next_version,
                    cue,
                    origin_inches: None,
                    start_frame: runtime.audio_frame(),
                    gain: 1.0,
                    rate: 1.0,
                    audible: true,
                });
            }
        }
        sources.was_active = true;
        return;
    }
    if sources.was_active {
        sources.source = None;
        let alias = if died || !alive {
            &parms.abort_alias
        } else {
            &parms.end_alias
        };
        play.write(crate::AliasCommand::Play(PlayAlias {
            event: None,
            namespace: AssetNamespace::Iw4,
            alias: alias.to_owned(),
            fallback: None,
            origin_inches: None,
            snd_ent: Some(SND_ENT_LOCAL),
        }));
    }
    sources.was_active = false;
}

/// A shock file's own timed sound (`ShockParams::timed_sound`), run the way
/// the game's client shock update runs it every frame of a shock.
#[derive(Resource, Default)]
pub(crate) struct TimedShockSound {
    context: Option<(u64, u32)>,
    /// The loop was started and not yet ended (the game's own flag, which
    /// outlives a shock that is cleared before it ends).
    playing: bool,
    voice: Option<CueHandle>,
    /// The snapshot last put in the shellshock's snapshot slot.
    snapshot: Option<String>,
}

/// The game's snapshot slot for a shellshock.
const SHOCK_SNAPSHOT_SLOT: u8 = 3;

impl TimedShockSound {
    fn set_snapshot(&mut self, mix: &mut crate::snapshot_mix::SnapshotMix, name: &str) {
        if self.snapshot.as_deref() != Some(name) {
            self.snapshot = Some(name.to_owned());
            mix.set_slot(SHOCK_SNAPSHOT_SLOT, name.to_owned(), 1.0);
        }
    }

    /// The loop a shock left playing when it was cleared before its end:
    /// IW4L fades it out over the shock's fade-out time.
    fn stop_voice(&mut self, frame: u64, fade_out_ms: i32) {
        if let Some(voice) = self.voice.take() {
            voice.release(frame, ms_frames(fade_out_ms));
        }
    }
}

fn ms_frames(ms: i32) -> u64 {
    u64::from(ms.max(0).unsigned_abs()) * u64::from(SAMPLE_RATE) / 1000
}

/// The shock sound's loop and end sound play on the shocked player's own
/// entity, which the game hears as a local sound.
#[allow(clippy::too_many_arguments)]
pub(crate) fn update_shellshock_timed_sound(
    screen: Option<Res<AppScreen>>,
    cg_clock: Option<Res<FrameClock>>,
    presented: Option<Res<PresentedSnapshot>>,
    local: Option<Res<LocalPresentClient>>,
    bank: Option<Res<SoundBank>>,
    family: Option<Res<crate::ambient::SoundBankNamespace>>,
    runtime: Res<crate::AudioRuntime>,
    epoch: Res<frame::ScopeEpoch<frame::MatchScope>>,
    mut state: ResMut<TimedShockSound>,
    mut mix: ResMut<crate::snapshot_mix::SnapshotMix>,
) {
    let frame = runtime.audio_frame();
    let in_game = screen.is_some_and(|s| matches!(*s, AppScreen::InGame));
    let (true, Some(cg_clock), Some(presented), Some(local)) =
        (in_game, cg_clock, presented, local)
    else {
        state.stop_voice(frame, 0);
        *state = TimedShockSound::default();
        return;
    };
    let context = (epoch.0, local.0.0);
    if state.context != Some(context) {
        state.stop_voice(frame, 0);
        *state = TimedShockSound {
            context: Some(context),
            ..Default::default()
        };
    }
    let Some(ps) = presented.player(local.0) else {
        return;
    };
    let elapsed = cg_clock.time().wrapping_sub(ps.shellshock_time);
    let shock = presented.shellshock(local.0);
    let Some(shock) = shock.filter(|_| ps.shellshock_time != 0 && elapsed >= 0) else {
        // The shock is over or cleared: the slot goes back to the default
        // snapshot (the shock's reverb room is let go too).
        state.set_snapshot(&mut mix, "default");
        state.stop_voice(frame, 0);
        return;
    };
    // A shock without its own sound only lets the reverb room go.
    let Some(sound) = &shock.timed_sound else {
        return;
    };
    let snapshot = if sound.snapshot.is_empty() {
        "default"
    } else {
        &sound.snapshot
    };
    // How long until the end delay and the fade-out have both run out.
    let left = sound
        .mod_end_delay_ms
        .wrapping_sub(elapsed)
        .wrapping_add(sound.fade_out_ms)
        .wrapping_add(ps.shellshock_duration);
    let active = if elapsed < sound.fade_in_ms || sound.fade_out_ms < left {
        state.set_snapshot(&mut mix, snapshot);
        true
    } else {
        if left >= 0 && left < sound.fade_out_ms {
            state.set_snapshot(&mut mix, "default");
        }
        false
    };
    let (Some(bank), Some(family)) = (bank, family) else {
        return;
    };
    let play = |alias: &str, fade_in_ms: i32| {
        let (namespace, name) = namespace_alias(alias, family.namespace);
        runtime.trigger_faded_cue(
            CueTrigger {
                event: None,
                bank: bank.0.clone(),
                namespace,
                alias: name.into(),
                bound: None,
                origin_inches: None,
                emitter: None,
                follow_emitter: false,
                class: SoundClass::World,
                epoch: epoch.0,
                pitch_scale: 1.0,
                volume_scale: 1.0,
                fallbacks: Vec::new(),
            },
            ms_frames(fade_in_ms),
        )
    };
    if state.playing {
        if active {
            // Played again with no fade: a loop still playing keeps on, one
            // stopped when its shock was cleared starts again at once.
            if state.voice.is_none() {
                state.voice = Some(play(&sound.loop_alias, 0));
            }
        } else {
            state.stop_voice(frame, sound.fade_out_ms);
            play(&sound.end_alias, 0);
            state.playing = false;
        }
    } else if active && !sound.loop_alias.is_empty() {
        state.voice = Some(play(&sound.loop_alias, sound.fade_in_ms));
        state.playing = true;
    }
}
