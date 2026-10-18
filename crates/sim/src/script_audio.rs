#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScriptAmbient {
    pub alias: Option<String>,
    pub start_ms: i32,
    pub end_ms: i32,
}

impl ScriptAmbient {
    pub fn valid(&self) -> bool {
        self.start_ms >= 0
            && self.end_ms >= self.start_ms
            && self
                .alias
                .as_ref()
                .is_none_or(|name| !name.is_empty() && name.len() <= 1023)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ScriptAudioCommand {
    MusicPlay(String),
    MusicStop {
        fade_ms: i32,
    },
    SoundFade {
        volume: f32,
        fade_ms: i32,
    },
    ChannelVolumes {
        client: crate::ClientId,
        priority: u8,
        volumes: Option<std::collections::BTreeMap<String, f32>>,
        fade_ms: i32,
    },
    DeactivateChannelVolumes {
        client: crate::ClientId,
        priority: u8,
        fade_ms: i32,
    },
    /// A sound a client's own scripts play (`playSound` on its client VM),
    /// named by the playback id they answered, for that client alone: at a
    /// point, or without one as a local sound.
    ClientSound {
        client: crate::ClientId,
        id: i32,
        alias: String,
        origin: Option<[f32; 3]>,
        volume: f32,
        /// How long it fades in from silence, in msec (0 starts at its
        /// volume).
        fade_in_ms: i32,
    },
    /// The client's scripts stop a sound they played (`stopSound`).
    ClientSoundStop {
        client: crate::ClientId,
        id: i32,
    },
    /// The volume a client's sound moves to (`setSoundVolume`), at its rate.
    ClientSoundVolume {
        client: crate::ClientId,
        id: i32,
        volume: f32,
    },
    /// How fast a client's sound's volume moves, per second
    /// (`setSoundVolumeRate`); zero moves it at once.
    ClientSoundVolumeRate {
        client: crate::ClientId,
        id: i32,
        rate: f32,
    },
    /// A mix snapshot a client's scripts put in one of the game's snapshot
    /// slots (`setGroupSnapshot` and its kin), at a weight of 0 to 1. An
    /// empty name is the default snapshot.
    ClientSnapshot {
        client: crate::ClientId,
        slot: u8,
        name: String,
        value: f32,
    },
    /// A sound context a client's scripts set (`setSoundContext`); an empty
    /// value unsets it.
    ClientSoundContext {
        client: crate::ClientId,
        context: String,
        value: String,
    },
    /// A turret's shot from one of its barrels, for every client's fire
    /// sound: its entity, weapon and the barrel's muzzle.
    TurretFire {
        entity: i32,
        weapon: u32,
        barrel: i32,
        origin: [f32; 3],
    },
}

/// What a client's sound system says of a sound its scripts played, for
/// their `soundPlaying`, `getPlaybackTime` and `getKnownLength`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClientSoundStatus {
    pub id: i32,
    pub playing: bool,
    /// How far it has played, in msec.
    pub playback_ms: i32,
    /// Its length in msec, or 0 while not known.
    pub length_ms: i32,
}

impl ScriptAudioCommand {
    pub fn target(&self) -> Option<crate::ClientId> {
        match self {
            Self::ChannelVolumes { client, .. }
            | Self::DeactivateChannelVolumes { client, .. }
            | Self::ClientSound { client, .. }
            | Self::ClientSoundStop { client, .. }
            | Self::ClientSoundVolume { client, .. }
            | Self::ClientSoundVolumeRate { client, .. }
            | Self::ClientSnapshot { client, .. }
            | Self::ClientSoundContext { client, .. } => Some(*client),
            _ => None,
        }
    }

    pub fn valid(&self) -> bool {
        match self {
            Self::MusicPlay(alias) => alias.len() <= 1023,
            Self::MusicStop { fade_ms } => *fade_ms >= 0,
            Self::ChannelVolumes {
                priority,
                volumes,
                fade_ms,
                ..
            } => {
                (1..=3).contains(priority)
                    && *fade_ms >= 0
                    && volumes.as_ref().is_none_or(|volumes| {
                        !volumes.is_empty()
                            && volumes.len() <= 64
                            && volumes.iter().all(|(name, gain)| {
                                !name.is_empty()
                                    && name.len() <= 64
                                    && *name == name.to_ascii_lowercase()
                                    && gain.is_finite()
                                    && (0.0..=1.0).contains(gain)
                            })
                    })
            }
            Self::DeactivateChannelVolumes {
                priority, fade_ms, ..
            } => (1..=3).contains(priority) && *fade_ms >= 0,
            Self::SoundFade { volume, fade_ms } => {
                volume.is_finite() && *volume >= 0.0 && *fade_ms >= 0
            }
            Self::ClientSound {
                alias,
                origin,
                volume,
                fade_in_ms,
                ..
            } => {
                !alias.is_empty()
                    && alias.len() <= 1023
                    && (0.0..=1.0).contains(volume)
                    && (0..=32_000).contains(fade_in_ms)
                    && origin.is_none_or(|origin| origin.iter().all(|c| c.is_finite()))
            }
            Self::ClientSoundStop { .. } => true,
            Self::ClientSoundVolume { volume, .. } => volume.is_finite(),
            Self::ClientSoundVolumeRate { rate, .. } => rate.is_finite(),
            Self::ClientSnapshot {
                slot, name, value, ..
            } => (1..=10).contains(slot) && name.len() <= 1023 && (0.0..=1.0).contains(value),
            Self::ClientSoundContext { context, value, .. } => {
                context.len() <= 1023 && value.len() <= 1023
            }
            Self::TurretFire { barrel, origin, .. } => {
                (0..=1).contains(barrel) && origin.iter().all(|c| c.is_finite())
            }
        }
    }
}
