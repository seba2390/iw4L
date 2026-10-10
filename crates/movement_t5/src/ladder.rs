//! Finding and leaving ladders. Climbing one is not run (a gap).

use crate::events::anim;
use crate::physics::{divisor, effective_stance, sqrt};
use crate::state::{ENTITYNUM_NONE, pm_flags, pm_type};
use crate::{MoveWorld, Pm, Pml, SURF_LADDER};

fn leave(ps: &mut crate::PlayerState) {
    if ps.pm_flags & pm_flags::LADDER != 0 {
        ps.pm_flags = (ps.pm_flags & !pm_flags::LADDER) | pm_flags::LADDER_FALL;
    }
}

pub(crate) fn check<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &mut Pml) {
    if pml.walking || pm.ps.water_level >= 3 {
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
