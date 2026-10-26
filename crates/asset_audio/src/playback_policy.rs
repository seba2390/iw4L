mod compiler;

use std::sync::Arc;

use crate::{AssetNamespace, CapturedAlias, ChannelKey, EntChannel, VoicePriority};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VoiceLimitMode {
    #[default]
    Unlimited,
    Oldest,
    Reject,
    Priority,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VoiceLimitSource {
    #[default]
    NotAuthored,
    Native,
    UnknownFlagsUnlimitedCompatibility,
    ZeroOldestCountOneCompatibility,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct VoiceLimit {
    pub mode: VoiceLimitMode,
    pub count: u8,
    pub per_emitter: bool,
    pub source: VoiceLimitSource,
}

#[derive(Clone, Copy, Debug)]
pub struct ChannelAdmission {
    pub key: ChannelKey,
    pub maximum: i32,
    pub restricted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpatialPolicyFailure {
    Unsupported(AssetNamespace),
    MissingChannel(AssetNamespace),
    MissingFalloffCurve,
    InvalidFalloffCurve,
}

#[derive(Clone, Debug)]
pub struct SpatialPlaybackPolicy {
    pub dist_min: f32,
    pub dist_max: f32,
    pub knots: Arc<[[f32; 2]]>,
    pub near_knots: Option<Arc<[[f32; 2]]>>,
}

/// An alias's reverb send: its level, and the distance falloff it takes
/// for a placed sound (the dry falloff's start, the send's own end and
/// curves) once its curves resolve.
#[derive(Clone, Debug)]
pub struct ReverbSendPolicy {
    pub send: f32,
    pub falloff: Option<SpatialPlaybackPolicy>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScalarRangePolicy {
    Authored([f32; 2]),
    ZeroRangeUnityCompatibility,
}

impl ScalarRangePolicy {
    fn sample(self, random: f32) -> f32 {
        match self {
            Self::Authored([low, high]) => crate::lerp_range(low, high, random),
            Self::ZeroRangeUnityCompatibility => 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GroupGainPolicy {
    Ungrouped,
    Resolved {
        native_index: u32,
        scale: f32,
    },
    Invalid {
        native_index: u32,
        error: crate::MixerGroupError,
    },
    MissingGroupUnityCompatibility {
        native_index: u32,
    },
    UnknownIndexUnityCompatibility,
}

pub const MAX_SECONDARY_DEPTH: u8 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecondaryActivation {
    OnResolution,
    OnPrimaryPrepared,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecondaryPolicySource {
    IndependentLayers,
    PrimaryPreparedCompatibility,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerLifetime {
    ParentGroup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerPitch {
    IndependentAuthoredRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerFailure {
    Independent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecondaryLayerPolicy {
    pub alias: String,
    pub activation: SecondaryActivation,
    pub source: SecondaryPolicySource,
    pub lifetime: LayerLifetime,
    pub pitch: LayerPitch,
    pub failure: LayerFailure,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnsupportedCueFeature {
    StartDelay(i32),
    Chain(String),
    MixerGroup(String),
    SpeakerMap(String),
    SpeakerGains,
}

#[derive(Clone, Debug)]
pub struct CueCompositionPolicy {
    pub start_delay_ms: u32,
    pub secondary: Option<SecondaryLayerPolicy>,
    pub unsupported: Arc<[UnsupportedCueFeature]>,
}

impl GroupGainPolicy {
    fn scale(self) -> Result<f32, crate::MixerGroupError> {
        match self {
            Self::Resolved { scale, .. } => Ok(scale),
            Self::Invalid { error, .. } => Err(error),
            _ => Ok(1.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopingPolicy {
    OneShot,
    Loop,
    UnknownOneShotCompatibility,
}

impl LoopingPolicy {
    pub fn is_looping(self) -> bool {
        self == Self::Loop
    }

    fn compile(authored: Option<bool>) -> Self {
        match authored {
            Some(false) => Self::OneShot,
            Some(true) => Self::Loop,
            None => Self::UnknownOneShotCompatibility,
        }
    }
}

/// A mix snapshot a sound applies while it plays (Black Ops' voice slot):
/// its name hash, and whether the sound's distance weakens it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoiceSnapshotPolicy {
    pub hash: u32,
    pub by_distance: bool,
}

/// Black Ops' per-voice low-pass: how many decibels its one-pole filter
/// takes off at the reference frequency. A sound is filtered by its
/// snapshot group's low-pass value (`(1 - value) x snapshot_db`) and, for an
/// alias flagged for it, by its distance; the stronger of the two applies.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceLowpassPolicy {
    /// The attenuation at a snapshot low-pass value of 0
    /// (`snd_occlusion_attenuation`).
    pub snapshot_db: f32,
    /// The distance low-pass: none at `min`, `db` at `max` and beyond.
    pub distance: Option<DistanceLowpass>,
    /// A big sound's distance low-pass is scaled in from none at the
    /// listener to all of it at this distance.
    pub big_distance: Option<f32>,
    /// Black Ops' occlusion of the voice: how much world lies between it
    /// and the listener, traced for a placed sound.
    pub occlusion: VoiceOcclusionPolicy,
}

/// Black Ops' occlusion bytes of an alias and what they act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoiceOcclusionPolicy {
    /// The alias's occlusion level: its share of the occlusion low-pass, and
    /// with `blend` how much occlusion moves the dry and wet levels.
    pub level: u8,
    pub blend: u8,
    /// A placed sound, whose occlusion is traced.
    pub traced: bool,
    /// Its occlusion low-pass runs (a placed sound with occlusion on).
    pub lowpass: bool,
    /// The sound is low-passed by distance: far away it starts at half its
    /// occlusion level instead of being traced.
    pub distance_lowpass: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DistanceLowpass {
    pub min: f32,
    pub max: f32,
    pub db: f32,
}

#[derive(Clone, Debug)]
pub struct AliasPlaybackPolicy {
    pub namespace: AssetNamespace,
    looping: LoopingPolicy,
    streamed_decode: crate::StreamedDecodePolicy,
    pub channel: Option<ChannelKey>,
    pub channel_admission: Option<ChannelAdmission>,
    pub priority: Option<VoicePriority>,
    pub spatial: Option<Result<SpatialPlaybackPolicy, SpatialPolicyFailure>>,
    pub limits: [VoiceLimit; 2],
    pub volume_range: ScalarRangePolicy,
    pub group_gain: GroupGainPolicy,
    pub pitch_range: ScalarRangePolicy,
    pub composition: CueCompositionPolicy,
    pub stereo_speaker_gains: Option<[[f32; 2]; 2]>,
    /// The snapshot group whose mix snapshot volume the alias plays at.
    pub snapshot_group: Option<u8>,
    /// The mix snapshot the alias applies while it plays.
    pub voice_snapshot: Option<VoiceSnapshotPolicy>,
    /// The game's per-voice low-pass, for a game that runs one.
    pub lowpass: Option<VoiceLowpassPolicy>,
    /// The game's whole-mix fade leaves the sound alone: Black Ops spares a
    /// one-shot whose group is in sound category 3.
    pub master_fade_exempt: bool,
    /// The sound category of the alias's mixer group, whose menu volume it
    /// plays at, for a game that reads one.
    pub volume_category: Option<i32>,
    /// What it sends to the game's reverb, for a game whose aliases carry a
    /// send.
    pub reverb: Option<ReverbSendPolicy>,
    /// The voice skips the game's bus ahead of its master bus (that bus's
    /// compressor) and joins the master bus directly.
    pub master_bus_direct: bool,
    pub loaded_binding_origin: crate::LoadedBindingOrigin,
}

impl AliasPlaybackPolicy {
    pub fn volume(&self, random: f32) -> Result<f32, crate::MixerGroupError> {
        Ok(self.volume_range.sample(random) * self.group_gain.scale()?)
    }

    pub fn pitch(&self, random: f32) -> f32 {
        self.pitch_range.sample(random)
    }

    pub fn streamed_decode(&self) -> crate::StreamedDecodePolicy {
        self.streamed_decode
    }

    pub fn looping(&self) -> LoopingPolicy {
        self.looping
    }

    pub(crate) fn compile(
        catalog: &crate::SoundCatalog,
        alias: usize,
        variant: usize,
    ) -> Option<Self> {
        compiler::compile(catalog, alias, variant)
    }
}
