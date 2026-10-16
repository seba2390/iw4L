//! Switching weapons: putting the held one away, raising the next (first,
//! quick, empty or alternate), and losing an empty clip-only weapon.

use super::{
    def, dual_wield, finish, fire::charge_reset, fire::keep_firing, fire_button, fire_type,
    hold_prone, in_vehicle, inventory_type, offhand_slot, some, start_anim, start_anim_both,
    start_anim_left, weap_anim, weapon_class, weapon_type,
};
use crate::events::{anim, event};
use crate::inventory as inv;
use crate::state::{
    PlayerState, e_flags, e_flags2, pm_flags, pm_type, weap_flags, weapon_state as ws,
};
use crate::{Gap, MoveWorld, Pm, Pml, tuning};

/// The weapon may be picked: one the game loaded and the player holds.
fn selectable<W: MoveWorld>(pm: &Pm<'_, W>, weapon: u32) -> bool {
    pm.world.weapon_exists(weapon) && inv::held(pm.ps, weapon).is_some()
}

/// Switching to the alternate (attachment) mode of the held weapon.
pub(super) fn alt_switch<W: MoveWorld>(pm: &Pm<'_, W>, from: u32, to: u32) -> bool {
    if from == 0 || !some(from) || to == 0 || !some(to) {
        return false;
    }
    let w = def(pm, from);
    to == w.alt_weapon
        || (w.def.inventory_type() == inventory_type::ALT_MODE
            && pm.ps.last_weapon_alt_mode_switch == to)
}

/// An alternate pair aimed only: switching between them raises nothing.
fn alt_ads_only_pair<W: MoveWorld>(pm: &Pm<'_, W>, a: u32, b: u32) -> bool {
    (def(pm, a).alt_weapon == b || def(pm, b).alt_weapon == a)
        && def(pm, a).def.alt_weapon_ads_only()
        && def(pm, b).def.alt_weapon_ads_only()
}

/// The switch drops and raises quickly.
fn quick_switch<W: MoveWorld>(pm: &Pm<'_, W>, from: u32, to: u32, state: i32) -> bool {
    let a = def(pm, from).def;
    let b = def(pm, to).def;
    let mut quick = state == ws::DROPPING_QUICK;
    if !some(from) && b.weapon_type() == weapon_type::PROJECTILE {
        quick = true;
    }
    if a.can_use_in_vehicle() || b.can_use_in_vehicle() {
        quick = true;
    }
    if b.offhand_slot() != 0 || a.offhand_slot() != 0 {
        quick = true;
    }
    if some(to)
        && matches!(
            b.weapon_class(),
            weapon_class::PISTOL | weapon_class::PISTOL_SPREAD | weapon_class::MELEE
        )
    {
        return true;
    }
    quick
}

/// Puts the held weapon (or the offhand in hand) away to switch to `to`.
fn begin_weapon_change<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml, to: u32, quick: bool) {
    let _ = pml;
    if some(to) && inv::held(pm.ps, to).is_none() {
        return;
    }
    let s = pm.ps.weaponstate;
    if matches!(s, ws::DROPPING | ws::DROPPING_QUICK | ws::DROPPING_ALT) {
        return;
    }
    if ws::is_reloading(s) {
        pm.event(event::STOP_WEAPON_SOUND, s);
    }
    let ps = &mut *pm.ps;
    ps.weap_flags &= !weap_flags::OFFHAND_EMPTY_HANDED;
    ps.weapon_delay = 0;
    let from = if (ws::OFFHAND_INIT..=ws::OFFHAND_END).contains(&ps.weaponstate) {
        ps.offhand_index
    } else {
        ps.weapon
    };
    let b = def(pm, to).def;
    let a = def(pm, from).def;
    let ps = &mut *pm.ps;
    if b.weapon_type() == weapon_type::RIOTSHIELD {
        ps.weap_flags |= weap_flags::SWITCHING_TO_RIOTSHIELD;
    } else {
        ps.weap_flags &= !weap_flags::SWITCHING_TO_RIOTSHIELD;
    }
    if a.fire_type() == fire_type::CHARGE_SHOT {
        charge_reset(ps);
    }
    if b.can_use_in_vehicle() && a.can_use_in_vehicle() {
        return;
    }
    if a.offhand_slot() != offhand_slot::EQUIPMENT {
        if !some(from) || inv::held(ps, from).is_none() || 0 < ps.grenade_time_left {
            ps.weap_flags &= !weap_flags::OFFHAND_ACTIVE;
            ps.weapon_time = 0;
            ps.grenade_time_left = 0;
            ps.weaponstate = if quick {
                ws::DROPPING_QUICK
            } else {
                ws::DROPPING
            };
            hold_prone(ps);
            return;
        }
    } else {
        ps.weap_flags &= !weap_flags::OFFHAND_ACTIVE;
    }
    ps.ads_zoom_select = 0;
    ps.ads_zoom_time = 0;
    let empty = inv::hand_clip_empty(pm.ps, pm.world);
    let alt = alt_switch(pm, from, to);
    pm.ps.grenade_time_left = 0;
    if pm.ps.weap_flags & weap_flags::OFFHAND_SWITCH == 0 {
        let held = pm.ps.weapon as i32;
        pm.event(event::WEAPON_SWITCH_STARTED, held);
    }
    let keep_up = pm.ps.weaponstate == ws::FIRING
        && a.weapon_type() == weapon_type::GRENADE
        && a.offhand_slot() == offhand_slot::SPECIFIC_USE;
    if alt {
        pm.event(event::WEAPON_ALT, 0);
        start_anim(pm.ps, weap_anim::ALT_DROP);
    } else if !keep_up {
        pm.event(event::PUTAWAY_WEAPON, 0);
        if !a.no_drops_or_raises() {
            let anim = if !empty || (quick && !a.no_quick_drop_when_empty()) {
                if quick {
                    weap_anim::QUICK_DROP
                } else {
                    weap_anim::DROP
                }
            } else {
                weap_anim::EMPTY_DROP
            };
            start_anim(pm.ps, anim);
            if dual_wield(pm, from) {
                start_anim_left(pm.ps, anim);
            }
        }
    }
    if tuning::ENFORCE_RECHAMBER_ON_SWITCH
        && a.bolt_action()
        && pm.ps.weaponstate == ws::RELOAD_START_RECHAMBER
        && let Some(h) = inv::held_mut(pm.ps, from)
    {
        h.needs_rechamber = true;
    }
    let ps = &mut *pm.ps;
    ps.weaponstate = if alt {
        ws::DROPPING_ALT
    } else if quick {
        ws::DROPPING_QUICK
    } else {
        ws::DROPPING
    };
    ps.weaponstate_left = ps.weaponstate;
    hold_prone(ps);
    ps.weapon_time = if a.no_drops_or_raises() {
        0
    } else if alt {
        a.alt_drop_time()
    } else if quick {
        a.quick_drop_time()
    } else if empty {
        a.empty_drop_time()
    } else {
        a.drop_time()
    };
    if super::pose::mantle_puts_away(ps) && ps.mantle_flags & crate::mantle::flag::FAST != 0 {
        match crate::mantle::drop_time_cap(pm) {
            Some(cap) => pm.ps.weapon_time = cap.min(pm.ps.weapon_time),
            None => pm.gap(Gap::Mantle),
        }
    }
    let ps = &*pm.ps;
    if !matches!(
        ps.pm_type,
        pm_type::LAST_STAND | pm_type::LAST_STAND_REVIVED | pm_type::LAST_STAND_GETTING_UP
    ) && ps.view_height_current == ps.view_height_target as f32
        && !alt
        && ps.pm_flags & (pm_flags::SPRINTING | pm_flags::MANTLE) == 0
        && b.inventory_type() != inventory_type::OFFHAND
        && a.offhand_slot() != offhand_slot::EQUIPMENT
    {
        pm.anim_event(anim::DROP_WEAPON, false, false);
    }
}

/// A clip-only weapon with no rounds left (not a detonator, plantable or
/// the inventory weapon) is thrown away.
fn drop_if_empty<W: MoveWorld>(pm: &mut Pm<'_, W>, weapon: u32) {
    if !some(weapon) || inv::held(pm.ps, weapon).is_none() {
        return;
    }
    let d = def(pm, weapon).def;
    if d.clip_only()
        && inv::clip(pm.ps, pm.world, weapon) == 0
        && inv::stock(pm.ps, pm.world, weapon) == 0
        && !d.has_detonator()
        && !d.plantable()
        && weapon != pm.ps.inventory_weapon
    {
        take_weapon(pm, weapon);
    }
}

/// Empties a weapon's clip and rounds and its held slots.
fn remove_weapon<W: MoveWorld>(pm: &mut Pm<'_, W>, weapon: u32) {
    inv::clear_rounds(pm.ps, pm.world, weapon);
    for h in &mut pm.ps.held_weapons {
        if h.weapon == weapon {
            h.weapon = 0;
        }
    }
}

/// The player loses a weapon, its alternates and its left gun.
pub(crate) fn take_weapon<W: MoveWorld>(pm: &mut Pm<'_, W>, weapon: u32) -> bool {
    if inv::held(pm.ps, weapon).is_none() {
        return false;
    }
    remove_weapon(pm, weapon);
    let ps = &mut *pm.ps;
    if ps.weapon == weapon {
        ps.weapon = 0;
    }
    if ps.melee_weapon == weapon {
        ps.melee_weapon = 0;
    }
    let mut alt = weapon;
    loop {
        alt = def(pm, alt).alt_weapon;
        if !some(alt) || inv::held(pm.ps, alt).is_none() {
            break;
        }
        remove_weapon(pm, alt);
        if pm.ps.weapon == weapon {
            pm.ps.weapon = 0;
        }
    }
    let left = def(pm, weapon).dual_wield_weapon;
    if some(left) && inv::held(pm.ps, left).is_some() {
        remove_weapon(pm, left);
        if pm.ps.weapon == weapon {
            pm.ps.weapon = 0;
        }
    }
    let airburst = def(pm, weapon).def.airburst_weapon();
    let ps = &mut *pm.ps;
    if matches!(
        ps.weaponstate,
        ws::DEPLOYING | ws::DEPLOYED | ws::BREAKING_DOWN
    ) {
        ps.e_flags &= !e_flags::TURRET;
        finish(ps);
    }
    if airburst {
        ps.airburst_distance = 0;
    }
    true
}

/// Every held weapon counts as raised once (after the first raise).
fn mark_all_used(ps: &mut PlayerState) {
    for h in &mut ps.held_weapons {
        if some(h.weapon) {
            h.used_before = true;
        }
    }
}

/// The weapon is away: the one asked for comes up (or none): first raise,
/// quick, empty or alternate as fits.
pub(super) fn finish_weapon_change<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml, state: i32) {
    let _ = pml;
    let ps = &*pm.ps;
    let mut to = 0;
    if !super::pose::mantle_puts_away(ps) {
        let asked = pm.cmd.weapon;
        if ps.weap_flags & weap_flags::OFFHAND_SWITCH == 0
            && ps.pm_flags & pm_flags::LADDER == 0
            && inv::held(ps, asked).is_some()
            && ps.weap_flags & weap_flags::WEAPONS_DISABLED == 0
            && (!in_vehicle(ps) || def(pm, asked).def.can_use_in_vehicle())
        {
            to = asked;
            if !pm.world.weapon_exists(to) {
                to &= !0xff;
            }
        }
    }
    if inv::held(pm.ps, to).is_none() {
        to &= !0xff;
    }
    let from = pm.ps.weapon;
    pm.ps.weapon = to;
    let mut alt = if some(to) { pm.cmd.alt_mode_weapon } else { 0 };
    if !pm.world.weapon_exists(alt) || inv::held(pm.ps, alt).is_none() {
        alt = 0;
    }
    pm.ps.last_weapon_alt_mode_switch = alt;
    if def(pm, pm.ps.weapon).def.inventory_type() == inventory_type::ALT_MODE && !some(alt) {
        pm.ps.weapon &= !0xff;
    }
    let held = pm.ps.weapon;
    let w = def(pm, held);
    let d = w.def;
    let ps = &mut *pm.ps;
    ps.ads_zoom_select = 0;
    ps.ads_zoom_time = 0;
    if ps.weap_flags & weap_flags::OFFHAND_SWITCH != 0 {
        ps.weaponstate = ws::OFFHAND_INIT;
        return;
    }
    if from == to {
        ps.weaponstate = ws::READY;
        ps.weaponstate_left = ws::READY;
        start_anim(ps, weap_anim::IDLE);
        if dual_wield(pm, held) {
            start_anim_left(pm.ps, weap_anim::IDLE);
        }
        return;
    }
    let first = !inv::held(ps, held).is_some_and(|h| h.used_before)
        && ps.pm_type != pm_type::LAST_STAND_GETTING_UP
        && ps.pm_type != pm_type::LAST_STAND;
    if let Some(h) = inv::held_mut(ps, held) {
        h.used_before = true;
    }
    let alt_raise = alt_switch(pm, from, to);
    pm.ps.e_flags2 &= !e_flags2::CLIP_EMPTY;
    let no_raise = d.no_drops_or_raises();
    let (anim, time, spread) = if alt_raise {
        let spread = pm.ps.aim_spread_scale.max(128.0);
        let left = pm.ps.left_hand;
        if !pm.cmd.buttons.held(fire_button(pm, held, left)) {
            pm.ps.weapon_shot_count = 0;
        }
        (weap_anim::ALT_RAISE, d.alt_raise_time(), spread)
    } else {
        let quick = quick_switch(pm, from, to, state);
        let empty = inv::hand_clip_empty(pm.ps, pm.world);
        if empty {
            pm.ps.e_flags2 |= e_flags2::CLIP_EMPTY;
        }
        let (a, t) = if no_raise {
            (0, 0)
        } else if empty {
            (weap_anim::EMPTY_RAISE, d.empty_raise_time())
        } else if first {
            mark_all_used(pm.ps);
            (weap_anim::FIRST_RAISE, d.first_raise_time())
        } else if quick {
            (weap_anim::QUICK_RAISE, d.quick_raise_time())
        } else {
            (weap_anim::RAISE, d.raise_time())
        };
        (a, t, 255.0)
    };
    if no_raise {
        let ps = &mut *pm.ps;
        ps.weaponstate = anim as i32;
        ps.weapon_time = time;
        ps.aim_spread_scale = spread;
        hold_prone(ps);
        start_anim(ps, weap_anim::IDLE);
        if dual_wield(pm, held) {
            start_anim_left(pm.ps, weap_anim::IDLE);
        }
    } else if !alt_ads_only_pair(pm, from, to) {
        let ps = &mut *pm.ps;
        ps.weapon_time = time;
        ps.aim_spread_scale = spread;
        ps.weaponstate = if alt_raise {
            ws::RAISING_ALT
        } else {
            ws::RAISING
        };
        hold_prone(ps);
        start_anim_both(ps, anim);
    }
    drop_if_empty(pm, from);
    if !some(from) || alt_raise {
        pm.event(event::RAISE_PICKUP_WEAPON, 0);
        if first {
            pm.anim_event(anim::FIRST_RAISE_WEAPON, false, false);
        }
        return;
    }
    pm.event(
        if first {
            event::FIRST_RAISE_WEAPON
        } else {
            event::RAISE_WEAPON
        },
        0,
    );
    let ps = &*pm.ps;
    if matches!(
        ps.pm_type,
        pm_type::LAST_STAND | pm_type::LAST_STAND_REVIVED | pm_type::LAST_STAND_GETTING_UP
    ) || ps.pm_flags & (pm_flags::LADDER_FALL | pm_flags::SPRINTING | pm_flags::LADDER) != 0
        || ps.view_height_current != ps.view_height_target as f32
    {
        return;
    }
    pm.anim_event(
        if first {
            anim::FIRST_RAISE_WEAPON
        } else {
            anim::RAISE_WEAPON
        },
        false,
        false,
    );
}

/// Switches to the weapon the command asks for when the weapon is free to;
/// puts it away for a mantle, a ladder, an offhand throw or disabled
/// weapons; breaks a turret down.
pub(super) fn check_change_weapon<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let ps = &*pm.ps;
    if ps.water_level > 3 {
        return;
    }
    let s = ps.weaponstate;
    if ws::is_melee(s)
        || ws::is_dtp(s)
        || ws::is_nightvision(s)
        || ws::is_riotshield(s)
        || ps.e_flags2 & e_flags2::MOUNTED != 0
        || ps.weap_flags & weap_flags::FIRING_GUIDED != 0
        || ps.link_flags & 4 != 0
        || super::pose::mantling_anim(ps)
    {
        return;
    }
    let offhand = if ps.offhand_index != 0 {
        Some(def(pm, ps.offhand_index).def.offhand_slot())
    } else {
        None
    };
    if ps.weapon_time != 0
        && !ws::is_reloading(s)
        && !matches!(s, ws::RECHAMBERING | ws::RECHAMBER_END)
        && (matches!(s, ws::FIRING | ws::CONT_FIRE_LOOP) || ws::is_melee(s) || ps.weapon_delay != 0)
    {
        return;
    }
    let held = ps.weapon;
    if super::pose::mantle_puts_away(ps)
        || ps.pm_flags & pm_flags::LADDER != 0
        || ps.weap_flags & weap_flags::OFFHAND_SWITCH != 0
    {
        if some(held) {
            begin_weapon_change(pm, pml, 0, true);
        }
        return;
    }
    if ps.weap_flags & weap_flags::WEAPONS_DISABLED != 0 {
        if some(held) {
            begin_weapon_change(pm, pml, 0, false);
        }
        return;
    }
    let asked = pm.cmd.weapon;
    let frozen_with_weapon =
        ps.pm_flags & (pm_flags::RESPAWNED | pm_flags::FROZEN) != 0 && some(held);
    if in_vehicle(ps) {
        if !some(held) && held == asked {
            return;
        }
        if !def(pm, asked).def.can_use_in_vehicle() {
            if some(held) {
                begin_weapon_change(pm, pml, 0, false);
            }
            return;
        }
        let ps = &*pm.ps;
        if held == asked
            || ps.weaponstate == ws::DEPLOYING
            || frozen_with_weapon
            || ps.other_flags & crate::state::other_flags::CONTROLS_LOCKED != 0
            || (some(asked) && !selectable(pm, asked))
        {
            return;
        }
        let quick = ps.mantle_flags & 0x100 != 0;
        begin_weapon_change(pm, pml, asked, quick);
        return;
    }
    let offhand_waits = (ws::OFFHAND_INIT..=ws::OFFHAND_END).contains(&s)
        && (offhand != Some(offhand_slot::EQUIPMENT)
            || !pm.cmd.buttons.held(crate::state::buttons::CANCEL));
    if held == asked
        || s == ws::DEPLOYING
        || frozen_with_weapon
        || (some(asked) && !selectable(pm, asked))
        || offhand_waits
    {
        if held == asked && matches!(s, ws::DROPPING | ws::DROPPING_QUICK | ws::DROPPING_ALT) {
            finish(pm.ps);
            start_anim(pm.ps, weap_anim::QUICK_IDLE);
            if s == ws::DROPPING_ALT
                && def(pm, held).def.alt_weapon_disable_switching()
                && keep_firing(pm, pm.ps.weaponstate, held)
            {
                pm.ps.weapon_time = 1;
            }
            return;
        }
        if some(held) && inv::held(pm.ps, held).is_none() {
            begin_weapon_change(pm, pml, 0, false);
        }
        return;
    }
    match s {
        ws::DEPLOYING | ws::BREAKING_DOWN => return,
        ws::DEPLOYED => {
            if pm.ps.weap_flags & weap_flags::BREAKDOWN == 0 {
                pm.ps.e_flags &= !e_flags::TURRET;
            }
            return;
        }
        _ => {}
    }
    if pm.ps.e_flags & e_flags::TURRET != 0 {
        pm.ps.weap_flags |= weap_flags::BREAKDOWN;
        pm.event(event::WEAPON_BREAKING_DOWN, 0);
        return;
    }
    let quick = pm.ps.mantle_flags & 0x100 != 0
        || pm.ps.weap_flags & weap_flags::OFFHAND_EMPTY_HANDED != 0
        || quick_switch(pm, held, asked, s);
    begin_weapon_change(pm, pml, asked, quick);
}
