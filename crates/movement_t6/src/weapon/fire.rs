//! Weapon timers, bursts, firing and the shot's timing, ammo and events,
//! charge shots and rechambering.

use super::{
    Hand, def, dual_wield, fire_button, fire_type, hold_prone, perks_with, some, start_anim,
    start_anim_both, start_anim_hand, start_anim_unless_playing, weap_anim, weapon_type,
};
use crate::events::{anim, event};
use crate::inventory as inv;
use crate::state::{
    ENTITYNUM_NONE, ENTITYNUM_WORLD, PlayerState, buttons, e_flags, e_flags2, other_flags,
    pm_flags, weap_flags, weapon_state as ws,
};
use crate::{Gap, MoveWorld, Pm, Pml, perks, tuning};

/// What a tactical insertion needs clear above the ground it stands on.
const TAC_INSERT_ROOM_MASK: u32 = 0x81_0011;

fn rounded(x: f32) -> i32 {
    crate::math::round_nudged(x)
}

/// Counts the hand's weapon time and delay down, faster with the reload,
/// fire rate, switch, toss and equipment perks; a burst's end waits for the
/// weapon's burst delay. True when the delay ran out this frame.
pub(super) fn weapon_timers<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) -> bool {
    let hand = Hand::of(pm.ps);
    let weapon = inv::hand_weapon(pm.ps, pm.world);
    let w = def(pm, weapon);
    let d = w.def;
    let offhand_slot = def(pm, pm.ps.offhand_index).def.offhand_slot();
    let perks = perks_with(pm, weapon);
    let msec = pml.msec;
    let ps = &mut *pm.ps;
    if ps.weapon_restrict_kick_time > 0 {
        ps.weapon_restrict_kick_time = (ps.weapon_restrict_kick_time - msec).max(0);
    }
    let s = hand.state(ps);
    let longest = hand.delay(ps).max(hand.time(ps));
    let step = if ws::is_reloading(s) && perks::has(perks, perks::FAST_RELOAD) {
        if tuning::PERK_WEAP_RELOAD_MULTIPLIER == 0.0 {
            longest
        } else {
            rounded(msec as f32 / tuning::PERK_WEAP_RELOAD_MULTIPLIER + msec as f32 * 0.0)
        }
    } else if matches!(
        s,
        ws::FIRING | ws::RECHAMBERING | ws::RECHAMBER_END | ws::CONT_FIRE_LOOP
    ) && perks::has(perks, perks::RATE_OF_FIRE)
    {
        if tuning::PERK_WEAP_RATE_MULTIPLIER == 0.0 {
            longest
        } else {
            rounded(msec as f32 / tuning::PERK_WEAP_RATE_MULTIPLIER)
        }
    } else if perks::has(perks, perks::FAST_WEAPON_SWITCH)
        && ws::is_raising_or_dropping(s)
        && d.offhand_slot() != super::offhand_slot::SPECIFIC_USE
    {
        rounded(msec as f32 / tuning::PERK_WEAP_SWITCH_MULTIPLIER)
    } else if s > ws::MELEE_END
        && s < ws::RIOTSHIELD_PREPARE
        && perks::has(perks, perks::FAST_TOSS)
        && offhand_slot != super::offhand_slot::EQUIPMENT
    {
        rounded(msec as f32 / tuning::PERK_WEAP_TOSS_MULTIPLIER)
    } else if s > ws::MELEE_END
        && s < ws::RIOTSHIELD_PREPARE
        && perks::has(perks, perks::FAST_EQUIPMENT_USE)
        && offhand_slot == super::offhand_slot::EQUIPMENT
    {
        rounded(msec as f32 / tuning::PERK_WEAP_EQUIPMENT_USE_MULTIPLIER)
    } else {
        msec
    };
    if hand.time(ps) != 0 {
        hand.set_time(ps, hand.time(ps) - step);
        let stacked_burst =
            def(pm, pm.ps.weapon).def.fire_type() == fire_type::STACKED && mid_burst(pm);
        let ps = &mut *pm.ps;
        if stacked_burst {
            hand.set_time(ps, 0);
        }
        if (stacked_burst || hand.time(ps) <= 0) && timer_ran_out(pm, hand, weapon) {
            return false;
        }
    }
    crawl_shot_counts(pm);
    let ps = &mut *pm.ps;
    if hand.delay(ps) != 0 {
        let left = hand.delay(ps) - step;
        hand.set_delay(ps, left);
        if left < 1 {
            hand.set_delay(ps, 0);
            return true;
        }
    }
    false
}

/// The hand's time ran out: a finished burst waits for its delay (true); a
/// weapon that keeps firing runs again; otherwise the press's shots are
/// counted out.
fn timer_ran_out<W: MoveWorld>(pm: &mut Pm<'_, W>, hand: Hand, weapon: u32) -> bool {
    let s = hand.state(pm.ps);
    if matches!(s, ws::FIRING | ws::CONT_FIRE_LOOP) && some(weapon) {
        let d = def(pm, weapon).def;
        if is_burst(d.fire_type()) && !mid_burst(pm) {
            let ps = &mut *pm.ps;
            let delay = d.burst_delay_time();
            hand.set_time(ps, if delay == 0 { 1 } else { delay });
            let left = ps.left_hand;
            start_anim_unless_playing(ps, weap_anim::IDLE, left);
            hand.set_state(ps, ws::READY);
            return true;
        }
    }
    if !keep_firing(pm, s, weapon) {
        let button = fire_button(pm, pm.ps.weapon, pm.ps.left_hand);
        if (!pm.cmd.buttons.held(button) || hand_latched(pm.ps)) && !mid_burst(pm) {
            hand.set_shot_count(pm.ps, 0);
        }
        hand.set_time(pm.ps, 0);
        return false;
    }
    let ps = &mut *pm.ps;
    hand.set_time(
        ps,
        if matches!(s, ws::RAISING_ALT | ws::DROPPING_ALT) {
            0
        } else {
            1
        },
    );
    match s {
        s if ws::is_reloading(s) => {
            hand.set_time(ps, 0);
            hand.set_shot_count(ps, 0);
        }
        ws::RECHAMBER_END => finish_rechamber(pm),
        ws::FIRING
        | ws::RECHAMBERING
        | ws::CONT_FIRE_LOOP
        | ws::MELEE_INIT
        | ws::MELEE_FIRE
        | ws::MELEE_END => {
            let left = ps.left_hand;
            start_anim_unless_playing(ps, weap_anim::IDLE, left);
            hand.set_state(ps, ws::READY);
        }
        _ => {}
    }
    false
}

/// Crawling forgets a hand's shots once its button is let go.
fn crawl_shot_counts<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    if !ws::is_crawl_loop(pm.ps.weaponstate) {
        return;
    }
    let weapon = pm.ps.weapon;
    let right = fire_button(pm, weapon, false);
    let left = fire_button(pm, weapon, true);
    if (!pm.cmd.buttons.held(right) || hand_latched(pm.ps)) && !mid_burst(pm) {
        pm.ps.weapon_shot_count = 0;
    }
    pm.ps.left_hand = !pm.ps.left_hand;
    if (!pm.cmd.buttons.held(left) || hand_latched(pm.ps)) && !mid_burst(pm) {
        pm.ps.weapon_shot_count_left = 0;
    }
    pm.ps.left_hand = !pm.ps.left_hand;
}

/// The hand keeps firing past its time: a burst not yet complete or a
/// held-to-throw grenade, its button still down, the same weapon (or an
/// alternate that cannot be switched away from), and rounds left.
pub(super) fn keep_firing<W: MoveWorld>(pm: &Pm<'_, W>, state: i32, weapon: u32) -> bool {
    if (ws::OFFHAND_INIT..=ws::OFFHAND_END).contains(&state)
        || (ws::CRAWL_IN..=ws::CRAWL_LEFT).contains(&state)
        || pm.ps.weap_flags & weap_flags::OFFHAND_SWITCH != 0
    {
        return false;
    }
    let d = def(pm, weapon).def;
    let burst_done = !hand_latched(pm.ps) && burst_complete(pm, weapon);
    let throw_held = matches!(d.weapon_type(), weapon_type::GRENADE | weapon_type::MINE)
        && d.hold_button_to_throw();
    let ps = &*pm.ps;
    if !(burst_done || throw_held)
        || !pm
            .cmd
            .buttons
            .held(fire_button(pm, ps.weapon, ps.left_hand))
    {
        return false;
    }
    if ps.weapon != pm.cmd.weapon
        && (!super::switch::alt_switch(pm, ps.weapon, pm.cmd.weapon)
            || !def(pm, ps.weapon).def.alt_weapon_disable_switching())
    {
        return false;
    }
    inv::hand_clip(ps, pm.world) != 0 || d.unlimited_ammo()
}

/// The fire types that fire in bursts.
fn is_burst(fire: i32) -> bool {
    (fire_type::BURST_2..=fire_type::BURST_5).contains(&fire) || fire == fire_type::CHARGE_SHOT
}

/// Enough shots for the fire type have been fired in this press; a stacked
/// weapon's stack is its stack-fire count.
pub(super) fn burst_complete<W: MoveWorld>(pm: &Pm<'_, W>, weapon: u32) -> bool {
    let ps = &*pm.ps;
    let shots = Hand::of(ps).shot_count(ps);
    if def(pm, ps.weapon).def.fire_type() == fire_type::STACKED {
        return if tuning::ZM_STACK_FIRE_FIX {
            def(pm, weapon).def.stack_fire() as u32 <= shots as u32
        } else {
            inv::clip_size(pm.world, ps.weapon) as u32 <= shots as u32
        };
    }
    match def(pm, weapon).def.fire_type() {
        fire_type::SINGLE_SHOT | fire_type::CHARGE_SHOT => shots > 0,
        fire_type::BURST_2 => shots > 1,
        fire_type::BURST_3 => shots > 2,
        fire_type::BURST_4 => shots > 3,
        fire_type::BURST_5 => shots > 4,
        _ => false,
    }
}

/// A burst is under way: shots fired, ammo left, and not all of them yet.
pub(super) fn mid_burst<W: MoveWorld>(pm: &Pm<'_, W>) -> bool {
    let weapon = inv::hand_weapon(pm.ps, pm.world);
    let ps = &*pm.ps;
    let shots = Hand::of(ps).shot_count(ps);
    if !some(weapon) || shots == 0 {
        return false;
    }
    let fire = def(pm, weapon).def.fire_type();
    if matches!(
        fire,
        fire_type::FULL_AUTO | fire_type::MINIGUN | fire_type::JETGUN
    ) {
        return false;
    }
    if inv::stock(ps, pm.world, weapon) == 0 && inv::clip(ps, pm.world, weapon) == 0 {
        return false;
    }
    !burst_complete(pm, weapon)
}

/// The shot about to fire is the last of the burst (or the clip).
fn last_shot_of_burst<W: MoveWorld>(pm: &Pm<'_, W>, weapon: u32) -> bool {
    let clip = inv::clip(pm.ps, pm.world, weapon);
    match def(pm, weapon).def.fire_type() {
        fire_type::BURST_2 => clip < 3,
        fire_type::BURST_3 => clip < 4,
        fire_type::BURST_4 => clip < 5,
        fire_type::BURST_5 => clip < 6,
        _ => clip < 2,
    }
}

/// The running hand's fire press latched.
pub(super) fn hand_latched(ps: &PlayerState) -> bool {
    let flag = if ps.left_hand {
        weap_flags::FIRE_LATCH_LEFT
    } else {
        weap_flags::FIRE_LATCH
    };
    ps.weap_flags & flag != 0
}

/// A burst or single-shot weapon's fire press latches for the hand it fires.
pub(super) fn latch_fire<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let fire = def(pm, pm.ps.weapon).def.fire_type();
    if matches!(fire, fire_type::FULL_AUTO | fire_type::SINGLE_SHOT) {
        return;
    }
    let (right, left) = if pm.cmd.fire_buttons_swapped {
        (buttons::THROW, buttons::ATTACK)
    } else {
        (buttons::ATTACK, buttons::THROW)
    };
    let pressed = |b: u32| pm.cmd.buttons.held(b) && !pm.oldcmd.buttons.held(b);
    if pressed(right) {
        pm.ps.weap_flags |= weap_flags::FIRE_LATCH;
    }
    if pressed(left) {
        pm.ps.weap_flags |= weap_flags::FIRE_LATCH_LEFT;
    }
}

/// The hand's fire button is held (a minigun spun up, a charge shot
/// released), or a burst or shot in progress keeps firing; otherwise the
/// hand goes back to ready unless continuous fire asked.
pub(super) fn fire_held<W: MoveWorld>(
    pm: &mut Pm<'_, W>,
    delay_done: bool,
    continuous: bool,
    left: bool,
) -> bool {
    let ps = &*pm.ps;
    if ps.e_flags2 & e_flags2::MOUNTED != 0 || ps.vehicle_entity != ENTITYNUM_NONE {
        return false;
    }
    let weapon = ps.weapon;
    let d = def(pm, weapon).def;
    if d.weapon_type() == weapon_type::RIOTSHIELD {
        return false;
    }
    let mut fire = pm.cmd.buttons.held(fire_button(pm, weapon, left));
    if d.freeze_movement_when_firing() && ps.ground_entity_num == ENTITYNUM_NONE {
        fire = false;
    }
    if d.can_use_in_vehicle() {
        fire = false;
    }
    let mut busy = delay_done || mid_burst(pm);
    match d.fire_type() {
        fire_type::MINIGUN => fire = fire && 1.0 <= ps.weapon_spin_lerp,
        fire_type::CHARGE_SHOT => fire = charge_shot_held(pm),
        _ => {}
    }
    if d.bolt_action() && d.shots_before_rechamber() != 0 && inv::needs_rechamber(ps, weapon) {
        fire = false;
        busy = false;
    }
    let overheated = tuning::ZM_STOP_FIRING_WHEN_OVERHEATED && super::overheated(pm, weapon);
    if !overheated && (fire || busy) {
        return true;
    }
    if !continuous {
        let ps = &mut *pm.ps;
        if left && d.dual_wield() {
            if ps.weaponstate_left == ws::FIRING {
                start_anim_unless_playing(ps, weap_anim::IDLE, true);
            }
            ps.weaponstate_left = ws::READY;
            return false;
        }
        if ps.weaponstate == ws::FIRING && !super::pose::mantling_anim(ps) {
            start_anim_unless_playing(ps, weap_anim::IDLE, false);
        }
        ps.weaponstate = ws::READY;
    }
    false
}

/// A cancel key took back a pulled-back throw of a weapon that is not a
/// grenade: the cook is dropped.
pub(super) fn cancel_grenade<W: MoveWorld>(pm: &mut Pm<'_, W>) -> bool {
    let ps = &*pm.ps;
    if ps.weap_flags & weap_flags::REVIVING != 0 {
        return false;
    }
    let kind = def(pm, ps.weapon).def.weapon_type();
    let ps = &mut *pm.ps;
    if !matches!(kind, weapon_type::GRENADE | weapon_type::MINE)
        && ps.weap_flags & weap_flags::CANCEL_GRENADE != 0
        && ps.e_flags2 & e_flags2::GRENADE_HELD == 0
    {
        ps.grenade_time_left = 0;
        return true;
    }
    false
}

/// A thrown weapon that is not held to throw has its fire button down.
pub(super) fn grenade_fire_held<W: MoveWorld>(pm: &Pm<'_, W>) -> bool {
    let weapon = pm.ps.weapon;
    if !some(weapon) {
        return false;
    }
    let d = def(pm, weapon).def;
    if !matches!(d.weapon_type(), weapon_type::GRENADE | weapon_type::MINE)
        || d.hold_button_to_throw()
    {
        return false;
    }
    pm.cmd.buttons.held(fire_button(pm, weapon, false))
}

/// A plantable grenade needs the ground (and a tactical insertion room to
/// stand); with neither, the quick raise plays unless `quiet`.
pub(super) fn plant_check<W: MoveWorld>(pm: &mut Pm<'_, W>, weapon: u32, quiet: bool) -> bool {
    let d = def(pm, weapon).def;
    if d.weapon_type() != weapon_type::GRENADE || !d.plantable() || d.projectile_speed_up() >= 1 {
        return true;
    }
    if pm.ps.ground_entity_num == ENTITYNUM_NONE {
        if !quiet {
            start_anim(pm.ps, weap_anim::QUICK_RAISE);
        }
        return false;
    }
    if pm.ps.e_flags & e_flags::VEHICLE_VIEW != 0 {
        return false;
    }
    if d.is_tactical_insertion() && !tac_insert_room(pm) {
        if !quiet {
            start_anim(pm.ps, weap_anim::QUICK_RAISE);
        }
        return false;
    }
    true
}

/// A tactical insertion is set on the world's ground with room to stand.
fn tac_insert_room<W: MoveWorld>(pm: &mut Pm<'_, W>) -> bool {
    let ps = &*pm.ps;
    if ps.weap_flags & weap_flags::PLACEMENT_BLOCKED != 0 || ps.ground_entity_num != ENTITYNUM_WORLD
    {
        return false;
    }
    let blocked = pm_flags::MANTLE | pm_flags::LADDER | pm_flags::DIVING | pm_flags::NO_STAND;
    let mask = if tuning::PLAYER_JUMP_BLOCKS_TAC_INSERT {
        blocked | pm_flags::JUMPING
    } else {
        blocked
    };
    if ps.pm_flags & mask != 0 {
        return false;
    }
    let o = ps.origin;
    let t = pm.trace_plain(
        o,
        o,
        [-15.0, -15.0, 0.0],
        [15.0, 15.0, 70.0],
        TAC_INSERT_ROOM_MASK,
    );
    !t.startsolid && !t.allsolid && 1.0 <= t.fraction
}

/// Rounds one shot takes: a charge shot's level (or the sum of its levels).
fn rounds_per_shot<W: MoveWorld>(pm: &Pm<'_, W>, weapon: u32) -> i32 {
    if def(pm, weapon).def.fire_type() != fire_type::CHARGE_SHOT {
        return 1;
    }
    let level = pm.ps.charge_shot_level;
    if tuning::CHARGE_SHOT_EXPONENTIAL_AMMO {
        if level != 0 {
            return (0..level).map(|i| 1 + i).sum::<i32>().max(0);
        }
    } else if !tuning::CHARGE_SHOT_ONE_AMMO_FOR_MULTIPLE_BULLETS {
        return level;
    }
    1
}

/// The hand may fire: rounds in its clip for a shot; with too few it reloads
/// from stock, or clicks empty and waits.
fn has_ammo_to_fire<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) -> bool {
    let hand = Hand::of(pm.ps);
    let weapon = inv::hand_weapon(pm.ps, pm.world);
    let d = def(pm, weapon).def;
    let need = rounds_per_shot(pm, weapon);
    if need <= inv::hand_clip(pm.ps, pm.world) {
        return true;
    }
    let stock = inv::stock(pm.ps, pm.world, weapon);
    let thrown = matches!(d.weapon_type(), weapon_type::GRENADE | weapon_type::MINE);
    if !d.no_ads_auto_reload() || pm.ps.weapon_pos_frac != 1.0 {
        if need <= stock {
            super::reload::begin_reload(pm, pml);
            return false;
        }
        if !thrown {
            let ev = if pm.ps.left_hand {
                event::NOAMMO_LEFT
            } else {
                event::NOAMMO
            };
            pm.event(ev, 0);
        }
    }
    if !super::pose::mantling_anim(pm.ps) {
        let ps = &mut *pm.ps;
        if let Some(h) = inv::held_mut(ps, weapon) {
            h.needs_rechamber = false;
        }
        let left = ps.left_hand;
        start_anim_unless_playing(ps, weap_anim::IDLE, left);
        if !thrown {
            hand.set_time(ps, hand.time(ps) + 500);
        }
    }
    false
}

/// Times a shot: the fire time (the burst's last, or the intro fire time
/// while the intro lasts), fire delay, the sights' delay for ADS-fire
/// weapons, kick restriction; grenades are pulled back and cooked.
fn fire_timing<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml, delay_done: bool) {
    let _ = pml;
    let hand = Hand::of(pm.ps);
    let weapon = inv::hand_weapon(pm.ps, pm.world);
    let d = def(pm, weapon).def;
    if hand.state(pm.ps) != ws::FIRING {
        pm.ps.intro_shots_fired = 0;
    }
    let thrown = matches!(d.weapon_type(), weapon_type::GRENADE | weapon_type::MINE);
    let mut fire_anim_event = true;
    if thrown {
        if !delay_done {
            if inv::hand_clip(pm.ps, pm.world) != 0 {
                pm.ps.grenade_time_left = d.fuse_time();
                start_anim_both(pm.ps, weap_anim::HOLD_FIRE);
                let held = pm.ps.weapon as i32;
                pm.event(event::PULLBACK_WEAPON, held);
            }
            let ps = &mut *pm.ps;
            ps.weapon_delay = d.hold_fire_time();
            ps.weapon_time = 0;
            fire_anim_event = false;
        }
    } else {
        let time = if last_shot_of_burst(pm, weapon) && d.last_fire_time() != 0 {
            d.last_fire_time()
        } else if d.intro_fire_length() != 0 && pm.ps.intro_shots_fired < d.intro_fire_length() {
            d.intro_fire_time()
        } else {
            d.fire_time()
        };
        let ps = &mut *pm.ps;
        hand.set_time(ps, time);
        if d.bolt_action()
            && d.shots_before_rechamber() == 0
            && let Some(h) = inv::held_mut(ps, weapon)
        {
            h.needs_rechamber = true;
        }
        let s = hand.state(ps);
        if s != ws::FIRING && s != ws::CONT_FIRE_LOOP {
            hand.set_delay(ps, d.fire_delay());
            let kick = if ps.weapon_pos_frac < 1.0 {
                d.hip_gun_kick_reduced_kick_bullets()
            } else {
                d.ads_gun_kick_reduced_kick_bullets()
            };
            ps.weapon_restrict_kick_time = d.fire_time() * kick + d.fire_delay();
        }
        if d.ads_fire() && d.fire_type() != fire_type::MINIGUN {
            let sights = ((1.0 / d.ads_in_rate()) * (1.0 - ps.weapon_pos_frac)) as i32;
            hand.set_delay(ps, hand.delay(ps).max(sights));
        }
        if d.guided_missile_type() == 6 {
            ps.weap_flags |= weap_flags::FIRING_GUIDED;
        }
        let shellshocked = ps.pm_flags & pm_flags::SHELLSHOCKED != 0;
        if shellshocked {
            pm.gap(Gap::Shellshock);
        }
        let ps = &*pm.ps;
        if shellshocked || ps.view_height_current != ps.view_height_target as f32 {
            fire_anim_event = false;
        }
    }
    if fire_anim_event {
        pm.anim_event(anim::FIRE_WEAPON, false, true);
    }
    let ps = &mut *pm.ps;
    hold_prone(ps);
    if !ws::is_cont_fire(hand.state(ps)) {
        hand.set_state(ps, ws::FIRING);
    }
    if d.bolt_action() && d.shots_before_rechamber() != 0 {
        ps.shots_fired_from_chamber += if d.fire_type() == fire_type::CHARGE_SHOT {
            ps.charge_shot_level
        } else {
            1
        };
        if d.shots_before_rechamber() as u32 <= ps.shots_fired_from_chamber as u32
            && let Some(h) = inv::held_mut(ps, weapon)
        {
            h.needs_rechamber = true;
        }
    }
    if hand.delay(ps) == 0 && d.fire_type() != fire_type::FULL_AUTO {
        hand.set_shot_count(ps, (hand.shot_count(ps) + 1).min(7));
    }
}

/// The viewmodel's fire animation: hip or sights, last shot or not.
fn fire_anim<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    if pm.ps.weap_anim & !weap_anim::TOGGLE == weap_anim::MANTLE {
        return;
    }
    let empty = inv::hand_clip_empty(pm.ps, pm.world);
    let ps = &mut *pm.ps;
    let anim = match (ps.weapon_pos_frac <= 0.75, empty) {
        (true, false) => weap_anim::FIRE,
        (true, true) => weap_anim::LAST_SHOT,
        (false, false) => weap_anim::ADS_FIRE,
        (false, true) => weap_anim::ADS_LAST_SHOT,
    };
    let left = ps.left_hand;
    start_anim_hand(ps, anim, left);
}

/// Each hip shot widens the spread.
fn spread_fire_add<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let add = def(pm, pm.ps.weapon).def.hip_spread_fire_add();
    let ps = &mut *pm.ps;
    if ps.weapon_pos_frac != 1.0 {
        ps.aim_spread_scale = (add * 255.0 + ps.aim_spread_scale).min(255.0);
    }
}

/// Either gun of the held weapon has rounds in its clip.
fn any_clip<W: MoveWorld>(pm: &Pm<'_, W>) -> bool {
    let ps = &*pm.ps;
    inv::clip(ps, pm.world, ps.weapon) != 0
        || inv::clip(ps, pm.world, def(pm, ps.weapon).dual_wield_weapon) != 0
}

/// Out of every round: the empty click.
fn no_ammo_sound<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let ps = &*pm.ps;
    let d = def(pm, ps.weapon).def;
    if d.unlimited_ammo() || ps.weap_flags & weap_flags::CYCLING_DISABLED != 0 {
        return;
    }
    if inv::clip(ps, pm.world, ps.weapon) == 0
        && inv::stock(ps, pm.world, ps.weapon) == 0
        && !any_clip(pm)
        && !d.has_detonator()
        && !matches!(d.guided_missile_type(), 5 | 6)
    {
        pm.event(event::NOAMMO, 0);
    }
}

/// The special fire button jams the weapon for two seconds.
fn special_fire<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let ps = &mut *pm.ps;
    ps.weaponstate = ws::FIRING_SPECIAL;
    ps.weapon_time = 2000;
    ps.weapon_delay = 2000;
    start_anim(ps, weap_anim::SPECIAL_FIRE);
    hold_prone(ps);
    pm.event(event::JAM_WEAPON, 0);
}

/// Fires the hand's weapon when it can: a mine needs soft ground below, a
/// planted grenade the ground; the shot's timing, ammo, lock-on, events and
/// spread follow.
pub(super) fn fire<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml, delay_done: bool) {
    let weapon = pm.ps.weapon;
    let d = def(pm, weapon).def;
    if pm.ps.water_level >= 4 || d.weapon_type() == weapon_type::MELEE {
        return;
    }
    if d.weapon_type() == weapon_type::MINE {
        let o = pm.ps.origin;
        let t = pm.trace_plain(
            [o[0], o[1], o[2] + 10.0],
            [o[0], o[1], o[2] - 50.0],
            [0.0; 3],
            [0.0; 3],
            0x831,
        );
        let surface = if t.fraction < 1.0 { t.surface_flags } else { 0 } & 0x3f0_0000;
        if !matches!(
            surface,
            0x60_0000 | 0x80_0000 | 0xa0_0000 | 0xe0_0000 | 0x120_0000 | 0x130_0000
        ) {
            return;
        }
    }
    if d.weapon_type() == weapon_type::GRENADE && !plant_check(pm, weapon, false) {
        pm.ps.weapon_time = 500;
        pm.event(event::CANNOTPLANT, 0);
        return;
    }
    if super::overheated(pm, weapon) {
        return;
    }
    if d.guided_missile_type() == 6 && pm.ps.ground_entity_num == ENTITYNUM_NONE {
        return;
    }
    if !d.unlimited_ammo() && !has_ammo_to_fire(pm, pml) {
        return;
    }
    if d.fire_type() == fire_type::CHARGE_SHOT {
        pm.ps.weap_flags &= !weap_flags::CHARGE_SHOT_CHARGING;
        pm.ps.charge_shot_timer = 0;
    }
    fire_timing(pm, pml, delay_done);
    let ps = &*pm.ps;
    let left = ps.left_hand;
    if !((ps.weapon_delay == 0 || left) && (ps.weapon_delay_left == 0 || !left)) {
        return;
    }
    let ps = &mut *pm.ps;
    ps.weap_flags &= !(weap_flags::FIRE_LATCH | weap_flags::FIRE_LATCH_LEFT);
    if d.require_lockon_to_fire() {
        let lock = ps.weap_lock_flags;
        let hint = if lock & 2 == 0 {
            Some(event::LOCKON_REQUIRED_HINT)
        } else if lock & 0x10 != 0 {
            Some(event::TARGET_TOO_CLOSE_HINT)
        } else if lock & 0x20 != 0 {
            Some(event::TARGET_NOT_ENOUGH_CLEARANCE)
        } else {
            None
        };
        if let Some(hint) = hint {
            pm.event(hint, 0);
            return;
        }
    }
    if pm.cmd.buttons.held(buttons::SPECIAL_FIRE) {
        special_fire(pm);
        return;
    }
    let turret = pm.ps.e_flags & e_flags::TURRET != 0;
    if inv::hand_clip(pm.ps, pm.world) != 0 && !d.fuel_tank_weapon() && !turret {
        let rounds = rounds_per_shot(pm, weapon);
        if !left || !dual_wield(pm, weapon) {
            if inv::use_clip(pm.ps, pm.world, weapon, rounds) == 0 {
                pm.ps.e_flags2 |= e_flags2::CLIP_EMPTY;
            }
        } else {
            let left_gun = def(pm, weapon).dual_wield_weapon;
            inv::use_clip(pm.ps, pm.world, left_gun, rounds);
        }
    }
    let ps = &mut *pm.ps;
    if matches!(d.weapon_type(), weapon_type::GRENADE | weapon_type::MINE) {
        ps.weapon_time = d.fire_time();
    }
    ps.intro_shots_fired = (ps.intro_shots_fired + 1).clamp(0, 250);
    if !ws::is_cont_fire(ps.weaponstate) {
        fire_anim(pm);
    }
    if d.guided_missile_type() == 6 {
        pm.ps.weap_flags |= weap_flags::GUIDED_MISSILE;
    }
    let hand = Hand::of(pm.ps);
    let parm = if d.fire_type() == fire_type::CHARGE_SHOT {
        pm.ps.charge_shot_level
    } else {
        hand.shot_count(pm.ps)
    };
    let empty = inv::hand_clip_empty(pm.ps, pm.world);
    let ev = match (empty, left) {
        (false, false) => event::FIRE_WEAPON,
        (false, true) => event::FIRE_WEAPON_LEFT,
        (true, false) => event::FIRE_WEAPON_LASTSHOT,
        (true, true) => event::FIRE_WEAPON_LASTSHOT_LEFT,
    };
    pm.event(ev, parm);
    super::ads::hold_breath_fire(pm);
    spread_fire_add(pm);
    no_ammo_sound(pm);
}

/// Clears a charge shot's charge.
pub(super) fn charge_reset(ps: &mut PlayerState) {
    ps.weap_flags &= !weap_flags::CHARGE_SHOT_CHARGING;
    ps.charge_shot_timer = 0;
    ps.charge_shot_level = 0;
}

/// The most levels a charge shot can queue: the dvar's cap, at most the
/// clip unless one round serves every bullet.
fn charge_shot_max<W: MoveWorld>(pm: &Pm<'_, W>, weapon: u32) -> i32 {
    let clip = inv::clip(pm.ps, pm.world, weapon);
    let mut n = tuning::CHARGE_SHOT_MAX_BULLETS_IN_QUEUE;
    if !tuning::CHARGE_SHOT_ONE_AMMO_FOR_MULTIPLE_BULLETS && clip < n {
        n = clip;
    }
    if tuning::CHARGE_SHOT_EXPONENTIAL_AMMO {
        let mut sum = 0;
        for i in 0..n {
            sum += 1 + i;
            if clip < sum {
                return i;
            }
        }
    }
    n
}

/// A charge shot charges while attack is held, a level per queue time up to
/// its most; it may discharge itself at the most.
pub(super) fn charge_shot<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let weapon = super::aimed_weapon(pm);
    let d = def(pm, weapon).def;
    if d.fire_type() != fire_type::CHARGE_SHOT {
        charge_reset(pm.ps);
        return;
    }
    let clip = inv::clip(pm.ps, pm.world, weapon);
    let mut most = charge_shot_max(pm, weapon);
    let ps = &mut *pm.ps;
    let s = ps.weaponstate;
    let can = !(ws::is_reloading(s)
        || ws::is_crawl(s)
        || matches!(s, ws::RECHAMBERING | ws::RECHAMBER_END)
        || ws::is_raising_or_dropping(s)
        || ws::is_dtp(s));
    if !pm.cmd.buttons.held(buttons::ATTACK) {
        return;
    }
    if !pm.oldcmd.buttons.held(buttons::ATTACK)
        && clip != 0
        && can
        && !(tuning::PREVENT_STARTING_CHARGE_SHOT_WHILE_FIRING && s == ws::FIRING)
    {
        ps.weap_flags |= weap_flags::CHARGE_SHOT_CHARGING;
        ps.charge_shot_level = 0;
        ps.charge_shot_timer = 0;
        return;
    }
    if ps.weap_flags & weap_flags::CHARGE_SHOT_CHARGING == 0 || !can {
        return;
    }
    let queue = tuning::CHARGE_SHOT_QUEUE_TIME;
    ps.charge_shot_timer += pml.msec;
    let mut level = ps.charge_shot_timer / queue + 1;
    if d.bolt_action() && d.shots_before_rechamber() != 0 {
        let left = d.shots_before_rechamber() - ps.shots_fired_from_chamber;
        if left < most {
            most = left;
        }
    }
    if ps.other_flags & other_flags::EMP != 0 && tuning::CHARGE_SHOT_MAX_BULLET_QUEUE_ON_EMP < most
    {
        most = tuning::CHARGE_SHOT_MAX_BULLET_QUEUE_ON_EMP;
    }
    if most <= level {
        level = most;
        if tuning::CHARGE_SHOT_DISCHARGE_WHEN_QUEUE_REACHES_MAX
            && tuning::CHARGE_SHOT_AUTO_DISCHARGE_DELAY < ps.charge_shot_timer - (most - 1) * queue
        {
            pm.event(event::CHARGESHOT_DISCHARGE, 0);
            charge_reset(pm.ps);
            return;
        }
    }
    let ps = &mut *pm.ps;
    if (ps.charge_shot_level as u32) < level as u32 {
        let old = ps.charge_shot_level;
        pm.event(event::INC_CHARGESHOT_LEVEL, old);
    }
    pm.ps.charge_shot_level = level;
}

/// A charge shot fires when attack is let go, or at its most when set to.
fn charge_shot_held<W: MoveWorld>(pm: &Pm<'_, W>) -> bool {
    let ps = &*pm.ps;
    let released = !pm.cmd.buttons.held(buttons::ATTACK) && pm.oldcmd.buttons.held(buttons::ATTACK);
    let level = ps.charge_shot_level as u32;
    let since_most = (ps.charge_shot_timer as u32)
        .wrapping_sub(level.wrapping_sub(1) * tuning::CHARGE_SHOT_QUEUE_TIME as u32);
    let at_most = 1 < level
        && tuning::CHARGE_SHOT_FIRE_WHEN_QUEUE_REACHES_MAX
        && tuning::CHARGE_SHOT_MAX_BULLETS_IN_QUEUE as u32 <= level
        && (tuning::CHARGE_SHOT_AUTO_FIRE_DELAY as u32) < since_most;
    (released || at_most) && ps.weap_flags & weap_flags::CHARGE_SHOT_CHARGING != 0
}

/// A bolt-action weapon that needs its bolt worked starts rechambering when
/// it is ready; true while that holds the weapon.
pub(super) fn rechamber<W: MoveWorld>(pm: &mut Pm<'_, W>, delay_done: bool) -> bool {
    let d = def(pm, pm.ps.weapon).def;
    let ps = &*pm.ps;
    let s = ps.weaponstate;
    if (ws::OFFHAND_INIT..=ws::OFFHAND_END).contains(&s)
        || ws::is_dtp(s)
        || ws::is_riotshield(s)
        || !d.bolt_action()
        || !inv::needs_rechamber(ps, ps.weapon)
    {
        return false;
    }
    if s == ws::RECHAMBERING && delay_done {
        pm.ps.weaponstate = ws::RECHAMBER_END;
        pm.event(event::EJECT_BRASS, 0);
        if pm.ps.weapon_time != 0 {
            return true;
        }
    } else if ps.weapon_time != 0
        && (matches!(
            s,
            ws::FIRING | ws::RECHAMBERING | ws::RECHAMBER_END | ws::CONT_FIRE_LOOP
        ) || ws::is_melee(s)
            || ps.weapon_delay != 0)
    {
        return false;
    }
    let s = pm.ps.weaponstate;
    if s == ws::RECHAMBER_END {
        finish_rechamber(pm);
        return false;
    }
    if s == ws::READY || ws::is_crawl(s) {
        let ps = &mut *pm.ps;
        let anim = if ps.weapon_pos_frac <= 0.75 {
            weap_anim::RECHAMBER
        } else {
            weap_anim::ADS_RECHAMBER
        };
        start_anim(ps, anim);
        ps.weaponstate = ws::RECHAMBERING;
        ps.weapon_time = d.rechamber_time();
        if d.bolt_action() && d.shots_before_rechamber() != 0 {
            ps.shots_fired_from_chamber = 0;
        }
        let bolt = d.rechamber_bolt_time();
        ps.weapon_delay = if bolt != 0 && bolt < d.rechamber_time() {
            bolt
        } else {
            1
        };
        pm.event(event::RECHAMBER_WEAPON, 0);
    }
    false
}

/// The bolt is worked: idle again and the weapon chambered.
pub(super) fn finish_rechamber<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let d = def(pm, pm.ps.weapon).def;
    let ps = &mut *pm.ps;
    let playing = ps.weap_anim & !weap_anim::TOGGLE;
    let playing_left = ps.weap_anim_left & !weap_anim::TOGGLE;
    if (playing != 0 || ps.left_hand) && (playing_left != 0 || !ps.left_hand) {
        start_anim(ps, weap_anim::IDLE);
    }
    let weapon = ps.weapon;
    if let Some(h) = inv::held_mut(ps, weapon) {
        h.needs_rechamber = false;
    }
    ps.weaponstate = ws::READY;
    if d.bolt_action() && d.shots_before_rechamber() != 0 {
        ps.shots_fired_from_chamber = 0;
    }
    if d.fire_type() == fire_type::CHARGE_SHOT {
        charge_reset(ps);
    }
}
