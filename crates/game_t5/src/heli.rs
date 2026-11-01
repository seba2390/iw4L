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
        if heli.piloted.pilot.is_some() || heli.piloted.yaw_rate != 0.0 || heli.speed != 0.0 {
            piloted_move(heli, world, &mut notes);
        }
        None
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

const FRAME_S: f32 = 0.05;
const STICK: f32 = 1.0 / 127.0;
/// The hull a helicopter's own trace sweeps (`spawnhelicopter`).
const HULL_MINS: [f32; 3] = [-50.0; 3];
const HULL_MAXS: [f32; 3] = [50.0; 3];
/// The share of its speed a helicopter keeps hitting a surface head on.
const HEAD_ON: f32 = 0.95;
const COLLISION_KEEP: f32 = 0.5;
const YAW_DAMP: f32 = 12.0;
const GRAPH_YAW_LIMIT: f32 = 80.0;
const GRAPH_YAW_AXIS: f32 = 10.0;
const LIFT_STEP: f32 = 50.0;
const LIFT_MAX: f32 = 2000.0;

fn blend_to(goal: f32, now: f32, rate: f32, dt: f32) -> f32 {
    let step = (goal - now) * rate * dt;
    let left = (goal - now).abs();
    if 0.05 < left && step.abs() <= left {
        now + step
    } else {
        goal
    }
}

fn dvar_or(world: &dyn HeliWorld, name: &str, default: f32) -> f32 {
    dvar_f32(world, name, default)
}

fn axes_of(angles: [f32; 3]) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let (sp, cp) = (angles[0].to_radians().sin(), angles[0].to_radians().cos());
    let (sy, cy) = (angles[1].to_radians().sin(), angles[1].to_radians().cos());
    let (sr, cr) = (angles[2].to_radians().sin(), angles[2].to_radians().cos());
    (
        [cp * cy, cp * sy, -sp],
        [-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp],
        [cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp],
    )
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn graph_value(motion: &game_api::heli::VehicleMotion, x: f32) -> f32 {
    let points = &motion.steer_graph[..usize::from(motion.steer_points)];
    let Some(first) = points.first() else {
        return x;
    };
    if x <= first[0] {
        return first[1];
    }
    for pair in points.windows(2) {
        if x <= pair[1][0] {
            let span = pair[1][0] - pair[0][0];
            let t = if span > 0.0 {
                (x - pair[0][0]) / span
            } else {
                0.0
            };
            return pair[0][1] + (pair[1][1] - pair[0][1]) * t;
        }
    }
    points[points.len() - 1][1]
}

/// The player's flight (BlackOpsMP.exe's helicopter move with no goal): their
/// sticks, or none with no one flying it, drive a wanted velocity along the
/// helicopter's axes and a turn toward where they look.
fn piloted_move(heli: &mut HeliMove, world: &mut dyn HeliWorld, notes: &mut HeliNotes) {
    let motion = heli.piloted.motion;
    let pilot = heli
        .piloted
        .pilot
        .unwrap_or(game_api::heli::HeliPilot::default());
    let mut sticks = [
        f32::from(pilot.forward),
        f32::from(pilot.right),
        0.0_f32,
        0.0_f32,
    ];
    let yaw = heli.previous_yaw;
    if heli.piloted.pilot.is_some() {
        // `vehHelicopterYawAltitudeControls` 2: the look stick turns it and
        // the two keys climb and descend.
        sticks[3] = f32::from(pilot.yaw_axis);
        let rate = dvar_or(world, "vehHelicopterAlwaysFaceCameraRate", 2.0);
        let delta = normalize_180(pilot.view[1] - yaw);
        if motion.steer_points == 0 {
            let wanted = (delta * rate - heli.piloted.yaw_rate * YAW_DAMP).clamp(-180.0, 180.0);
            sticks[3] = wanted.clamp(-127.0, 127.0).trunc();
        } else {
            let share = (delta / motion.turret_span_right * rate).clamp(-1.0, 1.0);
            let mut turn = graph_value(&motion, share.abs());
            if delta < 0.0 {
                turn = -turn;
            }
            let turn = (turn * 128.0).clamp(-GRAPH_YAW_LIMIT, GRAPH_YAW_LIMIT);
            sticks[3] = (f32::from(pilot.yaw_axis) * GRAPH_YAW_AXIS / 128.0 + turn).trunc();
        }
    }
    if heli.height_lock {
        if pilot.climb {
            heli.piloted.lift += LIFT_STEP;
        } else if pilot.descend {
            heli.piloted.lift -= LIFT_STEP;
        }
        heli.piloted.lift = heli.piloted.lift.clamp(0.0, LIFT_MAX);
        sticks[2] = altitude_stick(heli, world);
    }
    let (accel, rotate) = piloted_accel(heli, &motion, sticks, pilot.view, pilot.time_ms, world);
    heli.piloted.yaw_rate += rotate[1] * FRAME_S;
    let decel = dvar_or(world, "vehHelicopterRotDecel", 1.3);
    heli.piloted.yaw_rate -= decel * heli.piloted.yaw_rate * FRAME_S;
    let new_yaw = (heli.piloted.yaw_rate * FRAME_S + yaw).rem_euclid(360.0);
    heli.angles = [rotate[0], new_yaw, rotate[2]];
    let (forward, right, up) = axes_of([0.0, new_yaw, 0.0]);
    let push: [f32; 3] =
        std::array::from_fn(|i| forward[i] * accel[0] + right[i] * accel[1] + up[i] * accel[2]);
    for (v, p) in heli.velocity.iter_mut().zip(push) {
        *v += p * FRAME_S;
    }
    if heli.velocity != [0.0; 3] {
        let wanted: [f32; 3] = std::array::from_fn(|i| heli.origin[i] + heli.velocity[i] * FRAME_S);
        slide_move(heli, world);
        let moved: [f32; 3] = std::array::from_fn(|i| wanted[i] - heli.origin[i]);
        let length = dot3(moved, moved).sqrt();
        if 0.0 < length {
            let dir = moved.map(|v| v / length);
            notes.collision = Some(dir);
            let speed = dot3(heli.velocity, heli.velocity).sqrt();
            let v_dir = if speed > 0.0 {
                heli.velocity.map(|v| v / speed)
            } else {
                [0.0; 3]
            };
            if HEAD_ON < dot3(dir, v_dir) {
                heli.velocity = heli.velocity.map(|v| v * COLLISION_KEEP);
            }
        }
    }
    let lookahead = dvar_or(world, "vehHelicopterLookaheadTime", 1.0);
    if lookahead != 0.0 && heli.velocity != [0.0; 3] {
        let ahead: [f32; 3] =
            std::array::from_fn(|i| heli.origin[i] + heli.velocity[i] * lookahead);
        if let Some((fraction, normal)) = world.sweep(heli.origin, ahead, HULL_MINS, HULL_MAXS)
            && fraction < 1.0
        {
            notes.predicted_collision = Some(normal);
        }
    }
    let (fw, rg, upv) = axes_of([0.0, new_yaw, 0.0]);
    heli.piloted.local_velocity = [
        dot3(heli.velocity, fw),
        dot3(heli.velocity, rg),
        dot3(heli.velocity, upv),
    ];
    swing_with_yaw(heli);
    heli.speed = length(heli.velocity);
}

fn altitude_stick(heli: &mut HeliMove, world: &mut dyn HeliWorld) -> f32 {
    let at = [
        heli.velocity[0] + heli.origin[0],
        heli.velocity[1] + heli.origin[1],
        0.0,
    ];
    let height = heli.origin[2] + HULL_MINS[2] + heli.velocity[2] - heli.piloted.lift;
    let probe = [at[0], at[1], height];
    if let Some(ground) = world
        .height_lock_at(probe, 200.0)
        .or_else(|| world.height_lock_at(probe, 3000.0))
    {
        heli.piloted.ground = ground;
    }
    let off = height - heli.piloted.ground;
    let mut share = 1.0;
    let distance = off.abs();
    if distance < 50.0 {
        share = if distance < 15.0 {
            0.0
        } else {
            distance / 50.0
        };
    }
    if off < 0.0 {
        if heli.velocity[2] < -4.0 {
            share = 1.0;
        }
        (share * 127.0).trunc()
    } else {
        if 4.0 < heli.velocity[2] {
            share = 1.0;
        }
        (share * -127.0).trunc()
    }
}

/// BlackOpsMP.exe's move for a piloted helicopter: the wanted velocity and how
/// fast it gets there, its turn and its tilt.
fn piloted_accel(
    heli: &mut HeliMove,
    motion: &game_api::heli::VehicleMotion,
    sticks: [f32; 4],
    view: [f32; 3],
    time_ms: i32,
    world: &mut dyn HeliWorld,
) -> ([f32; 3], [f32; 3]) {
    let mut input = sticks.map(|v| v * STICK);
    if sticks[0] != 0.0 || sticks[1] != 0.0 {
        let largest = sticks[0].abs().max(sticks[1].abs());
        if largest != 0.0 {
            let scale = largest / (sticks[0] * sticks[0] + sticks[1] * sticks[1]).sqrt();
            input[0] *= scale;
            input[1] *= scale;
        }
        if input[1].abs() < dvar_or(world, "vehHelicopterStrafeDeadzone", 0.3) {
            input[1] = 0.0;
        }
        // `vehHelicopterScaleMovement`.
        let (a, b) = (input[0].abs(), input[1].abs());
        if a <= b {
            input[0] *= 1.0 - (b - a);
        } else {
            input[1] *= 1.0 - (a - b);
        }
    }
    let deadzone = dvar_or(world, "vehHelicopterRightStickDeadzone", 0.3);
    if input[2].abs() < deadzone {
        input[2] = 0.0;
    }
    let vert_speed = dvar_or(world, "vehHelicopterMaxSpeedVertical", 65.0) * MPH;
    let vert_accel = dvar_or(world, "vehHelicopterMaxAccelVertical", 60.0) * MPH;
    let (max_xy, accel_xy) = (motion.max_speed, motion.accel);
    let rate = [
        if max_xy == 0.0 {
            1.0
        } else {
            accel_xy / max_xy
        },
        if max_xy == 0.0 {
            1.0
        } else {
            accel_xy / max_xy
        },
        if vert_speed == 0.0 {
            1.0
        } else {
            vert_accel / vert_speed
        },
    ];
    let wanted = [max_xy * input[0], max_xy * input[1], input[2] * vert_speed];
    let wanted_speed =
        (wanted[0] * wanted[0] + wanted[1] * wanted[1] + wanted[2] * wanted[2]).sqrt();
    let unit = if wanted_speed > 0.0 {
        wanted.map(|v| v / wanted_speed)
    } else {
        [0.0; 3]
    };
    let local = heli.piloted.local_velocity;
    let along = dot3(local, unit);
    let across: [f32; 3] = std::array::from_fn(|i| local[i] - unit[i] * along);
    let parallel: [f32; 3] = std::array::from_fn(|i| unit[i] * along);
    let decel_fwd = dvar_or(world, "vehHelicopterDecelerationFwd", 0.5);
    let decel_side = dvar_or(world, "vehHelicopterDecelerationSide", 1.0);
    let decels = [decel_fwd, decel_side];
    let mut accel = [0.0_f32; 3];
    let mut across_push = [0.0_f32; 2];
    let mut along_push = [0.0_f32; 2];
    for i in 0..2 {
        let slow = decels[i] * rate[i];
        let settle = (blend_to(0.0, across[i], slow, FRAME_S) - across[i]) * 20.0;
        across_push[i] = settle;
        if wanted_speed <= along {
            let over = (blend_to(wanted[i], parallel[i], slow, FRAME_S) - parallel[i]) * 20.0;
            across_push[i] = settle + over;
        } else {
            along_push[i] =
                (blend_to(wanted[i], parallel[i], rate[i], FRAME_S) - parallel[i]) * 20.0;
        }
        let total = (along_push[i] + across_push[i]).clamp(-accel_xy, accel_xy);
        accel[i] = total;
        across_push[i] = across_push[i].clamp(-accel_xy, accel_xy);
        along_push[i] = along_push[i].clamp(-accel_xy, accel_xy);
    }
    accel[2] = ((blend_to(wanted[2], local[2], rate[2], FRAME_S) - local[2]) * 20.0)
        .clamp(-vert_accel, vert_accel);
    let rot_rate = motion.rot_rate;
    let yaw_on_stick = dvar_or(world, "vehHelicopterYawOnLeftStick", 5.0);
    let wanted_yaw_rate =
        (rot_rate * input[3] - yaw_on_stick * input[1] * input[0]).clamp(-rot_rate, rot_rate);
    let yaw_accel_rate = if rot_rate == 0.0 {
        0.0
    } else {
        motion.rot_accel / rot_rate
    };
    let yaw_push = (blend_to(
        wanted_yaw_rate,
        heli.piloted.yaw_rate,
        yaw_accel_rate,
        FRAME_S,
    ) - heli.piloted.yaw_rate)
        * 20.0;
    let yaw_push = yaw_push.clamp(-accel_xy, accel_xy);
    let (max_x, accel_x) = (
        if max_xy <= 0.0 {
            motion.max_speed
        } else {
            max_xy
        },
        if accel_xy <= 0.0 {
            motion.accel
        } else {
            accel_xy
        },
    );
    let axes = dvar_or(world, "vehHelicopterTiltFromControllerAxes", 0.0);
    let from_view = dvar_or(world, "vehHelicopterTiltFromViewangles", 3.0);
    let mut tilt_pitch = along_push[0] / accel_x * motion.tilt_from_accel[0]
        + across_push[0] / accel_x * motion.tilt_from_decel[0]
        + wanted[0] / max_x * axes
        + local[0] / max_x * motion.tilt_from_velocity[0];
    tilt_pitch = tilt_pitch.clamp(-1.0, 1.0);
    let view_share = normalize_180(view[0] - motion.view_pitch_offset) * from_view / 90.0;
    tilt_pitch = (view_share - tilt_pitch) * 0.5 + tilt_pitch;
    let mut tilt_roll = along_push[1] / accel_x * motion.tilt_from_accel[1]
        + across_push[1] / accel_x * motion.tilt_from_decel[1]
        + wanted[1] / max_x * axes
        + local[1] / max_x * motion.tilt_from_velocity[1];
    if 0.0 < local[0] && 0.0 < heli.piloted.yaw_rate.abs() {
        let forward_share = local[0] / max_x;
        let at_max = dvar_or(world, "vehHelicopterTiltFromFwdAndYaw_VelAtMaxTilt", 1.0);
        let scale = if forward_share < at_max {
            forward_share / at_max
        } else {
            1.0
        };
        tilt_roll += -1.0 / rot_rate
            * heli.piloted.yaw_rate
            * dvar_or(world, "vehHelicopterTiltFromFwdAndYaw", 0.0)
            * scale;
    }
    tilt_roll = tilt_roll.clamp(-1.0, 1.0);
    let momentum = 1.0 / dvar_or(world, "vehHelicopterTiltMomentum", 0.4);
    let (fw, rg, _) = axes_of([0.0, heli.previous_yaw, 0.0]);
    let goal = [
        fw[0] * tilt_pitch + rg[0] * tilt_roll,
        fw[1] * tilt_pitch + rg[1] * tilt_roll,
    ];
    let speeds = motion.tilt_speed;
    for i in 0..2 {
        let before = heli.piloted.tilt_rate[i];
        let now = (before * -momentum + speeds[i] * momentum * (goal[i] - heli.piloted.tilt[i]))
            * FRAME_S
            + before;
        heli.piloted.tilt_rate[i] = now;
        heli.piloted.tilt[i] += (now + before) * 0.5 * FRAME_S;
    }
    let tilt = heli.piloted.tilt;
    let local_tilt = [
        tilt[0] * fw[0] + tilt[1] * fw[1],
        tilt[0] * rg[0] + tilt[1] * rg[1],
    ];
    let mut pitch = motion.max_body_pitch * local_tilt[0];
    let mut roll = motion.max_body_roll * local_tilt[1];
    let hover = dvar_or(world, "vehHelicopterHoverSpeedThreshold", 400.0);
    let flat = local[0] * local[0] + local[1] * local[1];
    if flat < hover * hover {
        let share = (hover - flat.sqrt()) / hover;
        let jitter = jitter_step(&mut heli.piloted.jitter, time_ms, world);
        pitch += jitter[0] * share;
        roll += jitter[2] * share;
    }
    (accel, [pitch, yaw_push, roll])
}

/// `setjitterparams`' shake: a new goal each random period, stepped to and
/// smoothed by `vehHelicopterJitterJerkyness`.
fn jitter_step(
    jitter: &mut game_api::heli::HeliJitter,
    now: i32,
    world: &mut dyn HeliWorld,
) -> [f32; 3] {
    if jitter.min_ms == 0 && jitter.max_ms == 0 {
        return jitter.smoothed;
    }
    if jitter.next_ms < now {
        if (jitter.min_ms as f32) < FRAME_S {
            jitter.min_ms = 50;
        }
        let period = world
            .random_range(jitter.min_ms as f32, jitter.max_ms as f32)
            .trunc() as i32;
        jitter.next_ms = now + period;
        let mut goal = jitter.goal;
        for (i, slot) in goal.iter_mut().enumerate() {
            let amp = jitter.amplitude[i];
            if amp != 0.0 {
                *slot = world.random_range(-amp, amp);
            }
        }
        let frames = 50.0 / period.max(1) as f32;
        jitter.step = std::array::from_fn(|i| (goal[i] - jitter.goal[i]) * frames);
        jitter.goal = goal;
    }
    let jerk = dvar_or(world, "vehHelicopterJitterJerkyness", 0.3);
    for i in 0..3 {
        jitter.smoothed[i] = blend_to(jitter.goal[i], jitter.smoothed[i], jerk, FRAME_S);
    }
    jitter.smoothed
}

/// The helicopter's move through the world: up to four slides along what it
/// meets.
fn slide_move(heli: &mut HeliMove, world: &mut dyn HeliWorld) {
    let mut time_left = FRAME_S;
    for _ in 0..4 {
        let end: [f32; 3] = std::array::from_fn(|i| heli.origin[i] + heli.velocity[i] * time_left);
        let Some((fraction, normal)) = world.sweep(heli.origin, end, HULL_MINS, HULL_MAXS) else {
            heli.velocity[2] = 0.0;
            return;
        };
        if 0.0 < fraction {
            heli.origin =
                std::array::from_fn(|i| heli.origin[i] + (end[i] - heli.origin[i]) * fraction);
        }
        if fraction >= 1.0 {
            return;
        }
        time_left -= fraction * time_left;
        let into = dot3(heli.velocity, normal);
        let bounce = if into < 0.0 { 1.001 } else { 0.999 };
        heli.velocity = std::array::from_fn(|i| heli.velocity[i] - normal[i] * into * bounce);
    }
}
