pub const ENTITYNUM_WORLD: i32 = 1022;
pub const ENTITYNUM_NONE: i32 = 1023;

pub mod pm_type {
    pub const NORMAL: i32 = 0;
    pub const NORMAL_LINKED: i32 = 1;
    pub const NOCLIP: i32 = 2;
    pub const UFO: i32 = 3;
    pub const SPECTATOR: i32 = 4;
    pub const INTERMISSION: i32 = 5;
    pub const LAST_STAND: i32 = 6;
    pub const LAST_STAND_REVIVED: i32 = 7;
    pub const LAST_STAND_GETTING_UP: i32 = 8;
    pub const DEAD: i32 = 9;
    pub const DEAD_LINKED: i32 = 10;
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
    pub const LOW_READY: u32 = 0x20_0000;
    pub const NO_STAND: u32 = 0x40_0000;
    pub const NO_CROUCH: u32 = 0x80_0000;
    pub const NO_PRONE: u32 = 0x100_0000;
    pub const NO_LEAN: u32 = 0x200_0000;
    pub const VIEW_LINKED: u32 = 0x400_0000;
    pub const NO_MELEE: u32 = 0x800_0000;
    pub const PRONE_YAW_LOCKED: u32 = 0x1000_0000;
    pub const DIVING: u32 = 0x2000_0000;
    pub const SLIDING: u32 = 0x4000_0000;
    /// Breaching a door: the weapon goes down.
    pub const DOOR_BREACH: u32 = 0x8000_0000;
}

pub mod e_flags {
    pub const CROUCHING: u32 = 0x4;
    pub const PRONE: u32 = 0x8;
    pub const TURRET_PRONE: u32 = 0x100;
    pub const TURRET_CROUCH: u32 = 0x200;
    pub const TURRET: u32 = TURRET_PRONE | TURRET_CROUCH;
    pub const VEHICLE_VIEW: u32 = 0x4000;
    pub const TALKING: u32 = 0x2_0000;
}

/// The player state's weapon flags.
pub mod weap_flags {
    /// A reload asked for by script.
    pub const RELOAD_REQUESTED: u32 = 0x1;
    /// The offhand is in hand.
    pub const OFFHAND_ACTIVE: u32 = 0x2;
    pub const HOLD_BREATH: u32 = 0x4;
    /// A grenade throw is cancelled.
    pub const CANCEL_GRENADE: u32 = 0x8;
    pub const NO_ADS: u32 = 0x20;
    pub const NIGHTVISION: u32 = 0x40;
    /// `disableweapons`.
    pub const WEAPONS_DISABLED: u32 = 0x80;
    /// `disableoffhandweapons`.
    pub const OFFHANDS_DISABLED: u32 = 0x100;
    /// `disableweaponcycling`.
    pub const CYCLING_DISABLED: u32 = 0x200;
    /// The fire button must be let go before the next shot.
    pub const FIRE_LATCH: u32 = 0x400;
    pub const DEPLOY: u32 = 0x800;
    pub const BREAKDOWN: u32 = 0x1000;
    /// The weapon went away for swimming.
    pub const SWIM_PUTAWAY: u32 = 0x2000;
    pub const GUIDED_MISSILE: u32 = 0x8000;
    pub const RELOAD_LEFT: u32 = 0x1_0000;
    /// The offhand is thrown with the weapon away.
    pub const OFFHAND_SWITCH: u32 = 0x2_0000;
    /// The offhand was thrown with no weapon raised.
    pub const OFFHAND_EMPTY_HANDED: u32 = 0x8_0000;
    pub const FIRING_GUIDED: u32 = 0x10_0000;
    /// Scripts force a viewmodel animation.
    pub const FORCED_ANIM: u32 = 0x40_0000;
    /// A smoke throw is cancelled.
    pub const CANCEL_SMOKE: u32 = 0x100_0000;
    pub const NO_FIRE: u32 = 0x200_0000;
}

/// The second word of entity flags.
pub mod e_flags2 {
    /// Aiming is dropped at once.
    pub const ADS_RESET: u32 = 0x4_0000;
    /// A minigun's barrels spin up.
    pub const SPINNING: u32 = 0x40_0000;
    /// A grenade throw cannot be cancelled now.
    pub const GRENADE_HELD: u32 = 0x2000_0000;
    pub const MOUNTED: u32 = 0x1000_0000;
    pub const CONTROLS_LOCKED: u32 = 0x4000_0000;
}

pub mod weapon_state {
    pub const DEPLOYING: i32 = 0x24;
    pub const BREAKING_DOWN: i32 = 0x25;
    pub const DEPLOYED: i32 = 0x26;
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
    /// A stance key is held: blocked stance changes say nothing.
    pub const STANCE: u32 = 12;
    pub const HOLD_BREATH: u32 = 13;
    pub const FRAG: u32 = 14;
    pub const SMOKE: u32 = 15;
    pub const NIGHTVISION: u32 = 18;
    pub const THROW: u32 = 24;
    pub const DIVE: u32 = 44;
}

/// Black Ops' view heights: they also say which stance a player is in.
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
    /// The weapon the player asks to hold.
    pub weapon: u32,
    /// The offhand the player asks to throw.
    pub offhand_index: u32,
    /// The alternate weapon the player switched from.
    pub alt_mode_weapon: u32,
    pub forwardmove: i8,
    pub rightmove: i8,
    /// Where a melee lunge aims, found by the client.
    pub melee_charge_yaw: f32,
    pub melee_charge_dist: u8,
}

/// The weapons held, by slot.
pub const MAX_HELD_WEAPONS: usize = 15;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeldWeapon {
    pub weapon: u32,
    /// What a fuel tank weapon has burnt.
    pub fuel: i32,
    pub needs_rechamber: bool,
    /// Raised before: the first raise animation has played.
    pub used_before: bool,
    /// The second magazine of a dual-magazine weapon is in.
    pub dual_mag: bool,
}

/// Ammo by its shared number: rounds in clips, or rounds beside them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AmmoSlot {
    pub index: i32,
    pub count: i32,
}

/// The part of a Black Ops player's state its movement reads and writes.
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
    pub launch_time: i32,
    pub move_speed_scale_multiplier: f32,
    pub anim_lock_end: i32,
    pub perks: u32,
    pub aim_spread_scale: f32,
    pub water_level: i32,
    /// Set while something else steers the player: moves and most buttons are ignored.
    pub move_disabled: i32,

    pub weapon_time: i32,
    pub weapon_delay: i32,
    pub weapon_time_left: i32,
    pub weapon_delay_left: i32,
    pub grenade_time_left: i32,
    pub throw_back_grenade_owner: i32,
    pub throw_back_grenade_time_left: i32,
    pub weapon_restrict_kick_time: i32,
    /// The left hand's pass of the weapon rules is running.
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
    pub stack_fire_count: i32,
    pub damage_count: i32,
    pub held_weapons: [HeldWeapon; MAX_HELD_WEAPONS],
    pub ammo_not_in_clip: [AmmoSlot; MAX_HELD_WEAPONS],
    pub ammo_in_clip: [AmmoSlot; MAX_HELD_WEAPONS],
    pub cursor_hint_ent_index: i32,
    pub hold_breath_scale: f32,
    pub hold_breath_timer: i32,
    pub melee_charge_yaw: f32,
    pub melee_charge_dist: i32,
    pub melee_charge_time: i32,
    pub weap_lock_flags: u32,
    /// A viewmodel animation scripts force: the weapon, its state, and the
    /// weapon held before.
    pub forced_anim_weapon: u32,
    pub forced_anim_state: i32,
    pub forced_anim_prev_weapon: u32,
    pub weap_anim: u32,
    pub weap_anim_left: u32,
    /// How the held offhand is thrown.
    pub offhand_throw: i32,
    pub ads_zoom_select: i32,
    pub ads_zoom_time: i32,
    pub ads_zoom_latched: bool,
    pub vehicle_pos: i32,
    pub mantle_flags: u32,
}
