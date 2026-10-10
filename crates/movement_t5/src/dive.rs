//! Dive to prone: a sprinting player who goes prone launches forward and
//! lands sliding on their front.

use crate::events::{anim, event};
use crate::physics::sqrt;
use crate::state::{ENTITYNUM_NONE, buttons, pm_flags};
use crate::{MoveWorld, PlayerState, Pm, Pml, tuning};

/// The sound class of each surface type for dive starts and landings.
const SURFACE_CLASS: [i32; 31] = [
    0, 3, 2, 4, 4, 2, 5, 2, 7, 5, 7, 6, 4, 1, 8, 2, 2, 2, 5, 4, 8, 3, 2, 2, 2, 2, 5, 5, 1, 5, 7,
];

fn surface_class(surface: i32) -> i32 {
    SURFACE_CLASS.get(surface as usize).copied().unwrap_or(0)
}

/// Whether the dive is past its launch: the player has landed and slides.
pub(crate) fn in_flight(ps: &PlayerState) -> bool {
    tuning::DTP && ps.pm_flags & pm_flags::DIVING != 0 && ps.jump_time < 0
}

fn can_start<W: MoveWorld>(pm: &Pm<'_, W>) -> bool {
    let ps = &*pm.ps;
    if !tuning::DTP || pm.world.weapon(ps.weapon).blocks_prone {
        return false;
    }
    if ps.last_sprint_end != pm.cmd.server_time || !pm.cmd.buttons.held(buttons::DIVE) {
        return false;
    }
    let v = ps.velocity;
    tuning::DTP_MIN_SPEED * tuning::DTP_MIN_SPEED < v[0] * v[0] + v[1] * v[1]
}

/// Starts a dive or lands one; true while the player is diving.
pub(crate) fn check_start<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &mut Pml) -> bool {
    if !tuning::DTP {
        return false;
    }
    if pm.ps.pm_flags & pm_flags::DIVING != 0 {
        if pm.ps.jump_time > 0 {
            let surface = pm.surface_sound(pml);
            pm.event(event::DIVE_LAND, surface_class(surface));
            pm.anim_event(anim::DIVE_LAND, true, true);
            pm.ps.jump_time = -pm.cmd.server_time;
        }
        return true;
    }
    if !can_start(pm) {
        return false;
    }
    let now = pm.cmd.server_time;
    let since_last = now - pm.ps.dive_end_time;
    if since_last as f32 <= tuning::DTP_EXHAUSTION_WINDOW && since_last >= 0 {
        return false;
    }
    if (now - pm.ps.last_sprint_start) as f32 <= tuning::DTP_STARTUP_DELAY {
        return false;
    }
    launch(pm, pml);
    let surface = pm.surface_sound(pml);
    pm.event(event::DIVE_START, surface_class(surface));
    true
}

fn launch<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &mut Pml) {
    let ps = &mut *pm.ps;
    let v2 = ((f64::from(tuning::JUMP_HEIGHT) + f64::from(tuning::JUMP_HEIGHT))
        * f64::from(ps.gravity)) as f32;
    pml.ground_plane = false;
    pml.walking = false;
    ps.ground_entity_num = ENTITYNUM_NONE;
    ps.jump_time = pm.cmd.server_time;
    ps.velocity[2] = sqrt(v2);
    if tuning::DTP_NEW_TRAJECTORY {
        ps.velocity[2] *= tuning::DTP_NEW_TRAJECTORY_MULTIPLIER;
    }
    ps.pm_flags |= pm_flags::DIVING;
    pm.anim_event(anim::DIVE, false, true);
    let ps = &mut *pm.ps;
    ps.jump_origin_z = ps.origin[2];
    ps.pm_flags &= !(pm_flags::TIME_HARDLANDING | pm_flags::TIME_KNOCKBACK | pm_flags::JUMPING);
    ps.pm_time = 0;
}

fn end(ps: &mut PlayerState, server_time: i32) {
    ps.pm_flags &= !pm_flags::DIVING;
    ps.jump_time = 0;
    ps.velocity = [0.0; 3];
    ps.dive_end_time = server_time;
    ps.sprint_button_up_required = 0;
}

/// A sliding dive that has slowed down ends.
pub(crate) fn check_end<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let ps = &*pm.ps;
    if !(tuning::DTP && ps.pm_flags & pm_flags::DIVING != 0 && ps.jump_time < 0) {
        return;
    }
    let v = ps.velocity;
    if v[0] * v[0] + v[1] * v[1] + v[2] * v[2] < tuning::DTP_MIN_SPEED * tuning::DTP_MIN_SPEED {
        end(pm.ps, pm.cmd.server_time);
    }
}

/// At the top of the dive the player hangs for a moment.
pub(crate) fn air_control<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    if !tuning::DTP_NEW_TRAJECTORY {
        return;
    }
    let ps = &mut *pm.ps;
    if f64::from(ps.origin[2]) < f64::from(tuning::JUMP_HEIGHT) + f64::from(ps.jump_origin_z) {
        return;
    }
    if pm.cmd.server_time - ps.jump_time < tuning::DTP_MAX_APEX_DURATION {
        ps.velocity[2] = 0.0;
        return;
    }
    if -ps.velocity[2] < 0.0 {
        ps.velocity[2] = 0.0;
    }
}

/// How much ground friction a landed dive gets: none while it slides over
/// a slide surface, then it ramps in until the dive ends.
pub(crate) fn friction_scale<W: MoveWorld>(pm: &mut Pm<'_, W>) -> f32 {
    if pm.ps.jump_time >= 0 {
        return 1.0;
    }
    let sliding = pm.ps.jump_time + pm.cmd.server_time;
    let o = pm.ps.origin;
    let trace = pm.trace_plain(
        [o[0], o[1], o[2] + 1.0],
        [o[0], o[1], o[2] - 1.0],
        pm.mins,
        pm.maxs,
        tuning::DTP_SLIDE_CONTENTS,
    );
    if 1.0 > trace.fraction
        && tuning::DTP_MAX_SLIDE_ADDITION + tuning::DTP_MAX_SLIDE_DURATION > sliding as f32
    {
        return 0.0;
    }
    if tuning::DTP_MAX_SLIDE_DURATION > sliding as f32 {
        return sliding as f32 / tuning::DTP_MAX_SLIDE_DURATION;
    }
    end(pm.ps, pm.cmd.server_time);
    1.0
}

/// Fall damage after a dive, in percent of health.
pub(crate) fn fall_damage_percent(fall: f32) -> i32 {
    let min = tuning::DTP_FALL_DAMAGE_MIN_HEIGHT;
    let max = tuning::DTP_FALL_DAMAGE_MAX_HEIGHT;
    if min >= fall {
        return 0;
    }
    if fall >= max {
        return 100;
    }
    let damage = ((fall - min) / (max - min) * 100.0) as i32;
    damage.clamp(0, 100)
}

/// A dive into water stops at once.
pub(crate) fn abort<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let ps = &mut *pm.ps;
    ps.pm_flags &= !pm_flags::DIVING;
    ps.jump_time = 0;
    ps.velocity = [0.0; 3];
    ps.dive_end_time = pm.cmd.server_time - tuning::DTP_POST_MOVE_PAUSE as i32 - 1;
    if ps.dive_end_time < 0 {
        ps.dive_end_time = 0;
    }
    ps.sprint_button_up_required = 0;
}
