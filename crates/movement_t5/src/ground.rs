use crate::events::{anim, event};
use crate::math::lerp;
use crate::physics::sqrt;
use crate::state::{ENTITYNUM_NONE, ENTITYNUM_WORLD, pm_flags, pm_type};
use crate::{MoveWorld, Pm, Pml, SURF_NOFALLDAMAGE, SURF_NOSTEPS, SURF_SLICK, Trace, tuning};

const CORRECT_SOLID_OFFSETS: [[f32; 3]; 26] = [
    [0.0, 0.0, 1.0],
    [-1.0, 0.0, 1.0],
    [0.0, -1.0, 1.0],
    [1.0, 0.0, 1.0],
    [0.0, 1.0, 1.0],
    [-1.0, 0.0, 0.0],
    [0.0, -1.0, 0.0],
    [1.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, 0.0, -1.0],
    [-1.0, 0.0, -1.0],
    [0.0, -1.0, -1.0],
    [1.0, 0.0, -1.0],
    [0.0, 1.0, -1.0],
    [-1.0, -1.0, 1.0],
    [1.0, -1.0, 1.0],
    [1.0, 1.0, 1.0],
    [-1.0, 1.0, 1.0],
    [-1.0, -1.0, 0.0],
    [1.0, -1.0, 0.0],
    [1.0, 1.0, 0.0],
    [-1.0, 1.0, 0.0],
    [-1.0, -1.0, -1.0],
    [1.0, -1.0, -1.0],
    [1.0, 1.0, -1.0],
    [-1.0, 1.0, -1.0],
];

pub(crate) fn clear_jumping(ps: &mut crate::PlayerState) {
    ps.pm_flags &= !pm_flags::JUMPING;
    ps.jump_origin_z = 0.0;
}

/// Finds out what the player stands on.
pub(crate) fn ground_trace<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &mut Pml) {
    pm.ps.ground_surface_type = 0;
    let o = pm.ps.origin;
    let mut start = [o[0], o[1], o[2] + 0.25];
    let end = [o[0], o[1], o[2] - 0.25];
    let mut trace = pm.trace(start, end, pm.mins, pm.maxs, pm.tracemask);
    pml.ground_trace = trace;
    pm.ps.ground_surface_type = trace.surface_type();
    if pml.ground_trace.surface_flags & SURF_SLICK != 0 {
        pm.ps.pm_flags |= pm_flags::SLIDING;
        if pm.ps.slick_start_time == 0 {
            pm.ps.slick_start_time = pm.cmd.server_time;
            pm.event(event::SLIDE_START, 0);
            if pm.client_side {
                pm.event(event::SLIDE_LOOP_START, pm.surface_sound(pml));
            }
        }
    } else {
        pm.ps.pm_flags &= !pm_flags::SLIDING;
        if pm.ps.slick_start_time != 0 {
            pm.ps.slick_start_time = 0;
            pm.event(event::SLIDE_END, 0);
            if pm.client_side {
                pm.event(event::SLIDE_LOOP_END, pm.surface_sound(pml));
            }
        }
    }
    if trace.allsolid && !correct_all_solid(pm, pml, &mut trace) {
        return;
    }
    if trace.startsolid {
        start[2] = pm.ps.origin[2] - 0.001;
        trace = pm.trace(start, end, pm.mins, pm.maxs, pm.tracemask);
        if trace.startsolid {
            pm.ps.ground_entity_num = trace.entity;
            pml.ground_plane = true;
            pml.almost_ground_plane = true;
            pml.walking = trace.walkable;
            return;
        }
        pml.ground_trace = trace;
    }
    if trace.fraction == 1.0 {
        ground_trace_missed(pm, pml);
        return;
    }
    let ps = &*pm.ps;
    if ps.pm_flags & pm_flags::LADDER == 0
        && ps.velocity[2] > 0.0
        && ps.velocity[2] * trace.normal[2]
            + ps.velocity[1] * trace.normal[1]
            + trace.normal[0] * ps.velocity[0]
            > 10.0
        && (ps.ground_entity_num == ENTITYNUM_NONE || ps.ground_entity_num == ENTITYNUM_WORLD)
    {
        let deeper = [end[0], end[1], end[2] - 32.0];
        let below = pm.trace(start, deeper, pm.mins, pm.maxs, pm.tracemask);
        if below.fraction == 1.0 && pm.ps.pm_flags & pm_flags::DIVING == 0 {
            pm.anim_event(anim::JUMP, false, false);
        }
        pml.almost_ground_plane = false;
        pm.ps.ground_entity_num = ENTITYNUM_NONE;
        pml.ground_plane = false;
        pml.walking = false;
        return;
    }
    let walkable = trace.walkable && pm.world.can_stand_on(trace.entity);
    if walkable {
        pml.ground_plane = true;
        pml.almost_ground_plane = true;
        pml.walking = true;
        if pm.ps.ground_entity_num == ENTITYNUM_NONE {
            crash_land(pm, pml);
        }
        pm.ps.ground_entity_num = trace.entity;
        pm.touch(trace.entity);
        return;
    }
    pm.ps.ground_entity_num = ENTITYNUM_NONE;
    pml.ground_plane = true;
    pml.almost_ground_plane = true;
    pml.walking = false;
    clear_jumping(pm.ps);
}

/// Nudges a player stuck in solid to the first free spot one unit around.
fn correct_all_solid<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &mut Pml, trace: &mut Trace) -> bool {
    for offset in CORRECT_SOLID_OFFSETS {
        let o = pm.ps.origin;
        let point = [offset[0] + o[0], offset[1] + o[1], offset[2] + o[2]];
        *trace = pm.trace(point, point, pm.mins, pm.maxs, pm.tracemask);
        if !trace.startsolid {
            pm.ps.origin = point;
            let o = pm.ps.origin;
            let end = [point[0], point[1], o[2] - 1.0 - 0.25];
            *trace = pm.trace(o, end, pm.mins, pm.maxs, pm.tracemask);
            pml.ground_trace = *trace;
            pm.ps.origin = lerp(o, end, trace.fraction);
            return true;
        }
    }
    pm.ps.ground_entity_num = trace.entity;
    pml.ground_plane = false;
    pml.almost_ground_plane = false;
    pml.walking = false;
    clear_jumping(pm.ps);
    false
}

fn ground_trace_missed<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &mut Pml) {
    let o = pm.ps.origin;
    if pm.ps.ground_entity_num != ENTITYNUM_NONE {
        let below = pm.trace(o, [o[0], o[1], o[2] - 64.0], pm.mins, pm.maxs, pm.tracemask);
        if below.fraction == 1.0 {
            if pm.ps.pm_flags & pm_flags::DIVING == 0 {
                pm.anim_event(anim::JUMP, false, true);
            }
            pml.almost_ground_plane = false;
        } else {
            pml.almost_ground_plane = 0.015625 > below.fraction;
            if !pml.almost_ground_plane && pm.ps.pm_flags & pm_flags::DIVING == 0 {
                pm.anim_event(anim::JUMP, false, true);
            }
        }
    } else {
        let below = pm.trace(o, [o[0], o[1], o[2] - 1.0], pm.mins, pm.maxs, pm.tracemask);
        pml.almost_ground_plane = below.fraction != 1.0;
    }
    pm.ps.ground_entity_num = ENTITYNUM_NONE;
    pml.ground_plane = false;
    pml.walking = false;
}

/// The surface a landing sounds like.
fn landing_surface(pm_water: i32, trace: &Trace) -> i32 {
    if trace.surface_flags & SURF_NOSTEPS != 0 {
        0
    } else if pm_water != 0 {
        20
    } else {
        trace.surface_type()
    }
}

/// How hard a fall was, from how far the player fell this frame: the fall
/// height, and the damage in percent of health.
fn fall_damage_percent(ps: &crate::PlayerState, fall: f32) -> i32 {
    let min = tuning::FALL_DAMAGE_MIN_HEIGHT;
    let max = tuning::FALL_DAMAGE_MAX_HEIGHT;
    let mut damage = if min >= fall {
        0
    } else if fall >= max {
        100
    } else {
        let below = fall - min;
        let t = (f64::from(below) / (f64::from(max) - f64::from(min))) * 100.0;
        (t as i32).clamp(0, 100)
    };
    if ps.pm_flags & pm_flags::DIVING != 0 {
        damage = crate::dive::fall_damage_percent(fall);
    }
    damage
}

/// The landing at the end of a fall: hard landing slowdown, fall damage and
/// landing sounds.
pub(crate) fn crash_land<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &Pml) {
    let ps = &*pm.ps;
    if ps.water_level == 3 {
        return;
    }
    let g = ps.gravity as f32;
    let vz = pml.previous_velocity[2];
    let dist = pml.previous_origin[2] - ps.origin[2];
    let acc = -g * 0.5;
    let den = vz * vz - dist * acc * 4.0;
    if 0.0 > den {
        return;
    }
    let t = (-vz - sqrt(den)) / (acc * 2.0);
    let vel = -(t * -g + vz);
    let fall = vel * vel / (g * 2.0);
    let mut damage = fall_damage_percent(ps, fall);
    if pm.zombiemode && ps.gravity < tuning::GRAVITY as i32 {
        damage = 0;
    }
    if pm.zombiemode && ps.perks & crate::perks::FALL_DAMAGE != 0 && damage > 0 {
        damage = 1;
    }
    if pml.ground_trace.surface_flags & SURF_NOFALLDAMAGE != 0 || ps.pm_type >= pm_type::DEAD {
        damage = 0;
    }
    if ps.water_level == 2 {
        damage = (damage as f32 * 0.5) as i32;
    }
    let mut landing = 0;
    if fall > 12.0 {
        landing = ((((fall - 12.0) * 0.0384615399 + 1.0) * 4.0) as i32).min(24);
        if pm.ps.pm_flags & pm_flags::DIVING == 0 {
            pm.anim_event(anim::LAND, false, true);
        }
    }
    let surface = landing_surface(pm.ps.water_level, &pml.ground_trace);
    if damage != 0 {
        let scale;
        if damage < 100 && pml.ground_trace.surface_flags & SURF_SLICK == 0 {
            let mut time = damage * 35 + 500;
            if time > 2000 {
                scale = 0.200000003;
                time = 2000;
            } else if time > 500 {
                scale = if time >= 1500 {
                    0.200000003
                } else {
                    0.5 - (time as f32 - 500.0) * 0.00100000005 * 0.300000012
                };
            } else {
                scale = 0.5;
            }
            pm.ps.pm_flags |= pm_flags::TIME_HARDLANDING;
            pm.ps.pm_time = time;
        } else {
            scale = 0.670000017;
        }
        pm.ps.velocity[0] *= scale;
        pm.ps.velocity[1] *= scale;
        pm.ps.velocity[2] *= scale;
        let fall_event = if pml.ground_trace.surface_flags & SURF_NOSTEPS != 0 {
            event::FALL_DAMAGE
        } else {
            event::FALL_DAMAGE + surface
        };
        pm.event(fall_event, damage);
        return;
    }
    if fall <= 4.0 {
        return;
    }
    if 8.0 > fall {
        pm.event(event::LAND_SOFT, surface);
        return;
    }
    if 12.0 > fall {
        pm.event(event::LAND_MEDIUM, surface);
        return;
    }
    pm.ps.velocity[0] *= 0.670000017;
    pm.ps.velocity[1] *= 0.670000017;
    pm.ps.velocity[2] *= 0.670000017;
    pm.event(event::LAND + surface, landing);
}
