//! Black Ops' weapon rules, run by its player movement: raising, dropping
//! and switching weapons, firing, rechambering, reloading, melee, offhands,
//! sprint and dive poses, and the viewmodel animations they play.
//!
//! The running hand is the right one, or the left gun of a dual-wield weapon
//! when `left_hand` is set; movement runs the rules once for each.

use crate::events::{anim, event};
use crate::inventory::{self as inv, hand_clip, hand_clip_empty, hand_weapon};
use crate::state::{buttons, e_flags, e_flags2, pm_flags, pm_type, weap_flags};
use crate::{MoveWorld, Pm, Pml, tuning};

/// Black Ops' weapon states.
pub mod state {
    pub const READY: i32 = 0;
    pub const RAISING: i32 = 1;
    pub const RAISING_ALT: i32 = 2;
    pub const DROPPING: i32 = 3;
    pub const DROPPING_QUICK: i32 = 4;
    pub const DROPPING_ALT: i32 = 5;
    pub const FIRING: i32 = 6;
    pub const RECHAMBERING: i32 = 7;
    pub const RECHAMBER_END: i32 = 8;
    pub const RELOADING: i32 = 0xb;
    pub const RELOADING_INTERRUPT: i32 = 0xc;
    pub const RELOAD_START: i32 = 0xd;
    pub const RELOAD_START_INTERRUPT: i32 = 0xe;
    pub const RELOAD_END: i32 = 0xf;
    pub const RELOADING_LEFT: i32 = 0x10;
    pub const RELOADING_LEFT_INTERRUPT: i32 = 0x11;
    pub const MELEE_INIT: i32 = 0x12;
    pub const MELEE_FIRE: i32 = 0x13;
    pub const MELEE_END: i32 = 0x14;
    pub const OFFHAND_INIT: i32 = 0x15;
    pub const OFFHAND_PREPARE: i32 = 0x16;
    pub const OFFHAND_START: i32 = 0x17;
    pub const OFFHAND_HOLD: i32 = 0x18;
    pub const OFFHAND: i32 = 0x19;
    pub const OFFHAND_END: i32 = 0x1a;
    pub const DETONATING: i32 = 0x1b;
    pub const SPRINT_RAISE: i32 = 0x1c;
    pub const SPRINT_LOOP: i32 = 0x1d;
    pub const SPRINT_DROP: i32 = 0x1e;
    pub const CONT_FIRE_IN: i32 = 0x1f;
    pub const CONT_FIRE_LOOP: i32 = 0x20;
    pub const CONT_FIRE_OUT: i32 = 0x21;
    pub const NIGHTVISION_WEAR: i32 = 0x22;
    pub const NIGHTVISION_REMOVE: i32 = 0x23;
    pub const DEPLOYING: i32 = 0x24;
    pub const DEPLOYED: i32 = 0x25;
    pub const BREAKING_DOWN: i32 = 0x26;
    pub const SWIM_IN: i32 = 0x27;
    pub const SWIM_OUT: i32 = 0x28;
    pub const DTP_IN: i32 = 0x29;
    pub const DTP_LOOP: i32 = 0x2a;
    pub const DTP_OUT: i32 = 0x2b;
    pub const SLIDE_IN: i32 = 0x2c;
    pub const FIRING_SPECIAL: i32 = 0x2f;
    pub const LOWREADY_RAISE: i32 = 0x30;
    pub const LOWREADY_LOOP: i32 = 0x31;
    pub const LOWREADY_DROP: i32 = 0x32;

    pub fn is_reloading(s: i32) -> bool {
        matches!(s, 0xb..=0x11)
    }

    pub fn is_offhand(s: i32) -> bool {
        (OFFHAND_INIT..=OFFHAND_END).contains(&s)
    }

    pub fn is_dtp(s: i32) -> bool {
        (DTP_IN..=DTP_OUT).contains(&s)
    }

    pub fn is_melee(s: i32) -> bool {
        matches!(s, MELEE_INIT | MELEE_FIRE | MELEE_END)
    }

    pub fn is_raising_or_dropping(s: i32) -> bool {
        (RAISING..=DROPPING_ALT).contains(&s)
    }

    pub fn is_firing(s: i32) -> bool {
        matches!(s, FIRING | RECHAMBERING | RECHAMBER_END | CONT_FIRE_LOOP)
    }
}

/// Black Ops' viewmodel animations, by their slot in a weapon's animations.
pub mod weap_anim {
    pub const IDLE: u32 = 0;
    pub const QUICK_IDLE: u32 = 1;
    pub const FIRE: u32 = 2;
    pub const LAST_SHOT: u32 = 4;
    pub const RECHAMBER: u32 = 6;
    pub const ADS_FIRE: u32 = 7;
    pub const ADS_LAST_SHOT: u32 = 8;
    pub const ADS_RECHAMBER: u32 = 9;
    pub const MELEE: u32 = 10;
    pub const MELEE_CHARGE: u32 = 11;
    pub const DROP: u32 = 12;
    pub const RAISE: u32 = 13;
    pub const FIRST_RAISE: u32 = 14;
    pub const RELOAD: u32 = 15;
    pub const RELOAD_EMPTY: u32 = 16;
    pub const RELOAD_START: u32 = 17;
    pub const RELOAD_END: u32 = 18;
    pub const RELOAD_QUICK: u32 = 19;
    pub const RELOAD_QUICK_EMPTY: u32 = 20;
    pub const ALT_DROP: u32 = 21;
    pub const ALT_RAISE: u32 = 22;
    pub const QUICK_DROP: u32 = 23;
    pub const QUICK_RAISE: u32 = 24;
    pub const EMPTY_DROP: u32 = 25;
    pub const EMPTY_RAISE: u32 = 26;
    pub const SPRINT_IN: u32 = 27;
    pub const SPRINT_LOOP: u32 = 28;
    pub const SPRINT_OUT: u32 = 29;
    pub const SPRINT_EMPTY_IN: u32 = 30;
    pub const SPRINT_EMPTY_LOOP: u32 = 31;
    pub const SPRINT_EMPTY_OUT: u32 = 32;
    pub const LOWREADY_IN: u32 = 33;
    pub const LOWREADY_LOOP: u32 = 34;
    pub const LOWREADY_OUT: u32 = 35;
    pub const CONT_FIRE_IN: u32 = 36;
    pub const CONT_FIRE_LOOP: u32 = 37;
    pub const CONT_FIRE_OUT: u32 = 38;
    pub const HOLD_FIRE: u32 = 39;
    pub const DETONATE: u32 = 40;
    pub const NIGHTVISION_WEAR: u32 = 41;
    pub const NIGHTVISION_REMOVE: u32 = 42;
    pub const DEPLOY: u32 = 43;
    pub const BREAKDOWN: u32 = 44;
    pub const DTP_IN: u32 = 45;
    pub const DTP_LOOP: u32 = 46;
    pub const DTP_OUT: u32 = 47;
    pub const DTP_EMPTY_IN: u32 = 48;
    pub const DTP_EMPTY_LOOP: u32 = 49;
    pub const DTP_EMPTY_OUT: u32 = 50;
    pub const SLIDE_IN: u32 = 51;
    pub const MANTLE: u32 = 52;
    pub const SPECIAL_FIRE: u32 = 53;
    /// Flipped each time an animation starts, so the same one restarts.
    pub const TOGGLE: u32 = 0x400;
}

use state as ws;

/// Starts a viewmodel animation on the right hand.
fn start_anim(ps: &mut crate::PlayerState, anim: u32) {
    if ps.pm_type < pm_type::DEAD {
        ps.weap_anim = (!ps.weap_anim & weap_anim::TOGGLE) | anim;
    }
}

fn start_anim_left(ps: &mut crate::PlayerState, anim: u32) {
    if ps.pm_type < pm_type::DEAD {
        ps.weap_anim_left = (!ps.weap_anim_left & weap_anim::TOGGLE) | anim;
    }
}

fn start_anim_both(ps: &mut crate::PlayerState, anim: u32) {
    if ps.pm_type < pm_type::DEAD {
        ps.weap_anim = (!ps.weap_anim & weap_anim::TOGGLE) | anim;
        if ps.pm_type < pm_type::DEAD {
            ps.weap_anim_left = (!ps.weap_anim_left & weap_anim::TOGGLE) | anim;
        }
    }
}

/// Starts an animation on a hand unless it already plays there.
fn start_anim_unless_playing(ps: &mut crate::PlayerState, anim: u32, left: bool) {
    if ps.weap_anim & !weap_anim::TOGGLE == anim && !ps.left_hand {
        return;
    }
    if ps.weap_anim_left & !weap_anim::TOGGLE == anim && ps.left_hand {
        return;
    }
    if ps.pm_type >= pm_type::DEAD {
        return;
    }
    if left {
        ps.weap_anim_left = (!ps.weap_anim_left & weap_anim::TOGGLE) | anim;
    } else {
        ps.weap_anim = (!ps.weap_anim & weap_anim::TOGGLE) | anim;
    }
}

/// Starts an animation on the hand that runs.
fn start_anim_hand(ps: &mut crate::PlayerState, anim: u32) {
    if ps.left_hand {
        start_anim_left(ps, anim);
    } else {
        start_anim(ps, anim);
    }
}

/// A stance change waits for a weapon action that started while prone.
fn hold_prone(ps: &mut crate::PlayerState) {
    if ps.pm_flags & pm_flags::PRONE != 0 {
        ps.pm_flags |= pm_flags::PRONEMOVE_OVERRIDDEN;
    }
}

/// Back to ready on both hands.
pub(crate) fn finish(ps: &mut crate::PlayerState) {
    ps.weap_flags &= !weap_flags::OFFHAND_ACTIVE;
    ps.pm_flags &= !pm_flags::PRONEMOVE_OVERRIDDEN;
    ps.weapon_time = 0;
    ps.weapon_delay = 0;
    ps.weaponstate = ws::READY;
    ps.weapon_time_left = 0;
    ps.weapon_delay_left = 0;
    ps.weaponstate_left = ws::READY;
    start_anim(ps, weap_anim::IDLE);
    start_anim_left(ps, weap_anim::IDLE);
}

/// The running hand's weapon state, time and delay.
struct Hand {
    left: bool,
}

impl Hand {
    fn of(ps: &crate::PlayerState) -> Self {
        Self { left: ps.left_hand }
    }

    fn state(&self, ps: &crate::PlayerState) -> i32 {
        if self.left {
            ps.weaponstate_left
        } else {
            ps.weaponstate
        }
    }

    fn set_state(&self, ps: &mut crate::PlayerState, value: i32) {
        if self.left {
            ps.weaponstate_left = value;
        } else {
            ps.weaponstate = value;
        }
    }

    fn time(&self, ps: &crate::PlayerState) -> i32 {
        if self.left {
            ps.weapon_time_left
        } else {
            ps.weapon_time
        }
    }

    fn set_time(&self, ps: &mut crate::PlayerState, value: i32) {
        if self.left {
            ps.weapon_time_left = value;
        } else {
            ps.weapon_time = value;
        }
    }

    fn delay(&self, ps: &crate::PlayerState) -> i32 {
        if self.left {
            ps.weapon_delay_left
        } else {
            ps.weapon_delay
        }
    }

    fn set_delay(&self, ps: &mut crate::PlayerState, value: i32) {
        if self.left {
            ps.weapon_delay_left = value;
        } else {
            ps.weapon_delay = value;
        }
    }

    fn shot_count(&self, ps: &crate::PlayerState) -> i32 {
        if self.left {
            ps.weapon_shot_count_left
        } else {
            ps.weapon_shot_count
        }
    }

    fn set_shot_count(&self, ps: &mut crate::PlayerState, value: i32) {
        if self.left {
            ps.weapon_shot_count_left = value;
        } else {
            ps.weapon_shot_count = value;
        }
    }
}

fn def<'a, W: MoveWorld>(pm: &Pm<'a, W>, weapon: u32) -> crate::Weapon<'a> {
    pm.world.weapon(weapon)
}

/// The weapon the viewmodel shows: the offhand while one is thrown, the melee
/// weapon while knifing, else the held weapon.
pub(crate) fn view_weapon<W: MoveWorld>(pm: &Pm<'_, W>) -> u32 {
    let ps = &*pm.ps;
    if ps.weap_flags & weap_flags::OFFHAND_ACTIVE != 0
        && ps.weaponstate > ws::MELEE_END
        && ps.weaponstate < ws::DETONATING
    {
        return ps.offhand_index;
    }
    if ws::is_melee(ps.weaponstate) {
        let d = def(pm, ps.weapon).def;
        if !d.bayonet() && !d.use_as_melee() && ps.weaponstate != ws::MELEE_END {
            return ps.melee_weapon;
        }
    }
    ps.weapon
}

/// Aiming down the sights is possible.
fn can_ads<W: MoveWorld>(pm: &Pm<'_, W>, pml: &Pml) -> bool {
    let ps = &*pm.ps;
    match ps.pm_type {
        pm_type::NORMAL_LINKED if pml.almost_ground_plane => return false,
        pm_type::NOCLIP
        | pm_type::UFO
        | pm_type::SPECTATOR
        | pm_type::INTERMISSION
        | pm_type::DEAD
        | pm_type::DEAD_LINKED => return false,
        _ => {}
    }
    let weapon = view_weapon(pm);
    let w = def(pm, weapon);
    let s = ps.weaponstate;
    w.def.aim_down_sight()
        && (!tuning::DISABLE_WEAPONS_IN_WATER
            || ps.water_level < 3
            || ps.pm_type == pm_type::NORMAL_LINKED)
        && !ws::is_offhand(s)
        && s != ws::DETONATING
        && !ws::is_raising_or_dropping(s)
        && !ws::is_dtp(s)
        && ps.pm_flags & pm_flags::MANTLE == 0
        && ps.weap_flags & weap_flags::NO_ADS == 0
        && (!w.def.no_ads_when_mag_empty() || inv::clip(ps, &w) != 0)
}

/// Aiming through a scope overlay.
fn scoped<W: MoveWorld>(pm: &Pm<'_, W>) -> bool {
    def(pm, pm.ps.weapon).def.ads_overlay_reticle() != 0 && pm.ps.weapon_pos_frac > 0.0
}

/// The player means to aim down the sights this frame.
pub(crate) fn update_ads_intent<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    pm.ps.pm_flags &= !pm_flags::ADS_INTENT;
    let mut can = can_ads(pm, pml);
    let reticle = def(pm, pm.ps.weapon).def.ads_overlay_reticle();
    let held = pm.cmd.buttons;
    if held.held(buttons::SPRINT) && (reticle == 0 || !held.held(buttons::HOLD_BREATH)) {
        pm.event(event::RESET_ADS, 0);
        pm.ps.pm_flags &= !pm_flags::ADS_INTENT;
        can = false;
    }
    if held.held(buttons::ADS) && can {
        if pm.ps.pm_flags & pm_flags::PRONE == 0 || scoped(pm) {
            pm.ps.pm_flags |= pm_flags::ADS_INTENT;
        } else if !pm.oldcmd.buttons.held(buttons::ADS)
            || (pm.cmd.forwardmove == 0 && pm.cmd.rightmove == 0)
        {
            pm.ps.pm_flags |= pm_flags::ADS_INTENT | pm_flags::PRONEMOVE_OVERRIDDEN;
        }
    }
}

/// Aiming with the sights up and steady: the movement scales for it apply.
pub(crate) fn update_sight_aiming<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let ps = &mut *pm.ps;
    ps.pm_flags &= !pm_flags::SIGHT_AIMING;
    let f = ps.pm_flags;
    if ps.pm_type < pm_type::DEAD
        && pm.cmd.buttons.held(buttons::ADS)
        && f & pm_flags::PRONE == 0
        && f & pm_flags::ADS_INTENT != 0
        && !ws::is_reloading(ps.weaponstate)
    {
        ps.pm_flags = f | pm_flags::SIGHT_AIMING;
    }
}

/// The sights come up or go down at the weapon's rates.
fn ads_lerp<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let weapon = view_weapon(pm);
    let w = def(pm, weapon);
    let d = w.def;
    let ps = &mut *pm.ps;
    if ps.e_flags2 & e_flags2::ADS_RESET != 0 && ps.weapon_pos_frac > 0.0 {
        pm.out(crate::MoveEvent::Entity(event::RESET_ADS, 0));
        let ps = &mut *pm.ps;
        ps.pm_flags &= !pm_flags::ADS_INTENT;
        ps.weapon_pos_frac = 0.0;
        ps.ads_delay_time = 0;
        return;
    }
    if tuning::SCOPE_EXIT_ON_DAMAGE && ps.damage_count != 0 && d.ads_overlay_reticle() != 0 {
        pm.out(crate::MoveEvent::Entity(event::RESET_ADS, 0));
        let ps = &mut *pm.ps;
        ps.pm_flags &= !pm_flags::ADS_INTENT;
        ps.ads_delay_time = 0;
        ps.weapon_pos_frac = 0.0;
        return;
    }
    if !d.aim_down_sight() {
        ps.ads_delay_time = 0;
        ps.weapon_pos_frac = 0.0;
        return;
    }
    let s = ps.weaponstate;
    let trans = d.ads_reload_trans_time();
    let reload_blocks = !d.reload_while_ads()
        && ((!d.segmented_reload()
            && s == ws::RELOADING
            && ps.weapon_time != trans
            && ps.weapon_time - trans >= 0)
            || (d.segmented_reload()
                && (matches!(s, ws::RELOADING | ws::RELOADING_INTERRUPT)
                    || matches!(s, ws::RELOAD_START | ws::RELOAD_START_INTERRUPT)
                    || (s == ws::RELOAD_END
                        && ps.weapon_time != trans
                        && ps.weapon_time - trans >= 0))));
    let rechamber_blocks =
        !d.rechamber_while_ads() && matches!(s, ws::RECHAMBERING | ws::RECHAMBER_END);
    let mut up = !(reload_blocks || rechamber_blocks)
        && ps.pm_flags & pm_flags::LOW_READY == 0
        && ps.pm_flags & pm_flags::ADS_INTENT != 0;
    if d.ads_fire()
        && (ps.weapon_delay != 0
            || (d.fire_type() == fire_type::STACKED && ps.weap_flags & weap_flags::FIRE_LATCH != 0))
        && matches!(s, ws::FIRING | ws::CONT_FIRE_LOOP)
    {
        up = true;
    }
    let frac = ps.weapon_pos_frac;
    if frac != 1.0 || up || tuning::ADS_EXIT_DELAY < 1 {
        ps.ads_delay_time = 0;
        if !up {
            if frac == 0.0 {
                return;
            }
        } else if frac == 1.0 {
            return;
        }
    } else {
        if ps.ads_delay_time == 0 {
            ps.ads_delay_time = pm.cmd.server_time + tuning::ADS_EXIT_DELAY;
        }
        if ps.ads_delay_time <= pm.cmd.server_time {
            ps.ads_delay_time = 0;
            if frac == 0.0 {
                return;
            }
        } else {
            up = true;
            if frac == 1.0 {
                return;
            }
        }
    }
    let rate = if up {
        ads_in_rate(&w)
    } else {
        -ads_out_rate(&w)
    };
    let next = pml.msec as f32 * rate + frac;
    let capped = if next - 1.0 < 0.0 { next } else { 1.0 };
    ps.weapon_pos_frac = if -next < 0.0 { capped } else { 0.0 };
}

/// The sights' rates: 1 over the variant's transition times, as the game
/// computes them when it loads the weapon.
fn ads_in_rate(w: &crate::Weapon<'_>) -> f32 {
    let ms = w.def.ads_trans_in_time();
    if ms < 1 {
        0.00333333341
    } else {
        1.0 / ms as f32
    }
}

fn ads_out_rate(w: &crate::Weapon<'_>) -> f32 {
    let ms = w.def.ads_trans_out_time();
    if ms > 0 {
        1.0 / ms as f32
    } else {
        0.00200000009
    }
}

/// Black Ops' fire types.
pub mod fire_type {
    pub const FULL_AUTO: i32 = 0;
    pub const SINGLE_SHOT: i32 = 1;
    pub const BURST_2: i32 = 2;
    pub const BURST_3: i32 = 3;
    pub const BURST_4: i32 = 4;
    pub const STACKED: i32 = 5;
    pub const MINIGUN: i32 = 6;
}

/// A minigun spins up while fire is held or aimed, down otherwise.
fn spin<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let weapon = view_weapon(pm);
    let d = def(pm, weapon).def;
    let ps = &mut *pm.ps;
    if d.fire_type() != fire_type::MINIGUN {
        ps.weapon_spin_lerp = 0.0;
        return;
    }
    let (time, dir) = if pm.cmd.buttons.held(buttons::ADS) || pm.cmd.buttons.held(buttons::ATTACK) {
        ps.e_flags2 |= e_flags2::SPINNING;
        (d.spin_up_time(), 1.0)
    } else {
        ps.e_flags2 &= !e_flags2::SPINNING;
        (d.spin_down_time(), -1.0)
    };
    let next = pml.msec as f32 * (dir / time as f32) + ps.weapon_spin_lerp;
    let capped = if next - 1.0 < 0.0 { next } else { 1.0 };
    ps.weapon_spin_lerp = if -next < 0.0 { capped } else { 0.0 };
}

/// The melee key steps a variable scope's zoom while aiming through it.
fn scope_zoom<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let server_time = pm.cmd.server_time;
    let melee = pm.cmd.buttons.held(buttons::MELEE);
    if !pm.ps.ads_zoom_latched && server_time - pm.ps.ads_zoom_time > 0x28 && melee {
        let zoomable = scoped(pm);
        if zoomable && pm.ps.weapon_pos_frac >= 1.0 {
            let ps = &mut *pm.ps;
            ps.ads_zoom_select += 1;
            ps.ads_zoom_latched = true;
            ps.ads_zoom_time = server_time;
            pm.event(event::SCOPE_ZOOM, 0);
            return;
        }
    }
    if server_time - pm.ps.ads_zoom_time > 0x28 && !melee {
        let reticle = def(pm, pm.ps.weapon).def.ads_overlay_reticle();
        let ps = &mut *pm.ps;
        if reticle != 0 && ps.weapon_pos_frac > 0.0 && ps.weapon_pos_frac >= 1.0 {
            ps.ads_zoom_latched = false;
            ps.ads_zoom_time = server_time;
        }
    }
}

/// Holding breath steadies a scope, then the player gasps.
fn hold_breath<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let weapon = view_weapon(pm);
    let d = def(pm, weapon).def;
    let hold = (tuning::BREATH_HOLD_TIME * 1000.0) as i32;
    let gasp = (tuning::BREATH_GASP_TIME * 1000.0) as i32;
    let ps = &mut *pm.ps;
    if hold < 1 {
        ps.weap_flags &= !weap_flags::HOLD_BREATH;
        ps.hold_breath_scale = 1.0;
        ps.hold_breath_timer = 0;
        return;
    }
    if ps.weapon_pos_frac == 1.0
        && d.ads_overlay_reticle() != 0
        && d.weapon_class() != weapon_class::ITEM
        && pm.cmd.buttons.held(buttons::HOLD_BREATH)
        && ps.perks & crate::perks::DEADSHOT == 0
    {
        if ps.hold_breath_timer == 0 {
            ps.weap_flags |= weap_flags::HOLD_BREATH;
        }
    } else {
        ps.weap_flags &= !weap_flags::HOLD_BREATH;
    }
    let holding = ps.weap_flags & weap_flags::HOLD_BREATH != 0;
    if holding {
        ps.hold_breath_timer += pml.msec;
    } else {
        ps.hold_breath_timer -= pml.msec;
    }
    if ps.hold_breath_timer < 0 {
        ps.hold_breath_timer = 0;
    }
    if holding && hold < ps.hold_breath_timer {
        ps.hold_breath_timer = hold + gasp;
        ps.weap_flags &= !weap_flags::HOLD_BREATH;
    }
    let (target, lerp) = if ps.weap_flags & weap_flags::HOLD_BREATH == 0 {
        (
            (ps.hold_breath_timer as f32 / (gasp + hold) as f32)
                * (tuning::BREATH_GASP_SCALE - 1.0)
                + 1.0,
            tuning::BREATH_GASP_LERP,
        )
    } else {
        (0.0, tuning::BREATH_HOLD_LERP)
    };
    ps.hold_breath_scale = lerp_towards(
        (target - 1.0) * ps.weapon_pos_frac + 1.0,
        ps.hold_breath_scale,
        lerp,
        pml.frametime,
    );
}

/// Moves `from` towards `to` at `rate` per second; snaps when close.
fn lerp_towards(to: f32, from: f32, rate: f32, frametime: f32) -> f32 {
    let step = (to - from) * rate * frametime;
    let gap = (to - from).abs();
    if 0.00100000005 < gap && step.abs() <= gap {
        step + from
    } else {
        to
    }
}

/// Firing while holding breath spends it.
fn hold_breath_fire<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let weapon = view_weapon(pm);
    let d = def(pm, weapon).def;
    let ps = &mut *pm.ps;
    if ps.weapon_pos_frac == 1.0
        && d.ads_overlay_reticle() != 0
        && d.weapon_class() != weapon_class::ITEM
    {
        let hold = (tuning::BREATH_HOLD_TIME * 1000.0) as i32;
        if ps.hold_breath_timer < hold {
            ps.hold_breath_timer -= (tuning::BREATH_FIRE_DELAY * -1000.0) as i32;
            if hold < ps.hold_breath_timer {
                ps.hold_breath_timer = hold;
            }
        }
        ps.weap_flags &= !weap_flags::HOLD_BREATH;
    }
}

/// Black Ops' weapon classes.
pub mod weapon_class {
    pub const PISTOL: i32 = 4;
    pub const ITEM: i32 = 10;
    pub const MELEE: i32 = 11;
}

/// Hip fire spreads with movement and turning and settles over time.
pub(crate) fn aim_spread<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let d = def(pm, pm.ps.weapon).def;
    let ps = &*pm.ps;
    let max = 255.0f32;
    let mut decay = d.hip_spread_decay_rate();
    let mut frac = 1.0f32;
    let mut decrease = 1.0f32;
    let mut increase: Option<f32> = None;
    if decay != 0.0 {
        frac = (ps.spread_override as f32 - d.hip_spread_stand_min())
            / (d.hip_spread_max() - d.hip_spread_stand_min());
        let in_air = ps.ground_entity_num == crate::state::ENTITYNUM_NONE
            && ps.pm_type != pm_type::NORMAL_LINKED;
        if in_air {
            decay *= 0.5;
        } else if ps.e_flags & e_flags::PRONE != 0 {
            decay *= d.hip_spread_prone_decay();
            frac = (ps.spread_override as f32 - d.hip_spread_prone_min())
                / (d.hip_spread_prone_max() - d.hip_spread_prone_min());
        } else if ps.e_flags & e_flags::CROUCHING != 0 {
            decay *= d.hip_spread_ducked_decay();
            frac = (ps.spread_override as f32 - d.hip_spread_ducked_min())
                / (d.hip_spread_ducked_max() - d.hip_spread_ducked_min());
        }
        if ps.spread_override_state == 1 {
            decrease = (pml.frametime / frac) * decay;
        } else {
            let ft = pml.frametime;
            decrease = ft * decay;
            if ps.weapon_pos_frac != 1.0 {
                let mut add = 0.0f32;
                if d.hip_spread_turn_add() != 0.0 {
                    for axis in 0..2 {
                        let turn = (pm.cmd.angles[axis] as f32 * 0.00549316406
                            - pm.oldcmd.angles[axis] as f32 * 0.00549316406)
                            * 0.00277777785;
                        let turn = (turn - floor_half(turn)) * 360.0;
                        add += turn.abs() * d.hip_spread_turn_add() * 0.00999999978 * (1.0 / ft);
                    }
                }
                if d.hip_spread_move_add() != 0.0
                    && (pm.cmd.forwardmove != 0 || pm.cmd.rightmove != 0)
                {
                    let v = ps.velocity[0] * ps.velocity[0] + ps.velocity[1] * ps.velocity[1];
                    let t = tuning::AIM_SPREAD_MOVE_SPEED_THRESHOLD;
                    if t * t < v {
                        add += libm::sqrtf(v) * d.hip_spread_move_add() / ps.speed as f32;
                    }
                }
                if in_air {
                    add = add + 1.27999997 + 1.27999997;
                }
                if 0.0 < ft * add {
                    increase = Some(ft * add * 255.0 + ps.aim_spread_scale);
                }
            }
        }
    }
    let ps = &mut *pm.ps;
    let scale = increase.unwrap_or(ps.aim_spread_scale - decrease * 255.0);
    ps.aim_spread_scale = scale;
    if ps.spread_override_state == 1 && scale * frac < max {
        ps.spread_override_state = 0;
        ps.aim_spread_scale = scale * frac;
    }
    if ps.aim_spread_scale < 0.0 {
        ps.aim_spread_scale = 0.0;
    } else if max < ps.aim_spread_scale {
        ps.aim_spread_scale = max;
    }
}

/// `floor(x + 0.5)` the way the game rounds with the 2^23 trick.
fn floor_half(x: f32) -> f32 {
    let y = x + 0.5;
    let big = f32::from_bits(0x4b00_0000 | (y.to_bits() & 0x8000_0000));
    let r = (y + big) - big;
    if y < r { r - 1.0 } else { r }
}

/// Counts the hand's weapon time and delay down, faster with Speed Cola while
/// reloading and with Double Tap while firing; a burst's end waits for
/// `player_burstFireCooldown`. True when the delay ran out this frame.
fn weapon_timers<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) -> bool {
    let hand = Hand::of(pm.ps);
    let weapon = hand_weapon(pm.ps, pm.world);
    let d = def(pm, weapon).def;
    let ps = &mut *pm.ps;
    if ps.weapon_restrict_kick_time > 0 {
        ps.weapon_restrict_kick_time = (ps.weapon_restrict_kick_time - pml.msec).max(0);
    }
    let s = hand.state(ps);
    let perks = ps.perks;
    let instant = |ps: &crate::PlayerState| hand.delay(ps).max(hand.time(ps));
    let step = if ws::is_reloading(s) && perks & crate::perks::FAST_RELOAD != 0 {
        if tuning::PERK_WEAP_RELOAD_MULTIPLIER == 0.0 {
            instant(ps)
        } else {
            let level = ((perks >> 6) & 3) as f32 * 0.333333343;
            let msec = pml.msec as f32;
            crate::math::round_nudged(
                (msec / tuning::PERK_WEAP_RELOAD_MULTIPLIER) * level + (1.0 - level) * msec,
            )
        }
    } else if matches!(
        s,
        ws::FIRING | ws::RECHAMBERING | ws::RECHAMBER_END | ws::CONT_FIRE_LOOP
    ) && perks & crate::perks::RATE_OF_FIRE != 0
    {
        if tuning::PERK_WEAP_RATE_MULTIPLIER == 0.0 {
            instant(ps)
        } else {
            crate::math::round_nudged(pml.msec as f32 / tuning::PERK_WEAP_RATE_MULTIPLIER)
        }
    } else {
        pml.msec
    };
    if hand.time(ps) != 0 {
        hand.set_time(ps, hand.time(ps) - step);
        let held_burst = d.fire_type() == fire_type::STACKED && mid_burst(pm);
        let ps = &mut *pm.ps;
        if held_burst {
            hand.set_time(ps, 0);
        }
        if held_burst || hand.time(ps) <= 0 {
            let s = hand.state(ps);
            if matches!(s, ws::FIRING | ws::CONT_FIRE_LOOP)
                && weapon != 0
                && (fire_type::BURST_2..=fire_type::BURST_4)
                    .contains(&def(pm, weapon).def.fire_type())
                && !mid_burst(pm)
            {
                let ps = &mut *pm.ps;
                let cooldown = if tuning::BURST_FIRE_COOLDOWN == 0.0 {
                    1
                } else {
                    crate::math::round_nudged(tuning::BURST_FIRE_COOLDOWN * 1000.0)
                };
                hand.set_time(ps, cooldown);
                start_anim_unless_playing(ps, weap_anim::IDLE, false);
                hand.set_state(ps, ws::READY);
                return false;
            }
            let ps = &*pm.ps;
            let latch = ps.weap_flags & weap_flags::FIRE_LATCH != 0;
            let burst_done = !latch && burst_complete(ps, &def(pm, weapon).def);
            let throw_held = matches!(d.weapon_type(), weapon_type::GRENADE | weapon_type::MINE)
                && d.hold_button_to_throw();
            let fire_button = if ps.left_hand {
                buttons::THROW
            } else {
                buttons::ATTACK
            };
            let fire_held = pm.cmd.buttons.held(fire_button);
            let s = hand.state(ps);
            let mut keep_time = false;
            let mut released = !(!ws::is_offhand(s) && (burst_done || throw_held));
            let mut reset_shots = false;
            if !released {
                if fire_held {
                    let ammo = inv::clip(ps, &def(pm, hand_weapon(ps, pm.world)));
                    if ps.weapon == pm.cmd.weapon && (ammo != 0 || d.unlimited_ammo()) {
                        keep_time = true;
                        let ps = &mut *pm.ps;
                        hand.set_time(ps, 1);
                        match hand.state(ps) {
                            s if ws::is_reloading(s) => {
                                hand.set_time(ps, 0);
                                hand.set_shot_count(ps, 0);
                            }
                            ws::RECHAMBER_END => finish_rechamber(ps),
                            ws::FIRING
                            | ws::RECHAMBERING
                            | ws::CONT_FIRE_LOOP
                            | ws::MELEE_INIT
                            | ws::MELEE_FIRE
                            | ws::MELEE_END => {
                                start_anim_unless_playing(ps, weap_anim::IDLE, false);
                                hand.set_state(ps, ws::READY);
                            }
                            _ => {}
                        }
                    } else {
                        released = true;
                    }
                } else {
                    reset_shots = true;
                }
            }
            if released && (!fire_held || latch) {
                reset_shots = true;
            }
            if reset_shots && !mid_burst(pm) {
                let ps = &mut *pm.ps;
                hand.set_shot_count(ps, 0);
            }
            if !keep_time {
                let ps = &mut *pm.ps;
                hand.set_time(ps, 0);
            }
        }
    }
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

/// Black Ops' weapon types.
pub mod weapon_type {
    pub const BULLET: i32 = 0;
    pub const GRENADE: i32 = 1;
    pub const PROJECTILE: i32 = 2;
    pub const GAS: i32 = 5;
    pub const MINE: i32 = 6;
}

/// Enough shots for the weapon's fire type have been fired in this press.
fn burst_complete(ps: &crate::PlayerState, d: &fastfile_t5::weapon_def::WeaponDefView<'_>) -> bool {
    let shots = if ps.left_hand {
        ps.weapon_shot_count_left
    } else {
        ps.weapon_shot_count
    } as u32;
    match d.fire_type() {
        fire_type::SINGLE_SHOT => shots != 0,
        fire_type::BURST_2 => shots > 1,
        fire_type::BURST_3 => shots > 2,
        fire_type::BURST_4 => shots > 3,
        fire_type::STACKED => shots >= ps.stack_fire_count as u32,
        _ => false,
    }
}

/// A burst is under way: shots fired, ammo left, and not all of them yet.
fn mid_burst<W: MoveWorld>(pm: &Pm<'_, W>) -> bool {
    let weapon = hand_weapon(pm.ps, pm.world);
    let w = def(pm, weapon);
    let ps = &*pm.ps;
    let shots = if ps.left_hand {
        ps.weapon_shot_count_left
    } else {
        ps.weapon_shot_count
    };
    if weapon == 0
        || w.def.fire_type() == fire_type::FULL_AUTO
        || w.def.fire_type() == fire_type::MINIGUN
        || shots == 0
    {
        return false;
    }
    if inv::stock(ps, &w) == 0 && inv::clip(ps, &w) == 0 {
        return false;
    }
    !burst_complete(ps, &w.def)
}

/// The shot about to fire is the last of the burst (the burst's last fire time
/// applies).
fn last_shot_of_burst<W: MoveWorld>(pm: &Pm<'_, W>, weapon: u32) -> bool {
    let w = def(pm, weapon);
    let clip = inv::clip(pm.ps, &w);
    match w.def.fire_type() {
        fire_type::BURST_2 => clip < 3,
        fire_type::BURST_3 => clip < 4,
        fire_type::BURST_4 => clip < 5,
        _ => clip < 2,
    }
}

/// A bolt-action weapon that needs its bolt worked starts rechambering when
/// it is ready; true while that holds the weapon.
fn rechamber<W: MoveWorld>(pm: &mut Pm<'_, W>, delay_done: bool) -> bool {
    let d = def(pm, pm.ps.weapon).def;
    let ps = &mut *pm.ps;
    let s = ps.weaponstate;
    if ws::is_offhand(s)
        || ws::is_dtp(s)
        || !d.bolt_action()
        || !inv::needs_rechamber(ps, ps.weapon)
    {
        return false;
    }
    if s == ws::RECHAMBERING && delay_done {
        ps.weaponstate = ws::RECHAMBER_END;
        pm.event(event::EJECT_BRASS, 0);
        if pm.ps.weapon_time != 0 {
            return true;
        }
    } else if ps.weapon_time != 0
        && (matches!(
            s,
            ws::FIRING
                | ws::RECHAMBERING
                | ws::RECHAMBER_END
                | ws::CONT_FIRE_LOOP
                | ws::MELEE_INIT
                | ws::MELEE_FIRE
                | ws::MELEE_END
        ) || ps.weapon_delay != 0)
    {
        return false;
    }
    let ps = &mut *pm.ps;
    if ps.weaponstate == ws::RECHAMBER_END {
        finish_rechamber(ps);
        return false;
    }
    if ps.weaponstate == ws::READY {
        if ps.weapon_pos_frac <= 0.75 {
            start_anim(ps, weap_anim::RECHAMBER);
        } else {
            start_anim(ps, weap_anim::ADS_RECHAMBER);
        }
        ps.weaponstate = ws::RECHAMBERING;
        ps.weapon_time = d.rechamber_time();
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
fn finish_rechamber(ps: &mut crate::PlayerState) {
    let playing = ps.weap_anim & !weap_anim::TOGGLE;
    let playing_left = ps.weap_anim_left & !weap_anim::TOGGLE;
    if (playing != 0 || ps.left_hand) && (playing_left != 0 || !ps.left_hand) {
        start_anim(ps, weap_anim::IDLE);
    }
    let weapon = ps.weapon;
    inv::set_needs_rechamber(ps, weapon, false);
    ps.weaponstate = ws::READY;
}

/// When during a reload the ammo goes in: the add time, capped by the
/// reload's own time; a bolt-action weapon waits for its bolt.
fn reload_add_delay<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let hand = Hand::of(pm.ps);
    let weapon = hand_weapon(pm.ps, pm.world);
    let w = def(pm, weapon);
    let d = w.def;
    let ps = &*pm.ps;
    let s = hand.state(ps);
    let mut add = if matches!(s, ws::RELOAD_START | ws::RELOAD_START_INTERRUPT) {
        let t = d.reload_start_add_time();
        if t == 0 {
            0
        } else {
            t.min(d.reload_start_time())
        }
    } else {
        let quick = inv::dual_mag_in(ps, &w, weapon);
        let base = if quick {
            d.reload_quick_add_time()
        } else {
            d.reload_add_time()
        };
        let (reload, add_at) = if inv::clip(ps, &w) == 0 && d.weapon_type() == weapon_type::BULLET {
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
        if d.rechamber_bolt_time() < add {
            add = d.rechamber_bolt_time();
        }
        if add == 0 {
            add = 1;
        }
        hand.set_delay(ps, add);
        return;
    }
    if add != 0 {
        hand.set_delay(ps, add);
    }
}

/// The reload animation and time, by an empty or loaded clip and the quick
/// magazine.
fn start_reload_anim<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let hand = Hand::of(pm.ps);
    let weapon = hand_weapon(pm.ps, pm.world);
    let w = def(pm, weapon);
    let d = w.def;
    if d.unlimited_ammo() {
        return;
    }
    let ps = &mut *pm.ps;
    let quick = d.dual_mag() && inv::dual_mag_in(ps, &w, weapon);
    let (anim, time, ev) = if inv::clip(ps, &w) == 0 && d.weapon_type() == weapon_type::BULLET {
        if quick {
            (
                weap_anim::RELOAD_QUICK_EMPTY,
                d.reload_quick_empty_time(),
                event::RELOAD_FROM_EMPTY,
            )
        } else {
            (
                weap_anim::RELOAD_EMPTY,
                d.reload_empty_time(),
                event::RELOAD_FROM_EMPTY,
            )
        }
    } else if quick {
        (
            weap_anim::RELOAD_QUICK,
            d.reload_quick_time(),
            event::RELOAD,
        )
    } else {
        (weap_anim::RELOAD, d.reload_time(), event::RELOAD)
    };
    start_anim_hand(ps, anim);
    hand.set_time(ps, time);
    pm.event(ev, 0);
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
    let w = def(pm, ps.weapon);
    if !w.def.dual_wield() {
        return false;
    }
    let left = def(pm, w.dual_wield_weapon);
    ps.weaponstate == ws::RELOADING
        && ps.weaponstate_left == ws::RELOADING
        && (w.def.reload_time() >> 1) < ps.weapon_time
        && (left.def.reload_time() >> 1) < ps.weapon_time_left
}

/// Starts the hand's reload from a state it may reload from.
fn begin_reload<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let hand = Hand::of(pm.ps);
    let weapon = hand_weapon(pm.ps, pm.world);
    let d = def(pm, weapon).def;
    let s = hand.state(pm.ps);
    let from = matches!(
        s,
        ws::READY | ws::FIRING | ws::CONT_FIRE_LOOP | ws::RECHAMBERING | ws::RECHAMBER_END
    ) || (ws::SPRINT_RAISE..=ws::SPRINT_DROP).contains(&s)
        || (ws::LOWREADY_RAISE..=ws::LOWREADY_DROP).contains(&s);
    if !from
        || weapon == 0
        || !pm.world.weapon_exists(weapon)
        || pm.ps.e_flags2 & e_flags2::MOUNTED != 0
        || mantling_anim(pm.ps)
    {
        return;
    }
    let ps = &mut *pm.ps;
    hand.set_shot_count(ps, 0);
    pm.event(event::RESET_ADS, 0);
    pm.event(event::RELOAD_START_NOTIFY, 0);
    if d.segmented_reload() && d.reload_start_time() != 0 {
        let ps = &mut *pm.ps;
        start_anim_hand(ps, weap_anim::RELOAD_START);
        hand.set_time(ps, d.reload_start_time());
        hand.set_state(ps, ws::RELOAD_START);
        pm.event(event::RELOAD_START, 0);
        reload_add_delay(pm);
    } else {
        start_reload_anim(pm);
    }
    if d.dual_wield() {
        if dual_reloads_half_done(pm) && !def(pm, weapon).def.clip_only() {
            pm.anim_event(anim::RELOAD, false, true);
        }
        if d.dual_wield() {
            return;
        }
    }
    if !d.clip_only() {
        pm.anim_event(anim::RELOAD, false, true);
    }
}

/// Mantling with the mantle viewmodel animation up.
fn mantling_anim(ps: &crate::PlayerState) -> bool {
    ps.pm_flags & pm_flags::MANTLE != 0
        && (!tuning::MANTLE_ENABLE || ps.mantle_flags & mantle_flags::NO_WEAPON != 0)
}

/// Black Ops' mantle state flags.
pub mod mantle_flags {
    /// The weapon is down while mantling.
    pub const NO_WEAPON: u32 = 0x40;
}

/// The reload's delay ran out: the ammo goes in, or a bolt-action weapon
/// works its bolt first.
fn reload_add_ammo<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let hand = Hand::of(pm.ps);
    let weapon = hand_weapon(pm.ps, pm.world);
    let w = def(pm, weapon);
    let d = w.def;
    if d.bolt_action() && inv::needs_rechamber(pm.ps, weapon) {
        inv::set_needs_rechamber(pm.ps, weapon, false);
        pm.event(event::EJECT_BRASS, 0);
        let ps = &mut *pm.ps;
        let s = hand.state(ps);
        let start = matches!(s, ws::RELOAD_START | ws::RELOAD_START_INTERRUPT);
        if start && d.reload_start_add_time() == 0 {
            return;
        }
        if hand.time(ps) != 0 {
            let mut t = if start {
                d.reload_start_add_time().min(d.reload_start_time())
            } else {
                let mut add = d.reload_add_time();
                let reload = if inv::clip(ps, &w) == 0 && d.weapon_type() == weapon_type::BULLET {
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
            t -= if bolt < t { bolt } else { 1 };
            if t >= 1 {
                hand.set_delay(ps, t);
                return;
            }
        }
        transfer_ammo(pm);
        return;
    }
    transfer_ammo(pm);
    let held = pm.ps.weapon;
    let held_w = def(pm, held);
    let quick = inv::dual_mag_in(pm.ps, &held_w, held);
    if w.def.dual_mag() {
        inv::set_dual_mag(pm.ps, &held_w, held, !quick);
    }
}

/// Moves rounds from beside the clip into it: as many as fit, at most the
/// reload's add per step.
fn transfer_ammo<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let hand = Hand::of(pm.ps);
    let weapon = hand_weapon(pm.ps, pm.world);
    let w = def(pm, weapon);
    let d = w.def;
    let s = hand.state(pm.ps);
    let start = matches!(s, ws::RELOAD_START | ws::RELOAD_START_INTERRUPT);
    if start && d.reload_start_add() == 0 {
        return;
    }
    let ps = &mut *pm.ps;
    let stock = inv::stock(ps, &w);
    let mut in_clip = inv::clip(ps, &w);
    if d.fuel_tank_weapon() {
        in_clip = 0;
        inv::set_clip(ps, &w, 0);
    }
    let size = inv::clip_size(&w);
    let mut n = (size - in_clip).min(stock);
    let add = if start {
        Some(d.reload_start_add())
    } else {
        Some(d.reload_ammo_add()).filter(|&a| a != 0)
    };
    if let Some(add) = add
        && add < size
        && add < n
    {
        n = add;
    }
    if n != 0 {
        inv::add_stock(ps, &w, -n);
        let count = n + inv::clip(ps, &w);
        inv::set_clip(ps, &w, if count < 1 { 0 } else { count });
        if d.fuel_tank_weapon() {
            let held = ps.weapon;
            inv::set_fuel(ps, held, 0);
        }
        pm.event(event::RELOAD_ADDAMMO, 0);
    }
}

/// Either gun of the held weapon has rounds in its clip.
fn any_clip<W: MoveWorld>(pm: &Pm<'_, W>) -> bool {
    let ps = &*pm.ps;
    let w = def(pm, ps.weapon);
    if inv::clip(ps, &w) != 0 {
        return true;
    }
    inv::clip(ps, &def(pm, w.dual_wield_weapon)) != 0
}

/// Out of every round: the empty click.
fn no_ammo_sound<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let ps = &*pm.ps;
    let w = def(pm, ps.weapon);
    let d = w.def;
    if d.unlimited_ammo() || ps.weap_flags & weap_flags::CYCLING_DISABLED != 0 {
        return;
    }
    if inv::clip(ps, &w) == 0
        && inv::stock(ps, &w) == 0
        && !any_clip(pm)
        && !d.has_detonator()
        && d.guided_missile_type() != 6
        && d.guided_missile_type() != 5
    {
        pm.event(event::NOAMMO, 0);
    }
}

/// The hand may fire: rounds in its clip; with none it reloads from stock,
/// or clicks empty and waits.
fn has_ammo_to_fire<W: MoveWorld>(pm: &mut Pm<'_, W>) -> bool {
    let hand = Hand::of(pm.ps);
    let weapon = hand_weapon(pm.ps, pm.world);
    let w = def(pm, weapon);
    let d = w.def;
    if hand_clip(pm.ps, pm.world) > 0 {
        return true;
    }
    if pm.ps.weap_flags & weap_flags::NO_FIRE != 0 {
        return false;
    }
    let stock = inv::stock(pm.ps, &w);
    let thrown = matches!(d.weapon_type(), weapon_type::GRENADE | weapon_type::MINE);
    if !d.no_ads_auto_reload() || pm.ps.weapon_pos_frac != 1.0 {
        if stock > 0 {
            begin_reload(pm);
            return false;
        }
        if !thrown {
            pm.event(
                if pm.ps.left_hand {
                    event::NOAMMO_LEFT
                } else {
                    event::NOAMMO
                },
                0,
            );
        }
    }
    if !mantling_anim(pm.ps) {
        let left = pm.ps.left_hand;
        let ps = &mut *pm.ps;
        inv::set_needs_rechamber(ps, weapon, false);
        start_anim_unless_playing(ps, weap_anim::IDLE, left);
        if !thrown {
            hand.set_time(ps, hand.time(ps) + 500);
        }
    }
    false
}

/// Times a shot: fire time (the burst's last fire time on its last shot), fire
/// delay, the sights' delay for ADS-fire weapons, kick restriction; grenades
/// are pulled back and cooked.
fn fire_timing<W: MoveWorld>(pm: &mut Pm<'_, W>, delay_done: bool) {
    let hand = Hand::of(pm.ps);
    let weapon = hand_weapon(pm.ps, pm.world);
    let w = def(pm, weapon);
    let d = w.def;
    let thrown = matches!(d.weapon_type(), weapon_type::GRENADE | weapon_type::MINE);
    let mut fire_anim = false;
    if thrown {
        if !delay_done {
            let ps = &mut *pm.ps;
            if hand_clip(ps, pm.world) != 0 {
                ps.grenade_time_left = d.fuse_time();
                start_anim_both(ps, weap_anim::HOLD_FIRE);
                let held = ps.weapon as i32;
                pm.event(event::PULLBACK_WEAPON, held);
            }
            let ps = &mut *pm.ps;
            ps.weapon_delay = d.hold_fire_time();
            ps.weapon_time = 0;
        } else {
            fire_anim = true;
        }
    } else {
        let last = last_shot_of_burst(pm, weapon) && d.last_fire_time() != 0;
        let ps = &mut *pm.ps;
        hand.set_time(
            ps,
            if last {
                d.last_fire_time()
            } else {
                d.fire_time()
            },
        );
        if d.bolt_action() {
            inv::set_needs_rechamber(ps, weapon, true);
        }
        let s = hand.state(ps);
        if s != ws::FIRING && s != ws::CONT_FIRE_LOOP {
            hand.set_delay(ps, d.fire_delay());
            let kick = if ps.weapon_pos_frac < 1.0 {
                d.hip_gun_kick_reduced_kick_bullets()
            } else {
                d.ads_gun_kick_reduced_kick_bullets()
            };
            ps.weapon_restrict_kick_time = kick * d.fire_time() + d.fire_delay();
        }
        if d.ads_fire() && d.fire_type() != fire_type::MINIGUN {
            let sights = ((1.0 - ps.weapon_pos_frac) * (1.0 / ads_in_rate(&w))) as i32;
            hand.set_delay(ps, hand.delay(ps).max(sights));
        }
        if d.guided_missile_type() == 6 {
            ps.weap_flags |= weap_flags::FIRING_GUIDED;
        }
        let shellshocked = ps.pm_flags & pm_flags::SHELLSHOCKED != 0;
        let settled = ps.view_height_current == ps.view_height_target as f32;
        if shellshocked {
            pm.gap(crate::Gap::Shellshock);
        }
        fire_anim = settled;
    }
    if fire_anim {
        pm.anim_event(anim::FIRE_WEAPON, false, true);
    }
    let ps = &mut *pm.ps;
    hold_prone(ps);
    let s = hand.state(ps);
    if !(ws::CONT_FIRE_IN..=ws::CONT_FIRE_OUT).contains(&s) {
        hand.set_state(ps, ws::FIRING);
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
    let empty = hand_clip_empty(pm.ps, pm.world);
    let ps = &mut *pm.ps;
    let anim = if ps.weapon_pos_frac <= 0.75 {
        if empty {
            weap_anim::LAST_SHOT
        } else {
            weap_anim::FIRE
        }
    } else if empty {
        weap_anim::ADS_LAST_SHOT
    } else {
        weap_anim::ADS_FIRE
    };
    start_anim_hand(ps, anim);
}

/// Each hip shot widens the spread.
fn spread_fire_add<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let d = def(pm, pm.ps.weapon).def;
    let ps = &mut *pm.ps;
    if ps.weapon_pos_frac != 1.0 {
        let scale = d.hip_spread_fire_add() * 255.0 + ps.aim_spread_scale;
        ps.aim_spread_scale = scale;
        if 255.0 < scale {
            ps.aim_spread_scale = 255.0;
        }
    }
}

/// A special fire: two seconds of its own animation.
fn special_fire<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let ps = &mut *pm.ps;
    ps.weaponstate = ws::FIRING_SPECIAL;
    ps.weapon_time = 2000;
    ps.weapon_delay = 2000;
    start_anim(ps, weap_anim::SPECIAL_FIRE);
    hold_prone(ps);
    pm.event(event::RESET_ADS, 0);
}

/// Black Ops' special fire button.
const SPECIAL_FIRE_BUTTON: u32 = 38;

/// The fire button (or a dual wield's left trigger) is held, or a burst or a
/// shot in progress keeps firing.
fn fire_held<W: MoveWorld>(pm: &mut Pm<'_, W>, delay_done: bool, continuous: bool) -> bool {
    let ps = &*pm.ps;
    if ps.pm_flags & pm_flags::LOW_READY != 0 {
        return false;
    }
    let d = def(pm, ps.weapon).def;
    let detonator =
        matches!(d.weapon_type(), weapon_type::GRENADE | weapon_type::MINE) && d.has_detonator();
    let mut fire = pm.cmd.buttons.held(if detonator {
        buttons::THROW
    } else {
        buttons::ATTACK
    });
    if !d.dual_wield() || !pm.cmd.buttons.held(buttons::THROW) {
        if ps.left_hand && d.dual_wield() {
            fire = false;
        }
    } else if ps.left_hand {
        fire = true;
    }
    if d.freeze_movement_when_firing() && ps.ground_entity_num == crate::state::ENTITYNUM_NONE {
        fire = false;
    }
    if d.can_use_in_vehicle() {
        fire = false;
    }
    let busy = delay_done || mid_burst(pm);
    if d.fire_type() == fire_type::MINIGUN {
        if fire && pm.ps.weapon_spin_lerp >= 1.0 {
            return true;
        }
    } else if fire {
        return true;
    }
    if busy {
        return true;
    }
    if !continuous {
        let ps = &mut *pm.ps;
        if ps.left_hand && d.dual_wield() {
            if ps.weaponstate_left == ws::FIRING {
                start_anim_unless_playing(ps, weap_anim::IDLE, true);
            }
            ps.weaponstate_left = ws::READY;
            return false;
        }
        if ps.weaponstate == ws::FIRING && !mantling_anim(ps) {
            start_anim_unless_playing(ps, weap_anim::IDLE, false);
        }
        ps.weaponstate = ws::READY;
    }
    false
}

/// A thrown weapon that is not held to throw has its fire button down.
fn grenade_fire_held<W: MoveWorld>(pm: &Pm<'_, W>) -> bool {
    let weapon = pm.ps.weapon;
    if weapon == 0 {
        return false;
    }
    let d = def(pm, weapon).def;
    if !matches!(d.weapon_type(), weapon_type::GRENADE | weapon_type::MINE)
        || d.hold_button_to_throw()
    {
        return false;
    }
    pm.cmd.buttons.held(if d.has_detonator() {
        buttons::THROW
    } else {
        buttons::ATTACK
    })
}

/// Fires the hand's weapon when it can: a mine needs soft ground below; the
/// shot's timing, ammo, lock-on, events and spread follow.
fn fire<W: MoveWorld>(pm: &mut Pm<'_, W>, delay_done: bool) {
    let d = def(pm, pm.ps.weapon).def;
    if pm.ps.water_level >= 3 && tuning::DISABLE_WEAPONS_IN_WATER {
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
    if d.overheat_weapon() != 0 {
        pm.gap(crate::Gap::Overheat);
    }
    if d.guided_missile_type() == 6 && pm.ps.ground_entity_num == crate::state::ENTITYNUM_NONE {
        return;
    }
    if !d.unlimited_ammo() && !has_ammo_to_fire(pm) {
        return;
    }
    fire_timing(pm, delay_done);
    let ps = &*pm.ps;
    let left = ps.left_hand;
    if !((ps.weapon_delay == 0 || left) && (ps.weapon_delay_left == 0 || !left)) {
        return;
    }
    let ps = &mut *pm.ps;
    ps.weap_flags &= !weap_flags::FIRE_LATCH;
    if d.require_lockon_to_fire() {
        let lock = ps.weap_lock_flags;
        if lock & 2 == 0 {
            pm.event(event::LOCKON_REQUIRED_HINT, 0);
            return;
        }
        if lock & 0x10 != 0 {
            pm.event(event::TARGET_TOO_CLOSE_HINT, 0);
            return;
        }
        if lock & 0x20 != 0 {
            pm.event(event::TARGET_NOT_ENOUGH_CLEARANCE, 0);
            return;
        }
    }
    if hand_clip(pm.ps, pm.world) != 0 && !d.fuel_tank_weapon() {
        let ps = &*pm.ps;
        let turret = ps.e_flags & e_flags::TURRET != 0;
        if !turret {
            let used = if left && d.dual_wield() {
                def(pm, ps.weapon).dual_wield_weapon
            } else {
                ps.weapon
            };
            let w = def(pm, used);
            inv::use_clip(pm.ps, &w, 1);
        }
    }
    let ps = &mut *pm.ps;
    if matches!(d.weapon_type(), weapon_type::GRENADE | weapon_type::MINE) {
        ps.weapon_time = d.fire_time();
    }
    if !pm.cmd.buttons.held(SPECIAL_FIRE_BUTTON) {
        let s = pm.ps.weaponstate;
        if !(ws::CONT_FIRE_IN..=ws::CONT_FIRE_OUT).contains(&s) {
            fire_anim(pm);
        }
        if d.guided_missile_type() == 6 {
            pm.ps.weap_flags |= weap_flags::GUIDED_MISSILE;
        }
        let empty = hand_clip_empty(pm.ps, pm.world);
        let ev = match (empty, left) {
            (false, false) => event::FIRE_WEAPON,
            (false, true) => event::FIRE_WEAPON_LEFT,
            (true, false) => event::FIRE_WEAPON_LASTSHOT,
            (true, true) => event::FIRE_WEAPON_LASTSHOT_LEFT,
        };
        let shots = if left {
            pm.ps.weapon_shot_count_left
        } else {
            pm.ps.weapon_shot_count
        };
        pm.event(ev, shots);
    } else {
        special_fire(pm);
    }
    hold_breath_fire(pm);
    spread_fire_add(pm);
    no_ammo_sound(pm);
}

/// A burst weapon's fire press latches until the burst is fired.
fn latch_fire<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let d = def(pm, pm.ps.weapon).def;
    if d.fire_type() != fire_type::FULL_AUTO
        && d.fire_type() != fire_type::SINGLE_SHOT
        && pm.cmd.buttons.held(buttons::ATTACK)
        && !pm.oldcmd.buttons.held(buttons::ATTACK)
    {
        pm.ps.weap_flags |= weap_flags::FIRE_LATCH;
    }
}

/// A stacked-fire weapon stacks a shot each aimed tap of hold breath.
fn stack_fire<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let w = def(pm, pm.ps.weapon);
    let d = w.def;
    if d.fire_type() != fire_type::STACKED || d.stack_fire() as u32 <= 1 {
        return;
    }
    if pm.ps.stack_fire_count == 0 {
        pm.ps.stack_fire_count = 1;
    }
    let clip = inv::clip(pm.ps, &w) as u32;
    if pm.ps.weapon_pos_frac >= 1.0
        && pm.cmd.buttons.held(buttons::HOLD_BREATH)
        && !pm.oldcmd.buttons.held(buttons::HOLD_BREATH)
    {
        pm.event(event::STACKFIRE, 0);
        let ps = &mut *pm.ps;
        ps.stack_fire_count += 1;
        if (d.stack_fire() as u32) < (ps.stack_fire_count as u32)
            || clip < ps.stack_fire_count as u32
        {
            ps.stack_fire_count = 1;
        }
    }
    if !mid_burst(pm) && clip < pm.ps.stack_fire_count as u32 && clip != 0 {
        pm.ps.stack_fire_count = clip as i32;
    }
}

/// The weapon that does the hand's melee: the held one when it is a bayonet
/// or a melee weapon, else the player's melee weapon.
fn melee_weapon<W: MoveWorld>(pm: &Pm<'_, W>) -> u32 {
    let d = def(pm, pm.ps.weapon).def;
    if d.bayonet() || d.use_as_melee() {
        pm.ps.weapon
    } else {
        pm.ps.melee_weapon
    }
}

/// The melee swing lands.
fn melee_fire<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    pm.ps.weaponstate = ws::MELEE_FIRE;
    pm.event(event::FIRE_MELEE, 0);
    hold_prone(pm.ps);
}

/// The melee starts: a swing, or a lunge when the target is past melee range.
fn melee_start<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let d = def(pm, melee_weapon(pm)).def;
    let range = if d.bayonet() {
        tuning::BAYONET_RANGE
    } else {
        tuning::MELEE_RANGE
    };
    let ps = &mut *pm.ps;
    if ps.pm_flags & pm_flags::LAUNCHED == 0 || ps.melee_charge_dist as f32 <= range {
        ps.weapon_time = d.melee_time();
        ps.weapon_delay = d.melee_delay();
        start_anim(ps, weap_anim::MELEE);
    } else {
        ps.weapon_time = d.melee_charge_time();
        ps.weapon_delay = d.melee_charge_delay();
        start_anim(ps, weap_anim::MELEE_CHARGE);
    }
    pm.anim_event(anim::MELEE_ATTACK, false, true);
    pm.ps.weaponstate = ws::MELEE_INIT;
    pm.event(event::MELEE_SWIPE, 0);
    hold_prone(pm.ps);
}

/// After the swing: the held weapon comes back quickly if the melee used
/// another one.
fn melee_end<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let held = pm.ps.weapon;
    let swung = melee_weapon(pm);
    let d = def(pm, held).def;
    if !def(pm, swung).def.same_def(&d) {
        let ps = &mut *pm.ps;
        ps.weaponstate = ws::MELEE_END;
        ps.weapon_time = d.quick_raise_time();
        ps.weapon_delay = 0;
        if ps.water_level < 3 {
            start_anim(ps, weap_anim::QUICK_RAISE);
        }
        hold_prone(ps);
        return;
    }
    finish(pm.ps);
}

/// A melee lunge toward the client's target, when on the ground and free to.
fn melee_charge<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let mut dist = pm.cmd.melee_charge_dist;
    let mut yaw = pm.cmd.melee_charge_yaw;
    if tuning::BAYONET_LAUNCH_DEBUGGING {
        dist = 0xfa;
        yaw = pm.ps.viewangles[1];
    }
    let ps = &mut *pm.ps;
    let in_air = ps.ground_entity_num == crate::state::ENTITYNUM_NONE
        && ps.pm_type != pm_type::NORMAL_LINKED
        && ps.pm_type != pm_type::LAST_STAND;
    if ps.pm_flags & pm_flags::LAUNCHED == 0
        && dist != 0
        && ps.pm_type == pm_type::NORMAL
        && ps.e_flags & e_flags::TURRET == 0
        && ps.pm_flags & (pm_flags::MANTLE | pm_flags::LADDER) == 0
        && !in_air
    {
        ps.pm_flags |= pm_flags::LAUNCHED;
        ps.melee_charge_yaw = yaw;
        ps.melee_charge_dist = i32::from(dist);
        ps.melee_charge_time = 0;
        return;
    }
    ps.pm_flags &= !pm_flags::LAUNCHED;
    ps.melee_charge_yaw = 0.0;
    ps.melee_charge_dist = 0;
    ps.melee_charge_time = 0;
    if tuning::BAYONET_LAUNCH_PROOF {
        let cap = tuning::BAYONET_LAUNCH_Z_CAP;
        if cap - ps.velocity[2] < 0.0 {
            ps.velocity[2] = cap;
        }
    }
}

/// A frag throw cancelled by its key: the cook is dropped.
fn cancel_grenade<W: MoveWorld>(pm: &mut Pm<'_, W>) -> bool {
    let ps = &mut *pm.ps;
    if ps.weap_flags & weap_flags::CANCEL_GRENADE != 0 && ps.e_flags2 & e_flags2::GRENADE_HELD == 0
    {
        ps.grenade_time_left = 0;
        return true;
    }
    false
}

/// A smoke throw cancelled by its key.
fn cancel_smoke<W: MoveWorld>(pm: &mut Pm<'_, W>) -> bool {
    let ps = &mut *pm.ps;
    if ps.weap_flags & weap_flags::CANCEL_SMOKE == 0 {
        return false;
    }
    ps.grenade_time_left = 0;
    true
}

/// Starts a melee when its key is pressed (and, outside zombies, attack with
/// a bayonet or melee weapon) and nothing holds the weapon.
fn check_melee<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml, delay_done: bool) {
    let ps = &*pm.ps;
    if ps.water_level >= 3 && tuning::DISABLE_WEAPONS_IN_WATER {
        return;
    }
    if ps.pm_flags & (pm_flags::LADDER | pm_flags::LADDER_FALL) != 0
        || ps.weap_flags & weap_flags::WEAPONS_DISABLED != 0
    {
        return;
    }
    let weapon_def = def(pm, ps.weapon).def;
    let (melee, attack_melees) = {
        let held = def(pm, ps.weapon).def;
        if held.bayonet() || held.use_as_melee() {
            (ps.weapon, !pm.zombiemode)
        } else {
            (ps.melee_weapon, false)
        }
    };
    let s = ps.weaponstate;
    if ps.pm_flags & pm_flags::NO_MELEE != 0
        || ws::is_melee(s)
        || ws::is_offhand(s)
        || matches!(s, ws::NIGHTVISION_WEAR | ws::NIGHTVISION_REMOVE)
        || ws::is_dtp(s)
        || ps.pm_flags & pm_flags::MANTLE != 0
    {
        return;
    }
    let cmd = pm.cmd.buttons;
    let old = pm.oldcmd.buttons;
    if ((attack_melees && cmd.held(buttons::ATTACK)) || cmd.held(buttons::MELEE))
        && (cancel_grenade(pm) || cancel_smoke(pm))
    {
        return;
    }
    let md = def(pm, melee);
    let ps = &*pm.ps;
    if md.def.melee_damage() == 0 || delay_done || ps.other_flags & 0x20 != 0 {
        return;
    }
    if ps.weapon_delay != 0 && !ws::is_reloading(ps.weaponstate) {
        return;
    }
    let pressed = |b: u32| cmd.held(b) && !old.held(b);
    if !(pressed(buttons::MELEE) || (attack_melees && pressed(buttons::ATTACK))) {
        return;
    }
    if ps.weapon_pos_frac > 0.0 && weapon_def.ads_overlay_reticle() != 0 {
        return;
    }
    if !md.has_melee_anim() || ps.e_flags2 & e_flags2::MOUNTED != 0 {
        return;
    }
    if ws::is_raising_or_dropping(ps.weaponstate) {
        return;
    }
    let _ = pml;
    melee_charge(pm);
    melee_start(pm);
}

/// Starts a pose's animation on both hands, its empty-clip version when the
/// clip is empty.
fn pose_anim<W: MoveWorld>(pm: &mut Pm<'_, W>, anim: u32, empty_anim: u32) {
    let empty = hand_clip_empty(pm.ps, pm.world);
    start_anim_both(pm.ps, if empty { empty_anim } else { anim });
}

fn set_pose(ps: &mut crate::PlayerState, state: i32, time: i32) {
    ps.weaponstate = state;
    ps.weapon_time = time;
    ps.weapon_delay = 0;
}

fn sprint_raise<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let t = def(pm, pm.ps.weapon).def.sprint_in_time();
    set_pose(pm.ps, ws::SPRINT_RAISE, t);
    pose_anim(pm, weap_anim::SPRINT_IN, weap_anim::SPRINT_EMPTY_IN);
}

fn sprint_loop<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    set_pose(pm.ps, ws::SPRINT_LOOP, 0);
    pose_anim(pm, weap_anim::SPRINT_LOOP, weap_anim::SPRINT_EMPTY_LOOP);
}

fn sprint_drop<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let t = def(pm, pm.ps.weapon).def.sprint_out_time();
    set_pose(pm.ps, ws::SPRINT_DROP, t);
    pose_anim(pm, weap_anim::SPRINT_OUT, weap_anim::SPRINT_EMPTY_OUT);
}

fn lowready_raise<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let t = def(pm, pm.ps.weapon).def.low_ready_in_time();
    set_pose(pm.ps, ws::LOWREADY_RAISE, t);
    start_anim_both(pm.ps, weap_anim::LOWREADY_IN);
}

fn lowready_loop(ps: &mut crate::PlayerState) {
    set_pose(ps, ws::LOWREADY_LOOP, 0);
    start_anim_both(ps, weap_anim::LOWREADY_LOOP);
}

fn lowready_drop<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let t = def(pm, pm.ps.weapon).def.low_ready_out_time();
    set_pose(pm.ps, ws::LOWREADY_DROP, t);
    start_anim_both(pm.ps, weap_anim::LOWREADY_OUT);
}

/// A dive-to-prone pose on both hands, unless that animation already plays.
fn dtp_pose<W: MoveWorld>(pm: &mut Pm<'_, W>, state: i32, time: i32, anim: u32, empty_anim: u32) {
    let empty = hand_clip_empty(pm.ps, pm.world);
    let ps = &mut *pm.ps;
    ps.weaponstate = state;
    ps.weaponstate_left = state;
    ps.weapon_time = time;
    ps.weapon_delay = 0;
    let a = if empty { empty_anim } else { anim };
    start_anim_unless_playing(ps, a, false);
    start_anim_unless_playing(ps, a, true);
}

fn dtp_in<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let t = def(pm, pm.ps.weapon).def.dtp_in_time();
    dtp_pose(
        pm,
        ws::DTP_IN,
        t,
        weap_anim::DTP_IN,
        weap_anim::DTP_EMPTY_IN,
    );
}

fn dtp_loop<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    dtp_pose(
        pm,
        ws::DTP_LOOP,
        0,
        weap_anim::DTP_LOOP,
        weap_anim::DTP_EMPTY_LOOP,
    );
}

fn dtp_out<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let t = def(pm, pm.ps.weapon).def.dtp_out_time();
    dtp_pose(
        pm,
        ws::DTP_OUT,
        t,
        weap_anim::DTP_OUT,
        weap_anim::DTP_EMPTY_OUT,
    );
}

fn slide_in<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let t = def(pm, pm.ps.weapon).def.slide_in_time();
    let ps = &mut *pm.ps;
    set_pose(ps, ws::SLIDE_IN, t);
    start_anim_unless_playing(ps, weap_anim::SLIDE_IN, false);
}

fn deploy<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let t = def(pm, pm.ps.weapon).def.deploy_time();
    set_pose(pm.ps, ws::DEPLOYING, t);
    start_anim(pm.ps, weap_anim::DEPLOY);
}

fn breakdown<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let t = def(pm, pm.ps.weapon).def.breakdown_time();
    set_pose(pm.ps, ws::BREAKING_DOWN, t);
    start_anim(pm.ps, weap_anim::BREAKDOWN);
}

fn cont_fire_in<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let t = def(pm, pm.ps.weapon).def.cont_fire_in_time();
    set_pose(pm.ps, ws::CONT_FIRE_IN, t);
    start_anim(pm.ps, weap_anim::CONT_FIRE_IN);
}

fn cont_fire_loop(ps: &mut crate::PlayerState) {
    set_pose(ps, ws::CONT_FIRE_LOOP, 0);
    start_anim(ps, weap_anim::CONT_FIRE_LOOP);
}

fn cont_fire_out<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let t = def(pm, pm.ps.weapon).def.cont_fire_out_time();
    set_pose(pm.ps, ws::CONT_FIRE_OUT, t);
    start_anim(pm.ps, weap_anim::CONT_FIRE_OUT);
}

/// States in which a firing or melee hand blocks a pose change.
fn hand_busy(s: i32) -> bool {
    matches!(
        s,
        ws::FIRING | ws::RECHAMBERING | ws::RECHAMBER_END | ws::CONT_FIRE_LOOP
    ) || ws::is_melee(s)
}

/// A pose may start: in shallow water with a weapon asked for, neither hand
/// firing or meleeing, and the weapon not switching, throwing or diving.
fn pose_allowed<W: MoveWorld>(pm: &Pm<'_, W>) -> bool {
    let ps = &*pm.ps;
    let s = ps.weaponstate;
    ps.water_level < 3
        && pm.cmd.weapon != 0
        && !hand_busy(s)
        && !hand_busy(ps.weaponstate_left)
        && !ws::is_raising_or_dropping(s)
        && !ws::is_offhand(s)
        && !ws::is_dtp(s)
}

fn check_sprint<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let s = pm.ps.weaponstate;
    if !pose_allowed(pm) || s == ws::DETONATING || pm.ps.pm_flags & pm_flags::DIVING != 0 {
        return;
    }
    let sprinting = pm.ps.pm_flags & pm_flags::SPRINTING != 0;
    if sprinting && !(ws::SPRINT_RAISE..=ws::SPRINT_DROP).contains(&s) {
        sprint_raise(pm);
    } else if !sprinting && matches!(s, ws::SPRINT_RAISE | ws::SPRINT_LOOP) {
        sprint_drop(pm);
    }
}

fn check_lowready<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let s = pm.ps.weaponstate;
    if !pose_allowed(pm)
        || ws::is_reloading(s)
        || s == ws::DETONATING
        || pm.ps.pm_flags & pm_flags::DIVING != 0
    {
        return;
    }
    let low = pm.ps.pm_flags & pm_flags::LOW_READY != 0;
    if low && !(ws::LOWREADY_RAISE..=ws::LOWREADY_DROP).contains(&s) {
        lowready_raise(pm);
    } else if !low && matches!(s, ws::LOWREADY_RAISE | ws::LOWREADY_LOOP) {
        lowready_drop(pm);
    }
}

/// Diving to prone and not yet sliding along the ground.
fn diving<W: MoveWorld>(pm: &Pm<'_, W>) -> bool {
    tuning::DTP && pm.ps.pm_flags & pm_flags::DIVING != 0 && pm.ps.jump_time < 0
}

fn check_dtp<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let ps = &*pm.ps;
    let s = ps.weaponstate;
    let ok = ps.water_level < 3
        && pm.cmd.weapon != 0
        && !hand_busy(s)
        && !ws::is_raising_or_dropping(s)
        && !ws::is_offhand(s)
        && !ws::is_dtp(s)
        && ps.pm_flags & pm_flags::DIVING != 0;
    if ok && !diving(pm) {
        dtp_in(pm);
    }
}

fn check_slide<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let ps = &*pm.ps;
    let s = ps.weaponstate;
    if ps.water_level < 3
        && pm.cmd.weapon != 0
        && !hand_busy(s)
        && !ws::is_raising_or_dropping(s)
        && !ws::is_offhand(s)
        && !ws::is_dtp(s)
        && s != ws::SLIDE_IN
        && ps.pm_flags & pm_flags::SLIDING != 0
        && ps.slick_start_time == pm.cmd.server_time
    {
        slide_in(pm);
    }
}

fn check_deploy<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let w = def(pm, pm.ps.weapon);
    let s = pm.ps.weaponstate;
    if !w.def.mountable_weapon()
        || ws::is_offhand(s)
        || ws::is_raising_or_dropping(s)
        || ws::is_dtp(s)
    {
        return;
    }
    let ps = &mut *pm.ps;
    if ps.weap_flags & weap_flags::DEPLOY != 0 && !ws::is_reloading(s) && inv::clip(ps, &w) > 0 {
        ps.weap_flags &= !weap_flags::DEPLOY;
        if ps.ground_entity_num == crate::state::ENTITYNUM_NONE || ps.velocity[2] < 0.0 {
            return;
        }
        deploy(pm);
    } else if ps.weap_flags & weap_flags::BREAKDOWN != 0 {
        ps.weap_flags &= !weap_flags::BREAKDOWN;
        breakdown(pm);
    }
}

/// While mantling the weapon goes down; after, it comes back.
fn check_mantle_anim<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let ps = &mut *pm.ps;
    if !mantling_anim(ps) {
        if ps.pm_flags & pm_flags::MANTLE == 0
            && ps.weap_anim & !weap_anim::TOGGLE == weap_anim::MANTLE
        {
            finish(ps);
        }
        return;
    }
    if ps.weaponstate == ws::SPRINT_DROP {
        ps.weapon_time = 0;
    }
    start_anim_unless_playing(ps, weap_anim::MANTLE, false);
}

fn check_cont_fire<W: MoveWorld>(pm: &mut Pm<'_, W>, delay_done: bool) {
    let d = def(pm, pm.ps.weapon).def;
    let ps = &*pm.ps;
    let s = ps.weaponstate;
    let ok = pm.cmd.weapon != 0
        && d.continuous_fire()
        && (ps.water_level < 3 || !tuning::DISABLE_WEAPONS_IN_WATER)
        && (s == ws::CONT_FIRE_LOOP || !hand_busy(s))
        && !ws::is_melee(s)
        && !ws::is_raising_or_dropping(s)
        && !ws::is_offhand(s)
        && !ws::is_dtp(s)
        && s != ws::DETONATING;
    if !ok {
        return;
    }
    let held = fire_held(pm, delay_done, true);
    if d.overheat_weapon() != 0 {
        pm.gap(crate::Gap::Overheat);
    }
    let s = pm.ps.weaponstate;
    if held {
        if !(ws::CONT_FIRE_IN..=ws::CONT_FIRE_OUT).contains(&s) {
            cont_fire_in(pm);
        }
    } else if matches!(s, ws::CONT_FIRE_IN | ws::CONT_FIRE_LOOP) {
        cont_fire_out(pm);
    }
}

/// The nightvision key toggles the goggles; outside zombies the weapon goes
/// down for it.
fn check_nightvision<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let d = def(pm, pm.ps.weapon).def;
    let s = pm.ps.weaponstate;
    if pm.oldcmd.buttons.held(buttons::NIGHTVISION)
        || !pm.cmd.buttons.held(buttons::NIGHTVISION)
        || ws::is_reloading(s)
        || ws::is_melee(s)
        || ws::is_raising_or_dropping(s)
        || ws::is_offhand(s)
        || matches!(
            s,
            ws::NIGHTVISION_WEAR | ws::NIGHTVISION_REMOVE | ws::RECHAMBERING | ws::RECHAMBER_END
        )
    {
        return;
    }
    pm.event(event::RESET_ADS, 0);
    let zombies = pm.zombiemode;
    let ps = &mut *pm.ps;
    if ps.weap_flags & weap_flags::NIGHTVISION == 0 {
        ps.weap_flags |= weap_flags::NIGHTVISION;
        if !zombies {
            ps.weaponstate = ws::NIGHTVISION_WEAR;
            start_anim(ps, weap_anim::NIGHTVISION_WEAR);
            ps.weapon_time = d.night_vision_wear_time();
            pm.event(event::NIGHTVISION_WEAR, 0);
        }
    } else {
        ps.weap_flags &= !weap_flags::NIGHTVISION;
        if !zombies {
            ps.weaponstate = ws::NIGHTVISION_REMOVE;
            start_anim(ps, weap_anim::NIGHTVISION_REMOVE);
            ps.weapon_time = d.night_vision_remove_time();
            pm.event(event::NIGHTVISION_REMOVE, 0);
        }
    }
}

/// A detonator's fire key sets off what it placed.
fn check_detonate<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let ps = &*pm.ps;
    if ps.weapon == 0 {
        return;
    }
    let d = def(pm, ps.weapon).def;
    let s = ps.weaponstate;
    if matches!(d.weapon_type(), weapon_type::GRENADE | weapon_type::MINE)
        && d.has_detonator()
        && s != ws::DETONATING
        && !ws::is_reloading(s)
        && !hand_busy(s)
        && !ws::is_raising_or_dropping(s)
        && !ws::is_offhand(s)
        && !ws::is_dtp(s)
        && pm.cmd.buttons.held(buttons::ATTACK)
    {
        let ps = &mut *pm.ps;
        ps.weaponstate = ws::DETONATING;
        ps.weapon_time = d.detonate_time();
        ps.weapon_delay = d.detonate_delay();
        start_anim(ps, weap_anim::DETONATE);
    }
}

/// Black Ops' offhand throw buttons beyond frag and smoke.
mod throw_buttons {
    pub const A: u32 = 30;
    pub const B: u32 = 31;
    pub const C: u32 = 32;
    pub const D: u32 = 33;
    /// Cancels a cancelable offhand hold.
    pub const CANCEL: u32 = 49;
}

/// Black Ops' offhand slots.
pub mod offhand_slot {
    pub const FRAG: i32 = 1;
    pub const SPECIAL: i32 = 2;
}

/// The throw starts: the weapon goes down first when one is held.
fn offhand_init<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    if pm.ps.water_level >= 3 {
        return;
    }
    let ps = &mut *pm.ps;
    ps.weap_flags &= !weap_flags::OFFHAND_ACTIVE;
    ps.weaponstate = ws::OFFHAND_INIT;
    ps.weapon_delay = 0;
    if ps.weapon != 0 {
        ps.weap_flags |= weap_flags::OFFHAND_SWITCH;
    }
    ps.throw_back_grenade_owner = crate::state::ENTITYNUM_NONE;
    pm.event(event::RESET_ADS, 0);
    pm.ps.pm_flags &= !pm_flags::ADS_INTENT;
}

/// The throw is over: the weapon comes back quickly.
fn offhand_end<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let weapon = pm.ps.weapon;
    if weapon == 0 {
        let ps = &mut *pm.ps;
        ps.weap_flags |= weap_flags::OFFHAND_EMPTY_HANDED;
        ps.weapon_time = 0;
        ps.weapon_delay = 1;
    } else {
        let d = def(pm, weapon).def;
        let empty = hand_clip_empty(pm.ps, pm.world);
        let ps = &mut *pm.ps;
        ps.weapon_time = d.quick_raise_time();
        ps.weapon_delay = 0;
        start_anim(
            ps,
            if empty {
                weap_anim::EMPTY_RAISE
            } else {
                weap_anim::QUICK_RAISE
            },
        );
        if d.dual_wield() {
            start_anim_left(ps, weap_anim::QUICK_RAISE);
        }
    }
    let ps = &mut *pm.ps;
    ps.weap_flags &= !(weap_flags::OFFHAND_SWITCH | weap_flags::OFFHAND_ACTIVE);
    ps.pm_flags &= !pm_flags::PRONEMOVE_OVERRIDDEN;
    ps.throw_back_grenade_time_left = 0;
    ps.throw_back_grenade_owner = crate::state::ENTITYNUM_NONE;
    ps.weaponstate = ws::OFFHAND_END;
}

/// The first held weapon for an offhand slot.
fn offhand_in_slot<W: MoveWorld>(pm: &Pm<'_, W>, slot: i32) -> u32 {
    pm.ps
        .held_weapons
        .iter()
        .map(|h| h.weapon)
        .find(|&w| w != 0 && def(pm, w).def.offhand_slot() == slot)
        .unwrap_or(0)
}

/// No offhand to throw in that slot.
fn offhand_empty<W: MoveWorld>(pm: &mut Pm<'_, W>, slot: i32) {
    pm.event(event::EMPTY_OFFHAND, 0);
    if offhand_in_slot(pm, slot) != 0 {
        if slot == offhand_slot::FRAG {
            pm.event(event::NO_FRAG_GRENADE_HINT, 0);
        } else if slot == offhand_slot::SPECIAL {
            pm.event(event::NO_SPECIAL_GRENADE_HINT, 0);
        }
    }
}

/// The offhand to throw from a slot: one with ammo, or a grenade thrown back.
fn find_offhand<W: MoveWorld>(pm: &Pm<'_, W>, slot: i32) -> u32 {
    let ps = &*pm.ps;
    for h in &ps.held_weapons {
        let w = h.weapon;
        if w == 0 {
            continue;
        }
        let d = def(pm, w);
        if d.def.offhand_slot() != slot {
            continue;
        }
        if ps.throw_back_grenade_time_left > 0 {
            if !pm.zombiemode {
                return w;
            }
            if matches!(d.def.offhand_class(), 1 | 4) {
                return w;
            }
        }
        if inv::total(ps, &d) > 0 {
            return w;
        }
    }
    0
}

/// The offhand comes up: pulled back (primed) unless it is a grenade thrown
/// back.
fn offhand_prepare<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    if pm.ps.water_level >= 3 || pm.ps.weapon != 0 {
        return;
    }
    let offhand = pm.ps.offhand_index;
    let d = def(pm, offhand).def;
    let ps = &mut *pm.ps;
    ps.weaponstate = ws::OFFHAND_PREPARE;
    ps.weapon_time = d.hold_fire_time();
    ps.weapon_delay = 0;
    ps.weap_flags = (ps.weap_flags & !weap_flags::OFFHAND_SWITCH) | weap_flags::OFFHAND_ACTIVE;
    if ps.throw_back_grenade_owner == crate::state::ENTITYNUM_NONE {
        pm.event(event::PREP_OFFHAND, offhand as i32);
        let ps = &mut *pm.ps;
        start_anim_both(ps, weap_anim::HOLD_FIRE);
        if ps.weapon == 0 {
            pm.anim_event(anim::PRIME_GRENADE, true, true);
        }
    } else {
        start_anim(ps, weap_anim::ALT_RAISE);
    }
    hold_prone(pm.ps);
}

/// The offhand is held: its fuse starts burning.
fn offhand_hold<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let fuse = def(pm, pm.ps.offhand_index).def.fuse_time();
    let ps = &mut *pm.ps;
    ps.weap_flags |= weap_flags::OFFHAND_ACTIVE;
    ps.weaponstate = ws::OFFHAND_HOLD;
    ps.weapon_time = 0;
    ps.weapon_delay = 0;
    if ps.throw_back_grenade_owner == crate::state::ENTITYNUM_NONE {
        ps.grenade_time_left = fuse;
    }
}

/// Throws when the key is let go, the way the key asks.
fn offhand_start<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let d = def(pm, pm.ps.offhand_index).def;
    let cmd = pm.cmd.buttons;
    let old = pm.oldcmd.buttons;
    let thrown_key = |b: crate::Buttons| b.held(buttons::FRAG) || b.held(buttons::SMOKE);
    if !d.hold_button_to_throw() && thrown_key(old) && thrown_key(cmd) {
        if d.offhand_hold_is_cancelable() && cmd.held(throw_buttons::CANCEL) {
            offhand_end(pm);
            return;
        }
        if d.offhand_class() != 2
            || (!cmd.held(throw_buttons::A)
                && !cmd.held(throw_buttons::B)
                && !cmd.held(throw_buttons::C)
                && !cmd.held(throw_buttons::D))
        {
            pm.ps.weapon_delay = 1;
            return;
        }
    }
    let ps = &mut *pm.ps;
    ps.offhand_throw = 0x37;
    if cmd.held(throw_buttons::A) {
        ps.offhand_throw = 0x31;
    }
    if cmd.held(throw_buttons::B) {
        ps.offhand_throw = 0x32;
    }
    if cmd.held(throw_buttons::C) {
        ps.offhand_throw = 0x34;
    }
    if cmd.held(throw_buttons::D) {
        ps.offhand_throw = 0x33;
    }
    ps.weaponstate = ws::OFFHAND_START;
    ps.weapon_time = d.fire_time();
    ps.weap_flags |= weap_flags::OFFHAND_ACTIVE;
    ps.weapon_delay = d.fire_delay();
    start_anim(ps, weap_anim::FIRE);
    pm.anim_event(anim::FIRE_WEAPON, false, true);
}

/// The offhand leaves the hand: thrown, or dropped under water.
fn offhand_throw<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let offhand = pm.ps.offhand_index;
    if pm.ps.water_level > 2 {
        pm.event(event::GRENADE_DROP, offhand as i32);
        let ps = &mut *pm.ps;
        ps.pm_flags &= !pm_flags::PRONEMOVE_OVERRIDDEN;
        ps.weaponstate = ws::SWIM_IN;
        ps.weap_flags = (ps.weap_flags & !weap_flags::OFFHAND_ACTIVE) | weap_flags::SWIM_PUTAWAY;
        return;
    }
    pm.event(event::USE_OFFHAND, offhand as i32);
    if pm.ps.throw_back_grenade_owner == crate::state::ENTITYNUM_NONE {
        let w = def(pm, offhand);
        if inv::total(pm.ps, &w) == 0 {
            pm.event(event::EMPTY_OFFHAND, 0);
            let ps = &mut *pm.ps;
            ps.weap_flags |= weap_flags::OFFHAND_ACTIVE;
            ps.weaponstate = ws::OFFHAND;
            return;
        }
        inv::use_clip(pm.ps, &w, 1);
    }
    let ps = &mut *pm.ps;
    ps.weap_flags |= weap_flags::OFFHAND_ACTIVE;
    ps.weaponstate = ws::OFFHAND;
}

/// A cooking grenade burns down; held too long it goes off in hand. True
/// when it did.
fn grenade_cook<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) -> bool {
    let ps = &*pm.ps;
    let active = ps.weap_flags & weap_flags::OFFHAND_ACTIVE != 0;
    let weapon = if active { ps.offhand_index } else { ps.weapon };
    if !active && weapon == 0 {
        return false;
    }
    let w = def(pm, weapon);
    let d = w.def;
    if !matches!(d.weapon_type(), weapon_type::GRENADE | weapon_type::MINE)
        || pm.ps.grenade_time_left <= 0
    {
        return false;
    }
    if d.cook_off_hold() {
        pm.ps.grenade_time_left -= pml.msec;
    }
    let ps = &*pm.ps;
    if ps.water_level < 3 || !active || ps.weaponstate != ws::OFFHAND_HOLD {
        if ps.grenade_time_left < 1 {
            pm.ps.grenade_time_left = -1;
            let offhand = pm.ps.offhand_index as i32;
            pm.event(event::GRENADE_SUICIDE, offhand);
            if pm.zombiemode {
                let ps = &mut *pm.ps;
                finish(ps);
                start_anim(ps, weap_anim::QUICK_IDLE);
                ps.throw_back_grenade_time_left = 0;
                ps.throw_back_grenade_owner = crate::state::ENTITYNUM_NONE;
            }
            if pm.ps.throw_back_grenade_owner == crate::state::ENTITYNUM_NONE {
                inv::use_clip(pm.ps, &w, 1);
            }
            return true;
        }
    } else {
        pm.ps.weaponstate = ws::OFFHAND_START;
        inv::use_clip(pm.ps, &w, 1);
    }
    false
}

/// A held throw is let go of or cancelled before it leaves the hand.
fn check_offhand_cancel<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let cmd = pm.cmd.buttons;
    let s = pm.ps.weaponstate;
    if s == ws::OFFHAND_PREPARE {
        let d = def(pm, pm.ps.offhand_index).def;
        if !d.hold_button_to_throw() {
            if !d.offhand_hold_is_cancelable() || !cmd.held(throw_buttons::CANCEL) {
                return;
            }
        } else if cmd.held(buttons::FRAG) || cmd.held(buttons::SMOKE) {
            return;
        }
        offhand_end(pm);
    } else if s == ws::OFFHAND_INIT
        && def(pm, pm.ps.offhand_index)
            .def
            .offhand_hold_is_cancelable()
    {
        if !cmd.held(throw_buttons::CANCEL) {
            return;
        }
        offhand_end(pm);
        return;
    }
    let ps = &*pm.ps;
    let s = ps.weaponstate;
    let weapon = if ws::is_offhand(s) {
        ps.offhand_index
    } else {
        ps.weapon
    };
    let d = def(pm, weapon).def;
    let firing = hand_busy(s);
    if matches!(d.weapon_type(), weapon_type::GRENADE | weapon_type::MINE)
        && (d.offhand_hold_is_cancelable()
            || (ps.weapon != 0 && firing && d.hold_button_to_throw()))
        && !cmd.held(buttons::ATTACK)
        && (!d.offhand_hold_is_cancelable() || cmd.held(throw_buttons::CANCEL))
    {
        let ps = &mut *pm.ps;
        finish(ps);
        start_anim(ps, weap_anim::QUICK_IDLE);
    }
}

/// The frag or smoke key throws that slot's offhand.
fn check_offhand<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let ps = &*pm.ps;
    let s = ps.weaponstate;
    let in_vehicle = ps.e_flags & e_flags::VEHICLE_VIEW != 0 && ps.vehicle_pos < 5;
    if ps.water_level >= 3
        || ps.e_flags & e_flags::TURRET != 0
        || ps.weap_flags & (weap_flags::WEAPONS_DISABLED | weap_flags::OFFHANDS_DISABLED) != 0
        || in_vehicle
        || ps.pm_flags & pm_flags::SPRINTING != 0
        || grenade_fire_held(pm)
        || matches!(s, ws::DROPPING | ws::DROPPING_QUICK | ws::DROPPING_ALT)
        || (ws::is_offhand(s) && s != ws::OFFHAND_END)
        || ws::is_dtp(s)
        || (ws::LOWREADY_RAISE..=ws::LOWREADY_DROP).contains(&s)
        || ps.e_flags2 & e_flags2::MOUNTED != 0
        || ps.weap_flags & weap_flags::FIRING_GUIDED != 0
    {
        return;
    }
    let asked = pm.cmd.offhand_index;
    if inv::has_weapon(pm.ps, asked) {
        pm.ps.offhand_index = asked;
    }
    let cmd = pm.cmd.buttons;
    let old = pm.oldcmd.buttons;
    let slot = if cmd.held(buttons::FRAG) && !old.held(buttons::FRAG) {
        offhand_slot::FRAG
    } else if cmd.held(buttons::SMOKE) && !old.held(buttons::SMOKE) {
        offhand_slot::SPECIAL
    } else {
        return;
    };
    let w = find_offhand(pm, slot);
    if w == 0 {
        offhand_empty(pm, slot);
        return;
    }
    pm.event(event::SWITCH_OFFHAND, w as i32);
    pm.ps.offhand_index = w;
    let d = def(pm, w).def;
    if cmd.held(buttons::FRAG) && d.offhand_class() != 1 {
        return;
    }
    let ps = &*pm.ps;
    if ps.cursor_hint_ent_index == crate::state::ENTITYNUM_NONE
        && (ps.weapon == 0 || ps.weaponstate == ws::OFFHAND_END)
    {
        offhand_prepare(pm);
        return;
    }
    offhand_init(pm);
}

/// The weapon may be picked: one the game loaded and the player holds.
fn selectable<W: MoveWorld>(pm: &Pm<'_, W>, weapon: u32) -> bool {
    pm.world.weapon_exists(weapon) && inv::has_weapon(pm.ps, weapon)
}

/// Mantling with the weapon kept: it is put away.
fn mantle_puts_away(ps: &crate::PlayerState) -> bool {
    tuning::MANTLE_ENABLE
        && ps.pm_flags & pm_flags::MANTLE != 0
        && ps.mantle_flags & mantle_flags::NO_WEAPON == 0
}

/// Switching to the alternate (attachment) mode of the held weapon.
fn alt_switch<W: MoveWorld>(pm: &Pm<'_, W>, from: u32, to: u32) -> bool {
    if from == 0 || to == 0 {
        return false;
    }
    let w = def(pm, from);
    to == w.alt_weapon
        || (w.def.inventory_type() == inventory_type::ALT_MODE
            && pm.ps.last_weapon_alt_mode_switch == to)
}

/// Black Ops' weapon inventory types.
pub mod inventory_type {
    pub const PRIMARY: i32 = 0;
    pub const OFFHAND: i32 = 1;
    pub const ITEM: i32 = 2;
    pub const ALT_MODE: i32 = 3;
}

/// The switch drops and raises quickly.
fn quick_switch<W: MoveWorld>(pm: &Pm<'_, W>, from: u32, to: u32, state: i32) -> bool {
    let a = def(pm, from).def;
    let b = def(pm, to).def;
    let mut quick = state == ws::DROPPING_QUICK;
    if from == 0 && b.weapon_type() == weapon_type::PROJECTILE {
        quick = true;
    }
    if a.can_use_in_vehicle() || b.can_use_in_vehicle() {
        quick = true;
    }
    if b.offhand_slot() != 0 || a.offhand_slot() != 0 {
        quick = true;
    }
    if to != 0 && b.weapon_class() == weapon_class::PISTOL {
        return true;
    }
    quick
}

/// Puts the held weapon away to switch to `to`.
fn begin_weapon_change<W: MoveWorld>(pm: &mut Pm<'_, W>, to: u32, quick: bool) {
    if to != 0 && !inv::has_weapon(pm.ps, to) {
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
    let from = ps.weapon;
    ps.weapon_delay = 0;
    let a = def(pm, from).def;
    let b = def(pm, to).def;
    if b.can_use_in_vehicle() && a.can_use_in_vehicle() {
        return;
    }
    if from == 0 || !inv::has_weapon(pm.ps, from) || pm.ps.grenade_time_left > 0 {
        let ps = &mut *pm.ps;
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
    let ps = &mut *pm.ps;
    ps.ads_zoom_select = 0;
    ps.ads_zoom_time = 0;
    let empty = hand_clip_empty(pm.ps, pm.world);
    let alt = alt_switch(pm, from, to);
    pm.ps.grenade_time_left = 0;
    if pm.ps.weap_flags & weap_flags::OFFHAND_SWITCH != 0
        && def(pm, pm.ps.offhand_index).def.rotate_type() == 1
    {
        let ps = &mut *pm.ps;
        ps.weaponstate = ws::DROPPING_QUICK;
        ps.weapon_time = 0;
        return;
    }
    if !alt {
        pm.event(event::PUTAWAY_WEAPON, 0);
        let ps = &mut *pm.ps;
        let keep_up = (a.inventory_type() == inventory_type::ITEM
            && matches!(a.offhand_slot(), 3 | 4)
            && ps.weaponstate == ws::FIRING)
            || a.no_drops_or_raises();
        let mut state = ws::DROPPING;
        if keep_up {
            if quick {
                state = ws::DROPPING_QUICK;
            }
        } else {
            let quick_drop = !empty || (quick && !a.no_quick_drop_when_empty());
            start_anim(
                ps,
                if !quick_drop {
                    weap_anim::EMPTY_DROP
                } else if quick {
                    weap_anim::QUICK_DROP
                } else {
                    weap_anim::DROP
                },
            );
            if a.dual_wield() {
                if empty && (!quick || a.no_quick_drop_when_empty()) {
                    start_anim_left(ps, weap_anim::EMPTY_DROP);
                    if quick {
                        state = ws::DROPPING_QUICK;
                    }
                } else if quick {
                    start_anim_left(ps, weap_anim::QUICK_DROP);
                    state = ws::DROPPING_QUICK;
                } else {
                    start_anim_left(ps, weap_anim::DROP);
                }
            } else if quick {
                state = ws::DROPPING_QUICK;
            }
        }
        ps.weaponstate = state;
    } else {
        pm.event(event::WEAPON_ALT, 0);
        let ps = &mut *pm.ps;
        start_anim(ps, weap_anim::ALT_DROP);
        ps.weaponstate = ws::DROPPING_ALT;
    }
    let ps = &mut *pm.ps;
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
    let ps = &*pm.ps;
    if !matches!(
        ps.pm_type,
        pm_type::LAST_STAND | pm_type::LAST_STAND_REVIVED | pm_type::LAST_STAND_GETTING_UP
    ) && ps.view_height_current == ps.view_height_target as f32
        && !alt
        && ps.pm_flags & (pm_flags::SPRINTING | pm_flags::MANTLE) == 0
        && b.inventory_type() != inventory_type::OFFHAND
        && def(pm, to).name != "syrette_mp"
    {
        pm.anim_event(anim::DROP_WEAPON, false, true);
    }
}

/// A clip-only weapon with no ammo left is thrown away.
fn drop_if_empty<W: MoveWorld>(pm: &mut Pm<'_, W>, weapon: u32) {
    if weapon == 0 || !inv::has_weapon(pm.ps, weapon) {
        return;
    }
    let w = def(pm, weapon);
    if w.def.clip_only()
        && inv::clip(pm.ps, &w) == 0
        && inv::stock(pm.ps, &w) == 0
        && !w.def.has_detonator()
        && !w.def.plantable()
    {
        take_weapon(pm, weapon);
    }
}

/// The player loses a weapon and its alternates; their ammo goes unless
/// another held weapon shares it.
pub(crate) fn take_weapon<W: MoveWorld>(pm: &mut Pm<'_, W>, weapon: u32) -> bool {
    if !inv::has_weapon(pm.ps, weapon) {
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
    let mut alt = def(pm, weapon).alt_weapon;
    while alt != 0 {
        if !inv::has_weapon(pm.ps, alt) {
            break;
        }
        remove_weapon(pm, alt);
        if pm.ps.weapon == weapon {
            pm.ps.weapon = 0;
        }
        alt = def(pm, alt).alt_weapon;
    }
    let ps = &mut *pm.ps;
    if matches!(
        ps.weaponstate,
        ws::DEPLOYING | ws::DEPLOYED | ws::BREAKING_DOWN
    ) {
        ps.e_flags &= !e_flags::TURRET;
        finish(ps);
    }
    true
}

fn remove_weapon<W: MoveWorld>(pm: &mut Pm<'_, W>, weapon: u32) {
    let w = def(pm, weapon);
    let mut shared = false;
    for i in 0..pm.ps.held_weapons.len() {
        let held = pm.ps.held_weapons[i].weapon;
        if held == weapon {
            pm.ps.held_weapons[i].weapon = 0;
        } else if def(pm, held).ammo_index == w.ammo_index {
            shared = true;
        }
    }
    let ps = &mut *pm.ps;
    if let Some(slot) = ps.ammo_in_clip.iter_mut().find(|s| s.index == w.clip_index) {
        *slot = crate::state::AmmoSlot::default();
    }
    if !shared
        && let Some(slot) = ps
            .ammo_not_in_clip
            .iter_mut()
            .find(|s| s.index == w.ammo_index)
    {
        *slot = crate::state::AmmoSlot::default();
    }
}

/// The weapon is away: the one asked for comes up (or none), first raise,
/// quick or empty as fits.
fn finish_weapon_change<W: MoveWorld>(pm: &mut Pm<'_, W>, state: i32) {
    let ps = &*pm.ps;
    let mut to = 0;
    if !mantle_puts_away(ps)
        && !(tuning::DOOR_BREACH_WEAPON_DROP && ps.pm_flags & pm_flags::DOOR_BREACH != 0)
        && ps.weap_flags & weap_flags::OFFHAND_SWITCH == 0
        && ps.pm_flags & pm_flags::LADDER == 0
    {
        let asked = pm.cmd.weapon;
        let in_vehicle = ps.e_flags & e_flags::VEHICLE_VIEW != 0 && ps.vehicle_pos < 5;
        if inv::has_weapon(ps, asked)
            && ps.weap_flags & weap_flags::WEAPONS_DISABLED == 0
            && !(in_vehicle && !def(pm, asked).def.can_use_in_vehicle())
            && pm.world.weapon_exists(asked)
        {
            to = asked;
        }
    }
    if !inv::has_weapon(pm.ps, to) {
        to = 0;
    }
    let from = pm.ps.weapon;
    pm.ps.weapon = to;
    let mut alt = if to == 0 { 0 } else { pm.cmd.alt_mode_weapon };
    if !pm.world.weapon_exists(alt) || !inv::has_weapon(pm.ps, alt) {
        alt = 0;
    }
    pm.ps.last_weapon_alt_mode_switch = alt;
    let mut d = def(pm, pm.ps.weapon);
    if d.def.inventory_type() == inventory_type::ALT_MODE && pm.ps.last_weapon_alt_mode_switch == 0
    {
        pm.ps.weapon = 0;
        d = def(pm, 0);
    }
    let held = pm.ps.weapon;
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
        if d.def.dual_wield() {
            start_anim_left(ps, weap_anim::IDLE);
        }
        return;
    }
    let first = !inv::used_before(ps, held)
        && ps.pm_type != pm_type::LAST_STAND_GETTING_UP
        && ps.pm_type != pm_type::LAST_STAND;
    inv::set_used_before(ps, held, true);
    let alt_raise = state == ws::DROPPING_ALT;
    let (anim_n, time, spread) = if alt_raise {
        (
            weap_anim::ALT_RAISE,
            d.def.alt_raise_time(),
            ps.aim_spread_scale.max(128.0),
        )
    } else {
        let quick = quick_switch(pm, from, to, state);
        let empty = hand_clip_empty(pm.ps, pm.world);
        let (a, t) = if d.def.no_drops_or_raises() {
            (0, 0)
        } else if empty {
            (weap_anim::EMPTY_RAISE, d.def.empty_raise_time())
        } else if first {
            (weap_anim::FIRST_RAISE, d.def.first_raise_time())
        } else if quick {
            (weap_anim::QUICK_RAISE, d.def.quick_raise_time())
        } else {
            (weap_anim::RAISE, d.def.raise_time())
        };
        (a, t, 255.0)
    };
    let ps = &mut *pm.ps;
    if d.def.no_drops_or_raises() {
        ps.weaponstate = anim_n as i32;
        ps.weapon_time = time;
        ps.aim_spread_scale = spread;
        hold_prone(ps);
        start_anim(ps, weap_anim::IDLE);
        if d.def.dual_wield() {
            start_anim_left(ps, weap_anim::IDLE);
        }
    } else {
        ps.weapon_time = time;
        ps.aim_spread_scale = spread;
        ps.weaponstate = if alt_raise {
            ws::RAISING_ALT
        } else {
            ws::RAISING
        };
        hold_prone(ps);
        start_anim_both(ps, anim_n);
    }
    drop_if_empty(pm, from);
    if from != 0 && !alt_raise {
        pm.event(
            if first {
                event::FIRST_RAISE_WEAPON
            } else {
                event::RAISE_WEAPON
            },
            0,
        );
        let ps = &*pm.ps;
        if !matches!(
            ps.pm_type,
            pm_type::LAST_STAND | pm_type::LAST_STAND_REVIVED | pm_type::LAST_STAND_GETTING_UP
        ) && ps.pm_flags & (pm_flags::SPRINTING | pm_flags::LADDER_FALL | pm_flags::LADDER) == 0
            && ps.view_height_current == ps.view_height_target as f32
        {
            pm.anim_event(anim::RAISE_WEAPON, false, true);
        }
    }
}

/// Switches to the weapon the command asks for when the weapon is free to.
fn check_change_weapon<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let ps = &*pm.ps;
    if ps.water_level > 2 && tuning::DISABLE_WEAPONS_IN_WATER {
        return;
    }
    let s = ps.weaponstate;
    if ws::is_melee(s)
        || ws::is_dtp(s)
        || matches!(s, ws::NIGHTVISION_WEAR | ws::NIGHTVISION_REMOVE)
        || ps.e_flags2 & e_flags2::MOUNTED != 0
        || ps.weap_flags & weap_flags::FIRING_GUIDED != 0
        || mantling_anim(ps)
    {
        return;
    }
    if ps.weapon_time != 0
        && !ws::is_reloading(s)
        && !matches!(s, ws::RECHAMBERING | ws::RECHAMBER_END)
        && (hand_busy(s) || ps.weapon_delay != 0)
    {
        return;
    }
    if mantle_puts_away(ps)
        || (tuning::DOOR_BREACH_WEAPON_DROP && ps.pm_flags & pm_flags::DOOR_BREACH != 0)
        || ps.pm_flags & pm_flags::LADDER != 0
        || ps.weap_flags & weap_flags::OFFHAND_SWITCH != 0
    {
        if ps.weapon != 0 {
            begin_weapon_change(pm, 0, true);
        }
        return;
    }
    let put_away = |pm: &mut Pm<'_, W>| {
        if pm.ps.weapon != 0 {
            begin_weapon_change(pm, 0, false);
        }
    };
    if ps.weap_flags & weap_flags::WEAPONS_DISABLED != 0 {
        put_away(pm);
        return;
    }
    let asked = pm.cmd.weapon;
    let frozen_with_weapon =
        ps.pm_flags & (pm_flags::RESPAWNED | pm_flags::FROZEN) != 0 && ps.weapon != 0;
    let in_vehicle = ps.e_flags & e_flags::VEHICLE_VIEW != 0 && ps.vehicle_pos < 5;
    if in_vehicle {
        if ps.weapon == 0 && asked == 0 {
            return;
        }
        if !def(pm, asked).def.can_use_in_vehicle() {
            put_away(pm);
            return;
        }
        if ps.weapon == asked
            || ps.weaponstate == ws::DEPLOYING
            || frozen_with_weapon
            || ps.e_flags2 & e_flags2::CONTROLS_LOCKED != 0
            || (asked != 0 && !selectable(pm, asked))
        {
            return;
        }
        let quick = ps.mantle_flags & 0x100 != 0;
        begin_weapon_change(pm, asked, quick);
        return;
    }
    if ps.weapon == asked
        || ps.weaponstate == ws::DEPLOYING
        || frozen_with_weapon
        || (asked != 0 && !selectable(pm, asked))
        || ws::is_offhand(ps.weaponstate)
    {
        let ps = &*pm.ps;
        if ps.weapon == asked
            && matches!(
                ps.weaponstate,
                ws::DROPPING | ws::DROPPING_QUICK | ws::DROPPING_ALT
            )
        {
            let ps = &mut *pm.ps;
            finish(ps);
            start_anim(ps, weap_anim::QUICK_IDLE);
            return;
        }
        if ps.weapon != 0 && !inv::has_weapon(ps, ps.weapon) {
            begin_weapon_change(pm, 0, false);
        }
        return;
    }
    match ps.weaponstate {
        ws::BREAKING_DOWN => return,
        ws::DEPLOYED => {
            if ps.weap_flags & weap_flags::BREAKDOWN == 0 {
                pm.ps.e_flags &= !e_flags::TURRET;
            }
            return;
        }
        _ => {}
    }
    if ps.e_flags & e_flags::TURRET != 0 {
        pm.ps.weap_flags |= weap_flags::BREAKDOWN;
        pm.event(event::WEAPON_BREAKING_DOWN, 0);
        return;
    }
    let quick =
        if ps.mantle_flags & 0x100 != 0 || ps.weap_flags & weap_flags::OFFHAND_EMPTY_HANDED != 0 {
            true
        } else {
            quick_switch(pm, ps.weapon, asked, ps.weaponstate)
        };
    begin_weapon_change(pm, asked, quick);
}

/// A viewmodel animation scripts force on a weapon: it plays, then the held
/// weapon returns. True while it holds the weapon.
fn forced_anim<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) -> bool {
    let ps = &*pm.ps;
    if ps.weap_flags & weap_flags::FORCED_ANIM == 0 {
        return false;
    }
    if ps.weaponstate == ps.forced_anim_state && ps.weapon == ps.forced_anim_weapon {
        let ps = &mut *pm.ps;
        ps.weapon_time -= pml.msec;
        if ps.weapon_time < 1 {
            ps.weap_flags &= !weap_flags::FORCED_ANIM;
            ps.weapon = ps.forced_anim_weapon;
            ps.weapon_time = 0;
            start_anim(ps, weap_anim::IDLE);
        }
        return true;
    }
    let w = def(pm, ps.forced_anim_weapon);
    let d = w.def;
    let ps = &mut *pm.ps;
    let ev = match ps.forced_anim_state {
        ws::FIRING => {
            start_anim(ps, weap_anim::FIRE);
            ps.weapon_time = d.fire_time();
            event::FIRE_WEAPON
        }
        ws::RELOADING => {
            start_anim(ps, weap_anim::RELOAD);
            ps.weapon_time = d.reload_time();
            event::RELOAD
        }
        ws::NIGHTVISION_WEAR => {
            start_anim(ps, weap_anim::NIGHTVISION_WEAR);
            ps.weapon_time = d.night_vision_wear_time();
            event::NIGHTVISION_WEAR
        }
        ws::NIGHTVISION_REMOVE => {
            start_anim(ps, weap_anim::NIGHTVISION_REMOVE);
            ps.weapon_time = d.night_vision_remove_time();
            event::NIGHTVISION_REMOVE
        }
        _ => {
            ps.weap_flags &= !weap_flags::FORCED_ANIM;
            return false;
        }
    };
    pm.event(ev, 0);
    let ps = &mut *pm.ps;
    ps.forced_anim_prev_weapon = ps.weapon;
    ps.weapon = ps.forced_anim_weapon;
    ps.weaponstate = ps.forced_anim_state;
    true
}

/// The clip can take more: rounds beside it, room in it (or a fuel tank to
/// refill), and a no-partial-reload weapon only when a full add fits.
fn can_reload<W: MoveWorld>(pm: &Pm<'_, W>) -> bool {
    let weapon = hand_weapon(pm.ps, pm.world);
    let w = def(pm, weapon);
    let d = w.def;
    let ps = &*pm.ps;
    let fuel = d.fuel_tank_weapon() && inv::fuel(ps, weapon) > 0;
    let Some(slot) = ps.ammo_not_in_clip.iter().find(|s| s.index == w.ammo_index) else {
        return false;
    };
    if slot.count == 0 {
        return false;
    }
    let clip = inv::clip(ps, &w);
    let size = inv::clip_size(&w);
    if clip >= size && !fuel {
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
fn reload_start_state<W: MoveWorld>(pm: &mut Pm<'_, W>, delay_done: bool) {
    let w = def(pm, pm.ps.weapon);
    let d = w.def;
    if delay_done {
        reload_add_ammo(pm);
    }
    if pm.ps.weapon_time != 0 {
        return;
    }
    if d.segmented_reload() && pm.cmd.buttons.held(buttons::ATTACK) {
        pm.ps.weaponstate = ws::RELOAD_START_INTERRUPT;
    }
    if (pm.ps.weaponstate != ws::RELOAD_START_INTERRUPT || inv::clip(pm.ps, &w) == 0)
        && can_reload(pm)
    {
        start_reload_anim(pm);
        return;
    }
    let held = pm.ps.weapon;
    inv::set_needs_rechamber(pm.ps, held, false);
    reload_end(pm, &d);
}

/// The reload finishes: its end animation when the weapon has one.
fn reload_end<W: MoveWorld>(pm: &mut Pm<'_, W>, d: &fastfile_t5::weapon_def::WeaponDefView<'_>) {
    let ps = &mut *pm.ps;
    if d.reload_end_time() == 0 {
        ps.weaponstate = ws::READY;
        start_anim(ps, weap_anim::IDLE);
        if d.dual_wield() {
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
/// segment or ends; any other ends.
fn reloading<W: MoveWorld>(pm: &mut Pm<'_, W>, delay_done: bool) {
    let hand = Hand::of(pm.ps);
    let weapon = hand_weapon(pm.ps, pm.world);
    let d = def(pm, weapon).def;
    if delay_done {
        reload_add_ammo(pm);
    }
    if hand.time(pm.ps) != 0 {
        return;
    }
    if d.segmented_reload() && pm.cmd.buttons.held(buttons::ATTACK) {
        hand.set_state(pm.ps, ws::RELOADING_INTERRUPT);
    }
    let held = pm.ps.weapon;
    inv::set_needs_rechamber(pm.ps, held, false);
    if d.segmented_reload() {
        if hand.state(pm.ps) != ws::RELOADING_INTERRUPT && can_reload(pm) {
            start_reload_anim(pm);
            return;
        }
        if d.reload_end_time() == 0 {
            let ps = &mut *pm.ps;
            hand.set_state(ps, ws::READY);
            start_anim_hand(ps, weap_anim::IDLE);
            return;
        }
        let ps = &mut *pm.ps;
        hand.set_state(ps, ws::RELOAD_END);
        start_anim(ps, weap_anim::RELOAD_END);
        hand.set_time(ps, d.reload_end_time());
        pm.event(event::RELOAD_END, 0);
        return;
    }
    let ps = &mut *pm.ps;
    hand.set_state(ps, ws::READY);
    start_anim_hand(ps, weap_anim::IDLE);
}

/// A fuel tank weapon burns while firing; an empty tank ends the burst.
fn burn_fuel<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let hand = Hand::of(pm.ps);
    let held = pm.ps.weapon;
    let w = def(pm, held);
    let d = w.def;
    let s = hand.state(pm.ps);
    if !d.fuel_tank_weapon() || !hand_busy(s) || ws::is_melee(s) {
        return;
    }
    let ps = &mut *pm.ps;
    inv::add_fuel(ps, held, pml.msec);
    let life = d.tank_life_time();
    if life < inv::fuel(ps, held) {
        inv::set_fuel(ps, held, life);
        inv::set_clip(ps, &w, 0);
        hand.set_state(ps, ws::READY);
    }
}

/// The weapon may reload on its own: not raising, dropping, reloading,
/// firing, sprinting or mounted, and not unlimited.
fn may_auto_reload<W: MoveWorld>(pm: &Pm<'_, W>, state: i32) -> bool {
    let weapon = hand_weapon(pm.ps, pm.world);
    let d = def(pm, weapon).def;
    let ps = &*pm.ps;
    match state {
        ws::RAISING..=ws::DROPPING_ALT | ws::RELOADING..=ws::RELOAD_END | ws::SWIM_IN => false,
        _ => {
            state != ws::FIRING
                && state != ws::CONT_FIRE_LOOP
                && !(ws::SPRINT_RAISE..=ws::SPRINT_DROP).contains(&state)
                && !matches!(state, ws::DEPLOYING | ws::DEPLOYED | ws::BREAKING_DOWN)
                && ps.e_flags & e_flags::TURRET == 0
                && !d.unlimited_ammo()
                && (!d.no_ads_auto_reload() || ps.weapon_pos_frac != 1.0)
        }
    }
}

/// Reloads when asked (the key, a script, the left gun) or when the clip runs
/// dry with ammo left; a segmented reload can be interrupted by fire.
fn check_reload<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let hand = Hand::of(pm.ps);
    let weapon = hand_weapon(pm.ps, pm.world);
    let w = def(pm, weapon);
    let d = w.def;
    if !d.dual_wield() {
        pm.ps.weap_flags &= !weap_flags::RELOAD_LEFT;
    }
    let held_def = def(pm, weapon).def;
    if hand.state(pm.ps) == ws::MELEE_END
        && hand_clip_empty(pm.ps, pm.world)
        && !held_def.bayonet()
        && !held_def.use_as_melee()
    {
        let ps = &mut *pm.ps;
        hand.set_state(ps, ws::RAISING);
        hand.set_time(ps, d.empty_raise_time());
        start_anim(ps, weap_anim::EMPTY_RAISE);
        return;
    }
    let ps = &*pm.ps;
    let s = hand.state(ps);
    if ps.weap_flags & weap_flags::NO_FIRE != 0
        || ws::is_offhand(s)
        || ws::is_melee(s)
        || ws::is_dtp(s)
        || ps.pm_flags & pm_flags::MANTLE != 0
        || (ps.water_level >= 3 && tuning::DISABLE_WEAPONS_IN_WATER)
    {
        return;
    }
    let mut asked = false;
    if ps.pm_flags & pm_flags::FROZEN == 0 && ps.e_flags2 & e_flags2::CONTROLS_LOCKED == 0 {
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
    if d.segmented_reload()
        && matches!(s, ws::RELOAD_START | ws::RELOADING)
        && pm.cmd.buttons.held(buttons::ATTACK)
        && !pm.oldcmd.buttons.held(buttons::ATTACK)
    {
        if s == ws::RELOAD_START && d.reload_start_time() != 0 {
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
    if d.fuel_tank_weapon()
        && inv::stock(pm.ps, &w) != 0
        && d.tank_life_time() <= inv::fuel(pm.ps, pm.ps.weapon)
        && may_auto_reload(pm, s)
    {
        reload = true;
    }
    if reload || (inv::clip(pm.ps, &w) == 0 && inv::stock(pm.ps, &w) != 0 && may_auto_reload(pm, s))
    {
        begin_reload(pm);
    }
}

/// The states in which the left gun follows the right hand's state.
fn left_follows(s: i32) -> bool {
    matches!(
        s,
        ws::RAISING..=ws::DROPPING_ALT
            | ws::RELOADING_INTERRUPT..=ws::RELOAD_END
            | ws::MELEE_INIT..=ws::MELEE_END
            | ws::OFFHAND_PREPARE..=ws::OFFHAND
            | ws::DETONATING..=ws::CONT_FIRE_OUT
            | ws::DTP_IN..=ws::DTP_OUT
    )
}

/// Swimming: the weapon goes away, and comes back on leaving deep water.
fn swim_putaway<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    pm.event(event::PUTAWAY_WEAPON, 0);
    let ps = &mut *pm.ps;
    if ps.weap_flags & weap_flags::SWIM_PUTAWAY == 0 {
        start_anim_unless_playing(ps, weap_anim::DROP, false);
    } else {
        start_anim(ps, weap_anim::DROP);
    }
}

fn swim_raise<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    pm.ps.weaponstate = ws::RAISING;
    pm.event(event::RAISE_WEAPON, 0);
    start_anim(pm.ps, weap_anim::RAISE);
}

/// Black Ops' weapon rules for the running hand, once per movement slice.
pub(crate) fn pm_weapon<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let d = def(pm, pm.ps.weapon).def;
    if pm.ps.left_hand && !d.dual_wield() {
        return;
    }
    if pm.ps.pm_type >= pm_type::DEAD {
        if pm.ps.e_flags2 & e_flags2::MOUNTED == 0 {
            pm.ps.weapon = 0;
        }
        return;
    }
    if forced_anim(pm, pml) {
        return;
    }
    let ps = &*pm.ps;
    if ps.pm_flags & pm_flags::RESPAWNED != 0 || ps.e_flags & e_flags::TURRET != 0 {
        return;
    }
    if ps.water_level < 3 {
        let ps = &mut *pm.ps;
        if ps.weaponstate == ws::SWIM_IN {
            ps.weaponstate = ws::SWIM_OUT;
            ps.weapon_time = 1000;
        }
    } else if tuning::DISABLE_WEAPONS_IN_WATER {
        let delay_done = weapon_timers(pm, pml);
        if !mid_burst(pm) {
            check_melee(pm, pml, delay_done);
        }
        let s = pm.ps.weaponstate;
        if s != ws::SWIM_IN {
            if !(ws::OFFHAND_INIT..=ws::OFFHAND).contains(&s)
                && !matches!(s, ws::MELEE_INIT | ws::MELEE_FIRE)
            {
                let ps = &mut *pm.ps;
                ps.weap_flags |= weap_flags::SWIM_PUTAWAY;
                ps.weaponstate = ws::SWIM_IN;
                ps.weapon_delay = 0;
                ps.weapon_time = if s == ws::SWIM_OUT { 1000 } else { 0 };
                pm.reset_ads();
                let ps = &mut *pm.ps;
                ps.weapon_pos_frac = 0.0;
                ps.ads_delay_time = 0;
            }
        } else if pm.ps.weap_flags & weap_flags::SWIM_PUTAWAY != 0 {
            pm.ps.weap_flags &= !weap_flags::SWIM_PUTAWAY;
        } else {
            return;
        }
    }
    ads_lerp(pm, pml);
    spin(pm, pml);
    scope_zoom(pm);
    hold_breath(pm, pml);
    if pm.ps.left_hand {
        let ps = &mut *pm.ps;
        if left_follows(ps.weaponstate) {
            ps.weaponstate_left = ps.weaponstate;
            ps.weapon_delay_left = 0;
            ps.weapon_time_left = 0;
            ps.weap_anim_left = 0x42;
            return;
        }
        if ps.weap_anim_left == 0x42 {
            ps.weaponstate_left = ws::READY;
            ps.weapon_delay_left = 0;
            ps.weapon_time_left = 0;
            if ps.pm_type >= pm_type::DEAD {
                return;
            }
            ps.weap_anim_left = weap_anim::TOGGLE;
            return;
        }
    }
    if grenade_cook(pm, pml) {
        return;
    }
    latch_fire(pm);
    let delay_done = weapon_timers(pm, pml);
    if !mid_burst(pm) {
        check_nightvision(pm);
        check_sprint(pm);
        check_lowready(pm);
        check_dtp(pm);
        check_slide(pm);
        check_deploy(pm);
        check_offhand(pm);
        check_change_weapon(pm);
        check_reload(pm);
        burn_fuel(pm, pml);
        check_melee(pm, pml, delay_done);
        check_detonate(pm);
        check_offhand_cancel(pm);
        check_mantle_anim(pm);
        check_cont_fire(pm, delay_done);
    }
    if rechamber(pm, delay_done) {
        return;
    }
    let ps = &mut *pm.ps;
    let moving_prone = ps.pm_flags & pm_flags::PRONE != 0
        && (pm.cmd.forwardmove != 0 || pm.cmd.rightmove != 0)
        && ps.weapon_pos_frac != 1.0;
    if moving_prone || ws::is_melee(ps.weaponstate) {
        ps.aim_spread_scale = 255.0;
    }
    stack_fire(pm);
    let ps = &*pm.ps;
    if !delay_done {
        let (time, delay) = if ps.left_hand {
            (ps.weapon_time_left, ps.weapon_delay_left)
        } else {
            (ps.weapon_time, ps.weapon_delay)
        };
        if time != 0 || delay != 0 {
            return;
        }
    }
    if !ps.left_hand {
        match ps.weaponstate {
            ws::RELOAD_START | ws::RELOAD_START_INTERRUPT => {
                reload_start_state(pm, delay_done);
                return;
            }
            ws::RELOADING | ws::RELOADING_INTERRUPT => {
                reloading(pm, delay_done);
                return;
            }
            ws::RELOAD_END => {
                let ps = &mut *pm.ps;
                ps.weaponstate = ws::READY;
                start_anim(ps, weap_anim::IDLE);
                return;
            }
            ws::MELEE_INIT => {
                melee_fire(pm);
                return;
            }
            ws::MELEE_FIRE => {
                melee_end(pm);
                return;
            }
            ws::MELEE_END
            | ws::OFFHAND_END
            | ws::SPRINT_DROP
            | ws::CONT_FIRE_OUT
            | ws::DTP_OUT
            | ws::SLIDE_IN
            | ws::FIRING_SPECIAL
            | ws::LOWREADY_DROP => {
                finish(pm.ps);
                return;
            }
            ws::DEPLOYING => {
                pm.event(event::WEAPON_FINISH_DEPLOYING, 0);
                return;
            }
            ws::BREAKING_DOWN => {
                finish(pm.ps);
                pm.event(event::WEAPON_FINISH_BREAKING_DOWN, 0);
                return;
            }
            s @ (ws::DROPPING | ws::DROPPING_QUICK | ws::DROPPING_ALT) => {
                finish_weapon_change(pm, s);
                return;
            }
            ws::RAISING | ws::RAISING_ALT => {
                let ps = &mut *pm.ps;
                ps.weaponstate = ws::READY;
                ps.weaponstate_left = ws::READY;
                start_anim(ps, weap_anim::IDLE);
                start_anim_left(ps, weap_anim::IDLE);
                return;
            }
            ws::OFFHAND_INIT => {
                offhand_prepare(pm);
                return;
            }
            ws::OFFHAND_PREPARE => {
                offhand_hold(pm);
                return;
            }
            ws::OFFHAND_HOLD => {
                offhand_start(pm);
                return;
            }
            ws::OFFHAND_START => {
                offhand_throw(pm);
                return;
            }
            ws::OFFHAND => {
                offhand_end(pm);
                return;
            }
            ws::DETONATING => {
                if delay_done && pm.ps.weapon != 0 {
                    pm.event(event::DETONATE, 0);
                } else {
                    finish(pm.ps);
                }
                return;
            }
            ws::SPRINT_RAISE => {
                sprint_loop(pm);
                return;
            }
            ws::LOWREADY_RAISE => {
                lowready_loop(pm.ps);
                return;
            }
            ws::CONT_FIRE_IN => cont_fire_loop(pm.ps),
            ws::DTP_IN => {
                dtp_loop(pm);
                return;
            }
            ws::DTP_LOOP => {
                if diving(pm) || pm.ps.pm_flags & pm_flags::DIVING == 0 {
                    dtp_out(pm);
                }
                return;
            }
            ws::NIGHTVISION_WEAR | ws::NIGHTVISION_REMOVE => {
                let ps = &mut *pm.ps;
                if ps.weapon_time == 0 {
                    ps.weaponstate = ws::READY;
                    start_anim(ps, weap_anim::IDLE);
                }
                return;
            }
            ws::SWIM_IN => {
                swim_putaway(pm);
                return;
            }
            ws::SWIM_OUT => {
                swim_raise(pm);
                return;
            }
            ws::SPRINT_LOOP | ws::DEPLOYED | ws::LOWREADY_LOOP => return,
            _ => {}
        }
    }
    let ps = &*pm.ps;
    if ps.left_hand && matches!(ps.weaponstate_left, ws::RELOADING | ws::RELOADING_INTERRUPT) {
        reloading(pm, delay_done);
        return;
    }
    if pm.ps.weapon == 0 || !fire_held(pm, delay_done, false) {
        return;
    }
    if cancel_grenade(pm) || cancel_smoke(pm) {
        return;
    }
    if delay_done && grenade_fire_held(pm) {
        pm.ps.weapon_delay = 1;
        return;
    }
    let ps = &*pm.ps;
    if ps.pm_flags & pm_flags::FROZEN != 0
        || ps.weaponstate == ws::DEPLOYING
        || ps.e_flags2 & e_flags2::CONTROLS_LOCKED != 0
    {
        return;
    }
    fire(pm, delay_done);
}
