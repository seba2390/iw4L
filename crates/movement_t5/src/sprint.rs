use crate::perks::mp;
use crate::state::{buttons, e_flags, pm_flags, pm_type};
use crate::{MoveWorld, Pm, tuning};

/// Weapon states that keep (or stop) a player from sprinting: throws,
/// melee and the offhand states.
fn weapon_state_blocks_sprint(state: i32) -> bool {
    matches!(state, 0x12..=0x1a)
}

fn weapon_state_using(state: i32) -> bool {
    matches!(state, 6 | 7 | 8 | 0x20 | 0x12 | 0x13 | 0x14)
}

/// Equipment (claymores, monkeys, …) may be thrown while sprinting.
const OFFHAND_SLOT_EQUIPMENT: i32 = 3;

/// Sprint never runs out: zombies' perk bits, or the multiplayer
/// executable's `specialty_unlimitedsprint`.
fn unlimited<W: MoveWorld>(pm: &Pm<'_, W>) -> bool {
    if pm.zombiemode {
        pm.ps.perks & crate::perks::SPRINT_UNLIMITED != 0
    } else {
        mp::has(pm.ps, mp::UNLIMITED_SPRINT)
    }
}

/// How long an exhausted sprint waits to recharge, in ms: zombies' dvar,
/// or in multiplayer the cooldown its scripts set (`setSprintCooldown`).
fn recharge_pause<W: MoveWorld>(pm: &Pm<'_, W>) -> i32 {
    if pm.zombiemode {
        (tuning::SPRINT_RECHARGE_PAUSE * 1000.0) as i32
    } else {
        pm.ps.sprint_cooldown
    }
}

/// The longest sprint the player has, in ms.
fn max_sprint_time<W: MoveWorld>(pm: &Pm<'_, W>) -> i32 {
    let ps = &*pm.ps;
    let scale = pm.world.weapon(ps.weapon).def.sprint_duration_scale();
    let base = if ps.e_flags & e_flags::VEHICLE_VIEW != 0 {
        tuning::VEHICLE_PERK_BOOST_DURATION * 1000.0
    } else if pm.zombiemode {
        scale * (tuning::SPRINT_TIME * 1000.0)
    } else {
        // Multiplayer: the length its scripts set (`setSprintDuration`).
        ps.sprint_duration as f32 * scale
    };
    let level = if pm.zombiemode {
        ps.perks & crate::perks::SPRINT_LEVEL
    } else {
        0
    };
    let time = if !pm.zombiemode {
        if mp::has(ps, mp::LONGER_SPRINT) {
            tuning::PERK_SPRINT_MULTIPLIER * base
        } else {
            base
        }
    } else if level != 0 {
        (f64::from(level)
            * f64::from(0.333333343_f32)
            * f64::from(tuning::PERK_SPRINT_MULTIPLIER)
            * f64::from(base)) as f32
    } else {
        base
    };
    (time as i32).min(0x3fff)
}

/// How much sprint the player has left now, in ms.
fn sprint_left<W: MoveWorld>(pm: &Pm<'_, W>) -> i32 {
    let ps = &*pm.ps;
    let max = max_sprint_time(pm);
    let now = pm.cmd.server_time;
    let left = if tuning::SPRINT_UNLIMITED || unlimited(pm) || ps.last_sprint_start == 0 {
        max
    } else if ps.last_sprint_start > ps.last_sprint_end {
        ps.sprint_start_max_length - now + ps.last_sprint_start
    } else if ps.sprint_exhausted != 0 {
        let pause = recharge_pause(pm);
        -ps.last_sprint_end * 2 - pause + ps.sprint_start_max_length + ps.last_sprint_start + now
    } else {
        ps.sprint_start_max_length - ps.last_sprint_end * 2 + ps.last_sprint_start + now
    };
    left.max(0).min(max)
}

fn end_sprint<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    if pm.ps.pm_flags & pm_flags::SPRINTING == 0 {
        return;
    }
    pm.ps.sprint_exhausted = 0;
    pm.ps.last_sprint_end = pm.cmd.server_time;
    pm.ps.pm_flags &= !pm_flags::SPRINTING;
    if pm.cmd.buttons.held(buttons::SPRINT) {
        pm.ps.sprint_button_up_required = 1;
    }
}

fn throw_blocks<W: MoveWorld>(pm: &Pm<'_, W>) -> bool {
    pm.cmd.buttons.held(buttons::THROW)
        && pm.world.weapon(pm.ps.weapon).def.offhand_slot() != OFFHAND_SLOT_EQUIPMENT
}

fn other_actions_held<W: MoveWorld>(pm: &Pm<'_, W>) -> bool {
    let b = pm.cmd.buttons;
    b.held(buttons::JUMP)
        || b.held(buttons::RELOAD)
        || b.held(buttons::MELEE)
        || b.held(buttons::ATTACK)
        || b.held(buttons::USE_RELOAD)
        || b.held(buttons::FRAG)
        || b.held(buttons::SMOKE)
}

/// Whether something keeps the player from starting a sprint.
fn start_blocked<W: MoveWorld>(pm: &Pm<'_, W>) -> bool {
    let ps = &*pm.ps;
    if ps.pm_flags & pm_flags::LADDER != 0
        || ps.water_level >= 2
        || i32::from(pm.cmd.forwardmove) <= tuning::SPRINT_FORWARD_MINIMUM
        || other_actions_held(pm)
        || throw_blocks(pm)
        || ps.leanf != 0.0
        || ps.pm_flags
            & (pm_flags::SHELLSHOCKED | pm_flags::ADS_INTENT | pm_flags::LADDER | pm_flags::MANTLE)
            != 0
    {
        return true;
    }
    if ps.pm_flags & pm_flags::JUMPING != 0 && ps.pm_time == 0 {
        return false;
    }
    if weapon_state_blocks_sprint(ps.weaponstate) {
        return true;
    }
    if pm.world.weapon(ps.weapon).def.offhand_slot() == OFFHAND_SLOT_EQUIPMENT
        && weapon_state_using(ps.weaponstate)
    {
        return true;
    }
    if ps.pm_flags & pm_flags::SLIDING != 0 {
        return true;
    }
    ps.pm_flags & pm_flags::DIVING != 0
}

/// Whether a running sprint stops this frame.
fn must_stop<W: MoveWorld>(pm: &Pm<'_, W>) -> bool {
    let ps = &*pm.ps;
    if ps.water_level >= 2
        || ps.pm_flags & (pm_flags::SHELLSHOCKED | pm_flags::ADS_INTENT | pm_flags::LADDER) != 0
        || i32::from(pm.cmd.forwardmove) <= tuning::SPRINT_FORWARD_MINIMUM
        || other_actions_held(pm)
        || pm.cmd.buttons.held(buttons::PRONE)
        || pm.cmd.buttons.held(buttons::CROUCH)
        || ps.leanf != 0.0
        || throw_blocks(pm)
        || weapon_state_blocks_sprint(ps.weaponstate)
        || matches!(ps.weaponstate, 0x22 | 0x23)
        || ps.pm_flags & pm_flags::SLIDING != 0
    {
        return true;
    }
    ps.pm_flags & pm_flags::SPRINT_DISABLED != 0
}

fn can_rise_to_sprint<W: MoveWorld>(pm: &mut Pm<'_, W>) -> bool {
    if pm.ps.pm_flags & pm_flags::PRONE == 0 {
        return true;
    }
    let maxs_z = pm.maxs[2];
    pm.maxs[2] = 50.0;
    let fits = crate::stance::fits(pm, pm.mins, pm.maxs);
    pm.maxs[2] = maxs_z;
    fits
}

fn can_stand_to_sprint<W: MoveWorld>(pm: &mut Pm<'_, W>) -> bool {
    if pm.ps.pm_flags & (pm_flags::PRONE | pm_flags::DUCKED) == 0 {
        return true;
    }
    crate::stance::fits(pm, [-15.0, -15.0, 0.0], [15.0, 15.0, 70.0])
}

pub(crate) fn update<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    if pm.ps.sprint_button_up_required != 0 && !pm.cmd.buttons.held(buttons::SPRINT) {
        pm.ps.sprint_button_up_required = 0;
    }
    if pm.ps.pm_type != pm_type::NORMAL && pm.ps.pm_type != pm_type::NORMAL_LINKED {
        end_sprint(pm);
        return;
    }
    if matches!(pm.ps.weaponstate, 0x30..=0x32) {
        end_sprint(pm);
        return;
    }
    if max_sprint_time(pm) <= 0 {
        end_sprint(pm);
        return;
    }
    let now = pm.cmd.server_time;
    if pm.ps.pm_flags & pm_flags::SPRINTING != 0 {
        if !unlimited(pm)
            && !tuning::SPRINT_UNLIMITED
            && now - pm.ps.last_sprint_start >= pm.ps.sprint_start_max_length
        {
            end_sprint(pm);
            pm.ps.sprint_exhausted = 1;
            return;
        }
        if must_stop(pm) {
            end_sprint(pm);
            if pm.cmd.buttons.held(buttons::JUMP) {
                pm.ps.last_sprint_end += 800;
            }
        }
        return;
    }
    if pm.ps.sprint_exhausted != 0 && recharge_pause(pm) > now - pm.ps.last_sprint_end {
        return;
    }
    if !pm.cmd.buttons.held(buttons::SPRINT)
        || pm.ps.pm_flags & pm_flags::SPRINT_DISABLED != 0
        || pm.ps.sprint_button_up_required != 0
        || start_blocked(pm)
        || !can_rise_to_sprint(pm)
        || !can_stand_to_sprint(pm)
    {
        return;
    }
    let left = sprint_left(pm);
    if left as f32 > tuning::SPRINT_MIN_TIME * 1000.0 {
        pm.ps.sprint_start_max_length = left;
        pm.ps.last_sprint_start = now;
        pm.ps.pm_flags |= pm_flags::SPRINTING;
    }
}
