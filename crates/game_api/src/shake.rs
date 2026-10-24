//! A game's own camera shake: the earthquakes a client keeps and the turn
//! they add to its view each frame.

/// An earthquake as a client receives it: from its start, for its duration,
/// it shakes views within its radius of its origin.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Earthquake {
    pub scale: f32,
    pub duration_ms: i32,
    pub origin: [f32; 3],
    pub radius: f32,
    /// Client time, in milliseconds.
    pub start_ms: i32,
}

/// An earthquake a client keeps, with the strength and fade its shake last
/// found for it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShakeSlot {
    pub quake: Earthquake,
    pub strength: f32,
    pub fade: f32,
}

/// What a client's shake keeps from frame to frame; the game sizes the
/// slots.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShakeState {
    pub slots: Vec<ShakeSlot>,
    /// The phase the shake's waves start from.
    pub phase: f32,
    /// The client's random state the shake draws from.
    pub random: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct ShakeRules {
    /// The client takes an earthquake it receives at client time `now_ms`,
    /// or drops it.
    pub start: fn(&mut ShakeState, Earthquake, now_ms: i32),
    /// The turn (pitch, yaw, roll, in degrees) the client's earthquakes add
    /// to a view from `eye` at client time `now_ms`; called once a frame.
    pub angles: fn(&mut ShakeState, eye: [f32; 3], now_ms: i32) -> [f32; 3],
    /// What the game's shake leaves out (its ledger lists them).
    pub gaps: &'static [&'static crate::Unknown],
}
