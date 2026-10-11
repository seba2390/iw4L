//! A Black Ops weapon definition and its variant, read by field name from
//! the bytes the zone holds for them.

use crate::size::{WEAPON_DEF, WEAPON_VARIANT_DEF};

/// The definition's and the variant's bytes. Indices the game resolves at
/// load (ammo, clip, alternate and dual-wield weapons) are not among them.
#[derive(Clone, Copy, Debug)]
pub struct WeaponDefView<'a> {
    def: &'a [u8],
    variant: &'a [u8],
}

impl<'a> WeaponDefView<'a> {
    /// `None` when either block is shorter than the game's.
    pub fn new(def: &'a [u8], variant: &'a [u8]) -> Option<Self> {
        (def.len() >= WEAPON_DEF && variant.len() >= WEAPON_VARIANT_DEF)
            .then_some(Self { def, variant })
    }

    fn word(bytes: &[u8], off: usize) -> [u8; 4] {
        [bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]]
    }

    /// `weaponType`
    pub fn weapon_type(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x1c))
    }

    /// `weaponClass`
    pub fn weapon_class(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x20))
    }

    /// `inventoryType`
    pub fn inventory_type(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x2c))
    }

    /// `offhandClass`
    pub fn offhand_class(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x68))
    }

    /// `offhandSlot`
    pub fn offhand_slot(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x6c))
    }

    /// `fireType`
    pub fn fire_type(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x30))
    }

    /// `fireTime`
    pub fn fire_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x3ac))
    }

    /// `fireDelay`
    pub fn fire_delay(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x378))
    }

    /// `lastFireTime`
    pub fn last_fire_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x3b0))
    }

    /// `meleeTime`
    pub fn melee_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x3c4))
    }

    /// `meleeDelay`
    pub fn melee_delay(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x37c))
    }

    /// `meleeChargeTime`
    pub fn melee_charge_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x3c8))
    }

    /// `meleeChargeDelay`
    pub fn melee_charge_delay(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x380))
    }

    /// `meleeDamage`
    pub fn melee_damage(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x36c))
    }

    /// `reloadAddTime`
    pub fn reload_add_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x3dc))
    }

    /// `reloadEmptyAddTime`
    pub fn reload_empty_add_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x3e0))
    }

    /// `reloadStartTime`
    pub fn reload_start_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x3ec))
    }

    /// `reloadStartAddTime`
    pub fn reload_start_add_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x3f0))
    }

    /// `reloadEndTime`
    pub fn reload_end_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x3f4))
    }

    /// `reloadAmmoAdd`
    pub fn reload_ammo_add(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x584))
    }

    /// `reloadStartAdd`
    pub fn reload_start_add(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x588))
    }

    /// `rechamberTime`
    pub fn rechamber_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x3b4))
    }

    /// `rechamberBoltTime`
    pub fn rechamber_bolt_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x3b8))
    }

    /// `dropTime`
    pub fn drop_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x3f8))
    }

    /// `raiseTime`
    pub fn raise_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x3fc))
    }

    /// `altDropTime`
    pub fn alt_drop_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x400))
    }

    /// `quickDropTime`
    pub fn quick_drop_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x404))
    }

    /// `quickRaiseTime`
    pub fn quick_raise_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x408))
    }

    /// `firstRaiseTime`
    pub fn first_raise_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x40c))
    }

    /// `emptyRaiseTime`
    pub fn empty_raise_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x410))
    }

    /// `emptyDropTime`
    pub fn empty_drop_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x414))
    }

    /// `sprintInTime`
    pub fn sprint_in_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x418))
    }

    /// `sprintOutTime`
    pub fn sprint_out_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x420))
    }

    /// `lowReadyInTime`
    pub fn low_ready_in_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x424))
    }

    /// `lowReadyOutTime`
    pub fn low_ready_out_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x42c))
    }

    /// `contFireInTime`
    pub fn cont_fire_in_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x430))
    }

    /// `contFireOutTime`
    pub fn cont_fire_out_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x438))
    }

    /// `dtpInTime`
    pub fn dtp_in_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x43c))
    }

    /// `dtpOutTime`
    pub fn dtp_out_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x444))
    }

    /// `slideInTime`
    pub fn slide_in_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x448))
    }

    /// `deployTime`
    pub fn deploy_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x44c))
    }

    /// `breakdownTime`
    pub fn breakdown_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x450))
    }

    /// `nightVisionWearTime`
    pub fn night_vision_wear_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x454))
    }

    /// `nightVisionRemoveTime`
    pub fn night_vision_remove_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x460))
    }

    /// `detonateTime`
    pub fn detonate_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x3c0))
    }

    /// `detonateDelay`
    pub fn detonate_delay(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x384))
    }

    /// `holdFireTime`
    pub fn hold_fire_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x3bc))
    }

    /// `fuseTime`
    pub fn fuse_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x46c))
    }

    /// `boltAction`
    pub fn bolt_action(&self) -> bool {
        self.def[0x54f] != 0
    }

    /// `unlimitedAmmo`
    pub fn unlimited_ammo(&self) -> bool {
        self.def[0x358] != 0
    }

    /// `hasDetonator`
    pub fn has_detonator(&self) -> bool {
        self.def[0x639] != 0
    }

    /// `guidedMissileType`
    pub fn guided_missile_type(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x668))
    }

    /// `noADSAutoReload`
    pub fn no_ads_auto_reload(&self) -> bool {
        self.def[0x583] != 0
    }

    /// `dualWield`
    pub fn dual_wield(&self) -> bool {
        self.def[0x56a] != 0
    }

    /// `segmentedReload`
    pub fn segmented_reload(&self) -> bool {
        self.def[0x582] != 0
    }

    /// `noPartialReload`
    pub fn no_partial_reload(&self) -> bool {
        self.def[0x581] != 0
    }

    /// `fuelTankWeapon`
    pub fn fuel_tank_weapon(&self) -> bool {
        self.def[0x61] != 0
    }

    /// `tankLifeTime`
    pub fn tank_life_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x64))
    }

    /// `overheatWeapon`
    pub fn overheat_weapon(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x50))
    }

    /// `requireLockonToFire`
    pub fn require_lockon_to_fire(&self) -> bool {
        self.def[0x47c] != 0
    }

    /// `adsFire`
    pub fn ads_fire(&self) -> bool {
        self.def[0x564] != 0
    }

    /// `aimDownSight`
    pub fn aim_down_sight(&self) -> bool {
        self.def[0x553] != 0
    }

    /// `reloadWhileAds`
    pub fn reload_while_ads(&self) -> bool {
        self.def[0x555] != 0
    }

    /// `rechamberWhileAds`
    pub fn rechamber_while_ads(&self) -> bool {
        self.def[0x554] != 0
    }

    /// `adsReloadTransTime`
    pub fn ads_reload_trans_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x73c))
    }

    /// `clipOnly`
    pub fn clip_only(&self) -> bool {
        self.def[0x561] != 0
    }

    /// `noDropsOrRaises`
    pub fn no_drops_or_raises(&self) -> bool {
        self.def[0x563] != 0
    }

    /// `noQuickDropWhenEmpty`
    pub fn no_quick_drop_when_empty(&self) -> bool {
        self.def[0x574] != 0
    }

    /// `canUseInVehicle`
    pub fn can_use_in_vehicle(&self) -> bool {
        self.def[0x562] != 0
    }

    /// `mountableWeapon`
    pub fn mountable_weapon(&self) -> bool {
        self.def[0x498] != 0
    }

    /// `bayonet`
    pub fn bayonet(&self) -> bool {
        self.def[0x569] != 0
    }

    /// `useAsMelee`
    pub fn use_as_melee(&self) -> bool {
        self.def[0x64c] != 0
    }

    /// `holdButtonToThrow`
    pub fn hold_button_to_throw(&self) -> bool {
        self.def[0x63e] != 0
    }

    /// `offhandHoldIsCancelable`
    pub fn offhand_hold_is_cancelable(&self) -> bool {
        self.def[0x63f] != 0
    }

    /// `cookOffHold`
    pub fn cook_off_hold(&self) -> bool {
        self.def[0x560] != 0
    }

    /// `plantable`
    pub fn plantable(&self) -> bool {
        self.def[0x638] != 0
    }

    /// `continuousFire`
    pub fn continuous_fire(&self) -> bool {
        self.def[0x570] != 0
    }

    /// `stackFire`
    pub fn stack_fire(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x480))
    }

    /// `rotateType`
    pub fn rotate_type(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x634))
    }

    /// `hipGunKickReducedKickBullets`
    pub fn hip_gun_kick_reduced_kick_bullets(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x6cc))
    }

    /// `adsGunKickReducedKickBullets`
    pub fn ads_gun_kick_reduced_kick_bullets(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x688))
    }

    /// `hipSpreadFireAdd`
    pub fn hip_spread_fire_add(&self) -> f32 {
        f32::from_le_bytes(Self::word(self.def, 0x4e8))
    }

    /// `hipSpreadStandMin`
    pub fn hip_spread_stand_min(&self) -> f32 {
        f32::from_le_bytes(Self::word(self.def, 0x4cc))
    }

    /// `hipSpreadDuckedMin`
    pub fn hip_spread_ducked_min(&self) -> f32 {
        f32::from_le_bytes(Self::word(self.def, 0x4d0))
    }

    /// `hipSpreadProneMin`
    pub fn hip_spread_prone_min(&self) -> f32 {
        f32::from_le_bytes(Self::word(self.def, 0x4d4))
    }

    /// `hipSpreadMax`
    pub fn hip_spread_max(&self) -> f32 {
        f32::from_le_bytes(Self::word(self.def, 0x4d8))
    }

    /// `hipSpreadDuckedMax`
    pub fn hip_spread_ducked_max(&self) -> f32 {
        f32::from_le_bytes(Self::word(self.def, 0x4dc))
    }

    /// `hipSpreadProneMax`
    pub fn hip_spread_prone_max(&self) -> f32 {
        f32::from_le_bytes(Self::word(self.def, 0x4e0))
    }

    /// `hipSpreadDecayRate`
    pub fn hip_spread_decay_rate(&self) -> f32 {
        f32::from_le_bytes(Self::word(self.def, 0x4e4))
    }

    /// `hipSpreadDuckedDecay`
    pub fn hip_spread_ducked_decay(&self) -> f32 {
        f32::from_le_bytes(Self::word(self.def, 0x4f4))
    }

    /// `hipSpreadProneDecay`
    pub fn hip_spread_prone_decay(&self) -> f32 {
        f32::from_le_bytes(Self::word(self.def, 0x4f8))
    }

    /// `hipSpreadTurnAdd`
    pub fn hip_spread_turn_add(&self) -> f32 {
        f32::from_le_bytes(Self::word(self.def, 0x4ec))
    }

    /// `hipSpreadMoveAdd`
    pub fn hip_spread_move_add(&self) -> f32 {
        f32::from_le_bytes(Self::word(self.def, 0x4f0))
    }

    /// `adsOverlayReticle`
    pub fn ads_overlay_reticle(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x4b4))
    }

    /// `noAdsWhenMagEmpty`
    pub fn no_ads_when_mag_empty(&self) -> bool {
        self.def[0x47d] != 0
    }

    /// `spinUpTime`
    pub fn spin_up_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x388))
    }

    /// `spinDownTime`
    pub fn spin_down_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.def, 0x38c))
    }

    /// `moveSpeedScale`
    pub fn move_speed_scale(&self) -> f32 {
        f32::from_le_bytes(Self::word(self.def, 0x4a8))
    }

    /// `adsMoveSpeedScale`
    pub fn ads_move_speed_scale(&self) -> f32 {
        f32::from_le_bytes(Self::word(self.def, 0x4ac))
    }

    /// `sprintDurationScale`
    pub fn sprint_duration_scale(&self) -> f32 {
        f32::from_le_bytes(Self::word(self.def, 0x4b0))
    }

    /// `sprintScale`
    pub fn sprint_scale(&self) -> f32 {
        f32::from_le_bytes(Self::word(self.def, 0x22c))
    }

    /// `duckedSprintScale`
    pub fn ducked_sprint_scale(&self) -> f32 {
        f32::from_le_bytes(Self::word(self.def, 0x208))
    }

    /// `dtpScale`
    pub fn dtp_scale(&self) -> f32 {
        f32::from_le_bytes(Self::word(self.def, 0x268))
    }

    /// `blocksProne`
    pub fn blocks_prone(&self) -> bool {
        self.def[0x5a8] != 0
    }

    /// `freezeMovementWhenFiring`
    pub fn freeze_movement_when_firing(&self) -> bool {
        self.def[0x640] != 0
    }

    /// `meleeChargeRange`
    pub fn melee_charge_range(&self) -> f32 {
        f32::from_le_bytes(Self::word(self.def, 0x648))
    }

    /// `clipSize`
    pub fn clip_size(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.variant, 0x20))
    }

    /// `reloadTime`
    pub fn reload_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.variant, 0x24))
    }

    /// `reloadEmptyTime`
    pub fn reload_empty_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.variant, 0x28))
    }

    /// `reloadQuickTime`
    pub fn reload_quick_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.variant, 0x2c))
    }

    /// `reloadQuickEmptyTime`
    pub fn reload_quick_empty_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.variant, 0x30))
    }

    /// `altRaiseTime`
    pub fn alt_raise_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.variant, 0x3c))
    }

    /// `dualMag`
    pub fn dual_mag(&self) -> bool {
        self.variant[0x85] != 0
    }

    /// `adsTransInTime`
    pub fn ads_trans_in_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.variant, 0x34))
    }

    /// `adsTransOutTime`
    pub fn ads_trans_out_time(&self) -> i32 {
        i32::from_le_bytes(Self::word(self.variant, 0x38))
    }
}
