//! Black Ops keeps two bits per perk, in the order of its perk table.

pub const NAMES: [&str; 15] = [
    "specialty_longersprint",
    "specialty_unlimitedsprint",
    "specialty_scavanger",
    "specialty_fastreload",
    "specialty_bulletdamage",
    "specialty_bulletaccuracy",
    "specialty_flakjacket",
    "specialty_armorvest",
    "specialty_quickrevive",
    "specialty_altmelee",
    "specialty_rof",
    "specialty_extraammo",
    "specialty_endurance",
    "specialty_deadshot",
    "specialty_additionalprimaryweapon",
];

/// The bits `setperk` sets for a perk name.
pub fn bits(name: &str) -> Option<u32> {
    NAMES
        .iter()
        .position(|n| n.eq_ignore_ascii_case(name))
        .map(|i| 3 << (i * 2))
}

pub const SPRINT_LEVEL: u32 = 0x3;
pub const SPRINT_SPEED: u32 = SPRINT_LEVEL;
pub const SPRINT_UNLIMITED: u32 = 0xc;
pub const FALL_DAMAGE: u32 = 0x3000;
pub const ENDURANCE: u32 = 0x300_0000;
pub const DEADSHOT: u32 = 0xc00_0000;
pub const FAST_RELOAD: u32 = 0xc0;
pub const RATE_OF_FIRE: u32 = 0x30_0000;

/// The perks the multiplayer executable's movement and weapon code tests:
/// one bit each, numbered in its perk table's order, `perks` holding the
/// first 32 and `perks_high` the rest.
pub mod mp {
    use crate::PlayerState;

    pub const BULLET_ACCURACY: u32 = 2;
    pub const BULLET_PENETRATION: u32 = 5;
    pub const FALL_HEIGHT: u32 = 13;
    pub const FAST_MANTLE: u32 = 16;
    pub const FAST_MELEE_RECOVERY: u32 = 17;
    pub const FAST_RELOAD: u32 = 18;
    pub const FAST_WEAPON_SWITCH: u32 = 19;
    pub const LONGER_SPRINT: u32 = 30;
    pub const SPRINT_RECOVERY: u32 = 46;
    pub const UNLIMITED_SPRINT: u32 = 51;

    pub fn has(ps: &PlayerState, perk: u32) -> bool {
        let word = if perk < 32 { ps.perks } else { ps.perks_high };
        word >> (perk % 32) & 1 != 0
    }
}
