//! Finding, climbing and leaving ladders.

use crate::events::anim;
use crate::math::{angle_delta, vectoyaw};
use crate::moves::{air_move, check_jump};
use crate::physics::{accelerate, cmd_scale_air, divisor, effective_stance, sqrt};
use crate::slide::step_slide_move;
use crate::state::{ENTITYNUM_NONE, pm_flags, pm_type};
use crate::{MoveWorld, Pm, Pml, SURF_LADDER, perks, tuning};

fn leave(ps: &mut crate::PlayerState) {
    if ps.pm_flags & pm_flags::LADDER != 0 {
        ps.pm_flags = (ps.pm_flags & !pm_flags::LADDER) | pm_flags::LADDER_FALL;
    }
}

pub(crate) fn check<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &mut Pml) {
    if pml.walking || pm.ps.water_level >= 4 {
        pm.ps.pm_flags &= !pm_flags::LADDER_FALL;
    }
    let flags = pm.ps.pm_flags;
    if pm.ps.pm_time != 0
        && flags & pm_flags::LADDER == 0
        && flags & (pm_flags::TIME_HARDLANDING | pm_flags::TIME_KNOCKBACK) != 0
    {
        return;
    }
    let reach = if pml.walking { 8.0 } else { 30.0 };
    let on_ladder = flags & pm_flags::LADDER != 0;
    let hanging = on_ladder && pm.ps.ground_entity_num == ENTITYNUM_NONE;
    let dir = if hanging {
        let l = pm.ps.ladder_vec;
        [-l[0], -l[1], -l[2]]
    } else {
        let f = pml.forward;
        let n = 1.0 / divisor(sqrt(f[1] * f[1] + f[0] * f[0]));
        [n * f[0], n * f[1], n * 0.0]
    };
    if pm.ps.pm_type >= pm_type::DEAD {
        pm.ps.ground_entity_num = ENTITYNUM_NONE;
        pml.ground_plane = false;
        pml.almost_ground_plane = false;
        pml.walking = false;
        leave(pm.ps);
        return;
    }
    if flags & pm_flags::LADDER_FALL != 0
        || effective_stance(pm.ps) == 1
        || pm.cmd.server_time - pm.ps.jump_time < 300
    {
        leave(pm.ps);
        return;
    }
    let mins = [pm.mins[0] + 6.0, pm.mins[1] + 6.0, 8.0];
    let maxs = [
        pm.maxs[0] - 6.0,
        pm.maxs[1] - 6.0,
        if 8.0 > pm.maxs[2] { 8.0 } else { pm.maxs[2] },
    ];
    let o = pm.ps.origin;
    let end = [
        dir[0] * reach + o[0],
        dir[1] * reach + o[1],
        dir[2] * reach + o[2],
    ];
    let trace = pm.trace(o, end, mins, maxs, pm.tracemask);
    if 1.0 > trace.fraction
        && trace.surface_flags & SURF_LADDER != 0
        && (!pml.walking || pm.cmd.forwardmove > 0)
    {
        if on_ladder {
            pm.ps.pm_flags |= pm_flags::LADDER;
            return;
        }
        pm.ps.ladder_vec = trace.normal;
        let l = pm.ps.ladder_vec;
        let end = [
            -l[0] * reach + o[0],
            -l[1] * reach + o[1],
            -l[2] * reach + o[2],
        ];
        let trace = pm.trace(o, end, mins, maxs, pm.tracemask);
        if 1.0 > trace.fraction && trace.surface_flags & SURF_LADDER != 0 {
            pm.ps.pm_flags |= pm_flags::LADDER;
            return;
        }
    }
    leave(pm.ps);
    if hanging {
        pm.anim_event(anim::JUMP, false, true);
    }
}

pub(crate) fn ladder_move<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &mut Pml) {
    if check_jump(pm, pml) {
        air_move(pm, pml);
        return;
    }
    let mut climb = ((pml.forward[2] + 0.25) * 2.5).clamp(-1.0, 1.0);
    pml.forward[2] = 0.0;
    let n = 1.0
        / divisor(sqrt(
            pml.forward[0] * pml.forward[0] + pml.forward[1] * pml.forward[1],
        ));
    pml.forward = [pml.forward[0] * n, pml.forward[1] * n, n * 0.0];

    pml.right[2] = 0.0;
    let r = pml.right;
    let n = 1.0 / divisor(sqrt(r[0] * r[0] + r[1] * r[1] + r[2] * r[2]));
    let r = [r[0] * n, r[1] * n, r[2] * n];
    let l = pm.ps.ladder_vec;
    let dot = -(l[0] * r[0] + l[1] * r[1] + l[2] * r[2]);
    pml.right = [l[0] * dot + r[0], l[1] * dot + r[1], l[2] * dot + r[2]];

    let scale = cmd_scale_air(pm.ps, &pm.cmd);
    if perks::has(pm.ps.perks, perks::FAST_LADDER_CLIMB) {
        climb *= tuning::PERK_FAST_LADDER_CLIMB_MULTIPLIER;
    }
    let mut wish_x = 0.0;
    let mut wish_y = 0.0;
    let mut wish_z = 0.0;
    if pm.cmd.forwardmove != 0 {
        wish_z = climb * 0.5 * scale * f32::from(pm.cmd.forwardmove);
    }
    if pm.cmd.rightmove != 0 {
        let s = scale * 0.2 * f32::from(pm.cmd.rightmove);
        wish_x = s * pml.right[0];
        wish_y = s * pml.right[1];
        wish_z += s * pml.right[2];
    }
    let wishspeed = sqrt(wish_x * wish_x + wish_y * wish_y + wish_z * wish_z);
    let n = 1.0 / divisor(wishspeed);
    let wishdir = [wish_x * n, wish_y * n, wish_z * n];
    accelerate(pm.ps, pml, wishdir, wishspeed, 9.0);

    if pm.cmd.forwardmove == 0 {
        let drop = pm.ps.gravity as f32 * pml.frametime;
        if pm.ps.velocity[2] <= 0.0 {
            let v = pm.ps.velocity[2] + drop;
            pm.ps.velocity[2] = if v < 0.0 { v } else { 0.0 };
        } else {
            let v = pm.ps.velocity[2] - drop;
            pm.ps.velocity[2] = if v > 0.0 { v } else { 0.0 };
        }
    }
    if pm.cmd.rightmove == 0 {
        let rx = pml.right[0];
        let ry = pml.right[1];
        let n = 1.0 / divisor(sqrt(rx * rx + ry * ry));
        let rx = rx * n;
        let ry = ry * n;
        let lateral = pm.ps.velocity[0] * rx + pm.ps.velocity[1] * ry;
        if lateral != 0.0 {
            pm.ps.velocity[0] -= rx * lateral;
            pm.ps.velocity[1] -= ry * lateral;
            let mut decay = pml.frametime * lateral * 16.0;
            if libm::fabsf(decay) < libm::fabsf(lateral) {
                if libm::fabsf(decay) < 1.0 {
                    decay = libm::copysignf(1.0, decay);
                }
                pm.ps.velocity[0] += rx * (lateral - decay);
                pm.ps.velocity[1] += ry * (lateral - decay);
            }
        }
    }
    if !pml.walking {
        let l = pm.ps.ladder_vec;
        let into = -(l[0] * pm.ps.velocity[0] + l[1] * pm.ps.velocity[1]);
        pm.ps.velocity[0] += l[0] * into;
        pm.ps.velocity[1] += l[1] * into;
        let vx = pm.ps.velocity[0];
        let vy = pm.ps.velocity[1];
        let vz = pm.ps.velocity[2];
        if (vx != 0.0 || vy != 0.0 || vz != 0.0) && vx * vx + vy * vy <= vz * vz {
            pm.ps.velocity[0] -= l[0] * 50.0;
            pm.ps.velocity[1] -= l[1] * 50.0;
        }
    }
    step_slide_move(pm, pml, false);
    let yaw = vectoyaw(pm.ps.ladder_vec) + 180.0;
    let mut dir = angle_delta(yaw, pm.ps.viewangles[1]) as i32;
    if dir.abs() > 75 {
        dir = if dir > 0 { 75 } else { -75 };
    }
    pm.ps.movement_dir = i32::from(dir as i8);
}
