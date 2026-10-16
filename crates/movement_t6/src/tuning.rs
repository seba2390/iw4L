//! Black Ops 2's movement constants: dvar defaults the code reads, and the
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
pub const MOVE_FASTER_SPEED_MULTIPLIER: f32 = 1.07;
pub const NONAME_WALK_SPEED_SCALE: f32 = 1.5;
pub const NONAME_SPRINT_SPEED_SCALE: f32 = 1.5;
pub const SPECTATE_SPEED_SCALE: f32 = 1.0;

pub const SPRINT_TIME: f32 = 4.0;
pub const SPRINT_MIN_TIME: f32 = 1.0;
pub const SPRINT_RECHARGE_PAUSE: f32 = 0.0;
pub const SPRINT_FORWARD_MINIMUM: i32 = 105;
pub const SPRINT_UNLIMITED: bool = false;
pub const PERK_SPRINT_MULTIPLIER: f32 = 2.0;
pub const VEHICLE_PERK_BOOST_DURATION: f32 = 0.74;

pub const FALL_DAMAGE_MIN_HEIGHT: f32 = 128.0;
pub const FALL_DAMAGE_MAX_HEIGHT: f32 = 300.0;

pub const DMGTIMER_TIME_PER_POINT: f32 = 100.0;
pub const DMGTIMER_MAX_TIME: f32 = 750.0;
pub const DMGTIMER_MIN_SCALE: f32 = 0.0;
pub const DMGTIMER_FLINCH_TIME: i32 = 500;

pub const VIEW_PITCH_UP: f32 = 85.0;
pub const VIEW_PITCH_DOWN: f32 = 85.0;
pub const PRONE_YAWCAP: f32 = 45.0;
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

pub const DISABLE_WEAPONS_IN_WATER: bool = true;
pub const CLIP_SIZE_MULTIPLIER: f32 = 1.0;
pub const PERK_WEAP_RELOAD_MULTIPLIER: f32 = 0.5;
pub const PERK_WEAP_RATE_MULTIPLIER: f32 = 0.75;
pub const PERK_WEAP_SWITCH_MULTIPLIER: f32 = 0.5;
pub const PERK_WEAP_TOSS_MULTIPLIER: f32 = 0.5;
pub const PERK_WEAP_EQUIPMENT_USE_MULTIPLIER: f32 = 0.6;
pub const PERK_WEAP_ADS_MULTIPLIER: f32 = 0.5;
pub const PERK_SPRINT_RECOVERY_MULTIPLIER: f32 = 0.3;
pub const PERK_FAST_LADDER_CLIMB_MULTIPLIER: f32 = 1.75;
pub const PERK_EXTRA_BREATH: f32 = 5.0;
pub const WATER_SPEED_SCALE: f32 = 1.3;
pub const MELEE_RANGE: f32 = 55.0;
pub const BAYONET_RANGE: f32 = 85.0;
pub const BREATH_HOLD_TIME: f32 = 4.5;
pub const BREATH_FIRE_DELAY: f32 = 0.0;
pub const BREATH_GASP_TIME: f32 = 0.0;
pub const BREATH_GASP_SCALE: f32 = 2.5;
pub const BREATH_GASP_LERP: f32 = 4.0;
pub const BREATH_HOLD_LERP: f32 = 1.5;
pub const DOOR_BREACH_WEAPON_DROP: bool = true;
pub const MANTLE_ENABLE: bool = true;
pub const MANTLE_ADJUSTMENT_TU: bool = false;
pub const MANTLE_CHECK_ANGLE: f32 = 60.0;
pub const MANTLE_CHECK_GLASS_EXTRA_RANGE: f32 = 10.0;
pub const MANTLE_CHECK_RADIUS: f32 = 0.1;
pub const MANTLE_CHECK_RANGE: f32 = 20.0;
pub const MANTLE_CHECK_RANGE_MP: f32 = 10.0;
pub const MANTLE_VIEW_YAWCAP: f32 = 60.0;
pub const HATCH_MANTLE_FWD: f32 = 44.0;
pub const HATCH_MANTLE_UP: f32 = 10.0;
pub const PERK_MANTLE_REDUCTION: f32 = 0.4;
pub const PRONE_BIPOD_ENABLE: bool = true;
pub const SCOPE_EXIT_ON_DAMAGE: bool = false;
pub const ADS_EXIT_DELAY: i32 = 0;
pub const ADS_ZERO_SPREAD: bool = true;
pub const ANTI_QUICK_SCOPE: bool = true;
pub const AIM_SPREAD_MOVE_SPEED_THRESHOLD: f32 = 11.0;
pub const BAYONET_LAUNCH_PROOF: bool = true;
pub const BAYONET_LAUNCH_Z_CAP: f32 = 300.0;
pub const DISABLE_WEAPONS_ON_VEHICLE: bool = true;
pub const ENFORCE_RECHAMBER_ON_SWITCH: bool = true;
pub const ZM_STACK_FIRE_FIX: bool = true;
pub const ZM_STOP_FIRING_WHEN_OVERHEATED: bool = true;
pub const PLAYER_JUMP_BLOCKS_TAC_INSERT: bool = false;
pub const PREVENT_STARTING_CHARGE_SHOT_WHILE_FIRING: bool = true;
pub const CHARGE_SHOT_EXPONENTIAL_AMMO: bool = false;
pub const CHARGE_SHOT_ONE_AMMO_FOR_MULTIPLE_BULLETS: bool = false;
pub const CHARGE_SHOT_MAX_BULLETS_IN_QUEUE: i32 = 3;
pub const CHARGE_SHOT_MAX_BULLET_QUEUE_ON_EMP: i32 = 1;
pub const CHARGE_SHOT_QUEUE_TIME: i32 = 500;
pub const CHARGE_SHOT_AUTO_FIRE_DELAY: i32 = 0;
pub const CHARGE_SHOT_AUTO_DISCHARGE_DELAY: i32 = 0;
pub const CHARGE_SHOT_DISCHARGE_WHEN_QUEUE_REACHES_MAX: bool = false;
pub const CHARGE_SHOT_FIRE_WHEN_QUEUE_REACHES_MAX: bool = false;
pub const STANDING_VIEW_HEIGHT: i32 = 60;
pub const RIOTSHIELD_DEPLOY_ZDIFF_MAX: f32 = 30.0;
pub const RIOTSHIELD_DEPLOY_PITCH_MAX: f32 = 20.0;
pub const RIOTSHIELD_DEPLOY_ROLL_MAX: f32 = 25.0;
