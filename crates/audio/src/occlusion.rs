//! Black Ops' sound occlusion: how much of the static world lies between
//! the listener and each placed voice, found by fans of sight traces and
//! eased toward over time.
//!
//! A voice's first trace runs as it starts (two rays a fan). After that a
//! batch on a worker traces every placed voice again (four rays a fan), then
//! the reverb proximity's fans; when it is done its findings become the
//! voices' targets, the proximity gains take a step and the next batch
//! starts.

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, futures_lite::future};
use sim::{SightBlocker, SimContent};
use std::collections::HashMap;
use std::sync::Arc;

use crate::runtime::{AudioRuntime, OcclusionVoice};

/// `snd_losOcclusion`: the first trace runs at all.
const LOS_OCCLUSION: bool = true;
/// `snd_occlusion_rate`: how fast a voice's occlusion moves a second.
const OCCLUSION_RATE: f32 = 6.0;
/// `snd_occlusion_cull_distance`: beyond it a voice low-passed by distance
/// starts at half its occlusion level instead of being traced.
const CULL_DISTANCE: f32 = 2000.0;
/// The static world the traces stop on.
const TRACE_MASK: u32 = 0x31;
/// The smallest level share and segment length that count.
const EPSILON: f32 = f32::from_bits(0x3780_4000);
/// The fewest blocked rays a fan's share is taken as.
const MIN_SHARE: f32 = 0.2;

pub(crate) fn register(app: &mut App) {
    app.init_resource::<OcclusionJobs>()
        .init_resource::<crate::proximity::ProximityGains>();
}

/// Each traced voice's last blocker, kept from batch to batch, the batch
/// the worker is tracing and the last batch's proximity measures.
#[derive(Resource, Default)]
pub(crate) struct OcclusionJobs {
    caches: HashMap<u64, Option<SightBlocker>>,
    batch: Option<Task<(Vec<Job>, [f32; 4])>>,
    proximity: [f32; 4],
}

/// One voice's trace in a batch: where the listener and the voice were when
/// it was scheduled, and what it found.
struct Job {
    id: u64,
    listener: [f32; 3],
    voice: [f32; 3],
    cache: Option<SightBlocker>,
    result: f32,
}

/// The traces' state through one run of fans: the last blocker (a brush is
/// tested on its own before the world is walked again) and whether the last
/// ray the world was walked for started solid, which a ray blocked by the
/// cached brush keeps.
struct Rays<'a> {
    world: &'a SimContent,
    start_solid: bool,
}

impl Rays<'_> {
    /// Whether the segment is blocked, and whether it counts. A clear ray
    /// leaves the last blocker cached.
    fn ray(&mut self, cache: &mut Option<SightBlocker>, a: [f32; 3], b: [f32; 3]) -> (bool, bool) {
        if let Some(SightBlocker::Brush(brush)) = *cache
            && self.world.segment_meets_brush(brush, a, b)
        {
            return (true, !self.start_solid);
        }
        let hit = self.world.sight_trace_static_world(a, b, TRACE_MASK);
        self.start_solid = hit.is_some_and(|hit| hit.start_solid);
        if let Some(hit) = hit {
            *cache = Some(hit.blocker);
        }
        (hit.is_some(), !self.start_solid)
    }

    /// The share of one fan's rays that are blocked: from the listener
    /// stepped twice along `dir` by `start_step` to the voice stepped `n`
    /// times by `end_step`. Rays starting solid do not count; a share of
    /// less than a fifth is taken as a fifth, and a fan with no counted ray
    /// as fully blocked.
    #[allow(clippy::too_many_arguments)]
    fn fan(
        &mut self,
        cache: &mut Option<SightBlocker>,
        listener: [f32; 3],
        voice: [f32; 3],
        dir: [f32; 3],
        start_step: f32,
        end_step: f32,
        n: u32,
    ) -> f32 {
        let (mut total, mut blocked) = (0u32, 0u32);
        for i in 1..=2u32 {
            let along = i as f32 * start_step;
            let start = std::array::from_fn(|k| listener[k] + dir[k] * along);
            for j in 1..=n {
                let along = j as f32 * end_step;
                let end = std::array::from_fn(|k| voice[k] + dir[k] * along);
                let (hit, counts) = self.ray(cache, start, end);
                if counts {
                    total += 1;
                    blocked += u32::from(hit);
                }
            }
        }
        if total == 0 {
            return 1.0;
        }
        (blocked as f32 / total as f32).max(MIN_SHARE)
    }

    /// The voice's occlusion: 0 in sight of the listener, else the least
    /// blocked share of three fans (vertical, and to either side across the
    /// line between them) of `n` rays to the voice each.
    fn occlusion(
        &mut self,
        cache: &mut Option<SightBlocker>,
        n: u32,
        listener: [f32; 3],
        voice: [f32; 3],
    ) -> f32 {
        let end_vertical = 1.0 / n as f32 * 40.0;
        let end_across = 1.0 / n as f32 * 50.0;
        let blocked = match *cache {
            Some(SightBlocker::Brush(brush))
                if self.world.segment_meets_brush(brush, listener, voice) =>
            {
                true
            }
            _ => {
                let hit = self
                    .world
                    .sight_trace_static_world(listener, voice, TRACE_MASK);
                *cache = hit.map(|hit| hit.blocker);
                hit.is_some()
            }
        };
        if !blocked {
            return 0.0;
        }
        let d: [f32; 3] = std::array::from_fn(|k| listener[k] - voice[k]);
        let length = (d[2] * d[2] + d[1] * d[1] + d[0] * d[0]).sqrt();
        if length < EPSILON {
            return 0.0;
        }
        let inverse = 1.0 / length;
        let across = [d[1] * inverse, -(inverse * d[0]), 0.0];
        let vertical = self.fan(
            cache,
            listener,
            voice,
            [0.0, 0.0, 1.0],
            10.0,
            end_vertical,
            n,
        );
        let right = self.fan(cache, listener, voice, across, 15.0, end_across, n);
        let left = self.fan(cache, listener, voice, across, -15.0, -end_across, n);
        [vertical, right, left].into_iter().fold(1.0, f32::min)
    }
}

/// Traces the placed voices' occlusion and eases each voice toward it.
pub(crate) fn trace_voice_occlusion(
    runtime: Res<AudioRuntime>,
    prediction: Option<Res<net::ClientPredictionState>>,
    time: Res<Time>,
    (mut jobs, mut gains): (
        ResMut<OcclusionJobs>,
        ResMut<crate::proximity::ProximityGains>,
    ),
) {
    let voices = runtime.occlusion_voices();
    let world = prediction
        .as_ref()
        .filter(|prediction| prediction.0.is_armed() && prediction.0.world().has_world_clip())
        .map(|prediction| prediction.0.world().content());
    let pose = runtime.listener();
    let listener = pose.map(|listener| listener.origin_inches);
    let jobs = &mut *jobs;
    jobs.caches
        .retain(|id, _| voices.iter().any(|voice| voice.id == *id));
    if let (Some(world), Some(listener)) = (&world, listener) {
        for voice in &voices {
            if let std::collections::hash_map::Entry::Vacant(slot) = jobs.caches.entry(voice.id) {
                voice
                    .occlusion
                    .set_target(first_occlusion(world, voice, listener), true);
                slot.insert(None);
            }
        }
    }
    if let Some(batch) = &mut jobs.batch
        && let Some(done) = future::block_on(future::poll_once(batch))
    {
        jobs.batch = None;
        let (done, proximity) = done;
        jobs.proximity = proximity;
        for job in done {
            if let (Some(voice), Some(cache)) = (
                voices.iter().find(|voice| voice.id == job.id),
                jobs.caches.get_mut(&job.id),
            ) {
                voice.occlusion.set_target(job.result, false);
                *cache = job.cache;
            }
        }
    }
    if jobs.batch.is_none() {
        gains.step(&jobs.proximity);
    }
    if jobs.batch.is_none()
        && let (Some(world), Some(pose)) = (world, pose)
        && !voices.is_empty()
    {
        let listener = pose.origin_inches;
        let batch: Vec<Job> = voices
            .iter()
            .map(|voice| Job {
                id: voice.id,
                listener,
                voice: voice.origin_inches,
                cache: jobs.caches.get(&voice.id).copied().flatten(),
                result: 0.0,
            })
            .collect();
        jobs.batch = Some(AsyncComputeTaskPool::get().spawn(trace_batch(
            world,
            batch,
            listener,
            pose.forward,
        )));
    }
    let seconds = time.delta_secs();
    for voice in &voices {
        voice.occlusion.ease(OCCLUSION_RATE, seconds);
    }
}

/// The worker's batch: every scheduled voice traced in turn, then the
/// walls around the listener.
async fn trace_batch(
    world: Arc<SimContent>,
    mut batch: Vec<Job>,
    listener: [f32; 3],
    forward: [f32; 3],
) -> (Vec<Job>, [f32; 4]) {
    let mut rays = Rays {
        world: &world,
        start_solid: false,
    };
    for job in &mut batch {
        job.result = rays.occlusion(&mut job.cache, 4, job.listener, job.voice);
    }
    let proximity = crate::proximity::measure(&world, listener, forward);
    (batch, proximity)
}

/// A voice's occlusion as it starts: none for an alias without an
/// occlusion level, half its level beyond the cull distance for one
/// low-passed by distance, else traced with two rays a fan.
fn first_occlusion(world: &SimContent, voice: &OcclusionVoice, listener: [f32; 3]) -> f32 {
    let level = f32::from(voice.policy.level);
    if !LOS_OCCLUSION || level * (1.0 / 255.0) < EPSILON {
        return 0.0;
    }
    let d: [f32; 3] = std::array::from_fn(|k| listener[k] - voice.origin_inches[k]);
    let distance = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if CULL_DISTANCE < distance && voice.policy.distance_lowpass {
        return level * (1.0 / 510.0);
    }
    Rays {
        world,
        start_solid: false,
    }
    .occlusion(&mut None, 2, listener, voice.origin_inches)
}
