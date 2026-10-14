//! Black Ops 2 keeps one bit per perk across two 32-bit words, in the order
//! of its 63-entry perk table.

pub const NAMES: [&str; 63] = [
    "specialty_additionalprimaryweapon",
    "specialty_armorpiercing",
    "specialty_armorvest",
    "specialty_bulletaccuracy",
    "specialty_bulletdamage",
    "specialty_bulletflinch",
    "specialty_bulletpenetration",
    "specialty_deadshot",
    "specialty_delayexplosive",
    "specialty_detectexplosive",
    "specialty_disarmexplosive",
    "specialty_earnmoremomentum",
    "specialty_explosivedamage",
    "specialty_extraammo",
    "specialty_fallheight",
    "specialty_fastads",
    "specialty_fastequipmentuse",
    "specialty_fastladderclimb",
    "specialty_fastmantle",
    "specialty_fastmeleerecovery",
    "specialty_fastreload",
    "specialty_fasttoss",
    "specialty_fastweaponswitch",
    "specialty_finalstand",
    "specialty_fireproof",
    "specialty_flakjacket",
    "specialty_flashprotection",
    "specialty_gpsjammer",
    "specialty_grenadepulldeath",
    "specialty_healthregen",
    "specialty_holdbreath",
    "specialty_immunecounteruav",
    "specialty_immuneemp",
    "specialty_immunemms",
    "specialty_immunenvthermal",
    "specialty_immunerangefinder",
    "specialty_killstreak",
    "specialty_longersprint",
    "specialty_loudenemies",
    "specialty_marksman",
    "specialty_movefaster",
    "specialty_nomotionsensor",
    "specialty_noname",
    "specialty_nottargetedbyairsupport",
    "specialty_nokillstreakreticle",
    "specialty_nottargettedbysentry",
    "specialty_pin_back",
    "specialty_pistoldeath",
    "specialty_proximityprotection",
    "specialty_quickrevive",
    "specialty_quieter",
    "specialty_reconnaissance",
    "specialty_rof",
    "specialty_scavenger",
    "specialty_showenemyequipment",
    "specialty_stunprotection",
    "specialty_shellshock",
    "specialty_sprintrecovery",
    "specialty_showonradar",
    "specialty_stalker",
    "specialty_twogrenades",
    "specialty_twoprimaries",
    "specialty_unlimitedsprint",
];

pub const DEADSHOT: usize = 0x07;
pub const FALL_HEIGHT: usize = 0x0e;
pub const FAST_ADS: usize = 0x0f;
pub const FAST_EQUIPMENT_USE: usize = 0x10;
pub const FAST_LADDER_CLIMB: usize = 0x11;
pub const FAST_MANTLE: usize = 0x12;
pub const FAST_MELEE_RECOVERY: usize = 0x13;
pub const FAST_RELOAD: usize = 0x14;
pub const FAST_TOSS: usize = 0x15;
pub const FAST_WEAPON_SWITCH: usize = 0x16;
pub const FLAK_JACKET: usize = 0x19;
pub const HOLD_BREATH: usize = 0x1e;
pub const LONGER_SPRINT: usize = 0x25;
pub const MOVE_FASTER: usize = 0x28;
pub const NONAME: usize = 0x2a;
pub const RATE_OF_FIRE: usize = 0x34;
pub const SPRINT_RECOVERY: usize = 0x39;
pub const STALKER: usize = 0x3b;
pub const UNLIMITED_SPRINT: usize = 0x3e;

pub fn index(name: &str) -> Option<usize> {
    NAMES.iter().position(|n| n.eq_ignore_ascii_case(name))
}

pub fn has(perks: [u32; 2], idx: usize) -> bool {
    idx < 63 && (perks[idx >> 5] & (1u32 << (idx & 31))) != 0
}

pub fn set(perks: &mut [u32; 2], idx: usize, on: bool) {
    if idx >= 63 {
        return;
    }
    let mask = 1u32 << (idx & 31);
    if on {
        perks[idx >> 5] |= mask;
    } else {
        perks[idx >> 5] &= !mask;
    }
}
