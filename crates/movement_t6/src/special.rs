//! Black Ops 2's linked, spectator and intermission moves: the cases of its
//! `PmoveSingle` that leave the normal move out. Noclip, ufo and the viewer
//! type have no case of their own there; they take the normal move.

use crate::physics::{accelerate, cmd_scale_air, divisor, friction, sqrt};
use crate::slide::step_slide_move;
use crate::state::{ENTITYNUM_NONE, buttons, e_flags, pm_flags, pm_type};
use crate::{Gap, MoveWorld, Pm, Pml, footsteps, sprint, stance, tuning, weapon};

/// How far above and below a spectator the height-lock volumes are searched.
const HEIGHT_LOCK_REACH: f32 = 3000.0;
const FLY_ACCELERATE: f32 = 8.0;

/// A player on a ladder lets go of it.
pub(crate) fn ladder_falls(ps: &mut crate::PlayerState) {
    if ps.pm_flags & pm_flags::LADDER != 0 {
        ps.pm_flags = (ps.pm_flags & !pm_flags::LADDER) | pm_flags::LADDER_FALL;
    }
}

/// A scripted animation lock: it ends when the player animation script's
/// event runs out, which IW4L does not play.
pub(crate) fn anim_lock<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    if pm.ps.pm_flags & pm_flags::ANIM_LOCK != 0 {
        pm.gap(Gap::AnimLock);
    }
}

/// A player linked to an entity: held where the link puts him, his weapon
/// and stance still run.
pub(crate) fn linked_move<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &mut Pml) {
    ladder_falls(pm.ps);
    pm.ps.ground_entity_num = ENTITYNUM_NONE;
    pml.walking = false;
    pml.ground_plane = false;
    pml.almost_ground_plane = false;
    pm.ps.velocity = [0.0; 3];
    anim_lock(pm);
    weapon::update_ads_intent(pm, pml);
    sprint::update(pm);
    weapon::update_sight_aiming(pm);
    crate::pmove::drop_timers(pm, pml);
    stance::check_duck(pm, pml);
    footsteps::footsteps(pm, pml);
    weapon::pm_weapon(pm, pml);
    pm.ps.left_hand = true;
    weapon::pm_weapon(pm, pml);
    pm.ps.left_hand = false;
    if pm.ps.e_flags & e_flags::VEHICLE_VIEW != 0 && pm.ps.vehicle_entity != ENTITYNUM_NONE {
        pm.gap(Gap::MountedView);
    }
    if pm.ps.pm_flags & pm_flags::DIVING != 0 && pm.ps.pm_type == pm_type::NORMAL_LINKED {
        crate::dive::end_linked(pm.ps, pm.cmd.server_time);
    }
}

/// A free spectator flies, kept inside the map's height-lock volumes.
pub(crate) fn spectator_move<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &mut Pml) {
    ladder_falls(pm.ps);
    weapon::update_ads_intent(pm, pml);
    sprint::update(pm);
    weapon::update_sight_aiming(pm);
    stance::check_duck(pm, pml);
    crate::pmove::drop_timers(pm, pml);
    fly_move(pm, pml);
    weapon::ads_lerp(pm, pml);
    keep_in_height_lock(pm, pml.previous_origin);
}

/// The end of the game: only aiming and sprint wind down.
pub(crate) fn intermission_move<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    ladder_falls(pm.ps);
    weapon::update_ads_intent(pm, pml);
    sprint::update(pm);
    weapon::ads_lerp(pm, pml);
}

/// Level flight along the view's yaw; attack rises and aim sinks.
fn fly_move<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &mut Pml) {
    friction(pm, pml);
    let cmd = pm.cmd;
    let scale = cmd_scale_air(pm.ps, &cmd);
    let [mut wish_x, mut wish_y, mut wish_z] = [0.0_f32; 3];
    if scale != 0.0 {
        let r = pml.right;
        let ahead = [
            r[2] * 0.0 - r[1],
            r[0] - r[2] * 0.0,
            r[1] * 0.0 - r[0] * 0.0,
        ];
        let fmove = f32::from(cmd.forwardmove);
        let rmove = f32::from(cmd.rightmove);
        wish_x = (fmove * ahead[0] + rmove * r[0]) * scale;
        wish_y = (r[1] * rmove + fmove * ahead[1]) * scale;
        wish_z = (r[2] * rmove + fmove * ahead[2]) * scale;
    }
    let speed = pm.ps.speed;
    if speed != 0 {
        let mut up = speed as f32 * 127.0 / (libm::sqrt(16129.0_f64) as f32 * 127.0);
        if pm.ps.pm_flags & pm_flags::SIGHT_AIMING != 0 || pm.ps.leanf != 0.0 {
            up *= 0.4;
        }
        if pm.ps.pm_type == pm_type::SPECTATOR {
            up *= tuning::SPECTATE_SPEED_SCALE;
        }
        if cmd.buttons.held(buttons::ATTACK) {
            wish_z += up * 127.0;
        }
        if cmd.buttons.held(buttons::ADS) {
            wish_z -= up * 127.0;
        }
    }
    let wishspeed = sqrt(wish_y * wish_y + wish_x * wish_x + wish_z * wish_z);
    let n = 1.0 / divisor(wishspeed);
    let wishdir = [wish_x * n, wish_y * n, wish_z * n];
    accelerate(pm.ps, pml, wishdir, wishspeed, FLY_ACCELERATE);
    step_slide_move(pm, pml, false);
}

/// Pulls a spectator who left the height-lock volumes back along the axis
/// he crossed them on, or back to where he was.
fn keep_in_height_lock<W: MoveWorld>(pm: &mut Pm<'_, W>, previous: [f32; 3]) {
    let inside = |pm: &Pm<'_, W>, point| pm.world.height_lock(point, HEIGHT_LOCK_REACH);
    let Some(was_inside) = inside(pm, previous) else {
        return;
    };
    let o = pm.ps.origin;
    if inside(pm, o) == Some(true) {
        return;
    }
    if inside(pm, [previous[0], o[1], o[2]]) == Some(true) {
        pm.ps.origin[0] = previous[0];
        return;
    }
    if inside(pm, [o[0], previous[1], o[2]]) == Some(true) {
        pm.ps.origin[1] = previous[1];
        return;
    }
    if was_inside {
        pm.ps.origin = previous;
        pm.ps.velocity = [0.0; 3];
    }
}
