const DEG2RAD: f32 = 0.0174532924;
const INV_360: f32 = 0.00277777785;

fn sincos(degrees: f32) -> (f32, f32) {
    let radians = f64::from(degrees * DEG2RAD);
    (libm::sin(radians) as f32, libm::cos(radians) as f32)
}

pub fn angle_vectors(angles: [f32; 3]) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let (sy, cy) = sincos(angles[1]);
    let (sp, cp) = sincos(angles[0]);
    let (sr, cr) = sincos(angles[2]);
    let forward = [cp * cy, cp * sy, -sp];
    let srsp = sr * sp;
    let right = [cr * sy - srsp * cy, -(cr * cy) - srsp * sy, -(sr * cp)];
    let crsp = cr * sp;
    let up = [crsp * cy + sr * sy, crsp * sy - sr * cy, cr * cp];
    (forward, right, up)
}

/// The pitch a vector points at, in [0, 360).
pub fn vectopitch(v: [f32; 3]) -> f32 {
    if v[1] == 0.0 && v[0] == 0.0 {
        return if -v[2] >= 0.0 { 90.0 } else { 270.0 };
    }
    let xy = libm::sqrtf(v[1] * v[1] + v[0] * v[0]);
    let pitch = libm::atan2(f64::from(v[2]), f64::from(xy)) as f32 * -57.2957764;
    if pitch >= 0.0 { pitch } else { 360.0 + pitch }
}

fn wrap(turns_extended: f64) -> f32 {
    let turns = turns_extended as f32;
    let whole = libm::floor(turns_extended + 0.5);
    ((f64::from(turns) - whole) * 360.0) as f32
}

/// `angle` wrapped into [-180, 180).
pub fn angle_normalize_180(angle: f32) -> f32 {
    wrap(f64::from(angle) * f64::from(INV_360))
}

/// `a1 - a2` wrapped into [-180, 180).
pub fn angle_delta(a1: f32, a2: f32) -> f32 {
    wrap((f64::from(a1) - f64::from(a2)) * f64::from(INV_360))
}

/// Normalizes `v`, returning its length.
pub fn normalize(v: &mut [f32; 3]) -> f32 {
    let len = libm::sqrtf(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
    let n = 1.0 / crate::physics::divisor(len);
    v[0] *= n;
    v[1] *= n;
    v[2] *= n;
    len
}

pub fn lerp(a: [f32; 3], b: [f32; 3], f: f32) -> [f32; 3] {
    [
        (b[0] - a[0]) * f + a[0],
        (b[1] - a[1]) * f + a[1],
        (b[2] - a[2]) * f + a[2],
    ]
}

/// The yaw a vector points at, in [0, 360).
pub fn vectoyaw(v: [f32; 3]) -> f32 {
    if v[1] == 0.0 && v[0] == 0.0 {
        return 0.0;
    }
    let yaw = libm::atan2(f64::from(v[1]), f64::from(v[0])) as f32 * 57.2957764;
    if yaw >= 0.0 { yaw } else { 360.0 + yaw }
}

/// `x` truncated toward zero, as the engine converts floats to integers.
pub fn trunc(x: f32) -> i32 {
    x as i32
}
