use crate::events::{anim, event};
use crate::math::{angle_delta, lerp};
use crate::physics::effective_stance;
use crate::prone::{ProneCheck, check_prone};
use crate::state::{ENTITYNUM_NONE, buttons, e_flags, pm_flags, pm_type, view_height};
use crate::{MoveWorld, Pm, Pml, Trace, tuning, view_height as eye};

const STANCE_PLAYER_CONTENTS: u32 = 0x0200_c000;
const DEAD_VIEW_HEIGHT: i32 = 8;

fn in_last_stand(pm_type: i32) -> bool {
    matches!(
        pm_type,
        pm_type::LAST_STAND | pm_type::LAST_STAND_REVIVED | pm_type::LAST_STAND_GETTING_UP
    )
}

/// Whether `mins`..`maxs` fit where the player stands.
pub(crate) fn fits<W: MoveWorld>(pm: &mut Pm<'_, W>, mins: [f32; 3], maxs: [f32; 3]) -> bool {
    let o = pm.ps.origin;
    let mask = pm.tracemask & !STANCE_PLAYER_CONTENTS;
    !pm.trace_with_mask(o, o, mins, maxs, mask).allsolid
}

fn stance_fits<W: MoveWorld>(pm: &mut Pm<'_, W>) -> bool {
    fits(pm, pm.mins, pm.maxs)
}

fn stance_fits_crouched<W: MoveWorld>(pm: &mut Pm<'_, W>) -> bool {
    let maxs_z = pm.maxs[2];
    pm.maxs[2] = 50.0;
    let fits = stance_fits(pm);
    pm.maxs[2] = maxs_z;
    fits
}

/// Whether the player may lie down where they stand.
fn prone_allowed<W: MoveWorld>(pm: &mut Pm<'_, W>) -> bool {
    if pm.world.weapon(pm.ps.weapon).def.blocks_prone() {
        return false;
    }
    if pm.ps.pm_flags & pm_flags::PRONE != 0 {
        return true;
    }
    if pm.ps.ground_entity_num == ENTITYNUM_NONE || pm.ps.water_level >= 1 {
        return false;
    }
    let check = ProneCheck {
        origin: pm.ps.origin,
        radius: pm.maxs[0],
        height: 30.0,
        yaw: pm.ps.viewangles[1],
        already_prone: false,
        on_ground: true,
        ground_walkable: true,
        body_contents: false,
        feet_dist: 50.0,
    };
    let snapshot = *pm.ps;
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

/// After a dive ends, the player stays down for a moment.
pub(crate) fn may_change_stance<W: MoveWorld>(pm: &mut Pm<'_, W>) -> bool {
    let ps = &*pm.ps;
    if tuning::DTP
        && ps.pm_flags & pm_flags::DIVING != 0
        && ps.water_level > 0
        && (!tuning::DTP_NEW_TRAJECTORY || ps.velocity[2] <= 0.0)
    {
        crate::dive::abort(pm);
    }
    let ps = &*pm.ps;
    if tuning::DTP
        && ps.pm_flags & pm_flags::DIVING == 0
        && ps.dive_end_time != 0
        && ((pm.cmd.server_time - ps.dive_end_time) as f32) < tuning::DTP_POST_MOVE_PAUSE
    {
        return false;
    }
    true
}

fn force_stand<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    pm.ps.view_height_target = view_height::STAND;
    pm.event(event::STANCE_FORCE_STAND, 0);
    eye::update(pm, pml);
}

/// Stances the scripts disallow: a stance key or a disallowed stance moves
/// the player to one that is allowed.
fn restricted_stance<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let restrictions =
        pm.ps.pm_flags & (pm_flags::NO_STAND | pm_flags::NO_CROUCH | pm_flags::NO_PRONE);
    if restrictions == 0 {
        return;
    }
    let f = pm.ps.pm_flags;
    let stance_event = if pm.cmd.buttons.held(buttons::PRONE) {
        if f & pm_flags::NO_PRONE == 0 {
            return;
        }
        if (f & 3 == 0 && f & pm_flags::NO_STAND == 0) || f & pm_flags::NO_CROUCH != 0 {
            event::STANCE_FORCE_STAND
        } else {
            event::STANCE_FORCE_CROUCH
        }
    } else if pm.cmd.buttons.held(buttons::CROUCH) {
        if f & pm_flags::NO_CROUCH == 0 {
            return;
        }
        if (f & 3 == 0 && f & pm_flags::NO_PRONE == 0) || f & pm_flags::NO_STAND != 0 {
            event::STANCE_FORCE_PRONE
        } else {
            event::STANCE_FORCE_STAND
        }
    } else if f & pm_flags::NO_STAND != 0 {
        if (f & pm_flags::PRONE != 0 && f & pm_flags::NO_PRONE == 0) || f & pm_flags::NO_CROUCH != 0
        {
            event::STANCE_FORCE_PRONE
        } else {
            event::STANCE_FORCE_CROUCH
        }
    } else {
        return;
    };
    pm.cmd
        .buttons
        .release_all(&[buttons::PRONE, buttons::CROUCH]);
    if stance_event == event::STANCE_FORCE_PRONE {
        pm.cmd.buttons.press(buttons::PRONE);
    } else if stance_event == event::STANCE_FORCE_CROUCH {
        pm.cmd.buttons.press(buttons::CROUCH);
    }
    pm.event(stance_event, 0);
}

fn linked_stance<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let restrictions = pm.ps.pm_flags & (pm_flags::NO_STAND | pm_flags::NO_CROUCH);
    if restrictions != 0 || pm.cmd.buttons.held(buttons::PRONE) {
        let f = pm.ps.pm_flags;
        let stance_event = if pm.cmd.buttons.held(buttons::PRONE) {
            if (f & 3 == 0 && f & pm_flags::NO_STAND == 0) || f & pm_flags::NO_CROUCH != 0 {
                Some(event::STANCE_FORCE_STAND)
            } else {
                Some(event::STANCE_FORCE_CROUCH)
            }
        } else if pm.cmd.buttons.held(buttons::CROUCH) {
            (f & pm_flags::NO_CROUCH != 0).then_some(event::STANCE_FORCE_STAND)
        } else {
            (f & pm_flags::NO_STAND != 0).then_some(event::STANCE_FORCE_CROUCH)
        };
        if let Some(stance_event) = stance_event {
            pm.cmd
                .buttons
                .release_all(&[buttons::PRONE, buttons::CROUCH]);
            if stance_event == event::STANCE_FORCE_CROUCH {
                pm.cmd.buttons.press(buttons::CROUCH);
            }
            pm.event(stance_event, 0);
        }
    }
    let wants_lower = pm.cmd.buttons.held(buttons::PRONE) || pm.cmd.buttons.held(buttons::CROUCH);
    if wants_lower && pm.ps.e_flags & e_flags::VEHICLE_VIEW == 0 {
        pm.ps.pm_flags = (pm.ps.pm_flags & !pm_flags::PRONE) | pm_flags::DUCKED;
    } else {
        pm.ps.pm_flags &= !(pm_flags::PRONE | pm_flags::DUCKED);
    }
}

fn normal_stance<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    if pm.ps.pm_flags & pm_flags::LADDER != 0
        && (pm.cmd.buttons.held(buttons::PRONE) || pm.cmd.buttons.held(buttons::CROUCH))
    {
        pm.cmd
            .buttons
            .release_all(&[buttons::PRONE, buttons::CROUCH]);
        pm.event(event::STANCE_FORCE_STAND, 0);
    }
    restricted_stance(pm);
    let not_last_stand = !in_last_stand(pm.ps.pm_type);
    if pm.cmd.buttons.held(buttons::PRONE) && pm.ps.pm_flags & pm_flags::RESPAWNED == 0 {
        if prone_allowed(pm) {
            pm.ps.pm_flags = (pm.ps.pm_flags & !pm_flags::DUCKED) | pm_flags::PRONE;
            return;
        }
        if pm.ps.ground_entity_num == ENTITYNUM_NONE {
            return;
        }
        pm.ps.pm_flags |= pm_flags::STANCE_CHANGED;
        let f = pm.ps.pm_flags;
        if pm.cmd.buttons.held(buttons::STANCE) {
            return;
        }
        if f & (pm_flags::PRONE | pm_flags::DUCKED) != 0 {
            pm.event(event::STANCE_FORCE_CROUCH, 0);
        } else {
            pm.event(event::STANCE_FORCE_STAND, 0);
        }
        return;
    }
    if pm.cmd.buttons.held(buttons::CROUCH) {
        if pm.ps.water_level >= 2 {
            pm.event(event::STANCE_FORCE_STAND, 0);
            return;
        }
        if pm.ps.pm_flags & pm_flags::PRONE != 0 {
            if stance_fits_crouched(pm) {
                if not_last_stand && pm.ps.ground_entity_num != ENTITYNUM_NONE {
                    pm.anim_event(anim::PRONE_TO_CROUCH, false, true);
                }
                pm.ps.pm_flags = (pm.ps.pm_flags & !pm_flags::PRONE) | pm_flags::DUCKED;
            } else if !pm.cmd.buttons.held(buttons::STANCE) {
                pm.event(event::STANCE_FORCE_PRONE, 2);
            }
            return;
        }
        if pm.ps.e_flags & e_flags::VEHICLE_VIEW != 0 {
            return;
        }
        if pm.ps.pm_flags & pm_flags::DUCKED == 0 && tuning::PLAY_STAND_TO_CROUCH_ANIMS {
            pm.anim_event(anim::STAND_TO_CROUCH, false, false);
        }
        pm.ps.pm_flags |= pm_flags::DUCKED;
        return;
    }
    if pm.ps.pm_flags & pm_flags::PRONE != 0 {
        if stance_fits(pm) {
            pm.anim_event(anim::PRONE_TO_STAND, false, false);
            pm.ps.pm_flags &= !(pm_flags::PRONE | pm_flags::DUCKED);
        } else if stance_fits_crouched(pm) {
            if not_last_stand {
                pm.anim_event(anim::PRONE_TO_CROUCH, false, true);
            }
            pm.ps.pm_flags = (pm.ps.pm_flags & !pm_flags::PRONE) | pm_flags::DUCKED;
        } else if !pm.cmd.buttons.held(buttons::STANCE) {
            pm.event(event::STANCE_FORCE_PRONE, 1);
        }
        return;
    }
    if pm.ps.pm_flags & pm_flags::DUCKED != 0 {
        if stance_fits(pm) {
            if tuning::PLAY_STAND_TO_CROUCH_ANIMS {
                pm.anim_event(anim::CROUCH_TO_STAND, false, false);
            }
            pm.ps.pm_flags &= !pm_flags::DUCKED;
        } else if !pm.cmd.buttons.held(buttons::STANCE) {
            pm.event(event::STANCE_FORCE_CROUCH, 1);
        }
    }
}

/// The pitch a prone body facing `yaw` takes on the ground plane of `trace`.
pub(crate) fn prone_pitch_on_ground(yaw: f32, trace: &Trace) -> f32 {
    let r = f64::from(yaw * 0.0174532924);
    let s = libm::sin(r) as f32;
    let c = libm::cos(r) as f32;
    let n = trace.normal;
    if n[2] == 0.0 {
        return 270.0;
    }
    let d = (f64::from(n[1]) * f64::from(s) + f64::from(n[0]) * f64::from(c)) / f64::from(n[2]);
    (libm::atan(d) * f64::from(57.2957764_f32)) as f32
}

fn settle_prone<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    if pm.cmd.forwardmove != 0 || pm.cmd.rightmove != 0 {
        pm.ps.pm_flags &= !pm_flags::PRONEMOVE_OVERRIDDEN;
        pm.reset_ads();
    }
    let mask = pm.tracemask & !STANCE_PLAYER_CONTENTS;
    let o = pm.ps.origin;
    let up = [o[0], o[1], o[2] + 10.0];
    let tr = pm.trace_plain(o, up, pm.mins, pm.maxs, mask);
    let up = lerp(o, up, tr.fraction);
    let tr = pm.trace_plain(up, o, pm.mins, pm.maxs, mask);
    pm.ps.origin = lerp(up, o, tr.fraction);
    pm.ps.prone_direction = pm.ps.viewangles[1];
    let o = pm.ps.origin;
    let tr = pm.trace_plain(o, [o[0], o[1], o[2] - 0.25], pm.mins, pm.maxs, mask);
    pm.ps.prone_direction_pitch =
        if !tr.startsolid && 1.0 > tr.fraction && pm.ps.pm_type != pm_type::LAST_STAND {
            prone_pitch_on_ground(pm.ps.prone_direction, &tr)
        } else {
            0.0
        };
    let pitch = pm.ps.viewangles[0];
    let d = angle_delta(pm.ps.prone_direction_pitch, pitch);
    pm.ps.prone_torso_pitch = if -45.0 > d {
        pitch - 45.0
    } else if d > 45.0 {
        pitch + 45.0
    } else {
        pm.ps.prone_direction_pitch
    };
}

/// Going prone holds the player still for a moment.
fn prone_entry_lock(ps: &mut crate::PlayerState) {
    if ps.pm_time == 0 {
        ps.pm_flags |= pm_flags::JUMPING;
        ps.pm_time = 1800;
    }
}

/// Picks the stance from the stance buttons and what fits, and sets the
/// bounds and eye height for it.
pub(crate) fn check_duck<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let was_prone = pm.ps.pm_flags & pm_flags::PRONE != 0;
    pm.mins = [-15.0, -15.0, 0.0];
    pm.maxs = [15.0, 15.0, 70.0];
    if pm.ps.pm_type == pm_type::DEAD {
        pm.ps.view_height_target = DEAD_VIEW_HEIGHT;
        if pm.ps.pm_flags & pm_flags::DUCKED != 0 {
            pm.maxs[2] = 50.0;
        } else if pm.ps.pm_flags & pm_flags::PRONE != 0 {
            pm.maxs[2] = 30.0;
        }
        eye::update(pm, pml);
        return;
    }
    if was_prone && pm.ps.water_level >= 1 {
        pm.ps.view_height_target = view_height::CROUCH;
        pm.event(event::STANCE_FORCE_CROUCH, 0);
        if !in_last_stand(pm.ps.pm_type) {
            pm.anim_event(anim::PRONE_TO_CROUCH, false, true);
        }
        pm.ps.pm_flags = (pm.ps.pm_flags & !pm_flags::PRONE) | pm_flags::STANCE_CHANGED;
        if pm.ps.water_level >= 3 {
            pm.ps.view_height_target = view_height::STAND;
            pm.ps.pm_flags &= !(pm_flags::PRONE | pm_flags::DUCKED);
            pm.event(event::STANCE_FORCE_STAND, 0);
        }
        eye::update(pm, pml);
        return;
    }
    let flags = pm.ps.pm_flags;
    if flags & (pm_flags::PRONE | pm_flags::DUCKED) != 0
        && (pm.ps.e_flags & e_flags::VEHICLE_VIEW != 0 || pm.ps.water_level >= 2)
    {
        pm.ps.pm_flags &= !(pm_flags::PRONE | pm_flags::DUCKED);
        force_stand(pm, pml);
        return;
    }
    if flags & pm_flags::SPRINTING != 0 {
        if flags & pm_flags::PRONE != 0 {
            pm.anim_event(anim::PRONE_TO_SPRINT, false, true);
        }
        pm.ps.e_flags &= !(e_flags::CROUCHING | e_flags::PRONE);
        pm.ps.pm_flags &= !(pm_flags::PRONE | pm_flags::DUCKED);
        force_stand(pm, pml);
        return;
    }
    let turret = pm.ps.e_flags & e_flags::TURRET;
    if turret != 0 {
        pm.ps.pm_flags = match turret {
            e_flags::TURRET_PRONE => (flags & !pm_flags::DUCKED) | pm_flags::PRONE,
            e_flags::TURRET_CROUCH => (flags & !pm_flags::PRONE) | pm_flags::DUCKED,
            _ => flags & !(pm_flags::PRONE | pm_flags::DUCKED),
        };
    } else if flags & (pm_flags::RESPAWNED | pm_flags::FROZEN) != 0
        || pm.ps.e_flags2 & crate::state::e_flags2::CONTROLS_LOCKED != 0
        || pm.weapon_blocks_stance()
        || pm.ps.weaponstate == crate::state::weapon_state::DEPLOYING
    {
    } else if pm.ps.pm_type == pm_type::NORMAL_LINKED {
        linked_stance(pm);
    } else if in_last_stand(pm.ps.pm_type) && pm.ps.pm_type != pm_type::LAST_STAND_GETTING_UP {
        pm.ps.pm_flags = (pm.ps.pm_flags & !pm_flags::PRONE) | pm_flags::DUCKED;
    } else if pm.ps.pm_flags & pm_flags::DIVING != 0 || !may_change_stance(pm) {
        pm.cmd.buttons.press(buttons::PRONE);
        pm.cmd
            .buttons
            .release_all(&[buttons::SPRINT, buttons::CROUCH]);
        pm.ps.e_flags &= !e_flags::CROUCHING;
        pm.ps.pm_flags &= !pm_flags::DUCKED;
        pm.ps.view_height_target = view_height::PRONE;
        pm.event(event::STANCE_FORCE_PRONE, 0);
    } else {
        normal_stance(pm);
    }

    if pm.ps.view_height_lerp_time == 0 {
        if pm.ps.pm_type == pm_type::LAST_STAND || pm.ps.pm_type == pm_type::LAST_STAND_REVIVED {
            match pm.ps.view_height_target {
                view_height::STAND => pm.anim_event(anim::STAND_TO_LAST_STAND, false, true),
                view_height::CROUCH => pm.anim_event(anim::CROUCH_TO_LAST_STAND, false, true),
                view_height::PRONE => pm.anim_event(anim::PRONE_TO_LAST_STAND, false, true),
                _ => {}
            }
            pm.ps.view_height_target = view_height::DOWNED;
        } else if pm.ps.pm_flags & pm_flags::PRONE != 0 {
            if pm.ps.view_height_target == view_height::STAND {
                pm.ps.view_height_target = view_height::CROUCH;
            } else if pm.ps.view_height_target != view_height::PRONE {
                pm.ps.view_height_target = view_height::PRONE;
                pm.prone_anim();
                prone_entry_lock(pm.ps);
            }
        } else if pm.ps.view_height_target == view_height::PRONE {
            pm.ps.view_height_target = view_height::CROUCH;
            pm.prone_anim();
        } else {
            pm.ps.view_height_target = if pm.ps.pm_flags & pm_flags::DUCKED != 0 {
                view_height::CROUCH
            } else {
                view_height::STAND
            };
        }
    }

    eye::update(pm, pml);
    match effective_stance(pm.ps) {
        1 => {
            pm.maxs[2] = 30.0;
            pm.ps.e_flags = (pm.ps.e_flags & !e_flags::CROUCHING) | e_flags::PRONE;
            pm.ps.pm_flags = (pm.ps.pm_flags & !pm_flags::DUCKED) | pm_flags::PRONE;
        }
        2 => {
            pm.maxs[2] = 50.0;
            pm.ps.e_flags = (pm.ps.e_flags & !e_flags::PRONE) | e_flags::CROUCHING;
            pm.ps.pm_flags = (pm.ps.pm_flags & !pm_flags::PRONE) | pm_flags::DUCKED;
        }
        _ => {
            pm.maxs[2] = 70.0;
            pm.ps.e_flags &= !(e_flags::CROUCHING | e_flags::PRONE);
            pm.ps.pm_flags &= !(pm_flags::PRONE | pm_flags::DUCKED);
        }
    }
    if pm.ps.pm_flags & pm_flags::DIVING != 0 {
        let current = pm.ps.view_height_current;
        pm.maxs[2] = if 30.0 > current {
            30.0
        } else if 50.0 > current {
            50.0
        } else {
            70.0
        };
    }
    if pm.ps.pm_flags & pm_flags::PRONE != 0 {
        if !was_prone {
            settle_prone(pm);
        }
        if pm.ps.pm_type == pm_type::LAST_STAND {
            pm.ps.prone_direction = pm.ps.viewangles[1];
        }
    }
}
