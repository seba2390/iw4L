//! Black Ops 2's client automatic fire sound simulation (`snd_autoSim`):
//! a shot by a player this client does not view in first person, by a
//! turret, or in a killcam is queued here and replayed at the weapon's fire
//! rate while the shooter keeps firing.

use core::f32::consts::TAU;
use game_api::fire::{AutoFireShot, FireAutoSim};

const SIMS: usize = 64;
const QUEUED: usize = 64;
const MAX_AGE_MS: i32 = 200;
const WINDOW_RADIANS: f32 = 0.523_598_8;
const FIRING: u32 = 0x40;

pub(crate) fn new() -> Box<dyn FireAutoSim> {
    Box::new(AutoSim::default())
}

#[derive(Clone, Copy)]
struct ActiveSim {
    shot: AutoFireShot,
    timer_ms: i32,
    shots_left: i32,
}

#[derive(Clone, Copy)]
struct QueuedShot {
    shot: AutoFireShot,
    delay_ms: i32,
    age_ms: i32,
}

pub(crate) struct AutoSim {
    sims: [Option<ActiveSim>; SIMS],
    queued: [Option<QueuedShot>; QUEUED],
    active: usize,
    angle: f32,
}

impl Default for AutoSim {
    fn default() -> Self {
        Self {
            sims: [None; SIMS],
            queued: [None; QUEUED],
            active: 0,
            angle: 0.0,
        }
    }
}

impl FireAutoSim for AutoSim {
    fn queue(&mut self, shot: AutoFireShot) -> Option<AutoFireShot> {
        if let Some(Some(sim)) = self
            .sims
            .iter_mut()
            .find(|s| s.is_some_and(|s| same_emitter(&s.shot, &shot)))
        {
            let was_last = sim.shot.last_shot;
            sim.shot = shot;
            sim.shot.last_shot |= was_last;
            return None;
        }
        if let Some(Some(slot)) = self
            .queued
            .iter_mut()
            .find(|q| q.is_some_and(|q| same_emitter(&q.shot, &shot)))
        {
            slot.shot = shot;
            slot.age_ms = 0;
            return None;
        }
        let Some(slot) = self.queued.iter_mut().find(|q| q.is_none()) else {
            return Some(shot);
        };
        let dx = shot.origin[0];
        let dy = shot.origin[1];
        let mut angle = dy.atan2(dx) - self.angle;
        if angle < 0.0 {
            angle += TAU;
        }
        let fire_time = shot.fire.fire_time_ms.max(0) as f32;
        let delay_ms = if angle < WINDOW_RADIANS {
            0
        } else {
            ((angle / TAU) * fire_time) as i32
        };
        *slot = Some(QueuedShot {
            shot,
            delay_ms,
            age_ms: 0,
        });
        None
    }

    fn update(
        &mut self,
        frame_ms: i32,
        view_origin: [f32; 3],
        entity_flags: &dyn Fn(i32) -> u32,
    ) -> Vec<AutoFireShot> {
        let mut played = Vec::new();
        self.angle = (self.angle + (frame_ms as f32 * 0.001) * TAU) % TAU;

        if self.active > 0 {
            for slot in &mut self.sims {
                let Some(mut sim) = *slot else {
                    continue;
                };
                let firing = !sim.shot.player
                    || sim.shot.fire.burst
                    || entity_flags(sim.shot.entity) & FIRING != 0;
                if !firing {
                    if sim.shot.last_shot {
                        played.push(sim.shot);
                    }
                    *slot = None;
                    self.active -= 1;
                    continue;
                }
                sim.timer_ms -= frame_ms;
                if sim.timer_ms > 0 {
                    *slot = Some(sim);
                    continue;
                }
                let mut shot = sim.shot;
                let done = if sim.shot.fire.burst {
                    sim.shots_left -= 1;
                    sim.shots_left <= 0
                } else {
                    !sim.shot.player || sim.shot.last_shot
                };
                if !done {
                    shot.last_shot = false;
                }
                played.push(shot);
                if done {
                    *slot = None;
                    self.active -= 1;
                } else {
                    sim.timer_ms += sim.shot.fire.fire_time_ms;
                    *slot = Some(sim);
                }
            }
        }

        for slot in &mut self.queued {
            let Some(mut q) = *slot else {
                continue;
            };
            if q.age_ms > MAX_AGE_MS {
                *slot = None;
                continue;
            }
            q.age_ms += frame_ms;
            q.delay_ms -= frame_ms;
            let dx = q.shot.origin[0] - view_origin[0];
            let dy = q.shot.origin[1] - view_origin[1];
            let mut angle = dy.atan2(dx) - self.angle;
            if angle < 0.0 {
                angle += TAU;
            }
            if q.delay_ms > 0 && angle >= WINDOW_RADIANS {
                *slot = Some(q);
                continue;
            }
            let Some(sim_slot) = self.sims.iter_mut().find(|s| s.is_none()) else {
                *slot = Some(q);
                break;
            };
            let shots_left = if q.shot.fire.burst { 3 } else { -1 };
            let mut first = q.shot;
            if !q.shot.fire.burst && q.shot.player && !q.shot.last_shot {
                first.last_shot = false;
            } else if q.shot.fire.burst {
                first.last_shot = false;
            }
            played.push(first);
            let done = if q.shot.fire.burst {
                false
            } else {
                !q.shot.player || q.shot.last_shot
            };
            if !done {
                *sim_slot = Some(ActiveSim {
                    shot: q.shot,
                    timer_ms: q.shot.fire.fire_time_ms,
                    shots_left: if q.shot.fire.burst { 2 } else { shots_left },
                });
                self.active += 1;
            }
            *slot = None;
        }

        played
    }
}

fn same_emitter(a: &AutoFireShot, b: &AutoFireShot) -> bool {
    a.entity == b.entity && a.weapon == b.weapon && a.tag == b.tag
}
