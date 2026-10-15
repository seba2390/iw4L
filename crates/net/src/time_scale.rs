use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use frame::RuntimeRole;

use crate::client::runtime::LastAdoptedSnapshot;
use crate::{AuthorityClock, AuthorityWorld, FrameClock, GameActive};

#[derive(SystemParam)]
pub(crate) struct TimeScaleInputs<'w> {
    authority: Option<Res<'w, AuthorityWorld>>,
    authority_clock: Option<Res<'w, AuthorityClock>>,
    adopted: Option<Res<'w, LastAdoptedSnapshot>>,
    clock: Option<Res<'w, FrameClock>>,
    active: Option<Res<'w, GameActive>>,
}

/// `IW4L_TIMESCALE=<factor>`: a headless server runs its game clock that
/// many times faster than real time (from 1 to 64), so long scripted runs
/// finish sooner; ticks, script waits and console `wait Ns` all count game
/// time.
pub fn headless_time_scale() -> f64 {
    static SCALE: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *SCALE.get_or_init(|| {
        std::env::var("IW4L_TIMESCALE")
            .ok()
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .map_or(1.0, |v| v.clamp(1.0, 64.0))
    })
}

pub(crate) fn apply_time_scale(
    role: Res<RuntimeRole>,
    headless: Option<Res<frame::Headless>>,
    inputs: TimeScaleInputs,
    fixed: Res<Time<Fixed>>,
    mut time: ResMut<Time<Virtual>>,
    mut pace: Local<Option<(std::time::Instant, f64)>>,
) {
    let TimeScaleInputs {
        authority,
        authority_clock,
        adopted,
        clock,
        active,
    } = inputs;
    let (plan, game_ms) = if role.runs_authority() {
        (
            authority.as_ref().and_then(|world| world.0.slow_motion()),
            authority_clock
                .as_ref()
                .map_or(0.0, |clock| f64::from(clock.time_ms))
                + fixed.overstep().as_secs_f64() * 1000.0,
        )
    } else {
        let snapshot = adopted
            .as_ref()
            .and_then(|a| a.next().or(a.snap.as_deref()))
            .filter(|_| active.as_ref().is_none_or(|active| active.0));
        let game_ms = clock.as_ref().filter(|clock| clock.started()).map_or_else(
            || snapshot.map_or(0.0, |s| f64::from(s.tick.0) * f64::from(sim::MATCH_TICK_MS)),
            |clock| f64::from(clock.time()),
        );
        (
            snapshot.and_then(|s| s.meta.objectives.slow_motion),
            game_ms,
        )
    };
    let mut speed = plan
        .filter(|p| p.valid())
        .map_or(1.0, |p| f64::from(p.sample(game_ms)));
    if headless.is_some() && role.runs_authority() {
        let scale = headless_time_scale();
        if scale != 1.0 {
            speed *= scale;
            // One real frame may now cover many ticks.
            let max = std::time::Duration::from_secs_f64(0.25 * scale);
            if time.max_delta() != max {
                time.set_max_delta(max);
                diag::info!(
                    Net,
                    "time scale: headless game clock asked to run {scale}x real time"
                );
            }
            // What the machine keeps up with, every minute of game time.
            let now = std::time::Instant::now();
            match *pace {
                Some((wall, game)) if game_ms - game >= 60_000.0 => {
                    let ratio = (game_ms - game) / 1000.0 / now.duration_since(wall).as_secs_f64();
                    diag::info!(Net, "time scale: game clock ran {ratio:.1}x real time");
                    *pace = Some((now, game_ms));
                }
                Some((_, game)) if game_ms >= game => {}
                _ => *pace = Some((now, game_ms)),
            }
        }
    }
    time.set_relative_speed_f64(speed);
}
