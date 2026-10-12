//! The test player (`IW4L_TEST_PLAYER=1`): drives the first player who is
//! not a bot, for headless runs. It aims at the nearest living actor it can
//! see, fires (pressing and letting go each tick, so a semi-automatic gun
//! fires again), reloads an empty clip and backs away from an actor that is
//! close. Every five seconds it logs the round, its score, kills, health,
//! the actors alive, its weapon and ammo, and what its aim lines ended on.
//! It only presses buttons; the game decides the rest.

use bevy::prelude::*;
use net::{AuthorityClock, AuthorityWorld, ClientCommandInbox, look_angles_from_degrees};
use sim::script::ScriptOwner;
use sim::{BulletTraceQuery, ClientLifecycle, ColliderId, MASK_SHOT, Tick, TraceOutcome};

use crate::motor::{UserCmd, buttons};
use crate::roster::BotRoster;

/// The height above an actor's origin it aims at: its chest.
const AIM_HEIGHT: f32 = 48.0;
/// A shot line that ends this near the aim point reached the actor.
const HIT_SLACK: f32 = 32.0;
/// How many of the nearest actors it tries to see each tick.
const SIGHT_CANDIDATES: usize = 4;
/// It backs away from an actor nearer than this.
const BACK_OFF: f32 = 90.0;
const LOG_PERIOD_MS: i32 = 5000;

#[derive(Resource, Default)]
pub(crate) struct TestPlayer {
    enabled: bool,
    pressed: bool,
    next_log_ms: i32,
    shots: u32,
    /// What the sight lines to targets ended on: nothing, a model, the world.
    lines: [u32; 3],
}

impl TestPlayer {
    pub(crate) fn from_env() -> Self {
        Self {
            enabled: std::env::var("IW4L_TEST_PLAYER").is_ok_and(|value| value == "1"),
            ..Self::default()
        }
    }
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f32>().sqrt()
}

fn flat_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// View angles (pitch down positive, yaw) from `from` toward `to`.
fn look(from: [f32; 3], to: [f32; 3]) -> [f32; 3] {
    let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let flat = (d[0] * d[0] + d[1] * d[1]).sqrt();
    [
        -d[2].atan2(flat).to_degrees(),
        d[1].atan2(d[0]).to_degrees(),
        0.0,
    ]
}

pub(crate) fn drive_test_player(
    mut test: ResMut<TestPlayer>,
    clock: Res<AuthorityClock>,
    world: Res<AuthorityWorld>,
    roster: Res<BotRoster>,
    mut cmds: ResMut<ClientCommandInbox>,
) {
    if !test.enabled {
        return;
    }
    let snapshot = world.0.snapshot(Tick(clock.tick.saturating_sub(1)));
    let Some((id, ps)) = snapshot
        .players
        .iter()
        .find(|(id, _)| roster.bots.iter().all(|bot| bot.id != *id))
        .map(|(id, ps)| (*id, *ps))
    else {
        return;
    };
    let Some(meta) = snapshot.meta.for_client(id) else {
        return;
    };
    let actors = world.0.living_actors();
    if clock.time_ms >= test.next_log_ms {
        test.next_log_ms = clock.time_ms + LOG_PERIOD_MS;
        let weapon = world.0.weapon_script_name(ps.weapon).to_owned();
        diag::info!(
            Sim,
            "test player: t={}s round={} score={} kills={} health={} actors={} shots={} weapon={weapon} clip={} stock={} state={} lines(clear/model/world)={:?}",
            clock.time_ms / 1000,
            world
                .0
                .script_int(ScriptOwner::Level, "round_number")
                .map_or_else(|| "-".to_owned(), |round| round.to_string()),
            meta.score,
            meta.kills,
            ps.health,
            actors.len(),
            test.shots,
            meta.ammo_clip,
            meta.ammo_stock,
            ps.weaponstate_primary,
            test.lines
        );
    }
    if meta.lifecycle != ClientLifecycle::Alive {
        return;
    }

    let eye = [
        ps.origin[0],
        ps.origin[1],
        ps.origin[2] + ps.view_height_current,
    ];
    let mut near: Vec<_> = actors
        .iter()
        .map(|actor| (distance(eye, actor.origin), actor.origin))
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    let target = near.iter().take(SIGHT_CANDIDATES).find_map(|&(_, origin)| {
        let aim = [origin[0], origin[1], origin[2] + AIM_HEIGHT];
        let outcome = world.0.sensor_trace(BulletTraceQuery {
            start: eye,
            end: aim,
            mask: MASK_SHOT,
            ignore: Some(id),
            ignore_hit: None,
            ignore_model: None,
        });
        let seen = match outcome {
            TraceOutcome::Miss { .. } => true,
            TraceOutcome::Hit { end, .. } => distance(end, aim) <= HIT_SLACK,
            _ => false,
        };
        seen.then_some((aim, outcome))
    });

    if let Some((_, outcome)) = target {
        let line = match outcome {
            TraceOutcome::Hit {
                collider: ColliderId::EntityDObjBone { .. },
                ..
            } => 1,
            TraceOutcome::Hit { .. } => 2,
            _ => 0,
        };
        test.lines[line] += 1;
    }
    let target = target.map(|(aim, _)| aim);
    let view = target.map_or(ps.viewangles, |aim| look(eye, aim));
    let mut cmd = UserCmd {
        server_time: clock.time_ms,
        angles: look_angles_from_degrees(std::array::from_fn(|axis| {
            view[axis] - ps.delta_angles[axis]
        })),
        weapon: ps.weapon as u16,
        weapon_mapped: ps.weapon as u16,
        ..UserCmd::default()
    };
    test.pressed = !test.pressed;
    if test.pressed {
        if meta.ammo_clip == 0 {
            cmd.buttons |= buttons::RELOAD;
        } else if target.is_some() {
            cmd.buttons |= buttons::ATTACK;
            test.shots += 1;
        }
    }
    if near
        .first()
        .is_some_and(|&(_, origin)| flat_distance(ps.origin, origin) < BACK_OFF)
    {
        cmd.forwardmove = -127;
    }
    cmds.push(id, None, cmd, None);
}
