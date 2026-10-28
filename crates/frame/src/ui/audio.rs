use bevy::prelude::*;

#[derive(Message, Clone, Debug)]
pub struct UiPlaySound {
    pub alias: String,
}

#[derive(Message, Clone, Debug)]
pub struct UiPlayMusic {
    pub alias: String,
}

#[derive(Message, Clone, Debug, Default)]
pub struct UiStopMusic;

/// A cinematic's sound, interleaved: played over the game's until stopped
/// or ended.
#[derive(Message, Clone, Debug)]
pub struct UiPlayCinematic {
    pub samples: std::sync::Arc<[f32]>,
    pub rate: u32,
    pub channels: u16,
}

#[derive(Message, Clone, Debug, Default)]
pub struct UiStopCinematic;
