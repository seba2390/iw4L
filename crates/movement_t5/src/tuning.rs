//! Black Ops' movement constants: dvar defaults the code reads, and the
//! values it reads through fixed getters.

pub const STOPSPEED: f32 = 100.0;
pub const FRICTION: f32 = 5.5;
pub const SLIDING_FRICTION: f32 = 1.5;
pub const SLIDING_WISHSPEED: f32 = 450.0;
pub const SLIDING_VELOCITY_CAP: f32 = 500.0;
pub const GRAVITY: f32 = 800.0;

pub const JUMP_HEIGHT: f32 = 39.0;
pub const JUMP_STEP_SIZE: f32 = 18.0;
pub const JUMP_SPREAD_ADD: f32 = 64.0;
pub const JUMP_LADDER_PUSH_VEL: f32 = 128.0;
pub const JUMP_SLOWDOWN_ENABLE: bool = false;

pub const BACK_SPEED_SCALE: f32 = 0.7;
pub const STRAFE_SPEED_SCALE: f32 = 0.8;
pub const SPRINT_SPEED_SCALE: f32 = 1.5;
pub const SPRINT_STRAFE_SPEED_SCALE: f32 = 0.666999996;
pub const ENDURANCE_SPEED_SCALE: f32 = 1.7;
pub const SPECTATE_SPEED_SCALE: f32 = 1.0;

pub const SPRINT_TIME: f32 = 4.0;
pub const SPRINT_MIN_TIME: f32 = 1.0;
pub const SPRINT_RECHARGE_PAUSE: f32 = 0.0;
pub const SPRINT_FORWARD_MINIMUM: i32 = 105;
pub const SPRINT_UNLIMITED: bool = false;
pub const PERK_SPRINT_MULTIPLIER: f32 = 2.0;
pub const VEHICLE_PERK_BOOST_DURATION: f32 = 0.74;

pub const FALL_DAMAGE_MIN_HEIGHT: f32 = 128.0;
pub const FALL_DAMAGE_MAX_HEIGHT: f32 = 350.0;

pub const DMGTIMER_MAX_TIME: f32 = 750.0;
pub const DMGTIMER_MIN_SCALE: f32 = 0.0;
pub const DMGTIMER_FLINCH_TIME: i32 = 500;

pub const VIEW_PITCH_UP: f32 = 85.0;
pub const VIEW_PITCH_DOWN: f32 = 85.0;
pub const PRONE_YAWCAP: f32 = 85.0;
pub const LADDER_YAWCAP: f32 = 100.0;

pub const MOVE_THRESHOLD: f32 = 2.0;
pub const RUN_THRESHOLD: f32 = 110.0;
pub const RUNBK_THRESHOLD: f32 = 60.0;
pub const SPRINT_THRESHOLD: f32 = 185.0;
pub const ANIM_WALK_THRESHOLD: f32 = 30.0;
pub const ANIM_RUN_THRESHOLD: f32 = 140.0;
pub const ENABLE_SHUFFLE_ANIMS: bool = false;
pub const PLAY_STAND_TO_CROUCH_ANIMS: bool = false;
pub const SPRINT_CAMERA_BOB: f32 = 0.5;
pub const WEAPON_BOB_FREQUENCY_SWIMMING: f32 = 0.1;

pub const FOLIAGE_MIN_SPEED: f32 = 40.0;
pub const FOLIAGE_MAX_SPEED: f32 = 180.0;
pub const FOLIAGE_SLOW_INTERVAL: i32 = 1500;
pub const FOLIAGE_FAST_INTERVAL: i32 = 500;
pub const FOLIAGE_RESET_INTERVAL: i32 = 500;
pub const FOLIAGE_CONTENTS: u32 = 0x2;

pub const DTP: bool = true;
pub const DTP_EXHAUSTION_WINDOW: f32 = 1500.0;
pub const DTP_FALL_DAMAGE_MIN_HEIGHT: f32 = 65.0;
pub const DTP_FALL_DAMAGE_MAX_HEIGHT: f32 = 200.0;
pub const DTP_MAX_APEX_DURATION: i32 = 400;
pub const DTP_MAX_SLIDE_ADDITION: f32 = 0.0;
pub const DTP_MAX_SLIDE_DURATION: f32 = 300.0;
pub const DTP_MIN_SPEED: f32 = 3.16;
pub const DTP_NEW_TRAJECTORY: bool = true;
pub const DTP_NEW_TRAJECTORY_MULTIPLIER: f32 = 2.0;
pub const DTP_POST_MOVE_PAUSE: f32 = 100.0;
pub const DTP_STARTUP_DELAY: f32 = 250.0;
pub const DTP_SLIDE_CONTENTS: u32 = 0x0100_0000;

pub const MAX_CLIENTS: i32 = 4;
pub const ZOMBIETRON: bool = false;

// Weapons.
pub const DISABLE_WEAPONS_IN_WATER: bool = true;
pub const CLIP_SIZE_MULTIPLIER: f32 = 1.0;
pub const PERK_WEAP_RELOAD_MULTIPLIER: f32 = 0.5;
pub const PERK_WEAP_RATE_MULTIPLIER: f32 = 0.75;
pub const BURST_FIRE_COOLDOWN: f32 = 0.2;
pub const MELEE_RANGE: f32 = 64.0;
pub const BAYONET_RANGE: f32 = 85.0;
pub const BREATH_HOLD_TIME: f32 = 4.5;
pub const BREATH_FIRE_DELAY: f32 = 0.0;
pub const BREATH_GASP_TIME: f32 = 1.0;
pub const BREATH_GASP_SCALE: f32 = 4.5;
pub const BREATH_GASP_LERP: f32 = 6.0;
pub const BREATH_HOLD_LERP: f32 = 4.0;
pub const DOOR_BREACH_WEAPON_DROP: bool = true;
pub const MANTLE_ENABLE: bool = true;
pub const SCOPE_EXIT_ON_DAMAGE: bool = false;
pub const ADS_EXIT_DELAY: i32 = 0;
pub const AIM_SPREAD_MOVE_SPEED_THRESHOLD: f32 = 11.0;
pub const BAYONET_LAUNCH_DEBUGGING: bool = false;
pub const BAYONET_LAUNCH_PROOF: bool = true;
pub const BAYONET_LAUNCH_Z_CAP: f32 = 300.0;
