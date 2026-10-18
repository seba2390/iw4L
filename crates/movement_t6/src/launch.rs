//! The launch a melee lunge starts: the player is carried at the lunge's
//! target, before the move cases, until the charge time runs out.

use crate::state::{ENTITYNUM_NONE, e_flags, pm_flags, pm_type, weapon_state as ws};
use crate::{MoveWorld, PlayerState, Pm, Pml, tuning};

const DEG2RAD: f32 = 0.0174532924;

/// The yaw toward a direction: its angle times 180 over π, in 0..360.
fn vectoyaw(v: [f32; 3]) -> f32 {
    if v[1] == 0.0 && v[0] == 0.0 {
        return 0.0;
    }
    let yaw = libm::atan2(f64::from(v[1]), f64::from(v[0])) as f32 * 180.0 * 0.318309873;
    if yaw >= 0.0 { yaw } else { 360.0 + yaw }
}

/// The unit direction of a yaw, on the floor.
fn yaw_vector(yaw: f32) -> [f32; 2] {
    let radians = f64::from(yaw * DEG2RAD);
    [libm::cos(radians) as f32, libm::sin(radians) as f32]
}

/// A floor vector at unit length by the game's fast inverse square root.
fn normalize_fast(v: [f32; 2]) -> [f32; 2] {
    let len_sq = v[0] * v[0] + v[1] * v[1];
    let guess = f32::from_bits(0x5f37_59df_u32.wrapping_sub((len_sq.to_bits() as i32 >> 1) as u32));
    let scale = (1.5 - len_sq * 0.5 * guess * guess) * guess;
    [v[0] * scale, scale * v[1]]
}

/// The melee's delay on the hand that is meleeing: the right's while it is
/// in a melee state, else the left's.
fn melee_delay(ps: &PlayerState) -> i32 {
    if (ws::MELEE_INIT..=ws::MELEE_END).contains(&ps.weaponstate) {
        ps.weapon_delay
    } else {
        ps.weapon_delay_left
    }
}

/// The launch ends: its flag, target, distance and time are cleared, and
/// the upward speed is capped.
fn end(ps: &mut PlayerState) {
    ps.pm_flags &= !pm_flags::LAUNCHED;
    ps.melee_charge_dist = 0;
    ps.melee_charge_time = 0;
    ps.melee_charge_ent = ENTITYNUM_NONE;
    if tuning::BAYONET_LAUNCH_PROOF && 0.0 > tuning::BAYONET_LAUNCH_Z_CAP - ps.velocity[2] {
        ps.velocity[2] = tuning::BAYONET_LAUNCH_Z_CAP;
    }
}

/// The launch still holds: launched, normal, on no turret, neither
/// mantling nor on a ladder, and (with the jump clamp) not rising off the
/// ground too fast.
fn holds<W: MoveWorld>(pm: &mut Pm<'_, W>) -> bool {
    let ps = &mut *pm.ps;
    let holds = ps.pm_flags & pm_flags::LAUNCHED != 0
        && ps.pm_type == pm_type::NORMAL
        && ps.e_flags & e_flags::TURRET == 0
        && ps.pm_flags & (pm_flags::MANTLE | pm_flags::LADDER) == 0;
    if tuning::CLAMP_MELEE_CHARGE_JUMPING
        && holds
        && ps.ground_entity_num == ENTITYNUM_NONE
        && tuning::CLAMP_MELEE_CHARGE_JUMPING_MAX_Z_VEL < ps.velocity[2]
    {
        if tuning::CLAMP_MELEE_CHARGE_HORZ_LAUNCH {
            let speed = ps.speed as f32;
            let v = ps.velocity;
            if speed * speed < v[0] * v[0] + v[1] * v[1] + v[2] * v[2] {
                crate::math::normalize(&mut ps.velocity);
                ps.velocity = ps.velocity.map(|c| c * speed);
            }
        }
        return false;
    }
    holds
}

/// The yaw from the player to the launch's target; 0 when the target is not
/// in use.
fn target_yaw<W: MoveWorld>(pm: &Pm<'_, W>) -> f32 {
    let ps = &*pm.ps;
    let Some(at) = pm.world.entity_origin(ps.melee_charge_ent) else {
        return 0.0;
    };
    vectoyaw([
        at[0] - ps.origin[0],
        at[1] - ps.origin[1],
        at[2] - ps.origin[2],
    ])
}

/// The launch, before the move cases.
pub(crate) fn launch<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    if !holds(pm) {
        end(pm.ps);
        return;
    }
    if pm.ps.melee_charge_time == 0 {
        let friction = tuning::MELEE_CHARGE_FRICTION;
        let [x, y] = yaw_vector(target_yaw(pm));
        let ps = &mut *pm.ps;
        let speed = libm::sqrtf(ps.melee_charge_dist as f32 * 2.0 * friction);
        ps.velocity[0] = x * speed;
        ps.velocity[1] = y * speed;
        ps.melee_charge_time = (speed / friction * 1000.0) as i32;
    }
    speed_from_target(pm, pml);
    let ps = &mut *pm.ps;
    ps.melee_charge_time -= pml.msec;
    if ps.melee_charge_time < 1 {
        end(ps);
    }
}

/// While the melee plays, the floor velocity is aimed to arrive the melee's
/// reach short of where the target will be when the blow lands, no faster
/// than the charge's top speed.
fn speed_from_target<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    if !tuning::MELEE_CALC_SPEED_FROM_TARGET {
        return;
    }
    let range = tuning::MELEE_RANGE;
    let max_speed = tuning::MELEE_CHARGE_MAX_SPEED;
    let friction = tuning::MELEE_CHARGE_FRICTION;
    if !holds(pm) || pm.ps.melee_charge_ent == ENTITYNUM_NONE {
        return;
    }
    let ps = &*pm.ps;
    let in_melee = |state: i32| (ws::MELEE_INIT..=ws::MELEE_END).contains(&state);
    let delay = if in_melee(ps.weaponstate) {
        ps.weapon_delay
    } else if in_melee(ps.weaponstate_left) {
        ps.weapon_delay_left
    } else {
        return;
    };
    let secs = delay as f32 * 0.00100000005;
    if 0.0 >= secs {
        return;
    }
    let target = ps.melee_charge_ent;
    let (Some(at), Some(moving)) = (
        pm.world.entity_origin(target),
        pm.world.entity_velocity(target),
    ) else {
        return;
    };
    let [ox, oy, oz] = ps.origin;
    let (dx, dy, dz) = (at[0] - ox, at[1] - oy, at[2] - oz);
    let len_sq = dy * dy + dx * dx + dz * dz;
    if len_sq <= range * range {
        return;
    }
    let len = libm::sqrtf(len_sq);
    let k = (len - range) / len;
    let reach_x = -ox + (dx * k + ox);
    let reach_y = -oy + (dy * k + oy);
    let inv = 1.0 / secs;
    let mut vx = (moving[0] * secs + reach_x) * inv;
    let mut vy = (moving[1] * secs + reach_y) * inv;
    let push = pml.frametime * friction + 1.0;
    vy *= push;
    vx *= push;
    if vy * vy + vx * vx > max_speed * max_speed {
        [vx, vy] = normalize_fast([vx, vy]);
        vx *= max_speed;
        vy *= max_speed;
    }
    pm.ps.velocity[0] = vx;
    pm.ps.velocity[1] = vy;
}

/// What friction takes from a launched player's speed this frame: once the
/// blow has landed, what keeps them from overshooting the target (15 short
/// of it) in the charge time left; else the charge's own friction.
pub(crate) fn friction_drop<W: MoveWorld>(pm: &Pm<'_, W>, pml: &Pml) -> f32 {
    let ps = &*pm.ps;
    let fallback = (f64::from(tuning::MELEE_CHARGE_FRICTION) * f64::from(pml.frametime)) as f32;
    if !tuning::MELEE_CALC_SPEED_FROM_TARGET
        || melee_delay(ps) > 0
        || ps.melee_charge_time <= 0
        || ps.melee_charge_ent == ENTITYNUM_NONE
    {
        return fallback;
    }
    let Some(at) = pm.world.entity_origin(ps.melee_charge_ent) else {
        return fallback;
    };
    let dy = ps.origin[1] - at[1];
    let dx = ps.origin[0] - at[0];
    let dist = libm::sqrtf(dy * dy + dx * dx) - 15.0;
    let secs = ps.melee_charge_time as f32 * 0.00100000005;
    let v = ps.velocity;
    let speed = libm::sqrtf(v[0] * v[0] + v[1] * v[1]);
    if speed * secs > dist {
        (f64::from(speed) - f64::from(dist) / f64::from(secs)) as f32
    } else {
        fallback
    }
}
