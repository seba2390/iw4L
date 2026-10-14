use crate::walk::{LoadedAsset, Ptr, ZoneLoad};

pub mod variant {
    pub const INTERNAL_NAME: u32 = 0;
    pub const WEAP_DEF: u32 = 8;
    pub const DISPLAY_NAME: u32 = 12;
    pub const ALT_WEAPON_NAME: u32 = 16;
    pub const XANIMS: u32 = 32;
    pub const XANIM_COUNT: u32 = 88;
    pub const ATTACH_VIEW_MODEL: u32 = 40;
    pub const ATTACH_WORLD_MODEL: u32 = 44;
    pub const ATTACH_VIEW_MODEL_OFFSETS: u32 = 56;
    pub const ATTACH_WORLD_MODEL_OFFSETS: u32 = 152;
    pub const ATTACH_VIEW_MODEL_ROTATIONS: u32 = 248;
    pub const ATTACH_WORLD_MODEL_ROTATIONS: u32 = 344;
    pub const ATTACH_MODEL_COUNT: u32 = 8;
    pub const ATTACHMENT_UNIQUES: u32 = 28;
    pub const ATTACHMENTS: u32 = 24;
    pub const ATTACHMENT_COUNT: u32 = 63;
    pub const ATTACHMENT_UNIQUE_COUNT: u32 = 95;
    pub const CLIP_SIZE: u32 = 476;
    pub const RELOAD_TIME: u32 = 480;
    pub const RELOAD_EMPTY_TIME: u32 = 484;
    pub const RELOAD_QUICK_TIME: u32 = 488;
    pub const RELOAD_QUICK_EMPTY_TIME: u32 = 492;
    pub const ADS_TRANS_IN_TIME: u32 = 496;
    pub const ADS_TRANS_OUT_TIME: u32 = 500;
    pub const ALT_RAISE_TIME: u32 = 504;
    pub const AMMO_NAME: u32 = 512;
    pub const CLIP_NAME: u32 = 520;
    pub const OVERLAY_MATERIAL: u32 = 596;
    pub const ADS_VIEW_KICK_CENTER_SPEED: u32 = 540;
    pub const HIP_VIEW_KICK_CENTER_SPEED: u32 = 544;
    pub const ADS_ZOOM_FOV1: u32 = 548;
    pub const ADS_ZOOM_IN_FRAC: u32 = 560;
    pub const ADS_ZOOM_OUT_FRAC: u32 = 564;
    pub const SILENCED: u32 = 580;
    pub const DUAL_MAG: u32 = 581;
    /// The variant's two perk words, added to its holder's perks; bit 3 lays
    /// a spread weapon's pellets on a level line.
    pub const PERKS: u32 = 584;
    pub const SPREAD_FLAT_BIT: u32 = 3;
    pub const SIZE: usize = 716;
}

pub mod def {
    pub const CAMOUFLAGE: u32 = 2420;
    pub const GUN_XMODEL: u32 = 4;
    pub const HAND_XMODEL: u32 = 8;
    pub const PLAYER_ANIM_TYPE: u32 = 24;
    pub const WEAP_TYPE: u32 = 28;
    pub const WEAP_CLASS: u32 = 32;
    pub const PENETRATE_TYPE: u32 = 36;
    pub const IMPACT_TYPE: u32 = 40;
    pub const VIEW_FLASH_EFFECT: u32 = 108;
    pub const WORLD_FLASH_EFFECT: u32 = 112;
    pub const VIEW_SHELL_EJECT_EFFECT: u32 = 464;
    pub const WORLD_SHELL_EJECT_EFFECT: u32 = 468;
    pub const VIEW_LAST_SHOT_EJECT_EFFECT: u32 = 472;
    pub const WORLD_LAST_SHOT_EJECT_EFFECT: u32 = 476;
    pub const INVENTORY_TYPE: u32 = 44;
    pub const FIRE_TYPE: u32 = 48;
    pub const OVERHEAT_WEAPON: u32 = 72;
    pub const OVERHEAT_RATE: u32 = 76;
    pub const COOLDOWN_RATE: u32 = 80;
    pub const OVERHEAT_END_VAL: u32 = 84;
    pub const COOL_WHILE_FIRING: u32 = 88;
    pub const OFFHAND_CLASS: u32 = 96;
    pub const OFFHAND_SLOT: u32 = 100;
    pub const TANK_LIFE_TIME: u32 = 92;
    pub const WORLD_MODEL_COUNT: u32 = 16;
    pub const PICKUP_SOUND: u32 = 148;
    pub const PICKUP_SOUND_PLAYER: u32 = 152;
    pub const PULLBACK_SOUND: u32 = 168;
    pub const PULLBACK_SOUND_PLAYER: u32 = 172;
    pub const FIRE_SOUND: u32 = 176;
    pub const FIRE_SOUND_PLAYER: u32 = 180;
    pub const FIRE_LAST_SOUND: u32 = 224;
    pub const FIRE_LAST_SOUND_PLAYER: u32 = 228;
    pub const EMPTY_FIRE_SOUND: u32 = 232;
    pub const EMPTY_FIRE_SOUND_PLAYER: u32 = 236;
    pub const MELEE_SWIPE_SOUND: u32 = 248;
    pub const MELEE_SWIPE_SOUND_PLAYER: u32 = 252;
    pub const MELEE_HIT_SOUND: u32 = 256;
    pub const MELEE_MISS_SOUND: u32 = 260;
    pub const RAISE_SOUND: u32 = 384;
    pub const RAISE_SOUND_PLAYER: u32 = 388;
    pub const FIRST_RAISE_SOUND: u32 = 392;
    pub const FIRST_RAISE_SOUND_PLAYER: u32 = 396;
    pub const PUTAWAY_SOUND: u32 = 408;
    pub const PUTAWAY_SOUND_PLAYER: u32 = 412;
    /// The stand, crouch and prone mounted weapons' names.
    pub const MOUNTED_WEAPONS: [u32; 3] = [440, 444, 448];
    pub const PROJ_EXPLOSION_SOUND: u32 = 1820;
    pub const RETICLE_CENTER: u32 = 528;
    pub const RETICLE_SIDE: u32 = 532;
    pub const RETICLE_CENTER_SIZE: u32 = 536;
    pub const RETICLE_SIDE_SIZE: u32 = 540;
    pub const RETICLE_MIN_OFS: u32 = 544;
    pub const WORLD_MODEL: u32 = 916;
    pub const ROCKET_MODEL: u32 = 924;
    pub const HUD_ICON: u32 = 940;
    pub const AMMO_COUNTER_CLIP: u32 = 964;
    pub const START_AMMO: u32 = 968;
    pub const MAX_AMMO: u32 = 972;
    pub const SHOT_COUNT: u32 = 976;
    pub const UNLIMITED_AMMO: u32 = 992;
    pub const AMMO_COUNT_CLIP_RELATIVE: u32 = 993;
    pub const DAMAGE: u32 = 996;
    pub const DAMAGE_RANGE: u32 = 1020;
    pub const DAMAGE_STEPS: usize = 6;
    pub const MIN_PLAYER_DAMAGE: u32 = 1044;
    pub const PLAYER_DAMAGE: u32 = 1056;
    pub const MELEE_DAMAGE: u32 = 1060;
    pub const FIRE_DELAY: u32 = 1072;
    pub const MELEE_DELAY: u32 = 1076;
    pub const MELEE_CHARGE_DELAY: u32 = 1080;
    pub const DETONATE_DELAY: u32 = 1084;
    pub const SPIN_UP_TIME: u32 = 1088;
    pub const SPIN_DOWN_TIME: u32 = 1092;
    pub const FIRE_TIME: u32 = 1128;
    pub const LAST_FIRE_TIME: u32 = 1132;
    pub const RECHAMBER_TIME: u32 = 1136;
    pub const RECHAMBER_BOLT_TIME: u32 = 1140;
    pub const HOLD_FIRE_TIME: u32 = 1144;
    pub const DETONATE_TIME: u32 = 1148;
    pub const MELEE_TIME: u32 = 1152;
    pub const BURST_DELAY_TIME: u32 = 1156;
    pub const MELEE_CHARGE_TIME: u32 = 1160;
    pub const RELOAD_SHOW_ROCKET_TIME: u32 = 1172;
    pub const RELOAD_ADD_TIME: u32 = 1180;
    pub const RELOAD_EMPTY_ADD_TIME: u32 = 1184;
    pub const RELOAD_QUICK_ADD_TIME: u32 = 1188;
    pub const RELOAD_QUICK_EMPTY_ADD_TIME: u32 = 1192;
    pub const RELOAD_START_TIME: u32 = 1196;
    pub const RELOAD_START_ADD_TIME: u32 = 1200;
    pub const RELOAD_END_TIME: u32 = 1204;
    pub const DROP_TIME: u32 = 1208;
    pub const RAISE_TIME: u32 = 1212;
    pub const ALT_DROP_TIME: u32 = 1216;
    pub const QUICK_DROP_TIME: u32 = 1220;
    pub const QUICK_RAISE_TIME: u32 = 1224;
    pub const FIRST_RAISE_TIME: u32 = 1228;
    pub const FUSE_TIME: u32 = 1356;
    pub const NO_ADS_WHEN_MAG_EMPTY: u32 = 1373;
    pub const MOVE_SPEED_SCALE: u32 = 1416;
    pub const ADS_MOVE_SPEED_SCALE: u32 = 1420;
    pub const OVERLAY_RETICLE: u32 = 1428;
    pub const OVERLAY_INTERFACE: u32 = 1432;
    pub const OVERLAY_WIDTH: u32 = 1436;
    pub const OVERLAY_HEIGHT: u32 = 1440;
    pub const HIP_SPREAD_STAND_MIN: u32 = 1456;
    pub const HIP_RETICLE_SIDE_POS: u32 = 1504;
    pub const ADS_IDLE_AMOUNT: u32 = 1508;
    pub const HIP_IDLE_AMOUNT: u32 = 1512;
    pub const ADS_IDLE_SPEED: u32 = 1516;
    pub const HIP_IDLE_SPEED: u32 = 1520;
    pub const IDLE_CROUCH_FACTOR: u32 = 1524;
    pub const IDLE_PRONE_FACTOR: u32 = 1528;
    pub const GUN_MAX_PITCH: u32 = 1532;
    pub const GUN_MAX_YAW: u32 = 1536;
    pub const BOLT_ACTION: u32 = 1588;
    pub const AIM_DOWN_SIGHT: u32 = 1592;
    pub const RECHAMBER_WHILE_ADS: u32 = 1593;
    pub const COOK_OFF_HOLD: u32 = 1604;
    pub const CLIP_ONLY: u32 = 1605;
    pub const ADS_FIRE_ONLY: u32 = 1608;
    pub const DUAL_WIELD: u32 = 1615;
    pub const RETRIEVABLE: u32 = 1618;
    /// Listed by `getwatcherweapons`.
    pub const WATCHER: u32 = 1619;
    pub const KILL_ICON: u32 = 1632;
    pub const KILL_ICON_RATIO: u32 = 1636;
    pub const FLIP_KILL_ICON: u32 = 1640;
    pub const NO_PARTIAL_RELOAD: u32 = 1641;
    pub const SEGMENTED_RELOAD: u32 = 1642;
    pub const RELOAD_AMMO_ADD: u32 = 1644;
    pub const RELOAD_START_ADD: u32 = 1648;
    pub const DUAL_WIELD_WEAPON: u32 = 1656;
    pub const IS_TACTICAL_INSERTION: u32 = 1700;
    pub const EXPLOSION_RADIUS: u32 = 1708;
    pub const EXPLOSION_RADIUS_MIN: u32 = 1712;
    pub const EXPLOSION_INNER_DAMAGE: u32 = 1720;
    pub const EXPLOSION_OUTER_DAMAGE: u32 = 1724;
    pub const DAMAGE_CONE_ANGLE: u32 = 1728;
    pub const PROJECTILE_SPEED: u32 = 1732;
    pub const PROJECTILE_SPEED_UP: u32 = 1736;
    pub const PROJECTILE_SPEED_RELATIVE_UP: u32 = 1740;
    pub const PROJECTILE_SPEED_FORWARD: u32 = 1744;
    pub const PROJECTILE_ACTIVATE_DIST: u32 = 1752;
    pub const PROJECTILE_MODEL: u32 = 1768;
    pub const PROJ_EXPLOSION: u32 = 1772;
    pub const PROJ_EXPLOSION_EFFECT: u32 = 1776;
    pub const PROJ_EXPLOSION_EFFECT_FORCE_NORMAL_UP: u32 = 1780;
    pub const PROJ_IMPACT_EXPLODE: u32 = 1836;
    pub const EXPLODE_WHEN_STATIONARY: u32 = 1838;
    pub const STICKINESS: u32 = 1840;
    pub const ROTATE_TYPE: u32 = 1844;
    pub const PLANTABLE: u32 = 1848;
    pub const HAS_DETONATOR: u32 = 1849;
    pub const TIMED_DETONATION: u32 = 1850;
    pub const NO_CRUMPLE_MISSILE: u32 = 1851;
    pub const ROTATE: u32 = 1852;
    pub const KEEP_ROLLING: u32 = 1853;
    pub const HOLD_BUTTON_TO_THROW: u32 = 1854;
    pub const OFFHAND_HOLD_IS_CANCELABLE: u32 = 1855;
    pub const USE_AS_MELEE: u32 = 1872;
    pub const PARALLEL_BOUNCE: u32 = 1880;
    pub const PERPENDICULAR_BOUNCE: u32 = 1884;
    pub const SURF_TYPE_COUNT: usize = 32;
    pub const GUIDED_MISSILE_TYPE: u32 = 1904;
    pub const TIME_TO_ACCELERATE: u32 = 1912;
    pub const PROJECTILE_CURVATURE: u32 = 1916;
    pub const AI_FUSE_TIME: u32 = 1360;
    pub const EXPLODE_ON_GROUND: u32 = 1616;
    pub const IS_ROLLING_GRENADE: u32 = 1688;
    pub const IS_VALUABLE: u32 = 1696;
    pub const DESTABILIZATION_RATE_TIME: u32 = 2288;
    pub const DESTABILIZE_DISTANCE: u32 = 2296;
    pub const ADS_GUN_KICK_REDUCED_KICK_BULLETS: u32 = 1936;
    pub const ADS_GUN_KICK_REDUCED_KICK_PERCENT: u32 = 1940;
    pub const ADS_GUN_KICK_PITCH_MIN: u32 = 1944;
    pub const ADS_GUN_KICK_PITCH_MAX: u32 = 1948;
    pub const ADS_GUN_KICK_YAW_MIN: u32 = 1952;
    pub const ADS_GUN_KICK_YAW_MAX: u32 = 1956;
    pub const ADS_GUN_KICK_ACCEL: u32 = 1960;
    pub const ADS_GUN_KICK_SPEED_MAX: u32 = 1964;
    pub const ADS_GUN_KICK_SPEED_DECAY: u32 = 1968;
    pub const ADS_GUN_KICK_STATIC_DECAY: u32 = 1972;
    pub const ADS_VIEW_KICK_PITCH_MIN: u32 = 1976;
    pub const ADS_VIEW_KICK_PITCH_MAX: u32 = 1980;
    pub const ADS_VIEW_KICK_YAW_MIN: u32 = 1988;
    pub const ADS_VIEW_KICK_YAW_MAX: u32 = 1992;
    pub const ADS_VIEW_SCATTER_MIN: u32 = 2008;
    pub const ADS_VIEW_SCATTER_MAX: u32 = 2012;
    pub const ADS_SPREAD: u32 = 2016;
    pub const HIP_GUN_KICK_REDUCED_KICK_BULLETS: u32 = 2020;
    pub const HIP_GUN_KICK_REDUCED_KICK_PERCENT: u32 = 2024;
    pub const HIP_GUN_KICK_PITCH_MIN: u32 = 2028;
    pub const HIP_GUN_KICK_PITCH_MAX: u32 = 2032;
    pub const HIP_GUN_KICK_YAW_MIN: u32 = 2036;
    pub const HIP_GUN_KICK_YAW_MAX: u32 = 2040;
    pub const HIP_GUN_KICK_ACCEL: u32 = 2044;
    pub const HIP_GUN_KICK_SPEED_MAX: u32 = 2048;
    pub const HIP_GUN_KICK_SPEED_DECAY: u32 = 2052;
    pub const HIP_GUN_KICK_STATIC_DECAY: u32 = 2056;
    pub const HIP_VIEW_KICK_PITCH_MIN: u32 = 2060;
    pub const HIP_VIEW_KICK_PITCH_MAX: u32 = 2064;
    pub const HIP_VIEW_KICK_YAW_MIN: u32 = 2072;
    pub const HIP_VIEW_KICK_YAW_MAX: u32 = 2076;
    pub const HIP_VIEW_SCATTER_MIN: u32 = 2080;
    pub const HIP_VIEW_SCATTER_MAX: u32 = 2084;
    pub const LOCATION_DAMAGE_MULTIPLIERS: u32 = 2300;
    pub const HIT_LOCATIONS: usize = 21;
    /// A turret's pitch and yaw convergence times (seconds).
    pub const TURRET_CONVERGENCE: [u32; 2] = [2212, 2216];
    /// A turret's arcs (degrees): left, right, top, bottom.
    pub const TURRET_ARCS: [u32; 4] = [2168, 2172, 2176, 2180];
    /// A turret's spread (degrees) for an AI's and a player's shots.
    pub const TURRET_SPREAD: [u32; 2] = [2188, 2192];
    /// A turret's turn rates (degrees a second): least pitch, least yaw,
    /// most pitch, most yaw.
    pub const TURRET_TURN_RATES: [u32; 4] = [2196, 2200, 2204, 2208];
    pub const TURRET_MAX_RANGE: u32 = 2224;
    /// A sentry's scan: speed (degrees a second), acceleration, and the
    /// pause at each end (ms).
    pub const TURRET_SCAN_SPEED: u32 = 2344;
    pub const TURRET_SCAN_ACCEL: u32 = 2348;
    pub const TURRET_SCAN_PAUSE: u32 = 2352;
    /// How a shot uses the barrels: shots per fire and barrel order.
    pub const FIRE_BARRELS: u32 = 56;
    /// Bullet shots: rifle (else pistol) means of death, which also passes
    /// through bodies; bouncing off surfaces; exploding on impact.
    pub const RIFLE_BULLET: u32 = 1585;
    /// Its bullets' hits carry damage flag 2.
    pub const ARMOR_PIERCING: u32 = 1586;
    pub const BULLET_BOUNCE: u32 = 1623;
    pub const BULLET_IMPACT_EXPLODE: u32 = 1839;
    pub const TRACER_TYPE: u32 = 2320;
    pub const SIZE: usize = 2448;
}

pub mod weap_anim {
    pub const IDLE: usize = 1;
    pub const EMPTY_IDLE: usize = 2;
    pub const FIRE_INTRO: usize = 3;
    pub const FIRE: usize = 4;
    pub const HOLD_FIRE: usize = 5;
    pub const LASTSHOT: usize = 6;
    pub const RECHAMBER: usize = 8;
    pub const MELEE: usize = 9;
    pub const MELEE_CHARGE: usize = 14;
    pub const RELOAD: usize = 16;
    pub const RELOAD_EMPTY: usize = 18;
    pub const RELOAD_START: usize = 19;
    pub const RELOAD_END: usize = 20;
    pub const RELOAD_QUICK: usize = 21;
    pub const RELOAD_QUICK_EMPTY: usize = 22;
    pub const RAISE: usize = 23;
    pub const FIRST_RAISE: usize = 24;
    pub const DROP: usize = 25;
    pub const ALT_RAISE: usize = 26;
    pub const ALT_DROP: usize = 27;
    pub const QUICK_RAISE: usize = 28;
    pub const QUICK_DROP: usize = 29;
    pub const EMPTY_RAISE: usize = 30;
    pub const EMPTY_DROP: usize = 31;
    pub const SPRINT_IN: usize = 32;
    pub const SPRINT_LOOP: usize = 33;
    pub const SPRINT_OUT: usize = 34;
    pub const DETONATE: usize = 58;
    pub const ADS_FIRE: usize = 61;
    pub const ADS_LASTSHOT: usize = 62;
    pub const ADS_RECHAMBER: usize = 64;
    pub const DW_LEFT_FIRE: usize = 78;
    pub const DW_LEFT_LASTSHOT: usize = 79;
    pub const DW_LEFT_IDLE: usize = 81;
    pub const DW_LEFT_EMPTY_IDLE: usize = 82;
    pub const DW_LEFT_RELOAD_EMPTY: usize = 83;
    pub const DW_LEFT_RELOAD: usize = 84;
    pub const ADS_UP: usize = 85;
    pub const ADS_DOWN: usize = 86;
}

pub mod unique {
    pub const CAMOUFLAGE: u32 = 164;
    pub const NAME: u32 = 0;
    pub const TYPE: u32 = 4;
    pub const COMBINED_MASK: u32 = 16;
    pub const ALT_WEAPON_NAME: u32 = 20;
    pub const DUAL_WIELD_WEAPON_NAME: u32 = 28;
    pub const HIDE_TAGS: u32 = 36;
    pub const HIDE_TAG_COUNT: u32 = 32;
    pub const VIEW_MODEL: u32 = 40;
    pub const VIEW_MODEL_ADDITIONAL: u32 = 44;
    pub const VIEW_MODEL_ADS: u32 = 48;
    pub const WORLD_MODEL: u32 = 52;
    pub const WORLD_MODEL_ADDITIONAL: u32 = 56;
    pub const VIEW_MODEL_TAG: u32 = 60;
    pub const WORLD_MODEL_TAG: u32 = 64;
    pub const VIEW_MODEL_OFFSETS: u32 = 68;
    pub const WORLD_MODEL_OFFSETS: u32 = 80;
    pub const VIEW_MODEL_ROTATIONS: u32 = 92;
    pub const WORLD_MODEL_ROTATIONS: u32 = 104;
    pub const VIEW_MODEL_ADD_OFFSETS: u32 = 116;
    pub const WORLD_MODEL_ADD_OFFSETS: u32 = 128;
    pub const VIEW_MODEL_ADD_ROTATIONS: u32 = 140;
    pub const WORLD_MODEL_ADD_ROTATIONS: u32 = 152;
    pub const DISABLE_BASE_ATTACHMENT: u32 = 168;
    pub const DISABLE_BASE_CLIP: u32 = 169;
    pub const OVERLAY_MATERIAL: u32 = 196;
    pub const XANIMS: u32 = 232;
    pub const FIRE_SOUND: u32 = 256;
    pub const FIRE_SOUND_PLAYER: u32 = 260;
}

pub mod attachment {
    pub const NAME: u32 = 0;
    pub const TYPE: u32 = 8;
    pub const SILENCED: u32 = 44;
    pub const DUAL_MAG: u32 = 45;
    pub const LASER_SIGHT: u32 = 46;
    pub const SHARED_AMMO: u32 = 50;
    pub const DAMAGE_RANGE_SCALE: u32 = 52;
    pub const ADS_ZOOM_FOV: u32 = 56;
    pub const ADS_ZOOM_FOV2: u32 = 60;
    pub const ADS_ZOOM_FOV3: u32 = 64;
    pub const ADS_ZOOM_IN_FRAC: u32 = 68;
    pub const ADS_ZOOM_OUT_FRAC: u32 = 72;
    pub const ADS_TRANS_IN_TIME_SCALE: u32 = 76;
    pub const ADS_TRANS_OUT_TIME_SCALE: u32 = 80;
    pub const ADS_VIEW_KICK_CENTER_SPEED_SCALE: u32 = 92;
    pub const ADS_IDLE_AMOUNT_SCALE: u32 = 96;
    pub const ADS_MOVE_SPEED_SCALE: u32 = 156;
    pub const HIP_SPREAD_MIN_SCALE: u32 = 160;
    pub const HIP_SPREAD_MAX_SCALE: u32 = 164;
    pub const FIRE_TIME_SCALE: u32 = 188;
    pub const RELOAD_TIME_SCALES: u32 = 192;
    pub const CLIP_SIZE_SCALE: u32 = 232;
    pub const PERKS: u32 = 252;
}

#[derive(Clone, Copy)]
pub struct AttachmentView<'z> {
    load: &'z ZoneLoad,
    asset: &'z LoadedAsset,
}

impl<'z> AttachmentView<'z> {
    pub fn u32_at(&self, off: u32) -> u32 {
        let o = off as usize;
        self.asset
            .header
            .get(o..o + 4)
            .map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()))
    }

    pub fn f32_at(&self, off: u32) -> f32 {
        f32::from_bits(self.u32_at(off))
    }

    pub fn flag(&self, off: u32) -> bool {
        self.asset.header.get(off as usize).is_some_and(|&b| b != 0)
    }

    pub fn name(&self) -> Option<&'z str> {
        let p = crate::walk::decode_ptr(self.u32_at(attachment::NAME))?;
        self.load
            .blocks
            .cstr(p)
            .ok()
            .and_then(|b| core::str::from_utf8(b).ok())
    }

    pub fn attachment_type(&self) -> u32 {
        self.u32_at(attachment::TYPE)
    }
}

pub type UniqueModel<'z> = (&'z str, Option<&'z str>, [f32; 3], [f32; 3]);

#[derive(Clone, Copy)]
pub struct AttachmentUniqueView<'z> {
    load: &'z ZoneLoad,
    asset: &'z LoadedAsset,
}

impl<'z> AttachmentUniqueView<'z> {
    fn u32_at(&self, off: u32) -> u32 {
        let o = off as usize;
        self.asset
            .header
            .get(o..o + 4)
            .map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()))
    }

    fn vec3(&self, off: u32) -> [f32; 3] {
        core::array::from_fn(|i| f32::from_bits(self.u32_at(off + 4 * i as u32)))
    }

    fn str_at(&self, off: u32) -> Option<&'z str> {
        let p = crate::walk::decode_ptr(self.u32_at(off))?;
        self.load
            .blocks
            .cstr(p)
            .ok()
            .and_then(|b| core::str::from_utf8(b).ok())
            .filter(|name| !name.is_empty())
    }

    pub fn name(&self) -> Option<&'z str> {
        self.str_at(unique::NAME)
    }

    pub fn attachment_type(&self) -> u32 {
        self.u32_at(unique::TYPE)
    }

    pub fn combined_mask(&self) -> u32 {
        self.u32_at(unique::COMBINED_MASK)
    }

    pub fn alt_weapon(&self) -> Option<&'z str> {
        self.str_at(unique::ALT_WEAPON_NAME)
    }

    pub fn dual_wield_weapon(&self) -> Option<&'z str> {
        self.str_at(unique::DUAL_WIELD_WEAPON_NAME)
    }

    pub fn flag(&self, off: u32) -> bool {
        self.asset.header.get(off as usize).is_some_and(|&b| b != 0)
    }

    pub fn sound(&self, off: u32) -> Option<&'z str> {
        self.str_at(off)
    }

    fn model_name(&self, off: u32) -> Option<&'z str> {
        let model = &self.load.assets[self.asset.field(off)?];
        let p =
            crate::walk::decode_ptr(u32::from_le_bytes(model.header.get(0..4)?.try_into().ok()?))?;
        self.load
            .blocks
            .cstr(p)
            .ok()
            .and_then(|b| core::str::from_utf8(b).ok())
    }

    pub fn models(&self, view: bool) -> [Option<UniqueModel<'z>>; 2] {
        use unique as u;
        let (model, add, tag, offsets, rotations, add_offsets, add_rotations) = if view {
            (
                u::VIEW_MODEL,
                u::VIEW_MODEL_ADDITIONAL,
                u::VIEW_MODEL_TAG,
                u::VIEW_MODEL_OFFSETS,
                u::VIEW_MODEL_ROTATIONS,
                u::VIEW_MODEL_ADD_OFFSETS,
                u::VIEW_MODEL_ADD_ROTATIONS,
            )
        } else {
            (
                u::WORLD_MODEL,
                u::WORLD_MODEL_ADDITIONAL,
                u::WORLD_MODEL_TAG,
                u::WORLD_MODEL_OFFSETS,
                u::WORLD_MODEL_ROTATIONS,
                u::WORLD_MODEL_ADD_OFFSETS,
                u::WORLD_MODEL_ADD_ROTATIONS,
            )
        };
        let tag = self.str_at(tag);
        [
            (model, offsets, rotations),
            (add, add_offsets, add_rotations),
        ]
        .map(|(model, offsets, rotations)| {
            Some((
                self.model_name(model)?,
                tag,
                self.vec3(offsets),
                self.vec3(rotations),
            ))
        })
    }

    pub fn camouflage(&self) -> Option<crate::weapon_camo::WeaponCamoView<'z>> {
        let asset = self
            .load
            .assets
            .get(self.asset.field(unique::CAMOUFLAGE)?)?;
        crate::weapon_camo::WeaponCamoView::new(self.load, asset).ok()
    }

    pub fn asset_field_name(&self, off: u32) -> Option<&'z str> {
        self.model_name(off).filter(|name| !name.is_empty())
    }

    pub fn ads_model(&self) -> Option<UniqueModel<'z>> {
        use unique as u;
        Some((
            self.model_name(u::VIEW_MODEL_ADS)?,
            self.str_at(u::VIEW_MODEL_TAG),
            self.vec3(u::VIEW_MODEL_OFFSETS),
            self.vec3(u::VIEW_MODEL_ROTATIONS),
        ))
    }

    pub fn xanim(&self, slot: u32) -> Option<&'z str> {
        let arr = crate::walk::decode_ptr(self.u32_at(unique::XANIMS))?;
        let p = self.load.blocks.ptr_at(arr.at(4 * slot)).ok()??;
        self.load
            .blocks
            .cstr(p)
            .ok()
            .and_then(|b| core::str::from_utf8(b).ok())
            .filter(|name| !name.is_empty())
    }

    pub fn hide_tags(&self) -> impl Iterator<Item = &'z str> + 'z {
        let load = self.load;
        let arr = crate::walk::decode_ptr(self.u32_at(unique::HIDE_TAGS));
        (0..unique::HIDE_TAG_COUNT).filter_map(move |i| {
            let b = load.blocks.bytes(arr?.at(2 * i), 2).ok()?;
            let id = u16::from_le_bytes([b[0], b[1]]);
            (id != 0).then(|| load.script_string(id)).flatten()
        })
    }
}

pub mod weap_type {
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

pub mod weap_class {
    pub const RIFLE: i32 = 0;
    pub const MG: i32 = 1;
    pub const SMG: i32 = 2;
    pub const SPREAD: i32 = 3;
    pub const PISTOL: i32 = 4;
    pub const GRENADE: i32 = 5;
    pub const ROCKETLAUNCHER: i32 = 6;
    pub const TURRET: i32 = 7;
    pub const NON_PLAYER: i32 = 8;
    pub const GAS: i32 = 9;
    pub const ITEM: i32 = 10;
    pub const MELEE: i32 = 11;
    pub const KILLSTREAK_ALT_STORED_WEAPON: i32 = 12;
    pub const PISTOL_SPREAD: i32 = 13;
}

pub mod inventory_type {
    pub const PRIMARY: i32 = 0;
    pub const OFFHAND: i32 = 1;
    pub const ITEM: i32 = 2;
    pub const ALTMODE: i32 = 3;
    pub const MELEE: i32 = 4;
    pub const DWLEFTHAND: i32 = 5;
}

pub mod offhand_slot {
    pub const NONE: i32 = 0;
    pub const LETHAL_GRENADE: i32 = 1;
    pub const TACTICAL_GRENADE: i32 = 2;
    pub const EQUIPMENT: i32 = 3;
    pub const SPECIFIC_USE: i32 = 4;
}

#[derive(Clone, Copy)]
pub struct WeaponView<'z> {
    load: &'z ZoneLoad,
    asset: &'z LoadedAsset,
    variant: &'z [u8],
    def: Option<Ptr>,
}

impl ZoneLoad {
    /// Each weapon asset of the zone, as a view when its header is whole.
    pub fn weapons(&self) -> impl Iterator<Item = Option<WeaponView<'_>>> {
        self.assets
            .iter()
            .filter(|asset| asset.ty == crate::AssetType::Weapon)
            .map(|asset| WeaponView::new(self, asset))
    }
}

impl<'z> WeaponView<'z> {
    pub fn new(load: &'z ZoneLoad, asset: &'z LoadedAsset) -> Option<Self> {
        if asset.ty != crate::AssetType::Weapon || asset.header.len() < variant::SIZE {
            return None;
        }
        let mut view = Self {
            load,
            asset,
            variant: &asset.header,
            def: None,
        };
        view.def = view.variant_ptr(variant::WEAP_DEF);
        Some(view)
    }

    fn variant_u32(&self, off: u32) -> u32 {
        let o = off as usize;
        u32::from_le_bytes(self.variant[o..o + 4].try_into().unwrap())
    }

    fn variant_ptr(&self, off: u32) -> Option<Ptr> {
        crate::walk::decode_ptr(self.variant_u32(off))
    }

    pub fn attachments(&self) -> impl Iterator<Item = AttachmentView<'z>> + 'z {
        let load = self.load;
        let arr = self.variant_ptr(variant::ATTACHMENTS);
        (0..variant::ATTACHMENT_COUNT).filter_map(move |i| {
            let asset = load.asset_at(arr?.at(4 * i))?;
            (asset.ty == crate::AssetType::Attachment).then_some(AttachmentView { load, asset })
        })
    }

    pub fn attachment_uniques(&self) -> impl Iterator<Item = AttachmentUniqueView<'z>> + 'z {
        let load = self.load;
        let arr = self.variant_ptr(variant::ATTACHMENT_UNIQUES);
        (0..variant::ATTACHMENT_UNIQUE_COUNT).filter_map(move |i| {
            let asset = load.asset_at(arr?.at(4 * i))?;
            (asset.ty == crate::AssetType::AttachmentUnique)
                .then_some(AttachmentUniqueView { load, asset })
        })
    }

    pub fn variant_asset_name(&self, off: u32) -> Option<&'z str> {
        let asset = &self.load.assets[self.asset.field(off)?];
        let p =
            crate::walk::decode_ptr(u32::from_le_bytes(asset.header.get(0..4)?.try_into().ok()?))?;
        self.load
            .blocks
            .cstr(p)
            .ok()
            .and_then(|b| core::str::from_utf8(b).ok())
            .filter(|name| !name.is_empty())
    }

    pub fn has_def(&self) -> bool {
        self.def.is_some()
    }

    pub fn variant_i32(&self, off: u32) -> i32 {
        self.variant_u32(off) as i32
    }

    pub fn variant_f32(&self, off: u32) -> f32 {
        f32::from_bits(self.variant_u32(off))
    }

    pub fn variant_bool(&self, off: u32) -> bool {
        self.variant[off as usize] != 0
    }

    pub fn variant_str(&self, off: u32) -> Option<&'z str> {
        let p = self.variant_ptr(off)?;
        self.load
            .blocks
            .cstr(p)
            .ok()
            .and_then(|b| core::str::from_utf8(b).ok())
    }

    fn def_bytes(&self, off: u32, len: usize) -> Option<&'z [u8]> {
        self.load.blocks.bytes(self.def?.at(off), len).ok()
    }

    pub fn def_i32(&self, off: u32) -> i32 {
        self.def_bytes(off, 4)
            .map_or(0, |b| i32::from_le_bytes(b.try_into().unwrap()))
    }

    pub fn def_f32(&self, off: u32) -> f32 {
        self.def_bytes(off, 4)
            .map_or(0.0, |b| f32::from_le_bytes(b.try_into().unwrap()))
    }

    pub fn def_bool(&self, off: u32) -> bool {
        self.def_bytes(off, 1).is_some_and(|b| b[0] != 0)
    }

    pub fn def_str(&self, off: u32) -> Option<&'z str> {
        let p = self.load.blocks.ptr_at(self.def?.at(off)).ok()??;
        self.load
            .blocks
            .cstr(p)
            .ok()
            .and_then(|b| core::str::from_utf8(b).ok())
    }

    pub fn def_f32_array<const N: usize>(&self, off: u32) -> Option<[f32; N]> {
        let p = self.load.blocks.ptr_at(self.def?.at(off)).ok()??;
        let bytes = self.load.blocks.bytes(p, 4 * N).ok()?;
        Some(core::array::from_fn(|i| {
            f32::from_le_bytes(bytes[4 * i..4 * i + 4].try_into().unwrap())
        }))
    }

    pub fn name(&self) -> Option<&'z str> {
        self.variant_str(variant::INTERNAL_NAME)
    }

    pub fn camouflage(&self) -> Option<crate::weapon_camo::WeaponCamoView<'z>> {
        let asset = self
            .load
            .asset_in(self.asset, self.def?.at(def::CAMOUFLAGE))?;
        crate::weapon_camo::WeaponCamoView::new(self.load, asset).ok()
    }

    pub fn def_loaded_asset_name(&self, off: u32) -> Option<&'z str> {
        let asset = self.load.asset_in(self.asset, self.def?.at(off))?;
        let p =
            crate::walk::decode_ptr(u32::from_le_bytes(asset.header.get(0..4)?.try_into().ok()?))?;
        self.load
            .blocks
            .cstr(p)
            .ok()
            .and_then(|b| core::str::from_utf8(b).ok())
            .filter(|name| !name.is_empty())
    }

    pub fn def_asset_name(&self, off: u32) -> Option<&'z str> {
        asset_name(self.load, self.def?.at(off))
    }

    pub fn def_asset_array_name(&self, off: u32, index: u32) -> Option<&'z str> {
        let arr = self.load.blocks.ptr_at(self.def?.at(off)).ok()??;
        asset_name(self.load, arr.at(4 * index))
    }

    pub fn attached_model(&self, slot: u32, view: bool) -> Option<(&'z str, [f32; 3], [f32; 3])> {
        use variant as v;
        let (models, offsets, rotations) = if view {
            (
                v::ATTACH_VIEW_MODEL,
                v::ATTACH_VIEW_MODEL_OFFSETS,
                v::ATTACH_VIEW_MODEL_ROTATIONS,
            )
        } else {
            (
                v::ATTACH_WORLD_MODEL,
                v::ATTACH_WORLD_MODEL_OFFSETS,
                v::ATTACH_WORLD_MODEL_ROTATIONS,
            )
        };
        if slot >= v::ATTACH_MODEL_COUNT {
            return None;
        }
        let name = asset_name(self.load, self.variant_ptr(models)?.at(4 * slot))?;
        let vec3 =
            |base: u32| core::array::from_fn(|k| self.variant_f32(base + 12 * slot + 4 * k as u32));
        Some((name, vec3(offsets), vec3(rotations)))
    }

    pub fn xanim(&self, index: u32) -> Option<&'z str> {
        let arr = self.variant_ptr(variant::XANIMS)?;
        let p = self.load.blocks.ptr_at(arr.at(4 * index)).ok()??;
        self.load
            .blocks
            .cstr(p)
            .ok()
            .and_then(|b| core::str::from_utf8(b).ok())
    }

    pub fn damage_curve(&self) -> [(i32, f32); def::DAMAGE_STEPS] {
        core::array::from_fn(|i| {
            (
                self.def_i32(def::DAMAGE + 4 * i as u32),
                self.def_f32(def::DAMAGE_RANGE + 4 * i as u32),
            )
        })
    }

    pub fn raw_bytes(&self) -> Option<(&'z [u8], &'z [u8])> {
        Some((
            self.def_bytes(0, def::SIZE)?,
            self.variant.get(..variant::SIZE)?,
        ))
    }
}

/// A Black Ops 2 weapon definition and variant read from their captured bytes.
#[derive(Clone, Copy, Debug)]
pub struct WeaponDefView<'a> {
    def: &'a [u8],
    variant: &'a [u8],
}

impl<'a> WeaponDefView<'a> {
    pub fn new(def: &'a [u8], variant: &'a [u8]) -> Option<Self> {
        (def.len() >= def::SIZE && variant.len() >= variant::SIZE).then_some(Self { def, variant })
    }

    pub fn same_def(&self, other: &WeaponDefView<'_>) -> bool {
        self.def[..def::SIZE] == other.def[..def::SIZE]
    }

    pub fn def_i32(&self, off: u32) -> i32 {
        let o = off as usize;
        i32::from_le_bytes(self.def[o..o + 4].try_into().unwrap())
    }

    pub fn def_f32(&self, off: u32) -> f32 {
        let o = off as usize;
        f32::from_le_bytes(self.def[o..o + 4].try_into().unwrap())
    }

    pub fn def_bool(&self, off: u32) -> bool {
        self.def[off as usize] != 0
    }

    pub fn variant_i32(&self, off: u32) -> i32 {
        let o = off as usize;
        i32::from_le_bytes(self.variant[o..o + 4].try_into().unwrap())
    }

    pub fn variant_f32(&self, off: u32) -> f32 {
        let o = off as usize;
        f32::from_le_bytes(self.variant[o..o + 4].try_into().unwrap())
    }

    pub fn variant_bool(&self, off: u32) -> bool {
        self.variant[off as usize] != 0
    }

    pub fn weapon_type(&self) -> i32 {
        self.def_i32(0x1c)
    }

    pub fn player_anim_type(&self) -> i32 {
        self.def_i32(0x18)
    }

    pub fn weapon_class(&self) -> i32 {
        self.def_i32(0x20)
    }

    pub fn penetrate_type(&self) -> i32 {
        self.def_i32(0x24)
    }

    pub fn inventory_type(&self) -> i32 {
        self.def_i32(0x2c)
    }

    pub fn fire_type(&self) -> i32 {
        self.def_i32(0x30)
    }

    pub fn overheat_weapon(&self) -> i32 {
        self.def_i32(0x48)
    }

    pub fn overheat_rate(&self) -> f32 {
        self.def_f32(0x4c)
    }

    pub fn cooldown_rate(&self) -> f32 {
        self.def_f32(0x50)
    }

    pub fn overheat_end_val(&self) -> f32 {
        self.def_f32(0x54)
    }

    pub fn cool_while_firing(&self) -> bool {
        self.def_bool(0x58)
    }

    pub fn fuel_tank_weapon(&self) -> bool {
        self.def_bool(0x59)
    }

    pub fn tank_life_time(&self) -> i32 {
        self.def_i32(0x5c)
    }

    pub fn offhand_class(&self) -> i32 {
        self.def_i32(0x60)
    }

    pub fn offhand_slot(&self) -> i32 {
        self.def_i32(0x64)
    }

    pub fn ducked_sprint_scale(&self) -> f32 {
        self.def_f32(0x278)
    }

    pub fn sprint_scale(&self) -> f32 {
        self.def_f32(0x29c)
    }

    pub fn dtp_scale(&self) -> f32 {
        self.def_f32(0x2f0)
    }

    pub fn shot_count(&self) -> i32 {
        self.def_i32(0x3d0)
    }

    pub fn unlimited_ammo(&self) -> bool {
        self.def_bool(0x3e0)
    }

    pub fn damage(&self) -> i32 {
        self.def_i32(0x3e4)
    }

    pub fn min_damage(&self) -> i32 {
        self.def_i32(0x3f8)
    }

    pub fn max_damage_range(&self) -> f32 {
        self.def_f32(0x3fc)
    }

    pub fn min_damage_range(&self) -> f32 {
        self.def_f32(0x410)
    }

    pub fn min_player_damage(&self) -> i32 {
        self.def_i32(0x414)
    }

    pub fn player_damage(&self) -> i32 {
        self.def_i32(0x420)
    }

    pub fn melee_damage(&self) -> i32 {
        self.def_i32(0x424)
    }

    pub fn fire_delay(&self) -> i32 {
        self.def_i32(0x430)
    }

    pub fn melee_delay(&self) -> i32 {
        self.def_i32(0x434)
    }

    pub fn melee_charge_delay(&self) -> i32 {
        self.def_i32(0x438)
    }

    pub fn detonate_delay(&self) -> i32 {
        self.def_i32(0x43c)
    }

    pub fn spin_up_time(&self) -> i32 {
        self.def_i32(0x440)
    }

    pub fn spin_down_time(&self) -> i32 {
        self.def_i32(0x444)
    }

    pub fn fire_time(&self) -> i32 {
        self.def_i32(0x468)
    }

    pub fn last_fire_time(&self) -> i32 {
        self.def_i32(0x46c)
    }

    pub fn rechamber_time(&self) -> i32 {
        self.def_i32(0x470)
    }

    pub fn rechamber_bolt_time(&self) -> i32 {
        self.def_i32(0x474)
    }

    pub fn hold_fire_time(&self) -> i32 {
        self.def_i32(0x478)
    }

    pub fn detonate_time(&self) -> i32 {
        self.def_i32(0x47c)
    }

    pub fn melee_time(&self) -> i32 {
        self.def_i32(0x480)
    }

    pub fn burst_delay_time(&self) -> i32 {
        self.def_i32(0x484)
    }

    pub fn melee_charge_time(&self) -> i32 {
        self.def_i32(0x488)
    }

    pub fn reload_add_time(&self) -> i32 {
        self.def_i32(0x49c)
    }

    pub fn reload_empty_add_time(&self) -> i32 {
        self.def_i32(0x4a0)
    }

    pub fn reload_quick_add_time(&self) -> i32 {
        self.def_i32(0x4a4)
    }

    pub fn reload_quick_empty_add_time(&self) -> i32 {
        self.def_i32(0x4a8)
    }

    pub fn reload_start_time(&self) -> i32 {
        self.def_i32(0x4ac)
    }

    pub fn reload_start_add_time(&self) -> i32 {
        self.def_i32(0x4b0)
    }

    pub fn reload_end_time(&self) -> i32 {
        self.def_i32(0x4b4)
    }

    pub fn drop_time(&self) -> i32 {
        self.def_i32(0x4b8)
    }

    pub fn raise_time(&self) -> i32 {
        self.def_i32(0x4bc)
    }

    pub fn alt_drop_time(&self) -> i32 {
        self.def_i32(0x4c0)
    }

    pub fn quick_drop_time(&self) -> i32 {
        self.def_i32(0x4c4)
    }

    pub fn quick_raise_time(&self) -> i32 {
        self.def_i32(0x4c8)
    }

    pub fn first_raise_time(&self) -> i32 {
        self.def_i32(0x4cc)
    }

    pub fn empty_raise_time(&self) -> i32 {
        self.def_i32(0x4d0)
    }

    pub fn empty_drop_time(&self) -> i32 {
        self.def_i32(0x4d4)
    }

    pub fn sprint_in_time(&self) -> i32 {
        self.def_i32(0x4d8)
    }

    pub fn sprint_out_time(&self) -> i32 {
        self.def_i32(0x4e0)
    }

    pub fn low_ready_in_time(&self) -> i32 {
        self.def_i32(0x4e4)
    }

    pub fn low_ready_out_time(&self) -> i32 {
        self.def_i32(0x4ec)
    }

    pub fn cont_fire_in_time(&self) -> i32 {
        self.def_i32(0x4f0)
    }

    pub fn cont_fire_out_time(&self) -> i32 {
        self.def_i32(0x4f8)
    }

    pub fn dtp_in_time(&self) -> i32 {
        self.def_i32(0x4fc)
    }

    pub fn dtp_out_time(&self) -> i32 {
        self.def_i32(0x504)
    }

    pub fn crawl_in_time(&self) -> i32 {
        self.def_i32(0x508)
    }

    pub fn crawl_out_fire_time(&self) -> i32 {
        self.def_i32(0x51c)
    }

    pub fn crawl_out_time(&self) -> i32 {
        self.def_i32(0x520)
    }

    pub fn slide_in_time(&self) -> i32 {
        self.def_i32(0x524)
    }

    pub fn deploy_time(&self) -> i32 {
        self.def_i32(0x528)
    }

    pub fn breakdown_time(&self) -> i32 {
        self.def_i32(0x52c)
    }

    pub fn night_vision_wear_time(&self) -> i32 {
        self.def_i32(0x534)
    }

    pub fn night_vision_remove_time(&self) -> i32 {
        self.def_i32(0x540)
    }

    pub fn fuse_time(&self) -> i32 {
        self.def_i32(0x54c)
    }

    pub fn ai_fuse_time(&self) -> i32 {
        self.def_i32(0x550)
    }

    pub fn require_lockon_to_fire(&self) -> bool {
        self.def_bool(0x55c)
    }

    pub fn no_ads_when_mag_empty(&self) -> bool {
        self.def_bool(0x55d)
    }

    pub fn stack_fire(&self) -> i32 {
        self.def_i32(0x560)
    }

    pub fn stack_fire_spread(&self) -> f32 {
        self.def_f32(0x564)
    }

    pub fn mountable_weapon(&self) -> bool {
        self.def_bool(0x578)
    }

    pub fn move_speed_scale(&self) -> f32 {
        self.def_f32(0x588)
    }

    pub fn ads_move_speed_scale(&self) -> f32 {
        self.def_f32(0x58c)
    }

    pub fn sprint_duration_scale(&self) -> f32 {
        self.def_f32(0x590)
    }

    pub fn ads_overlay_reticle(&self) -> i32 {
        self.def_i32(0x594)
    }

    pub fn hip_spread_stand_min(&self) -> f32 {
        self.def_f32(0x5b0)
    }

    pub fn hold_breath_to_steady(&self) -> bool {
        self.def_bool(0x5ac)
    }

    pub fn hip_spread_ducked_min(&self) -> f32 {
        self.def_f32(0x5b4)
    }

    pub fn hip_spread_prone_min(&self) -> f32 {
        self.def_f32(0x5b8)
    }

    pub fn hip_spread_max(&self) -> f32 {
        self.def_f32(0x5bc)
    }

    pub fn hip_spread_ducked_max(&self) -> f32 {
        self.def_f32(0x5c0)
    }

    pub fn hip_spread_prone_max(&self) -> f32 {
        self.def_f32(0x5c4)
    }

    pub fn hip_spread_decay_rate(&self) -> f32 {
        self.def_f32(0x5c8)
    }

    pub fn hip_spread_fire_add(&self) -> f32 {
        self.def_f32(0x5cc)
    }

    pub fn hip_spread_turn_add(&self) -> f32 {
        self.def_f32(0x5d0)
    }

    pub fn hip_spread_move_add(&self) -> f32 {
        self.def_f32(0x5d4)
    }

    pub fn hip_spread_ducked_decay(&self) -> f32 {
        self.def_f32(0x5d8)
    }

    pub fn hip_spread_prone_decay(&self) -> f32 {
        self.def_f32(0x5dc)
    }

    pub fn ads_idle_speed(&self) -> f32 {
        self.def_f32(0x5ec)
    }

    pub fn hip_idle_speed(&self) -> f32 {
        self.def_f32(0x5f0)
    }

    pub fn rifle_bullet(&self) -> bool {
        self.def_bool(0x631)
    }

    pub fn armor_piercing(&self) -> bool {
        self.def_bool(0x632)
    }

    pub fn airburst_weapon(&self) -> bool {
        self.def_bool(0x633)
    }

    pub fn bolt_action(&self) -> bool {
        self.def_bool(0x634)
    }

    pub fn aim_down_sight(&self) -> bool {
        self.def_bool(0x638)
    }

    pub fn rechamber_while_ads(&self) -> bool {
        self.def_bool(0x639)
    }

    pub fn reload_while_ads(&self) -> bool {
        self.def_bool(0x63a)
    }

    pub fn cook_off_hold(&self) -> bool {
        self.def_bool(0x644)
    }

    pub fn clip_only(&self) -> bool {
        self.def_bool(0x645)
    }

    pub fn can_use_in_vehicle(&self) -> bool {
        self.def_bool(0x646)
    }

    pub fn no_drops_or_raises(&self) -> bool {
        self.def_bool(0x647)
    }

    pub fn ads_fire(&self) -> bool {
        self.def_bool(0x648)
    }

    pub fn bayonet(&self) -> bool {
        self.def_bool(0x64e)
    }

    pub fn dual_wield(&self) -> bool {
        self.def_bool(0x64f)
    }

    pub fn explode_on_ground(&self) -> bool {
        self.def_bool(0x650)
    }

    pub fn retrievable(&self) -> bool {
        self.def_bool(0x652)
    }

    pub fn continuous_fire(&self) -> bool {
        self.def_bool(0x655)
    }

    pub fn no_quick_drop_when_empty(&self) -> bool {
        self.def_bool(0x659)
    }

    pub fn alt_weapon_ads_only(&self) -> bool {
        self.def_bool(0x65c)
    }

    pub fn alt_weapon_disable_switching(&self) -> bool {
        self.def_bool(0x65d)
    }

    pub fn no_partial_reload(&self) -> bool {
        self.def_bool(0x669)
    }

    pub fn segmented_reload(&self) -> bool {
        self.def_bool(0x66a)
    }

    pub fn no_ads_auto_reload(&self) -> bool {
        self.def_bool(0x66b)
    }

    pub fn reload_ammo_add(&self) -> i32 {
        self.def_i32(0x66c)
    }

    pub fn reload_start_add(&self) -> i32 {
        self.def_i32(0x670)
    }

    pub fn shots_before_rechamber(&self) -> i32 {
        self.def_i32(0x690)
    }

    pub fn blocks_prone(&self) -> bool {
        self.def_bool(0x694)
    }

    pub fn is_rolling_grenade(&self) -> i32 {
        self.def_i32(0x698)
    }

    pub fn is_tactical_insertion(&self) -> bool {
        self.def_i32(0x6a4) != 0
    }

    pub fn explosion_radius(&self) -> i32 {
        self.def_i32(0x6ac)
    }

    pub fn explosion_radius_min(&self) -> i32 {
        self.def_i32(0x6b0)
    }

    pub fn explosion_inner_damage(&self) -> i32 {
        self.def_i32(0x6b8)
    }

    pub fn explosion_outer_damage(&self) -> i32 {
        self.def_i32(0x6bc)
    }

    pub fn damage_cone_angle(&self) -> f32 {
        self.def_f32(0x6c0)
    }

    pub fn projectile_speed(&self) -> i32 {
        self.def_i32(0x6c4)
    }

    pub fn projectile_speed_up(&self) -> i32 {
        self.def_i32(0x6c8)
    }

    pub fn projectile_speed_relative_up(&self) -> i32 {
        self.def_i32(0x6cc)
    }

    pub fn projectile_speed_forward(&self) -> i32 {
        self.def_i32(0x6d0)
    }

    pub fn projectile_activate_dist(&self) -> i32 {
        self.def_i32(0x6d8)
    }

    pub fn proj_explosion_type(&self) -> i32 {
        self.def_i32(0x6ec)
    }

    pub fn proj_explosion_effect_force_normal_up(&self) -> bool {
        self.def_bool(0x6f4)
    }

    pub fn proj_impact_explode(&self) -> bool {
        self.def_bool(0x72c)
    }

    pub fn bullet_impact_explode(&self) -> bool {
        self.def_bool(0x72f)
    }

    pub fn stickiness(&self) -> i32 {
        self.def_i32(0x730)
    }

    pub fn rotate_type(&self) -> i32 {
        self.def_i32(0x734)
    }

    pub fn plantable(&self) -> bool {
        self.def_bool(0x738)
    }

    pub fn has_detonator(&self) -> bool {
        self.def_bool(0x739)
    }

    pub fn timed_detonation(&self) -> bool {
        self.def_bool(0x73a)
    }

    pub fn rotate(&self) -> bool {
        self.def_bool(0x73c)
    }

    pub fn keep_rolling(&self) -> bool {
        self.def_bool(0x73d)
    }

    pub fn hold_button_to_throw(&self) -> bool {
        self.def_bool(0x73e)
    }

    pub fn offhand_hold_is_cancelable(&self) -> bool {
        self.def_bool(0x73f)
    }

    pub fn freeze_movement_when_firing(&self) -> bool {
        self.def_bool(0x740)
    }

    pub fn melee_charge_range(&self) -> f32 {
        self.def_f32(0x74c)
    }

    pub fn use_as_melee(&self) -> bool {
        self.def_bool(0x750)
    }

    pub fn is_hold_use_grenade(&self) -> bool {
        self.def_bool(0x754)
    }

    pub fn guided_missile_type(&self) -> i32 {
        self.def_i32(0x770)
    }

    pub fn time_to_accelerate(&self) -> f32 {
        self.def_f32(0x778)
    }

    pub fn projectile_curvature(&self) -> f32 {
        self.def_f32(0x77c)
    }

    pub fn ads_gun_kick_reduced_kick_bullets(&self) -> i32 {
        self.def_i32(0x790)
    }

    pub fn ads_spread(&self) -> f32 {
        self.def_f32(0x7e0)
    }

    pub fn hip_gun_kick_reduced_kick_bullets(&self) -> i32 {
        self.def_i32(0x7e4)
    }

    /// Seconds.
    pub fn anti_quick_scope_time(&self) -> f32 {
        self.def_f32(0x830)
    }

    pub fn anti_quick_scope_scale(&self) -> f32 {
        self.def_f32(0x834)
    }

    pub fn ads_reload_trans_time(&self) -> i32 {
        self.def_i32(0x874)
    }

    pub fn destabilization_rate_time(&self) -> f32 {
        self.def_f32(0x8f0)
    }

    pub fn destabilize_distance(&self) -> i32 {
        self.def_i32(0x8f8)
    }

    pub fn intro_fire_time(&self) -> i32 {
        self.def_i32(0x95c)
    }

    /// Shots fired at the intro fire time.
    pub fn intro_fire_length(&self) -> i32 {
        self.def_i32(0x960)
    }

    /// Negative: the variant loaded under another one's.
    pub fn variant_count(&self) -> i32 {
        self.variant_i32(0x4)
    }

    pub fn ignore_attachments(&self) -> bool {
        self.variant_bool(0x1d8)
    }

    pub fn clip_size(&self) -> i32 {
        self.variant_i32(0x1dc)
    }

    pub fn reload_time(&self) -> i32 {
        self.variant_i32(0x1e0)
    }

    pub fn reload_empty_time(&self) -> i32 {
        self.variant_i32(0x1e4)
    }

    pub fn reload_quick_time(&self) -> i32 {
        self.variant_i32(0x1e8)
    }

    pub fn reload_quick_empty_time(&self) -> i32 {
        self.variant_i32(0x1ec)
    }

    pub fn ads_trans_in_time(&self) -> i32 {
        self.variant_i32(0x1f0)
    }

    pub fn ads_trans_out_time(&self) -> i32 {
        self.variant_i32(0x1f4)
    }

    pub fn alt_raise_time(&self) -> i32 {
        self.variant_i32(0x1f8)
    }

    pub fn dual_mag(&self) -> bool {
        self.variant_bool(0x245)
    }

    /// Weapon position change per ms when aiming in (1 / ADS in time).
    pub fn ads_in_rate(&self) -> f32 {
        self.variant_f32(0x23c)
    }

    /// Weapon position change per ms when aiming out (1 / ADS out time).
    pub fn ads_out_rate(&self) -> f32 {
        self.variant_f32(0x240)
    }

    pub fn variant_perks(&self) -> [u32; 2] {
        [0x248, 0x24c].map(|off| self.variant_i32(off) as u32)
    }

    pub fn anti_quick_scope(&self) -> bool {
        self.variant_bool(0x250)
    }
}

pub fn asset_name(load: &ZoneLoad, slot: Ptr) -> Option<&str> {
    let asset = load.asset_at(slot)?;
    let off = if asset.ty == crate::AssetType::Image {
        72
    } else {
        0
    };
    let raw = u32::from_le_bytes(asset.header.get(off..off + 4)?.try_into().ok()?);
    let p = crate::walk::decode_ptr(raw)?;
    load.blocks
        .cstr(p)
        .ok()
        .and_then(|b| core::str::from_utf8(b).ok())
}
