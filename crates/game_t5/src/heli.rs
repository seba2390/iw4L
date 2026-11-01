//! Black Ops multiplayer's helicopter flight, as BlackOpsMP.exe's vehicle
//! think flies a helicopter with a goal: it speeds up toward the goal or, told
//! to stop there, brakes to arrive at rest and then hovers around it; it turns
//! toward where it goes (or what it is told to look at) and tilts with its
//! acceleration.

use fastfile_t5::weapon_def::WeaponDefView;
use game_api::fire::FireWorld;
use game_api::heli::{HeliAim, HeliMove, HeliNotes, HeliShot, HeliWorld};
use game_api::{Unknown, unknown};

const FRAME: f32 = 0.05;
/// Inches a second in a mile an hour.
pub(crate) const MPH: f32 = 17.6;
/// The turn in degrees past which it flies no faster than standing still.
const TURN_SLOWDOWN: f32 = 90.0;
/// Under this speed a helicopter counts as hovering at its goal.
const HOVER_ARRIVAL_SPEED: f32 = 35.2;

fn no_goal_unknown() -> &'static Unknown {
    unknown!(
        "t5.vehicle.heli_no_goal",
        "a helicopter with no goal stays where it is",
        "BlackOpsMP.exe's move for a helicopter without a goal (the player-controlled one's)"
    )
}

fn length(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn normalize_180(angle: f32) -> f32 {
    let turns = angle * 0.002_777_777_8;
    (turns - (turns + 0.5).floor()) * 360.0
}

fn dvar_f32(world: &dyn HeliWorld, name: &str, default: f32) -> f32 {
    world
        .dvar(name)
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn yaw_of(dir: [f32; 3]) -> f32 {
    if dir[1] == 0.0 && dir[0] == 0.0 {
        0.0
    } else {
        movement_t5::math::vectoyaw(dir)
    }
}

/// The yaw it turns toward: what it looks at; else its goal yaw once it is
/// slowing for it; else its target yaw; else where it heads, unless hovering.
fn wanted_yaw(heli: &HeliMove, dir: [f32; 3], world: &mut dyn HeliWorld) -> f32 {
    if let Some(target) = world.look_at() {
        return yaw_of(std::array::from_fn(|i| target[i] - heli.origin[i]));
    }
    if let Some(goal_yaw) = heli.goal_yaw
        && (heli.stopping || heli.hovering)
    {
        let stop_time = heli.speed / heli.decel;
        let mut brake = (heli.angle_limit[1] / heli.yaw_accel) * 2.0;
        let brake_turn = heli.angle_limit[1] * brake * 0.5;
        let turn = normalize_180(goal_yaw - heli.angles[1]).abs();
        if brake < stop_time && brake_turn < turn {
            brake += (turn - brake_turn) / heli.angle_limit[1];
        }
        if stop_time <= brake {
            return goal_yaw;
        }
    }
    if let Some(target_yaw) = heli.target_yaw {
        return target_yaw;
    }
    if heli.hovering {
        return heli.angles[1];
    }
    yaw_of(dir)
}

/// One frame of an angle turning toward `target`: it accelerates to its
/// limit and brakes to arrive, past the arrival by `overshoot` of the braking.
fn turn_angle(
    heli: &mut HeliMove,
    index: usize,
    target: f32,
    accel: f32,
    decel: f32,
    overshoot: f32,
) {
    let off = normalize_180(target - heli.angles[index]);
    if off * off < 0.000_1 && heli.angle_rate[index] * heli.angle_rate[index] < 0.002_5 {
        heli.angle_rate[index] = 0.0;
        heli.angles[index] = target;
        return;
    }
    let rate = heli.angle_rate[index];
    let mut wanted = heli.angle_limit[index];
    let mut change = accel;
    let rate_size = rate.abs();
    if 0.0 <= rate * off && off.abs() <= (rate_size / decel) * rate_size * 0.5 * (1.0 - overshoot) {
        wanted = 0.0;
        change = decel;
    }
    if off < 0.0 {
        wanted = -wanted;
    }
    let change = change * FRAME;
    if rate_size < change && off.abs() < rate_size * FRAME {
        heli.angles[index] = target;
        heli.angle_rate[index] = 0.0;
        return;
    }
    let next = if wanted <= rate {
        let next = rate - change;
        if wanted < next { next } else { wanted }
    } else {
        let next = change + rate;
        if next < wanted { next } else { wanted }
    };
    heli.angle_rate[index] = next;
    heli.angles[index] = normalize_180(next * FRAME + heli.angles[index]);
}

/// The yaw's frame, saying `goal_yaw` as it arrives at a goal or target yaw.
fn turn_yaw(heli: &mut HeliMove, wanted: f32, notes: &mut HeliNotes) {
    let fixed = heli.goal_yaw.or(heli.target_yaw);
    let before = fixed.map_or(0.0, |yaw| normalize_180(heli.angles[1] - yaw));
    let old_rate = heli.angle_rate[1];
    if wanted != heli.last_yaw_goal {
        heli.yaw_settling = false;
        heli.last_yaw_goal = wanted;
    }
    let (mut accel, mut decel) = (heli.yaw_accel, heli.yaw_decel);
    if heli.yaw_settling {
        accel *= 0.2;
        decel *= 0.2;
    }
    let overshoot = if fixed.is_some() {
        heli.yaw_overshoot
    } else {
        0.0
    };
    turn_angle(heli, 1, wanted, accel, decel, overshoot);
    if heli.goal_yaw.is_some() && old_rate * heli.angle_rate[1] < 0.0 {
        heli.yaw_settling = true;
    }
    if let Some(yaw) = fixed {
        let after = normalize_180(heli.angles[1] - yaw);
        if after * before < 0.0 || after.abs() <= 1.5 {
            notes.goal_yaw = true;
        }
    }
}

/// The body's turn: its yaw, and its pitch and roll tilting into its
/// acceleration and speed, less so while it brakes to a stop.
fn turn_body(heli: &mut HeliMove, dir: [f32; 3], world: &mut dyn HeliWorld, notes: &mut HeliNotes) {
    let wanted = wanted_yaw(heli, dir, world);
    turn_yaw(heli, wanted, notes);
    let [vx, vy, _] = heli.velocity;
    let speed_xy = (vy * vy + vx * vx).sqrt();
    let capped = if heli.tilt_speed - speed_xy < 0.0 {
        heli.tilt_speed
    } else {
        speed_xy
    };
    let over = if 0.0 <= -speed_xy { 1.0 } else { speed_xy };
    let share = (capped / heli.tilt_speed) * (capped / heli.tilt_speed) * 5.0;
    let ax = share * (1.0 / over) * vx + heli.tilt_accel[0];
    let ay = vy * (1.0 / over) * share + heli.tilt_accel[1];
    let az = heli.tilt_accel[2];
    let all = (az * az + ax * ax + ay * ay).sqrt();
    let flat = (ax * ax + ay * ay).sqrt() * 20.0;
    let all = if 0.0 <= -all { 1.0 } else { all };
    let (ax, ay) = (ax / all, ay / all);
    let (sin, cos) = (f64::from(heli.angles[1] * 0.017_453_292)).sin_cos();
    let (sin, cos) = (sin as f32, cos as f32);
    let def_accel = heli.def_accel;
    let mut force = if 0.0 <= flat - def_accel {
        def_accel
    } else {
        flat
    };
    if 0.0 <= -flat {
        force = 0.0;
    }
    let force = force / def_accel;
    let mut scale = 1.0;
    if heli.stopping && 0.0 < flat {
        let ratio = speed_xy / flat;
        let wanted_ratio = (1.0 - (1.0 - force)) * 2.5 + (1.0 - force) * 3.5;
        if ratio < wanted_ratio {
            scale = ratio / wanted_ratio;
        }
    }
    let tilt = (1.0 - force) * 0.1 + force;
    let (accel, decel) = (heli.yaw_accel, heli.yaw_accel * 0.4);
    let pitch = (ay * sin + ax * cos) * scale * heli.max_pitch * tilt;
    turn_angle(heli, 0, pitch, accel, decel, 0.0);
    let roll = (ay * -cos + ax * sin) * scale * heli.max_roll * tilt;
    turn_angle(heli, 2, roll, accel, decel, 0.0);
}

/// The frame's share it flies before braking, and whether to brake: told to
/// stop at its goal, it brakes once this frame's speed would pass the
/// distance it needs to stop.
fn stop_slice(heli: &mut HeliMove, distance: f32) -> f32 {
    let speed = heli.speed;
    let wanted = heli.max_speed;
    let step = heli.accel * FRAME;
    let next = if wanted <= speed {
        let next = speed - step;
        if wanted < next { next } else { wanted }
    } else {
        let next = step + speed;
        if next < wanted { next } else { wanted }
    };
    let stop_distance = (next / heli.decel) * next * 0.5;
    if distance - next * FRAME <= stop_distance && 0.0 < speed {
        heli.stopping = true;
        if stop_distance < distance {
            let slice = FRAME - (distance - stop_distance) / speed;
            let capped = if slice - FRAME < 0.0 { slice } else { FRAME };
            return if 0.0 <= -slice { 0.0 } else { capped };
        }
        return FRAME;
    }
    heli.stopping = false;
    FRAME
}

/// The speed it may reach by the end of `slice`, by `scale` of its top speed,
/// and the acceleration it does that with.
fn next_speed(heli: &HeliMove, slice: f32, hover: bool, scale: f32) -> (f32, f32) {
    let (top, accel, decel) = if hover {
        (heli.hover_speed, heli.hover_accel, heli.hover_accel * 0.5)
    } else if heli.script_speed {
        (heli.max_speed, heli.accel, heli.decel)
    } else {
        (heli.def_speed, heli.def_accel, heli.def_accel * 0.5)
    };
    let top = top * scale;
    if heli.stopping {
        let speed = heli.speed;
        let next = if 0.0 <= speed {
            let next = speed - decel * slice;
            if 0.0 < next { next } else { 0.0 }
        } else {
            let next = decel * slice + speed;
            if next < 0.0 { next } else { 0.0 }
        };
        return (next, decel);
    }
    let speed = heli.speed;
    let next = if top <= speed {
        let next = speed - accel * slice;
        if top < next { next } else { top }
    } else {
        let next = accel * slice + speed;
        if next < top { next } else { top }
    };
    (next, accel)
}

/// Bends the sideways velocity change into the tightest circle that still
/// reaches the goal, slowing it when it cannot.
fn tighten_turn(heli: &mut HeliMove, push: &mut [f32; 3], to: [f32; 3], accel: f32) {
    let to_xy = (to[0] * to[0] + to[1] * to[1]).sqrt();
    if to_xy < 1.0 {
        return;
    }
    let [vx, vy, _] = heli.velocity;
    let speed_xy = (vx * vx + vy * vy).sqrt();
    if heli.stop_at_goal {
        let (nx, ny) = (vx + push[0], vy + push[1]);
        let new_speed = (ny * ny + nx * nx).sqrt();
        if 0.0 < new_speed {
            let limit = ((speed_xy * speed_xy) / (to_xy * 2.0)) * FRAME;
            if limit <= (new_speed - speed_xy).abs() {
                return;
            }
            let keep = (speed_xy - limit) / new_speed;
            push[0] = keep * nx - vx;
            push[1] = ny * keep - vy;
            let all = (push[2] * push[2] + push[1] * push[1] + push[0] * push[0]).sqrt();
            if all <= accel {
                return;
            }
            let cut = accel / all;
            push[0] *= cut;
            push[1] *= cut;
            push[2] *= cut;
            return;
        }
    }
    let across = (to[1] * -vx + to[0] * vy).abs();
    if to_xy < across {
        let over = 1.0 / speed_xy;
        let ahead = over * (vy * to[1] + vx * to[0]);
        let side = over * across;
        let radius = (ahead * ahead + side * side) / (side * 2.0);
        if 1.0 < radius && radius * accel < speed_xy * speed_xy {
            let needed = (speed_xy * speed_xy) / radius;
            let needed = if speed_xy < needed { speed_xy } else { needed };
            let share = if heli.stop_at_goal { 1.0 } else { heli.turning };
            let cut = over * share * needed * -FRAME;
            let (dx, dy) = (heli.velocity[0] * cut, heli.velocity[1] * cut);
            heli.velocity[0] += dx;
            heli.velocity[1] += dy;
            push[0] += dx;
            push[1] += dy;
        }
    }
}

/// Stops a vertical change that would overshoot the goal's height.
fn level_off(heli: &mut HeliMove, push: &mut [f32; 3], dz: f32) {
    let (change, vz) = (push[2], heli.velocity[2]);
    if 0.001 > change.abs() || 0.001 > vz.abs() || 0.0 <= change * vz || 0.0 >= vz * dz {
        return;
    }
    let time = dz / (vz * 0.5);
    if time >= vz * FRAME * (-1.0 / change) {
        return;
    }
    let wanted = (vz / time) * -FRAME;
    if change * change < wanted * wanted {
        let limit = heli.accel * 0.15;
        let wanted = if wanted < limit { wanted } else { limit };
        let wanted = if -limit < wanted { wanted } else { -limit };
        heli.velocity[2] += wanted - change;
        push[2] = wanted;
    }
}

/// Above 50 units a second part of its speed swings round with its yaw.
fn swing_with_yaw(heli: &mut HeliMove) {
    let forward = |yaw: f32| {
        let (sin, cos) = (f64::from(yaw * 0.017_453_292)).sin_cos();
        [cos as f32, sin as f32, 0.0]
    };
    let before = forward(heli.previous_yaw);
    let v = heli.velocity;
    let along = v[1] * before[1] + v[2] * before[2] + v[0] * before[0];
    if 50.0 < along {
        let share = heli.turning * along;
        let now = forward(heli.angles[1]);
        for i in 0..3 {
            heli.velocity[i] += now[i] * share - before[i] * share;
        }
    }
}

/// Whether it has reached its goal this frame.
fn at_goal(heli: &mut HeliMove, distance: f32) -> bool {
    if !heli.stop_at_goal {
        return distance <= heli.speed * FRAME;
    }
    let radius = heli.hover_radius;
    let arrived = if radius == 0.0 {
        (heli.stopping || distance == 0.0) && heli.speed == 0.0
    } else {
        distance <= radius && heli.speed < HOVER_ARRIVAL_SPEED
    };
    if arrived {
        heli.hovering = true;
        if radius == 0.0 {
            heli.tilt_accel = [0.0; 3];
            heli.velocity = [0.0; 3];
        }
    }
    arrived
}

/// One frame toward `goal`.
fn fly_to(
    heli: &mut HeliMove,
    goal: [f32; 3],
    hover: bool,
    world: &mut dyn HeliWorld,
    notes: &mut HeliNotes,
) {
    let to: [f32; 3] = std::array::from_fn(|i| goal[i] - heli.origin[i]);
    let distance = length(to);
    let over = 1.0 / distance;
    let dir = to.map(|v| v * over);
    let wanted = wanted_yaw(heli, dir, world);
    if distance <= 0.0 {
        heli.velocity = [0.0; 3];
        return;
    }
    let old = heli.velocity;
    let slice = if heli.stop_at_goal {
        stop_slice(heli, distance)
    } else {
        FRAME
    };
    let off = normalize_180(wanted - heli.angles[1]);
    let turn_scale = if off.abs() < TURN_SLOWDOWN {
        1.0 - off / TURN_SLOWDOWN
    } else {
        0.0
    };
    let (speed_xy, _) = next_speed(heli, slice, hover, turn_scale);
    let (speed_z, accel) = next_speed(heli, slice, hover, 0.5);
    let limit_z = dvar_f32(world, "vehHelicopterMaxSpeedVertical", 65.0) * MPH;
    let speed_z = if limit_z < speed_z { limit_z } else { speed_z };
    let mut push = [
        dir[0] * speed_xy - old[0],
        dir[1] * speed_xy - old[1],
        dir[2] * speed_z - old[2],
    ];
    if !heli.stopping
        && heli.speed <= heli.max_speed
        && 0.0 < dir[1] * speed_xy * old[1] + dir[0] * speed_xy * old[0]
        && push[1] * old[1] + push[0] * old[0] < 0.0
    {
        // Slowing a turn would not gain anything: only the sideways part acts.
        let flat = (old[0] * old[0] + old[1] * old[1]).sqrt();
        let over = 1.0 / if 0.0 <= -flat { 1.0 } else { flat };
        let (hx, hy) = (old[0] * over, old[1] * over);
        let along = hy * push[1] + push[0] * hx;
        push[0] -= hx * along;
        push[1] -= hy * along;
    }
    let budget = accel * slice;
    let flat = (push[1] * push[1] + push[0] * push[0]).sqrt();
    if budget < flat {
        let cut = budget / flat;
        push[0] *= cut;
        push[1] *= cut;
    }
    let budget_z = budget * 0.5;
    let z = if budget_z < push[2] {
        budget_z
    } else if push[2] < -budget_z {
        -budget_z
    } else {
        push[2]
    };
    push[2] = z;
    if !hover {
        tighten_turn(heli, &mut push, to, accel);
        if to[2] != 0.0 {
            level_off(heli, &mut push, to[2]);
        }
    }
    heli.velocity = std::array::from_fn(|i| push[i] + old[i]);
    heli.speed = length(heli.velocity);
    for (tilt, pushed) in heli.tilt_accel.iter_mut().zip(push) {
        *tilt += (pushed - *tilt) * 0.5;
    }
    let tilt = length(heli.tilt_accel);
    if budget_z < tilt && !heli.stop_at_goal {
        let cut = budget_z / tilt;
        heli.tilt_accel = heli.tilt_accel.map(|v| v * cut);
    }
    let rest = FRAME - slice;
    if slice < FRAME {
        for (at, v) in heli.origin.iter_mut().zip(old) {
            *at += rest * v;
        }
    }
    let velocity = heli.velocity;
    for ((at, new), was) in heli.origin.iter_mut().zip(velocity).zip(old) {
        *at += (new + was) * 0.5 * slice;
    }
    turn_body(heli, dir, world, notes);
    swing_with_yaw(heli);
    if !hover {
        if heli.near_goal != 0.0 && distance < heli.near_goal {
            notes.near_goal = true;
        }
        notes.goal = at_goal(heli, distance);
    }
}

/// One frame around its goal, drifting toward a new random point when it
/// gets near the last.
fn hover(heli: &mut HeliMove, world: &mut dyn HeliWorld, notes: &mut HeliNotes) {
    let target: [f32; 3] = std::array::from_fn(|i| heli.hover_offset[i] + heli.goal[i]);
    fly_to(heli, target, true, world, notes);
    let off: [f32; 3] = std::array::from_fn(|i| target[i] - heli.origin[i]);
    let near = heli.hover_radius * 0.25;
    if near * near <= off[0] * off[0] + off[2] * off[2] + off[1] * off[1] {
        return;
    }
    if heli.hover_radius == 0.0 {
        heli.hover_offset = [0.0; 3];
        return;
    }
    let radius = heli.hover_radius;
    for i in 0..3 {
        heli.hover_offset[i] = world.random_range(-radius, radius) - heli.hover_offset[i] * 0.5;
    }
}

fn approach_angle(target: f32, current: f32, speed: f32) -> f32 {
    let mut target = target;
    while target - current > 180.0 {
        target -= 360.0;
    }
    while target - current < -180.0 {
        target += 360.0;
    }
    let mut step = speed * FRAME;
    if target - current <= 0.0 {
        step = -step;
    }
    let delta = (target - current).abs();
    if 0.001 < delta && step.abs() <= delta {
        target = step + current;
    }
    normalize_180(target)
}

/// The world angles of the turret's aim off a body turned `body` degrees.
fn turret_world_angles(heli: &HeliMove) -> [f32; 3] {
    [
        heli.turret_angles[0] + heli.angles[0],
        heli.turret_angles[1] + heli.angles[1],
        heli.angles[2],
    ]
}

/// The turret's frame: it turns toward what it aims at, keeping off the body's
/// turn, and says when it has stopped and whether it is on target.
fn turret_frame(heli: &mut HeliMove, world: &mut dyn HeliWorld, notes: &mut HeliNotes) {
    let Some(aim) = heli.turret_aim else {
        heli.turret_settling = match heli.turret_settling {
            2 => 1,
            _ => 0,
        };
        return;
    };
    let (point, target) = match aim {
        HeliAim::Point(point) => (point, None),
        HeliAim::Entity { entity, offset } => {
            let Some(at) = world.entity_origin(entity) else {
                return;
            };
            (std::array::from_fn(|i| at[i] + offset[i]), Some(entity))
        }
    };
    let Some(barrel) = world.barrel() else {
        return;
    };
    let to = std::array::from_fn(|i| point[i] - barrel[i]);
    let pitch = movement_t5::math::vectopitch(to);
    let wanted = [
        normalize_180(pitch - heli.angles[0]),
        normalize_180(yaw_of(to) - heli.angles[1]),
    ];
    // The body's turn this frame takes the gun's yaw with it.
    let current = [
        heli.turret_angles[0],
        heli.turret_angles[1] - heli.angles[1] + heli.previous_yaw,
    ];
    let off = [
        normalize_180(wanted[0] - current[0]).abs(),
        normalize_180(wanted[1] - current[1]).abs(),
    ];
    heli.turret_angles = [
        approach_angle(wanted[0], current[0], heli.turret_rate),
        approach_angle(wanted[1], current[1], heli.turret_rate),
    ];
    if 2.0 <= off[0] || 2.0 <= off[1] {
        heli.turret_settling = 2;
    } else if heli.turret_settling == 2 {
        heli.turret_settling = 1;
    } else if heli.turret_settling == 1 {
        notes.turret_rotate_stopped = true;
        heli.turret_settling = 0;
    }
    if 1.0 <= off[0] || 1.0 <= off[1] {
        notes.turret_not_on_target = true;
        notes.turret_no_vis = true;
        return;
    }
    notes.turret_on_target = true;
    match target {
        Some(entity) if world.sight_clear(barrel, point, entity) => {
            notes.turret_on_vistarget = true;
        }
        _ => notes.turret_no_vis = true,
    }
}

/// `fireweapon`: a bullet along the turret or a projectile along its barrel
/// tag, from the helicopter itself; it hears `weapon_fired`.
pub(crate) fn fire(
    heli: &HeliMove,
    shot: &HeliShot,
    world: &mut dyn FireWorld,
) -> (Option<i32>, Vec<&'static Unknown>) {
    let mut gaps = Vec::new();
    let Some((def_bytes, variant_bytes)) = world
        .weapon(shot.weapon)
        .map(|w| (w.def.clone(), w.variant.clone()))
    else {
        return (None, vec![crate::movement::weapon_unknown()]);
    };
    let Some(def) = WeaponDefView::new(&def_bytes, &variant_bytes) else {
        return (None, vec![crate::movement::weapon_unknown()]);
    };
    let (forward, right, up) = if heli.turret_aim.is_some() {
        movement_t5::math::angle_vectors(turret_world_angles(heli))
    } else {
        (shot.axis[0], shot.axis[1].map(|v| -v), shot.axis[2])
    };
    // The model's tags are not turned with the gun here: the muzzle swings
    // about the gun's pivot by the gun's own turn.
    let muzzle = match shot.pivot {
        Some(pivot) if heli.turret_aim.is_some() => {
            let arm = length(std::array::from_fn(|i| shot.origin[i] - pivot[i]));
            std::array::from_fn(|i| pivot[i] + forward[i] * arm)
        }
        _ => shot.origin,
    };
    let parms = crate::fire::WeaponParms {
        forward,
        right,
        up,
        muzzle,
    };
    let mut missile = None;
    match def.weapon_type() {
        0 => {
            let time = world.level_time();
            crate::fire::Bullets {
                attacker: shot.entity,
                weapon_entity: shot.entity,
                hand: 0,
                zombies: false,
                deep_penetration: false,
                gaps: &mut gaps,
            }
            .fire(&def, shot.weapon, def.ai_spread(), &parms, time, world);
        }
        2 => {
            let dir = crate::missile::spread_dir(def.ads_spread(), forward, right, up, world);
            missile = crate::missile::spawn_rocket_mp(
                shot.entity,
                shot.entity,
                shot.weapon,
                &def,
                muzzle,
                dir,
                None,
                shot.target,
                world,
                &mut gaps,
            );
        }
        _ => gaps.push(unknown!(
            "t5.vehicle.heli_weapon_type",
            "a helicopter's bomb, gas or grenade weapon does not fire",
            "BlackOpsMP.exe's fireweapon for the other weapon types"
        )),
    }
    world.notify(shot.entity, "weapon_fired", &[]);
    (missile, gaps)
}

/// A frame of flight. Its gap when BlackOpsMP.exe's move for this state is not
/// ported.
pub(crate) fn frame(
    heli: &mut HeliMove,
    world: &mut dyn HeliWorld,
) -> (HeliNotes, Option<&'static Unknown>) {
    let mut notes = HeliNotes::default();
    heli.previous_yaw = heli.angles[1];
    let gap = if !heli.has_goal {
        Some(no_goal_unknown())
    } else {
        if heli.hovering {
            hover(heli, world, &mut notes);
        } else {
            let goal = heli.goal;
            fly_to(heli, goal, false, world, &mut notes);
        }
        None
    };
    turret_frame(heli, world, &mut notes);
    (notes, gap)
}
