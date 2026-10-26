//! Black Ops' reverb proximity: how close the walls are on each side of
//! the listener, measured by four fans of sight traces in the occlusion
//! worker's batch, each side's walls cutting its late reverb level.

use bevy::prelude::*;
use sim::SimContent;

/// `snd_proximity_enable`.
const ENABLE: bool = true;
/// `snd_proximity_min_dist` and `snd_proximity_max_dist`: a side's
/// measure at or below the first takes the whole cut, at or past the second
/// none.
const MIN_DISTANCE: f32 = 20.0;
const MAX_DISTANCE: f32 = 400.0;
/// `snd_proximity_max_late_cut`, in decibels.
const MAX_LATE_CUT: f32 = -20.0;
/// `snd_proximity_filter`: the share of the new gain taken each time.
const FILTER: f32 = 0.1;
/// How far each ray reaches, over the sixteen rays of a side.
const RAY_LENGTH: f32 = 1000.0;
const RAY_SHARE: f32 = 62.5;
/// The angle between two rays of a side.
const RAY_STEP: f32 = f32::from_bits(0x3dc9_0fdb);
const TRACE_MASK: u32 = 0x31;
/// The smallest gain that is not silence.
const EPSILON: f32 = f32::from_bits(0x3780_4000);

/// The late reverb gain of each side of the listener (left to front,
/// front to right, right to back, back to left), eased each time the
/// worker is free. Black Ops' start at 0 with the process.
#[derive(Resource, Default, Clone, Copy)]
pub(crate) struct ProximityGains(pub [f32; 4]);

impl ProximityGains {
    /// Eases each side's gain toward what its last measure asks for: the
    /// cut lessens from the whole of it at the near distance to none at
    /// the far one.
    pub fn step(&mut self, measures: &[f32; 4]) {
        if !ENABLE {
            self.0 = [1.0; 4];
            return;
        }
        for (gain, measure) in self.0.iter_mut().zip(measures) {
            let span = MAX_DISTANCE - MIN_DISTANCE;
            let along = measure - MIN_DISTANCE;
            let along = if -along >= 0.0 {
                0.0
            } else if along - span >= 0.0 {
                span
            } else {
                along
            };
            let db = (1.0 - along / span) * MAX_LATE_CUT + along / span * 0.0;
            *gain = (1.0 - FILTER) * *gain + FILTER * db_gain(db);
        }
    }
}

/// A decibel level as a gain, from silence to 1.
fn db_gain(db: f32) -> f32 {
    let gain = 10.0f64.powf(f64::from(db * 0.05)) as f32;
    let gain = if gain < EPSILON { 0.0 } else { gain };
    if -gain >= 0.0 { 0.0 } else { gain.min(1.0) }
}

/// Each side's measure of open space around the listener: sixteen rays
/// over the side's quarter turn, level, out to 1000; a clear ray adds
/// 62.5 and a blocked one 62.5 x (1 - (1 - fraction)²).
pub(crate) fn measure(world: &SimContent, listener: [f32; 3], forward: [f32; 3]) -> [f32; 4] {
    use std::f32::consts::FRAC_PI_2;

    let base = f64::from(forward[0]).atan2(f64::from(forward[1])) as f32;
    std::array::from_fn(|side| {
        let first = side as f32 * FRAC_PI_2 + base - FRAC_PI_2;
        let mut open = 0.0f32;
        for ray in 0..16 {
            let angle = f64::from(ray as f32 * RAY_STEP + first);
            let end = [
                angle.sin() as f32 * RAY_LENGTH + listener[0],
                angle.cos() as f32 * RAY_LENGTH + listener[1],
                listener[2],
            ];
            open += match world.sight_trace_static_world(listener, end, TRACE_MASK) {
                Some(hit) => {
                    let near = 1.0 - hit.fraction;
                    (1.0 - near * near) * RAY_SHARE
                }
                None => RAY_SHARE,
            };
        }
        open
    })
}
