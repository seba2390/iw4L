//! Black Ops 2's client view kick: how the local player's own shots kick
//! the view and gun, and how each client frame eases them back.

use fastfile_t6::weapon::{WeaponDefView, def as d, variant as v};
use game_api::fire::{KickWeapons, ViewKick, ViewKickRules};
use game_api::movement::MovePlayer;
use movement_t6::state::{pm_flags, weap_flags};
use movement_t6::{PlayerState, perks};

pub(crate) const RULES: ViewKickRules = ViewKickRules { fire, frame, aim };

const CROUCH_HEIGHT: f32 = 40.0;
const PRONE_HEIGHT: f32 = 11.0;

const CRAWL_SCALE: f32 = 0.0;
const PRONE_SCALE: f32 = 0.25;
const CROUCH_SCALE: f32 = 0.65;
const SPRINT_SCALE: f32 = 3.0;
const CROWD_SCALE: f32 = 2.0;
const REDUCED_PERCENT_SCALE: f32 = 0.01;
const ROLL_FROM_YAW: f32 = -0.5;
const DEADSHOT_KICK_SCALE: f32 = 0.25;

const KICK_STEP_MS: i32 = 5;
const NO_WEAPON_CENTER_SPEED: f32 = 2400.0;
const KICK_LIMIT: f32 = 10.0;
const KICK_RETURN_SCALE: f32 = 0.06;

fn fire(
    player: &MovePlayer,
    weapons: KickWeapons<'_>,
    rand: &mut dyn FnMut() -> f32,
    kick: &mut ViewKick,
) {
    let (ps, _) = crate::movement::unpack(&mut player.clone());
    let Some((def_bytes, variant_bytes)) = active_weapon(&ps, weapons) else {
        return;
    };
    let Some(def) = WeaponDefView::new(&def_bytes, &variant_bytes) else {
        return;
    };
    let ads = ps.weapon_pos_frac == 1.0;
    let draw = |min: f32, max: f32, rand: &mut dyn FnMut() -> f32| min + (max - min) * rand();
    let (gun_pitch, gun_yaw) = if ads {
        (
            draw(
                def.def_f32(d::ADS_GUN_KICK_PITCH_MIN),
                def.def_f32(d::ADS_GUN_KICK_PITCH_MAX),
                rand,
            ),
            draw(
                def.def_f32(d::ADS_GUN_KICK_YAW_MIN),
                def.def_f32(d::ADS_GUN_KICK_YAW_MAX),
                rand,
            ),
        )
    } else {
        (
            draw(
                def.def_f32(d::HIP_GUN_KICK_PITCH_MIN),
                def.def_f32(d::HIP_GUN_KICK_PITCH_MAX),
                rand,
            ),
            draw(
                def.def_f32(d::HIP_GUN_KICK_YAW_MIN),
                def.def_f32(d::HIP_GUN_KICK_YAW_MAX),
                rand,
            ),
        )
    };
    let (mut pitch, mut yaw, min_mag) = if ads {
        (
            draw(
                def.def_f32(d::ADS_VIEW_KICK_PITCH_MIN),
                def.def_f32(d::ADS_VIEW_KICK_PITCH_MAX),
                rand,
            ),
            draw(
                def.def_f32(d::ADS_VIEW_KICK_YAW_MIN),
                def.def_f32(d::ADS_VIEW_KICK_YAW_MAX),
                rand,
            ),
            def.def_f32(d::ADS_VIEW_KICK_MIN_MAGNITUDE),
        )
    } else {
        (
            draw(
                def.def_f32(d::HIP_VIEW_KICK_PITCH_MIN),
                def.def_f32(d::HIP_VIEW_KICK_PITCH_MAX),
                rand,
            ),
            draw(
                def.def_f32(d::HIP_VIEW_KICK_YAW_MIN),
                def.def_f32(d::HIP_VIEW_KICK_YAW_MAX),
                rand,
            ),
            def.def_f32(d::HIP_VIEW_KICK_MIN_MAGNITUDE),
        )
    };
    let mag = (pitch * pitch + yaw * yaw).sqrt();
    if mag > 0.0 && mag < min_mag {
        let up = min_mag / mag;
        pitch *= up;
        yaw *= up;
    }
    let scale = kick_scale(&ps, &def, ads);
    kick.velocity[0] = -pitch * scale;
    kick.velocity[1] = yaw * scale;
    kick.velocity[2] = yaw * scale * ROLL_FROM_YAW;
    kick.gun_velocity[0] += gun_pitch * scale;
    kick.gun_velocity[1] += gun_yaw * scale;
}

fn kick_scale(ps: &PlayerState, def: &WeaponDefView<'_>, ads: bool) -> f32 {
    let mut scale = if ps.prone_check_waist_pitch != 0.0 {
        CRAWL_SCALE
    } else if ps.view_height_current <= PRONE_HEIGHT {
        PRONE_SCALE
    } else if ps.view_height_current <= CROUCH_HEIGHT {
        CROUCH_SCALE
    } else if ps.pm_flags & pm_flags::SPRINTING != 0 {
        SPRINT_SCALE
    } else {
        1.0
    };
    if ps.weap_flags & 0x18 != 0 {
        scale *= CROWD_SCALE;
    }
    let (bullets, percent) = if ads {
        (
            def.def_i32(d::ADS_GUN_KICK_REDUCED_KICK_BULLETS),
            def.def_f32(d::ADS_GUN_KICK_REDUCED_KICK_PERCENT),
        )
    } else {
        (
            def.def_i32(d::HIP_GUN_KICK_REDUCED_KICK_BULLETS),
            def.def_f32(d::HIP_GUN_KICK_REDUCED_KICK_PERCENT),
        )
    };
    if ps.weapon_shot_count <= bullets {
        scale *= percent * REDUCED_PERCENT_SCALE;
    }
    if perks::has(ps.perks, perks::DEADSHOT) {
        scale *= DEADSHOT_KICK_SCALE;
    }
    scale
}

fn frame(
    player: &MovePlayer,
    weapons: KickWeapons<'_>,
    mut frametime_ms: i32,
    kick: &mut ViewKick,
) {
    let (ps, _) = crate::movement::unpack(&mut player.clone());
    let weapon = active_weapon(&ps, weapons);
    let def = weapon.as_ref().and_then(|(d, v)| WeaponDefView::new(d, v));
    let center_speed = match &def {
        None => NO_WEAPON_CENTER_SPEED,
        Some(def) if ps.weapon_pos_frac == 1.0 => def.variant_f32(v::ADS_VIEW_KICK_CENTER_SPEED),
        Some(def) => def.variant_f32(v::HIP_VIEW_KICK_CENTER_SPEED),
    };
    let center_speed = if center_speed > 0.0 {
        center_speed
    } else {
        NO_WEAPON_CENTER_SPEED
    };
    let mut dist_factor = 0.0f32;
    if let Some(def) = &def {
        let fight = def.def_f32(d::FIGHT_DIST);
        let max = def.def_f32(d::MAX_DIST);
        let mag = (kick.angles[0] * kick.angles[0] + kick.angles[1] * kick.angles[1]).sqrt();
        if mag > fight && max > fight {
            dist_factor = ((mag - fight) / (max - fight)).clamp(0.0, 1.0);
        }
    }
    while frametime_ms > 0 {
        let step_ms = frametime_ms.min(KICK_STEP_MS);
        let dt = step_ms as f32 * 0.001;
        for axis in 0..3 {
            if kick.angles[axis] != 0.0 || kick.velocity[axis] != 0.0 {
                if kick.angles[axis] != 0.0 {
                    let sign = if kick.angles[axis] > 0.0 { -1.0 } else { 1.0 };
                    let step = center_speed * (1.0 + dist_factor) * dt;
                    if step >= kick.angles[axis].abs() {
                        kick.angles[axis] = 0.0;
                    } else {
                        kick.angles[axis] += sign * step;
                    }
                }
                let return_accel = -kick.angles[axis] * (1.0 / KICK_RETURN_SCALE);
                kick.velocity[axis] += return_accel * dt;
                kick.angles[axis] =
                    (kick.angles[axis] + kick.velocity[axis] * dt).clamp(-KICK_LIMIT, KICK_LIMIT);
                kick.velocity[axis] *= 1.0 - (center_speed * 0.0005 * dt).min(1.0);
                if kick.angles[axis].abs() < 1.0e-4 && kick.velocity[axis].abs() < 1.0e-3 {
                    kick.angles[axis] = 0.0;
                    kick.velocity[axis] = 0.0;
                }
            }
        }
        frametime_ms -= KICK_STEP_MS;
    }
    if let Some(def) = &def {
        let max_pitch = def.def_f32(d::GUN_MAX_PITCH);
        let max_yaw = def.def_f32(d::GUN_MAX_YAW);
        let decay = if ps.weapon_pos_frac == 1.0 {
            def.def_f32(d::ADS_GUN_KICK_SPEED_DECAY)
        } else {
            def.def_f32(d::HIP_GUN_KICK_SPEED_DECAY)
        };
        let dt = 0.005;
        let damp = (1.0 - decay * dt).clamp(0.0, 1.0);
        kick.gun_velocity[0] = (kick.gun_velocity[0] * damp).clamp(-max_pitch, max_pitch);
        kick.gun_velocity[1] = (kick.gun_velocity[1] * damp).clamp(-max_yaw, max_yaw);
    } else {
        kick.gun_velocity = [0.0; 2];
    }
}

fn aim(_player: &MovePlayer, _weapons: KickWeapons<'_>, kick: &ViewKick) -> [f32; 3] {
    kick.angles
}

fn active_weapon(ps: &PlayerState, weapons: KickWeapons<'_>) -> Option<(Box<[u8]>, Box<[u8]>)> {
    let index = if ps.weap_flags & weap_flags::OFFHAND_ACTIVE != 0 && ps.offhand_index != 0 {
        ps.offhand_index
    } else {
        ps.weapon
    };
    if index == 0 {
        return None;
    }
    let w = weapons(index)?;
    Some((w.def, w.variant))
}
