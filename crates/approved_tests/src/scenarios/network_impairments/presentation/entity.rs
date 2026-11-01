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
    let mut runtime = EntityPlayback::default();
    for tick in [1, 2] {
        let mut snapshot = Snapshot::unpublished(Tick(tick));
        add_moving_entity(&mut snapshot);
        if tick == 1 {
            reset_entity(&mut runtime.runtime, snapshot.meta.entities[0], 50, true);
        }
        runtime.adopt(&snapshot);
    }
    let mut clock = FrameClock::default();
    for (at, expected) in [(125, 9.0), (133, 9.96), (300, 12.0)] {
        clock.assign_server_time(at);
        if !runtime
            .sample(&clock)
            .is_some_and(|x| (x - expected).abs() < 0.001)
        {
            return Err("calibration: entity pose interpolation/hold changed".into());
        }
    }
    Ok(())
}
