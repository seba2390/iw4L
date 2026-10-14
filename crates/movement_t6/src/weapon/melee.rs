//! Melee: the key (or attack with a melee weapon, or a dual wield's left
//! trigger), the lunge, the swing and the raise after.

use super::{
    Hand, def, dual_wield, finish, fire::cancel_grenade, hold_prone, melee_weapon, start_anim,
    start_anim_left, weap_anim, weapon_type,
};
use crate::events::{anim, event};
use crate::inventory as inv;
use crate::state::{
    ENTITYNUM_NONE, buttons, e_flags, e_flags2, pm_flags, pm_type, weap_flags, weapon_state as ws,
};
use crate::{MoveWorld, Pm, Pml, tuning};

/// Off the ground and not held to it by a link or last stand.
fn in_air<W: MoveWorld>(pm: &Pm<'_, W>, pml: &Pml) -> bool {
    let ps = &*pm.ps;
    ps.ground_entity_num == ENTITYNUM_NONE
        && ps.pm_type != pm_type::NORMAL_LINKED
        && ps.pm_type != pm_type::LAST_STAND
        && !pml.almost_ground_plane
}

/// The variant's zoom levels differ: a variable zoom.
fn variable_zoom<W: MoveWorld>(pm: &Pm<'_, W>) -> bool {
    let fov = def(pm, pm.ps.weapon).def.ads_zoom_fovs();
    fov[0] != fov[1] || fov[1] != fov[2]
}

/// Starts a melee when its key (or attack with a melee-type weapon, or a
/// dual wield's left trigger) is pressed and nothing holds the weapon.
pub(super) fn check_melee<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml, delay_done: bool) {
    let weapon = pm.ps.weapon;
    let dual = dual_wield(pm, weapon);
    let (right_button, left_button) = if dual && pm.cmd.fire_buttons_swapped {
        (buttons::THROW, buttons::ATTACK)
    } else {
        (buttons::ATTACK, buttons::THROW)
    };
    let cmd = pm.cmd.buttons;
    let old = pm.oldcmd.buttons;
    let mut right = cmd.held(buttons::MELEE) || cmd.held(right_button);
    let mut left = cmd.held(left_button);
    let ps = &*pm.ps;
    if ps.pm_flags & (pm_flags::LADDER | pm_flags::LADDER_FALL) != 0
        || ps.weap_flags & weap_flags::WEAPONS_DISABLED != 0
    {
        return;
    }
    let d = def(pm, weapon).def;
    let melee_type = d.weapon_type() == weapon_type::MELEE;
    let melee = melee_weapon(pm);
    let md = def(pm, melee);
    let s = ps.weaponstate;
    let left_gun = def(pm, weapon).dual_wield_weapon;
    if ps.e_flags & e_flags::VEHICLE_VIEW != 0
        || ps.vehicle_entity != ENTITYNUM_NONE
        || (ws::OFFHAND_INIT..=ws::OFFHAND_END).contains(&s)
        || ws::is_nightvision(s)
        || ws::is_dtp(s)
        || ws::is_riotshield(s)
        || ps.pm_flags & pm_flags::MANTLE != 0
        || (cmd.held(buttons::THROW)
            && dual
            && def(pm, left_gun).def.weapon_type() != weapon_type::MELEE)
    {
        return;
    }
    let triggered = (melee_type && cmd.held(buttons::ATTACK))
        || cmd.held(buttons::MELEE)
        || cmd.held(buttons::THROW);
    if triggered && cancel_grenade(pm) {
        return;
    }
    let ps = &*pm.ps;
    if ws::is_melee(ps.weaponstate) && (cmd.held(right_button) || cmd.held(buttons::MELEE)) {
        right = false;
    }
    if ws::is_melee(ps.weaponstate_left) && cmd.held(left_button) {
        left = false;
    }
    if !dual {
        left = false;
    }
    let pressed = |b: u32| cmd.held(b) && !old.held(b);
    let delay_free = ps.weapon_delay == 0 || ws::is_reloading(ps.weaponstate);
    let press = pressed(buttons::MELEE)
        || (dual && pressed(buttons::THROW))
        || (melee_type && pressed(buttons::ATTACK));
    if md.def.melee_damage() == 0 || delay_done || !delay_free || !press {
        return;
    }
    if ps.weapon_pos_frac > 0.0 && (d.ads_overlay_reticle() != 0 || variable_zoom(pm)) {
        return;
    }
    let ps = &*pm.ps;
    if !md.has_melee_anim()
        || ps.e_flags2 & e_flags2::MOUNTED != 0
        || ws::is_raising_or_dropping(ps.weaponstate)
        || !(right || left)
    {
        return;
    }
    melee_charge(pm, pml);
    melee_start(pm, right, left);
}

/// A melee lunge toward the client's target, when on the ground and free to.
fn melee_charge<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let target = pm.cmd.melee_charge_ent;
    let free = pm.ps.pm_flags & pm_flags::LAUNCHED == 0
        && i32::from(target) != ENTITYNUM_NONE
        && pm.ps.pm_type == pm_type::NORMAL
        && pm.ps.e_flags & e_flags::TURRET == 0
        && pm.ps.pm_flags & (pm_flags::MANTLE | pm_flags::LADDER) == 0
        && !in_air(pm, pml);
    let ps = &mut *pm.ps;
    if free {
        ps.pm_flags |= pm_flags::LAUNCHED;
        ps.melee_charge_time = 0;
        ps.melee_charge_ent = i32::from(target);
        ps.melee_charge_dist = i32::from(pm.cmd.melee_charge_dist);
        return;
    }
    ps.pm_flags &= !pm_flags::LAUNCHED;
    ps.melee_charge_dist = 0;
    ps.melee_charge_time = 0;
    ps.melee_charge_ent = ENTITYNUM_NONE;
    if tuning::BAYONET_LAUNCH_PROOF {
        ps.velocity[2] = ps.velocity[2].min(tuning::BAYONET_LAUNCH_Z_CAP);
    }
}

/// The melee starts on the hands asked: a swing, or a lunge when the target
/// is past melee range.
fn melee_start<W: MoveWorld>(pm: &mut Pm<'_, W>, right: bool, left: bool) {
    let melee = melee_weapon(pm);
    let w = def(pm, melee);
    let d = w.def;
    let range = if d.bayonet() {
        tuning::BAYONET_RANGE
    } else {
        tuning::MELEE_RANGE
    };
    let charge =
        pm.ps.pm_flags & pm_flags::LAUNCHED != 0 && range < pm.ps.melee_charge_dist as f32;
    let empty = inv::hand_clip_empty(pm.ps, pm.world);
    let ps = &mut *pm.ps;
    if charge {
        if left {
            ps.weapon_time_left = d.melee_charge_time();
            ps.weapon_delay_left = d.melee_charge_delay();
        } else {
            ps.weapon_time = d.melee_charge_time();
            ps.weapon_delay = d.melee_charge_delay();
        }
        start_anim(
            ps,
            if empty {
                weap_anim::MELEE_CHARGE_EMPTY
            } else {
                weap_anim::MELEE_CHARGE
            },
        );
    } else {
        if left {
            ps.weapon_time_left = d.melee_time();
            ps.weapon_delay_left = d.melee_delay();
            start_anim_left(ps, weap_anim::FIRE);
        }
        if right {
            ps.weapon_time = d.melee_time();
            ps.weapon_delay = d.melee_delay();
            start_anim(
                ps,
                if empty {
                    weap_anim::MELEE_EMPTY
                } else {
                    weap_anim::MELEE
                },
            );
        }
    }
    // The player animation events only; IW4L does not play them.
    let has_gun_model = d.has_gun_models();
    let event = if d.weapon_type() == weapon_type::RIOTSHIELD {
        Some(anim::MELEE_ATTACK)
    } else if has_gun_model && charge {
        Some(anim::KNIFE_MELEE_CHARGE)
    } else if left {
        Some(anim::MELEE_LEFT)
    } else {
        pm.anim_event(
            if has_gun_model {
                anim::KNIFE_MELEE
            } else {
                anim::MELEE_ATTACK
            },
            false,
            true,
        );
        None
    };
    if let Some(event) = event {
        pm.anim_event(event, false, true);
        if left {
            pm.ps.weaponstate_left = ws::MELEE_INIT;
        }
    }
    if right {
        pm.ps.weaponstate = ws::MELEE_INIT;
    }
    pm.event(event::MELEE_SWIPE, 0);
    hold_prone(pm.ps);
}

/// The swing lands.
pub(super) fn melee_fire<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    pm.event(event::FIRE_MELEE, 0);
    hold_prone(pm.ps);
}

/// After the swing: the held weapon comes back quickly if another weapon
/// swung; else the hand is ready.
pub(super) fn melee_end<W: MoveWorld>(pm: &mut Pm<'_, W>, left: bool) {
    let held = pm.ps.weapon;
    let d = def(pm, held).def;
    let swung = def(pm, melee_weapon(pm)).def;
    let hand = Hand { left };
    if !swung.same_def(&d) {
        let ps = &mut *pm.ps;
        hand.set_state(ps, ws::MELEE_END);
        hand.set_time(ps, d.quick_raise_time());
        hand.set_delay(ps, 0);
        if ps.water_level < 4 {
            start_anim(ps, weap_anim::QUICK_RAISE);
        }
        hold_prone(ps);
        return;
    }
    if !dual_wield(pm, held) {
        finish(pm.ps);
        return;
    }
    let ps = &mut *pm.ps;
    hand.set_time(ps, 0);
    hand.set_delay(ps, 0);
    hand.set_state(ps, ws::READY);
    if left {
        start_anim_left(ps, weap_anim::IDLE);
    } else {
        start_anim(ps, weap_anim::IDLE);
    }
}
