use asset_iw4::size::{WEAPON_ANIM_COUNT, weap_anim};

pub const ACTION_GOAL_TIME_SECS: f32 = 0.0;

pub const IDLE_INTERRUPT_GOAL_TIME_SECS: f32 = 0.5;

pub const ACTIVE_GOAL_WEIGHT: f32 = 1.0;
pub const INACTIVE_GOAL_WEIGHT: f32 = 0.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimRateOffsets {
    pub weapon_def: i32,
    pub complete_def: i32,
}

impl AnimRateOffsets {
    pub const NATIVE: Self = Self {
        weapon_def: -1,
        complete_def: -1,
    };

    pub const fn is_native(self) -> bool {
        self.weapon_def < 0 && self.complete_def < 0
    }
}

pub const WEAPON_ANIM_SLOTS: usize = WEAPON_ANIM_COUNT + 2 + weap_anim_extra::LATER_GAMES;

/// Slots past Modern Warfare 2's own animations: its quick reloads, then the
/// later games' animations it has none of (a game's viewmodel rule plays
/// them).
pub mod weap_anim_extra {
    use super::WEAPON_ANIM_COUNT;

    pub const RELOAD_QUICK: usize = WEAPON_ANIM_COUNT;
    pub const RELOAD_QUICK_EMPTY: usize = WEAPON_ANIM_COUNT + 1;
    pub const RELOAD_RIGHT: usize = WEAPON_ANIM_COUNT + 2;
    pub const SPRINT_EMPTY_IN: usize = WEAPON_ANIM_COUNT + 3;
    pub const SPRINT_EMPTY_LOOP: usize = WEAPON_ANIM_COUNT + 4;
    pub const SPRINT_EMPTY_OUT: usize = WEAPON_ANIM_COUNT + 5;
    pub const LOWREADY_IN: usize = WEAPON_ANIM_COUNT + 6;
    pub const LOWREADY_LOOP: usize = WEAPON_ANIM_COUNT + 7;
    pub const LOWREADY_OUT: usize = WEAPON_ANIM_COUNT + 8;
    pub const CONT_FIRE_IN: usize = WEAPON_ANIM_COUNT + 9;
    pub const CONT_FIRE_LOOP: usize = WEAPON_ANIM_COUNT + 10;
    pub const CONT_FIRE_OUT: usize = WEAPON_ANIM_COUNT + 11;
    pub const DEPLOY: usize = WEAPON_ANIM_COUNT + 12;
    pub const BREAKDOWN: usize = WEAPON_ANIM_COUNT + 13;
    pub const DTP_IN: usize = WEAPON_ANIM_COUNT + 14;
    pub const DTP_LOOP: usize = WEAPON_ANIM_COUNT + 15;
    pub const DTP_OUT: usize = WEAPON_ANIM_COUNT + 16;
    pub const DTP_EMPTY_IN: usize = WEAPON_ANIM_COUNT + 17;
    pub const DTP_EMPTY_LOOP: usize = WEAPON_ANIM_COUNT + 18;
    pub const DTP_EMPTY_OUT: usize = WEAPON_ANIM_COUNT + 19;
    pub const SLIDE_IN: usize = WEAPON_ANIM_COUNT + 20;
    pub const MANTLE: usize = WEAPON_ANIM_COUNT + 21;
    pub const SPRINT_CAMERA: usize = WEAPON_ANIM_COUNT + 22;
    pub const DTP_IN_CAMERA: usize = WEAPON_ANIM_COUNT + 23;
    pub const DTP_LOOP_CAMERA: usize = WEAPON_ANIM_COUNT + 24;
    pub const DTP_OUT_CAMERA: usize = WEAPON_ANIM_COUNT + 25;
    pub const MANTLE_CAMERA: usize = WEAPON_ANIM_COUNT + 26;
    /// The camera animations' slots.
    pub const CAMERAS: core::ops::Range<usize> = SPRINT_CAMERA..MANTLE_CAMERA + 1;
    pub const FIRE_INTRO: usize = WEAPON_ANIM_COUNT + 27;
    pub const ADS_FIRE_INTRO: usize = WEAPON_ANIM_COUNT + 28;
    pub const MELEE1: usize = WEAPON_ANIM_COUNT + 29;
    pub const MELEE2: usize = WEAPON_ANIM_COUNT + 30;
    pub const MELEE3: usize = WEAPON_ANIM_COUNT + 31;
    pub const MELEE_EMPTY: usize = WEAPON_ANIM_COUNT + 32;
    pub const MELEE_CHARGE_EMPTY: usize = WEAPON_ANIM_COUNT + 33;
    pub const CRAWL_IN: usize = WEAPON_ANIM_COUNT + 34;
    pub const CRAWL_FORWARD: usize = WEAPON_ANIM_COUNT + 35;
    pub const CRAWL_BACK: usize = WEAPON_ANIM_COUNT + 36;
    pub const CRAWL_RIGHT: usize = WEAPON_ANIM_COUNT + 37;
    pub const CRAWL_LEFT: usize = WEAPON_ANIM_COUNT + 38;
    pub const CRAWL_OUT: usize = WEAPON_ANIM_COUNT + 39;
    pub const CRAWL_EMPTY_IN: usize = WEAPON_ANIM_COUNT + 40;
    pub const CRAWL_EMPTY_FORWARD: usize = WEAPON_ANIM_COUNT + 41;
    pub const CRAWL_EMPTY_BACK: usize = WEAPON_ANIM_COUNT + 42;
    pub const CRAWL_EMPTY_RIGHT: usize = WEAPON_ANIM_COUNT + 43;
    pub const CRAWL_EMPTY_LEFT: usize = WEAPON_ANIM_COUNT + 44;
    pub const CRAWL_EMPTY_OUT: usize = WEAPON_ANIM_COUNT + 45;
    pub(super) const LATER_GAMES: usize = 44;
}

pub const ANIM_RATE_TABLE: [AnimRateOffsets; WEAPON_ANIM_COUNT] = [
    AnimRateOffsets::NATIVE,
    AnimRateOffsets::NATIVE,
    AnimRateOffsets::NATIVE,
    AnimRateOffsets::NATIVE,
    AnimRateOffsets {
        weapon_def: 0x25c,
        complete_def: -1,
    },
    AnimRateOffsets::NATIVE,
    AnimRateOffsets::NATIVE,
    AnimRateOffsets {
        weapon_def: 0x264,
        complete_def: -1,
    },
    AnimRateOffsets {
        weapon_def: 0x268,
        complete_def: -1,
    },
    AnimRateOffsets {
        weapon_def: 0x26c,
        complete_def: -1,
    },
    AnimRateOffsets {
        weapon_def: 0x274,
        complete_def: -1,
    },
    AnimRateOffsets {
        weapon_def: 0x27c,
        complete_def: -1,
    },
    AnimRateOffsets {
        weapon_def: 0x284,
        complete_def: -1,
    },
    AnimRateOffsets {
        weapon_def: 0x28c,
        complete_def: -1,
    },
    AnimRateOffsets {
        weapon_def: -1,
        complete_def: 0x54,
    },
    AnimRateOffsets {
        weapon_def: 0x29c,
        complete_def: -1,
    },
    AnimRateOffsets {
        weapon_def: 0x288,
        complete_def: -1,
    },
    AnimRateOffsets {
        weapon_def: -1,
        complete_def: 0x44,
    },
    AnimRateOffsets {
        weapon_def: 0x290,
        complete_def: -1,
    },
    AnimRateOffsets {
        weapon_def: 0x298,
        complete_def: -1,
    },
    AnimRateOffsets {
        weapon_def: 0x294,
        complete_def: -1,
    },
    AnimRateOffsets {
        weapon_def: 0x2a0,
        complete_def: -1,
    },
    AnimRateOffsets {
        weapon_def: 0x2a4,
        complete_def: -1,
    },
    AnimRateOffsets {
        weapon_def: 0x2a8,
        complete_def: -1,
    },
    AnimRateOffsets {
        weapon_def: 0x2ac,
        complete_def: -1,
    },
    AnimRateOffsets {
        weapon_def: 0x2b0,
        complete_def: -1,
    },
    AnimRateOffsets {
        weapon_def: 0x2b4,
        complete_def: -1,
    },
    AnimRateOffsets {
        weapon_def: 0x2b8,
        complete_def: -1,
    },
    AnimRateOffsets {
        weapon_def: 0x2bc,
        complete_def: -1,
    },
    AnimRateOffsets::NATIVE,
    AnimRateOffsets {
        weapon_def: 0x2c0,
        complete_def: -1,
    },
    AnimRateOffsets {
        weapon_def: 0x2cc,
        complete_def: -1,
    },
    AnimRateOffsets::NATIVE,
    AnimRateOffsets::NATIVE,
    AnimRateOffsets::NATIVE,
    AnimRateOffsets::NATIVE,
    AnimRateOffsets::NATIVE,
];

pub const WEAP_ANIM_EVENT_MASK: u32 = 0xffff_fdff;

pub fn slot_for_weap_anim_event(masked_event: u32) -> Option<usize> {
    let ev = masked_event & WEAP_ANIM_EVENT_MASK;
    Some(match ev {
        0 | 1 => return None,
        2 => weap_anim::FIRE,
        3 => weap_anim::LASTSHOT,
        4 => weap_anim::RECHAMBER,
        5 => weap_anim::ADS_FIRE,
        6 => weap_anim::ADS_LASTSHOT,
        7 => weap_anim::ADS_RECHAMBER,
        8 => weap_anim::MELEE,
        9 => weap_anim::MELEE_CHARGE,
        0xa => weap_anim::DROP,
        0xb => weap_anim::RAISE,
        0xc => weap_anim::FIRST_RAISE,
        0xd => weap_anim::RELOAD,
        0xe => weap_anim::RELOAD_EMPTY,
        0xf => weap_anim::RELOAD_START,
        0x10 => weap_anim::RELOAD_END,
        0x11 => weap_anim::ALT_DROP,
        0x12 => weap_anim::ALT_RAISE,
        0x13 => weap_anim::QUICK_DROP,
        0x14 => weap_anim::QUICK_RAISE,
        0x15 => weap_anim::EMPTY_DROP,
        0x16 => weap_anim::EMPTY_RAISE,
        0x17 => weap_anim::SPRINT_IN,
        0x18 => weap_anim::SPRINT_LOOP,
        0x19 => weap_anim::SPRINT_OUT,
        0x1a => weap_anim::STUNNED_START,
        0x1b => weap_anim::STUNNED_LOOP,
        0x1c => weap_anim::STUNNED_END,
        0x1d => weap_anim::HOLD_FIRE,
        0x1e => weap_anim::DETONATE,
        0x1f => weap_anim::NIGHTVISION_WEAR,
        0x20 => weap_anim::NIGHTVISION_REMOVE,
        0x21 => weap_anim_extra::RELOAD_QUICK,
        0x22 => weap_anim_extra::RELOAD_QUICK_EMPTY,
        _ => weap_anim::IDLE,
    })
}

pub fn playback_rate(clip_length_ms: i32, weapon_timer_ms: i32) -> f32 {
    if weapon_timer_ms < 0 {
        return 1.0;
    }
    if weapon_timer_ms == 0 {
        return 0.0;
    }
    if clip_length_ms <= 0 {
        return 1.0;
    }
    clip_length_ms as f32 / weapon_timer_ms as f32
}

pub fn slot_uses_native_rate(slot: usize) -> bool {
    ANIM_RATE_TABLE
        .get(slot)
        .map(|e| e.is_native())
        .unwrap_or(true)
}

pub fn known_rate_timer_offset(slot: usize) -> Option<i32> {
    let e = ANIM_RATE_TABLE.get(slot)?;
    if e.weapon_def >= 0 {
        Some(e.weapon_def)
    } else {
        None
    }
}

pub fn known_complete_rate_timer_offset(slot: usize) -> Option<i32> {
    let e = ANIM_RATE_TABLE.get(slot)?;
    if e.complete_def >= 0 {
        Some(e.complete_def)
    } else {
        None
    }
}
