use crate::Trace;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pml {
    pub forward: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
    pub frametime: f32,
    pub msec: i32,
    pub walking: bool,
    pub ground_plane: bool,
    pub almost_ground_plane: bool,
    pub ground_trace: Trace,
    pub impact_speed: f32,
    pub previous_origin: [f32; 3],
    pub previous_velocity: [f32; 3],
}
