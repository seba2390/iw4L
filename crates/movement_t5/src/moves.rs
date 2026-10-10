use crate::events::{anim, event};
use crate::math::{angle_delta, angle_normalize_180, vectoyaw};
use crate::physics::{
    accelerate, clip_into_ground, cmd_scale_air, cmd_scale_walk, divisor, effective_stance,
    friction, project_onto_ground, slick_accelerate, sqrt,
};
use crate::slide::step_slide_move;
use crate::state::{ENTITYNUM_NONE, buttons, e_flags, pm_flags};
use crate::{MoveWorld, Pm, Pml, SURF_SLICK, tuning};

pub(crate) fn air_move<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &mut Pml) {
    if pm.ps.pm_flags & pm_flags::DIVING != 0 {
        crate::dive::air_control(pm);
    }
    friction(pm, pml);
    let fmove = f32::from(pm.cmd.forwardmove);
    let smove = f32::from(pm.cmd.rightmove);
    let cmd = pm.cmd;
    let scale = cmd_scale_air(pm.ps, &cmd);
    pml.forward[2] = 0.0;
    pml.right[2] = 0.0;
    let n = 1.0
        / divisor(sqrt(
            pml.forward[0] * pml.forward[0] + pml.forward[1] * pml.forward[1],
        ));
    pml.forward = [pml.forward[0] * n, pml.forward[1] * n, n * 0.0];
    let r = pml.right;
    let n = 1.0 / divisor(sqrt(r[0] * r[0] + r[1] * r[1] + r[2] * r[2]));
    pml.right = [r[0] * n, r[1] * n, r[2] * n];
    let wish_x = pml.right[0] * smove + pml.forward[0] * fmove;
    let wish_y = pml.right[1] * smove + fmove * pml.forward[1];
    let wishspeed = sqrt(wish_y * wish_y + wish_x * wish_x);
    let n = 1.0 / divisor(wishspeed);
    let wishdir = [n * wish_x, wish_y * n, n * 0.0];
    accelerate(pm.ps, pml, wishdir, wishspeed * scale, 1.0);
    slick_accelerate(pm.ps, pml);
    if pml.ground_plane {
        clip_into_ground(pm.ps, pml.ground_trace.normal);
    }
    step_slide_move(pm, pml, true);
    set_movement_dir(pm, pml);
}

pub(crate) fn walk_move<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &mut Pml) {
    if pm.ps.pm_flags & pm_flags::JUMPING != 0 {
        landing_slowdown(pm.ps);
    }
    if pm.ps.pm_flags & pm_flags::SPRINTING != 0 {
        let strafe = f64::from(tuning::SPRINT_STRAFE_SPEED_SCALE) * f64::from(pm.cmd.rightmove);
        pm.cmd.rightmove = strafe as i32 as i8;
    }
    let may_jump =
        pm.ps.ground_entity_num == ENTITYNUM_NONE || pm.world.can_stand_on(pm.ps.ground_entity_num);
    if (may_jump && check_jump(pm, pml)) || crate::dive::check_start(pm, pml) {
        air_move(pm, pml);
        return;
    }
    friction(pm, pml);
    let fmove = f32::from(pm.cmd.forwardmove);
    let smove = f32::from(pm.cmd.rightmove);
    let cmd = pm.cmd;
    let scale = cmd_scale_walk(pm, &cmd);
    let timer = pm.ps.damage_timer;
    let mut damage_scale = 1.0_f32;
    if timer != 0 && tuning::DMGTIMER_MAX_TIME != 0.0 {
        damage_scale =
            -1.0 / tuning::DMGTIMER_MAX_TIME * tuning::DMGTIMER_MIN_SCALE * timer as f32 + 1.0;
    }
    let scale = damage_scale * scale;
    pm.ps.damage_timer = timer - (pml.frametime * 1000.0) as i32;
    if pm.ps.damage_timer <= 0 {
        pm.ps.damage_timer = 0;
    }
    pml.forward[2] = 0.0;
    pml.right[2] = 0.0;
    let f = pml.forward;
    let n = 1.0 / divisor(sqrt(f[0] * f[0] + f[1] * f[1]));
    pml.forward[0] = f[0] * n;
    pml.forward[1] = f[1] * n;
    let r = pml.right;
    let n = 1.0 / divisor(sqrt(r[0] * r[0] + r[1] * r[1]));
    pml.right[0] = r[0] * n;
    pml.right[1] = r[1] * n;
    let wish_x = pml.right[0] * smove + pml.forward[0] * fmove;
    let wish_y = pml.right[1] * smove + pml.forward[1] * fmove;
    let wishspeed = sqrt(wish_x * wish_x + wish_y * wish_y);
    let n = 1.0 / divisor(wishspeed);
    let mut wishdir = [n * wish_x, n * wish_y, n * 0.0];
    let wishspeed = wishspeed * scale;
    project_onto_ground(&mut wishdir, pml.ground_trace.normal);

    let slick = pml.ground_trace.surface_flags & SURF_SLICK != 0;
    let mut accel = if slick {
        2.0
    } else if pm.ps.pm_flags & pm_flags::TIME_KNOCKBACK != 0 {
        1.0
    } else {
        match effective_stance(pm.ps) {
            1 => 19.0,
            2 => 12.0,
            _ => 9.0,
        }
    };
    if pm.ps.pm_flags & pm_flags::TIME_HARDLANDING != 0 {
        accel *= 0.25;
    }
    accelerate(pm.ps, pml, wishdir, wishspeed, accel);
    slick_accelerate(pm.ps, pml);
    if slick || pm.ps.pm_flags & pm_flags::TIME_KNOCKBACK != 0 {
        pm.ps.velocity[2] -= pm.ps.gravity as f32 * pml.frametime;
    }
    project_onto_ground(&mut pm.ps.velocity, pml.ground_trace.normal);
    step_slide_move(pm, pml, false);
    set_movement_dir(pm, pml);
}

/// Black Ops slows the first steps after landing a jump.
fn landing_slowdown(ps: &mut crate::PlayerState) {
    let scale;
    if ps.pm_time > 1800 {
        ps.pm_flags &= !pm_flags::JUMPING;
        ps.jump_origin_z = 0.0;
        scale = 0.649999976;
    } else if ps.pm_time == 0 {
        if ps.jump_origin_z + 18.0 > ps.origin[2] {
            scale = 0.649999976;
            ps.pm_time = 1800;
        } else {
            scale = 0.5;
            ps.pm_time = 1200;
        }
    } else {
        scale = 1.0;
    }
    if tuning::JUMP_SLOWDOWN_ENABLE {
        ps.velocity[0] *= scale;
        ps.velocity[1] *= scale;
        ps.velocity[2] *= scale;
    }
}

fn check_jump<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &mut Pml) -> bool {
    let ps = &*pm.ps;
    if ps.pm_flags & pm_flags::NO_JUMP != 0
        || pm.cmd.server_time - ps.jump_time < 500
        || ps.pm_flags & pm_flags::RESPAWNED != 0
        || ps.pm_flags & pm_flags::MANTLE != 0
        || ps.pm_type >= crate::state::pm_type::DEAD
    {
        return false;
    }
    if effective_stance(ps) != 0 && ps.ground_entity_num != ENTITYNUM_NONE {
        return false;
    }
    if !pm.cmd.buttons.held(buttons::JUMP) {
        return false;
    }
    if pm.oldcmd.buttons.held(buttons::JUMP) {
        pm.cmd.buttons.release(buttons::JUMP);
        return false;
    }
    if pml.ground_trace.surface_flags & SURF_SLICK != 0 {
        return false;
    }
    jump_launch(pm, pml, tuning::JUMP_HEIGHT);
    jump_sound(pm, pml);
    if pm.ps.pm_flags & pm_flags::LADDER != 0 {
        ladder_jump(pm.ps, pml);
    }
    pm.anim_event(anim::JUMP, false, true);
    true
}

fn jump_launch<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &mut Pml, height: f32) {
    let ps = &mut *pm.ps;
    let mut v2 = ps.gravity as f32 * (height * 2.0);
    if ps.pm_flags & pm_flags::JUMPING != 0 && ps.pm_time <= 1800 {
        let factor = if !tuning::JUMP_SLOWDOWN_ENABLE {
            1.0
        } else if ps.pm_time >= 1700 {
            2.5
        } else {
            ps.pm_time as f32 * 0.000882352935 + 1.0
        };
        v2 /= factor;
    }
    pml.ground_plane = false;
    pml.walking = false;
    ps.jump_origin_z = ps.origin[2];
    ps.ground_entity_num = ENTITYNUM_NONE;
    ps.jump_time = pm.cmd.server_time;
    ps.pm_flags = (ps.pm_flags & !(pm_flags::TIME_HARDLANDING | pm_flags::TIME_KNOCKBACK))
        | pm_flags::JUMPING;
    ps.pm_time = 0;
    ps.sprint_button_up_required = 0;
    ps.velocity[2] = sqrt(v2);
    ps.aim_spread_scale += tuning::JUMP_SPREAD_ADD;
    if ps.aim_spread_scale > 255.0 {
        ps.aim_spread_scale = 255.0;
    }
}

fn jump_sound<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    if pm.ps.pm_flags & pm_flags::LADDER != 0 {
        pm.event(event::JUMP, 21);
        return;
    }
    let surface = pm.surface_sound(pml);
    if surface != 0 {
        pm.event(event::JUMP, surface);
    }
}

/// Jumping off a ladder pushes the player away from it.
fn ladder_jump(ps: &mut crate::PlayerState, pml: &Pml) {
    ps.velocity[2] *= 0.75;
    let f = pml.forward;
    let n = 1.0 / divisor(sqrt(f[1] * f[1] + f[0] * f[0]));
    let fy = f[1] * n;
    let fz = n * 0.0;
    let fx = n * f[0];
    let l = ps.ladder_vec;
    let mut push_x = fx;
    let mut push_y = fy;
    if 0.0 > l[2] * f[2] + l[1] * f[1] + l[0] * f[0] {
        let d = (l[1] * fy + l[2] * fz + l[0] * fx) * -2.0;
        let x = l[0] * d + fx;
        let y = l[1] * d + fy;
        let z = l[2] * d + fz;
        let n = 1.0 / divisor(sqrt(z * z + y * y + x * x));
        push_x = x * n;
        push_y = y * n;
    }
    ps.velocity[0] = tuning::JUMP_LADDER_PUSH_VEL * push_x;
    ps.velocity[1] = push_y * tuning::JUMP_LADDER_PUSH_VEL;
    ps.pm_flags &= !pm_flags::LADDER;
}

fn clamp_dir(d: i32) -> i32 {
    if d.abs() > 90 {
        if d <= 0 { -90 } else { 90 }
    } else {
        d
    }
}

/// Which way the legs move relative to the view, for the player's animations.
pub(crate) fn set_movement_dir<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let ps = &*pm.ps;

    let dir = if ps.pm_flags & pm_flags::PRONE != 0 && ps.e_flags & e_flags::TURRET == 0 {
        clamp_dir(angle_delta(ps.prone_direction, ps.viewangles[1]) as i32)
    } else if ps.pm_flags & pm_flags::LADDER != 0 {
        let yaw = vectoyaw(ps.ladder_vec) + 180.0;
        clamp_dir(angle_delta(yaw, ps.viewangles[1]) as i32)
    } else {
        let d = [
            ps.origin[0] - pml.previous_origin[0],
            ps.origin[1] - pml.previous_origin[1],
            ps.origin[2] - pml.previous_origin[2],
        ];
        let moving = pm.cmd.forwardmove != 0 || pm.cmd.rightmove != 0;
        let len = sqrt(d[2] * d[2] + d[0] * d[0] + d[1] * d[1]);
        if !moving
            || ps.ground_entity_num == ENTITYNUM_NONE
            || len == 0.0
            || len <= pml.frametime * 5.0
            || tuning::ZOMBIETRON
        {
            pm.ps.movement_dir = 0;
            return;
        }
        let mut dir_n = d;
        crate::math::normalize(&mut dir_n);
        let yaw = vectoyaw(dir_n);
        let mut d = angle_delta(yaw, ps.viewangles[1]) as i32;
        if pm.cmd.forwardmove < 0 {
            d = angle_normalize_180(d as f32 + 180.0) as i32;
        }
        clamp_dir(d)
    };
    pm.ps.movement_dir = i32::from(dir as i8);
}
