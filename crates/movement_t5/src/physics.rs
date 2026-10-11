use crate::state::{pm_flags, pm_type, view_height};
use crate::{MoveWorld, PlayerState, Pm, Pml, SURF_SLICK, UserCmd, tuning};

pub(crate) fn sqrt(v: f32) -> f32 {
    libm::sqrtf(v)
}

/// The length a normalize divides by: 1 when the vector has no length.
pub(crate) fn divisor(len: f32) -> f32 {
    if -len >= 0.0 { 1.0 } else { len }
}

/// Which stance the view height target says the player is in: 0 standing,
/// 1 prone, 2 crouched. A downed player counts by how deep they are in water.
pub fn effective_stance(ps: &PlayerState) -> i32 {
    match ps.view_height_target {
        view_height::DOWNED => {
            if ps.water_level >= 2 {
                0
            } else {
                i32::from(ps.water_level != 0) + 1
            }
        }
        view_height::CROUCH => 2,
        target => i32::from(target == view_height::PRONE),
    }
}

pub(crate) fn friction<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let v = pm.ps.velocity;
    let vz = if pml.walking { 0.0 } else { v[2] };
    let speed = sqrt(v[0] * v[0] + v[1] * v[1] + vz * vz);
    if speed < 1.0 {
        pm.ps.velocity = [0.0; 3];
        return;
    }
    let flags = pm.ps.pm_flags;
    let mut drop = 0.0_f32;
    let slick = pml.ground_trace.surface_flags & SURF_SLICK != 0;
    if flags & pm_flags::LAUNCHED != 0 {
        drop = speed / (pm.ps.launch_time as f32 * 0.00100000005) * pml.frametime;
    } else if pm.ps.water_level <= 1 {
        if pml.walking && !slick && flags & pm_flags::TIME_KNOCKBACK == 0 {
            let mut control = if tuning::STOPSPEED > speed {
                tuning::STOPSPEED
            } else {
                speed
            };
            if flags & pm_flags::TIME_HARDLANDING != 0 {
                control *= 0.300000012;
            } else if flags & pm_flags::JUMPING != 0 {
                control *= jump_slowdown_friction(pm.ps);
            }
            if pm.ps.pm_flags & pm_flags::DIVING != 0 {
                control *= crate::dive::friction_scale(pm);
            }
            drop = (f64::from(tuning::FRICTION) * f64::from(pml.frametime) * f64::from(control))
                as f32;
        }
        if slick {
            drop += tuning::SLIDING_FRICTION * pml.frametime * speed;
        }
    }
    let ps = &mut *pm.ps;
    if ps.water_level != 0 {
        const WATER_FRICTION: [f32; 6] = [0.0, 1.0, 2.0, 3.0, 3.0, 3.0];
        drop += WATER_FRICTION[ps.water_level as usize] * pml.frametime * speed;
    }
    if ps.pm_type == pm_type::SPECTATOR {
        drop += pml.frametime * speed * 5.0;
    }
    let mut newspeed = speed - drop;
    if 0.0 > newspeed {
        newspeed = 0.0;
    }
    newspeed /= speed;
    ps.velocity[0] *= newspeed;
    ps.velocity[1] *= newspeed;
    ps.velocity[2] *= newspeed;
}

/// The friction factor while the landing slowdown after a jump runs.
fn jump_slowdown_friction(ps: &mut PlayerState) -> f32 {
    if ps.pm_time > 1800 {
        ps.pm_flags &= !pm_flags::JUMPING;
        ps.jump_origin_z = 0.0;
        return 1.0;
    }
    if !tuning::JUMP_SLOWDOWN_ENABLE {
        return 1.0;
    }
    if ps.pm_time >= 1700 {
        2.5
    } else {
        ps.pm_time as f32 * 0.000882352935 + 1.0
    }
}

pub(crate) fn accelerate(
    ps: &mut PlayerState,
    pml: &Pml,
    wishdir: [f32; 3],
    wishspeed: f32,
    accel: f32,
) {
    if ps.pm_flags & pm_flags::LADDER == 0 {
        let currentspeed =
            ps.velocity[2] * wishdir[2] + ps.velocity[1] * wishdir[1] + ps.velocity[0] * wishdir[0];
        let addspeed = wishspeed - currentspeed;
        if 0.0 >= addspeed {
            return;
        }
        let control = if tuning::STOPSPEED > wishspeed {
            tuning::STOPSPEED
        } else {
            wishspeed
        };
        let mut accelspeed = pml.frametime * control * accel;
        if accelspeed > addspeed {
            accelspeed = addspeed;
        }
        ps.velocity[0] += wishdir[0] * accelspeed;
        ps.velocity[1] += accelspeed * wishdir[1];
        ps.velocity[2] += wishdir[2] * accelspeed;
    } else {
        let d = [
            wishdir[0] * wishspeed - ps.velocity[0],
            wishdir[1] * wishspeed - ps.velocity[1],
            wishdir[2] * wishspeed - ps.velocity[2],
        ];
        let len = sqrt(d[2] * d[2] + d[1] * d[1] + d[0] * d[0]);
        let n = 1.0 / divisor(len);
        let mut accelspeed = pml.frametime * wishspeed * accel;
        if accelspeed > len {
            accelspeed = len;
        }
        ps.velocity[0] += n * d[0] * accelspeed;
        ps.velocity[1] += d[1] * n * accelspeed;
        ps.velocity[2] += d[2] * n * accelspeed;
    }
}

/// On a slick surface the player is pushed down its slope.
pub(crate) fn slick_accelerate(ps: &mut PlayerState, pml: &Pml) {
    if pml.ground_trace.surface_flags & SURF_SLICK == 0 {
        return;
    }
    let nx = pml.ground_trace.normal[0];
    let ny = pml.ground_trace.normal[1];
    let n = 1.0 / divisor(sqrt(ny * ny + nx * nx));
    let dir = [n * nx, ny * n, n * 0.0];
    accelerate(ps, pml, dir, tuning::SLIDING_WISHSPEED, 2.0);
    let cap = tuning::SLIDING_VELOCITY_CAP;
    let v = ps.velocity;
    let speed = sqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
    if speed > cap * cap {
        let n = 1.0 / divisor(sqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]));
        ps.velocity = [v[0] * n * cap, v[1] * n * cap, v[2] * n * cap];
    }
}

/// Scales `v` onto the ground plane `normal`, keeping its speed (unless the
/// slope would speed it up while it climbs).
pub(crate) fn project_onto_ground(v: &mut [f32; 3], normal: [f32; 3]) {
    let lenxy = v[0] * v[0] + v[1] * v[1];
    if 0.001 > libm::fabsf(normal[2]) || lenxy == 0.0 {
        return;
    }
    let newz = -((normal[1] * v[1] + normal[0] * v[0]) / normal[2]);
    let scale = sqrt((v[2] * v[2] + lenxy) / (newz * newz + lenxy));
    if 1.0 > scale || 0.0 > newz || v[2] > 0.0 {
        *v = [v[0] * scale, v[1] * scale, scale * newz];
    }
}

/// Removes the part of the velocity going into the ground, slightly over.
pub(crate) fn clip_into_ground(ps: &mut PlayerState, normal: [f32; 3]) {
    let mut backoff =
        normal[1] * ps.velocity[1] + ps.velocity[2] * normal[2] + ps.velocity[0] * normal[0];
    backoff = -(backoff - libm::fabsf(backoff) * 0.001);
    ps.velocity[0] += normal[0] * backoff;
    ps.velocity[1] += normal[1] * backoff;
    ps.velocity[2] += backoff * normal[2];
}

/// How much of the player's speed a stance keeps; a stance change blends
/// over 400 ms.
pub(crate) fn stance_speed_scale(ps: &PlayerState, server_time: i32) -> f32 {
    const PRONE: f32 = 0.15;
    const CROUCH: f32 = 0.65;
    if ps.pm_flags & pm_flags::DIVING != 0 {
        return 1.0;
    }
    let lerp = |from: f32, to: f32| -> Option<f32> {
        let mut frac = (server_time - ps.view_height_lerp_time) as f32 * 0.0025;
        if 0.0 > frac {
            return None;
        }
        if frac > 1.0 {
            frac = 1.0;
        } else if frac == 0.0 {
            return None;
        }
        Some((1.0 - frac) * from + frac * to)
    };
    if ps.view_height_lerp_time != 0
        && ps.view_height_lerp_target == view_height::PRONE
        && let Some(scale) = lerp(CROUCH, PRONE)
    {
        return scale;
    }
    if ps.view_height_lerp_time != 0
        && ps.view_height_lerp_target == view_height::CROUCH
        && ps.view_height_lerp_down == 0
        && let Some(scale) = lerp(PRONE, CROUCH)
    {
        return scale;
    }
    match effective_stance(ps) {
        1 => PRONE,
        2 => CROUCH,
        _ => 1.0,
    }
}

fn prone_ads(ps: &PlayerState) -> bool {
    ps.pm_flags & pm_flags::PRONE != 0 && ps.weapon_pos_frac > 0.0
}

/// The fraction of the player's speed a walking command asks for.
pub(crate) fn cmd_scale_walk<W: MoveWorld>(pm: &mut Pm<'_, W>, cmd: &UserCmd) -> f32 {
    let ps = &*pm.ps;
    let prone_ads = prone_ads(ps);
    let fmove = i32::from(cmd.forwardmove);
    let rmove = i32::from(cmd.rightmove);
    let total = sqrt((fmove * fmove + rmove * rmove) as f32);
    let mut forward = fmove as f32;
    if cmd.forwardmove < 0 {
        forward *= tuning::BACK_SPEED_SCALE;
    }
    let side = libm::fabsf(rmove as f32 * tuning::STRAFE_SPEED_SCALE);
    let forward = libm::fabsf(forward);
    let max = if forward - side >= 0.0 { forward } else { side };
    if max == 0.0 {
        return 0.0;
    }
    let mut scale = ps.speed as f32 * max / (total * 127.0);
    if ps.pm_flags & pm_flags::SIGHT_AIMING != 0 || ps.leanf != 0.0 || prone_ads {
        scale *= 0.4;
    }
    if ps.pm_flags & pm_flags::SPRINTING != 0 && ps.view_height_target == view_height::STAND {
        scale *= tuning::SPRINT_SPEED_SCALE;
    }
    match ps.pm_type {
        pm_type::NOCLIP => scale *= 3.0,
        pm_type::UFO => scale *= 6.0,
        _ => {
            scale *= stance_speed_scale(ps, cmd.server_time);
            if ps.water_level >= 1 {
                scale *= 1.0 - ps.water_level as f32 * 0.333333343 * 0.75;
            }
        }
    }
    if ps.perks & crate::perks::ENDURANCE != 0 {
        scale *= tuning::ENDURANCE_SPEED_SCALE;
    }
    if ps.weapon != 0 {
        let weapon = pm.world.weapon(ps.weapon);
        let (move_scale, ads_scale) = (
            weapon.def.move_speed_scale(),
            weapon.def.ads_move_speed_scale(),
        );
        if move_scale > 0.0 && ps.pm_flags & pm_flags::SIGHT_AIMING == 0 && !prone_ads {
            scale *= weapon_move_speed_scale(ps, pm.zombiemode, move_scale);
        } else if ads_scale > 0.0 {
            scale *= ads_scale;
        }
    }
    let multiplier = ps.move_speed_scale_multiplier;
    if ps.pm_flags & pm_flags::SHELLSHOCKED != 0 {
        pm.gap(crate::Gap::Shellshock);
    }
    multiplier * scale
}

/// In zombies a sprinting player with a sprint perk moves at least 1.1 times
/// as fast.
pub(crate) fn weapon_move_speed_scale(ps: &PlayerState, zombiemode: bool, scale: f32) -> f32 {
    if zombiemode
        && ps.pm_flags & pm_flags::SPRINTING != 0
        && ps.perks & crate::perks::SPRINT_SPEED != 0
        && scale - 1.1 < 0.0
    {
        return 1.1;
    }
    scale
}

/// The fraction of the player's speed an air (or swim) command asks for.
pub(crate) fn cmd_scale_air(ps: &PlayerState, cmd: &UserCmd) -> f32 {
    let fmove = i32::from(cmd.forwardmove);
    let rmove = i32::from(cmd.rightmove);
    let total = sqrt((fmove * fmove + rmove * rmove) as f32);
    let max = fmove.abs().max(rmove.abs());
    if max == 0 {
        return 0.0;
    }
    let mut scale = ps.speed as f32 * max as f32 / (total * 127.0);
    if ps.pm_flags & pm_flags::SIGHT_AIMING != 0
        || ps.leanf != 0.0
        || ps.pm_flags & pm_flags::DIVING != 0
    {
        scale *= 0.4;
    }
    match ps.pm_type {
        pm_type::NOCLIP => scale *= 3.0,
        pm_type::UFO => scale *= 6.0,
        pm_type::SPECTATOR => scale *= tuning::SPECTATE_SPEED_SCALE,
        _ => {}
    }
    scale
}
