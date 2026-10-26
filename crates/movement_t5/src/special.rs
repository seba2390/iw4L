//! Black Ops' moves for a player who does not walk: noclip and ufo fly
//! through the world, a spectator flies and slides along it.

use crate::physics::{accelerate, cmd_scale_air, friction, sqrt};
use crate::slide::step_slide_move;
use crate::state::{pm_flags, pm_type, view_height};
use crate::{MoveWorld, PlayerState, Pm, Pml, tuning};

/// The two buttons that move a flying player up and down.
const UP: u32 = 24;
const DOWN: u32 = 25;
/// A full stick or key, as a move command counts it.
const FULL_MOVE: f32 = 127.0;
const FLY_ACCELERATE: f32 = 9.0;
const SPECTATOR_ACCELERATE: f32 = 8.0;
/// Noclip and ufo slow down as walking friction would, half again as hard.
const FLY_FRICTION: f32 = 5.5;
const FLY_FRICTION_SCALE: f32 = 1.5;

/// How much of the player's speed a three-axis command asks for.
fn cmd_scale_3d(ps: &PlayerState, forward: f32, right: f32, up: f32) -> f32 {
    let max = libm::fabsf(forward)
        .max(libm::fabsf(right))
        .max(libm::fabsf(up));
    if max == 0.0 {
        return 0.0;
    }
    let total = sqrt(forward * forward + right * right + up * up);
    let mut scale = ps.speed as f32 * max / (total * FULL_MOVE);
    if ps.pm_flags & pm_flags::SIGHT_AIMING != 0 || ps.leanf != 0.0 {
        scale *= 0.4;
    }
    match ps.pm_type {
        pm_type::NOCLIP => scale *= 3.0,
        pm_type::UFO => scale *= tuning::UFO_SCALER,
        pm_type::SPECTATOR => scale *= tuning::SPECTATE_SPEED_SCALE,
        _ => {}
    }
    scale
}

/// The up and down buttons as a move: full up, full down, or none.
fn up_move<W: MoveWorld>(pm: &Pm<'_, W>) -> f32 {
    let mut up = 0.0;
    if pm.cmd.buttons.held(UP) {
        up = FULL_MOVE;
    }
    if pm.cmd.buttons.held(DOWN) {
        up -= FULL_MOVE;
    }
    up
}

/// Noclip and ufo's slowing: walking friction without the ground, or a
/// stop when slower than one unit a second.
fn fly_friction(ps: &mut PlayerState, pml: &Pml) {
    let v = ps.velocity;
    let speed = sqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
    if 1.0 > speed {
        ps.velocity = [0.0; 3];
        return;
    }
    let friction = FLY_FRICTION * FLY_FRICTION_SCALE;
    let control = if tuning::STOPSPEED > speed {
        tuning::STOPSPEED
    } else {
        speed
    };
    let mut newspeed = speed - pml.frametime * control * friction;
    if 0.0 > newspeed {
        newspeed = 0.0;
    }
    newspeed /= speed;
    for c in &mut ps.velocity {
        *c *= newspeed;
    }
}

/// A direction and its length; a length of 0 or less leaves it as it is.
fn normalize(v: [f32; 3]) -> ([f32; 3], f32) {
    let length = sqrt(v[2] * v[2] + v[1] * v[1] + v[0] * v[0]);
    let inverse = 1.0 / if -length >= 0.0 { 1.0 } else { length };
    ([inverse * v[0], v[1] * inverse, v[2] * inverse], length)
}

fn fly(ps: &mut PlayerState, pml: &Pml) {
    for k in 0..3 {
        ps.origin[k] += ps.velocity[k] * pml.frametime;
    }
}

/// Noclip: the player flies along the view through everything.
pub(crate) fn noclip_move<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    pm.ps.view_height_target = view_height::STAND;
    fly_friction(pm.ps, pml);
    let forward = f32::from(pm.cmd.forwardmove);
    let right = f32::from(pm.cmd.rightmove);
    let up = up_move(pm);
    let scale = cmd_scale_3d(pm.ps, forward, right, up);
    let wish: [f32; 3] =
        core::array::from_fn(|k| pml.up[k] * up + pml.right[k] * right + pml.forward[k] * forward);
    let (dir, length) = normalize(wish);
    accelerate(pm.ps, pml, dir, length * scale, FLY_ACCELERATE);
    fly(pm.ps, pml);
}

/// Ufo: the player flies level with the view's turn through everything,
/// straight up and down on the buttons, and stops when nothing is held.
pub(crate) fn ufo_move<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    pm.ps.view_height_target = view_height::STAND;
    let forward = f32::from(pm.cmd.forwardmove);
    let right = f32::from(pm.cmd.rightmove);
    let up = up_move(pm);
    if forward == 0.0 && right == 0.0 && up == 0.0 {
        pm.ps.velocity = [0.0; 3];
    } else {
        fly_friction(pm.ps, pml);
    }
    let scale = cmd_scale_3d(pm.ps, forward, right, up);
    let wish = level_wish(pml.right, forward, right, up);
    let (dir, length) = normalize(wish);
    accelerate(pm.ps, pml, dir, length * scale, FLY_ACCELERATE);
    fly(pm.ps, pml);
}

/// A move along the level forward (straight up crossed with `right`),
/// `right` and straight up.
fn level_wish(r: [f32; 3], forward: f32, right: f32, up: f32) -> [f32; 3] {
    let level = [
        r[2] * 0.0 - r[1],
        r[0] - r[2] * 0.0,
        r[1] * 0.0 - r[0] * 0.0,
    ];
    [
        r[0] * right + level[0] * forward + up * 0.0,
        r[1] * right + level[1] * forward + up * 0.0,
        r[2] * right + level[2] * forward + up,
    ]
}

/// A spectator: the player flies level with the view's turn, up and down
/// on the buttons, and slides along the world.
pub(crate) fn spectator_move<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &mut Pml) {
    friction(pm, pml);
    let scale = cmd_scale_air(pm.ps, &pm.cmd);
    let mut wish = if scale == 0.0 {
        [0.0; 3]
    } else {
        let forward = f32::from(pm.cmd.forwardmove);
        let right = f32::from(pm.cmd.rightmove);
        level_wish(pml.right, forward, right, 0.0).map(|c| c * scale)
    };
    if pm.ps.speed != 0 {
        let up = cmd_scale_3d(pm.ps, 0.0, 0.0, FULL_MOVE);
        if pm.cmd.buttons.held(DOWN) {
            wish[2] -= up * FULL_MOVE;
        }
        if pm.cmd.buttons.held(UP) {
            wish[2] += up * FULL_MOVE;
        }
    }
    let (dir, length) = normalize(wish);
    accelerate(pm.ps, pml, dir, length, SPECTATOR_ACCELERATE);
    step_slide_move(pm, pml, false);
}
