//! The bob cycle, footstep sounds and which movement animation the legs play.

use crate::events::{MoveEvent, anim, event};
use crate::physics::{effective_stance, sqrt, stance_speed_scale};
use crate::state::{ENTITYNUM_NONE, e_flags, pm_flags, pm_type};
use crate::{MoveWorld, Pm, Pml, SURF_SLICK, tuning};

/// Black Ops' movement animation types the legs are asked to play.
pub mod movetype {
    pub const IDLE: i32 = 1;
    pub const SPRINT: i32 = 7;
    pub const SWIM: i32 = 24;
    pub const DIVE: i32 = 25;
    pub const SLIDE: i32 = 26;
}

/// `[stance (+3 running backwards)][speed]` → movement animation type.
const MOVING: [[i32; 3]; 6] = [
    [4, 3, 2],
    [3, 3, 2],
    [4, 3, 2],
    [4, 3, 2],
    [3, 3, 2],
    [4, 3, 2],
];
/// `[stance (+3 running backwards)]` → movement animation type in the air.
const FALLING: [i32; 6] = [3, 3, 3, 3, 3, 3];
/// `[stance (+3 running backwards)][ads or slow]` → how much the view bobs.
const BOB: [[f32; 2]; 6] = [
    [0.335000008, 0.280000001],
    [0.25, 0.239999995],
    [0.340000004, 0.289999992],
    [0.360000014, 0.300000012],
    [0.25, 0.239999995],
    [0.340000004, 0.289999992],
];
const DIVE_BOB: f32 = 0.239999995;

/// Advances the bob cycle by the frame; the weapon sets how fast it bobs
/// sprinting, crouch-sprinting and diving.
fn advance_bob<W: MoveWorld>(pm: &Pm<'_, W>, pml: &Pml, old: i32, scale: f32) -> i32 {
    let ps = &*pm.ps;
    let msec = pml.msec as f32;
    let f = if ps.pm_flags & pm_flags::DIVING != 0 {
        msec * pm.world.weapon(ps.weapon).dtp_scale
    } else if ps.pm_flags & pm_flags::SPRINTING != 0
        && ps.view_height_target == crate::state::view_height::CROUCH
    {
        msec * pm.world.weapon(ps.weapon).ducked_sprint_scale
    } else if ps.pm_flags & pm_flags::SPRINTING == 0 && ps.water_level < 3 {
        msec
    } else {
        msec * pm.world.weapon(ps.weapon).sprint_scale
    };
    ((f * scale + old as f32) as i32) & 0xff
}

/// The footstep a step sounds like.
fn footstep_event<W: MoveWorld>(pm: &Pm<'_, W>) -> i32 {
    let ps = &*pm.ps;
    let slowed = ps.pm_flags & pm_flags::SIGHT_AIMING != 0 || ps.leanf != 0.0;
    if ps.pm_flags & pm_flags::PRONE != 0 {
        return event::FOOTSTEP_PRONE;
    }
    if ps.pm_flags & pm_flags::DUCKED != 0 {
        if slowed || tuning::RUN_THRESHOLD > pm.xyspeed {
            return event::FOOTSTEP_CROUCH_WALK;
        }
        return event::FOOTSTEP_CROUCH_RUN;
    }
    if slowed {
        return event::FOOTSTEP_WALK;
    }
    if ps.pm_flags & pm_flags::SPRINTING != 0 {
        return event::FOOTSTEP_SPRINT;
    }
    if tuning::RUN_THRESHOLD > pm.xyspeed {
        event::FOOTSTEP_WALK
    } else {
        event::FOOTSTEP_RUN
    }
}

/// A step sounds when the bob cycle crosses into a new step.
pub(crate) fn bob_event<W: MoveWorld>(
    pm: &mut Pm<'_, W>,
    pml: &Pml,
    old: i32,
    new: i32,
    footsteps: bool,
) {
    if (old ^ new) & !0x3f == 0 || !footsteps {
        return;
    }
    if pm.ps.ground_entity_num == ENTITYNUM_NONE {
        if pm.ps.pm_flags & pm_flags::LADDER != 0 {
            pm.gap(crate::Gap::Ladder);
        }
        return;
    }
    let surface = pm.surface_sound(pml);
    pm.event(footstep_event(pm), surface);
}

/// The fastest the player could move as they move now, for the bob rate.
fn bob_max_speed<W: MoveWorld>(pm: &Pm<'_, W>, ads: bool, sprint: bool) -> f32 {
    let ps = &*pm.ps;
    let mut speed = ps.speed as f32;
    let fmove = pm.cmd.forwardmove;
    if fmove != 0 {
        if pm.cmd.rightmove != 0 {
            speed = ((tuning::STRAFE_SPEED_SCALE - 1.0) * 0.75 + 1.0 + 1.0) * speed * 0.5;
            if fmove < 0 {
                speed = (tuning::BACK_SPEED_SCALE + 1.0) * speed * 0.5;
            }
        } else if fmove < 0 {
            speed *= tuning::BACK_SPEED_SCALE;
        }
    } else if pm.cmd.rightmove != 0 {
        speed *= (tuning::STRAFE_SPEED_SCALE - 1.0) * 0.75 + 1.0;
    }
    if ads {
        speed *= 0.4;
    } else if sprint {
        speed *= tuning::SPRINT_SPEED_SCALE;
    }
    if ps.weapon != 0 {
        let weapon = pm.world.weapon(ps.weapon);
        if weapon.move_speed_scale > 0.0 && ps.pm_flags & pm_flags::SIGHT_AIMING == 0 {
            speed *=
                crate::physics::weapon_move_speed_scale(ps, pm.zombiemode, weapon.move_speed_scale);
        } else if weapon.ads_move_speed_scale > 0.0 {
            speed *= weapon.ads_move_speed_scale;
        }
    }
    stance_speed_scale(ps, pm.cmd.server_time) * speed * ps.move_speed_scale_multiplier
}

fn moving_movetype<W: MoveWorld>(
    pm: &mut Pm<'_, W>,
    pml: &Pml,
    stance: usize,
    speed: usize,
    sprint: bool,
) {
    let ps = &*pm.ps;
    let mut kind = MOVING[stance][speed];
    if stance == 0 && sprint {
        kind = movetype::SPRINT;
    }
    if ps.pm_flags & pm_flags::DIVING != 0 {
        kind = movetype::DIVE;
    }
    if pml.ground_trace.surface_flags & SURF_SLICK != 0 {
        kind = movetype::SLIDE;
    }
    if ps.water_level >= 3 {
        kind = movetype::SWIM;
    }
    pm.out(MoveEvent::LegsMove(kind, sprint));
}

/// A player whose damage timer runs flinches instead of idling.
fn flinching<W: MoveWorld>(pm: &Pm<'_, W>) -> bool {
    let ps = &*pm.ps;
    if ps.view_height_target as f32 != ps.view_height_current {
        return false;
    }
    let left = (ps.damage_duration - tuning::DMGTIMER_FLINCH_TIME).max(0);
    ps.damage_timer > left
}

fn idle_movetype<W: MoveWorld>(pm: &mut Pm<'_, W>, may_flinch: bool) {
    if pm.xyspeed < 1.0 && pm.ps.water_level < 3 {
        pm.ps.bob_cycle = 0;
    }
    if pm.ps.water_level >= 3 {
        pm.out(MoveEvent::LegsMove(movetype::SWIM, false));
        return;
    }
    if may_flinch && flinching(pm) {
        pm.anim_event(anim::FLINCH, true, true);
        return;
    }
    pm.out(MoveEvent::LegsMove(movetype::IDLE, false));
}

fn standing_still_movetype<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    if pm.xyspeed > 120.0 {
        return;
    }
    if pm.ps.water_level >= 3 {
        pm.out(MoveEvent::LegsMove(movetype::SWIM, false));
    } else if flinching(pm) {
        pm.anim_event(anim::FLINCH, true, true);
    } else {
        pm.out(MoveEvent::LegsMove(movetype::IDLE, false));
    }
}

fn falling_movetype<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml, stance: usize) {
    let ps = &*pm.ps;
    if !pml.almost_ground_plane
        || pml.walking
        || ps.pm_flags & pm_flags::MANTLE != 0
        || ps.pm_type != pm_type::NORMAL
    {
        return;
    }
    if tuning::MOVE_THRESHOLD > pm.xyspeed {
        idle_movetype(pm, false);
        return;
    }
    let mut kind = FALLING[stance];
    if ps.pm_flags & pm_flags::DIVING != 0 {
        kind = movetype::DIVE;
    }
    if pml.ground_trace.surface_flags & SURF_SLICK != 0 {
        kind = movetype::SLIDE;
    }
    if ps.water_level >= 3 {
        kind = movetype::SWIM;
    }
    pm.out(MoveEvent::LegsMove(kind, false));
}

pub(crate) fn footsteps<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    if pm.ps.pm_type >= pm_type::DEAD {
        return;
    }
    let v = pm.ps.velocity;
    pm.xyspeed = sqrt(v[0] * v[0] + v[1] * v[1]);
    if pm.ps.pm_flags & pm_flags::DIVING != 0 {
        let old = pm.ps.bob_cycle;
        pm.ps.bob_cycle = advance_bob(pm, pml, old, DIVE_BOB);
    }
    if pm.ps.e_flags & e_flags::TURRET != 0 {
        pm.out(MoveEvent::LegsMove(movetype::IDLE, false));
        return;
    }
    let stance = effective_stance(pm.ps) as usize;
    let flags = pm.ps.pm_flags;
    let stance_anim = stance
        + if flags & pm_flags::BACKWARDS_RUN != 0 {
            3
        } else {
            0
        };
    let ps = &*pm.ps;
    if ps.ground_entity_num == ENTITYNUM_NONE
        && ps.pm_type != pm_type::NORMAL_LINKED
        && ps.pm_type != pm_type::LAST_STAND
        && ps.water_level == 0
        && flags & (pm_flags::DIVING | pm_flags::SLIDING) == 0
        && flags & pm_flags::MANTLE == 0
    {
        if flags & pm_flags::LADDER != 0 {
            pm.gap(crate::Gap::Ladder);
            return;
        }
        falling_movetype(pm, pml, stance_anim);
        return;
    }
    let mut slowed = flags & pm_flags::SIGHT_AIMING != 0 || ps.leanf != 0.0;
    let mut sprint = flags & pm_flags::SPRINTING != 0;
    if (tuning::MOVE_THRESHOLD > pm.xyspeed || ps.pm_type == pm_type::NORMAL_LINKED)
        && flags & pm_flags::SLIDING == 0
        && flags & pm_flags::MANTLE == 0
    {
        idle_movetype(pm, true);
        if pm.ps.water_level >= 3 {
            let old = pm.ps.bob_cycle;
            pm.ps.bob_cycle = advance_bob(pm, pml, old, tuning::WEAPON_BOB_FREQUENCY_SWIMMING);
        }
        return;
    }
    if pm.cmd.forwardmove == 0 && pm.cmd.rightmove == 0 && flags & pm_flags::SLIDING == 0 {
        standing_still_movetype(pm);
        if pm.ps.water_level >= 3 {
            let old = pm.ps.bob_cycle;
            pm.ps.bob_cycle = advance_bob(pm, pml, old, tuning::WEAPON_BOB_FREQUENCY_SWIMMING);
        }
        return;
    }
    let mut speed = usize::from(slowed);
    if stance == 0 && pm.ps.pm_flags & pm_flags::DIVING == 0 {
        slowed = slowed || tuning::RUNBK_THRESHOLD >= pm.xyspeed;
        sprint = sprint && pm.xyspeed >= tuning::SPRINT_THRESHOLD;
        speed = if tuning::ANIM_WALK_THRESHOLD > pm.xyspeed {
            if tuning::ENABLE_SHUFFLE_ANIMS { 2 } else { 1 }
        } else if tuning::ANIM_RUN_THRESHOLD > pm.xyspeed {
            1
        } else {
            0
        };
    }
    moving_movetype(pm, pml, stance_anim, speed, sprint);
    let max = bob_max_speed(pm, slowed, sprint);
    let bob = if stance_anim == 0 && sprint {
        tuning::SPRINT_CAMERA_BOB
    } else {
        BOB[stance_anim][usize::from(slowed)]
    };
    let mut scale = pm.xyspeed / max * bob;
    if pm.ps.pm_flags & (pm_flags::DIVING | pm_flags::SLIDING) != 0 {
        return;
    }
    if pm.ps.water_level >= 3 {
        scale = tuning::WEAPON_BOB_FREQUENCY_SWIMMING;
    }
    let old = pm.ps.bob_cycle;
    let new = advance_bob(pm, pml, old, scale);
    pm.ps.bob_cycle = new;
    bob_event(pm, pml, old, new, true);
}
