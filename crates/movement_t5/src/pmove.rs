use crate::math::angle_vectors;
use crate::physics::{effective_stance, sqrt};
use crate::state::{ENTITYNUM_NONE, buttons, e_flags, e_flags2, pm_flags, pm_type, weapon_state};
use crate::{
    Gap, MoveWorld, Pm, Pml, footsteps, ground, moves, sprint, stance, tuning, view_angles,
};

/// What a whole command's movement left behind.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MoveOutput {
    pub walking: bool,
    pub ground_plane: bool,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub xyspeed: f32,
}

const COMMAND_SLICE_MS: i32 = 66;

fn set_bounds<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    pm.mins = [-15.0, -15.0, 0.0];
    pm.maxs = [15.0, 15.0, 70.0];
    if pm.ps.pm_flags & pm_flags::PRONE != 0 {
        pm.maxs[2] = 30.0;
    } else if pm.ps.pm_flags & pm_flags::DUCKED != 0 {
        pm.maxs[2] = 50.0;
    }
}

/// How deep the player stands in water, from the surface above or below.
fn set_water_level<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let o = pm.ps.origin;
    let level = match pm.world.water_surface(o, 200.0, -200.0) {
        Some(surface) if surface >= o[2] + 60.0 => 5,
        Some(surface) if surface >= o[2] + 54.0 => 4,
        Some(surface) if surface >= o[2] + 45.0 => 3,
        Some(surface) if surface >= o[2] + 24.0 => 2,
        Some(surface) if surface >= o[2] + 3.0 => 1,
        _ => 0,
    };
    pm.ps.water_level = level;
}

fn drop_timers<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let ps = &mut *pm.ps;
    if ps.pm_time != 0 {
        if pml.msec >= ps.pm_time {
            if ps.pm_flags & pm_flags::JUMPING != 0 {
                ground::clear_jumping(ps);
            }
            ps.pm_flags &=
                !(pm_flags::JUMPING | pm_flags::TIME_KNOCKBACK | pm_flags::TIME_HARDLANDING);
            ps.pm_time = 0;
        } else {
            ps.pm_time -= pml.msec;
        }
    }
    if ps.legs_timer > 0 {
        ps.legs_timer -= pml.msec;
        if ps.legs_timer < 0 {
            ps.legs_timer = 0;
        }
    }
    if ps.torso_timer > 0 {
        ps.torso_timer -= pml.msec;
        if ps.torso_timer < 0 {
            ps.torso_timer = 0;
        }
    }
}

/// A dead or downed player on the ground slows to a stop.
fn dead_move(ps: &mut crate::PlayerState, pml: &Pml) {
    if !pml.walking {
        return;
    }
    let v = ps.velocity;
    let mut speed = sqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
    if ps.pm_flags & pm_flags::SLIDING == 0 {
        speed -= 20.0;
    }
    if speed <= 0.0 {
        ps.velocity = [0.0; 3];
    }
    let v = ps.velocity;
    let n = 1.0 / crate::physics::divisor(sqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]));
    ps.velocity = [v[0] * n * speed, v[1] * n * speed, v[2] * n * speed];
}

/// Without a launch running, the upward speed is capped.
fn launch_end<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let ps = &mut *pm.ps;
    if ps.pm_flags & pm_flags::LAUNCHED != 0 {
        pm.gap(Gap::Launched);
        return;
    }
    ps.launch_time = 0;
    if tuning::BAYONET_LAUNCH_PROOF && 0.0 > tuning::BAYONET_LAUNCH_Z_CAP - ps.velocity[2] {
        ps.velocity[2] = tuning::BAYONET_LAUNCH_Z_CAP;
    }
}

/// Moving through foliage rustles, more often the faster the player goes.
fn foliage_sound<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let now = pm.cmd.server_time;
    if tuning::FOLIAGE_MIN_SPEED > pm.xyspeed {
        if tuning::FOLIAGE_RESET_INTERVAL + pm.ps.foliage_sound_time < now {
            pm.ps.foliage_sound_time = 0;
        }
        return;
    }
    let mut t = (pm.xyspeed - tuning::FOLIAGE_MIN_SPEED)
        / (tuning::FOLIAGE_MAX_SPEED - tuning::FOLIAGE_MIN_SPEED);
    if t > 1.0 {
        t = 1.0;
    }
    let slow = tuning::FOLIAGE_SLOW_INTERVAL;
    let interval = ((tuning::FOLIAGE_FAST_INTERVAL - slow) as f32 * t + slow as f32) as i32;
    if interval + pm.ps.foliage_sound_time >= now {
        return;
    }
    let mins = [pm.mins[0] * 0.75, pm.mins[1] * 0.75, pm.mins[2] * 0.75];
    let maxs = [
        pm.maxs[0] * 0.75,
        pm.maxs[1] * 0.75,
        pm.maxs[2] * 0.899999976,
    ];
    let o = pm.ps.origin;
    if pm
        .trace(o, o, mins, maxs, tuning::FOLIAGE_CONTENTS)
        .startsolid
    {
        pm.event(crate::events::event::FOLIAGE_SOUND, 0);
        pm.ps.foliage_sound_time = now;
    }
}

fn freeze_moves<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    pm.cmd.forwardmove = 0;
    pm.cmd.rightmove = 0;
}

/// States in which commands lose their moves and some buttons.
fn mask_command<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let ps = &*pm.ps;
    let locked = ps.pm_flags & pm_flags::FROZEN != 0
        || ps.weaponstate == weapon_state::DEPLOYING
        || ps.e_flags2 & e_flags2::CONTROLS_LOCKED != 0;
    if locked || ps.pm_flags & pm_flags::RESPAWNED != 0 {
        let keep: &[u32] = if locked {
            &[buttons::STANCE, buttons::PRONE, buttons::CROUCH]
        } else {
            &[
                buttons::STANCE,
                buttons::PRONE,
                buttons::CROUCH,
                buttons::ATTACK,
            ]
        };
        pm.cmd.buttons.keep_only(keep);
        freeze_moves(pm);
        pm.ps.velocity = [0.0; 3];
    } else if pm.weapon_blocks_stance()
        || (ps.e_flags2 & e_flags2::MOUNTED != 0 && ps.e_flags & e_flags::VEHICLE_VIEW == 0)
    {
        freeze_moves(pm);
        pm.cmd
            .buttons
            .release_all(&[buttons::JUMP, buttons::LEAN_LEFT, buttons::LEAN_RIGHT]);
        pm.ps.velocity = [0.0; 3];
    }
    if pm.ps.move_disabled != 0 {
        pm.cmd.buttons.keep_only(&[
            buttons::SPRINT,
            buttons::ADS,
            buttons::STANCE,
            buttons::PRONE,
            buttons::CROUCH,
        ]);
        freeze_moves(pm);
    }
}

/// Whether the weapon state lets a prone player's move cancel aiming.
fn weapon_lets_prone_move(state: i32) -> bool {
    !matches!(state, 0x1b | 6 | 0x20 | 0x12..=0x1a | 0x22 | 0x23)
}

fn scoped_ads<W: MoveWorld>(pm: &Pm<'_, W>) -> bool {
    pm.world.weapon(pm.ps.weapon).def.ads_overlay_reticle() != 0 && pm.ps.weapon_pos_frac > 0.0
}

/// A prone player aiming may not move until they move harder than before.
fn prone_move_override<W: MoveWorld>(pm: &mut Pm<'_, W>) {
    let ps = &*pm.ps;
    if ps.pm_flags & pm_flags::PRONE == 0 || scoped_ads(pm) || ps.e_flags & e_flags::TURRET != 0 {
        pm.ps.pm_flags &= !pm_flags::PRONEMOVE_OVERRIDDEN;
        return;
    }
    let grew = |now: i8, before: i8| {
        now != before && libm::fabsf(f32::from(now)) > libm::fabsf(f32::from(before))
    };
    if grew(pm.cmd.forwardmove, pm.oldcmd.forwardmove)
        || grew(pm.cmd.rightmove, pm.oldcmd.rightmove)
    {
        if ps.pm_flags & pm_flags::DIVING == 0 && weapon_lets_prone_move(ps.weaponstate) {
            pm.ps.pm_flags &= !pm_flags::PRONEMOVE_OVERRIDDEN;
            pm.reset_ads();
        }
        return;
    }
    let idle = matches!(ps.weaponstate, 0..=5 | 0xb);
    let dual = pm.world.weapon(ps.weapon).def.dual_wield();
    let keep = ps.pm_flags & pm_flags::ADS_INTENT != 0
        || !idle
        || (dual && !matches!(ps.weaponstate_left, 0 | 0xb) && !matches!(ps.weaponstate, 1..=5));
    if !keep {
        pm.ps.pm_flags &= !pm_flags::PRONEMOVE_OVERRIDDEN;
    }
}

fn snap_velocity(v: &mut [f32; 3]) {
    for c in v.iter_mut() {
        *c = libm::rintf(*c);
    }
}

fn pmove_single<W: MoveWorld>(pm: &mut Pm<'_, W>) -> MoveOutput {
    set_bounds(pm);
    pm.ps.water_level = 0;
    if pm.ps.e_flags2 & e_flags2::MOUNTED != 0 {
        pm.ps.velocity = [0.0; 3];
    }
    if pm.ps.pm_flags & pm_flags::LAUNCHED != 0 {
        pm.cmd.forwardmove = 127;
        pm.cmd.rightmove = 0;
    }
    mask_command(pm);
    pm.ps.pm_flags &= !pm_flags::STANCE_CHANGED;
    if pm.ps.pm_type >= pm_type::DEAD && pm.ps.e_flags2 & e_flags2::MOUNTED == 0 {
        pm.tracemask &= !crate::PLAYER_BODY_CONTENTS;
    }
    prone_move_override(pm);
    let stance = effective_stance(pm.ps);
    if pm.ps.pm_flags & pm_flags::ADS_INTENT != 0 && stance == 1 && !scoped_ads(pm) {
        freeze_moves(pm);
    }
    if pm.cmd.buttons.0[0] & 0x4 != 0 {
        pm.ps.e_flags |= e_flags::TALKING;
    } else {
        pm.ps.e_flags &= !e_flags::TALKING;
    }
    if pm.ps.pm_type < pm_type::DEAD
        && !pm.cmd.buttons.held(buttons::ATTACK)
        && !pm.cmd.buttons.held(buttons::PRONE)
    {
        pm.ps.pm_flags &= !pm_flags::RESPAWNED;
    }

    let msec = (pm.cmd.server_time - pm.ps.command_time).clamp(1, 200);
    pm.ps.command_time = pm.cmd.server_time;
    let mut pml = Pml {
        msec,
        frametime: msec as f32 * 0.00100000005,
        previous_origin: pm.ps.origin,
        previous_velocity: pm.ps.velocity,
        ..Pml::default()
    };
    let start_origin = pm.ps.origin;
    view_angles::update(pm, msec as f32);
    set_water_level(pm);
    let (forward, right, up) = angle_vectors(pm.ps.viewangles);
    pml.forward = forward;
    pml.right = right;
    pml.up = up;

    if pm.cmd.forwardmove < 0 {
        pm.ps.pm_flags |= pm_flags::BACKWARDS_RUN;
    } else if pm.cmd.forwardmove > 0 || pm.cmd.rightmove != 0 {
        pm.ps.pm_flags &= !pm_flags::BACKWARDS_RUN;
    }
    if pm.ps.pm_type > pm_type::LAST_STAND && pm.ps.e_flags2 & e_flags2::MOUNTED == 0 {
        freeze_moves(pm);
    }
    if stance == 1 && pm.ps.pm_flags & pm_flags::PRONEMOVE_OVERRIDDEN != 0 {
        freeze_moves(pm);
    }
    if !stance::may_change_stance(pm) {
        freeze_moves(pm);
    }

    match pm.ps.pm_type {
        pm_type::NORMAL_LINKED
        | pm_type::DEAD_LINKED
        | pm_type::NOCLIP
        | pm_type::UFO
        | pm_type::SPECTATOR
        | pm_type::INTERMISSION => {
            pm.gap(Gap::SpecialMove);
            return output(pm, &pml);
        }
        pm_type::LAST_STAND | pm_type::LAST_STAND_REVIVED => {
            if pm.ps.pm_flags & pm_flags::LADDER != 0 {
                pm.ps.pm_flags = (pm.ps.pm_flags & !pm_flags::LADDER) | pm_flags::LADDER_FALL;
            }
            pm.ps.e_flags &= !e_flags::TURRET;
        }
        _ => {}
    }
    if pm.ps.e_flags & e_flags::TURRET != 0 || pm.ps.e_flags2 & e_flags2::MOUNTED != 0 {
        pm.gap(Gap::MountedView);
        return output(pm, &pml);
    }
    if pm.ps.pm_flags & pm_flags::MANTLE == 0 {
        pm.mins = [-15.0, -15.0, 0.0];
        pm.maxs = [15.0, 15.0, 70.0];
        sprint::update(pm);
        stance::check_duck(pm, &pml);
        ground::ground_trace(pm, &mut pml);
    }
    pm.gap(Gap::Mantle);
    if pm.ps.pm_flags & pm_flags::MANTLE != 0 {
        return output(pm, &pml);
    }
    if pm.ps.pm_flags & pm_flags::ANIM_LOCK != 0 {
        pm.gap(Gap::AnimLock);
    }
    crate::prone::upkeep(pm, &pml);
    drop_timers(pm, &pml);
    if pm.ps.pm_type > pm_type::INTERMISSION {
        dead_move(pm.ps, &pml);
    }
    crate::ladder::check(pm, &mut pml);
    launch_end(pm);
    crate::dive::check_end(pm);
    if pm.ps.pm_flags & pm_flags::LADDER != 0 {
        pm.gap(Gap::Ladder);
    } else if pm.ps.pm_flags & pm_flags::LAUNCHED != 0 {
        pm.gap(Gap::Launched);
    } else if pm.ps.water_level >= 3 {
        pm.gap(Gap::Swim);
    } else if pml.walking {
        moves::walk_move(pm, &mut pml);
    } else {
        moves::air_move(pm, &mut pml);
    }
    ground::ground_trace(pm, &mut pml);
    footsteps::footsteps(pm, &pml);
    foliage_sound(pm);

    let o = pm.ps.origin;
    let d = [
        o[0] - start_origin[0],
        o[1] - start_origin[1],
        o[2] - start_origin[2],
    ];
    let v = pm.ps.velocity;
    let ft = pml.frametime;
    if (d[2] * d[2] + d[1] * d[1] + d[0] * d[0]) / (ft * ft)
        < (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]) * 0.25
    {
        let n = 1.0 / ft;
        pm.ps.velocity = [n * d[0], d[1] * n, d[2] * n];
    }
    snap_velocity(&mut pm.ps.velocity);
    output(pm, &pml)
}

fn output<W: MoveWorld>(pm: &Pm<'_, W>, pml: &Pml) -> MoveOutput {
    MoveOutput {
        walking: pml.walking,
        ground_plane: pml.ground_plane,
        mins: pm.mins,
        maxs: pm.maxs,
        xyspeed: pm.xyspeed,
    }
}

/// Runs a command, in slices of at most 66 ms.
pub fn pmove<W: MoveWorld>(pm: &mut Pm<'_, W>) -> MoveOutput {
    let ps = &*pm.ps;
    if ps.pm_flags & (pm_flags::FROZEN | pm_flags::RESPAWNED) != 0
        || ps.weaponstate == weapon_state::DEPLOYING
        || ps.e_flags2 & e_flags2::CONTROLS_LOCKED != 0
    {
        freeze_moves(pm);
    }
    let finish = pm.cmd.server_time;
    let mut out = MoveOutput {
        mins: pm.mins,
        maxs: pm.maxs,
        ..MoveOutput::default()
    };
    if finish < pm.ps.command_time {
        return out;
    }
    if finish > pm.ps.command_time + 1000 {
        pm.ps.command_time = finish - 1000;
    }
    while pm.ps.command_time != finish {
        let msec = (finish - pm.ps.command_time).min(COMMAND_SLICE_MS);
        pm.cmd.server_time = pm.ps.command_time + msec;
        out = pmove_single(pm);
        pm.oldcmd = pm.cmd;
    }
    let _ = ENTITYNUM_NONE;
    out
}
