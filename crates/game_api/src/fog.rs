//! A game's own frame fog: the fog its renderer draws each frame, from the
//! map's fog data and what the client's scripts and server set, given as the
//! fog code constants its materials read.

/// The fog code constant rows a game's materials read, in this order: fog
/// colour, fog density, fog density integral, sun fog direction, sun fog
/// colour, sun fog angles.
pub type FogRows = [[f32; 4]; 6];

/// A map's fog volumes as its world asset holds them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldFogData {
    /// The fog outside every volume, and the rate a change toward it runs at.
    pub default_fog: [f32; 16],
    pub default_rate: f32,
    pub volumes: Vec<WorldFogVolume>,
    /// The planes the volumes' packed words index.
    pub volume_planes: Vec<[f32; 4]>,
    pub modifiers: Vec<WorldFogModifier>,
    /// The planes the modifiers' packed words index.
    pub modifier_planes: Vec<[f32; 4]>,
}

/// A fog volume: its packed word (bank bits, plane count and first plane),
/// the rate a change toward it runs at, and its fog.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WorldFogVolume {
    pub packed: u32,
    pub rate: f32,
    pub fog: [f32; 16],
}

/// A fog modifier volume: its packed word, the rate a change toward it runs
/// at, and the scales it applies.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WorldFogModifier {
    pub packed: u32,
    pub rate: f32,
    pub scales: [f32; 6],
}

/// A switch of the client's script fog: to slot `slot` (whose contents are
/// `block`), at `start_ms`, over `transition_ms`. Each switch carries a new
/// `serial`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScriptFogSwitch {
    pub serial: u32,
    pub slot: u8,
    pub block: [f32; 20],
    pub start_ms: i32,
    pub transition_ms: i32,
}

/// What a frame's fog is drawn from.
#[derive(Clone, Copy, Debug)]
pub struct FogFrame {
    /// The client's time, in milliseconds.
    pub time_ms: i32,
    pub eye: [f32; 3],
    /// The client's active world fog bank mask, once its scripts or server
    /// set one.
    pub bank: Option<u16>,
    /// The view jumped this frame (the snapshot cut the view).
    pub view_cut: bool,
    /// The client's last script fog switch.
    pub script: Option<ScriptFogSwitch>,
}

/// A client's frame fog between frames.
pub trait FrameFog: Send + Sync {
    /// Runs the frame and gives the rows to draw with.
    fn frame(&mut self, frame: &FogFrame) -> FogRows;
}

/// A game's frame fog: `start` makes a client's state for a map with the
/// given fog data.
#[derive(Clone, Copy, Debug)]
pub struct FrameFogRules {
    pub start: fn(world: Option<&WorldFogData>) -> Box<dyn FrameFog>,
}
