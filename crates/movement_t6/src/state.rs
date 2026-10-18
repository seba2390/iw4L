pub const ENTITYNUM_WORLD: i32 = 1022;
pub const ENTITYNUM_NONE: i32 = 1023;

pub mod pm_type {
    pub const NORMAL: i32 = 0;
    pub const NORMAL_LINKED: i32 = 1;
    pub const NOCLIP: i32 = 2;
    pub const UFO: i32 = 3;
    pub const MPVIEWER: i32 = 4;
    pub const SPECTATOR: i32 = 5;
    pub const INTERMISSION: i32 = 6;
    pub const LAST_STAND: i32 = 7;
    pub const LAST_STAND_REVIVED: i32 = 8;
    pub const LAST_STAND_GETTING_UP: i32 = 9;
    pub const DEAD: i32 = 10;
    pub const DEAD_LINKED: i32 = 11;
}

pub mod pm_flags {
    pub const PRONE: u32 = 0x1;
    pub const DUCKED: u32 = 0x2;
    pub const MANTLE: u32 = 0x4;
    pub const LADDER: u32 = 0x8;
    pub const ADS_INTENT: u32 = 0x10;
    pub const BACKWARDS_RUN: u32 = 0x20;
    pub const SIGHT_AIMING: u32 = 0x40;
    pub const TIME_HARDLANDING: u32 = 0x80;
    pub const TIME_KNOCKBACK: u32 = 0x100;
    pub const PRONEMOVE_OVERRIDDEN: u32 = 0x200;
    pub const RESPAWNED: u32 = 0x400;
    pub const FROZEN: u32 = 0x800;
    pub const STANCE_CHANGED: u32 = 0x1000;
    pub const LADDER_FALL: u32 = 0x2000;
    pub const JUMPING: u32 = 0x4000;
    pub const SPRINTING: u32 = 0x8000;
    pub const SHELLSHOCKED: u32 = 0x1_0000;
    pub const LAUNCHED: u32 = 0x2_0000;
    pub const SPRINT_DISABLED: u32 = 0x4_0000;
    pub const NO_JUMP: u32 = 0x8_0000;
    pub const ANIM_LOCK: u32 = 0x10_0000;
    pub const DIVING: u32 = 0x20_0000;
    pub const SLIDING: u32 = 0x40_0000;
    pub const NO_STAND: u32 = 0x80_0000;
    pub const NO_CROUCH: u32 = 0x100_0000;
    pub const NO_PRONE: u32 = 0x200_0000;
    pub const VIEW_LINKED: u32 = 0x400_0000;
    pub const PRONE_YAW_LOCKED: u32 = 0x800_0000;
    pub const NO_LEAN: u32 = 0x1000_0000;
    pub const NO_MELEE: u32 = 0x2000_0000;
    pub const LOW_READY: u32 = 0x4000_0000;
    pub const DOOR_BREACH: u32 = 0x8000_0000;
}

pub mod e_flags {
    pub const CROUCHING: u32 = 0x4;
    pub const PRONE: u32 = 0x8;
    pub const FIRING: u32 = 0x40;
    pub const TURRET_PRONE: u32 = 0x100;
    pub const TURRET_CROUCH: u32 = 0x200;
    pub const TURRET: u32 = TURRET_PRONE | TURRET_CROUCH;
    pub const VEHICLE_VIEW: u32 = 0x4000;
    pub const MANTLE: u32 = 0x8000;
    pub const TALKING: u32 = 0x2_0000;
}

/// The player state's weapon flags (the low word of Black Ops 2's 64-bit
/// field; the weapon code uses no bit of the high word).
pub mod weap_flags {
    /// A reload asked for by script.
    pub const RELOAD_REQUESTED: u32 = 0x1;
    /// The offhand is in hand.
    pub const OFFHAND_ACTIVE: u32 = 0x2;
    pub const HOLD_BREATH: u32 = 0x4;
    /// A grenade throw is cancelled.
    pub const CANCEL_GRENADE: u32 = 0x8;
    /// `allowads(false)`.
    pub const NO_ADS: u32 = 0x20;
    pub const NIGHTVISION: u32 = 0x40;
    /// `disableweapons`.
    pub const WEAPONS_DISABLED: u32 = 0x80;
    /// `disableoffhandweapons`.
    pub const OFFHANDS_DISABLED: u32 = 0x100;
    /// `disableweaponcycling`.
    pub const CYCLING_DISABLED: u32 = 0x200;
    /// The right hand's fire button must be let go before the next shot.
    pub const FIRE_LATCH: u32 = 0x400;
    pub const FIRE_LATCH_LEFT: u32 = 0x800;
    pub const DEPLOY: u32 = 0x1000;
    pub const BREAKDOWN: u32 = 0x2000;
    /// The weapon went away for swimming.
    pub const SWIM_PUTAWAY: u32 = 0x4000;
    pub const GUIDED_MISSILE: u32 = 0x1_0000;
    pub const RELOAD_LEFT: u32 = 0x2_0000;
    /// The offhand is thrown with the weapon away.
    pub const OFFHAND_SWITCH: u32 = 0x4_0000;
    /// The offhand was thrown with no weapon raised.
    pub const OFFHAND_EMPTY_HANDED: u32 = 0x10_0000;
    pub const FIRING_GUIDED: u32 = 0x20_0000;
    /// The weapon being switched to is a riot shield; dropped outside the
    /// raise and drop states.
    pub const SWITCHING_TO_RIOTSHIELD: u32 = 0x40_0000;
    /// A riot shield or tactical insertion cannot be placed here.
    pub const PLACEMENT_BLOCKED: u32 = 0x200_0000;
    pub const CHARGE_SHOT_CHARGING: u32 = 0x400_0000;
    /// Set by the server while a revive weapon is used on a downed player:
    /// a held grenade cannot be cancelled.
    pub const REVIVING: u32 = 0x4000_0000;
}

/// The second word of entity flags.
pub mod e_flags2 {
    /// A grenade throw cannot be cancelled now.
    pub const GRENADE_HELD: u32 = 0x200;
    /// The weapon's clip ran dry: set when the last round is fired or an
    /// empty weapon is raised, cleared by a reload.
    pub const CLIP_EMPTY: u32 = 0x400;
    /// An airburst weapon is aimed.
    pub const AIRBURST_ADS: u32 = 0x2000;
    /// Aiming is dropped at once.
    pub const ADS_RESET: u32 = 0x4_0000;
    /// A minigun's barrels spin up.
    pub const SPINNING: u32 = 0x40_0000;
    /// The player drives something from afar (a guided missile's camera, a
    /// remote vehicle seat): `EF2_REMOTE_CONTROL`, as the server's player
    /// death names it when it refuses to run.
    pub const REMOTE_CONTROL: u32 = 0x1000_0000;
}

/// The player state's other flags.
pub mod other_flags {
    /// The state belongs to a playing client (not a spectator): set by the
    /// server at the end of each of its frames; aiming needs it.
    pub const PLAYER: u32 = 0x4;
    pub const SPECTATOR: u32 = 0x8;
    /// The player is EMP'd: a charge shot's queue is capped.
    pub const EMP: u32 = 0x40;
    /// Movement, stance, lean, reload and fire are locked.
    pub const CONTROLS_LOCKED: u32 = 0x400;
}

/// Black Ops 2's weapon states, the same for both hands.
pub mod weapon_state {
    pub const READY: i32 = 0;
    pub const RAISING: i32 = 1;
    pub const RAISING_ALT: i32 = 2;
    pub const DROPPING: i32 = 3;
    pub const DROPPING_QUICK: i32 = 4;
    pub const DROPPING_ALT: i32 = 5;
    pub const FIRING: i32 = 6;
    pub const RECHAMBERING: i32 = 7;
    pub const RECHAMBER_END: i32 = 8;
    pub const RELOADING: i32 = 0xb;
    pub const RELOADING_INTERRUPT: i32 = 0xc;
    pub const RELOAD_START: i32 = 0xd;
    /// A reload start that rechambers a bolt-action weapon first.
    pub const RELOAD_START_RECHAMBER: i32 = 0xe;
    pub const RELOAD_START_INTERRUPT: i32 = 0xf;
    pub const RELOAD_END: i32 = 0x10;
    pub const RELOADING_LEFT: i32 = 0x11;
    pub const RELOADING_LEFT_INTERRUPT: i32 = 0x12;
    pub const MELEE_INIT: i32 = 0x13;
    pub const MELEE_FIRE: i32 = 0x14;
    pub const MELEE_END: i32 = 0x15;
    pub const OFFHAND_INIT: i32 = 0x16;
    pub const OFFHAND_PREPARE: i32 = 0x17;
    pub const OFFHAND_START: i32 = 0x18;
    /// An offhand with a detonator and no ammo detonates.
    pub const OFFHAND_DETONATE: i32 = 0x19;
    pub const OFFHAND_HOLD: i32 = 0x1a;
    pub const OFFHAND: i32 = 0x1b;
    /// `forceoffhandend`: the offhand is put away at once.
    pub const OFFHAND_FORCE_END: i32 = 0x1c;
    pub const OFFHAND_END: i32 = 0x1d;
    pub const RIOTSHIELD_PREPARE: i32 = 0x1e;
    pub const RIOTSHIELD_DEPLOY: i32 = 0x1f;
    pub const RIOTSHIELD_PLACING: i32 = 0x20;
    pub const RIOTSHIELD_END: i32 = 0x22;
    pub const DETONATING: i32 = 0x23;
    pub const SPRINT_RAISE: i32 = 0x24;
    pub const SPRINT_LOOP: i32 = 0x25;
    pub const SPRINT_DROP: i32 = 0x26;
    pub const CONT_FIRE_IN: i32 = 0x27;
    pub const CONT_FIRE_LOOP: i32 = 0x28;
    pub const CONT_FIRE_OUT: i32 = 0x29;
    pub const NIGHTVISION_WEAR: i32 = 0x2a;
    pub const NIGHTVISION_REMOVE: i32 = 0x2b;
    pub const DEPLOYING: i32 = 0x2c;
    pub const DEPLOYED: i32 = 0x2d;
    pub const BREAKING_DOWN: i32 = 0x2e;
    pub const SWIM_IN: i32 = 0x2f;
    pub const SWIM_OUT: i32 = 0x30;
    pub const DTP_IN: i32 = 0x31;
    pub const DTP_LOOP: i32 = 0x32;
    pub const DTP_OUT: i32 = 0x33;
    pub const SLIDE_IN: i32 = 0x34;
    /// Special fire: the weapon jams.
    pub const FIRING_SPECIAL: i32 = 0x37;
    pub const LOWREADY_RAISE: i32 = 0x38;
    pub const LOWREADY_LOOP: i32 = 0x39;
    pub const LOWREADY_DROP: i32 = 0x3a;
    pub const CRAWL_IN: i32 = 0x3b;
    pub const CRAWL_FORWARD: i32 = 0x3c;
    pub const CRAWL_BACK: i32 = 0x3d;
    pub const CRAWL_RIGHT: i32 = 0x3e;
    pub const CRAWL_LEFT: i32 = 0x3f;
    pub const CRAWL_OUT: i32 = 0x40;

    pub fn is_raising_or_dropping(s: i32) -> bool {
        (RAISING..=DROPPING_ALT).contains(&s)
    }

    pub fn is_firing(s: i32) -> bool {
        matches!(s, FIRING | RECHAMBERING | RECHAMBER_END | CONT_FIRE_LOOP)
    }

    pub fn is_reloading(s: i32) -> bool {
        (RELOADING..=RELOADING_LEFT_INTERRUPT).contains(&s)
    }

    pub fn is_reload_start(s: i32) -> bool {
        matches!(
            s,
            RELOAD_START | RELOAD_START_RECHAMBER | RELOAD_START_INTERRUPT
        )
    }

    pub fn is_melee(s: i32) -> bool {
        (MELEE_INIT..=MELEE_END).contains(&s)
    }

    pub fn is_offhand(s: i32) -> bool {
        (OFFHAND_INIT..=OFFHAND_END).contains(&s)
    }

    pub fn is_riotshield(s: i32) -> bool {
        (RIOTSHIELD_PREPARE..=RIOTSHIELD_END).contains(&s)
    }

    pub fn is_sprint(s: i32) -> bool {
        (SPRINT_RAISE..=SPRINT_DROP).contains(&s)
    }

    pub fn is_cont_fire(s: i32) -> bool {
        (CONT_FIRE_IN..=CONT_FIRE_OUT).contains(&s)
    }

    pub fn is_nightvision(s: i32) -> bool {
        matches!(s, NIGHTVISION_WEAR | NIGHTVISION_REMOVE)
    }

    pub fn is_dtp(s: i32) -> bool {
        (DTP_IN..=DTP_OUT).contains(&s)
    }

    pub fn is_lowready(s: i32) -> bool {
        (LOWREADY_RAISE..=LOWREADY_DROP).contains(&s)
    }

    pub fn is_crawl(s: i32) -> bool {
        (CRAWL_IN..=CRAWL_OUT).contains(&s)
    }

    pub fn is_crawl_loop(s: i32) -> bool {
        (CRAWL_FORWARD..=CRAWL_LEFT).contains(&s)
    }
}

pub mod buttons {
    pub const ATTACK: u32 = 0;
    pub const SPRINT: u32 = 1;
    pub const MELEE: u32 = 2;
    pub const USE: u32 = 3;
    pub const RELOAD: u32 = 4;
    pub const USE_RELOAD: u32 = 5;
    pub const LEAN_LEFT: u32 = 6;
    pub const LEAN_RIGHT: u32 = 7;
    pub const PRONE: u32 = 8;
    pub const CROUCH: u32 = 9;
    pub const JUMP: u32 = 10;
    pub const ADS: u32 = 11;
    pub const STANCE: u32 = 12;
    pub const HOLD_BREATH: u32 = 13;
    pub const FRAG: u32 = 14;
    pub const SMOKE: u32 = 15;
    pub const NIGHTVISION: u32 = 18;
    pub const THROW: u32 = 24;
    pub const CHANGE_ZOOM: u32 = 29;
    /// The throw keys beyond frag and smoke.
    pub const THROW_A: u32 = 30;
    pub const THROW_B: u32 = 31;
    pub const THROW_C: u32 = 32;
    pub const THROW_D: u32 = 33;
    pub const VEHICLE_ATTACK: u32 = 34;
    pub const VEHICLE_ATTACK_ALT: u32 = 35;
    pub const SPECIAL_FIRE: u32 = 38;
    pub const DIVE: u32 = 44;
    /// Cancels a cancelable offhand.
    pub const CANCEL: u32 = 49;
    /// Sets an airburst weapon's distance.
    pub const AIRBURST_RANGE: u32 = 51;
    pub const AIRBURST_ADS: u32 = 52;
}

pub mod view_height {
    pub const STAND: i32 = 60;
    pub const CROUCH: i32 = 40;
    pub const PRONE: i32 = 11;
    pub const DOWNED: i32 = 22;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Buttons(pub [u32; 2]);

impl Buttons {
    pub const fn bit(button: u32) -> u32 {
        0x8000_0000 >> (button & 31)
    }

    pub fn held(self, button: u32) -> bool {
        self.0[(button >> 5) as usize] & Self::bit(button) != 0
    }

    pub fn press(&mut self, button: u32) {
        self.0[(button >> 5) as usize] |= Self::bit(button);
    }

    pub fn release(&mut self, button: u32) {
        self.0[(button >> 5) as usize] &= !Self::bit(button);
    }

    pub fn release_all(&mut self, list: &[u32]) {
        for &button in list {
            self.release(button);
        }
    }

    pub fn keep_only(&mut self, list: &[u32]) {
        let mut kept = [0u32; 2];
        for &button in list {
            kept[(button >> 5) as usize] |= Self::bit(button);
        }
        self.0[0] &= kept[0];
        self.0[1] &= kept[1];
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UserCmd {
    pub server_time: i32,
    pub buttons: Buttons,
    pub angles: [i32; 3],
    pub weapon: u32,
    pub offhand_index: u32,
    pub alt_mode_weapon: u32,
    pub forwardmove: i8,
    pub rightmove: i8,
    /// The entity a melee lunge aims at, `ENTITYNUM_NONE` when none.
    pub melee_charge_ent: u16,
    pub melee_charge_dist: u8,
    /// The client swapped which trigger fires which gun of a dual wield.
    pub fire_buttons_swapped: bool,
    /// An airburst weapon's set distance.
    pub airburst_distance: u16,
}

pub const MAX_HELD_WEAPONS: usize = 15;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HeldWeapon {
    pub weapon: u32,
    /// Percent; overheated at 100.
    pub heat: f32,
    pub fuel: i32,
    /// The weapon's own zoom, 0 or 1.
    pub zoom: i32,
    pub overheating: bool,
    pub needs_rechamber: bool,
    pub used_before: bool,
    /// The second magazine is in.
    pub dual_mag: bool,
    pub riotshield: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerState {
    pub command_time: i32,
    pub pm_type: i32,
    pub bob_cycle: i32,
    pub pm_flags: u32,
    pub weap_flags: u32,
    pub other_flags: u32,
    pub pm_time: i32,
    pub origin: [f32; 3],
    pub velocity: [f32; 3],
    pub foliage_sound_time: i32,
    pub gravity: i32,
    pub leanf: f32,
    pub speed: i32,
    pub delta_angles: [f32; 3],
    pub ground_entity_num: i32,
    pub ground_surface_type: i32,
    pub ladder_vec: [f32; 3],
    pub jump_time: i32,
    pub jump_origin_z: f32,
    pub slick_start_time: i32,
    pub legs_timer: i32,
    pub torso_timer: i32,
    pub damage_timer: i32,
    pub damage_duration: i32,
    /// Where the last hit came from against the view: 1 ahead, 2 behind
    /// (or no direction), 3 the left, 4 the right (the player animation
    /// script's `DMGDIRECTION`).
    pub dmg_direction: i32,
    /// What kind of hit the last one was (the player animation script's
    /// `DMGTYPE`).
    pub dmg_type: i32,
    pub movement_dir: i32,
    pub e_flags: u32,
    pub e_flags2: u32,
    pub client_num: i32,
    pub weapon: u32,
    pub weaponstate: i32,
    pub weaponstate_left: i32,
    pub weapon_pos_frac: f32,
    pub viewangles: [f32; 3],
    pub view_height_target: i32,
    pub view_height_current: f32,
    pub view_height_lerp_time: i32,
    pub view_height_lerp_target: i32,
    pub view_height_lerp_down: i32,
    pub prone_direction: f32,
    pub prone_direction_pitch: f32,
    pub prone_torso_pitch: f32,
    pub sprint_button_up_required: i32,
    pub sprint_exhausted: i32,
    pub last_sprint_start: i32,
    pub last_sprint_end: i32,
    pub sprint_start_max_length: i32,
    pub dive_end_time: i32,
    pub prone_check_torso_pitch: f32,
    pub prone_check_waist_pitch: f32,
    pub move_speed_scale_multiplier: f32,
    pub anim_lock_end: i32,
    pub perks: [u32; 2],
    pub aim_spread_scale: f32,
    pub water_level: i32,
    pub move_disabled: i32,

    pub weapon_time: i32,
    pub weapon_delay: i32,
    pub weapon_time_left: i32,
    pub weapon_delay_left: i32,
    pub weapon_idle_time: i32,
    pub grenade_time_left: i32,
    pub throw_back_grenade_owner: i32,
    pub throw_back_grenade_time_left: i32,
    pub weapon_restrict_kick_time: i32,
    pub left_hand: bool,
    pub offhand_index: u32,
    pub last_weapon_alt_mode_switch: u32,
    pub melee_weapon: u32,
    pub weapon_shot_count: i32,
    pub weapon_shot_count_left: i32,
    pub ads_delay_time: i32,
    pub spread_override: i32,
    pub spread_override_state: i32,
    pub weapon_spin_lerp: f32,
    pub damage_count: i32,
    /// The vehicle the player rides, `ENTITYNUM_NONE` when none.
    pub vehicle_entity: i32,
    pub held_weapons: [HeldWeapon; MAX_HELD_WEAPONS],
    /// Rounds beside the clip, by the held slot of the weapon that owns them.
    pub ammo_not_in_clip: [i32; MAX_HELD_WEAPONS],
    /// Rounds in the clip, by the held slot of the weapon that owns them.
    pub ammo_in_clip: [i32; MAX_HELD_WEAPONS],
    pub cursor_hint_ent_index: i32,
    pub hold_breath_scale: f32,
    pub hold_breath_timer: i32,
    pub charge_shot_timer: i32,
    pub charge_shot_level: i32,
    pub shots_fired_from_chamber: i32,
    pub quick_scope_scale: f32,
    pub quick_scope_timer: i32,
    pub melee_charge_ent: i32,
    pub melee_charge_dist: i32,
    pub melee_charge_time: i32,
    pub weap_lock_flags: u32,
    pub airburst_distance: i32,
    pub inventory_weapon: u32,
    pub weap_anim: u32,
    pub weap_anim_left: u32,
    pub shellshock_index: i32,
    pub shellshock_time: i32,
    pub shellshock_duration: i32,
    pub offhand_throw: i32,
    pub ads_zoom_select: i32,
    pub ads_zoom_time: i32,
    pub ads_zoom_latched: bool,
    pub ads_prev_zoom_select: i32,
    pub ads_prev_zoom_select_time: i32,
    pub intro_shots_fired: i32,
    pub vehicle_pos: i32,
    pub link_flags: u32,
    pub mantle_yaw: f32,
    pub mantle_timer: i32,
    pub mantle_trans_index: i32,
    pub mantle_flags: u32,
    /// The entity the player stands on as it moves (a mantle onto one).
    pub mover_entity_num: i32,
    pub mover_timestamp: i32,
}
