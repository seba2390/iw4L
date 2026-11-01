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
    /// What its turret aims at; none with the turret off.
    pub turret_aim: Option<HeliAim>,
    /// The turret's pitch and yaw off the body, and how fast it turns.
    pub turret_angles: [f32; 2],
    pub turret_rate: f32,
    /// How far its turning has settled, for `turret_rotate_stopped`.
    pub turret_settling: u8,
    /// The flight its seat 0 player steers while it has no goal.
    pub piloted: PilotedFlight,
    /// `setheliheightlock`: the pilot's altitude holds over the ground.
    pub height_lock: bool,
}

/// What a piloted helicopter's frame reads from its pilot and keeps.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PilotedFlight {
    /// This frame's pilot, when one sits in its driver's seat.
    pub pilot: Option<HeliPilot>,
    /// Its velocity along its own axes.
    pub local_velocity: [f32; 3],
    /// Degrees a second its yaw turns.
    pub yaw_rate: f32,
    /// How far it tilts, and how fast, pitch then roll.
    pub tilt: [f32; 2],
    pub tilt_rate: [f32; 2],
    /// How high over its ground the pilot holds it, and that ground's height.
    pub lift: f32,
    pub ground: f32,
    pub jitter: HeliJitter,
    /// Its vehicle file.
    pub motion: VehicleMotion,
}

/// `setjitterparams`: a shake of its tilt, stronger the slower it flies.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HeliJitter {
    pub min_ms: i32,
    pub max_ms: i32,
    pub next_ms: i32,
    pub amplitude: [f32; 3],
    pub step: [f32; 3],
    pub goal: [f32; 3],
    pub smoothed: [f32; 3],
}

/// A pilot's controls for one frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HeliPilot {
    /// The move sticks, -127 to 127.
    pub forward: i8,
    pub right: i8,
    /// The yaw axis of the look stick.
    pub yaw_axis: i8,
    pub climb: bool,
    pub descend: bool,
    /// Where the pilot looks.
    pub view: [f32; 3],
    /// Now, for the jitter.
    pub time_ms: i32,
}

/// What a helicopter's turret aims at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HeliAim {
    Point([f32; 3]),
    Entity { entity: i32, offset: [f32; 3] },
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
    pub turret_rot_rate: f32,
    /// The fields a pilot's move reads.
    pub remote_control: bool,
    pub view_pitch_offset: f32,
    pub view_influence: f32,
    pub tilt_from_accel: [f32; 2],
    pub tilt_from_decel: [f32; 2],
    pub tilt_from_velocity: [f32; 2],
    pub tilt_speed: [f32; 2],
    pub turret_span_right: f32,
    /// Points of its `steerGraph`, x then y; empty when it has none.
    pub steer_graph: [[f32; 2]; 8],
    pub steer_points: u8,
}

/// What a frame of flight says to the scripts.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HeliNotes {
    pub goal: bool,
    pub near_goal: bool,
    pub goal_yaw: bool,
    pub turret_rotate_stopped: bool,
    pub turret_on_target: bool,
    pub turret_not_on_target: bool,
    pub turret_on_vistarget: bool,
    pub turret_no_vis: bool,
    /// `veh_collision`: the surface's normal.
    pub collision: Option<[f32; 3]>,
    /// `veh_predictedcollision`: the surface ahead's normal.
    pub predicted_collision: Option<[f32; 3]>,
}

/// A helicopter's shot (`fireweapon`): its gun, what it may home on.
#[derive(Clone, Copy, Debug)]
pub struct HeliShot {
    pub entity: i32,
    pub weapon: u32,
    /// The barrel tag's place and axis (forward, left, up) on the model.
    pub origin: [f32; 3],
    pub axis: [[f32; 3]; 3],
    /// The tag the gun turns about, when the model has one.
    pub pivot: Option<[f32; 3]>,
    pub target: Option<i32>,
}

/// What a helicopter's frame reads from the match.
pub trait HeliWorld {
    fn dvar(&self, name: &str) -> Option<String>;
    /// A random float between the two.
    fn random_range(&mut self, low: f32, high: f32) -> f32;
    /// Where the entity it looks at stands (`setlookatent`).
    fn look_at(&mut self) -> Option<[f32; 3]>;
    /// Where its barrel is, and the entity's place.
    fn barrel(&mut self) -> Option<[f32; 3]>;
    fn entity_origin(&mut self, entity: i32) -> Option<[f32; 3]>;
    /// Nothing solid between the two, the target itself left out.
    fn sight_clear(&mut self, from: [f32; 3], to: [f32; 3], target: i32) -> bool;
    /// A box swept through what stops a vehicle: how far it got and what it
    /// met, none when it started in something solid.
    fn sweep(
        &mut self,
        from: [f32; 3],
        to: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
    ) -> Option<(f32, [f32; 3])>;
    /// The height the pilot's height lock meets at a place, searching this far
    /// up and down.
    fn height_lock_at(&mut self, at: [f32; 3], reach: f32) -> Option<f32>;
}
