/// Black Ops 2's entity events movement raises, by their number in the game.
pub mod event {
    pub const FOLIAGE_SOUND: i32 = 1;
    pub const STOP_WEAPON_SOUND: i32 = 2;
    pub const STANCE_FORCE_STAND: i32 = 8;
    pub const STANCE_FORCE_CROUCH: i32 = 9;
    pub const STANCE_FORCE_PRONE: i32 = 10;
    pub const SLIDE_START: i32 = STANCE_FORCE_CROUCH;
    pub const SLIDE_END: i32 = STANCE_FORCE_STAND;
    pub const NOAMMO: i32 = 13;
    pub const NOAMMO_LEFT: i32 = 14;
    pub const EMPTY_OFFHAND: i32 = 16;
    pub const RESET_ADS: i32 = 17;
    pub const RELOAD: i32 = 18;
    pub const RELOAD_FROM_EMPTY: i32 = 19;
    pub const RELOAD_START: i32 = 20;
    pub const RELOAD_END: i32 = 21;
    pub const RELOAD_START_NOTIFY: i32 = 22;
    pub const RELOAD_ADDAMMO: i32 = 23;
    pub const RAISE_WEAPON: i32 = 24;
    pub const FIRST_RAISE_WEAPON: i32 = 25;
    pub const RAISE_PICKUP_WEAPON: i32 = 26;
    pub const PUTAWAY_WEAPON: i32 = 27;
    pub const WEAPON_ALT: i32 = 28;
    pub const WEAPON_SWITCH_STARTED: i32 = 29;
    pub const PULLBACK_WEAPON: i32 = 30;
    pub const FIRE_WEAPON: i32 = 32;
    pub const FIRE_WEAPON_LASTSHOT: i32 = 33;
    pub const FIRE_WEAPON_LEFT: i32 = 34;
    pub const FIRE_WEAPON_LASTSHOT_LEFT: i32 = 35;
    pub const RECHAMBER_WEAPON: i32 = 37;
    pub const EJECT_BRASS: i32 = 38;
    pub const MELEE_SWIPE: i32 = 39;
    pub const FIRE_MELEE: i32 = 40;
    pub const WEAPON_FINISH_DEPLOYING: i32 = 42;
    pub const WEAPON_BREAKING_DOWN: i32 = 43;
    pub const WEAPON_FINISH_BREAKING_DOWN: i32 = 44;
    pub const INC_CHARGESHOT_LEVEL: i32 = 45;
    pub const CHARGESHOT_DISCHARGE: i32 = 46;
    pub const PREP_OFFHAND: i32 = 47;
    pub const USE_OFFHAND: i32 = 48;
    pub const SWITCH_OFFHAND: i32 = 49;
    pub const PREP_RIOTSHIELD: i32 = 50;
    pub const DEPLOY_RIOTSHIELD: i32 = 51;
    pub const LOWER_RIOTSHIELD: i32 = 52;
    pub const GRENADE_DROP: i32 = 106;
    pub const GRENADE_SUICIDE: i32 = 107;
    pub const DETONATE: i32 = 108;
    pub const NIGHTVISION_WEAR: i32 = 109;
    pub const NIGHTVISION_REMOVE: i32 = 110;
    pub const NO_FRAG_GRENADE_HINT: i32 = 123;
    pub const NO_SPECIAL_GRENADE_HINT: i32 = 124;
    pub const TARGET_TOO_CLOSE_HINT: i32 = 125;
    pub const TARGET_NOT_ENOUGH_CLEARANCE: i32 = 126;
    pub const LOCKON_REQUIRED_HINT: i32 = 127;
    pub const FOOTSTEP_SPRINT: i32 = 128;
    pub const FOOTSTEP_RUN: i32 = 129;
    pub const FOOTSTEP_WALK: i32 = 130;
    pub const FOOTSTEP_CROUCH_RUN: i32 = 131;
    pub const FOOTSTEP_CROUCH_WALK: i32 = 132;
    pub const FOOTSTEP_PRONE: i32 = 133;
    pub const MANTLE: i32 = 135;
    pub const LAND_MEDIUM: i32 = FOOTSTEP_RUN;
    pub const LAND_SOFT: i32 = FOOTSTEP_WALK;
    pub const JUMP: i32 = 136;
    pub const LAND: i32 = 137;
    pub const FALL_DAMAGE: i32 = 169;
    pub const CANNOTPLANT: i32 = 213;
    pub const DIVE_START: i32 = 215;
    pub const DIVE_LAND: i32 = 216;
    pub const SLIDE_LOOP_START: i32 = 217;
    pub const SLIDE_LOOP_END: i32 = 218;
    pub const SCOPE_ZOOM: i32 = 219;
    pub const JAM_WEAPON: i32 = 220;
    pub const STACKFIRE: i32 = 221;
    pub const PRONE_BLOCKED: i32 = 234;
}

/// Black Ops 2's player animation script events, by their number in the game.
pub mod anim {
    pub const FIRE_WEAPON: i32 = 2;
    pub const JUMP: i32 = 3;
    pub const LAND: i32 = 4;
    pub const DROP_WEAPON: i32 = 5;
    pub const RAISE_WEAPON: i32 = 6;
    pub const FIRST_RAISE_WEAPON: i32 = 7;
    pub const RELOAD: i32 = 8;
    pub const CROUCH_TO_PRONE: i32 = 9;
    pub const PRONE_TO_CROUCH: i32 = 10;
    pub const STAND_TO_CROUCH: i32 = 11;
    pub const CROUCH_TO_STAND: i32 = 12;
    pub const PRONE_TO_STAND: i32 = 13;
    pub const PRONE_TO_SPRINT: i32 = 14;
    pub const MELEE_ATTACK: i32 = 16;
    pub const MELEE_LEFT: i32 = 17;
    pub const KNIFE_MELEE: i32 = 18;
    pub const KNIFE_MELEE_CHARGE: i32 = 19;
    pub const FLINCH: i32 = 20;
    pub const DIVE: i32 = 26;
    pub const DIVE_LAND: i32 = 27;
    pub const STAND_TO_LAST_STAND: i32 = 28;
    pub const CROUCH_TO_LAST_STAND: i32 = 29;
    pub const PRONE_TO_LAST_STAND: i32 = 30;
    pub const LAST_STAND_TO_STAND: i32 = 31;
    pub const LAST_STAND_TO_CROUCH: i32 = 32;
    pub const LAST_STAND_TO_PRONE: i32 = 33;
    pub const PRIME_GRENADE: i32 = 36;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveEvent {
    Entity(i32, i32),
    Anim(i32, bool, bool),
    ProneAnim,
    LegsMove(i32, bool),
}
