use net::client::presentation::centity_runtime::{apply_existing, reset_entity};
use net::{CEntityRuntime, FrameClock, PresentedSnapshot};
use render_anim::occupancy::script_model::present_script_mover_pose;
use serde_json::Value;
use sim::{Snapshot, Tick};

const ENTITY: i32 = 700;

pub(in super::super) fn add_moving_entity(snapshot: &mut Snapshot) {
    let time = u64::from(snapshot.tick.0) * super::super::TICK_MS;
    let mut mover = sim::ScriptMoverGentity {
        state: sim::init_script_mover_state(ENTITY, [0.0; 3], [0.0; 3]),
        ..Default::default()
    };
    for at in [time as i32 - net::AUTHORITY_MS, time as i32] {
        mover.pose(
            at,
            [(f64::from(at) * super::SPEED / 1000.0) as f32, 0.0, 64.0],
            [0.0; 3],
        );
    }
    snapshot.meta.entities.push(mover.state);
}

#[derive(Default)]
pub(super) struct EntityPlayback {
    runtime: CEntityRuntime,
    presented: PresentedSnapshot,
}

impl EntityPlayback {
    pub(super) fn adopt(&mut self, snapshot: &Snapshot) {
        if let Some(state) = snapshot.meta.entities.iter().find(|es| es.number == ENTITY) {
            apply_existing(
                &mut self.runtime,
                *state,
                snapshot.tick.0 as i32 * net::AUTHORITY_MS,
            );
            self.presented.publish_decoded(snapshot.clone());
        }
    }

    pub(super) fn sample(&self, clock: &FrameClock) -> Option<f64> {
        present_script_mover_pose(&self.runtime, &self.presented, Some(clock), None)
            .map(|(origin, _)| f64::from(origin[0]))
    }
}

pub(super) fn metrics(measurements: &super::Measurements) -> Value {
    let mut out = measurements.report();
    let fields = out.as_object_mut().expect("measurement object");
    for name in [
        "starved_frames",
        "starved_percent",
        "starved_ms",
        "longest_starvation_ms",
        "max_interpolated_error_units",
    ] {
        fields.remove(name);
    }
    if let Some(error) = fields.remove("interpolation_error_units") {
        fields.insert("nominal_view_error_units".into(), error);
    }
    fields.remove("buffer_delay_ms");
    fields.insert("nominal_view_delay_ms".into(), net::AUTHORITY_MS.into());
    out
}

pub(super) fn calibrate() -> Result<(), String> {
    let mut fresh = EntityPlayback::default();
    let mut snapshot = Snapshot::unpublished(Tick(2));
    add_moving_entity(&mut snapshot);
    fresh.adopt(&snapshot);
    let mut clock = FrameClock::default();
    clock.assign_server_time(300);
    if !fresh
        .sample(&clock)
        .is_some_and(|x| (x - 12.0).abs() < 0.001)
    {
        return Err("calibration: new entity extrapolated without pose history".into());
    }
    let mut runtime = EntityPlayback::default();
    for tick in [1, 2] {
        let mut snapshot = Snapshot::unpublished(Tick(tick));
        add_moving_entity(&mut snapshot);
        if tick == 1 {
            reset_entity(&mut runtime.runtime, snapshot.meta.entities[0], 50, true);
        }
        runtime.adopt(&snapshot);
    }
    for (at, expected) in [(125, 9.0), (133, 9.96), (175, 15.0), (300, 18.0)] {
        clock.assign_server_time(at);
        if !runtime
            .sample(&clock)
            .is_some_and(|x| (x - expected).abs() < 0.001)
        {
            return Err("calibration: entity pose interpolation/hold changed".into());
        }
    }
    let mut snapshot = Snapshot::unpublished(Tick(4));
    add_moving_entity(&mut snapshot);
    runtime.adopt(&snapshot);
    clock.assign_server_time(225);
    if !runtime
        .sample(&clock)
        .is_some_and(|x| (x - 21.0).abs() < 0.001)
    {
        return Err("calibration: sparse entity updates changed the view delay".into());
    }
    let mut mover = sim::ScriptMoverGentity {
        state: sim::init_script_mover_state(ENTITY, [0.0; 3], [0.0; 3]),
        ..Default::default()
    };
    for (at, x) in [(150, 18.0), (200, 24.0), (250, 24.0)] {
        mover.pose(at, [x, 0.0, 64.0], [0.0; 3]);
    }
    let mut stopped = Snapshot::unpublished(Tick(5));
    stopped.meta.entities.push(mover.state);
    runtime.adopt(&stopped);
    clock.assign_server_time(400);
    if !runtime
        .sample(&clock)
        .is_some_and(|x| (x - 24.0).abs() < 0.001)
    {
        return Err("calibration: stopped entity continued moving".into());
    }
    mover.pose(300, [224.0, 0.0, 64.0], [0.0; 3]);
    let mut teleported = Snapshot::unpublished(Tick(6));
    teleported.meta.entities.push(mover.state);
    runtime.adopt(&teleported);
    clock.assign_server_time(317);
    if !runtime
        .sample(&clock)
        .is_some_and(|x| (x - 224.0).abs() < 0.001)
    {
        return Err("calibration: entity teleport blended across epochs".into());
    }
    Ok(())
}
