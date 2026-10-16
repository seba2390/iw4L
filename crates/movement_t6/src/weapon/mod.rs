//! Black Ops 2's weapon rules, run by its player movement: raising,
//! dropping and switching weapons, firing, rechambering, reloading, melee,
//! offhands, riot shields and weapon poses, and the viewmodel animations
//! they play.
//!
//! The running hand is the right one, or the left gun of a dual-wield weapon
//! when `left_hand` is set; movement runs the rules once for each.

mod ads;
pub(crate) use ads::ads_lerp;
mod fire;
mod melee;
mod offhand;
mod pose;
mod reload;
mod switch;

use crate::events::event;
use crate::inventory as inv;
use crate::state::{
    PlayerState, buttons, e_flags, e_flags2, pm_flags, pm_type, weap_flags, weapon_state as ws,
};
use crate::{Gap, MoveWorld, Pm, Pml, perks, tuning};
/// Black Ops 2's viewmodel animations as the player state names them (not
/// a weapon's animation file slots).
pub mod weap_anim {
    pub const IDLE: u32 = 0;
    pub const QUICK_IDLE: u32 = 1;
    pub const FIRE: u32 = 2;
    pub const LAST_SHOT: u32 = 4;
    pub const RECHAMBER: u32 = 6;
    pub const ADS_FIRE: u32 = 7;
    pub const ADS_LAST_SHOT: u32 = 8;
    pub const ADS_RECHAMBER: u32 = 9;
    pub const MELEE: u32 = 0xa;
    pub const MELEE_EMPTY: u32 = 0xb;
    pub const MELEE_CHARGE: u32 = 0xc;
    pub const MELEE_CHARGE_EMPTY: u32 = 0xd;
    pub const DROP: u32 = 0xe;
    pub const RAISE: u32 = 0xf;
    pub const FIRST_RAISE: u32 = 0x10;
    pub const RELOAD: u32 = 0x11;
    pub const RELOAD_EMPTY: u32 = 0x12;
    pub const RELOAD_START: u32 = 0x13;
    pub const RELOAD_END: u32 = 0x14;
    pub const RELOAD_QUICK: u32 = 0x15;
    pub const RELOAD_QUICK_EMPTY: u32 = 0x16;
    pub const ALT_DROP: u32 = 0x17;
    pub const ALT_RAISE: u32 = 0x18;
    pub const QUICK_DROP: u32 = 0x19;
    pub const QUICK_RAISE: u32 = 0x1a;
    pub const EMPTY_DROP: u32 = 0x1b;
    pub const EMPTY_RAISE: u32 = 0x1c;
    pub const SPRINT_IN: u32 = 0x1d;
    pub const SPRINT_LOOP: u32 = 0x1e;
    pub const SPRINT_OUT: u32 = 0x1f;
    pub const SPRINT_EMPTY_IN: u32 = 0x20;
    pub const SPRINT_EMPTY_LOOP: u32 = 0x21;
    pub const SPRINT_EMPTY_OUT: u32 = 0x22;
    pub const LOWREADY_IN: u32 = 0x23;
    pub const LOWREADY_LOOP: u32 = 0x24;
    pub const LOWREADY_OUT: u32 = 0x25;
    pub const CONT_FIRE_IN: u32 = 0x26;
    pub const CONT_FIRE_LOOP: u32 = 0x27;
    pub const CONT_FIRE_OUT: u32 = 0x28;
    pub const HOLD_FIRE: u32 = 0x29;
    pub const DETONATE: u32 = 0x2a;
    pub const NIGHTVISION_WEAR: u32 = 0x2b;
    pub const NIGHTVISION_REMOVE: u32 = 0x2c;
    pub const DEPLOY: u32 = 0x2d;
    pub const BREAKDOWN: u32 = 0x2e;
    pub const DTP_IN: u32 = 0x2f;
    pub const DTP_LOOP: u32 = 0x30;
    pub const DTP_OUT: u32 = 0x31;
    pub const DTP_EMPTY_IN: u32 = 0x32;
    pub const DTP_EMPTY_LOOP: u32 = 0x33;
    pub const DTP_EMPTY_OUT: u32 = 0x34;
    pub const SLIDE_IN: u32 = 0x35;
    pub const MANTLE: u32 = 0x36;
    /// Special fire: the weapon jams.
    pub const SPECIAL_FIRE: u32 = 0x37;
    pub const CRAWL_IN: u32 = 0x38;
    pub const CRAWL_FORWARD: u32 = 0x39;
    pub const CRAWL_BACK: u32 = 0x3a;
    pub const CRAWL_RIGHT: u32 = 0x3b;
    pub const CRAWL_LEFT: u32 = 0x3c;
    pub const CRAWL_OUT: u32 = 0x3d;
    pub const CRAWL_EMPTY_IN: u32 = 0x3e;
    pub const CRAWL_EMPTY_FORWARD: u32 = 0x3f;
    pub const CRAWL_EMPTY_BACK: u32 = 0x40;
    pub const CRAWL_EMPTY_RIGHT: u32 = 0x41;
    pub const CRAWL_EMPTY_LEFT: u32 = 0x42;
    pub const CRAWL_EMPTY_OUT: u32 = 0x43;
    /// The left hand's animation while it follows the right hand.
    pub const LEFT_FOLLOWS: u32 = 0x58;
    /// Flipped each time an animation starts, so the same one restarts.
    pub const TOGGLE: u32 = 0x400;
}

/// Black Ops 2's fire types.
pub mod fire_type {
    pub const FULL_AUTO: i32 = 0;
    pub const SINGLE_SHOT: i32 = 1;
    pub const BURST_2: i32 = 2;
    pub const BURST_3: i32 = 3;
    pub const BURST_4: i32 = 4;
    pub const BURST_5: i32 = 5;
    pub const STACKED: i32 = 6;
    pub const MINIGUN: i32 = 7;
    pub const CHARGE_SHOT: i32 = 8;
    pub const JETGUN: i32 = 9;
}

/// Black Ops 2's weapon types.
pub mod weapon_type {
    pub const BULLET: i32 = 0;
    pub const GRENADE: i32 = 1;
    pub const PROJECTILE: i32 = 2;
    pub const BINOCULARS: i32 = 3;
    pub const GAS: i32 = 4;
    pub const BOMB: i32 = 5;
    pub const MINE: i32 = 6;
    pub const MELEE: i32 = 7;
    pub const RIOTSHIELD: i32 = 8;
}

/// Black Ops 2's weapon classes.
pub mod weapon_class {
    pub const PISTOL: i32 = 4;
    pub const ITEM: i32 = 0xa;
    pub const MELEE: i32 = 0xb;
    pub const PISTOL_SPREAD: i32 = 0xd;
}

/// Black Ops 2's weapon inventory types.
pub mod inventory_type {
    pub const PRIMARY: i32 = 0;
    pub const OFFHAND: i32 = 1;
    pub const ITEM: i32 = 2;
    pub const ALT_MODE: i32 = 3;
    pub const MELEE: i32 = 4;
    pub const DW_LEFT_HAND: i32 = 5;
}

/// Black Ops 2's offhand slots.
pub mod offhand_slot {
    pub const LETHAL: i32 = 1;
    pub const TACTICAL: i32 = 2;
    pub const EQUIPMENT: i32 = 3;
    pub const SPECIFIC_USE: i32 = 4;
}

/// Black Ops 2's offhand classes.
pub mod offhand_class {
    pub const FRAG: i32 = 1;
    pub const SMOKE: i32 = 2;
    pub const FLASH: i32 = 3;
    pub const GEAR: i32 = 4;
    pub const SUPPLY_DROP: i32 = 5;
}

/// The player state's weapon animation for no animation of the left hand
/// while it follows the right.
const LEFT_FOLLOWS: u32 = weap_anim::LEFT_FOLLOWS;

fn start_anim(ps: &mut PlayerState, anim: u32) {
    if ps.pm_type < pm_type::DEAD {
        ps.weap_anim = (!ps.weap_anim & weap_anim::TOGGLE) | anim;
    }
}

fn start_anim_left(ps: &mut PlayerState, anim: u32) {
    if ps.pm_type < pm_type::DEAD {
        ps.weap_anim_left = (!ps.weap_anim_left & weap_anim::TOGGLE) | anim;
    }
}

fn start_anim_both(ps: &mut PlayerState, anim: u32) {
    start_anim(ps, anim);
    start_anim_left(ps, anim);
}

fn start_anim_hand(ps: &mut PlayerState, anim: u32, left: bool) {
    if left {
        start_anim_left(ps, anim);
    } else {
        start_anim(ps, anim);
    }
}

/// Starts an animation on a hand unless it already plays on the running one.
fn start_anim_unless_playing(ps: &mut PlayerState, anim: u32, left: bool) {
    if ps.weap_anim & !weap_anim::TOGGLE == anim && !ps.left_hand {
        return;
    }
    if ps.weap_anim_left & !weap_anim::TOGGLE == anim && ps.left_hand {
        return;
    }
    start_anim_hand(ps, anim, left);
}

/// A stance change waits for a weapon action that started while prone.
fn hold_prone(ps: &mut PlayerState) {
    if ps.pm_flags & pm_flags::PRONE != 0 {
        ps.pm_flags |= pm_flags::PRONEMOVE_OVERRIDDEN;
    }
}

/// Back to ready on both hands.
pub(crate) fn finish(ps: &mut PlayerState) {
    ps.weap_flags &= !weap_flags::OFFHAND_ACTIVE;
    ps.pm_flags &= !pm_flags::PRONEMOVE_OVERRIDDEN;
    ps.weapon_time = 0;
    ps.weapon_delay = 0;
    ps.weaponstate = ws::READY;
    ps.weapon_time_left = 0;
    ps.weapon_delay_left = 0;
    ps.weaponstate_left = ws::READY;
    start_anim_both(ps, weap_anim::IDLE);
}

/// The running hand's weapon state, time, delay and shot count.
#[derive(Clone, Copy)]
struct Hand {
    left: bool,
}

impl Hand {
    fn of(ps: &PlayerState) -> Self {
        Self { left: ps.left_hand }
    }

    fn state(self, ps: &PlayerState) -> i32 {
        if self.left {
            ps.weaponstate_left
        } else {
            ps.weaponstate
        }
    }

    fn set_state(self, ps: &mut PlayerState, value: i32) {
        if self.left {
            ps.weaponstate_left = value;
        } else {
            ps.weaponstate = value;
        }
    }

    fn time(self, ps: &PlayerState) -> i32 {
        if self.left {
            ps.weapon_time_left
        } else {
            ps.weapon_time
        }
    }

    fn set_time(self, ps: &mut PlayerState, value: i32) {
        if self.left {
            ps.weapon_time_left = value;
        } else {
            ps.weapon_time = value;
        }
    }

    fn delay(self, ps: &PlayerState) -> i32 {
        if self.left {
            ps.weapon_delay_left
        } else {
            ps.weapon_delay
        }
    }

    fn set_delay(self, ps: &mut PlayerState, value: i32) {
        if self.left {
            ps.weapon_delay_left = value;
        } else {
            ps.weapon_delay = value;
        }
    }

    fn shot_count(self, ps: &PlayerState) -> i32 {
        if self.left {
            ps.weapon_shot_count_left
        } else {
            ps.weapon_shot_count
        }
    }

    fn set_shot_count(self, ps: &mut PlayerState, value: i32) {
        if self.left {
            ps.weapon_shot_count_left = value;
        } else {
            ps.weapon_shot_count = value;
        }
    }
}

/// A weapon's definition, with any attachment scales and overrides applied.
fn def<'a, W: MoveWorld>(pm: &Pm<'a, W>, weapon: u32) -> crate::Weapon<'a> {
    pm.world.weapon(weapon)
}

/// The weapon slot of a weapon number is filled.
fn some(weapon: u32) -> bool {
    weapon & 0xff != 0
}

fn dual_wield<W: MoveWorld>(pm: &Pm<'_, W>, weapon: u32) -> bool {
    def(pm, weapon).def.dual_wield()
}

/// The weapon's variant grants the perk.
fn weapon_grants<W: MoveWorld>(pm: &Pm<'_, W>, weapon: u32, perk: usize) -> bool {
    perks::has(def(pm, weapon).def.variant_perks(), perk)
}

/// The player's perks with those the weapon's variant grants.
fn perks_with<W: MoveWorld>(pm: &Pm<'_, W>, weapon: u32) -> [u32; 2] {
    let v = def(pm, weapon).def.variant_perks();
    [pm.ps.perks[0] | v[0], pm.ps.perks[1] | v[1]]
}

/// The held weapon or its bayonet does the melee itself.
fn melee_rule<W: MoveWorld>(pm: &Pm<'_, W>, weapon: u32) -> bool {
    let d = def(pm, weapon).def;
    d.bayonet() || d.use_as_melee()
}

/// The weapon that does the melee: the held one by the melee rule, else the
/// player's melee weapon.
fn melee_weapon<W: MoveWorld>(pm: &Pm<'_, W>) -> u32 {
    if melee_rule(pm, pm.ps.weapon) {
        pm.ps.weapon
    } else {
        pm.ps.melee_weapon
    }
}

/// The weapon the viewmodel shows: the offhand while one is in hand, the
/// melee weapon while knifing with another, else the held weapon.
pub fn view_weapon<W: MoveWorld>(pm: &Pm<'_, W>) -> u32 {
    let ps = &*pm.ps;
    let s = ps.weaponstate;
    if ps.weap_flags & weap_flags::OFFHAND_ACTIVE != 0
        && s > ws::MELEE_END
        && s < ws::RIOTSHIELD_PREPARE
    {
        return ps.offhand_index;
    }
    if ws::is_melee(s) && !melee_rule(pm, ps.weapon) && s != ws::MELEE_END {
        return ps.melee_weapon;
    }
    ps.weapon
}

/// The weapon the sights and spin follow: a turret's or vehicle's when
/// mounted, else the viewmodel's.
fn aimed_weapon<W: MoveWorld>(pm: &Pm<'_, W>) -> u32 {
    if in_vehicle(pm.ps) || pm.ps.e_flags & e_flags::TURRET != 0 {
        pm.gap(Gap::MountedView);
    }
    view_weapon(pm)
}

/// Weapons are put away in a vehicle seat.
fn in_vehicle(ps: &PlayerState) -> bool {
    ps.e_flags & e_flags::VEHICLE_VIEW != 0
        && tuning::DISABLE_WEAPONS_ON_VEHICLE
        && ps.vehicle_pos < 5
}

/// Reloads happen off screen in a vehicle seat that keeps its weapons.
fn offscreen_reload<W: MoveWorld>(pm: &Pm<'_, W>) -> bool {
    let offscreen =
        pm.ps.e_flags & e_flags::VEHICLE_VIEW != 0 && !tuning::DISABLE_WEAPONS_ON_VEHICLE;
    if offscreen {
        pm.gap(Gap::MountedView);
    }
    offscreen
}

/// The weapon overheated (it fires no more until it cools).
fn overheated<W: MoveWorld>(pm: &Pm<'_, W>, weapon: u32) -> bool {
    if def(pm, weapon).def.overheat_weapon() == 0 {
        return false;
    }
    inv::held(pm.ps, weapon).is_some_and(|h| h.overheating || h.heat >= 100.0)
}

/// A held weapon's heat percentage and whether it is overheating.
pub fn heat(ps: &PlayerState, weapon: u32) -> (f32, bool) {
    inv::held(ps, weapon).map_or((0.0, false), |h| (h.heat, h.overheating || h.heat >= 100.0))
}

/// Adds `delta` to a held weapon's heat and updates its overheating latch.
pub fn add_heat_raw(
    ps: &mut PlayerState,
    weapon: u32,
    delta: f32,
    end_val: f32,
    overheat_weapon: bool,
) {
    let Some(held) = inv::held_mut(ps, weapon) else {
        return;
    };
    held.heat += delta;
    if held.heat >= 100.0 {
        held.heat = 100.0;
        held.overheating = true;
    }
    if held.heat < 0.0 {
        held.heat = 0.0;
    }
    if held.heat <= end_val || !overheat_weapon {
        held.overheating = false;
    }
}

/// Steps each held weapon's heat and cooldown for the frame (`pml.msec`).
pub(crate) fn update_weapon_heat<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let msec = pml.msec.max(0) as f32;
    let start = pm
        .ps
        .held_weapons
        .iter()
        .position(|h| h.weapon == pm.ps.weapon && (pm.ps.weapon & 0xff) != 0)
        .unwrap_or(0);
    let len = pm.ps.held_weapons.len();
    for offset in 0..len {
        let slot = (start + offset) % len;
        let weapon = pm.ps.held_weapons[slot].weapon;
        if (weapon & 0xff) == 0 {
            continue;
        }
        let d = def(pm, weapon).def;
        let end_val = d.overheat_end_val();
        let overheat_weap = d.overheat_weapon() != 0;
        let overheat_rate = d.overheat_rate();
        let cooldown_rate = d.cooldown_rate();
        let cool_firing = d.cool_while_firing();
        let firing = matches!(pm.ps.weaponstate, ws::FIRING | ws::CONT_FIRE_LOOP)
            && pm.ps.weapon == weapon
            && pm.ps.water_level < 4;
        if firing {
            add_heat_raw(
                pm.ps,
                weapon,
                msec * overheat_rate * 0.001,
                end_val,
                overheat_weap,
            );
        }
        if !firing || cool_firing {
            add_heat_raw(
                pm.ps,
                weapon,
                -(msec * cooldown_rate * 0.001),
                end_val,
                overheat_weap,
            );
        }
    }
}

/// The left gun runs its own state; otherwise it copies the right hand's.
fn left_independent<W: MoveWorld>(pm: &Pm<'_, W>) -> bool {
    let ps = &*pm.ps;
    if (perks::has(ps.perks, perks::NONAME) || weapon_grants(pm, view_weapon(pm), perks::NONAME))
        && pm.zombiemode
    {
        return true;
    }
    match ps.weaponstate {
        ws::RAISING..=ws::DROPPING_ALT
        | ws::RELOADING_INTERRUPT..=ws::RELOAD_END
        | ws::OFFHAND_PREPARE
        | ws::OFFHAND_START
        | ws::OFFHAND_HOLD
        | ws::OFFHAND
        | ws::DETONATING..=ws::CONT_FIRE_OUT
        | ws::DTP_IN..=ws::DTP_OUT
        | ws::CRAWL_IN..=ws::CRAWL_OUT => false,
        ws::MELEE_INIT..=ws::MELEE_END => {
            def(pm, ps.weapon).def.weapon_type() == weapon_type::MELEE && dual_wield(pm, ps.weapon)
        }
        _ => true,
    }
}

/// The button that fires a hand: attack for the right, the throw button for
/// a dual wield's left (swapped by the client's choice); a detonator fires
/// with the throw button.
fn fire_button<W: MoveWorld>(pm: &Pm<'_, W>, weapon: u32, left: bool) -> u32 {
    let d = def(pm, weapon).def;
    if matches!(d.weapon_type(), weapon_type::GRENADE | weapon_type::MINE) && d.has_detonator() {
        return buttons::THROW;
    }
    if !dual_wield(pm, weapon) {
        return buttons::ATTACK;
    }
    if left != pm.cmd.fire_buttons_swapped {
        buttons::THROW
    } else {
        buttons::ATTACK
    }
}

/// The firing flag, set again each move: in a vehicle while a vehicle fire
/// button or attack is held; off a turret, for each hand ready or firing
/// with rounds (or unlimited ammo), not overheated and no grenade cancelled,
/// while its fire button is held out of deep water or its weapon time runs.
pub(crate) fn update_firing_flag<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    pm.ps.e_flags &= !e_flags::FIRING;
    let ps = &*pm.ps;
    if ps.pm_type == pm_type::INTERMISSION || ps.pm_flags & pm_flags::RESPAWNED != 0 {
        return;
    }
    let cmd = pm.cmd.buttons;
    if ps.e_flags & e_flags::VEHICLE_VIEW != 0 && tuning::DISABLE_WEAPONS_ON_VEHICLE {
        if cmd.held(buttons::VEHICLE_ATTACK)
            || cmd.held(buttons::VEHICLE_ATTACK_ALT)
            || cmd.held(buttons::ATTACK)
        {
            pm.ps.e_flags |= e_flags::FIRING;
        }
        return;
    }
    if ps.e_flags & e_flags::TURRET != 0 {
        return;
    }
    let ready = |s: i32| matches!(s, ws::READY | ws::FIRING | ws::CONT_FIRE_LOOP);
    let cancelled = ps.weap_flags & weap_flags::CANCEL_GRENADE != 0;
    let mut firing = false;
    if ready(ps.weaponstate)
        && (inv::hand_clip(ps, pm.world) != 0 || def(pm, ps.weapon).def.unlimited_ammo())
        && !overheated(pm, ps.weapon)
        && !cancelled
        && ((cmd.held(buttons::ATTACK) && ps.water_level < 4) || ps.weapon_time > 0)
    {
        firing = true;
    }
    if !firing && ready(ps.weaponstate_left) && dual_wield(pm, ps.weapon) {
        let left = def(pm, ps.weapon).dual_wield_weapon;
        if (inv::clip(ps, pm.world, left) != 0 || def(pm, left).def.unlimited_ammo())
            && !overheated(pm, left)
            && !cancelled
            && ((cmd.held(buttons::THROW) && ps.water_level < 4) || ps.weapon_time_left > 0)
        {
            firing = true;
        }
    }
    if firing {
        pm.ps.e_flags |= e_flags::FIRING;
    }
}

/// The sights may come up.
fn can_ads<W: MoveWorld>(pm: &Pm<'_, W>, pml: &Pml) -> bool {
    let ps = &*pm.ps;
    match ps.pm_type {
        pm_type::NORMAL_LINKED => {
            if ps.link_flags & 4 != 0 {
                return true;
            }
            if pml.almost_ground_plane {
                return false;
            }
        }
        pm_type::SPECTATOR | pm_type::INTERMISSION | pm_type::DEAD | pm_type::DEAD_LINKED => {
            if ps.link_flags & 4 != 0 {
                return true;
            }
            if ps.e_flags & e_flags::VEHICLE_VIEW == 0 || ps.e_flags2 & e_flags2::MOUNTED == 0 {
                return false;
            }
        }
        _ => {}
    }
    if ps.other_flags & crate::state::other_flags::PLAYER == 0 {
        return false;
    }
    let weapon = aimed_weapon(pm);
    let d = def(pm, weapon).def;
    if !d.aim_down_sight() {
        return false;
    }
    if ps.water_level > 3 && ps.pm_type != pm_type::NORMAL_LINKED {
        return false;
    }
    let s = ps.weaponstate;
    if ws::is_offhand(s) || ws::is_riotshield(s) {
        return false;
    }
    match s {
        ws::RAISING | ws::DROPPING | ws::DROPPING_QUICK => return false,
        ws::RAISING_ALT | ws::DROPPING_ALT if !d.alt_weapon_ads_only() => return false,
        _ => {}
    }
    !ws::is_dtp(s)
        && ps.pm_flags & pm_flags::MANTLE == 0
        && ps.weap_flags & weap_flags::NO_ADS == 0
        && (!d.no_ads_when_mag_empty() || inv::clip(ps, pm.world, weapon) != 0)
}

/// The player means to aim down the sights this frame; while prone, only
/// when standing still or newly pressed, and then the prone move waits.
pub(crate) fn update_ads_intent<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    pm.ps.pm_flags &= !pm_flags::ADS_INTENT;
    let mut can = can_ads(pm, pml);
    let cmd = pm.cmd.buttons;
    let reticle = def(pm, pm.ps.weapon).def.ads_overlay_reticle();
    if cmd.held(buttons::SPRINT)
        && !cmd.held(buttons::THROW)
        && (reticle == 0 || !cmd.held(buttons::HOLD_BREATH))
    {
        pm.reset_ads();
        can = false;
    }
    if !cmd.held(buttons::ADS) || !can {
        return;
    }
    let ps = &mut *pm.ps;
    if ps.pm_flags & pm_flags::PRONE != 0 && (reticle == 0 || ps.weapon_pos_frac < 1.0) {
        if pm.oldcmd.buttons.held(buttons::ADS)
            && (pm.cmd.forwardmove != 0 || pm.cmd.rightmove != 0)
        {
            return;
        }
        ps.pm_flags |= pm_flags::ADS_INTENT | pm_flags::PRONEMOVE_OVERRIDDEN;
        return;
    }
    ps.pm_flags |= pm_flags::ADS_INTENT;
}

/// Aiming with the sights up and steady: the movement scales for it apply.
pub(crate) fn update_sight_aiming<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let ps = &mut *pm.ps;
    ps.pm_flags &= !pm_flags::SIGHT_AIMING;
    if ps.pm_type < pm_type::DEAD
        && pm.cmd.buttons.held(buttons::ADS)
        && ps.pm_flags & pm_flags::PRONE == 0
        && ps.pm_flags & pm_flags::ADS_INTENT != 0
        && !ws::is_reloading(ps.weaponstate)
        && !perks::has(ps.perks, perks::STALKER)
    {
        ps.pm_flags |= pm_flags::SIGHT_AIMING;
    }
}

/// `floor(x + 0.5)` the way the game rounds with the 2^23 trick.
fn floor_half(x: f32) -> f32 {
    let y = x + 0.5;
    let big = f32::from_bits(0x4b00_0000 | (y.to_bits() & 0x8000_0000));
    let r = (y + big) - big;
    if y < r { r - 1.0 } else { r }
}

/// Hip fire spreads with movement, turning and jumping and settles over
/// time; aimed it is none.
pub(crate) fn aim_spread<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let d = def(pm, pm.ps.weapon).def;
    let ps = &*pm.ps;
    let mut frac = 1.0f32;
    let mut decay = d.hip_spread_decay_rate();
    let mut increase = None;
    if decay == 0.0 {
        decay = 1.0;
    } else {
        let so = ps.spread_override as f32;
        frac = (so - d.hip_spread_stand_min()) / (d.hip_spread_max() - d.hip_spread_stand_min());
        let in_air = ps.ground_entity_num == crate::state::ENTITYNUM_NONE
            && ps.pm_type != pm_type::NORMAL_LINKED;
        if in_air {
            decay *= 0.5;
        } else if ps.e_flags & e_flags::PRONE != 0 {
            decay *= d.hip_spread_prone_decay();
            frac = (so - d.hip_spread_prone_min())
                / (d.hip_spread_prone_max() - d.hip_spread_prone_min());
        } else if ps.e_flags & e_flags::CROUCHING != 0 {
            decay *= d.hip_spread_ducked_decay();
            frac = (so - d.hip_spread_ducked_min())
                / (d.hip_spread_ducked_max() - d.hip_spread_ducked_min());
        }
        let ft = pml.frametime;
        if ps.spread_override_state == 1 {
            decay = ft * decay / frac;
        } else {
            decay *= ft;
            let aimed = ps.weapon_pos_frac == 1.0
                && (!tuning::ANTI_QUICK_SCOPE || ps.quick_scope_timer == 0);
            if aimed {
                if tuning::ADS_ZERO_SPREAD {
                    pm.ps.aim_spread_scale = 0.0;
                }
            } else {
                let mut add = ads::quick_scope_spread(pm);
                let ps = &*pm.ps;
                if d.hip_spread_turn_add() != 0.0 {
                    for axis in 0..2 {
                        let turn = (pm.cmd.angles[axis] as f32 * 0.005493164
                            - pm.oldcmd.angles[axis] as f32 * 0.005493164)
                            * 0.0027777778;
                        let turn = (turn - floor_half(turn)) * 360.0;
                        add += turn.abs() * 0.01 * d.hip_spread_turn_add() * (1.0 / ft);
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
                if ps.e_flags & e_flags::VEHICLE_VIEW != 0 && !tuning::DISABLE_WEAPONS_ON_VEHICLE {
                    pm.gap(Gap::MountedView);
                }
                if in_air {
                    add = add + 1.28 + 1.28;
                }
                let add = ft * add;
                if 0.0 < add {
                    increase = Some(add * 255.0 + ps.aim_spread_scale);
                }
            }
        }
    }
    let ps = &mut *pm.ps;
    let scale = increase.unwrap_or(ps.aim_spread_scale - decay * 255.0);
    ps.aim_spread_scale = scale;
    if ps.spread_override_state == 1 && scale * frac < 255.0 {
        ps.spread_override_state = 0;
        ps.aim_spread_scale = scale * frac;
    }
    ps.aim_spread_scale = ps.aim_spread_scale.clamp(0.0, 255.0);
}

/// Black Ops 2's weapon rules for the running hand, once per movement slice.
pub(crate) fn pm_weapon<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    if pm.ps.left_hand && !dual_wield(pm, pm.ps.weapon) {
        return;
    }
    let ps = &mut *pm.ps;
    if ps.pm_type >= pm_type::DEAD && ps.link_flags & 4 == 0 && ps.e_flags2 & e_flags2::MOUNTED == 0
    {
        ps.weapon = 0;
        return;
    }
    if ps.pm_flags & pm_flags::RESPAWNED != 0 {
        return;
    }
    if ps.e_flags & e_flags::TURRET != 0
        && (ps.e_flags2 & e_flags2::MOUNTED == 0 || ps.weaponstate == ws::READY)
    {
        return;
    }
    if ps.water_level < 4 {
        if ps.weaponstate == ws::SWIM_IN {
            ps.weaponstate = ws::SWIM_OUT;
            ps.weapon_time = 1000;
        }
    } else {
        let delay_done = fire::weapon_timers(pm, pml);
        if !fire::mid_burst(pm) {
            melee::check_melee(pm, pml, delay_done);
        }
        let s = pm.ps.weaponstate;
        if s != ws::SWIM_IN
            && !(ws::OFFHAND_INIT..=ws::OFFHAND_FORCE_END).contains(&s)
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
        } else if s == ws::SWIM_IN {
            if pm.ps.weap_flags & weap_flags::SWIM_PUTAWAY == 0 {
                return;
            }
            pm.ps.weap_flags &= !weap_flags::SWIM_PUTAWAY;
        }
    }
    if !ws::is_raising_or_dropping(pm.ps.weaponstate) {
        pm.ps.weap_flags &= !weap_flags::SWITCHING_TO_RIOTSHIELD;
    }
    let old_frac = pm.ps.weapon_pos_frac;
    ads::ads_lerp(pm, pml);
    ads::spin(pm, pml);
    ads::scope_zoom(pm);
    ads::hold_breath(pm, pml);
    ads::idle_time(pm, pml);
    ads::anti_quick_scope(pm, pml, old_frac);
    fire::charge_shot(pm, pml);
    ads::airburst_ads(pm);
    if pm.ps.left_hand {
        if !left_independent(pm) {
            let ps = &mut *pm.ps;
            ps.weaponstate_left = ps.weaponstate;
            ps.weapon_delay_left = 0;
            ps.weapon_time_left = 0;
            ps.weap_anim_left = LEFT_FOLLOWS;
            return;
        }
        let ps = &mut *pm.ps;
        if ps.weap_anim_left == LEFT_FOLLOWS {
            ps.weaponstate_left = ws::READY;
            ps.weapon_delay_left = 0;
            ps.weapon_time_left = 0;
            if ps.pm_type < pm_type::DEAD {
                ps.weap_anim_left = weap_anim::TOGGLE;
            }
            return;
        }
    }
    if offhand::grenade_cook(pm, pml) {
        return;
    }
    fire::latch_fire(pm);
    let delay_done = fire::weapon_timers(pm, pml);
    if !fire::mid_burst(pm) {
        pose::check_nightvision(pm);
        pose::check_sprint(pm);
        pose::check_dtp(pm);
        pose::check_slide(pm);
        pose::check_deploy(pm);
        offhand::check_offhand(pm, pml);
        offhand::check_riotshield(pm);
        switch::check_change_weapon(pm, pml);
        reload::check_reload(pm, pml);
        reload::burn_fuel(pm, pml);
        melee::check_melee(pm, pml, delay_done);
        pose::check_detonate(pm);
        offhand::check_offhand_cancel(pm);
        pose::check_mantle_anim(pm);
        pose::check_cont_fire(pm, delay_done);
        pose::check_prone_crawl(pm);
        if pm.cmd.buttons.held(buttons::AIRBURST_RANGE) {
            pm.ps.airburst_distance = i32::from(pm.cmd.airburst_distance);
        }
    }
    if fire::rechamber(pm, delay_done) {
        return;
    }
    let ps = &mut *pm.ps;
    let moving_prone = ps.pm_flags & pm_flags::PRONE != 0
        && (pm.cmd.forwardmove != 0 || pm.cmd.rightmove != 0)
        && ps.weapon_pos_frac != 1.0;
    if moving_prone || ws::is_melee(ps.weaponstate) {
        ps.aim_spread_scale = 255.0;
    }
    let hand = Hand::of(ps);
    if !delay_done && (hand.time(ps) != 0 || hand.delay(ps) != 0) {
        return;
    }
    if !pm.ps.left_hand {
        match pm.ps.weaponstate {
            ws::RAISING | ws::RAISING_ALT => {
                let ps = &mut *pm.ps;
                ps.weaponstate = ws::READY;
                ps.weaponstate_left = ws::READY;
                start_anim_both(ps, weap_anim::IDLE);
                return;
            }
            s @ (ws::DROPPING | ws::DROPPING_QUICK | ws::DROPPING_ALT) => {
                switch::finish_weapon_change(pm, pml, s);
                return;
            }
            ws::RELOADING | ws::RELOADING_INTERRUPT => {
                reload::reloading(pm, pml, delay_done);
                return;
            }
            ws::RELOAD_START | ws::RELOAD_START_RECHAMBER | ws::RELOAD_START_INTERRUPT => {
                reload::reload_start_state(pm, delay_done);
                return;
            }
            ws::RELOAD_END => {
                let ps = &mut *pm.ps;
                ps.weaponstate = ws::READY;
                start_anim(ps, weap_anim::IDLE);
                return;
            }
            ws::MELEE_INIT => {
                pm.ps.weaponstate = ws::MELEE_FIRE;
                melee::melee_fire(pm);
                return;
            }
            ws::MELEE_FIRE => {
                melee::melee_end(pm, false);
                return;
            }
            ws::MELEE_END
            | ws::OFFHAND_END
            | ws::RIOTSHIELD_END
            | ws::SPRINT_DROP
            | ws::CONT_FIRE_OUT
            | ws::DTP_OUT
            | ws::SLIDE_IN
            | ws::FIRING_SPECIAL
            | ws::CRAWL_OUT => {
                finish(pm.ps);
                return;
            }
            ws::OFFHAND_INIT => {
                offhand::offhand_prepare(pm, pml);
                return;
            }
            ws::OFFHAND_PREPARE => {
                offhand::offhand_hold(pm);
                return;
            }
            ws::OFFHAND_START => {
                offhand::offhand_throw(pm);
                return;
            }
            ws::OFFHAND_DETONATE | ws::DETONATING => {
                pose::detonating(pm, delay_done);
                return;
            }
            ws::OFFHAND_HOLD => {
                offhand::offhand_start(pm, pml);
                return;
            }
            ws::OFFHAND => {
                offhand::offhand_end(pm);
                return;
            }
            ws::OFFHAND_FORCE_END => {
                offhand::offhand_force_putaway(pm, pml);
                return;
            }
            ws::RIOTSHIELD_PREPARE => {
                let ps = &mut *pm.ps;
                ps.weaponstate = ws::RIOTSHIELD_PLACING;
                ps.weapon_time = 0;
                ps.weapon_delay = 0;
                return;
            }
            ws::RIOTSHIELD_DEPLOY => {
                let weapon = pm.ps.weapon;
                pm.event(event::DEPLOY_RIOTSHIELD, weapon as i32);
                inv::use_clip(pm.ps, pm.world, weapon, 1);
                pm.ps.weaponstate = ws::RIOTSHIELD_END;
                return;
            }
            ws::RIOTSHIELD_PLACING => {
                offhand::riotshield_place(pm, pml);
                return;
            }
            ws::SPRINT_RAISE => {
                pose::sprint_loop(pm);
                return;
            }
            ws::SPRINT_LOOP | ws::DEPLOYED => return,
            ws::CONT_FIRE_IN => pose::cont_fire_loop(pm.ps),
            ws::NIGHTVISION_WEAR | ws::NIGHTVISION_REMOVE => {
                let ps = &mut *pm.ps;
                if ps.weapon_time == 0 {
                    ps.weaponstate = ws::READY;
                    start_anim(ps, weap_anim::IDLE);
                }
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
            ws::SWIM_IN => {
                pose::swim_putaway(pm);
                return;
            }
            ws::SWIM_OUT => {
                pose::swim_raise(pm);
                return;
            }
            ws::DTP_IN => {
                pose::dtp_loop(pm);
                return;
            }
            ws::DTP_LOOP => {
                if pose::diving(pm.ps) || pm.ps.pm_flags & pm_flags::DIVING == 0 {
                    pose::dtp_out(pm);
                }
                return;
            }
            ws::CRAWL_IN => {
                pose::crawl_direction(pm, delay_done);
                return;
            }
            ws::CRAWL_FORWARD..=ws::CRAWL_LEFT => {
                pose::crawl_loop(pm, delay_done);
                return;
            }
            _ => {}
        }
    } else {
        match pm.ps.weaponstate_left {
            ws::RELOADING | ws::RELOADING_INTERRUPT => {
                reload::reloading(pm, pml, delay_done);
                return;
            }
            ws::MELEE_INIT => {
                pm.ps.weaponstate_left = ws::MELEE_FIRE;
                melee::melee_fire(pm);
                return;
            }
            ws::MELEE_FIRE => {
                melee::melee_end(pm, true);
                return;
            }
            _ => {}
        }
    }
    if !some(pm.ps.weapon) {
        return;
    }
    let left = pm.ps.left_hand;
    if !fire::fire_held(pm, delay_done, false, left) || fire::cancel_grenade(pm) {
        return;
    }
    if delay_done && fire::grenade_fire_held(pm) {
        pm.ps.weapon_delay = 1;
        return;
    }
    let ps = &*pm.ps;
    if ps.pm_flags & pm_flags::FROZEN != 0
        || ps.weaponstate == ws::DEPLOYING
        || ps.other_flags & crate::state::other_flags::CONTROLS_LOCKED != 0
    {
        return;
    }
    fire::fire(pm, pml, delay_done);
}
