//! The runtime's rigid-body integrator for launched bodies: gravity, a box
//! swept through the world, bounce and friction off what it meets, a spin
//! that turns the body (its box stays upright), and sleep once it rests.
//! The client's dynamic entities and the server's launched script entities
//! move by it; no game's own physics engine is ported.

use glam::{Quat, Vec3};

/// Gravity, in units per second squared.
pub const GRAVITY_Z: f32 = -800.0;
/// The longest step a body takes at once, in milliseconds.
pub const STEP_MS: i32 = 17;
/// A body on the ground this slow for this long sleeps.
pub const SLEEP_SPEED: f32 = 4.0;
pub const SLEEP_MS: i32 = 200;

const CONTENTS_SOLID: u32 = 1;
const CONTENTS_ITEMCLIP: u32 = 0x400;
const WAKE_MASKS: [u32; 2] = [CONTENTS_SOLID | CONTENTS_ITEMCLIP, CONTENTS_SOLID];
const WAKE_LIFTS: [f32; 6] = [0.0, CONTACT_SLOP, 0.25, 0.5, 1.0, 2.0];
const WAKE_FOOTPRINTS: [f32; 3] = [1.0, 0.75, 0.5];
const GROUND_NZ: f32 = 0.7;
const CONTACT_SLOP: f32 = 0.125;

/// One box sweep through the world.
#[derive(Clone, Copy, Debug)]
pub struct Sweep {
    pub fraction: f32,
    pub endpos: [f32; 3],
    pub normal: [f32; 3],
    pub startsolid: bool,
    pub allsolid: bool,
}

/// What a body is swept against.
pub trait BodyWorld {
    fn sweep(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        mask: u32,
    ) -> Sweep;
}

/// A launched body.
#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub origin: Vec3,
    pub vel: Vec3,
    pub rot: Quat,
    pub ang_vel: Vec3,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub bounce: f32,
    pub friction: f32,
    pub gravity_scale: f32,
    pub mask: u32,
    pub on_ground: bool,
    pub asleep_ms: i32,
}

/// What a body is made of.
#[derive(Clone, Copy, Debug)]
pub struct Material {
    pub bounce: f32,
    pub friction: f32,
    pub gravity_scale: f32,
}

impl Body {
    /// A body woken at `origin` with its box: lifted or narrowed until it
    /// starts clear of the world when it can, swept with solids alone when
    /// item clip holds it.
    pub fn wake(
        world: Option<&dyn BodyWorld>,
        origin: Vec3,
        rot: Quat,
        (mins, maxs): ([f32; 3], [f32; 3]),
        material: Material,
        (vel, ang_vel): (Vec3, Vec3),
    ) -> Self {
        let pose = world.and_then(|world| clear_wake_pose(world, origin, mins, maxs));
        let (origin, mins, maxs, mask) = pose.unwrap_or((origin, mins, maxs, CONTENTS_SOLID));
        Self {
            origin,
            vel,
            rot,
            ang_vel,
            mins,
            maxs,
            bounce: material.bounce,
            friction: material.friction,
            gravity_scale: if material.gravity_scale > 0.0 {
                material.gravity_scale
            } else {
                1.0
            },
            mask,
            on_ground: false,
            asleep_ms: 0,
        }
    }

    /// One step of `dt` seconds, swept through `world` when there is one.
    pub fn step(&mut self, world: Option<&dyn BodyWorld>, dt: f32) {
        match world {
            Some(world) => integrate_body(self, world, dt),
            None => integrate_free(self, dt),
        }
    }

    /// Counts the time the body rests; answers whether it now sleeps.
    pub fn rest(&mut self, advanced_ms: i32) -> bool {
        if self.on_ground && self.vel.length() < SLEEP_SPEED {
            self.asleep_ms = self.asleep_ms.saturating_add(advanced_ms);
            self.asleep_ms >= SLEEP_MS
        } else {
            self.asleep_ms = 0;
            false
        }
    }
}

fn clear_wake_pose(
    world: &dyn BodyWorld,
    origin: Vec3,
    mins: [f32; 3],
    maxs: [f32; 3],
) -> Option<(Vec3, [f32; 3], [f32; 3], u32)> {
    for mask in WAKE_MASKS {
        for scale in WAKE_FOOTPRINTS {
            let (mins, maxs) = narrowed_footprint(mins, maxs, scale);
            for lift in WAKE_LIFTS {
                let at = (origin + Vec3::Z * lift).to_array();
                let probe = world.sweep(at, at, mins, maxs, mask);
                if !probe.startsolid && !probe.allsolid {
                    return Some((Vec3::from_array(at), mins, maxs, mask));
                }
            }
        }
    }
    None
}

fn narrowed_footprint(mins: [f32; 3], maxs: [f32; 3], scale: f32) -> ([f32; 3], [f32; 3]) {
    let mut lo = mins;
    let mut hi = maxs;
    for axis in 0..2 {
        let mid = (mins[axis] + maxs[axis]) * 0.5;
        let half = (maxs[axis] - mins[axis]) * 0.5 * scale;
        lo[axis] = mid - half;
        hi[axis] = mid + half;
    }
    (lo, hi)
}

fn integrate_free(body: &mut Body, dt: f32) {
    body.vel.z += GRAVITY_Z * body.gravity_scale * dt;
    body.origin += body.vel * dt;
    let w = body.ang_vel.length();
    if w > 1e-4 {
        body.rot = (Quat::from_axis_angle(body.ang_vel / w, w * dt) * body.rot).normalize();
    }
    body.on_ground = false;
}

fn integrate_body(body: &mut Body, world: &dyn BodyWorld, dt: f32) {
    body.vel.z += GRAVITY_Z * body.gravity_scale * dt;
    let w = body.ang_vel.length();
    if w > 1e-4 {
        body.rot = (Quat::from_axis_angle(body.ang_vel / w, w * dt) * body.rot).normalize();
    }
    let start = body.origin;
    let end = start + body.vel * dt;
    let hit = world.sweep(
        start.to_array(),
        end.to_array(),
        body.mins,
        body.maxs,
        body.mask,
    );
    if hit.allsolid || hit.startsolid {
        body.vel = Vec3::ZERO;
        body.ang_vel = Vec3::ZERO;
        body.on_ground = true;
        return;
    }
    if hit.fraction < 1.0 {
        let n = Vec3::from_array(hit.normal);
        body.origin = Vec3::from_array(hit.endpos) + n * CONTACT_SLOP;
        let vn = body.vel.dot(n);
        if vn < 0.0 {
            body.vel -= (1.0 + body.bounce.max(0.0)) * vn * n;
            let n_comp = body.vel.dot(n) * n;
            let tan = body.vel - n_comp;
            let friction = body.friction.clamp(0.0, 1.0);
            body.vel = n_comp + tan * (1.0 - friction);
            body.ang_vel = body.ang_vel * (1.0 - friction * 0.5) + n.cross(tan) * 0.05;
        }
        body.on_ground = n.z > GROUND_NZ;
    } else {
        body.origin = end;
        body.on_ground = false;
    }
}
