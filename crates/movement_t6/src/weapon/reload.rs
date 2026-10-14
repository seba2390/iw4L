//! Reloading: starting, segments, adding the rounds, the end, and fuel.

use super::{
    Hand, def, dual_wield, fire::charge_reset, fire_type, melee_rule, offscreen_reload, start_anim,
    start_anim_hand, start_anim_left, weap_anim, weapon_type,
};
use crate::events::{anim, event};
use crate::inventory as inv;
use crate::state::{
    buttons, e_flags, e_flags2, other_flags, pm_flags, weap_flags, weapon_state as ws,
};
use crate::{MoveWorld, Pm, Pml, tuning};

/// The clip is empty and the weapon fires bullets: the empty reload plays.
fn empty_bullets<W: MoveWorld>(pm: &Pm<'_, W>, weapon: u32) -> bool {
    inv::clip(pm.ps, pm.world, weapon) == 0
        && def(pm, weapon).def.weapon_type() == weapon_type::BULLET
}

/// When during a reload the rounds go in: the add time, capped by the
/// reload's own time; a bolt-action weapon waits for its bolt.
fn reload_add_delay<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let hand = Hand::of(pm.ps);
    let weapon = inv::hand_weapon(pm.ps, pm.world);
    let d = def(pm, weapon).def;
    let mut add = if ws::is_reload_start(hand.state(pm.ps)) {
        let t = d.reload_start_add_time();
        if t == 0 {
            0
        } else {
            t.min(d.reload_start_time())
        }
    } else {
        let quick = inv::dual_mag_in(pm.ps, pm.world, weapon);
        let base = if quick {
            d.reload_quick_add_time()
        } else {
            d.reload_add_time()
        };
        let (reload, add_at) = if empty_bullets(pm, weapon) {
            let add_at = if quick && d.reload_quick_empty_add_time() != 0 {
                d.reload_quick_empty_add_time()
            } else if d.reload_empty_add_time() != 0 {
                d.reload_empty_add_time()
            } else {
                base
            };
            (d.reload_empty_time(), add_at)
        } else {
            (d.reload_time(), base)
        };
        if add_at != 0 && add_at < reload {
            add_at
        } else {
            reload
        }
    };
    let ps = &mut *pm.ps;
    if d.bolt_action() && inv::needs_rechamber(ps, weapon) {
        if add == 0 {
            add = hand.time(ps);
        }
        add = add.min(d.rechamber_bolt_time());
        if add == 0 {
            add = 1;
        }
        hand.set_delay(ps, add);
    } else if add != 0 {
        hand.set_delay(ps, add);
    }
    let _ = offscreen_reload(pm);
}

/// The reload animation and time, by an empty or loaded clip and the
/// second magazine.
fn start_reload_anim<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let hand = Hand::of(pm.ps);
    let weapon = inv::hand_weapon(pm.ps, pm.world);
    let d = def(pm, weapon).def;
    if d.unlimited_ammo() {
        return;
    }
    let quick = inv::dual_mag_in(pm.ps, pm.world, weapon);
    let empty = empty_bullets(pm, weapon);
    if offscreen_reload(pm) {
        let ev = if empty {
            event::RELOAD_FROM_EMPTY
        } else {
            event::RELOAD
        };
        pm.event(ev, 0);
    } else {
        let (anim, time, ev) = match (empty, quick) {
            (true, true) => (
                weap_anim::RELOAD_QUICK_EMPTY,
                d.reload_quick_empty_time(),
                event::RELOAD_FROM_EMPTY,
            ),
            (true, false) => (
                weap_anim::RELOAD_EMPTY,
                d.reload_empty_time(),
                event::RELOAD_FROM_EMPTY,
            ),
            (false, true) => (
                weap_anim::RELOAD_QUICK,
                d.reload_quick_time(),
                event::RELOAD,
            ),
            (false, false) => (weap_anim::RELOAD, d.reload_time(), event::RELOAD),
        };
        let ps = &mut *pm.ps;
        start_anim_hand(ps, anim, ps.left_hand);
        hand.set_time(ps, time);
        pm.event(ev, 0);
    }
    let ps = &mut *pm.ps;
    let s = hand.state(ps);
    hand.set_state(
        ps,
        if s == ws::RELOAD_START_INTERRUPT {
            ws::RELOADING_INTERRUPT
        } else {
            ws::RELOADING
        },
    );
    reload_add_delay(pm);
}

/// Both guns of a dual-wield weapon are past half their reloads.
fn dual_reloads_half_done<W: MoveWorld>(pm: &Pm<'_, W>) -> bool {
    let ps = &*pm.ps;
    if !dual_wield(pm, ps.weapon) {
        return false;
    }
    let left = def(pm, ps.weapon).dual_wield_weapon;
    ps.weaponstate == ws::RELOADING
        && ps.weaponstate_left == ws::RELOADING
        && (def(pm, ps.weapon).def.reload_time() >> 1) < ps.weapon_time
        && (def(pm, left).def.reload_time() >> 1) < ps.weapon_time_left
}

/// The player animation for a reload: not for a clip-only weapon, and for a
/// dual wield only once both guns are half done.
fn reload_anim_event<W: MoveWorld>(pm: &mut Pm<'_, W>, weapon: u32) {
    let clip_only = def(pm, weapon).def.clip_only();
    if clip_only {
        return;
    }
    if !dual_wield(pm, weapon) || dual_reloads_half_done(pm) {
        pm.anim_event(anim::RELOAD, false, true);
    }
}

/// Starts the hand's reload from a state it may reload from.
pub(super) fn begin_reload<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let _ = pml;
    let hand = Hand::of(pm.ps);
    let weapon = inv::hand_weapon(pm.ps, pm.world);
    let d = def(pm, weapon).def;
    let s = hand.state(pm.ps);
    let from = matches!(
        s,
        ws::READY
            | ws::FIRING
            | ws::CONT_FIRE_LOOP
            | ws::RECHAMBERING
            | ws::RECHAMBER_END
            | ws::CRAWL_FORWARD..=ws::CRAWL_LEFT
    ) || ws::is_sprint(s)
        || ws::is_lowready(s);
    if !from
        || !super::some(weapon)
        || !pm.world.weapon_exists(weapon)
        || pm.ps.e_flags2 & e_flags2::MOUNTED != 0
        || super::pose::mantling_anim(pm.ps)
    {
        return;
    }
    hand.set_shot_count(pm.ps, 0);
    pm.event(event::RESET_ADS, 0);
    pm.event(event::RELOAD_START_NOTIFY, 0);
    if d.fire_type() == fire_type::CHARGE_SHOT {
        charge_reset(pm.ps);
    }
    if offscreen_reload(pm) {
        let ps = &mut *pm.ps;
        start_anim_hand(ps, weap_anim::DROP, ps.left_hand);
        hand.set_time(ps, d.drop_time());
        hand.set_state(ps, ws::RELOAD_START);
        pm.event(event::RELOAD_START, 0);
    } else if d.reload_start_time() == 0 || !d.segmented_reload() {
        start_reload_anim(pm);
    } else {
        let rechamber = tuning::ENFORCE_RECHAMBER_ON_SWITCH
            && d.bolt_action()
            && inv::needs_rechamber(pm.ps, weapon);
        let ps = &mut *pm.ps;
        start_anim_hand(ps, weap_anim::RELOAD_START, ps.left_hand);
        hand.set_time(ps, d.reload_start_time());
        hand.set_state(
            ps,
            if rechamber {
                ws::RELOAD_START_RECHAMBER
            } else {
                ws::RELOAD_START
            },
        );
        pm.event(event::RELOAD_START, 0);
        reload_add_delay(pm);
    }
    reload_anim_event(pm, weapon);
}

/// Moves rounds from beside the clip into it: as many as fit, at most the
/// reload's (or the start's) add.
fn transfer_ammo<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let weapon = inv::hand_weapon(pm.ps, pm.world);
    let d = def(pm, weapon).def;
    let start_add = d.reload_start_add();
    let start = ws::is_reload_start(Hand::of(pm.ps).state(pm.ps));
    if start && start_add == 0 {
        return;
    }
    let stock = inv::stock(pm.ps, pm.world, weapon);
    let mut in_clip = inv::clip(pm.ps, pm.world, weapon);
    if d.fuel_tank_weapon() {
        in_clip = 0;
        inv::set_clip(pm.ps, pm.world, weapon, 0);
    }
    let size = inv::clip_size(pm.world, weapon);
    let mut n = (size - in_clip).min(stock);
    if start {
        if start_add < size && start_add < n {
            n = start_add;
        }
    } else {
        let add = d.reload_ammo_add();
        if add != 0 && add < size && add < n && !offscreen_reload(pm) {
            n = add;
        }
    }
    if n == 0 {
        return;
    }
    inv::add_stock(pm.ps, pm.world, weapon, -n);
    inv::add_clip(pm.ps, pm.world, weapon, n);
    pm.ps.e_flags2 &= !e_flags2::CLIP_EMPTY;
    if d.fuel_tank_weapon() {
        let held = pm.ps.weapon;
        inv::set_fuel(pm.ps, held, 0);
    }
    pm.event(event::RELOAD_ADDAMMO, 0);
}

/// The reload's delay ran out: the rounds go in, or a bolt-action weapon
/// works its bolt first; a dual magazine flips.
fn reload_add_ammo<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let hand = Hand::of(pm.ps);
    let weapon = inv::hand_weapon(pm.ps, pm.world);
    if !super::some(weapon) {
        return;
    }
    let d = def(pm, weapon).def;
    if d.bolt_action() {
        if d.shots_before_rechamber() != 0 {
            pm.ps.shots_fired_from_chamber = 0;
        }
        if inv::needs_rechamber(pm.ps, weapon) {
            if let Some(h) = inv::held_mut(pm.ps, weapon) {
                h.needs_rechamber = false;
            }
            if d.fire_type() == fire_type::CHARGE_SHOT {
                charge_reset(pm.ps);
            }
            pm.event(event::EJECT_BRASS, 0);
            let start = ws::is_reload_start(hand.state(pm.ps));
            if start && d.reload_start_add_time() == 0 {
                return;
            }
            if hand.time(pm.ps) != 0 {
                let t = if start {
                    d.reload_start_add_time().min(d.reload_start_time())
                } else {
                    let mut add = d.reload_add_time();
                    let reload = if empty_bullets(pm, weapon) {
                        if d.reload_empty_add_time() != 0 {
                            add = d.reload_empty_add_time();
                        }
                        d.reload_empty_time()
                    } else {
                        d.reload_time()
                    };
                    if add != 0 && add < reload {
                        add
                    } else {
                        reload
                    }
                };
                let bolt = d.rechamber_bolt_time();
                let after = t - if t <= bolt { 1 } else { bolt };
                if 0 < after {
                    hand.set_delay(pm.ps, after);
                    return;
                }
            }
            transfer_ammo(pm);
            return;
        }
    }
    transfer_ammo(pm);
    let held = pm.ps.weapon;
    let dual_mag = def(pm, held).def.dual_mag();
    let quick = inv::dual_mag_in(pm.ps, pm.world, held);
    if dual_mag {
        if let Some(h) = inv::held_mut(pm.ps, held) {
            h.dual_mag = !quick;
        }
    }
}

/// The clip can take more: rounds beside it, room in it (or a fuel tank to
/// refill), and a no-partial-reload weapon only when a full add fits.
fn can_reload<W: MoveWorld>(pm: &Pm<'_, W>) -> bool {
    let weapon = inv::hand_weapon(pm.ps, pm.world);
    let d = def(pm, weapon).def;
    let ps = &*pm.ps;
    let fuel = d.fuel_tank_weapon() && inv::fuel(ps, weapon) > 0;
    let clip = inv::clip(ps, pm.world, weapon);
    let size = inv::clip_size(pm.world, weapon);
    if inv::stock(ps, pm.world, weapon) == 0 || !(clip < size || fuel) {
        return false;
    }
    if d.no_partial_reload() {
        let add = d.reload_ammo_add();
        if add != 0 && add < size {
            return add <= size - clip;
        }
        if clip != 0 {
            return false;
        }
    }
    true
}

/// A segmented reload's start: on to the next segment, or the reload's end.
pub(super) fn reload_start_state<W: MoveWorld>(pm: &mut Pm<'_, W>, delay_done: bool) {
    let weapon = pm.ps.weapon;
    let d = def(pm, weapon).def;
    if delay_done {
        reload_add_ammo(pm);
    }
    if pm.ps.weapon_time != 0 {
        return;
    }
    if pm.cmd.buttons.held(buttons::ATTACK) && d.segmented_reload() {
        pm.ps.weaponstate = ws::RELOAD_START_INTERRUPT;
    }
    if (pm.ps.weaponstate != ws::RELOAD_START_INTERRUPT || inv::clip(pm.ps, pm.world, weapon) == 0)
        && can_reload(pm)
    {
        start_reload_anim(pm);
        return;
    }
    if let Some(h) = inv::held_mut(pm.ps, weapon) {
        h.needs_rechamber = false;
    }
    let dual = dual_wield(pm, weapon);
    let ps = &mut *pm.ps;
    if d.reload_end_time() == 0 {
        ps.weaponstate = ws::READY;
        start_anim(ps, weap_anim::IDLE);
        if dual {
            start_anim_left(ps, weap_anim::IDLE);
        }
        return;
    }
    ps.weaponstate = ws::RELOAD_END;
    start_anim(ps, weap_anim::RELOAD_END);
    ps.weapon_time = d.reload_end_time();
    pm.event(event::RELOAD_END, 0);
}

/// Reloading: when the time runs out, a segmented reload loads the next
/// segment or ends; any other is done.
pub(super) fn reloading<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml, delay_done: bool) {
    let _ = pml;
    let hand = Hand::of(pm.ps);
    let weapon = inv::hand_weapon(pm.ps, pm.world);
    let d = def(pm, weapon).def;
    if delay_done {
        reload_add_ammo(pm);
    }
    if hand.time(pm.ps) != 0 {
        return;
    }
    let segmented = d.segmented_reload();
    if segmented && pm.cmd.buttons.held(buttons::ATTACK) {
        hand.set_state(pm.ps, ws::RELOADING_INTERRUPT);
    }
    let held = pm.ps.weapon;
    let ps = &mut *pm.ps;
    if let Some(h) = inv::held_mut(ps, held) {
        h.needs_rechamber = false;
    }
    if d.bolt_action() && d.shots_before_rechamber() != 0 {
        ps.shots_fired_from_chamber = 0;
    }
    if d.fire_type() == fire_type::CHARGE_SHOT {
        charge_reset(ps);
    }
    let end_time = if offscreen_reload(pm) {
        let ps = &mut *pm.ps;
        hand.set_state(ps, ws::RELOAD_END);
        start_anim(ps, weap_anim::RAISE);
        d.raise_time()
    } else {
        if !segmented {
            let ps = &mut *pm.ps;
            hand.set_state(ps, ws::READY);
            start_anim_hand(ps, weap_anim::IDLE, ps.left_hand);
            return;
        }
        if hand.state(pm.ps) != ws::RELOADING_INTERRUPT && can_reload(pm) {
            start_reload_anim(pm);
            return;
        }
        let ps = &mut *pm.ps;
        if d.reload_end_time() == 0 {
            hand.set_state(ps, ws::READY);
            start_anim_hand(ps, weap_anim::IDLE, ps.left_hand);
            return;
        }
        hand.set_state(ps, ws::RELOAD_END);
        start_anim(ps, weap_anim::RELOAD_END);
        d.reload_end_time()
    };
    hand.set_time(pm.ps, end_time);
    pm.event(event::RELOAD_END, 0);
}

/// A fuel tank weapon burns while firing; an empty tank empties the clip.
pub(super) fn burn_fuel<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let held = pm.ps.weapon;
    let d = def(pm, held).def;
    let hand = Hand::of(pm.ps);
    if !d.fuel_tank_weapon() || !ws::is_firing(hand.state(pm.ps)) {
        return;
    }
    let ps = &mut *pm.ps;
    let fuel = inv::fuel(ps, held) + pml.msec;
    inv::set_fuel(ps, held, fuel);
    let life = d.tank_life_time();
    if life < inv::fuel(ps, held) {
        inv::set_fuel(ps, held, life);
        inv::set_clip(ps, pm.world, held, 0);
        hand.set_state(ps, ws::READY);
    }
}

/// The weapon may reload on its own now; a reload start plays the player's
/// reload animation instead.
fn may_auto_reload<W: MoveWorld>(pm: &mut Pm<'_, W>, state: i32) -> bool {
    let weapon = inv::hand_weapon(pm.ps, pm.world);
    let d = def(pm, weapon).def;
    match state {
        ws::RAISING..=ws::DROPPING_ALT
        | ws::RELOADING
        | ws::RELOADING_INTERRUPT
        | ws::RELOAD_END
        | ws::SWIM_IN => false,
        // The game plays the player's reload animation here when a movement
        // field IW4L has not named is set; player animations are not played.
        ws::RELOAD_START | ws::RELOAD_START_RECHAMBER | ws::RELOAD_START_INTERRUPT => false,
        _ => {
            let ps = &*pm.ps;
            !matches!(state, ws::FIRING | ws::CONT_FIRE_LOOP)
                && !ws::is_sprint(state)
                && !matches!(state, ws::DEPLOYING | ws::DEPLOYED | ws::BREAKING_DOWN)
                && ps.e_flags & e_flags::TURRET == 0
                && !d.unlimited_ammo()
                && (!d.no_ads_auto_reload() || ps.weapon_pos_frac != 1.0)
        }
    }
}

/// Reloads when asked (the key, a script, the left gun) or when the clip
/// runs dry with rounds left; a segmented reload is interrupted by a fresh
/// fire press; an empty weapon comes back from a melee raising.
pub(super) fn check_reload<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let hand = Hand::of(pm.ps);
    let weapon = inv::hand_weapon(pm.ps, pm.world);
    let d = def(pm, weapon).def;
    let dual = dual_wield(pm, weapon);
    if !dual {
        pm.ps.weap_flags &= !weap_flags::RELOAD_LEFT;
    }
    if hand.state(pm.ps) == ws::MELEE_END
        && inv::hand_clip_empty(pm.ps, pm.world)
        && !melee_rule(pm, weapon)
    {
        let ps = &mut *pm.ps;
        hand.set_state(ps, ws::RAISING);
        hand.set_time(ps, d.empty_raise_time());
        start_anim(ps, weap_anim::EMPTY_RAISE);
        return;
    }
    let ps = &*pm.ps;
    let s = hand.state(ps);
    if (ws::OFFHAND_INIT..=ws::OFFHAND_END).contains(&s)
        || ws::is_melee(s)
        || ws::is_dtp(s)
        || ws::is_riotshield(s)
        || ps.pm_flags & pm_flags::MANTLE != 0
        || (d.weapon_type() == weapon_type::MELEE && dual)
        || ps.water_level >= 4
    {
        return;
    }
    let mut asked = false;
    if ps.pm_flags & pm_flags::FROZEN == 0 && ps.other_flags & other_flags::CONTROLS_LOCKED == 0 {
        let key = pm.cmd.buttons.held(buttons::RELOAD);
        let ps = &mut *pm.ps;
        let scripted = ps.weap_flags & weap_flags::RELOAD_REQUESTED != 0;
        if scripted {
            ps.weap_flags &= !weap_flags::RELOAD_REQUESTED;
        }
        asked = scripted || key;
        if ps.weap_flags & weap_flags::RELOAD_LEFT != 0 && ps.left_hand {
            ps.weap_flags &= !weap_flags::RELOAD_LEFT;
            asked = true;
        }
    }
    let s = hand.state(pm.ps);
    if matches!(
        s,
        ws::RELOAD_START | ws::RELOAD_START_RECHAMBER | ws::RELOADING
    ) && d.segmented_reload()
        && pm.cmd.buttons.held(buttons::ATTACK)
        && !pm.oldcmd.buttons.held(buttons::ATTACK)
    {
        if s != ws::RELOADING && d.reload_start_time() != 0 {
            let start = d.reload_start_time();
            if 0.4 < (start - hand.time(pm.ps)) as f32 / start as f32 {
                hand.set_state(pm.ps, ws::RELOAD_START_INTERRUPT);
            }
        } else if s == ws::RELOADING {
            hand.set_state(pm.ps, ws::RELOADING_INTERRUPT);
        }
    }
    let mut reload = asked && can_reload(pm);
    let s = hand.state(pm.ps);
    let held = pm.ps.weapon;
    if d.fuel_tank_weapon()
        && inv::stock(pm.ps, pm.world, weapon) != 0
        && d.tank_life_time() <= inv::fuel(pm.ps, held)
        && may_auto_reload(pm, s)
    {
        reload = true;
    }
    if reload
        || (inv::clip(pm.ps, pm.world, weapon) == 0
            && inv::stock(pm.ps, pm.world, weapon) != 0
            && may_auto_reload(pm, s))
    {
        begin_reload(pm, pml);
    }
}
