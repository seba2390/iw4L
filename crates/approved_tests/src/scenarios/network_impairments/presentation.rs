use std::sync::Arc;

use net::transport::udp_session::SnapshotPacingStats;
use net::{FrameClock, PresentationSampleOutcome, ProxySample, RemoteProxy, ServerTime};
use serde_json::{Value, json};
use sim::{ClientId, Snapshot, Tick};

mod entity;

pub(super) const REMOTE: ClientId = ClientId(2);
const SPEED: f64 = 120.0;
pub(super) const IMPAIRMENT_START_MS: u64 = 10_000;
const WARMUP_MS: u64 = 5000;
const STABLE_MS: u64 = 1000;

pub(super) fn add_motion_probes(snapshot: &mut Snapshot) {
    let at_ms = u64::from(snapshot.tick.0) * super::TICK_MS;
    let mut ps = sim::blank_player_state();
    ps.origin = [(at_ms as f64 * SPEED / 1000.0) as f32, 0.0, 64.0];
    ps.health = 100;
    snapshot.players.push((REMOTE, ps));
    entity::add_moving_entity(snapshot);
}

#[derive(Default)]
struct Measurements {
    frames: usize,
    starved_frames: usize,
    unavailable_frames: usize,
    backwards_frames: usize,
    clock_backwards_frames: usize,
    freeze_ms: u64,
    longest_freeze_ms: u64,
    freeze_run_ms: u64,
    starved_ms: u64,
    longest_starvation_ms: u64,
    starvation_run_ms: u64,
    buffer_delays: Vec<f64>,
    clock_lags: Vec<f64>,
    state_ages: Vec<f64>,
    interpolation_errors: Vec<f64>,
    world_errors: Vec<f64>,
    movement_errors: Vec<f64>,
    max_interpolated_error: f64,
    previous: Option<(u64, i32, Option<f64>)>,
}

struct Reading {
    x: Option<f64>,
    starved: bool,
    interpolation_error: f64,
    state_age: f64,
    movement_error: f64,
    backwards: bool,
}

fn distribution(values: &[f64]) -> Value {
    if values.is_empty() {
        return Value::Null;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let at = |p: usize| sorted[(sorted.len() * p).div_ceil(100).saturating_sub(1)];
    json!({"count": sorted.len(), "min": sorted[0], "p50": at(50),
        "p95": at(95), "p99": at(99), "max": sorted.last()})
}

impl Measurements {
    fn baseline_smooth(&self) -> bool {
        self.starved_frames == 0
            && self.unavailable_frames == 0
            && self.backwards_frames == 0
            && self.clock_backwards_frames == 0
            && self.freeze_ms == 0
            && self.interpolation_errors.iter().all(|e| *e <= 0.25)
            && self.movement_errors.iter().all(|e| *e <= 0.5)
    }

    fn observe(&mut self, now: u64, clock: i32, delay: i32, sample: ProxySample) -> Reading {
        let (pose, starved, at) = match sample {
            ProxySample::Pose { ps, provenance } => (Some(ps), false, provenance.effective_time.0),
            ProxySample::Starved { held, provenance } => (held, true, provenance.effective_time.0),
        };
        self.observe_point(
            now,
            clock,
            delay,
            pose.map(|ps| f64::from(ps.origin[0])),
            at,
            starved,
        )
    }

    fn observe_point(
        &mut self,
        now: u64,
        clock: i32,
        delay: i32,
        x: Option<f64>,
        at: i32,
        starved: bool,
    ) -> Reading {
        let dt = self.previous.map_or(0, |(last, _, _)| now - last);
        let dx = self
            .previous
            .and_then(|(_, _, before)| before.zip(x))
            .map(|(a, b)| b - a);
        let freeze = dx.is_some_and(|v| v.abs() < 0.0001);
        let backwards = dx.is_some_and(|v| v < -0.0001);
        let clock_backwards = self.previous.is_some_and(|(_, old, _)| clock < old);
        self.frames += 1;
        self.starved_frames += usize::from(starved);
        self.unavailable_frames += usize::from(x.is_none());
        self.backwards_frames += usize::from(backwards);
        self.clock_backwards_frames += usize::from(clock_backwards);
        self.freeze_run_ms = if freeze { self.freeze_run_ms + dt } else { 0 };
        self.freeze_ms += if freeze { dt } else { 0 };
        self.longest_freeze_ms = self.longest_freeze_ms.max(self.freeze_run_ms);
        self.starvation_run_ms = if starved {
            self.starvation_run_ms + dt
        } else {
            0
        };
        self.starved_ms += if starved { dt } else { 0 };
        self.longest_starvation_ms = self.longest_starvation_ms.max(self.starvation_run_ms);
        self.buffer_delays.push(f64::from(delay));
        self.clock_lags.push(now as f64 - f64::from(clock));
        let mut reading = Reading {
            x,
            starved,
            interpolation_error: f64::INFINITY,
            state_age: f64::INFINITY,
            movement_error: f64::INFINITY,
            backwards: backwards || clock_backwards,
        };
        if let Some(x) = x {
            reading.interpolation_error = (x - f64::from(at) * SPEED / 1000.0).abs();
            reading.state_age = now as f64 - x * 1000.0 / SPEED;
            self.interpolation_errors.push(reading.interpolation_error);
            if !starved {
                self.max_interpolated_error =
                    self.max_interpolated_error.max(reading.interpolation_error);
            }
            self.state_ages.push(reading.state_age);
            self.world_errors
                .push((x - now as f64 * SPEED / 1000.0).abs());
            if let Some(dx) = dx {
                reading.movement_error = (dx - dt as f64 * SPEED / 1000.0).abs();
                self.movement_errors.push(reading.movement_error);
            }
        }
        self.previous = Some((now, clock, x));
        reading
    }

    fn report(&self) -> Value {
        json!({"frames": self.frames, "starved_frames": self.starved_frames,
            "starved_percent": 100.0 * self.starved_frames as f64 / self.frames.max(1) as f64,
            "unavailable_frames": self.unavailable_frames,
            "position_regressions": self.backwards_frames,
            "clock_regressions": self.clock_backwards_frames,
            "max_interpolated_error_units": self.max_interpolated_error,
            "freeze_ms": self.freeze_ms, "longest_freeze_ms": self.longest_freeze_ms,
            "starved_ms": self.starved_ms, "longest_starvation_ms": self.longest_starvation_ms,
            "buffer_delay_ms": distribution(&self.buffer_delays),
            "clock_lag_ms": distribution(&self.clock_lags),
            "motion_age_ms": distribution(&self.state_ages),
            "interpolation_error_units": distribution(&self.interpolation_errors),
            "world_position_error_units": distribution(&self.world_errors),
            "movement_step_error_units": distribution(&self.movement_errors)})
    }
}

struct NormalBounds {
    error: f64,
    step: f64,
    age: f64,
    delay: f64,
}

impl NormalBounds {
    fn from_baseline(baseline: &Measurements) -> Self {
        let max = |values: &[f64]| values.iter().copied().fold(0.0, f64::max);
        let p95 = |values: &[f64]| distribution(values)["p95"].as_f64().unwrap_or(0.0);
        Self {
            error: max(&baseline.interpolation_errors) + 0.25,
            step: max(&baseline.movement_errors) + 0.5,
            age: p95(&baseline.state_ages) + 20.0,
            delay: p95(&baseline.buffer_delays) + 10.0,
        }
    }

    fn contains(&self, reading: &Reading, delay: i32) -> bool {
        !reading.starved
            && !reading.backwards
            && reading.interpolation_error <= self.error
            && reading.movement_error <= self.step
            && reading.state_age <= self.age
            && f64::from(delay) <= self.delay
    }

    fn report(&self) -> Value {
        json!({"continuous_ms": STABLE_MS, "max_reference_error_units": self.error,
            "max_movement_step_error_units": self.step, "max_motion_age_ms": self.age,
            "max_buffer_delay_ms": self.delay, "no_unavailable_starved_or_backwards_motion": true,
            "relative_bounds": "baseline maximum reference/step error +0.25/+0.5 units; p95 age/delay +20/+10 ms"})
    }
}

#[derive(Default)]
struct Recovery {
    normal_since: Option<u64>,
    first_confirmed: Option<u64>,
    unstable_after_first: usize,
    last_normal: Option<u64>,
}

impl Recovery {
    fn observe(&mut self, now: u64, restored_at: u64, normal: bool) {
        if normal {
            self.last_normal = Some(now);
            let since = *self.normal_since.get_or_insert(now);
            if now - since >= STABLE_MS && self.first_confirmed.is_none() {
                self.first_confirmed = Some(now - restored_at);
            }
        } else {
            self.normal_since = None;
            self.unstable_after_first += usize::from(self.first_confirmed.is_some());
        }
    }

    fn sustained(&self, restored_at: u64) -> Option<u64> {
        self.normal_since
            .zip(self.last_normal)
            .filter(|(since, last)| last - since >= STABLE_MS)
            .map(|(since, _)| since - restored_at)
    }

    fn report(&self, restored_at: u64, budget: u64) -> Value {
        let sustained = self.sustained(restored_at);
        json!({"first_normal_window_confirmed_after_ms": self.first_confirmed,
            "sustained_normal_from_ms": sustained, "unstable_frames_after_first_normal_window": self.unstable_after_first,
            "recovered_by_end": sustained.is_some(), "recovery_budget_ms": budget,
            "recovery_goal_met": sustained.is_some_and(|ms| ms <= budget)})
    }
}

pub(super) struct Playback {
    proxy: RemoteProxy,
    clock: FrameClock,
    pending: Vec<Arc<Snapshot>>,
    latest: Option<Tick>,
    next_frame: u64,
    last_frame: u64,
    impaired_end: u64,
    measure_end: u64,
    phases: [Measurements; 4],
    adjustments: [std::collections::BTreeMap<&'static str, u64>; 4],
    continuous: Measurements,
    recovery: Recovery,
    entity: entity::EntityPlayback,
    entity_phases: [Measurements; 4],
    entity_continuous: Measurements,
    entity_recovery: Recovery,
    record_samples: bool,
    samples: Vec<Value>,
    recovery_budget_ms: u64,
}

impl Playback {
    pub(super) fn new(
        impaired_end: u64,
        measure_end: u64,
        record_samples: bool,
        recovery_budget_ms: u64,
    ) -> Self {
        Self {
            proxy: RemoteProxy::default(),
            clock: FrameClock::default(),
            pending: Vec::new(),
            latest: None,
            next_frame: 0,
            last_frame: 0,
            impaired_end,
            measure_end,
            phases: Default::default(),
            adjustments: Default::default(),
            recovery: Recovery::default(),
            entity: entity::EntityPlayback::default(),
            entity_phases: Default::default(),
            entity_continuous: Measurements::default(),
            entity_recovery: Recovery::default(),
            continuous: Measurements::default(),
            record_samples,
            samples: Vec::new(),
            recovery_budget_ms,
        }
    }

    pub(super) fn enqueue(&mut self, snapshot: Snapshot) {
        self.pending.push(Arc::new(snapshot));
    }

    pub(super) fn advance(&mut self, now: u64, pacing: SnapshotPacingStats) {
        if now > self.measure_end || now * 60 < self.next_frame * 1000 {
            return;
        }
        self.next_frame += 1;
        let dt = now - self.last_frame;
        self.last_frame = now;
        let hint = self
            .clock
            .started()
            .then(|| self.clock.time().saturating_add(self.clock.frametime()));
        let entity_snapshot = self.pending.last().cloned();
        for snapshot in self.pending.drain(..) {
            self.latest = Some(snapshot.tick);
            self.proxy.push_arc(snapshot, hint);
        }
        if let Some(snapshot) = entity_snapshot {
            self.entity.adopt(&snapshot);
        }
        self.clock.advance_remote(
            dt as f32 / 1000.0,
            self.latest.is_some(),
            self.latest.map(ServerTime::from_tick),
        );
        let phase = self.phase(now);
        if let Some(adjustment) = self.clock.last_adjust() {
            *self.adjustments[phase].entry(adjustment).or_default() += 1;
        }
        let delay = self.proxy.delay_ms();
        let sample = self.proxy.interpolate_at(REMOTE, self.clock.time());
        let mut reading = self.phases[phase].observe(now, self.clock.time(), delay, sample);
        if now >= WARMUP_MS {
            reading = self
                .continuous
                .observe(now, self.clock.time(), delay, sample);
        }
        let entity_x = self.entity.sample(&self.clock);
        let entity_delay = net::AUTHORITY_MS;
        let entity_at = self.clock.time().saturating_sub(entity_delay);
        let mut entity_reading = self.entity_phases[phase].observe_point(
            now,
            self.clock.time(),
            entity_delay,
            entity_x,
            entity_at,
            entity_x.is_none(),
        );
        if now >= WARMUP_MS {
            entity_reading = self.entity_continuous.observe_point(
                now,
                self.clock.time(),
                entity_delay,
                entity_x,
                entity_at,
                entity_x.is_none(),
            );
        }
        let normal = NormalBounds::from_baseline(&self.phases[1]).contains(&reading, delay);
        let entity_normal = NormalBounds::from_baseline(&self.entity_phases[1])
            .contains(&entity_reading, entity_delay);
        if self.record_samples {
            let finite = |value: f64| value.is_finite().then_some(value);
            self.samples.push(json!([
                now,
                phase,
                self.clock.time(),
                delay,
                reading.x,
                reading.starved,
                finite(reading.interpolation_error),
                finite(reading.state_age),
                finite(reading.movement_error),
                (phase >= 2).then_some(normal),
                self.latest.map(|tick| tick.0),
                pacing.byte_budget_holds,
                pacing.stale_ack_holds,
                self.clock.last_adjust(),
                entity_x,
                finite(entity_reading.interpolation_error),
                finite(entity_reading.state_age),
                finite(entity_reading.movement_error),
                (phase >= 2).then_some(entity_normal)
            ]));
        }
        if phase != 3 {
            return;
        }
        self.recovery.observe(now, self.impaired_end, normal);
        self.entity_recovery
            .observe(now, self.impaired_end, entity_normal);
    }

    pub(super) fn phase(&self, now: u64) -> usize {
        if now < WARMUP_MS {
            0
        } else if now < IMPAIRMENT_START_MS {
            1
        } else if now < self.impaired_end {
            2
        } else {
            3
        }
    }

    pub(super) fn valid(&self) -> bool {
        let baseline = &self.phases[1];
        let entity_baseline = &self.entity_phases[1];
        self.phases
            .iter()
            .all(|p| p.clock_backwards_frames == 0 && p.max_interpolated_error < 0.01)
            && baseline.frames == 300
            && baseline.unavailable_frames == 0
            && baseline.starved_frames <= 15
            && baseline.longest_freeze_ms <= 100
            && baseline.freeze_ms <= 50
            && baseline.backwards_frames == 0
            && distribution(&baseline.state_ages)["p95"]
                .as_f64()
                .is_some_and(|age| age <= 200.0)
            && entity_baseline.frames == 300
            && entity_baseline.unavailable_frames == 0
            && distribution(&entity_baseline.state_ages)["p95"]
                .as_f64()
                .is_some_and(|age| age <= 200.0)
    }

    pub(super) fn report(&self) -> Value {
        let sustained = self.recovery.sustained(self.impaired_end);
        let mut report = json!({"sample_hz": 60, "trajectory_units_per_second": SPEED,
            "trajectory": "one-dimensional constant velocity, independent of command ACKs",
            "continuous_after_warmup": self.continuous.report(),
            "phases": {
                "warmup": {"start_ms": 0, "end_ms": WARMUP_MS, "metrics": self.phases[0].report()},
                "baseline": {"start_ms": WARMUP_MS, "end_ms": IMPAIRMENT_START_MS, "metrics": self.phases[1].report()},
                "impairment": {"start_ms": IMPAIRMENT_START_MS, "end_ms": self.impaired_end, "metrics": self.phases[2].report()},
                "recovery": {"start_ms": self.impaired_end, "end_ms": self.measure_end, "metrics": self.phases[3].report()}},
            "normal_definition": NormalBounds::from_baseline(&self.phases[1]).report(),
            "baseline_smoothness_goal_met": self.phases[1].baseline_smooth(),
            "first_normal_window_confirmed_after_ms": self.recovery.first_confirmed,
            "sustained_normal_from_ms": sustained,
            "unstable_frames_after_first_normal_window": self.recovery.unstable_after_first,
            "recovered_by_end": sustained.is_some(), "oracle_passed": self.valid(),
            "recovery_budget_ms": self.recovery_budget_ms,
            "recovery_goal_met": sustained.is_some_and(|ms| ms <= self.recovery_budget_ms)});
        for (name, adjustments) in ["warmup", "baseline", "impairment", "recovery"]
            .into_iter()
            .zip(&self.adjustments)
        {
            report["phases"][name]["clock_adjustments"] = json!(adjustments);
        }
        if self.record_samples {
            report["frame_sample_columns"] = json!([
                "wall_ms",
                "phase_index",
                "clock_ms",
                "buffer_delay_ms",
                "position_x",
                "starved",
                "interpolation_error_units",
                "motion_age_ms",
                "movement_step_error_units",
                "within_normal_bounds",
                "newest_tick",
                "byte_budget_holds_total",
                "stale_ack_holds_total",
                "clock_adjustment",
                "entity_position_x",
                "entity_nominal_view_error_units",
                "entity_motion_age_ms",
                "entity_movement_step_error_units",
                "entity_within_normal_bounds"
            ]);
            report["frame_samples"] = json!(self.samples);
        }
        let mut entity_report = self
            .entity_recovery
            .report(self.impaired_end, self.recovery_budget_ms);
        entity_report["continuous_after_warmup"] = entity::metrics(&self.entity_continuous);
        for (index, name) in ["warmup", "baseline", "impairment", "recovery"]
            .into_iter()
            .enumerate()
        {
            entity_report["phases"][name] = json!({"metrics": entity::metrics(&self.entity_phases[index]),
                "start_ms": report["phases"][name]["start_ms"], "end_ms": report["phases"][name]["end_ms"]});
        }
        entity_report["normal_definition"] =
            NormalBounds::from_baseline(&self.entity_phases[1]).report();
        entity_report["baseline_smoothness_goal_met"] =
            self.entity_phases[1].baseline_smooth().into();
        entity_report["coverage"] = "production script-mover/actor pose sampler; nominal view reference is clock minus one authority tick".into();
        let entity_sustained = self.entity_recovery.sustained(self.impaired_end);
        let combined = sustained.zip(entity_sustained).map(|(a, b)| a.max(b));
        let mut combined_report = json!({"sample_hz": 60, "remote_player": report, "entity": entity_report,
            "recovered_by_end": combined.is_some(), "sustained_normal_from_ms": combined,
            "recovery_goal_met": combined.is_some_and(|ms| ms <= self.recovery_budget_ms),
            "recovery_budget_ms": self.recovery_budget_ms, "oracle_passed": self.valid(),
            "baseline_smoothness_goal_met": self.phases[1].baseline_smooth() && self.entity_phases[1].baseline_smooth(),
            "baseline_smoothness_definition": {"no_holds_or_backwards_motion": true,
                "max_reference_error_units": 0.25, "max_movement_step_error_units": 0.5,
                "scope": "all clean baseline samples; separate from recovery to baseline behavior"},
            "longest_freeze_ms": self.continuous.longest_freeze_ms.max(self.entity_continuous.longest_freeze_ms)});
        for key in ["frame_samples", "frame_sample_columns"] {
            if let Some(value) = combined_report["remote_player"]
                .as_object_mut()
                .and_then(|p| p.remove(key))
            {
                combined_report[key] = value;
            }
        }
        combined_report
    }
}

pub(super) fn calibrate() -> Result<(), String> {
    entity::calibrate()?;
    let mut proxy = RemoteProxy::default();
    for (tick, x) in [(0, 0.0), (2, 12.0)] {
        let mut snapshot = Snapshot::unpublished(Tick(tick));
        let mut player = sim::blank_player_state();
        player.origin = [x, 0.0, 64.0];
        snapshot.players.push((REMOTE, player));
        proxy.push(&snapshot);
    }
    let mut measured = Measurements::default();
    for (now, expected_x) in [(150, 6.0), (160, 7.2)] {
        let sample = proxy.interpolate_at(REMOTE, now);
        let ProxySample::Pose { ps, provenance } = sample else {
            return Err("calibration: known interpolation starved".into());
        };
        if (ps.origin[0] - expected_x).abs() > 0.001
            || !matches!(
                provenance.outcome,
                PresentationSampleOutcome::Interpolated { .. }
            )
        {
            return Err("calibration: known midpoint changed".into());
        }
        measured.observe(now as u64, now, 100, sample);
    }
    for now in [350, 400] {
        measured.observe(now as u64, now, 100, proxy.interpolate_at(REMOTE, now));
    }
    if measured.starved_frames != 2
        || measured.longest_freeze_ms != 50
        || (measured
            .interpolation_errors
            .iter()
            .copied()
            .fold(0.0, f64::max)
            - 24.0)
            .abs()
            > 0.001
    {
        return Err("calibration: metric collector missed known held-motion error".into());
    }
    Ok(())
}
