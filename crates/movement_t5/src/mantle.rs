//! Black Ops' mantle: the check that finds a ledge ahead, the start that
//! sets the mantle up on the mantle animations, the move that carries the
//! player along their root translation, and the yaw cap while it runs.

use crate::events::{anim, event};
use crate::math::{angle_delta, vectoyaw};
use crate::physics::{divisor, sqrt};
use crate::state::{PlayerState, buttons, e_flags, pm_flags, pm_type};
use crate::view_angles::wrap_360;
use crate::{Gap, MoveEvent, MoveWorld, Pm, Pml, tuning};

/// Black Ops' mantle state flags (`mantleState.flags`).
pub mod flag {
    /// The mantle goes over the ledge: the over animation follows the climb.
    pub const OVER: u32 = 0x1;
    /// The surface is a mantle-onto one.
    pub const ON: u32 = 0x2;
    /// Standing is blocked at the ledge: the player is forced to crouch.
    pub const CROUCH: u32 = 0x4;
    /// The player can stand where the mantle ends.
    pub const STAND_AT_END: u32 = 0x8;
    /// A ledge is in reach this frame.
    pub const AVAILABLE: u32 = 0x10;
    /// The surface takes a mounted weapon.
    pub const MOUNT_SURFACE: u32 = 0x20;
    /// The mantle has run to its end.
    pub const DONE: u32 = 0x100;
    /// A mounted weapon is set down on a ledge: no mantle is offered.
    pub const MOUNTED: u32 = 0x200;
}

/// The `PLAYER_MANTLE` animation tree, in Black Ops' order.
const ANIMS: [&str; 11] = [
    "player_mantle_root",
    "player_mantle_up_57",
    "player_mantle_up_51",
    "player_mantle_up_45",
    "player_mantle_up_39",
    "player_mantle_up_33",
    "player_mantle_up_27",
    "player_mantle_up_21",
    "player_mantle_over_high",
    "player_mantle_over_mid",
    "player_mantle_over_low",
];

/// Each transition's climb animation, over animation and ledge height.
pub const TRANSITIONS: [(usize, usize, f32); 7] = [
    (1, 8, 57.0),
    (2, 8, 51.0),
    (3, 9, 45.0),
    (4, 9, 39.0),
    (5, 9, 33.0),
    (6, 10, 27.0),
    (7, 10, 21.0),
];

const LEDGE_HEIGHTS: [f32; 3] = [60.0, 40.0, 20.0];
const SURF_MANTLE_OVER: u32 = 0x0800_0000;
const SURF_MANTLE_ON: u32 = 0x0400_0000;
const SURF_MOUNT: u32 = 0x1000_0000;
const SURF_MANTLE_ANY: u32 = SURF_MANTLE_OVER | SURF_MANTLE_ON | SURF_MOUNT;
const CONTENTS_MANTLE: u32 = 0x0100_0000;
/// The player trace mask less bodies and the 0x4000 and 0x8000 contents.
const LEDGE_MASK: u32 = 0xfdff_3fff;
const PLAYER_HALF_WIDTH: f32 = 15.0;
const FORWARD_DIST: f32 = 16.0;
const LEDGE_FLOOR: f32 = 18.0;
const OVER_FORWARD: f32 = 31.0;
const FRONT_MAXS_Z: f32 = 70.0;
const STEP_MAXS_Z: f32 = 30.0;
const CLEARANCE_MAXS_Z: f32 = 50.0;
const MOUNT_HEIGHT: f32 = 50.0;
const RAD2DEG: f64 = 57.295_776_367_187_5;

#[derive(Clone, Copy, Debug, Default)]
struct Results {
    dir: [f32; 3],
    start_pos: [f32; 3],
    ledge_pos: [f32; 3],
    end_pos: [f32; 3],
    flags: u32,
}

fn add(a: [f32; 3], b: [f32; 3], scale: f32) -> [f32; 3] {
    [
        b[0] * scale + a[0],
        b[1] * scale + a[1],
        b[2] * scale + a[2],
    ]
}

fn transition_of(ps: &PlayerState) -> (usize, usize, f32) {
    TRANSITIONS[ps.mantle_trans_index.clamp(0, 6) as usize]
}

/// The climb animation's length and the over animation's (0 when the
/// mantle does not go over).
fn lengths<W: MoveWorld>(pm: &Pm<'_, W>, ps: &PlayerState) -> Option<(i32, i32)> {
    let (up, over, _) = transition_of(ps);
    let up_len = pm.world.anim_length_ms(ANIMS[up])?;
    let over_len = if ps.mantle_flags & flag::OVER != 0 {
        pm.world.anim_length_ms(ANIMS[over])?
    } else {
        0
    };
    Some((up_len, over_len))
}

/// The whole mantle's length.
fn duration<W: MoveWorld>(pm: &Pm<'_, W>) -> Option<i32> {
    lengths(pm, pm.ps).map(|(up, over)| up + over)
}

/// The root translation the mantle has made `time` ms in, turned to the
/// mantle's yaw.
fn delta_at<W: MoveWorld>(pm: &Pm<'_, W>, time: i32) -> Option<[f32; 3]> {
    let ps = &*pm.ps;
    let (up, over, _) = transition_of(ps);
    let (up_len, over_len) = lengths(pm, ps)?;
    let mut delta = if up_len < time {
        let climb = pm.world.anim_root_delta(ANIMS[up], 1.0)?;
        let rest = pm
            .world
            .anim_root_delta(ANIMS[over], (time - up_len) as f32 / over_len as f32)?;
        [rest[0] + climb[0], rest[1] + climb[1], rest[2] + climb[2]]
    } else {
        pm.world
            .anim_root_delta(ANIMS[up], time as f32 / up_len as f32)?
    };
    let yaw = f64::from(ps.mantle_yaw * 0.017_453_292);
    let (c, s) = (libm::cos(yaw) as f32, libm::sin(yaw) as f32);
    let y = delta[1];
    delta[1] = y * c + delta[0] * s;
    delta[0] = delta[0] * c - y * s;
    Some(delta)
}

/// The animation running `time` ms into the mantle.
fn anim_at<W: MoveWorld>(pm: &Pm<'_, W>, time: i32) -> Option<usize> {
    let (up, over, _) = transition_of(pm.ps);
    let (up_len, _) = lengths(pm, pm.ps)?;
    Some(if time <= up_len { up } else { over })
}

/// The transition whose ledge height is nearest the climb.
fn transition(start_z: f32, ledge_z: f32) -> i32 {
    let height = ledge_z - start_z;
    let mut best = 0;
    let mut best_diff = libm::fabsf(TRANSITIONS[0].2 - height);
    for (i, t) in TRANSITIONS.iter().enumerate().skip(1) {
        let diff = libm::fabsf(t.2 - height);
        if best_diff > diff {
            best = i as i32;
            best_diff = diff;
        }
    }
    best
}

/// The mantle surface ahead along the view: the direction into it and its
/// surface flags.
fn front_probe<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) -> Option<([f32; 3], u32)> {
    let r = tuning::MANTLE_CHECK_RADIUS;
    let inner = PLAYER_HALF_WIDTH - r;
    let range = tuning::MANTLE_CHECK_RANGE + inner;
    let f = pml.forward;
    let n = 1.0 / divisor(sqrt(f[1] * f[1] + f[0] * f[0]));
    let facing = [f[0] * n, f[1] * n, n * 0.0];
    let o = pm.ps.origin;
    let start = add(o, facing, -inner);
    let end = add(o, facing, range);
    let t = pm.trace_plain(
        start,
        end,
        [-r, -r, 0.0],
        [r, r, FRONT_MAXS_Z],
        CONTENTS_MANTLE,
    );
    if t.startsolid || t.allsolid || t.fraction == 1.0 || t.surface_flags & SURF_MANTLE_ANY == 0 {
        return None;
    }
    let mut dir = [-t.normal[0], -t.normal[1], 0.0];
    let len = sqrt(dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]);
    if len != 0.0 {
        let inv = 1.0 / len;
        dir = [dir[0] * inv, dir[1] * inv, dir[2] * inv];
    }
    if 0.0001 > len {
        return None;
    }
    let dot = f64::from(facing[1]) * f64::from(dir[1])
        + f64::from(facing[0]) * f64::from(dir[0])
        + f64::from(facing[2]) * f64::from(dir[2]);
    let angle = (libm::acos(dot) * RAD2DEG) as f32;
    if angle > tuning::MANTLE_CHECK_ANGLE {
        return None;
    }
    Some((dir, t.surface_flags))
}

/// A ledge `height` above the start: room to reach over it, a walkable top
/// to stand on and no glass in the way.
fn ledge_at<W: MoveWorld>(pm: &mut Pm<'_, W>, results: &mut Results, height: f32) -> bool {
    let mask = pm.tracemask & LEDGE_MASK;
    let mins = [-PLAYER_HALF_WIDTH, -PLAYER_HALF_WIDTH, 0.0];
    let step = [PLAYER_HALF_WIDTH, PLAYER_HALF_WIDTH, STEP_MAXS_Z];
    let s = results.start_pos;
    let raised = [s[0], s[1], s[2] + height];
    let reach = add(raised, results.dir, FORWARD_DIST);
    let t = pm.trace_plain(raised, reach, mins, step, mask);
    if t.startsolid || t.fraction < 1.0 {
        return false;
    }
    let drop_end = [reach[0], reach[1], s[2] + LEDGE_FLOOR];
    let t = pm.trace_plain(reach, drop_end, mins, step, mask);
    if t.startsolid || t.fraction == 1.0 || !t.walkable {
        return false;
    }
    results.ledge_pos = [
        drop_end[0],
        drop_end[1],
        (drop_end[2] - reach[2]) * t.fraction + reach[2],
    ];
    let ledge = results.ledge_pos;
    let clearance = [PLAYER_HALF_WIDTH, PLAYER_HALF_WIDTH, CLEARANCE_MAXS_Z];
    let tracemask = pm.tracemask;
    if pm
        .trace_plain(ledge, ledge, mins, clearance, tracemask)
        .startsolid
    {
        return false;
    }
    let glass_reach = tuning::MANTLE_CHECK_GLASS_EXTRA_RANGE + FORWARD_DIST;
    let far = add(raised, results.dir, glass_reach);
    let t = pm.trace_plain(raised, far, mins, step, mask);
    if t.startsolid || (t.fraction < 1.0 && t.glass) {
        return false;
    }
    if pm.ps.mantle_flags & flag::MOUNTED == 0 {
        mount_at_ledge(pm, results);
    }
    if results.flags & (flag::OVER | flag::ON) == 0 {
        return false;
    }
    pm.ps.mantle_flags |= flag::AVAILABLE;
    results.flags |= flag::AVAILABLE;
    over_path(pm, results);
    if pm.cmd.buttons.held(buttons::JUMP) {
        if pm.ps.e_flags & e_flags::CROUCHING == 0 {
            let stand = [PLAYER_HALF_WIDTH, PLAYER_HALF_WIDTH, FRONT_MAXS_Z];
            let (ledge, end) = (results.ledge_pos, results.end_pos);
            if pm
                .trace_plain(ledge, ledge, mins, stand, tracemask)
                .startsolid
            {
                results.flags |= flag::CROUCH;
            }
            if !pm.trace_plain(end, end, mins, stand, tracemask).startsolid {
                results.flags |= flag::STAND_AT_END;
            }
        }
        start(pm, results);
    }
    true
}

/// A mounted weapon set down on the ledge: not run.
fn mount_at_ledge<W: MoveWorld>(pm: &mut Pm<'_, W>, results: &Results) {
    let weapon = pm.world.weapon(pm.ps.weapon);
    if weapon.def.mountable_weapon()
        && results.flags & flag::MOUNT_SURFACE != 0
        && libm::fabsf(results.ledge_pos[2] - pm.ps.origin[2]) < MOUNT_HEIGHT
    {
        pm.gap(Gap::MountedView);
    }
}

/// Over the ledge: the far side 31 units on must be clear and drop at
/// least 18; otherwise the mantle ends on the ledge.
fn over_path<W: MoveWorld>(pm: &mut Pm<'_, W>, results: &mut Results) {
    if results.flags & flag::OVER != 0 {
        let mins = [-PLAYER_HALF_WIDTH, -PLAYER_HALF_WIDTH, 0.0];
        let maxs = [PLAYER_HALF_WIDTH, PLAYER_HALF_WIDTH, CLEARANCE_MAXS_Z];
        let tracemask = pm.tracemask;
        let ledge = results.ledge_pos;
        let ahead = add(ledge, results.dir, OVER_FORWARD);
        let t = pm.trace_plain(ledge, ahead, mins, maxs, tracemask);
        if !t.startsolid && t.fraction >= 1.0 {
            let down = [ahead[0], ahead[1], ahead[2] - LEDGE_FLOOR];
            let t = pm.trace_plain(ahead, down, mins, maxs, tracemask);
            if t.fraction >= 1.0 {
                results.end_pos = [
                    ahead[0],
                    ahead[1],
                    (down[2] - ahead[2]) * t.fraction + ahead[2],
                ];
                return;
            }
        }
        results.flags &= !(flag::OVER | flag::ON);
    }
    results.end_pos = results.ledge_pos;
}

/// Sets the mantle up: its yaw, transition and flags, and the player where
/// the animations will carry them to the end.
fn start<W: MoveWorld>(pm: &mut Pm<'_, W>, results: &Results) {
    let ps = &mut *pm.ps;
    ps.mantle_yaw = vectoyaw(results.dir);
    ps.mantle_timer = 0;
    ps.mantle_trans_index = transition(results.start_pos[2], results.ledge_pos[2]);
    ps.mantle_flags = results.flags;
    let Some(total) = duration(pm) else {
        pm.gap(Gap::Mantle);
        return;
    };
    let Some(delta) = delta_at(pm, total) else {
        pm.gap(Gap::Mantle);
        return;
    };
    let ps = &mut *pm.ps;
    ps.mantle_length = total;
    let end = results.end_pos;
    ps.origin = [end[0] - delta[0], end[1] - delta[1], end[2] - delta[2]];
    ps.pm_flags |= pm_flags::MANTLE;
    pm.event(event::MANTLE, 0);
}

/// Each frame: looks for a ledge ahead and starts a mantle when jump is
/// held at one.
pub(crate) fn check<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    pm.ps.mantle_flags &= !flag::DONE;
    if !tuning::MANTLE_ENABLE {
        return;
    }
    pm.ps.mantle_flags &= !(0x80 | flag::AVAILABLE);
    let ps = &*pm.ps;
    if ps.pm_type >= pm_type::DEAD
        || ps.pm_flags & pm_flags::MANTLE != 0
        || ps.e_flags & (e_flags::CROUCHING | e_flags::PRONE) != 0
    {
        return;
    }
    let Some((dir, surface)) = front_probe(pm, pml) else {
        return;
    };
    let mut flags = if surface & SURF_MANTLE_OVER != 0 {
        flag::OVER
    } else if surface & SURF_MANTLE_ON != 0 {
        flag::ON
    } else {
        0
    };
    if surface & SURF_MOUNT != 0 {
        flags |= flag::MOUNT_SURFACE;
    }
    let mut results = Results {
        dir,
        start_pos: pm.ps.origin,
        flags,
        ..Results::default()
    };
    for height in LEDGE_HEIGHTS {
        if ledge_at(pm, &mut results, height) {
            break;
        }
    }
}

/// One step of a running mantle along the animations' root translation.
pub(crate) fn mantle_move<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    if !tuning::MANTLE_ENABLE {
        return;
    }
    pm.ps.mantle_flags &= !flag::AVAILABLE;
    if pm.ps.mantle_flags & flag::CROUCH != 0 {
        pm.event(event::STANCE_FORCE_CROUCH, 0);
    }
    let Some(total) = duration(pm) else {
        pm.gap(Gap::Mantle);
        return;
    };
    let old = pm.ps.mantle_timer;
    pm.ps.mantle_timer = (old + pml.msec).min(total);
    let now = pm.ps.mantle_timer;
    let (Some(from), Some(to), Some(anim)) =
        (delta_at(pm, old), delta_at(pm, now), anim_at(pm, now))
    else {
        pm.gap(Gap::Mantle);
        return;
    };
    pm.out(MoveEvent::LegsMove(anim as i32 + 8, true));
    let ps = &mut *pm.ps;
    let step = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    ps.origin = [
        ps.origin[0] + step[0],
        ps.origin[1] + step[1],
        ps.origin[2] + step[2],
    ];
    let n = 1.0 / ((now - old) as f32 * 0.001);
    ps.velocity = [n * step[0], step[1] * n, step[2] * n];
    if now == total {
        ps.pm_flags &= !pm_flags::MANTLE;
        if ps.mantle_flags & flag::OVER != 0 {
            pm.anim_event(anim::JUMP, false, true);
        }
        if pm.ps.mantle_flags & flag::STAND_AT_END != 0 {
            pm.event(event::STANCE_FORCE_STAND, 0);
        }
        pm.ps.mantle_flags |= flag::DONE;
    }
}

/// While mantling the view turns at most `mantle_view_yawcap` from the
/// mantle's yaw.
pub(crate) fn cap_view_yaw<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    if !tuning::MANTLE_ENABLE {
        return;
    }
    let ps = &mut *pm.ps;
    let mut d = angle_delta(ps.mantle_yaw, ps.viewangles[1]);
    let cap = tuning::MANTLE_VIEW_YAWCAP;
    if d >= -cap && cap >= d {
        return;
    }
    while -cap > d {
        d += cap;
    }
    while d > cap {
        d -= cap;
    }
    let edge = if d > 0.0 { -cap } else { cap };
    ps.delta_angles[1] += d;
    ps.viewangles[1] = wrap_360(edge + ps.mantle_yaw);
}

/// Black Ops' client plays the weapon's mantle animations only for a
/// mantle higher than `mantle_weapon_anim_height`.
pub fn tall_enough_for_weapon_anim(ps: &PlayerState) -> bool {
    transition_of(ps).2 > tuning::MANTLE_WEAPON_ANIM_HEIGHT
}
