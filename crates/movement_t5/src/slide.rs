use crate::physics::{divisor, sqrt};
use crate::{MoveWorld, Pm, Pml};

const MAX_CLIP_PLANES: usize = 8;

/// Removes the part of `v` going into the plane `n`, slightly over.
pub(crate) fn clip_velocity(v: [f32; 3], n: [f32; 3]) -> [f32; 3] {
    let mut backoff = v[1] * n[1] + n[0] * v[0] + n[2] * v[2];
    backoff = -(backoff - libm::fabsf(backoff) * 0.001);
    [
        n[0] * backoff + v[0],
        backoff * n[1] + v[1],
        n[2] * backoff + v[2],
    ]
}

/// The planes ordered from the one the velocity goes into most; returns that
/// plane's dot with the velocity.
fn rank_planes(v: [f32; 3], planes: &[[f32; 3]], order: &mut [usize; MAX_CLIP_PLANES + 2]) -> f32 {
    let mut dots = [0.0_f32; MAX_CLIP_PLANES + 2];
    for (i, p) in planes.iter().enumerate() {
        let d = p[0] * v[0] + p[2] * v[2] + p[1] * v[1];
        dots[i] = d;
        let mut j = i;
        while j > 0 {
            let prev = order[j - 1];
            if d > dots[prev] {
                break;
            }
            order[j] = prev;
            j -= 1;
        }
        order[j] = i;
    }
    dots[order[0]]
}

fn dot_yzx(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[1] * b[1] + a[2] * b[2] + a[0] * b[0]
}

/// Moves the player along its velocity for the frame, sliding along what it
/// hits. Returns whether anything was hit.
pub(crate) fn slide_move<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &mut Pml, gravity: bool) -> bool {
    let mut primal = pm.ps.velocity;
    let mut end_velocity = pm.ps.velocity;
    if gravity {
        end_velocity[2] = pm.ps.velocity[2] - pm.ps.gravity as f32 * pml.frametime;
        pm.ps.velocity[2] = (end_velocity[2] + pm.ps.velocity[2]) * 0.5;
        primal[2] = end_velocity[2];
        if pml.ground_plane {
            pm.ps.velocity = clip_velocity(pm.ps.velocity, pml.ground_trace.normal);
        }
    }
    let mut time_left = pml.frametime;
    let mut planes = [[0.0_f32; 3]; MAX_CLIP_PLANES + 2];
    let mut numplanes = 0;
    if pml.ground_plane {
        planes[0] = pml.ground_trace.normal;
        numplanes = 1;
    }
    let v = pm.ps.velocity;
    let n = 1.0 / divisor(sqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]));
    planes[numplanes] = [v[0] * n, v[1] * n, v[2] * n];
    numplanes += 1;

    let mut bumpcount = 0;
    while bumpcount < 4 {
        let o = pm.ps.origin;
        let v = pm.ps.velocity;
        let end = [
            time_left * v[0] + o[0],
            v[1] * time_left + o[1],
            v[2] * time_left + o[2],
        ];
        let trace = pm.trace(o, end, pm.mins, pm.maxs, pm.tracemask);
        if trace.allsolid {
            pm.ps.velocity[2] = 0.0;
            return true;
        }
        if trace.fraction > 0.0 {
            let f = trace.fraction;
            pm.ps.origin = [
                (end[0] - o[0]) * f + o[0],
                (end[1] - o[1]) * f + o[1],
                (end[2] - o[2]) * f + o[2],
            ];
        }
        if trace.fraction == 1.0 {
            break;
        }
        pm.touch(trace.entity);
        time_left -= trace.fraction * time_left;
        if numplanes >= MAX_CLIP_PLANES {
            pm.ps.velocity = [0.0; 3];
            return true;
        }
        let normal = trace.normal;
        if planes[..numplanes]
            .iter()
            .any(|p| p[2] * normal[2] + p[0] * normal[0] + p[1] * normal[1] > 0.999)
        {
            let c = clip_velocity(pm.ps.velocity, normal);
            pm.ps.velocity = [c[0] + normal[0], c[1] + normal[1], c[2] + normal[2]];
            bumpcount += 1;
            continue;
        }
        planes[numplanes] = normal;
        numplanes += 1;
        let mut order = [0usize; MAX_CLIP_PLANES + 2];
        let most = rank_planes(pm.ps.velocity, &planes[..numplanes], &mut order);
        if most >= 0.1 {
            bumpcount += 1;
            continue;
        }
        if -most > pml.impact_speed {
            pml.impact_speed = -most;
        }
        let first = planes[order[0]];
        let mut clip = clip_velocity(pm.ps.velocity, first);
        let mut end_clip = clip_velocity(end_velocity, first);
        for i in 1..numplanes {
            let q = planes[order[i]];
            if dot_yzx(q, clip) >= 0.1 {
                continue;
            }
            clip = clip_velocity(clip, q);
            end_clip = clip_velocity(end_clip, q);
            if dot_yzx(first, clip) >= 0.0 {
                continue;
            }
            let p = first;
            let mut dir = [
                q[2] * p[1] - q[1] * p[2],
                q[0] * p[2] - p[0] * q[2],
                p[0] * q[1] - q[0] * p[1],
            ];
            let len = sqrt(dir[2] * dir[2] + dir[1] * dir[1] + dir[0] * dir[0]);
            let n = 1.0 / divisor(len);
            dir = [n * dir[0], dir[1] * n, dir[2] * n];
            let v = pm.ps.velocity;
            let d = v[2] * dir[2] + v[1] * dir[1] + v[0] * dir[0];
            clip = [dir[0] * d, dir[1] * d, dir[2] * d];
            let e = dir[1] * end_velocity[1] + dir[0] * end_velocity[0] + dir[2] * end_velocity[2];
            end_clip = [dir[0] * e, dir[1] * e, dir[2] * e];
            for (k, &other) in order.iter().enumerate().take(numplanes).skip(1) {
                if k == i {
                    continue;
                }
                if dot_yzx(planes[other], clip) < 0.1 {
                    pm.ps.velocity = [0.0; 3];
                    return true;
                }
            }
        }
        pm.ps.velocity = clip;
        end_velocity = end_clip;
        bumpcount += 1;
    }
    if gravity {
        pm.ps.velocity = end_velocity;
    }
    if pm.ps.pm_time != 0 {
        pm.ps.velocity = primal;
    }
    bumpcount != 0
}

fn clear_jumping(ps: &mut crate::PlayerState) {
    ps.pm_flags &= !crate::state::pm_flags::JUMPING;
    ps.jump_origin_z = 0.0;
}

/// While jumping, the player may step up onto a ledge as long as it stays
/// under the jump's apex; `step_size` shrinks to what is left.
fn jump_step_size(ps: &crate::PlayerState, start: [f32; 3], step_size: &mut f32) -> bool {
    let apex = ps.jump_origin_z + crate::tuning::JUMP_HEIGHT;
    if apex <= start[2] {
        return false;
    }
    *step_size = crate::tuning::JUMP_STEP_SIZE;
    if *step_size + start[2] > ps.jump_origin_z + crate::tuning::JUMP_HEIGHT {
        *step_size = ps.jump_origin_z + crate::tuning::JUMP_HEIGHT - start[2];
    }
    true
}

/// After a step up while jumping, the upward speed is capped to what reaches
/// the jump's apex.
fn cap_jump_step_velocity(ps: &mut crate::PlayerState, down: [f32; 3]) {
    if ps.origin[2] - down[2] <= 0.0 {
        return;
    }
    let left = ps.jump_origin_z + crate::tuning::JUMP_HEIGHT - ps.origin[2];
    if 0.1 > left {
        ps.velocity[2] = 0.0;
        return;
    }
    let speed = sqrt(ps.gravity as f32 * (left * 2.0));
    if ps.velocity[2] > speed {
        ps.velocity[2] = speed;
    }
}

fn lerp_to(o: [f32; 3], end: [f32; 3], f: f32) -> [f32; 3] {
    [
        (end[0] - o[0]) * f + o[0],
        (end[1] - o[1]) * f + o[1],
        (end[2] - o[2]) * f + o[2],
    ]
}

/// Slide move that also climbs steps and keeps a grounded player on the floor
/// going down slopes and stairs.
pub(crate) fn step_slide_move<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &mut Pml, gravity: bool) {
    use crate::state::{ENTITYNUM_NONE, pm_flags, pm_type};
    let mut stepped = 0.0_f32;
    let mut jump_step = false;
    let on_ground;
    if pm.ps.pm_flags & pm_flags::LADDER != 0 {
        on_ground = false;
        clear_jumping(pm.ps);
    } else if pml.ground_plane {
        on_ground = true;
    } else {
        on_ground = false;
        if pm.ps.pm_flags & pm_flags::JUMPING != 0 && pm.ps.pm_time != 0 {
            clear_jumping(pm.ps);
        }
    }
    let start_o = pm.ps.origin;
    let start_v = pm.ps.velocity;
    let moved = slide_move(pm, pml, gravity);
    if !moved && pm.ps.pm_type == pm_type::LAST_STAND && pm.ps.pm_flags & pm_flags::SLIDING != 0 {
        return;
    }
    let mut step_size = if pm.ps.pm_flags & pm_flags::PRONE != 0 {
        10.0
    } else {
        18.0
    };
    if pm.ps.ground_entity_num == ENTITYNUM_NONE {
        if pm.ps.pm_flags & pm_flags::JUMPING != 0 && pm.ps.pm_time != 0 {
            clear_jumping(pm.ps);
        }
        if moved
            && pm.ps.pm_flags & pm_flags::JUMPING != 0
            && jump_step_size(pm.ps, start_o, &mut step_size)
        {
            if 1.0 > step_size {
                return;
            }
            jump_step = true;
        } else if pm.ps.pm_flags & pm_flags::LADDER == 0 || pm.ps.velocity[2] <= 0.0 {
            return;
        }
    }
    let down_o = pm.ps.origin;
    let down_v = pm.ps.velocity;
    let dx = down_o[0] - start_o[0];
    let dy = down_o[1] - start_o[1];
    if moved || (pml.ground_plane && 0.9 > pml.ground_trace.normal[2]) {
        let up = [start_o[0], start_o[1], start_o[2] + step_size + 1.0];
        let trace = pm.trace(start_o, up, pm.mins, pm.maxs, pm.tracemask);
        stepped = (step_size + 1.0) * trace.fraction - 1.0;
        if 1.0 > stepped {
            stepped = 0.0;
        } else {
            pm.ps.origin = [up[0], up[1], start_o[2] + stepped];
            pm.ps.velocity = start_v;
            slide_move(pm, pml, gravity);
        }
    }
    let mut restore = false;
    if on_ground || stepped != 0.0 {
        let o = pm.ps.origin;
        let mut down = [o[0], o[1], o[2] - stepped];
        if on_ground {
            down[2] -= 9.0;
        }
        let trace = pm.trace(o, down, pm.mins, pm.maxs, pm.tracemask);
        if trace.entity < crate::tuning::MAX_CLIENTS {
            pm.ps.origin = down_o;
            pm.ps.velocity = down_v;
            return;
        }
        if 1.0 > trace.fraction {
            if !trace.walkable && 0.3 > trace.normal[2] {
                pm.ps.origin = down_o;
                pm.ps.velocity = down_v;
                return;
            }
            pm.ps.origin = lerp_to(o, down, trace.fraction);
            crate::physics::project_onto_ground(&mut pm.ps.velocity, trace.normal);
        } else if stepped != 0.0 {
            pm.ps.origin[2] -= stepped;
        }
    }
    let o = pm.ps.origin;
    let step_progress = (o[1] - start_o[1]) * start_v[1] + (o[0] - start_o[0]) * start_v[0];
    let slide_progress = dy * start_v[1] + dx * start_v[0] + 0.001;
    if slide_progress >= step_progress || (jump_step && crate::slide::at_jump_apex(pm.ps)) {
        restore = true;
    }
    if restore {
        pm.ps.origin = down_o;
        pm.ps.velocity = down_v;
        if on_ground {
            let o = pm.ps.origin;
            let down = [o[0], o[1], o[2] - 9.0];
            let trace = pm.trace(o, down, pm.mins, pm.maxs, pm.tracemask);
            if 1.0 > trace.fraction {
                pm.ps.origin = lerp_to(o, down, trace.fraction);
                pm.ps.velocity = clip_velocity(pm.ps.velocity, trace.normal);
            }
        }
    }
    if jump_step {
        cap_jump_step_velocity(pm.ps, down_o);
    }
    if !(on_ground && pm.ps.pm_type < pm_type::DEAD) {
        return;
    }
    if !crate::prone::keep_prone_after_move(pm, start_o, start_v) {
        return;
    }
    let dz = pm.ps.origin[2] - down_o[2];
    if libm::fabsf(dz) <= 0.5 {
        return;
    }
    let steps = libm::rint(f64::from(dz) + 9.313225746154785e-10) as i32;
    if steps == 0 {
        return;
    }
    let command_time = pm.ps.command_time;
    if pm.step_smooth_time < command_time || pm.step_smooth_time > command_time + 1000 {
        pm.step_smooth_z += pm.ps.origin[2] - down_o[2];
        pm.step_smooth_time = command_time;
    }
    let f = (1.0 - libm::fabsf(pm.ps.origin[2] - start_o[2]) / step_size) * 0.8 + 0.199999988;
    pm.ps.velocity[0] *= f;
    pm.ps.velocity[1] *= f;
    pm.ps.velocity[2] *= f;
    let v = pm.ps.velocity;
    pm.xyspeed = sqrt(v[0] * v[0] + v[1] * v[1]);
    let steps = steps.abs();
    if steps > 3 && pm.ps.ground_entity_num != ENTITYNUM_NONE {
        let bump = (steps >> 1).min(4);
        let old = pm.ps.bob_cycle;
        let new = ((bump as f32 * 1.25 + 7.0 + old as f32) as i32) & 0xff;
        pm.ps.bob_cycle = new;
        crate::footsteps::bob_event(pm, pml, old, new, true);
    }
}

/// Whether a jump step reached the jump's apex.
pub(crate) fn at_jump_apex(ps: &crate::PlayerState) -> bool {
    ps.origin[2] >= ps.jump_origin_z + crate::tuning::JUMP_HEIGHT
}
