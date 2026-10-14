use crate::state::{buttons, pm_flags, pm_type, weapon_state as ws};
use crate::{MoveWorld, Pm, Pml};

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

pub(crate) fn update_firing_flag<W: MoveWorld>(_pm: &mut Pm<'_, W>) {}

pub(crate) fn aim_spread<W: MoveWorld>(_pm: &mut Pm<'_, W>, _pml: &Pml) {}

pub(crate) fn update_ads_intent<W: MoveWorld>(pm: &mut Pm<'_, W>, _pml: &Pml) {
    let ps = &mut *pm.ps;
    if ps.pm_type >= pm_type::DEAD
        || ps.pm_flags & (pm_flags::RESPAWNED | pm_flags::MANTLE | pm_flags::LADDER) != 0
        || ps.water_level >= 4
        || ps.weap_flags & crate::state::weap_flags::NO_ADS != 0
        || !pm.cmd.buttons.held(buttons::ADS)
    {
        ps.pm_flags &= !pm_flags::ADS_INTENT;
        return;
    }
    if pm.world.weapon(ps.weapon).def.aim_down_sight() {
        ps.pm_flags |= pm_flags::ADS_INTENT;
    } else {
        ps.pm_flags &= !pm_flags::ADS_INTENT;
    }
}

pub(crate) fn update_sight_aiming<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let ps = &mut *pm.ps;
    ps.pm_flags &= !pm_flags::SIGHT_AIMING;
    if ps.pm_type < pm_type::DEAD
        && pm.cmd.buttons.held(buttons::ADS)
        && ps.pm_flags & pm_flags::PRONE == 0
        && ps.pm_flags & pm_flags::ADS_INTENT != 0
        && !ws::is_reloading(ps.weaponstate)
        && !crate::perks::has(ps.perks, crate::perks::STALKER)
    {
        ps.pm_flags |= pm_flags::SIGHT_AIMING;
    }
}

pub(crate) fn pm_weapon<W: MoveWorld>(_pm: &mut Pm<'_, W>, _pml: &Pml) {}
