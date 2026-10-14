use crate::state::{buttons, pm_flags, pm_type};
use crate::{MoveWorld, Pm, Pml};

pub mod weapon_class {
    pub const ITEM: i32 = 8;
    pub const MELEE: i32 = 9;
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
        && !matches!(ps.weaponstate, 0xb | 0xd..=0x12)
        && !crate::perks::has(ps.perks, crate::perks::STALKER)
    {
        ps.pm_flags |= pm_flags::SIGHT_AIMING;
    }
}

pub(crate) fn pm_weapon<W: MoveWorld>(_pm: &mut Pm<'_, W>, _pml: &Pml) {}
