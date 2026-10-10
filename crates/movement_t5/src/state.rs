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
    pub const MOUNTED_SPEED: u32 = 0x1_0000;
    pub const LAUNCHED: u32 = 0x2_0000;
    pub const SPRINT_DISABLED: u32 = 0x4_0000;
    pub const NO_JUMP: u32 = 0x8_0000;
    pub const ANIM_LOCK: u32 = 0x10_0000;
    pub const NO_STAND: u32 = 0x40_0000;
    pub const NO_CROUCH: u32 = 0x80_0000;
    pub const NO_PRONE: u32 = 0x100_0000;
    pub const NO_LEAN: u32 = 0x200_0000;
    pub const VIEW_LINKED: u32 = 0x400_0000;
    pub const PRONE_YAW_LOCKED: u32 = 0x1000_0000;
    pub const DIVING: u32 = 0x2000_0000;
    pub const SLIDING: u32 = 0x4000_0000;
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

/// The player state's other flags.
pub mod other_flags {
    pub const NO_ADS: u32 = 0x20;
}

/// The second word of entity flags.
pub mod e_flags2 {
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
    pub forwardmove: i8,
    pub rightmove: i8,
}

/// The part of a Black Ops player's state its movement reads and writes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerState {
    pub command_time: i32,
    pub pm_type: i32,
    pub bob_cycle: i32,
    pub pm_flags: u32,
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
}
