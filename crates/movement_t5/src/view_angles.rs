use crate::math::{angle_delta, angle_vectors};
use crate::physics::effective_stance;
use crate::prone::{ProneCheck, check_prone};
use crate::state::{ENTITYNUM_NONE, buttons, e_flags, e_flags2, pm_flags, pm_type, weapon_state};
use crate::{MoveWorld, Pm, tuning};

const SHORT2ANGLE: f32 = 0.00549316406;
const INV_360: f32 = 0.00277777785;
const LEAN_CONTENTS: u32 = 0x0381_c813;

/// `a` wrapped into [-180, 180) in single precision.
pub(crate) fn wrap_180(a: f32) -> f32 {
    let turns = a * INV_360;
    (turns - libm::floorf(turns + 0.5)) * 360.0
}

/// `a` wrapped into [0, 360) in single precision.
pub(crate) fn wrap_360(a: f32) -> f32 {
    let turns = a * INV_360;
    let r = (turns - libm::floorf(turns)) * 360.0;
    if r - 360.0 >= 0.0 { r - 360.0 } else { r }
}

/// The view follows the command's angles, offset by the delta angles; pitch
/// stops at the limits by moving the delta.
fn angles_from_cmd<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let up = tuning::VIEW_PITCH_UP;
    let down = tuning::VIEW_PITCH_DOWN;
    for i in 0..3 {
        let cmd = pm.cmd.angles[i] as f32 * SHORT2ANGLE;
        let mut angle = wrap_180(cmd + pm.ps.delta_angles[i]);
        if i == 0 {
            let limit = if angle > down {
                Some(down)
            } else if -up > angle {
                Some(-up)
            } else {
                None
            };
            if let Some(limit) = limit {
                pm.ps.delta_angles[0] = limit - cmd;
                angle = limit;
            }
        }
        pm.ps.viewangles[i] = wrap_180(angle);
    }
}

/// Leaning around corners, stopped by what is beside the head.
fn update_lean<W: MoveWorld>(pm: &mut Pm<'_, W>, msec: f32) {
    let ps = &*pm.ps;
    let mut dir = 0;
    if ps.weaponstate != weapon_state::DEPLOYING
        && (pm.cmd.buttons.held(buttons::LEAN_LEFT) || pm.cmd.buttons.held(buttons::LEAN_RIGHT))
        && ps.e_flags2 & e_flags2::CONTROLS_LOCKED == 0
        && ps.pm_flags & pm_flags::FROZEN == 0
        && ps.pm_type < pm_type::DEAD
        && (ps.ground_entity_num != ENTITYNUM_NONE || ps.pm_type == pm_type::NORMAL_LINKED)
        && ps.pm_flags & pm_flags::NO_LEAN == 0
    {
        if pm.cmd.buttons.held(buttons::LEAN_LEFT) {
            dir = -1;
        }
        if pm.cmd.buttons.held(buttons::LEAN_RIGHT) {
            dir += 1;
        }
    }
    if ps.e_flags & (e_flags::TURRET | e_flags::VEHICLE_VIEW) != 0 {
        dir = 0;
    }
    let max = if effective_stance(ps) == 1 { 0.25 } else { 0.5 };
    let mut leanf = ps.leanf;
    if dir == 0 {
        if leanf > 0.0 {
            leanf -= msec * 0.00357142859 * max;
            if 0.0 > leanf {
                leanf = 0.0;
            }
        } else if 0.0 > leanf {
            leanf += msec * 0.00357142859 * max;
            if leanf > 0.0 {
                leanf = 0.0;
            }
        }
    } else if dir > 0 {
        if max > leanf {
            leanf += msec * 0.00285714283 * max;
        }
        if leanf > max {
            leanf = max;
        }
    } else {
        if leanf > -max {
            leanf -= msec * 0.00285714283 * max;
        }
        if -max > leanf {
            leanf = -max;
        }
    }
    pm.ps.leanf = leanf;
    if leanf == 0.0 || pm.ps.pm_type == pm_type::NORMAL_LINKED {
        return;
    }
    let sign = if leanf >= 0.0 { 1.0 } else { -1.0 };
    let o = pm.ps.origin;
    let start = [o[0], o[1], pm.ps.view_height_current + o[2]];
    let l = (2.0 - libm::fabsf(sign)) * sign;
    let (_, right, _) = angle_vectors([0.0, pm.ps.viewangles[1], l * 16.0]);
    let reach = l * 20.0;
    let end = [
        right[0] * reach + start[0],
        right[1] * reach + start[1],
        right[2] * reach + start[2],
    ];
    let trace = pm.trace_plain(start, end, [-8.0; 3], [8.0; 3], LEAN_CONTENTS);
    let room = (1.0 - libm::sqrt(1.0 - f64::from(trace.fraction))) as f32;
    if libm::fabsf(pm.ps.leanf) > room {
        pm.ps.leanf = if pm.ps.leanf >= 0.0 { room } else { -room };
    }
}

/// Whether a prone body fits turned toward `yaw` (blending the turn and the
/// body length by how far it is from the view).
fn prone_fits_at_yaw<W: MoveWorld>(pm: &mut Pm<'_, W>, yaw: f32) -> bool {
    let d = angle_delta(yaw, pm.ps.viewangles[1]);
    let k = libm::fabsf(d) * 0.00416666688;
    let w = 1.0 - k;
    let turned = wrap_360(yaw - w * d);
    let feet = k * 50.0 + 45.0 * w;
    prone_check(pm, turned, feet, true)
}

fn prone_check<W: MoveWorld>(pm: &mut Pm<'_, W>, yaw: f32, feet: f32, keep_pitches: bool) -> bool {
    let check = ProneCheck {
        origin: pm.ps.origin,
        radius: 15.0,
        height: 30.0,
        yaw,
        already_prone: true,
        on_ground: pm.ps.ground_entity_num != ENTITYNUM_NONE,
        ground_walkable: true,
        body_contents: false,
        feet_dist: feet,
    };
    let snapshot = *pm.ps;
    if !keep_pitches {
        return check_prone(pm.world, Some(&snapshot), pm.client_num(), &check, None);
    }
    let mut torso = pm.ps.prone_check_torso_pitch;
    let mut waist = pm.ps.prone_check_waist_pitch;
    let fits = check_prone(
        pm.world,
        Some(&snapshot),
        pm.client_num(),
        &check,
        Some((&mut torso, &mut waist)),
    );
    pm.ps.prone_check_torso_pitch = torso;
    pm.ps.prone_check_waist_pitch = waist;
    fits
}

/// A prone body turns slowly after the view, only where it fits; the view
/// may not turn further than the yaw cap from the body.
fn prone_yaw_cap<W: MoveWorld>(pm: &mut Pm<'_, W>, msec: f32, old_yaw: f32) {
    let yaw_at_entry = pm.ps.viewangles[1];
    let mut blocked = false;
    let d0 = angle_delta(pm.ps.prone_direction, yaw_at_entry);
    let soft_cap = tuning::PRONE_YAWCAP - 5.0;
    let outside = d0 > soft_cap || -soft_cap > d0;
    let turning = (pm.cmd.forwardmove != 0 || pm.cmd.rightmove != 0) && d0 != 0.0;
    if outside || turning {
        let step = msec * 0.0550000034;
        let mut target = if step > libm::fabsf(d0) {
            pm.ps.viewangles[1]
        } else if d0 > 0.0 {
            pm.ps.prone_direction - step
        } else {
            pm.ps.prone_direction + step
        };
        if pm.ps.pm_flags & pm_flags::PRONE_YAW_LOCKED == 0 {
            let mut keep_trying = true;
            let mut found = prone_fits_at_yaw(pm, target);
            while !found {
                if !keep_trying {
                    break;
                }
                let d = angle_delta(pm.ps.prone_direction, target);
                let s = if libm::fabsf(d) > 1.0 {
                    keep_trying = true;
                    if d > 0.0 { 1.0 } else { -1.0 }
                } else {
                    keep_trying = false;
                    blocked = true;
                    d
                };
                target = wrap_360(target + s);
                found = prone_fits_at_yaw(pm, target);
            }
            if found {
                let view_yaw = pm.ps.viewangles[1];
                if prone_check(pm, view_yaw, 45.0, false) && prone_check(pm, target, 45.0, false) {
                    pm.ps.prone_direction = target;
                } else {
                    blocked = true;
                }
            }
        }
    }
    let mut d = angle_delta(pm.ps.prone_direction, pm.ps.viewangles[1]);
    if d != 0.0 && pm.ps.pm_flags & pm_flags::PRONE_YAW_LOCKED == 0 {
        let mut body = pm.ps.prone_direction;
        let mut keep_trying = true;
        loop {
            let fits = prone_check(pm, body, 45.0, false);
            if fits && prone_fits_at_yaw(pm, body) {
                pm.ps.prone_direction = body;
                break;
            }
            if !keep_trying {
                break;
            }
            let s = if libm::fabsf(d) > 1.0 {
                keep_trying = true;
                if d > 0.0 { 1.0 } else { -1.0 }
            } else {
                keep_trying = false;
                d
            };
            pm.ps.delta_angles[1] += s;
            pm.ps.viewangles[1] = wrap_360(s + pm.ps.viewangles[1]);
            blocked = true;
            d = angle_delta(pm.ps.prone_direction, pm.ps.viewangles[1]);
            if !fits {
                body = wrap_360(body + d);
            }
        }
    }
    clamp_yaw_to_body(pm, blocked, d, old_yaw, yaw_at_entry);
    clamp_pitch_to_torso(pm);
}

fn clamp_yaw_to_body<W: MoveWorld>(
    pm: &mut Pm<'_, W>,
    blocked: bool,
    d: f32,
    old_yaw: f32,
    yaw_at_entry: f32,
) {
    let cap = tuning::PRONE_YAWCAP;
    if d > cap || -cap > d {
        let excess = if d > cap { d - cap } else { cap + d };
        pm.ps.delta_angles[1] += excess;
        let yaw = if excess > 0.0 {
            pm.ps.prone_direction - cap
        } else {
            pm.ps.prone_direction + cap
        };
        pm.ps.viewangles[1] = wrap_360(yaw);
    }
    if !blocked {
        return;
    }
    pm.ps.pm_flags |= pm_flags::STANCE_CHANGED;
    let e = angle_delta(old_yaw, pm.ps.viewangles[1]);
    if libm::fabsf(e) > 1.0 {
        return;
    }
    if angle_delta(yaw_at_entry, pm.ps.viewangles[1]) * e > 0.0 {
        let s = e * 0.980000019;
        pm.ps.viewangles[1] = wrap_360(pm.ps.viewangles[1] + s);
        pm.ps.delta_angles[1] += s;
    }
}

fn clamp_pitch_to_torso<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let d = angle_delta(pm.ps.prone_torso_pitch, pm.ps.viewangles[0]);
    if !(d > 45.0 || -45.0 > d) {
        return;
    }
    let excess = if d > 45.0 { d - 45.0 } else { d + 45.0 };
    pm.ps.delta_angles[0] += excess;
    let pitch = if excess > 0.0 {
        pm.ps.prone_torso_pitch - 45.0
    } else {
        pm.ps.prone_torso_pitch + 45.0
    };
    pm.ps.viewangles[0] = wrap_180(pitch);
}

pub(crate) fn update<W: MoveWorld>(pm: &mut Pm<'_, W>, msec: f32) {
    let ps = &*pm.ps;
    if ps.pm_type == pm_type::INTERMISSION {
        return;
    }
    if ps.pm_type >= pm_type::DEAD && ps.e_flags2 & e_flags2::MOUNTED == 0 {
        update_lean(pm, msec);
        return;
    }
    let old_yaw = ps.viewangles[1];
    angles_from_cmd(pm);
    let ps = &*pm.ps;
    let linked = ps.pm_type == pm_type::NORMAL_LINKED && ps.pm_flags & pm_flags::VIEW_LINKED == 0;
    if ps.e_flags & (e_flags::TURRET | e_flags::VEHICLE_VIEW) != 0 || linked {
        pm.gap(crate::Gap::MountedView);
        return;
    }
    if ps.pm_flags & pm_flags::MANTLE != 0 {
        pm.gap(crate::Gap::Mantle);
        return;
    }
    if ps.pm_flags & pm_flags::LADDER != 0
        && ps.ground_entity_num == ENTITYNUM_NONE
        && tuning::LADDER_YAWCAP != 0.0
    {
        pm.gap(crate::Gap::Ladder);
    }
    if pm.ps.pm_flags & pm_flags::PRONE != 0 {
        prone_yaw_cap(pm, msec, old_yaw);
    }
    if !matches!(
        pm.ps.pm_type,
        pm_type::UFO | pm_type::NOCLIP | pm_type::SPECTATOR
    ) {
        update_lean(pm, msec);
    }
}
