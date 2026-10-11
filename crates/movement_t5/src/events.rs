/// Black Ops' entity events movement raises, by their number in the game.
pub mod event {
    pub const FOLIAGE_SOUND: i32 = 1;
    pub const RESET_ADS: i32 = 17;
    pub const STANCE_FORCE_STAND: i32 = 8;
    pub const STANCE_FORCE_CROUCH: i32 = 9;
    pub const STANCE_FORCE_PRONE: i32 = 10;
    pub const SLIDE_START: i32 = STANCE_FORCE_CROUCH;
    pub const SLIDE_END: i32 = STANCE_FORCE_STAND;
    pub const NIGHTVISION_WEAR: i32 = 93;
    pub const NIGHTVISION_REMOVE: i32 = 94;
    pub const FOOTSTEP_RUN: i32 = 102;
    pub const FOOTSTEP_WALK: i32 = 103;
    pub const FOOTSTEP_CROUCH_RUN: i32 = 104;
    pub const FOOTSTEP_CROUCH_WALK: i32 = 105;
    pub const FOOTSTEP_PRONE: i32 = 106;
    pub const FOOTSTEP_SPRINT: i32 = 101;
    pub const MANTLE: i32 = 107;
    pub const LAND_MEDIUM: i32 = FOOTSTEP_RUN;
    pub const LAND_SOFT: i32 = FOOTSTEP_WALK;
    pub const JUMP: i32 = 108;
    pub const LAND: i32 = 109;
    pub const FALL_DAMAGE: i32 = 140;
    pub const DIVE_START: i32 = 184;
    pub const DIVE_LAND: i32 = 185;
    pub const SLIDE_LOOP_START: i32 = 186;
    pub const SLIDE_LOOP_END: i32 = 187;

    pub const STOP_WEAPON_SOUND: i32 = 2;
    pub const NOAMMO: i32 = 13;
    pub const NOAMMO_LEFT: i32 = 14;
    pub const EMPTY_OFFHAND: i32 = 16;
    pub const RELOAD: i32 = 18;
    pub const RELOAD_FROM_EMPTY: i32 = 19;
    pub const RELOAD_START: i32 = 20;
    pub const RELOAD_END: i32 = 21;
    pub const RELOAD_START_NOTIFY: i32 = 22;
    pub const RELOAD_ADDAMMO: i32 = 23;
    pub const RAISE_WEAPON: i32 = 24;
    pub const FIRST_RAISE_WEAPON: i32 = 25;
    pub const PUTAWAY_WEAPON: i32 = 26;
    pub const WEAPON_ALT: i32 = 27;
    pub const PULLBACK_WEAPON: i32 = 28;
    pub const FIRE_WEAPON: i32 = 29;
    pub const FIRE_WEAPON_LASTSHOT: i32 = 30;
    pub const FIRE_WEAPON_LEFT: i32 = 31;
    pub const FIRE_WEAPON_LASTSHOT_LEFT: i32 = 32;
    pub const RECHAMBER_WEAPON: i32 = 33;
    pub const EJECT_BRASS: i32 = 34;
    pub const MELEE_SWIPE: i32 = 35;
    pub const FIRE_MELEE: i32 = 36;
    pub const WEAPON_FINISH_DEPLOYING: i32 = 38;
    pub const WEAPON_BREAKING_DOWN: i32 = 39;
    pub const WEAPON_FINISH_BREAKING_DOWN: i32 = 40;
    pub const PREP_OFFHAND: i32 = 41;
    pub const USE_OFFHAND: i32 = 42;
    pub const SWITCH_OFFHAND: i32 = 43;
    pub const NO_FRAG_GRENADE_HINT: i32 = 96;
    pub const NO_SPECIAL_GRENADE_HINT: i32 = 97;
    pub const GRENADE_DROP: i32 = 90;
    pub const GRENADE_SUICIDE: i32 = 91;
    pub const DETONATE: i32 = 92;
    pub const TARGET_TOO_CLOSE_HINT: i32 = 98;
    pub const TARGET_NOT_ENOUGH_CLEARANCE: i32 = 99;
    pub const LOCKON_REQUIRED_HINT: i32 = 100;
    pub const SCOPE_ZOOM: i32 = 188;
    pub const STACKFIRE: i32 = 190;
}

/// Black Ops' player animation script events, by their number in the game.
pub mod anim {
    pub const FIRE_WEAPON: i32 = 2;
    pub const JUMP: i32 = 3;
    pub const DROP_WEAPON: i32 = 5;
    pub const RAISE_WEAPON: i32 = 6;
    pub const RELOAD: i32 = 7;
    pub const MELEE_ATTACK: i32 = 14;
    pub const PRIME_GRENADE: i32 = 33;
    pub const LAND: i32 = 4;
    pub const CROUCH_TO_PRONE: i32 = 8;
    pub const PRONE_TO_CROUCH: i32 = 9;
    pub const STAND_TO_CROUCH: i32 = 10;
    pub const CROUCH_TO_STAND: i32 = 11;
    pub const PRONE_TO_STAND: i32 = 12;
    pub const PRONE_TO_SPRINT: i32 = 13;
    pub const FLINCH: i32 = 17;
    pub const DIVE: i32 = 23;
    pub const DIVE_LAND: i32 = 24;
    pub const STAND_TO_LAST_STAND: i32 = 25;
    pub const CROUCH_TO_LAST_STAND: i32 = 26;
    pub const PRONE_TO_LAST_STAND: i32 = 27;
}

/// What one movement step tells the rest of the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveEvent {
    /// A predictable entity event: `(event, parm)`.
    Entity(i32, i32),
    /// A player animation script event: `(event, continue, force)`.
    Anim(i32, bool, bool),
    /// The player's prone animation starts over.
    ProneAnim,
    /// The movement animation type the legs play: `(type, sprinting)`.
    LegsMove(i32, bool),
}
