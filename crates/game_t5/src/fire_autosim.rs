//! Black Ops' client automatic fire simulation (`snd_autoSim`): a shot it
//! takes starts (or refreshes) a simulation for its shooter, weapon and tag,
//! which plays the weapon's fire sound at once and then every fire time
//! while shots keep coming, and the shooter, when a player, keeps firing.

use game_api::fire::{AutoFireShot, FireAutoSim};

/// The simulations a client runs at once.
const SIMS: usize = 64;
/// The shots it holds queued to play.
const QUEUED: usize = 64;
/// A simulation no shot refreshed for longer than this stops (msec).
const MAX_AGE_MS: i32 = 300;
/// Of two shots of one weapon within this angle of each other from the
/// view, only the nearer plays (`snd_autosim_window`, on).
const WINDOW_RADIANS: f32 = 0.5235988;
/// The firing flag a player's movement sets.
const FIRING: u32 = 0x40;

#[derive(Clone, Copy)]
struct Sim {
    shot: AutoFireShot,
    age: i32,
    timer: i32,
    interval: i32,
    count: i32,
    fire_now: bool,
}

pub(crate) struct AutoSim {
    sims: [Option<Sim>; SIMS],
    /// Shots queued to play, stamped with the frame that queued them.
    queued: [Option<(u32, AutoFireShot)>; QUEUED],
    frame: u32,
}

pub(crate) fn new() -> Box<dyn FireAutoSim> {
    Box::new(AutoSim {
        sims: [None; SIMS],
        queued: [None; QUEUED],
        frame: 0,
    })
}

impl FireAutoSim for AutoSim {
    /// A shot of a running simulation refreshes it (its age back to 0, the
    /// shot's place and flags taken); else it takes the first free slot,
    /// due to play at once and then every fire time (`snd_auto_random` is
    /// 0, so the interval is the fire time). With every slot taken the game
    /// prints "ran out of automatic sims" and plays the shot at once.
    fn queue(&mut self, shot: AutoFireShot) -> Option<AutoFireShot> {
        let mut free = None;
        for (index, sim) in self.sims.iter_mut().enumerate() {
            match sim {
                None => {
                    free.get_or_insert(index);
                }
                Some(sim)
                    if sim.shot.entity == shot.entity
                        && sim.shot.weapon == shot.weapon
                        && sim.shot.tag == shot.tag =>
                {
                    sim.age = 0;
                    sim.shot = shot;
                    return None;
                }
                Some(_) => {}
            }
        }
        match free {
            Some(index) => {
                self.sims[index] = Some(Sim {
                    shot,
                    age: 0,
                    timer: shot.fire.fire_time_ms,
                    interval: shot.fire.fire_time_ms,
                    count: 0,
                    fire_now: true,
                });
                None
            }
            None => {
                diag::info!(Audio, "WARNING: ran out of automatic sims???");
                Some(shot)
            }
        }
    }

    /// Each simulation ages by the frame and stops past 300 msec, or when
    /// its shooter is a player no longer firing (unless the shot was its
    /// last); a burst weapon holds after three shots. Otherwise its timer
    /// runs, and when it reaches the interval (or the simulation is new) a
    /// shot is queued. Then, with the window on, of two queued shots of one
    /// weapon the nearer silences the farther when they lie within 30
    /// degrees from the view; the shots queued the frame before play.
    fn update(
        &mut self,
        frame_ms: i32,
        view_origin: [f32; 3],
        entity_flags: &dyn Fn(i32) -> u32,
    ) -> Vec<AutoFireShot> {
        self.frame = self.frame.wrapping_add(1);
        for slot in 0..SIMS {
            let Some(mut sim) = self.sims[slot] else {
                continue;
            };
            sim.age = sim.age.wrapping_add(frame_ms);
            let stopped = sim.age as u32 > MAX_AGE_MS as u32
                || sim.shot.player
                    && entity_flags(sim.shot.entity) & FIRING == 0
                    && !sim.shot.last_shot;
            let keep = if stopped {
                false
            } else if sim.shot.fire.burst && sim.count > 2 {
                if sim.count > 3 {
                    sim.timer = 0;
                    sim.count = 0;
                    false
                } else {
                    true
                }
            } else {
                if !sim.fire_now {
                    sim.timer = sim.timer.wrapping_add(frame_ms);
                }
                if sim.interval as u32 <= sim.timer as u32 || sim.fire_now {
                    sim.timer = 0;
                    self.push(sim.shot);
                    sim.count += 1;
                    sim.fire_now = false;
                }
                true
            };
            self.sims[slot] = keep.then_some(sim);
        }
        self.window(view_origin);
        let due = self.frame.wrapping_sub(1);
        let mut out = Vec::new();
        for entry in &mut self.queued {
            if let Some((stamp, shot)) = *entry
                && stamp == due
            {
                out.push(shot);
                *entry = None;
            }
        }
        out
    }
}

impl AutoSim {
    /// Queues a shot to play on the next frame; with the queue full it is
    /// dropped.
    fn push(&mut self, shot: AutoFireShot) {
        if let Some(entry) = self.queued.iter_mut().find(|entry| entry.is_none()) {
            *entry = Some((self.frame, shot));
        }
    }

    fn window(&mut self, view: [f32; 3]) {
        for i in 0..QUEUED {
            let Some((_, near)) = self.queued[i] else {
                continue;
            };
            for j in 0..QUEUED {
                let Some((_, far)) = self.queued[j] else {
                    continue;
                };
                if i == j || far.weapon != near.weapon {
                    continue;
                }
                let a = sub(near.origin, view);
                let b = sub(far.origin, view);
                let near_length = (a[1] * a[1] + a[2] * a[2] + a[0] * a[0]).sqrt();
                let far_length = (b[2] * b[2] + b[1] * b[1] + b[0] * b[0]).sqrt();
                if !(near_length < far_length) {
                    continue;
                }
                let inverse = |length: f32| 1.0 / if 0.0 <= -length { 1.0 } else { length };
                let a_scale = inverse((a[2] * a[2] + a[1] * a[1] + a[0] * a[0]).sqrt());
                let b_scale = inverse((b[2] * b[2] + b[1] * b[1] + b[0] * b[0]).sqrt());
                let dot = b[2] * b_scale * a[2] * a_scale
                    + b[1] * b_scale * a[1] * a_scale
                    + b_scale * b[0] * a_scale * a[0];
                if (f64::from(dot).acos() as f32) < WINDOW_RADIANS {
                    self.queued[j] = None;
                    break;
                }
            }
        }
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
