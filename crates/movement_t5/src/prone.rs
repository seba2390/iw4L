use crate::math::{angle_delta, angle_normalize_180, angle_vectors, lerp, normalize, vectopitch};
use crate::state::{pm_flags, pm_type};
use crate::{MoveWorld, PlayerState, Pm, Trace};

const PRONE_CONTENTS: u32 = 0x0081_0011;
const BODY_CONTENTS: u32 = 0x0001_0000;
const CAPSULE: f32 = 6.0;
/// A downed player lies the other way round: distances along the body flip.
const DOWNED_SCALE: f32 = -0.75;

pub(crate) struct ProneCheck {
    pub origin: [f32; 3],
    pub radius: f32,
    pub height: f32,
    pub yaw: f32,
    pub already_prone: bool,
    pub on_ground: bool,
    pub ground_walkable: bool,
    pub body_contents: bool,
    pub feet_dist: f32,
}

/// Whether a player fits lying down at `check.origin` facing `check.yaw`;
/// writes the body's torso and waist pitches.
pub(crate) fn check_prone<W: MoveWorld>(
    world: &W,
    ps: Option<&PlayerState>,
    pass_entity: i32,
    check: &ProneCheck,
    pitches: Option<(&mut f32, &mut f32)>,
) -> bool {
    let zero_pitches =
        ps.is_some_and(|ps| ps.pm_flags & pm_flags::DIVING != 0 && !crate::dive::in_flight(ps));
    let downed = ps.is_some_and(|ps| ps.pm_type == pm_type::LAST_STAND);
    let mut waist_along = 18.0_f32;
    let mut feet_dist = check.feet_dist;
    if downed {
        feet_dist *= DOWNED_SCALE;
        waist_along = DOWNED_SCALE * 18.0;
    }
    let mask = PRONE_CONTENTS
        | if check.body_contents {
            BODY_CONTENTS
        } else {
            0
        };
    let trace = |start: [f32; 3], end: [f32; 3], mins: [f32; 3], maxs: [f32; 3]| -> Trace {
        world.trace(start, end, mins, maxs, pass_entity, mask)
    };
    let origin = check.origin;
    let r = check.radius;
    let fail = |pitches: Option<(&mut f32, &mut f32)>| -> bool {
        if check.on_ground {
            return false;
        }
        if let Some((a, b)) = pitches {
            *a = 0.0;
            *b = 0.0;
        }
        true
    };

    if !check.already_prone {
        let lifted = [origin[0], origin[1], origin[2] + 10.0];
        if trace(origin, lifted, [-r, -r, 0.0], [r, r, check.height]).allsolid {
            return false;
        }
    }
    if check.on_ground && !check.ground_walkable {
        return false;
    }
    let (mut forward, _, _) = angle_vectors([0.0, check.yaw - 180.0, 0.0]);
    let cap_mins = [-CAPSULE; 3];
    let cap_maxs = [CAPSULE; 3];
    let lift = check.height - CAPSULE;
    let mut start = [origin[0], origin[1], origin[2] + lift];
    let along = feet_dist - CAPSULE;
    let mut end = [
        along * forward[0] + start[0],
        along * forward[1] + start[1],
        forward[2] * along + start[2],
    ];
    let mut tr = trace(start, end, cap_mins, cap_maxs);
    let mut first_hit = false;
    let first_dist;
    if tr.fraction >= 1.0 {
        first_dist = feet_dist;
    } else {
        if !check.on_ground {
            return false;
        }
        let dist = along * tr.fraction + CAPSULE;
        first_hit = true;
        if r + 2.0 > dist {
            return false;
        }
        let short = lift * 0.699999988 + waist_along;
        if short > dist {
            end[2] += 22.0;
            first_hit = false;
            let mut delta = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
            let pitch_len = normalize(&mut delta);
            forward = delta;
            tr = trace(start, end, cap_mins, cap_maxs);
            if tr.fraction >= 1.0 {
                first_dist = feet_dist;
            } else {
                let retry = pitch_len * tr.fraction + CAPSULE;
                first_hit = true;
                if short > retry {
                    return false;
                }
                first_dist = retry;
            }
        } else {
            first_dist = dist;
        }
    }
    let mut feet = lerp(start, end, tr.fraction);

    start = [
        forward[0] * waist_along + origin[0],
        forward[1] * waist_along + origin[1],
        forward[2] * waist_along + origin[2] + lift,
    ];
    let down = r * 2.5 + lift - CAPSULE;
    end = [start[0], start[1], start[2] - down];
    tr = trace(start, end, cap_mins, cap_maxs);
    if tr.fraction == 1.0 {
        return fail(pitches);
    }
    if !tr.walkable {
        return false;
    }
    let waist_dist = down * tr.fraction + CAPSULE;
    let mut waist = lerp(start, end, tr.fraction);
    waist[2] -= CAPSULE;

    if first_hit {
        if waist_dist * -0.75 > first_dist - waist_dist {
            return fail(pitches);
        }
        let mut delta = [
            forward[0] * CAPSULE + (feet[0] - waist[0]),
            forward[1] * CAPSULE + (feet[1] - waist[1]),
            forward[2] * CAPSULE + (feet[2] - waist[2]) + CAPSULE,
        ];
        normalize(&mut delta);
        let mad = along - waist_along;
        end = [
            (along * forward[0] + origin[0] + (mad * delta[0] + start[0])) * 0.5,
            (along * forward[1] + origin[1] + (mad * delta[1] + start[1])) * 0.5,
            mad * delta[2] + start[2],
        ];
        tr = trace(start, end, cap_mins, cap_maxs);
        if 1.0 > tr.fraction {
            start = lerp(start, end, tr.fraction);
            start[2] += 18.0;
            end[2] += 18.0;
            tr = trace(start, end, cap_mins, cap_maxs);
            if 1.0 > tr.fraction {
                return fail(pitches);
            }
        }
        feet = lerp(start, end, tr.fraction);
    }

    start = feet;
    end = [feet[0], feet[1], feet[2] - ((feet[2] - waist[2]) * 2.0 + r)];
    tr = trace(start, end, cap_mins, cap_maxs);
    if tr.fraction == 1.0 {
        return fail(pitches);
    }
    if !tr.walkable {
        return false;
    }
    feet = lerp(start, end, tr.fraction);
    feet[2] -= CAPSULE;

    let mut fits = true;
    let mut torso = angle_normalize_180(vectopitch([
        origin[0] - waist[0],
        origin[1] - waist[1],
        origin[2] - waist[2],
    ]));
    let mut waist_pitch = angle_normalize_180(vectopitch([
        waist[0] - feet[0],
        waist[1] - feet[1],
        waist[2] - feet[2],
    ]));
    let bend = angle_delta(torso, waist_pitch);
    if -50.0 > bend || bend > 70.0 {
        fits = false;
    }
    let point = [0.0_f32; 3];
    let neck = [origin[0], origin[1], origin[2] + 5.0];
    let hip = [waist[0], waist[1], waist[2] + 5.0];
    if 1.0 > trace(neck, hip, point, point).fraction {
        fits = false;
    }
    if downed {
        torso = -torso;
        waist_pitch = -waist_pitch;
    }
    let mut pitches = pitches;
    if let Some((a, b)) = pitches.as_mut() {
        **a = if zero_pitches { 0.0 } else { torso };
        **b = if zero_pitches { 0.0 } else { waist_pitch };
    }
    if fits {
        return true;
    }
    fail(pitches)
}

/// A prone player whose new position does not fit lying down goes back to
/// where the move started, when that one does. Returns whether the move stands.
pub(crate) fn keep_prone_after_move<W: MoveWorld>(
    pm: &mut Pm<'_, W>,
    start_o: [f32; 3],
    start_v: [f32; 3],
) -> bool {
    if pm.ps.pm_flags & pm_flags::PRONE == 0 {
        return true;
    }
    if prone_fits_here(pm) {
        return true;
    }
    let moved_o = pm.ps.origin;
    let moved_v = pm.ps.velocity;
    pm.ps.origin = start_o;
    pm.ps.velocity = start_v;
    if prone_fits_here(pm) {
        return false;
    }
    pm.ps.origin = moved_o;
    pm.ps.velocity = moved_v;
    true
}

fn prone_fits_here<W: MoveWorld>(pm: &mut Pm<'_, W>) -> bool {
    let check = ProneCheck {
        origin: pm.ps.origin,
        radius: 15.0,
        height: 30.0,
        yaw: pm.ps.prone_direction,
        already_prone: true,
        on_ground: true,
        ground_walkable: true,
        body_contents: false,
        feet_dist: 50.0,
    };
    let snapshot = *pm.ps;
    let mut torso = pm.ps.prone_check_torso_pitch;
    let mut waist = pm.ps.prone_check_waist_pitch;
    let fits = check_prone(
        pm.world,
        Some(&snapshot),
        pm.client_num(),
        &check,
        Some((&mut torso, &mut waist)),
    );
    pm.ps.prone_check_torso_pitch = torso;
    pm.ps.prone_check_waist_pitch = waist;
    fits
}

fn step_pitch(current: f32, d: f32, frametime: f32) -> f32 {
    let step = frametime * 70.0;
    let next = if libm::fabsf(d) > step {
        if d >= 0.0 {
            step * 1.0 + current
        } else {
            -step + current
        }
    } else {
        current + d
    };
    let turns = next * 0.00277777785;
    (turns - libm::floorf(turns + 0.5)) * 360.0
}

/// A prone player who no longer fits lying down is pushed up to crouch; the
/// body's pitch follows the ground under it.
pub(crate) fn upkeep<W: MoveWorld>(pm: &mut Pm<'_, W>, pml: &crate::Pml) {
    use crate::events::event;
    use crate::state::{ENTITYNUM_NONE, view_height};
    if pm.ps.pm_flags & pm_flags::PRONE == 0 {
        return;
    }
    if pm.ps.ground_entity_num == ENTITYNUM_NONE {
        if pm.ps.view_height_target == view_height::DOWNED {
            pm.event(event::STANCE_FORCE_CROUCH, 0);
        } else {
            let check = ProneCheck {
                origin: pm.ps.origin,
                radius: 15.0,
                height: 30.0,
                yaw: pm.ps.prone_direction,
                already_prone: true,
                on_ground: false,
                ground_walkable: !pml.ground_plane || pml.ground_trace.walkable,
                body_contents: false,
                feet_dist: 50.0,
            };
            let snapshot = *pm.ps;
            let mut torso = pm.ps.prone_check_torso_pitch;
            let mut waist = pm.ps.prone_check_waist_pitch;
            let fits = check_prone(
                pm.world,
                Some(&snapshot),
                pm.client_num(),
                &check,
                Some((&mut torso, &mut waist)),
            );
            pm.ps.prone_check_torso_pitch = torso;
            pm.ps.prone_check_waist_pitch = waist;
            if !fits || pm.ps.water_level >= 2 {
                pm.event(event::STANCE_FORCE_CROUCH, 0);
                pm.ps.pm_flags |= pm_flags::STANCE_CHANGED;
            }
        }
    } else if (pml.ground_plane && !pml.ground_trace.walkable)
        || (pm.ps.view_height_target == view_height::DOWNED
            && (libm::fabsf(pm.ps.prone_check_torso_pitch) > 50.0
                || libm::fabsf(pm.ps.prone_check_waist_pitch) > 50.0))
    {
        pm.event(event::STANCE_FORCE_CROUCH, 0);
    }
    let target = if !pml.ground_plane {
        0.0
    } else if pm.ps.pm_type == pm_type::LAST_STAND {
        crate::stance::prone_pitch_on_ground(pm.ps.viewangles[1], &pml.ground_trace)
    } else {
        crate::stance::prone_pitch_on_ground(pm.ps.prone_direction, &pml.ground_trace)
    };
    let d = angle_delta(target, pm.ps.prone_direction_pitch);
    if d != 0.0 {
        pm.ps.prone_direction_pitch = step_pitch(pm.ps.prone_direction_pitch, d, pml.frametime);
    }
    let target = if pml.ground_plane {
        crate::stance::prone_pitch_on_ground(pm.ps.viewangles[1], &pml.ground_trace)
    } else {
        0.0
    };
    let d = angle_delta(target, pm.ps.prone_torso_pitch);
    if d != 0.0 {
        pm.ps.prone_torso_pitch = step_pitch(pm.ps.prone_torso_pitch, d, pml.frametime);
    }
}
