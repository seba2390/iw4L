//! Black Ops' view kick, as its client runs it for its own player: a shot
//! draws the view's and the gun's kick from the weapon's hip or ADS ranges,
//! each frame eases the view's kick back to centre in steps of at most
//! 5 ms, and the kick turns the angles the client's commands aim by.

use fastfile_t5::size as sz;
use game_api::fire::{KickWeapon, KickWeapons, ViewKick, ViewKickRules};
use game_api::movement::MovePlayer;
use movement_t5::PlayerState;
use movement_t5::weapon::state as ws;

pub(crate) const RULES: ViewKickRules = ViewKickRules { fire, frame, aim };

/// Black Ops multiplayer's: its kick tests no perk.
pub(crate) const MP_RULES: ViewKickRules = ViewKickRules {
    aim: mp_aim,
    ..RULES
};

/// `weapType` gas: its kick builds while it fires and does not ease back.
const WEAPTYPE_GAS: i32 = 4;
/// The view kick's ease back with no weapon, in degrees per second.
const NO_WEAPON_CENTER_SPEED: f32 = 2400.0;
/// The view kick's largest angle.
const KICK_LIMIT: f32 = 10.0;
/// A step moving the kick back towards centre goes this share as far.
const KICK_RETURN_SCALE: f32 = 0.06;
/// The longest step of the ease back (ms).
const KICK_STEP_MS: i32 = 5;
/// The longest step of the gun kick spring (s).
const GUN_KICK_STEP: f32 = 0.005;
/// The smallest gun kick angle and speed the spring keeps moving.
const GUN_KICK_MIN_ANGLE: f32 = 0.25;
const GUN_KICK_MIN_SPEED: f32 = 1.0;
/// A gas weapon's kick velocity on its first shot.
const GAS_KICK_STEP: f32 = 0.001;
/// The roll a shot's yaw kick gives, per degree.
const ROLL_PER_YAW: f32 = -0.5;
/// Deadshot's share of the kick in a scoped weapon's sights.
const DEADSHOT_KICK_SCALE: f32 = 0.25;

fn f(bytes: &[u8], off: usize) -> f32 {
    bytes
        .get(off..off + 4)
        .map_or(0.0, |b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn i(bytes: &[u8], off: usize) -> i32 {
    bytes
        .get(off..off + 4)
        .map_or(0, |b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn firing(state: i32) -> bool {
    matches!(state, ws::FIRING | ws::CONT_FIRE_LOOP)
}

/// The weapon the viewmodel shows: the offhand while one is thrown, the
/// melee weapon in a blow with a weapon that is neither a bayonet nor used
/// as a melee weapon, else the held weapon.
fn view_weapon(ps: &PlayerState, weapons: KickWeapons<'_>) -> u32 {
    use movement_t5::state::weap_flags::OFFHAND_ACTIVE;
    let s = ps.weaponstate;
    if ps.weap_flags & OFFHAND_ACTIVE != 0 && ws::OFFHAND_INIT < s && s < ws::DETONATING {
        return ps.offhand_index;
    }
    if matches!(s, ws::MELEE_INIT | ws::MELEE_FIRE) {
        let plain = weapons(ps.weapon).is_some_and(|w| {
            fastfile_t5::weapon_def::WeaponDefView::new(&w.def, &w.variant)
                .is_some_and(|d| !d.bayonet() && !d.use_as_melee())
        });
        if plain {
            return ps.melee_weapon;
        }
    }
    ps.weapon
}

/// A kick range: (pitch min, pitch max, yaw min, yaw max) offsets.
type Range = [usize; 4];
const ADS_VIEW: Range = [
    sz::WEAPON_DEF_ADS_VIEW_KICK_PITCH_MIN_OFF,
    sz::WEAPON_DEF_ADS_VIEW_KICK_PITCH_MAX_OFF,
    sz::WEAPON_DEF_ADS_VIEW_KICK_YAW_MIN_OFF,
    sz::WEAPON_DEF_ADS_VIEW_KICK_YAW_MAX_OFF,
];
const HIP_VIEW: Range = [
    sz::WEAPON_DEF_HIP_VIEW_KICK_PITCH_MIN_OFF,
    sz::WEAPON_DEF_HIP_VIEW_KICK_PITCH_MAX_OFF,
    sz::WEAPON_DEF_HIP_VIEW_KICK_YAW_MIN_OFF,
    sz::WEAPON_DEF_HIP_VIEW_KICK_YAW_MAX_OFF,
];
const ADS_GUN: Range = [
    sz::WEAPON_DEF_ADS_GUN_KICK_PITCH_MIN_OFF,
    sz::WEAPON_DEF_ADS_GUN_KICK_PITCH_MAX_OFF,
    sz::WEAPON_DEF_ADS_GUN_KICK_YAW_MIN_OFF,
    sz::WEAPON_DEF_ADS_GUN_KICK_YAW_MAX_OFF,
];
const HIP_GUN: Range = [
    sz::WEAPON_DEF_HIP_GUN_KICK_PITCH_MIN_OFF,
    sz::WEAPON_DEF_HIP_GUN_KICK_PITCH_MAX_OFF,
    sz::WEAPON_DEF_HIP_GUN_KICK_YAW_MIN_OFF,
    sz::WEAPON_DEF_HIP_GUN_KICK_YAW_MAX_OFF,
];

/// `rand · (max − min) + min` for the pitch, then the yaw.
fn draw(def: &[u8], range: Range, rand: &mut dyn FnMut() -> f32) -> (f32, f32) {
    let (pitch_min, yaw_min) = (f(def, range[0]), f(def, range[2]));
    let pitch = rand() * (f(def, range[1]) - pitch_min) + pitch_min;
    let yaw = rand() * (f(def, range[3]) - yaw_min) + yaw_min;
    (pitch, yaw)
}

/// Both hands' ranges summed: `(rand · ((lmax + rmax) − (lmin + rmin)) +
/// rmin) + lmin`.
fn draw_both(left: &[u8], right: &[u8], range: Range, rand: &mut dyn FnMut() -> f32) -> (f32, f32) {
    let mut one = |min_off: usize, max_off: usize| {
        let (lmin, rmin) = (f(left, min_off), f(right, min_off));
        let r = rand();
        let span = (f(left, max_off) + f(right, max_off)) - (rmin + lmin);
        (r * span + rmin) + lmin
    };
    let pitch = one(range[0], range[1]);
    let yaw = one(range[2], range[3]);
    (pitch, yaw)
}

/// The hip draw of a dual-wield weapon: both hands' ranges while both
/// fire, the left hand's while it alone fires, else the right's.
fn draw_hip(
    ps: &PlayerState,
    def: &[u8],
    left: Option<&KickWeapon>,
    range: Range,
    rand: &mut dyn FnMut() -> f32,
) -> (f32, f32) {
    match left {
        Some(left) if firing(ps.weaponstate) && firing(ps.weaponstate_left) => {
            draw_both(&left.def, def, range, rand)
        }
        Some(left) if firing(ps.weaponstate_left) => draw(&left.def, range, rand),
        _ => draw(def, range, rand),
    }
}

/// `pm_flags` of a player on a turret or in a vehicle: its shots kick nothing.
const PMF_MOUNTED: u32 = 0x300;

/// A shot: the view's kick velocity is set from the hip or (in full ADS)
/// the ADS view kick range, the gun's kick velocity grows by the hip or (in
/// any ADS) the ADS gun kick range; both are cut to the weapon's reduced
/// kick share while the kick is restricted.
fn fire(
    player: &MovePlayer,
    weapons: KickWeapons<'_>,
    rand: &mut dyn FnMut() -> f32,
    kick: &mut ViewKick,
) {
    let (ps, _) = crate::movement::unpack(&mut player.clone());
    if ps.pm_flags & PMF_MOUNTED != 0 {
        return;
    }
    let Some(weapon) = weapons(view_weapon(&ps, weapons)) else {
        return;
    };
    let def = &weapon.def[..];
    let left = weapon
        .dual_wield
        .filter(|_| {
            def.get(sz::WEAPON_DEF_DUAL_WIELD_OFF)
                .is_some_and(|&b| b != 0)
        })
        .and_then(weapons);
    let frac = ps.weapon_pos_frac;
    let mut scale = 1.0;
    if ps.weapon_restrict_kick_time > 0 {
        let percent = if frac == 1.0 {
            f(def, sz::WEAPON_DEF_ADS_GUN_KICK_REDUCED_PERCENT_OFF)
        } else {
            f(def, sz::WEAPON_DEF_HIP_GUN_KICK_REDUCED_PERCENT_OFF)
        };
        scale = percent * 0.01;
    }
    let (pitch, yaw) = if frac == 1.0 {
        draw(def, ADS_VIEW, rand)
    } else {
        draw_hip(&ps, def, left.as_ref(), HIP_VIEW, rand)
    };
    let (pitch, yaw) = (pitch * scale, yaw * scale);
    if i(def, sz::WEAPON_TYPE_OFF) == WEAPTYPE_GAS && kick.velocity[0] == 0.0 {
        kick.velocity[0] -= GAS_KICK_STEP;
    } else {
        kick.velocity[0] = -pitch;
    }
    kick.velocity[1] = yaw;
    kick.velocity[2] = yaw * ROLL_PER_YAW;
    let (pitch, yaw) = if frac > 0.0 {
        draw(def, ADS_GUN, rand)
    } else {
        draw_hip(&ps, def, left.as_ref(), HIP_GUN, rand)
    };
    kick.gun_velocity[0] += pitch * scale;
    kick.gun_velocity[1] += yaw * scale;
}

/// The view kick's ease back towards centre, in degrees per second: the
/// weapon variant's ADS centre speed past half ADS, else its hip centre
/// speed (both hands' summed while both fire, the left's while it alone
/// fires); none for a gas weapon while it fires.
fn center_speed(ps: &PlayerState, weapons: KickWeapons<'_>, weapon: u32) -> f32 {
    if weapon == 0 {
        return NO_WEAPON_CENTER_SPEED;
    }
    let Some(own) = weapons(weapon) else {
        return 0.0;
    };
    let ads = |w: &KickWeapon| {
        f(
            &w.variant,
            sz::WEAPON_VARIANT_ADS_VIEW_KICK_CENTER_SPEED_OFF,
        )
    };
    let hip = |w: &KickWeapon| {
        f(
            &w.variant,
            sz::WEAPON_VARIANT_HIP_VIEW_KICK_CENTER_SPEED_OFF,
        )
    };
    if ps.weapon_pos_frac > 0.5 {
        return ads(&own);
    }
    let dual = own
        .def
        .get(sz::WEAPON_DEF_DUAL_WIELD_OFF)
        .is_some_and(|&b| b != 0);
    if dual {
        let left = own.dual_wield.and_then(weapons);
        return match left {
            Some(left) if firing(ps.weaponstate) && firing(ps.weaponstate_left) => {
                hip(&left) + hip(&own)
            }
            Some(left) if firing(ps.weaponstate_left) => hip(&left),
            _ => hip(&own),
        };
    }
    // A gas weapon that overheats keeps easing back while it is hot; no
    // Black Ops zombies weapon is one, and the runtime keeps no heat.
    if i(&own.def, sz::WEAPON_TYPE_OFF) == WEAPTYPE_GAS && firing(ps.weaponstate) {
        return 0.0;
    }
    hip(&own)
}

/// A frame: in steps of at most 5 ms, each axis away from centre speeds
/// back towards it, then moves by its velocity (6% as far when that is
/// back towards centre), stopping at centre and at the limit; a stop drops
/// its velocity.
fn frame(player: &MovePlayer, weapons: KickWeapons<'_>, frametime_ms: i32, kick: &mut ViewKick) {
    let (ps, _) = crate::movement::unpack(&mut player.clone());
    let weapon = view_weapon(&ps, weapons);
    let mut left = frametime_ms;
    while left >= 1 {
        let t = left.min(KICK_STEP_MS) as f32 * 0.001;
        for axis in 0..3 {
            let angle = kick.angles[axis];
            if angle != 0.0 {
                let toward = if angle > 0.0 { -1.0 } else { 1.0 };
                let speed = center_speed(&ps, weapons, weapon) * toward;
                kick.velocity[axis] += speed * t;
            } else if kick.velocity[axis] == 0.0 {
                continue;
            }
            let angle = kick.angles[axis];
            let mut step = kick.velocity[axis] * t;
            if angle * step < 0.0 {
                step *= KICK_RETURN_SCALE;
            }
            let moved = angle + step;
            if moved * angle < 0.0 {
                kick.angles[axis] = 0.0;
            } else if moved != 0.0 && moved.abs() <= KICK_LIMIT {
                kick.angles[axis] = moved;
                continue;
            } else if moved == 0.0 {
                kick.angles[axis] = 0.0;
            } else {
                kick.angles[axis] = if moved > 0.0 { KICK_LIMIT } else { -KICK_LIMIT };
            }
            kick.velocity[axis] = 0.0;
        }
        left -= KICK_STEP_MS;
    }
    gun_kick(&ps, weapons, weapon, frametime_ms, kick);
}

/// The gun's own recoil on the viewmodel: a weapon with sights steps its
/// pitch and yaw springs in slices of at most 5 ms (accelerated back to
/// centre, damped by its speed and static decays, and clamped to its max
/// gun pitch/yaw and speed); a hip-only weapon subtracts the view's kick
/// angles instead. Switching weapons zeroes the gun kick.
fn gun_kick(
    ps: &PlayerState,
    weapons: KickWeapons<'_>,
    weapon: u32,
    frametime_ms: i32,
    kick: &mut ViewKick,
) {
    if weapon != kick.weapon {
        kick.weapon = weapon;
        kick.gun_angles = [0.0; 2];
        kick.gun_velocity = [0.0; 2];
    }
    let Some(own) = weapons(weapon) else {
        kick.viewmodel_angles = [0.0; 3];
        return;
    };
    let def = &own.def[..];
    let has_ads = def
        .get(sz::WEAPON_DEF_AIM_DOWN_SIGHT_OFF)
        .is_some_and(|&b| b != 0);
    if !has_ads {
        kick.viewmodel_angles = kick.angles.map(|a| -a);
        return;
    }
    let frac = ps.weapon_pos_frac;
    let lerp = |hip_off: usize, ads_off: usize| {
        let hip = f(def, hip_off);
        (f(def, ads_off) - hip) * frac + hip
    };
    let accel = lerp(
        sz::WEAPON_DEF_HIP_GUN_KICK_ACCEL_OFF,
        sz::WEAPON_DEF_ADS_GUN_KICK_ACCEL_OFF,
    );
    let speed_max = lerp(
        sz::WEAPON_DEF_HIP_GUN_KICK_SPEED_MAX_OFF,
        sz::WEAPON_DEF_ADS_GUN_KICK_SPEED_MAX_OFF,
    );
    let speed_decay = lerp(
        sz::WEAPON_DEF_HIP_GUN_KICK_SPEED_DECAY_OFF,
        sz::WEAPON_DEF_ADS_GUN_KICK_SPEED_DECAY_OFF,
    );
    let static_decay = lerp(
        sz::WEAPON_DEF_HIP_GUN_KICK_STATIC_DECAY_OFF,
        sz::WEAPON_DEF_ADS_GUN_KICK_STATIC_DECAY_OFF,
    );
    let limits = [
        f(def, sz::WEAPON_DEF_GUN_MAX_PITCH_OFF),
        f(def, sz::WEAPON_DEF_GUN_MAX_YAW_OFF),
    ];
    let mut rem = frametime_ms as f32 * 0.001;
    while rem > 0.0 {
        let (dt, next_rem) = if rem <= GUN_KICK_STEP {
            (rem, 0.0)
        } else {
            (GUN_KICK_STEP, rem - GUN_KICK_STEP)
        };
        let mut pitch_stopped = false;
        for (axis, max) in limits.into_iter().enumerate() {
            let mut angle = kick.gun_angles[axis];
            let mut vel = kick.gun_velocity[axis];
            if angle.abs() >= GUN_KICK_MIN_ANGLE || vel.abs() >= GUN_KICK_MIN_SPEED {
                angle += vel * dt;
                if angle > max {
                    angle = max;
                    if !(vel < 0.0) && vel != 0.0 {
                        vel = 0.0;
                    }
                } else if angle < -max {
                    angle = -max;
                    if !(0.0 < vel) && vel != 0.0 {
                        vel = 0.0;
                    }
                }
                if angle > 0.0 {
                    vel -= dt * accel;
                } else if angle < 0.0 {
                    vel += dt * accel;
                }
                vel -= vel * dt * speed_decay;
                let step = dt * static_decay;
                if vel <= 0.0 {
                    vel += step;
                    if !(vel < 0.0) && vel != 0.0 {
                        vel = 0.0;
                    }
                } else {
                    vel -= step;
                    if !(0.0 < vel) && vel != 0.0 {
                        vel = 0.0;
                    }
                }
                if vel > speed_max {
                    vel = speed_max;
                } else if vel < -speed_max {
                    vel = -speed_max;
                }
                kick.gun_angles[axis] = angle;
                kick.gun_velocity[axis] = vel;
            } else {
                kick.gun_angles[axis] = 0.0;
                kick.gun_velocity[axis] = 0.0;
                if axis == 0 {
                    pitch_stopped = true;
                } else if pitch_stopped {
                    break;
                }
            }
        }
        rem = next_rem;
    }
    kick.viewmodel_angles = [kick.gun_angles[0], kick.gun_angles[1], 0.0];
}

fn mp_aim(_player: &MovePlayer, _weapons: KickWeapons<'_>, kick: &ViewKick) -> [f32; 3] {
    kick.angles
}

/// The aim the kick turns: its angles, a quarter of them with Deadshot in a
/// scoped weapon's full sights.
fn aim(player: &MovePlayer, weapons: KickWeapons<'_>, kick: &ViewKick) -> [f32; 3] {
    let (ps, _) = crate::movement::unpack(&mut player.clone());
    let scoped = weapons(ps.weapon).is_some_and(|w| {
        i(&w.def, sz::WEAPON_DEF_ADS_OVERLAY_RETICLE_OFF) != 0 && ps.weapon_pos_frac > 0.0
    });
    if ps.perks & movement_t5::perks::DEADSHOT != 0 && scoped && ps.weapon_pos_frac == 1.0 {
        return kick.angles.map(|angle| angle * DEADSHOT_KICK_SCALE);
    }
    kick.angles
}
