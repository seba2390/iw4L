//! Black Ops' actor physics in its default collision (`phys_ai_collision_mode`
//! 1): the actor's body is the player's character shape built from its own
//! box, every sweep is a cast through the physics world, the ground sweep is
//! the physics ground cast and the slide is the physics slide, which pushes
//! a body that starts inside something out of it. Around them runs the
//! actor move both collision modes share: the ground check, the walk along
//! the ground or the fall, and the step up and back down.

use game_api::actor::{
    ActorPhysics, ActorPhysicsHit, ActorPhysicsKind, ActorPhysicsMove, ActorPhysicsResult,
    ActorWorld,
};
use game_api::movement::ENTITY_WORLD;
use game_api::{Rule, Unknown, unknown};

use crate::physics::{Hit, PhysicsWorld};

pub(crate) struct T5ActorPhysics;

pub(crate) static ACTOR_PHYSICS: T5ActorPhysics = T5ActorPhysics;

type V3 = [f32; 3];

/// The ground sweep starts this far up, and reaches this far down off the
/// ground.
const GROUND_LIFT: f32 = 0.25;
const STEP_HEIGHT: f32 = 18.0;
const PRONE_STEP_HEIGHT: f32 = 10.0;
/// Rising into the ground plane faster than this leaves the ground.
const LEAVING_GROUND: f32 = 10.0;
const OVERCLIP: f32 = f32::from_bits(0x3f80_20c5);
const UNDERCLIP: f32 = f32::from_bits(0x3f7f_be87);
/// A step back down onto a plane facing up less than this is not taken.
const STEP_DOWN_NORMAL: f32 = f32::from_bits(0x3e99_999a);
/// A step that ends this far above where the move started went up a step:
/// what the move hit was that step.
const STEPPED_UP: f32 = f32::from_bits(0x3dcc_cccd);
const CONTENTS_BODY: u32 = 0x0200_0000;

fn physics_world_unknown() -> &'static Unknown {
    unknown!(
        "t5.ai.physics_world",
        "the match cannot list Black Ops' physics world, so actors move by the trace slide (Black Ops' collision mode 0)",
        "the map's brush corner points and partition vertices in the match's collision"
    )
}

impl ActorPhysics for T5ActorPhysics {
    fn physics(
        &self,
        world: &dyn ActorWorld,
        actor_move: &ActorPhysicsMove,
    ) -> Rule<Option<ActorPhysicsResult>> {
        let mode = world
            .dvar("phys_ai_collision_mode")
            .and_then(|value| value.trim().parse::<i32>().ok());
        if mode == Some(0) {
            return Rule::Known(None);
        }
        if !world.lists_solid_pieces() {
            return Rule::Unknown(physics_world_unknown());
        }
        let physics = PhysicsWorld::new(world);
        let mut actor = Actor {
            physics: &physics,
            world,
            mv: actor_move,
            step_height: if actor_move.prone {
                PRONE_STEP_HEIGHT
            } else {
                STEP_HEIGHT
            },
            s: State {
                origin: actor_move.origin,
                velocity: actor_move.velocity,
                ground_entity: actor_move.ground_entity,
                hit: None,
                deflected: false,
                mask: actor_move.mask,
                walking: false,
                ground_plane: false,
                ground: None,
            },
        };
        if actor.run().is_none() {
            return Rule::Unknown(physics_world_unknown());
        }
        let s = actor.s;
        Rule::Known(Some(ActorPhysicsResult {
            origin: s.origin,
            velocity: s.velocity,
            ground_entity: s.ground_entity,
            walking: s
                .walking
                .then(|| s.ground.map_or([0.0, 0.0, 1.0], |g| g.normal)),
            hit: s.hit,
            deflected: s.deflected,
        }))
    }
}

/// The move's state: the actor's and what the move found under it.
#[derive(Clone, Copy)]
struct State {
    origin: V3,
    velocity: V3,
    ground_entity: Option<i32>,
    hit: Option<ActorPhysicsHit>,
    deflected: bool,
    mask: u32,
    walking: bool,
    /// The ground sweep met a plane under the actor.
    ground_plane: bool,
    /// The last ground sweep.
    ground: Option<Hit>,
}

struct Actor<'a, 'w> {
    physics: &'a PhysicsWorld<'w>,
    world: &'a dyn ActorWorld,
    mv: &'a ActorPhysicsMove,
    step_height: f32,
    s: State,
}

fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn lerp(a: V3, b: V3, t: f32) -> V3 {
    core::array::from_fn(|i| (b[i] - a[i]) * t + a[i])
}

/// `v` with its part into `normal` taken off, a little over.
fn clip(v: V3, normal: V3) -> V3 {
    let into = dot(v, normal);
    let into = into * if into < 0.0 { OVERCLIP } else { UNDERCLIP };
    core::array::from_fn(|i| v[i] - normal[i] * into)
}

/// `v` along a walkable plane at its own horizontal pace, when it runs more
/// across than up or down.
fn along_walkable(v: V3, normal: V3) -> V3 {
    [
        v[0] * normal[2],
        v[1] * normal[2],
        -(v[0] * normal[0]) - v[1] * normal[1],
    ]
}

impl Actor<'_, '_> {
    fn run(&mut self) -> Option<bool> {
        let inv = 1.0 / self.mv.seconds;
        let delta = self.mv.delta;
        if self.mv.kind == ActorPhysicsKind::Slide {
            self.s.velocity = delta.map(|d| d * inv);
            self.slide(false)?;
            return Some(true);
        }
        self.ground_trace()?;
        match self.mv.kind {
            ActorPhysicsKind::Across => {
                self.s.velocity = [delta[0] * inv, delta[1] * inv, 0.0];
                self.step_slide(true)
            }
            _ if !self.s.walking => self.air_move(),
            _ => self.ground_walk(),
        }
    }

    fn trace(&self, start: V3, end: V3) -> Option<Hit> {
        self.physics.trace(
            start,
            end,
            self.mv.mins,
            self.mv.maxs,
            self.mv.entity,
            self.s.mask,
        )
    }

    fn ground_cast(&self, start: V3, end: V3) -> Option<(Hit, f32)> {
        self.physics.ground_trace_first(
            start,
            end,
            self.mv.mins,
            self.mv.maxs,
            self.mv.entity,
            self.s.mask,
        )
    }

    /// What the actor stands on: a sweep down from a hair above it, a
    /// step's height while it stood and is not rising (or rides a moving
    /// entity), else a hair. A sweep that starts inside a body is tried
    /// again without bodies, and one that starts inside anything again from
    /// the actor's feet; still inside, the actor walks on what it is in.
    fn ground_trace(&mut self) -> Option<()> {
        let o = self.s.origin;
        let mover = self.s.ground_entity.is_some_and(|e| self.world.moving(e));
        let mut start = [o[0], o[1], o[2] + GROUND_LIFT];
        let depth = if mover || (self.s.velocity[2] <= 0.0 && self.s.ground_entity.is_some()) {
            self.step_height
        } else {
            GROUND_LIFT
        };
        let end = [o[0], o[1], o[2] - depth];
        let (mut trace, mut first) = self.ground_cast(start, end)?;
        if trace.startsolid {
            if trace.contents & CONTENTS_BODY != 0 {
                self.s.mask &= !CONTENTS_BODY;
                (trace, first) = self.ground_cast(start, end)?;
            }
            if trace.startsolid {
                start = o;
                (trace, first) = self.ground_cast(start, end)?;
                if trace.startsolid {
                    self.s.ground = Some(trace);
                    self.s.ground_entity = trace.entity;
                    self.s.ground_plane = false;
                    self.s.walking = true;
                    return Some(());
                }
            }
        }
        self.s.ground = Some(trace);
        if trace.fraction == 1.0 {
            self.s.ground_entity = None;
            self.s.ground_plane = false;
            self.s.walking = false;
            return Some(());
        }
        let n = trace.normal;
        if 0.0 < self.s.velocity[2] && LEAVING_GROUND < dot(self.s.velocity, n) && !mover {
            self.s.ground_entity = None;
            self.s.ground_plane = false;
            self.s.walking = false;
            return Some(());
        }
        if !trace.walkable {
            self.s.ground_entity = None;
            self.s.ground_plane = true;
            self.s.walking = false;
            if self.s.hit.is_none() && (n[0] != 0.0 || n[1] != 0.0) {
                self.s.hit = Some(ActorPhysicsHit {
                    entity: trace.entity.unwrap_or(ENTITY_WORLD),
                    origin: o,
                    normal: n,
                });
            }
            return Some(());
        }
        // The height comes from the earliest contact, the ground from the
        // one the sweep picked.
        let z = (end[2] - start[2]) * first + start[2];
        if z < self.s.origin[2] || self.mv.kind == ActorPhysicsKind::Across {
            self.s.origin[2] = z;
        }
        self.s.ground_plane = true;
        self.s.walking = true;
        self.s.ground_entity = trace.entity;
        Some(())
    }

    /// A step on the ground: the frame's step as a velocity laid along the
    /// ground at its own horizontal speed.
    fn ground_walk(&mut self) -> Option<bool> {
        let inv = 1.0 / self.mv.seconds;
        let mut v = [self.mv.delta[0] * inv, self.mv.delta[1] * inv, 0.0];
        let old = [v[0], v[1]];
        let speed = (v[0] * v[0] + v[1] * v[1]).sqrt();
        let (normal, walkable) = self
            .s
            .ground
            .map_or(([0.0; 3], false), |g| (g.normal, g.walkable));
        v = if walkable {
            along_walkable(v, normal)
        } else {
            clip(v, normal)
        };
        if 0.0 < v[1] * old[1] + v[0] * old[0] {
            let length = dot(v, v).sqrt();
            let length = if length <= 0.0 { 1.0 } else { length };
            let inv_length = 1.0 / length;
            v = v.map(|c| c * inv_length * speed);
        }
        self.s.velocity = v;
        if v[0] == 0.0 && v[1] == 0.0 && self.s.ground_plane {
            return Some(true);
        }
        self.step_slide(false)
    }

    /// A step in the air: the velocity the actor has, laid along or off a
    /// ground plane it is over, falling.
    fn air_move(&mut self) -> Option<bool> {
        if self.s.ground_plane
            && let Some(ground) = self.s.ground
        {
            let v = self.s.velocity;
            if ground.walkable && v[2] * v[2] <= v[0] * v[0] + v[1] * v[1] {
                self.s.velocity = along_walkable(v, ground.normal);
                return self.step_slide(true);
            }
            self.s.velocity = clip(v, ground.normal);
        }
        self.step_slide(true)
    }

    /// The slide, then the same slide a step higher and back down; the
    /// stepped one stands unless the plain one ended nearer the wished
    /// point, or it would come down on ground too steep.
    fn step_slide(&mut self, gravity: bool) -> Option<bool> {
        let start = self.s.origin;
        let start_velocity = self.s.velocity;
        if !self.slide(gravity)? {
            return Some(true);
        }
        let step = self.step_height;
        if 0.0 < self.s.velocity[2] && !self.s.walking {
            let below = [start[0], start[1], start[2] - step];
            let trace = self.trace(start, below)?;
            if trace.fraction == 1.0 || !trace.walkable {
                return Some(true);
            }
        }
        let up = [start[0], start[1], start[2] + step];
        // A physics cast is never all solid: the step up always runs.
        let trace = self.trace(start, up)?;
        let plain = self.s;
        self.s.hit = None;
        self.s.ground_plane = false;
        self.s.origin = lerp(start, up, trace.fraction);
        let lifted = self.s.origin[2];
        self.s.velocity = start_velocity;
        self.slide(gravity)?;
        let ft = self.mv.seconds;
        let wished = [
            start_velocity[0] * ft + start[0],
            start_velocity[1] * ft + start[1],
        ];
        let miss = |p: V3| (wished[0] - p[0]).powi(2) + (wished[1] - p[1]).powi(2);
        if miss(plain.origin) < miss(self.s.origin) {
            self.s = plain;
            return Some(true);
        }
        let o = self.s.origin;
        let down = [o[0], o[1], start[2] - lifted + o[2]];
        let trace = self.trace(o, down)?;
        if !trace.startsolid {
            self.s.origin = lerp(o, down, trace.fraction);
        }
        if trace.fraction < 1.0 {
            if trace.normal[2] < STEP_DOWN_NORMAL {
                self.s = plain;
                return Some(true);
            }
            let v = self.s.velocity;
            self.s.velocity = if trace.walkable && v[2] * v[2] <= v[0] * v[0] + v[1] * v[1] {
                along_walkable(v, trace.normal)
            } else {
                let into = dot(v, trace.normal);
                let into = if 0.0 <= into {
                    into / OVERCLIP
                } else {
                    into * OVERCLIP
                };
                core::array::from_fn(|i| v[i] - trace.normal[i] * into)
            };
            if start[2].max(plain.origin[2]) + STEPPED_UP < self.s.origin[2] {
                self.s.hit = None;
            }
        }
        Some(true)
    }

    /// The physics slide along the velocity for the frame; whether it met
    /// anything. Every contact marks the move deflected, and the first one
    /// facing across is what the move hit.
    fn slide(&mut self, gravity: bool) -> Option<bool> {
        let slide = movement_t5::PhysicsSlide {
            origin: self.s.origin,
            velocity: self.s.velocity,
            mins: self.mv.mins,
            maxs: self.mv.maxs,
            contentmask: self.s.mask,
            frametime: self.mv.seconds,
            // The slide reads the gravity as a whole number.
            gravity: gravity.then_some(self.mv.gravity as i32 as f32),
            stick_to_ground: false,
        };
        let mut hit = self.s.hit;
        let mut deflected = self.s.deflected;
        let out = self
            .physics
            .slide(&slide, self.mv.entity, &mut |entity, normal, contact| {
                deflected = true;
                if hit.is_none() && (normal[0] != 0.0 || normal[1] != 0.0) {
                    hit = Some(ActorPhysicsHit {
                        entity,
                        origin: contact,
                        normal,
                    });
                }
            })?;
        self.s.hit = hit;
        self.s.deflected = deflected;
        self.s.origin = out.origin;
        self.s.velocity = out.velocity;
        Some(out.contacted)
    }
}
