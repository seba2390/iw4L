//! A game's helicopter flight: what the match keeps for a flying vehicle and
//! the game's frame that moves it (`PlayerMovement::helicopter_frame`).

/// What the scripts and the frame keep for a helicopter.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HeliMove {
    pub origin: [f32; 3],
    pub velocity: [f32; 3],
    /// The length of `velocity`.
    pub speed: f32,
    pub angles: [f32; 3],
    /// The yaw at the start of this frame.
    pub previous_yaw: f32,
    /// Degrees a second each angle turns now, and at most.
    pub angle_rate: [f32; 3],
    pub angle_limit: [f32; 3],
    pub yaw_accel: f32,
    pub yaw_decel: f32,
    pub yaw_overshoot: f32,
    /// Slowed down near its yaw goal after overshooting it.
    pub yaw_settling: bool,
    pub last_yaw_goal: f32,
    /// The acceleration it tilts by.
    pub tilt_accel: [f32; 3],
    pub max_pitch: f32,
    pub max_roll: f32,
    /// The speed at which it tilts fully.
    pub tilt_speed: f32,
    /// How much of its speed swings round with its yaw.
    pub turning: f32,
    /// Its vehicle file's speed and acceleration, for `resumespeed`.
    pub def_speed: f32,
    pub def_accel: f32,
    /// Script's speed, acceleration and deceleration, when set.
    pub script_speed: bool,
    pub max_speed: f32,
    pub accel: f32,
    pub decel: f32,
    /// Heading for `goal`; it then hovers there when it stops at its goal.
    pub has_goal: bool,
    pub hovering: bool,
    pub goal: [f32; 3],
    pub stop_at_goal: bool,
    pub stopping: bool,
    pub near_goal: f32,
    pub target_yaw: Option<f32>,
    pub goal_yaw: Option<f32>,
    pub hover_radius: f32,
    pub hover_speed: f32,
    pub hover_accel: f32,
    pub hover_offset: [f32; 3],
}

/// What a vehicle file says of how the vehicle flies; speeds in inches a
/// second.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VehicleMotion {
    pub max_speed: f32,
    pub accel: f32,
    pub rot_rate: f32,
    pub rot_accel: f32,
    pub max_body_pitch: f32,
    pub max_body_roll: f32,
}

/// What a frame of flight says to the scripts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeliNotes {
    pub goal: bool,
    pub near_goal: bool,
    pub goal_yaw: bool,
}

/// What a helicopter's frame reads from the match.
pub trait HeliWorld {
    fn dvar(&self, name: &str) -> Option<String>;
    /// A random float between the two.
    fn random_range(&mut self, low: f32, high: f32) -> f32;
    /// Where the entity it looks at stands (`setlookatent`).
    fn look_at(&mut self) -> Option<[f32; 3]>;
}
