//! The game's reverb rooms (`setReverb` / `deactivateReverb`): three
//! priorities, each holding a room and the dry and wet levels it fades to;
//! the highest active one is in force, and each frame its levels move
//! toward their targets by their steps per msec. The room's reverb preset
//! from the banks sets the reverb's parameters; its dry level scales every
//! voice's dry mix and its wet level every voice's reverb send.

use asset_core::AssetNamespace;
use bevy::prelude::*;
use frame::ScopeApp;

use crate::radverb::ReverbParameters;

/// The priorities: 0 the default room, 1 the level's, 2 the shellshock's.
const PRIORITIES: usize = 3;
pub(crate) const SHELLSHOCK_PRIORITY: usize = 2;
/// The sample rate the reverb's parameters name.
const SAMPLE_RATE: f32 = 48_000.0;

#[derive(Clone, Copy)]
struct Room {
    hash: u32,
    dry: f32,
    dry_target: f32,
    dry_step: f32,
    wet: f32,
    wet_target: f32,
    wet_step: f32,
    active: bool,
}

impl Room {
    const IDLE: Self = Self {
        hash: 0,
        dry: 0.0,
        dry_target: 0.0,
        dry_step: 0.0,
        wet: 0.0,
        wet_target: 0.0,
        wet_step: 0.0,
        active: false,
    };
}

#[derive(Resource)]
pub(crate) struct ReverbMix {
    rooms: [Room; PRIORITIES],
    current: usize,
    /// The msec the frames have run that the levels have not moved by yet.
    pending_ms: f64,
}

impl Default for ReverbMix {
    /// The default room, fully dry, in force.
    fn default() -> Self {
        let mut rooms = [Room::IDLE; PRIORITIES];
        rooms[0] = Room {
            hash: default_hash(),
            dry: 1.0,
            dry_target: 1.0,
            active: true,
            ..Room::IDLE
        };
        Self {
            rooms,
            current: 0,
            pending_ms: 0.0,
        }
    }
}

fn default_hash() -> u32 {
    asset_transport::sab::snd_hash_name("default")
}

impl ReverbMix {
    /// A priority takes a room, starting from the levels in force and
    /// stepping to its own over `fade_ms` (at least 1); it comes into force
    /// unless a higher priority is active.
    pub(crate) fn set(&mut self, priority: usize, hash: u32, dry: f32, wet: f32, fade_ms: i32) {
        let fade = fade_ms.max(1) as f32;
        let from = self.rooms[self.current];
        let room = &mut self.rooms[priority];
        room.active = true;
        room.dry_target = dry;
        room.dry = from.dry;
        room.dry_step = (dry - from.dry) / fade;
        room.wet_target = wet;
        room.wet = from.wet;
        room.wet_step = (wet - from.wet) / fade;
        room.hash = hash;
        if priority != self.current && self.rooms[priority + 1..].iter().any(|room| room.active) {
            return;
        }
        self.current = priority;
    }

    /// A priority lets its room go; when it was in force, the next active
    /// one below takes over from its levels, stepping to its own over
    /// `fade_ms` (at least 1).
    pub(crate) fn deactivate(&mut self, priority: usize, fade_ms: i32) {
        self.rooms[priority].active = false;
        if priority != self.current {
            return;
        }
        let Some(next) = (0..priority).rev().find(|&at| self.rooms[at].active) else {
            return;
        };
        let fade = fade_ms.max(1) as f32;
        let from = self.rooms[priority];
        let room = &mut self.rooms[next];
        room.dry = from.dry;
        room.dry_step = (room.dry_target - room.dry) / fade;
        room.wet = from.wet;
        room.wet_step = (room.wet_target - room.wet) / fade;
        self.current = next;
    }

    /// The room in force moves its levels by `ms` steps, each stopping at
    /// its target.
    fn advance(&mut self, ms: i32) {
        let room = &mut self.rooms[self.current];
        let step = |value: &mut f32, target: f32, step: &mut f32| {
            if *step == 0.0 {
                return;
            }
            *value += ms as f32 * *step;
            let reached = if 0.0 <= *step {
                target <= *value
            } else {
                *value <= target
            };
            if reached {
                *value = target;
                *step = 0.0;
            }
        };
        step(&mut room.dry, room.dry_target, &mut room.dry_step);
        step(&mut room.wet, room.wet_target, &mut room.wet_step);
    }
}

/// The reverb's parameters for a room's preset: the sample rate, the
/// preset's values (its fourth value four times over), the listener's
/// surroundings' late gains and the matrix choice.
fn parameters(values: &[f32; 15]) -> ReverbParameters {
    let mut p = [0.0; 25];
    p[0] = SAMPLE_RATE;
    p[1..6].copy_from_slice(&values[0..5]);
    // The late gains by the walls around the listener; unscaled while
    // their traces are not run.
    p[6..10].fill(1.0);
    p[10..15].copy_from_slice(&values[5..10]);
    p[15..19].fill(values[10]);
    p[19..23].copy_from_slice(&values[11..15]);
    ReverbParameters(p)
}

#[allow(clippy::too_many_arguments)]
fn update_reverb_mix(
    mut mix: ResMut<ReverbMix>,
    mut events: MessageReader<net::SvcScriptAudio>,
    bank: Option<Res<crate::SoundBank>>,
    namespace: Option<Res<crate::ambient::SoundBankNamespace>>,
    local: Option<Res<net::LocalPresentClient>>,
    runtime: Res<crate::AudioRuntime>,
    time: Res<Time>,
) {
    let local = local.map(|local| local.0);
    for event in events.read() {
        if let sim::ScriptAudioCommand::ClientReverb {
            client,
            priority,
            room,
            fade_ms,
        } = &event.0
            && Some(*client) == local
        {
            let priority = usize::from(*priority).min(PRIORITIES - 1);
            match room {
                Some((name, dry, wet)) => {
                    let hash = asset_transport::sab::snd_hash_name(name);
                    mix.set(priority, hash, *dry, *wet, *fade_ms);
                }
                None => mix.deactivate(priority, *fade_ms),
            }
        }
    }
    mix.pending_ms += f64::from(time.delta_secs()) * 1000.0;
    let ms = mix.pending_ms.floor();
    mix.pending_ms -= ms;
    mix.advance(ms as i32);
    let reverb = bank.zip(namespace).and_then(|(bank, namespace)| {
        let namespace: AssetNamespace = namespace.namespace;
        let room = mix.rooms[mix.current].hash;
        let room = if room == 0 { default_hash() } else { room };
        let preset = bank
            .0
            .reverb_preset(namespace, room)
            .or_else(|| bank.0.reverb_preset(namespace, default_hash()))?;
        Some((parameters(&preset.values), namespace))
    });
    let room = mix.rooms[mix.current];
    runtime.set_reverb(reverb, room.dry, room.wet);
}

pub(crate) fn register(app: &mut App) {
    app.scoped::<ReverbMix>(frame::MatchScope::Live)
        .add_systems(
            Update,
            update_reverb_mix
                .in_set(frame::InMatch)
                .in_set(net::ClientSet::Effects),
        );
}
