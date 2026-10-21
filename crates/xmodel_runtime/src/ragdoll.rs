//! A game-neutral articulated ragdoll: capsule bodies on a skeleton's bones,
//! joined where a child body's first bone sits, kept within each joint's
//! angle limits, apart from the pairs the definition names and out of the
//! world through the caller's sphere sweep. A game supplies the definition
//! (bones, joints, limits, masses), its tuning (gravity, impulses, the start
//! velocity rules) and decides when a ragdoll settles; the stepper hands back
//! the bones as model-space overrides for the DObj pose path.
//!
//! The solver is a position-based rigid body one (substepped, one pass per
//! substep), with no heap allocation once a ragdoll exists.

use glam::{EulerRot, Mat4, Quat, Vec3};

use crate::BoneOverride;

/// The most bodies, joints and smoothly following bones a ragdoll holds.
pub const RAGDOLL_MAX_BODIES: usize = 16;
pub const RAGDOLL_MAX_JOINTS: usize = 16;
pub const RAGDOLL_MAX_LERP_BONES: usize = 8;
/// Substeps per [`Ragdoll::step`].
const SUBSTEPS: usize = 8;
/// World planes kept per capsule end: this step's sweep and the last one's.
const PLANES_PER_END: usize = 2;

/// One body: a capsule from bone `p0` towards bone `p1`.
#[derive(Clone, Debug, PartialEq)]
pub struct RagdollBoneDef {
    pub p0: String,
    pub p1: String,
    pub radius: f32,
    /// Where the body's centre sits from `p0` (0) to `p1` (1).
    pub cog_lerp: f32,
    pub mass: f32,
    /// The capsule's contact friction coefficient.
    pub friction: f32,
    /// The body this one hangs from, which the joint at `p0` ties it to.
    pub parent: Option<usize>,
    /// Added to the capsule's half length past `p0` to `p1`.
    pub extra_half_length: f32,
}

/// One axis of a joint's angle limit (radians), with the friction torque
/// that resists turning about it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RagdollLimitDef {
    pub friction: f32,
    pub min: f32,
    pub max: f32,
}

/// The joint that ties `bone`'s body to its parent's. A hinge turns about
/// its z axis only; a swivel about all three. Limits are on the child
/// body's own axes, from the pose the ragdoll started in; an axis without
/// one turns freely.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RagdollJointDef {
    pub bone: usize,
    pub hinge: bool,
    pub limits: [Option<RagdollLimitDef>; 3],
}

/// A bone without a body of its own (a hand, the head) that follows the body
/// whose `p1` it is, blending over `millis` from where the animation puts it
/// to where it was when the ragdoll started.
#[derive(Clone, Debug, PartialEq)]
pub struct RagdollLerpBoneDef {
    pub tag: String,
    pub millis: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RagdollDef {
    pub bones: Vec<RagdollBoneDef>,
    pub joints: Vec<RagdollJointDef>,
    /// Body pairs that collide with each other.
    pub self_pairs: Vec<(usize, usize)>,
    pub lerp_bones: Vec<RagdollLerpBoneDef>,
}

/// How a starting ragdoll takes the animation's motion: velocities from the
/// two poses `dt_ms` apart, the speed capped at `max_speed` and the upward
/// part at `max_up_speed`, the angular velocity scaled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RagdollStartRules {
    pub dt_ms: f32,
    pub max_speed: f32,
    pub max_up_speed: f32,
    pub angular_scale: f32,
}

/// Values a game reads each step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RagdollTuning {
    pub gravity: [f32; 3],
    /// Scales the radii two bodies of a self-collision pair keep apart by.
    pub self_collision_scale: f32,
    /// Scales every joint's friction (a game may ramp it over the life).
    pub joint_friction_scale: f32,
}

/// What a sphere swept through the world met.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RagdollSweep {
    Clear,
    /// It first touches the world at `fraction` of the way, on a surface
    /// facing `normal`.
    Hit {
        fraction: f32,
        normal: [f32; 3],
    },
    /// It starts inside the world.
    StartSolid,
}

/// The world a ragdoll collides with.
pub trait RagdollWorld {
    fn sweep_sphere(&mut self, start: [f32; 3], end: [f32; 3], radius: f32) -> RagdollSweep;
}

/// How far out of a surface a contact keeps a capsule's end, so the next
/// sweep starts clear of it.
const CONTACT_SKIN: f32 = 0.25;

#[derive(Clone, Copy, Debug, Default)]
struct Plane {
    normal: Vec3,
    dist: f32,
}

#[derive(Clone, Copy, Debug)]
struct Body {
    pos: Vec3,
    rot: Quat,
    vel: Vec3,
    ang: Vec3,
    prev_pos: Vec3,
    prev_rot: Quat,
    inv_mass: f32,
    /// Inverse principal inertia in the capsule's frame (x along its axis).
    inv_inertia: Vec3,
    /// The capsule's frame in the body's.
    capsule: Quat,
    half: f32,
    radius: f32,
    friction: f32,
    /// The body's `p0` bone in the body's frame.
    bone_rot: Quat,
    bone_pos: Vec3,
    planes: [[Option<Plane>; PLANES_PER_END]; 2],
}

impl Body {
    fn inv_inertia_world(&self, v: Vec3) -> Vec3 {
        let frame = self.rot * self.capsule;
        frame * (self.inv_inertia * (frame.inverse() * v))
    }

    fn end(&self, which: usize) -> Vec3 {
        let axis = self.rot * (self.capsule * Vec3::X);
        if which == 0 {
            self.pos - axis * self.half
        } else {
            self.pos + axis * self.half
        }
    }

    fn prev_end(&self, which: usize) -> Vec3 {
        let axis = self.prev_rot * (self.capsule * Vec3::X);
        if which == 0 {
            self.prev_pos - axis * self.half
        } else {
            self.prev_pos + axis * self.half
        }
    }

    fn world(&self) -> Mat4 {
        Mat4::from_rotation_translation(self.rot, self.pos)
    }

    /// Moves the body as if pushed by `impulse` at `arm` from its centre.
    fn shift(&mut self, impulse: Vec3, arm: Vec3) {
        self.pos += impulse * self.inv_mass;
        let turn = self.inv_inertia_world(arm.cross(impulse));
        self.rot =
            (self.rot + Quat::from_xyzw(turn.x, turn.y, turn.z, 0.0) * self.rot * 0.5).normalize();
    }

    fn turn(&mut self, angular: Vec3) {
        let turn = self.inv_inertia_world(angular);
        self.rot =
            (self.rot + Quat::from_xyzw(turn.x, turn.y, turn.z, 0.0) * self.rot * 0.5).normalize();
    }

    fn weight(&self, arm: Vec3, normal: Vec3) -> f32 {
        let lever = arm.cross(normal);
        self.inv_mass + lever.dot(self.inv_inertia_world(lever))
    }
}

#[derive(Clone, Copy, Debug)]
struct Joint {
    parent: usize,
    child: usize,
    hinge: bool,
    limits: [Option<RagdollLimitDef>; 3],
    /// The anchor in each body's frame.
    parent_arm: Vec3,
    child_arm: Vec3,
    /// The child's orientation in the parent's when the ragdoll started.
    rest: Quat,
}

#[derive(Clone, Copy, Debug)]
struct LerpBone {
    def: usize,
    body: usize,
    /// The bone's pose relative to its body's `p0` bone at the start.
    start_rot: Quat,
    start_pos: Vec3,
}

/// Whether a ragdoll moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RagdollState {
    Active,
    Settled,
}

/// A queued push: an impulse on one body (or every body) at its centre plus
/// an offset.
#[derive(Clone, Copy, Debug)]
struct Push {
    body: Option<usize>,
    impulse: Vec3,
}

#[derive(Clone, Debug)]
pub struct Ragdoll {
    bodies: [Body; RAGDOLL_MAX_BODIES],
    body_count: usize,
    joints: [Joint; RAGDOLL_MAX_JOINTS],
    joint_count: usize,
    lerps: [LerpBone; RAGDOLL_MAX_LERP_BONES],
    lerp_count: usize,
    pushes: [Option<Push>; 2],
    pub state: RagdollState,
    /// Milliseconds since the ragdoll started or was last woken.
    pub life_ms: u32,
    rng: u32,
}

const EMPTY_BODY: Body = Body {
    pos: Vec3::ZERO,
    rot: Quat::IDENTITY,
    vel: Vec3::ZERO,
    ang: Vec3::ZERO,
    prev_pos: Vec3::ZERO,
    prev_rot: Quat::IDENTITY,
    inv_mass: 0.0,
    inv_inertia: Vec3::ZERO,
    capsule: Quat::IDENTITY,
    half: 0.0,
    radius: 0.0,
    friction: 0.0,
    bone_rot: Quat::IDENTITY,
    bone_pos: Vec3::ZERO,
    planes: [[None; PLANES_PER_END]; 2],
};

const EMPTY_JOINT: Joint = Joint {
    parent: 0,
    child: 0,
    hinge: false,
    limits: [None; 3],
    parent_arm: Vec3::ZERO,
    child_arm: Vec3::ZERO,
    rest: Quat::IDENTITY,
};

const EMPTY_LERP: LerpBone = LerpBone {
    def: 0,
    body: 0,
    start_rot: Quat::IDENTITY,
    start_pos: Vec3::ZERO,
};

fn rotation_translation(m: Mat4) -> (Quat, Vec3) {
    let (_, rot, pos) = m.to_scale_rotation_translation();
    (rot.normalize(), pos)
}

impl Ragdoll {
    /// Builds a ragdoll on a skeleton posed at `now` (world bone matrices),
    /// moving as it did since `prev`. `find` looks a bone up by name. `None`
    /// when the skeleton lacks a body's bone or the definition does not fit.
    pub fn start(
        def: &RagdollDef,
        find: impl Fn(&str) -> Option<usize>,
        now: &[Mat4],
        prev: &[Mat4],
        rules: RagdollStartRules,
        seed: u32,
    ) -> Option<Self> {
        if def.bones.is_empty()
            || def.bones.len() > RAGDOLL_MAX_BODIES
            || def.joints.len() > RAGDOLL_MAX_JOINTS
        {
            return None;
        }
        let mut ragdoll = Self {
            bodies: [EMPTY_BODY; RAGDOLL_MAX_BODIES],
            body_count: def.bones.len(),
            joints: [EMPTY_JOINT; RAGDOLL_MAX_JOINTS],
            joint_count: 0,
            lerps: [EMPTY_LERP; RAGDOLL_MAX_LERP_BONES],
            lerp_count: 0,
            pushes: [None; 2],
            state: RagdollState::Active,
            life_ms: 0,
            rng: seed | 1,
        };
        let per_second = 1000.0 / rules.dt_ms.max(1.0);
        for (index, bone) in def.bones.iter().enumerate() {
            let p0 = find(&bone.p0)?;
            let p1 = find(&bone.p1)?;
            let (bone_rot, a) = rotation_translation(*now.get(p0)?);
            let (_, b) = rotation_translation(*now.get(p1)?);
            let (prev_rot, prev_a) = rotation_translation(*prev.get(p0)?);
            let (_, prev_b) = rotation_translation(*prev.get(p1)?);
            let pos = a.lerp(b, bone.cog_lerp);
            let prev_pos = prev_a.lerp(prev_b, bone.cog_lerp);
            let span = b - a;
            let length = span.length();
            let axis = if length > 1e-4 {
                bone_rot.inverse() * (span / length)
            } else {
                Vec3::X
            };
            let half = length * 0.5 + bone.extra_half_length;
            let mass = bone.mass.max(1e-3);
            let r2 = bone.radius * bone.radius;
            let along = 0.5 * mass * r2;
            let across = mass * (3.0 * r2 + 4.0 * half * half) / 12.0;
            let mut vel = (pos - prev_pos) * per_second;
            if vel.length_squared() > rules.max_speed * rules.max_speed {
                vel = vel.normalize() * rules.max_speed;
            }
            vel.z = vel.z.min(rules.max_up_speed);
            let mut delta = bone_rot * prev_rot.inverse();
            if delta.w < 0.0 {
                delta = -delta;
            }
            let (spin_axis, angle) = delta.to_axis_angle();
            let ang = if angle.is_finite() {
                spin_axis * angle * rules.angular_scale * per_second
            } else {
                Vec3::ZERO
            };
            ragdoll.bodies[index] = Body {
                pos,
                rot: bone_rot,
                vel,
                ang,
                prev_pos: pos,
                prev_rot: bone_rot,
                inv_mass: 1.0 / mass,
                inv_inertia: Vec3::new(1.0 / along.max(1e-6), 1.0 / across, 1.0 / across),
                capsule: Quat::from_rotation_arc(Vec3::X, axis),
                half,
                radius: bone.radius,
                friction: bone.friction,
                bone_rot: Quat::IDENTITY,
                bone_pos: bone_rot.inverse() * (a - pos),
                planes: [[None; PLANES_PER_END]; 2],
            };
        }
        for joint in &def.joints {
            let child = joint.bone;
            let parent = def.bones.get(child)?.parent?;
            if parent >= ragdoll.body_count {
                return None;
            }
            let (a, b) = (&ragdoll.bodies[parent], &ragdoll.bodies[child]);
            let anchor = b.pos + b.rot * b.bone_pos;
            ragdoll.joints[ragdoll.joint_count] = Joint {
                parent,
                child,
                hinge: joint.hinge,
                limits: joint.limits,
                parent_arm: a.rot.inverse() * (anchor - a.pos),
                child_arm: b.rot.inverse() * (anchor - b.pos),
                rest: a.rot.inverse() * b.rot,
            };
            ragdoll.joint_count += 1;
        }
        for (index, lerp) in def.lerp_bones.iter().enumerate() {
            if ragdoll.lerp_count == RAGDOLL_MAX_LERP_BONES {
                break;
            }
            let Some(bone) = find(&lerp.tag) else {
                continue;
            };
            let Some(body) = def.bones.iter().position(|b| b.p1 == lerp.tag) else {
                continue;
            };
            let parent = find(&def.bones[body].p0)?;
            let relative = now.get(parent)?.inverse() * *now.get(bone)?;
            let (start_rot, start_pos) = rotation_translation(relative);
            ragdoll.lerps[ragdoll.lerp_count] = LerpBone {
                def: index,
                body,
                start_rot,
                start_pos,
            };
            ragdoll.lerp_count += 1;
        }
        Some(ragdoll)
    }

    fn random(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }

    /// A point within `scale` of each body's centre on every axis, as games
    /// jitter where their pushes land.
    fn jitter(&mut self, scale: f32) -> Vec3 {
        Vec3::new(
            (self.random() * 2.0 - 1.0) * scale,
            (self.random() * 2.0 - 1.0) * scale,
            (self.random() * 2.0 - 1.0) * scale,
        )
    }

    /// Queues a push (at most two between steps): `impulse` on the body whose
    /// first or last bone is `bone`, or on every body.
    pub fn launch(&mut self, def: &RagdollDef, impulse: [f32; 3], bone: Option<&str>) {
        let Some(slot) = self.pushes.iter_mut().find(|push| push.is_none()) else {
            return;
        };
        let body =
            match bone {
                None => None,
                Some(tag) => {
                    let Some(body) = def.bones.iter().position(|b| {
                        b.p0.eq_ignore_ascii_case(tag) || b.p1.eq_ignore_ascii_case(tag)
                    }) else {
                        return;
                    };
                    Some(body)
                }
            };
        *slot = Some(Push {
            body,
            impulse: Vec3::from_array(impulse),
        });
    }

    /// Each body's centre, for a game's own push (an explosion's).
    pub fn body_centers(&self) -> impl Iterator<Item = [f32; 3]> + '_ {
        self.bodies[..self.body_count]
            .iter()
            .map(|body| body.pos.to_array())
    }

    pub fn body_count(&self) -> usize {
        self.body_count
    }

    /// The first body's centre (the root).
    pub fn root(&self) -> [f32; 3] {
        self.bodies[0].pos.to_array()
    }

    /// Pushes body `body` by `impulse` at its centre plus `offset` now.
    pub fn push_body(&mut self, body: usize, impulse: [f32; 3], offset: [f32; 3]) {
        if let Some(target) = self.bodies[..self.body_count].get_mut(body) {
            let impulse = Vec3::from_array(impulse);
            target.vel += impulse * target.inv_mass;
            let spin = target.inv_inertia_world(Vec3::from_array(offset).cross(impulse));
            target.ang += spin;
        }
    }

    /// Wakes a settled ragdoll and restarts its life.
    pub fn wake(&mut self) {
        self.state = RagdollState::Active;
        self.life_ms = 0;
    }

    /// The bodies' mean height.
    pub fn mean_height(&self) -> f32 {
        let bodies = &self.bodies[..self.body_count];
        bodies.iter().map(|b| b.pos.z).sum::<f32>() / bodies.len() as f32
    }

    /// Steps an active ragdoll `dt_ms` forward.
    pub fn step(
        &mut self,
        def: &RagdollDef,
        tuning: RagdollTuning,
        dt_ms: u32,
        jitter: f32,
        world: &mut impl RagdollWorld,
    ) {
        self.life_ms = self.life_ms.saturating_add(dt_ms);
        if self.state == RagdollState::Settled || dt_ms == 0 {
            return;
        }
        let dt = dt_ms as f32 / 1000.0;
        let gravity = Vec3::from_array(tuning.gravity);
        let pushes = [self.pushes[0].take(), self.pushes[1].take()];
        for push in pushes.into_iter().flatten() {
            for body in 0..self.body_count {
                if push.body.is_none_or(|only| only == body) {
                    let offset = self.jitter(jitter);
                    self.push_body(body, push.impulse.to_array(), offset.to_array());
                }
            }
        }
        self.sweep(dt, gravity, world);
        let h = dt / SUBSTEPS as f32;
        for _ in 0..SUBSTEPS {
            for body in &mut self.bodies[..self.body_count] {
                body.prev_pos = body.pos;
                body.prev_rot = body.rot;
                body.vel += gravity * h;
                body.pos += body.vel * h;
                let spin = body.ang;
                body.rot = (body.rot
                    + Quat::from_xyzw(spin.x, spin.y, spin.z, 0.0) * body.rot * (0.5 * h))
                    .normalize();
            }
            for joint in 0..self.joint_count {
                self.solve_anchor(joint);
                self.solve_limits(joint);
            }
            for &(a, b) in &def.self_pairs {
                self.solve_pair(a, b, tuning.self_collision_scale);
            }
            for body in 0..self.body_count {
                self.solve_contacts(body);
            }
            for body in &mut self.bodies[..self.body_count] {
                body.vel = (body.pos - body.prev_pos) / h;
                let mut delta = body.rot * body.prev_rot.inverse();
                if delta.w < 0.0 {
                    delta = -delta;
                }
                body.ang = Vec3::new(delta.x, delta.y, delta.z) * (2.0 / h);
            }
            for joint in 0..self.joint_count {
                self.solve_friction(joint, tuning.joint_friction_scale * h);
            }
        }
    }

    /// Sweeps each capsule end along where it is heading this step and keeps
    /// the plane it would touch.
    fn sweep(&mut self, dt: f32, gravity: Vec3, world: &mut impl RagdollWorld) {
        for body in &mut self.bodies[..self.body_count] {
            for end in 0..2 {
                let start = body.end(end);
                let arm = start - body.pos;
                let heading = body.vel + body.ang.cross(arm);
                let target = start + heading * dt + gravity * (0.5 * dt * dt);
                let planes = &mut body.planes[end];
                let found =
                    match world.sweep_sphere(start.to_array(), target.to_array(), body.radius) {
                        // Inside the world already: keep the surfaces it had.
                        RagdollSweep::StartSolid => continue,
                        RagdollSweep::Clear => None,
                        RagdollSweep::Hit { fraction, normal } => {
                            let normal = Vec3::from_array(normal);
                            let at = start.lerp(target, fraction.clamp(0.0, 1.0));
                            Some(Plane {
                                normal,
                                dist: normal.dot(at) + CONTACT_SKIN,
                            })
                        }
                    };
                if found.is_some() || planes[0].is_some() {
                    planes[1] = planes[0];
                    planes[0] = found;
                }
            }
        }
    }

    fn pair_mut(&mut self, a: usize, b: usize) -> (&mut Body, &mut Body) {
        if a < b {
            let (low, high) = self.bodies.split_at_mut(b);
            (&mut low[a], &mut high[0])
        } else {
            let (low, high) = self.bodies.split_at_mut(a);
            (&mut high[0], &mut low[b])
        }
    }

    /// Moves `b` by `correction` at `arm_b` and `a` the other way at `arm_a`,
    /// shared by their weights along the correction.
    fn correct(&mut self, a: usize, b: usize, arm_a: Vec3, arm_b: Vec3, correction: Vec3) {
        let length = correction.length();
        if length <= 1e-6 {
            return;
        }
        let normal = correction / length;
        let (body_a, body_b) = self.pair_mut(a, b);
        let total = body_a.weight(arm_a, normal) + body_b.weight(arm_b, normal);
        if total <= 0.0 {
            return;
        }
        let impulse = normal * (length / total);
        body_b.shift(impulse, arm_b);
        body_a.shift(-impulse, arm_a);
    }

    fn solve_anchor(&mut self, joint: usize) {
        let joint = self.joints[joint];
        let (a, b) = (&self.bodies[joint.parent], &self.bodies[joint.child]);
        let arm_a = a.rot * joint.parent_arm;
        let arm_b = b.rot * joint.child_arm;
        let gap = (a.pos + arm_a) - (b.pos + arm_b);
        self.correct(joint.parent, joint.child, arm_a, arm_b, gap);
    }

    fn solve_limits(&mut self, joint: usize) {
        let joint = self.joints[joint];
        let (a, b) = (&self.bodies[joint.parent], &self.bodies[joint.child]);
        let deviation = joint.rest.inverse() * (a.rot.inverse() * b.rot);
        let (x, y, z) = deviation.to_euler(EulerRot::XYZ);
        let mut angles = [x, y, z];
        let mut clamped = false;
        for (axis, angle) in angles.iter_mut().enumerate() {
            let (min, max) = match joint.limits[axis] {
                _ if joint.hinge && axis < 2 => (0.0, 0.0),
                Some(limit) => (limit.min, limit.max),
                None => continue,
            };
            let limited = angle.clamp(min, max);
            if limited != *angle {
                *angle = limited;
                clamped = true;
            }
        }
        if !clamped {
            return;
        }
        let allowed = Quat::from_euler(EulerRot::XYZ, angles[0], angles[1], angles[2]);
        let target = a.rot * joint.rest * allowed;
        let mut error = target * b.rot.inverse();
        if error.w < 0.0 {
            error = -error;
        }
        let (axis, angle) = error.to_axis_angle();
        if !angle.is_finite() || angle <= 1e-6 {
            return;
        }
        let total = axis.dot(a.inv_inertia_world(axis)) + axis.dot(b.inv_inertia_world(axis));
        if total <= 0.0 {
            return;
        }
        let impulse = axis * (angle / total);
        let (parent, child) = self.pair_mut(joint.parent, joint.child);
        child.turn(impulse);
        parent.turn(-impulse);
    }

    fn solve_pair(&mut self, a: usize, b: usize, scale: f32) {
        if a >= self.body_count || b >= self.body_count || a == b {
            return;
        }
        let (body_a, body_b) = (&self.bodies[a], &self.bodies[b]);
        let (pa, pb) =
            closest_between_segments(body_a.end(0), body_a.end(1), body_b.end(0), body_b.end(1));
        let reach = (body_a.radius + body_b.radius) * scale;
        let apart = pb - pa;
        let distance = apart.length();
        if distance >= reach || distance <= 1e-5 {
            return;
        }
        let correction = apart / distance * (reach - distance);
        let (arm_a, arm_b) = (pa - body_a.pos, pb - body_b.pos);
        self.correct(a, b, arm_a, arm_b, correction);
    }

    fn solve_contacts(&mut self, index: usize) {
        for end in 0..2 {
            for plane in 0..PLANES_PER_END {
                let body = &mut self.bodies[index];
                let Some(Plane { normal, dist }) = body.planes[end][plane] else {
                    continue;
                };
                let centre = body.end(end);
                let depth = dist - normal.dot(centre);
                if depth <= 0.0 {
                    continue;
                }
                let arm = centre - body.pos;
                let weight = body.weight(arm, normal);
                if weight <= 0.0 {
                    continue;
                }
                body.shift(normal * (depth / weight), arm);
                // Coulomb friction on how far the contact slid this substep.
                let centre = body.end(end);
                let slid = centre - body.prev_end(end);
                let tangent = slid - normal * normal.dot(slid);
                let length = tangent.length();
                if length <= 1e-6 {
                    continue;
                }
                let arm = centre - body.pos;
                let direction = tangent / length;
                let weight = body.weight(arm, direction);
                if weight <= 0.0 {
                    continue;
                }
                let stop = length.min(body.friction * depth);
                body.shift(-direction * (stop / weight), arm);
            }
        }
    }

    /// Joint friction: a torque up to each limited axis's friction resists
    /// the child turning against its parent.
    fn solve_friction(&mut self, joint: usize, torque_time: f32) {
        let joint = self.joints[joint];
        for axis in 0..3 {
            let Some(limit) = joint.limits[axis] else {
                continue;
            };
            let (a, b) = (&self.bodies[joint.parent], &self.bodies[joint.child]);
            let direction = b.rot * Vec3::AXES[axis];
            let relative = (b.ang - a.ang).dot(direction);
            let total = direction.dot(a.inv_inertia_world(direction))
                + direction.dot(b.inv_inertia_world(direction));
            if total <= 0.0 {
                continue;
            }
            let most = limit.friction * torque_time;
            let impulse = (-relative / total).clamp(-most, most);
            let spin_b = b.inv_inertia_world(direction * impulse);
            let spin_a = a.inv_inertia_world(direction * impulse);
            let (parent, child) = self.pair_mut(joint.parent, joint.child);
            child.ang += spin_b;
            parent.ang -= spin_a;
        }
    }

    /// The bones the ragdoll places, in the model space of `world_from_model`.
    /// `animated` is the skeleton as its animation poses it now (for the
    /// bones that blend), `elapsed_ms` how far that blend is.
    pub fn pose(
        &self,
        def: &RagdollDef,
        find: impl Fn(&str) -> Option<usize>,
        animated: &[Mat4],
        world_from_model: Mat4,
        out: &mut Vec<BoneOverride>,
    ) {
        out.clear();
        let model_from_world = world_from_model.inverse();
        let mut bone_world = [Mat4::IDENTITY; RAGDOLL_MAX_BODIES];
        for (index, body) in self.bodies[..self.body_count].iter().enumerate() {
            let world =
                body.world() * Mat4::from_rotation_translation(body.bone_rot, body.bone_pos);
            bone_world[index] = world;
            let (rotation, origin) = rotation_translation(model_from_world * world);
            out.push(BoneOverride {
                tag: def.bones[index].p0.clone(),
                rotation: rotation.to_array(),
                origin: origin.to_array(),
            });
        }
        for lerp in &self.lerps[..self.lerp_count] {
            let lerp_def = &def.lerp_bones[lerp.def];
            let t = if lerp_def.millis == 0 {
                1.0
            } else {
                (self.life_ms as f32 / lerp_def.millis as f32).clamp(0.0, 1.0)
            };
            let parent = find(&def.bones[lerp.body].p0);
            let bone = find(&lerp_def.tag);
            let (now_rot, now_pos) = match (parent, bone) {
                (Some(parent), Some(bone)) if parent < animated.len() && bone < animated.len() => {
                    rotation_translation(animated[parent].inverse() * animated[bone])
                }
                _ => (lerp.start_rot, lerp.start_pos),
            };
            let rotation = now_rot.slerp(lerp.start_rot, t);
            let position = now_pos.lerp(lerp.start_pos, t);
            let world = bone_world[lerp.body] * Mat4::from_rotation_translation(rotation, position);
            let (rotation, origin) = rotation_translation(model_from_world * world);
            out.push(BoneOverride {
                tag: lerp_def.tag.clone(),
                rotation: rotation.to_array(),
                origin: origin.to_array(),
            });
        }
    }
}

/// The closest points between segments `a0..a1` and `b0..b1`.
fn closest_between_segments(a0: Vec3, a1: Vec3, b0: Vec3, b1: Vec3) -> (Vec3, Vec3) {
    let d1 = a1 - a0;
    let d2 = b1 - b0;
    let r = a0 - b0;
    let a = d1.dot(d1);
    let e = d2.dot(d2);
    let f = d2.dot(r);
    let (s, t) = if a <= 1e-8 && e <= 1e-8 {
        (0.0, 0.0)
    } else if a <= 1e-8 {
        (0.0, (f / e).clamp(0.0, 1.0))
    } else {
        let c = d1.dot(r);
        if e <= 1e-8 {
            ((-c / a).clamp(0.0, 1.0), 0.0)
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            let mut s = if denom > 1e-8 {
                ((b * f - c * e) / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let mut t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
            (s, t)
        }
    };
    (a0 + d1 * s, b0 + d2 * t)
}
