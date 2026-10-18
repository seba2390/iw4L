//! A camera that looks down on the players from above, which a game's
//! client dvars turn on in place of the player's own eye: where it sits, how
//! the mouse aims through it, and the yaw the player's moves follow.

/// What the client's dvars ask of the camera; the game reads them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TopDownCamera {
    /// The game's own numbering of the camera's ways of following.
    pub mode: i32,
    pub offset: [f32; 3],
    pub angles: [f32; 3],
    /// The point the camera stays near.
    pub center: [f32; 3],
    /// How far from the player the aiming cursor sits.
    pub cursor_dist: f32,
}

/// What the camera looks at this frame.
#[derive(Clone, Copy, Debug)]
pub struct TopDownScene<'a> {
    /// The local player's origin and view angles, as predicted.
    pub origin: [f32; 3],
    pub viewangles: [f32; 3],
    /// Where each player in the game's camera set stands.
    pub players: &'a [[f32; 3]],
    /// The frame's length, in milliseconds.
    pub frametime_ms: i32,
    /// The screen is 4:3.
    pub four_by_three: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TopDownPose {
    pub origin: [f32; 3],
    pub angles: [f32; 3],
}

/// What the camera and its aim keep from frame to frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TopDownState {
    /// The last frame's view origin, which the camera eases from.
    pub last_origin: Option<[f32; 3]>,
    /// The aiming cursor, right and down of the player on the screen.
    pub cursor: [f32; 2],
    /// The pitch and yaw the aim turns from, once it has turned.
    pub aim: Option<[f32; 2]>,
}

#[derive(Clone, Copy, Debug)]
pub struct TopDownRules {
    /// The camera the client's dvars ask for; `None` keeps the player's eye.
    pub camera: fn(dvar: &dyn Fn(&str) -> Option<String>) -> Option<TopDownCamera>,
    /// Where the camera sits this frame.
    pub view: fn(&TopDownCamera, &TopDownScene<'_>, &mut TopDownState) -> TopDownPose,
    /// The turn of the player's view angles, in degrees, that this frame's
    /// mouse (after sensitivity, right and down) makes through the cursor;
    /// `None` leaves the view where it is.
    pub aim: fn(
        &TopDownCamera,
        mouse: [f32; 2],
        frametime: f32,
        viewangles: [f32; 3],
        &mut TopDownState,
    ) -> Option<[f32; 2]>,
    /// The yaw the player's moves follow in place of the view's.
    pub move_yaw: fn(&TopDownCamera) -> Option<f32>,
    /// What the game's camera leaves out (its ledger lists them).
    pub gaps: &'static [&'static crate::Unknown],
}
