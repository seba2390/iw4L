//! Black Ops 2's client melee lunge pick (aim assist's automelee): on a
//! melee press, the nearest enemy player or actor in range, in the middle of
//! the view, in sight and being approached, is named in the command with its
//! distance and yaw for the server's lunge.

use fastfile_t6::weapon::WeaponDefView;
use game_api::movement::{AimCandidate, MeleeAim, MeleeTarget, buttons};

const DEG2RAD: f32 = 0.0174532924;
/// A weapon's own automelee range, when it is a melee weapon or bayonet.
const AUTOMELEE_RANGE: u32 = 0x74c;
const BAYONET_EXTRA: u32 = 0x750;
const THROW_MELEE: u32 = 0x64f;
const WEAPTYPE_MELEE: i32 = 7;
/// At most this many aim targets.
const MAX_TARGETS: usize = 64;
/// The automelee region, in a 640 × 480 view: 320 × 240 about the centre.
const REGION_HALF: f32 = 0.5;
/// Taken off the distance the command carries.
const REACH: f32 = 30.0;

fn dvar_f32(aim: &MeleeAim<'_>, name: &str, default: f32) -> f32 {
    (aim.dvar)(name)
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

fn dvar_bool(aim: &MeleeAim<'_>, name: &str, default: bool) -> bool {
    (aim.dvar)(name)
        .and_then(|v| v.trim().parse::<f32>().ok())
        .map_or(default, |v| v != 0.0)
}

fn pressed(aim: &MeleeAim<'_>, button: u32) -> bool {
    aim.cmd.buttons & button != 0 && aim.last.buttons & button == 0
}

/// One aim target: its entity, its screen box and its distance.
struct Target {
    entity: i32,
    origin: [f32; 3],
    min: [f32; 2],
    max: [f32; 2],
    dist_sq: f32,
    center: [f32; 3],
}

fn angle_vectors(angles: [f32; 3]) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let sc = |deg: f32| {
        let r = f64::from(deg * DEG2RAD);
        (libm::sin(r) as f32, libm::cos(r) as f32)
    };
    let (sy, cy) = sc(angles[1]);
    let (sp, cp) = sc(angles[0]);
    let (sr, cr) = sc(angles[2]);
    let forward = [cp * cy, cp * sy, -sp];
    let right = [
        -(sr * sp * cy) + cr * sy,
        -(sr * sp * sy) - cr * cy,
        -(sr * cp),
    ];
    let up = [cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp];
    (forward, right, up)
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn vectoyaw(x: f32, y: f32) -> f32 {
    if y == 0.0 && x == 0.0 {
        return 0.0;
    }
    let yaw = libm::atan2(f64::from(y), f64::from(x)) as f32 * 180.0 * 0.318309873;
    if yaw >= 0.0 { yaw } else { 360.0 + yaw }
}

/// A sentient's aim box about its origin: a cross of the target radius
/// along its facing and side, reaching back the radius and 20 more, and
/// (Black Ops 2's fixed actor size) twice the half height tall.
fn sentient_box(aim: &MeleeAim<'_>, angles: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let radius = dvar_f32(aim, "aim_target_sentient_radius", 10.0);
    let half_height = dvar_f32(aim, "aim_target_sentient_half_height", 32.0);
    let (forward, right, _) = angle_vectors(angles);
    let side = right.map(|c| c * radius);
    let back = forward.map(|c| c * (-(radius + 20.0)));
    let front = forward.map(|c| c * radius);
    let opposite = side.map(|c| -c);
    let mut mins = front;
    let mut maxs = front;
    for v in [back, side, opposite] {
        for axis in 0..3 {
            mins[axis] = mins[axis].min(v[axis]);
            maxs[axis] = maxs[axis].max(v[axis]);
        }
    }
    maxs[2] = half_height + half_height;
    (mins, maxs)
}

/// The box's corners, turned by `angles` and set at `origin`, on the screen
/// of a view through `eye` along `view`: the screen box they span within
/// the screen; `None` when it is off the screen.
fn screen_box(
    eye: [f32; 3],
    view: ([f32; 3], [f32; 3], [f32; 3]),
    tan: [f32; 2],
    origin: [f32; 3],
    angles: [f32; 3],
    (mins, maxs): ([f32; 3], [f32; 3]),
) -> Option<([f32; 2], [f32; 2])> {
    let (forward, right, up) = angle_vectors(angles);
    let (vf, vr, vu) = view;
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for corner in 0..8 {
        let local = [
            if corner & 1 == 0 { mins[0] } else { maxs[0] },
            if corner & 2 == 0 { mins[1] } else { maxs[1] },
            if corner & 4 == 0 { mins[2] } else { maxs[2] },
        ];
        let world: [f32; 3] = core::array::from_fn(|i| {
            local[0] * forward[i] - local[1] * right[i] + local[2] * up[i] + origin[i]
        });
        let d = [world[0] - eye[0], world[1] - eye[1], world[2] - eye[2]];
        let depth = dot(d, vf);
        if depth <= 0.0 {
            continue;
        }
        let inv = 1.0 / depth;
        let p = [dot(d, vr) / tan[0] * inv, -(dot(d, vu) / tan[1] * inv), inv];
        for axis in 0..3 {
            lo[axis] = lo[axis].min(p[axis]);
            hi[axis] = hi[axis].max(p[axis]);
        }
    }
    let on = lo[0] < hi[0]
        && lo[1] < hi[1]
        && lo[2] < hi[2]
        && lo[0] <= 1.0
        && lo[1] <= 1.0
        && lo[2] <= 1.0
        && -1.0 <= hi[0]
        && -1.0 <= hi[1]
        && 0.0 <= hi[2];
    on.then(|| {
        (
            [lo[0].clamp(-1.0, 1.0), lo[1].clamp(-1.0, 1.0)],
            [hi[0].clamp(-1.0, 1.0), hi[1].clamp(-1.0, 1.0)],
        )
    })
}

/// How far a lunge reaches for the weapon: its own range for a melee weapon
/// or bayonet that has one, else the bayonet's or the knife's.
fn range(aim: &MeleeAim<'_>, def: &WeaponDefView<'_>) -> f32 {
    let own = def.def_f32(AUTOMELEE_RANGE);
    let bayonet = def.bayonet() || def.def_bool(BAYONET_EXTRA);
    if own != 0.0 && (def.weapon_type() == WEAPTYPE_MELEE || bayonet) {
        own
    } else if def.bayonet() {
        dvar_f32(aim, "aim_autobayonet_range", 200.0)
    } else {
        dvar_f32(aim, "aim_automelee_range", 120.0)
    }
}

/// The player is closing on the target: near it already, or moving at a
/// share of their speed toward it.
fn approaching(aim: &MeleeAim<'_>, target: &Target) -> bool {
    let near = dvar_f32(aim, "aim_automelee_move_limit_range", 20.0);
    if target.dist_sq <= near * near {
        return true;
    }
    let v = aim.player.velocity;
    let speed = libm::sqrtf(v[0] * v[0] + v[1] * v[1]);
    let limit = dvar_f32(aim, "aim_automelee_move_limit", 0.1);
    if f64::from(limit) * f64::from(aim.player.speed) > f64::from(speed) {
        return false;
    }
    let o = aim.player.origin;
    let (dx, dy) = (target.origin[0] - o[0], target.origin[1] - o[1]);
    let len = libm::sqrtf(dy * dy + dx * dx);
    let inv = 1.0 / if -len >= 0.0 { 1.0 } else { len };
    let (tx, ty) = (dx * inv, dy * inv);
    let vinv = 1.0 / if -speed >= 0.0 { 1.0 } else { speed };
    let (mx, my) = (v[0] * vinv, v[1] * vinv);
    if speed <= 0.00100000005 {
        return true;
    }
    let along = (f64::from(my) * f64::from(ty) + f64::from(mx) * f64::from(tx)) as f32;
    let angle = dvar_f32(aim, "aim_automelee_move_limit_angle", 0.1);
    !(f64::from(angle) > f64::from(along))
}

pub(crate) fn melee_target(aim: &MeleeAim<'_>) -> Option<MeleeTarget> {
    if !dvar_bool(aim, "aim_automelee_enabled", true) {
        return None;
    }
    let weapon = aim.player.weapon;
    let held = (aim.weapon)(weapon)?;
    let def = WeaponDefView::new(&held.def, &held.variant)?;
    let press = pressed(aim, buttons::MELEE)
        || def.weapon_type() == WEAPTYPE_MELEE
            && (pressed(aim, buttons::ATTACK)
                || pressed(aim, buttons::THROW) && def.def_bool(THROW_MELEE));
    if !press {
        return None;
    }
    let reach = range(aim, &def);
    let farthest = dvar_f32(aim, "aim_assist_min_target_distance", 10000.0);
    let fov = dvar_f32(aim, "cg_fov", 65.0);
    let tan_y = (f64::from(fov * DEG2RAD * 0.5)).tan() as f32 * 0.75;
    let tan = [tan_y * aim.aspect.unwrap_or(4.0 / 3.0), tan_y];
    let view = angle_vectors(aim.player.viewangles);
    let mut targets: Vec<Target> = Vec::new();
    for &AimCandidate {
        entity,
        origin,
        angles,
    } in aim.candidates
    {
        let o = aim.player.origin;
        let d = [origin[0] - o[0], origin[1] - o[1], origin[2] - o[2]];
        let dist_sq = d[1] * d[1] + d[0] * d[0] + d[2] * d[2];
        if dist_sq > farthest * farthest {
            continue;
        }
        let bounds = sentient_box(aim, angles);
        let Some((min, max)) = screen_box(aim.eye, view, tan, origin, angles, bounds) else {
            continue;
        };
        let (mins, maxs) = bounds;
        let center: [f32; 3] = core::array::from_fn(|i| origin[i] + (mins[i] + maxs[i]) * 0.5);
        targets.push(Target {
            entity,
            origin,
            min,
            max,
            dist_sq,
            center,
        });
    }
    if dvar_bool(aim, "aim_target_closest_first", true) {
        targets.sort_by(|a, b| a.dist_sq.total_cmp(&b.dist_sq));
    } else {
        let off = |t: &Target| {
            let x = (t.min[0] + t.max[0]) * 0.5;
            let y = (t.min[1] + t.max[1]) * 0.5;
            x * x + y * y
        };
        targets.sort_by(|a, b| off(a).total_cmp(&off(b)));
    }
    targets.truncate(MAX_TARGETS);
    let target = targets.into_iter().find(|t| {
        t.dist_sq <= reach * reach
            && REGION_HALF >= t.min[0]
            && t.max[0] >= -REGION_HALF
            && REGION_HALF >= t.min[1]
            && t.max[1] >= -REGION_HALF
            && (aim.visible)(t.entity, t.center)
    })?;
    if target.dist_sq <= 0.00100000005 || !approaching(aim, &target) {
        return None;
    }
    let dist = libm::sqrtf(target.dist_sq) - REACH;
    if dist <= 0.0 {
        return None;
    }
    let dist = if dist <= 255.0 {
        dist as i32 as u8
    } else {
        255
    };
    Some(MeleeTarget {
        entity: target.entity,
        dist,
        yaw: vectoyaw(target.origin[0] - aim.eye[0], target.origin[1] - aim.eye[1]),
    })
}
