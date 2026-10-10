//! How the eye height follows a stance change: along fixed curves for
//! stand, crouch and prone, at a steady rate for anything else.

use crate::state::{pm_type, view_height};
use crate::{MoveWorld, Pm, Pml};

/// `(percent of the change, eye height)`.
type Curve = &'static [(i32, f32)];

const STAND_TO_CROUCH: Curve = &[
    (0, 60.0),
    (1, 59.5),
    (4, 58.5),
    (30, 56.0),
    (80, 44.0),
    (90, 41.5),
    (95, 40.5),
    (100, 40.0),
];
const CROUCH_TO_STAND: Curve = &[
    (0, 40.0),
    (5, 40.5),
    (10, 41.5),
    (20, 44.0),
    (70, 56.0),
    (96, 58.5),
    (99, 59.5),
    (100, 60.0),
];
const CROUCH_TO_PRONE: Curve = &[
    (0, 40.0),
    (11, 38.0),
    (22, 33.0),
    (34, 25.0),
    (45, 16.0),
    (50, 15.0),
    (55, 16.0),
    (70, 18.0),
    (90, 17.0),
    (100, 11.0),
];
const PRONE_TO_CROUCH: Curve = &[
    (0, 11.0),
    (5, 10.0),
    (30, 21.0),
    (50, 25.0),
    (67, 31.0),
    (83, 34.0),
    (100, 40.0),
];

fn curve(target: i32, down: bool) -> Curve {
    match target {
        view_height::PRONE => CROUCH_TO_PRONE,
        view_height::CROUCH if down => STAND_TO_CROUCH,
        view_height::CROUCH => PRONE_TO_CROUCH,
        _ => CROUCH_TO_STAND,
    }
}

/// How long a stance change of the eye takes, in ms.
fn duration(target: i32, down: bool) -> i32 {
    match target {
        view_height::PRONE => 400,
        view_height::CROUCH => {
            if down {
                200
            } else {
                400
            }
        }
        _ => 200,
    }
}

fn height_at(curve: Curve, percent: i32) -> f32 {
    if percent == 0 {
        return curve[0].1;
    }
    for i in 1..curve.len() {
        let (at, height) = curve[i];
        if at == percent {
            return height;
        }
        if at > percent {
            let (prev_at, prev_height) = curve[i - 1];
            let t = (percent - prev_at) as f32 / (at - prev_at) as f32;
            return (height - prev_height) * t + prev_height;
        }
    }
    curve[0].1
}

pub(crate) fn update<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let server_time = pm.cmd.server_time;
    let ps = &mut *pm.ps;
    let target = ps.view_height_target;
    if target == 0 || ps.view_height_current == 0.0 {
        ps.view_height_current = if ps.pm_type == pm_type::SPECTATOR {
            0.0
        } else {
            target as f32
        };
        return;
    }
    let target_f = target as f32;
    if ps.view_height_current == target_f && ps.view_height_lerp_time == 0 {
        return;
    }
    if target != view_height::PRONE && target != view_height::CROUCH && target != view_height::STAND
    {
        ps.view_height_lerp_time = 0;
        let step = pml.frametime * 180.0;
        let current = ps.view_height_current;
        if target_f > current {
            let next = step + current;
            ps.view_height_current = if target_f > next { next } else { target_f };
        } else {
            let next = current - step;
            ps.view_height_current = if target_f > next { target_f } else { next };
        }
        return;
    }
    let mut percent = 0;
    if ps.view_height_lerp_time != 0 {
        let lerp_target = ps.view_height_lerp_target;
        let down = ps.view_height_lerp_down != 0;
        percent = (server_time - ps.view_height_lerp_time) * 100 / duration(lerp_target, down);
        if percent < 0 {
            percent = 0;
            ps.view_height_current = height_at(curve(lerp_target, down), percent);
        } else if percent >= 100 {
            percent = 100;
            if ps.pm_type == pm_type::LAST_STAND_GETTING_UP {
                ps.pm_type = pm_type::NORMAL;
            }
            ps.view_height_current = lerp_target as f32;
            ps.view_height_lerp_time = 0;
        } else {
            ps.view_height_current = height_at(curve(lerp_target, down), percent);
        }
    }
    let target = ps.view_height_target;
    if ps.view_height_lerp_time != 0 {
        let lerp_target = ps.view_height_lerp_target;
        if target == lerp_target {
            return;
        }
        let down = ps.view_height_lerp_down;
        let reverse = (target < lerp_target && down == 0) || (target > lerp_target && down != 0);
        if !reverse {
            return;
        }
        let left = 100 - percent;
        let down = down ^ 1;
        ps.view_height_lerp_down = down;
        ps.view_height_lerp_target = match (down != 0, lerp_target) {
            (true, view_height::STAND) => view_height::CROUCH,
            (true, view_height::CROUCH) => view_height::PRONE,
            (false, view_height::PRONE) => view_height::CROUCH,
            (false, view_height::CROUCH) => view_height::STAND,
            (_, other) => other,
        };
        let lerp_target = ps.view_height_lerp_target;
        if left == 100 {
            ps.view_height_current = lerp_target as f32;
            ps.view_height_lerp_time = 0;
            return;
        }
        let elapsed =
            (duration(lerp_target, down != 0) as f32 * left as f32 * 0.00999999978) as i32;
        ps.view_height_lerp_time = server_time - elapsed;
        ps.view_height_current = height_at(curve(lerp_target, down != 0), left);
        return;
    }
    let current = ps.view_height_current;
    if current == target as f32 {
        return;
    }
    ps.view_height_lerp_time = server_time;
    match target {
        view_height::PRONE => {
            ps.view_height_lerp_down = 1;
            ps.view_height_lerp_target = if current > 40.0 {
                view_height::CROUCH
            } else {
                view_height::PRONE
            };
        }
        view_height::CROUCH => {
            ps.view_height_lerp_down = i32::from(current > 40.0);
            ps.view_height_lerp_target = view_height::CROUCH;
        }
        _ => {
            ps.view_height_lerp_down = 0;
            ps.view_height_lerp_target = if 40.0 > current {
                view_height::CROUCH
            } else {
                view_height::STAND
            };
        }
    }
}
