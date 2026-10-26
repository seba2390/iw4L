//! An actor's physics move by the game's own actor physics: where its step
//! through the world ends, what it stands on and what it met.

use crate::Rule;
use crate::collision::SolidSource;

/// How the move treats the actor, by its animation mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorPhysicsKind {
    /// On the ground it walks `delta` along it, stepping up and down; off
    /// it, it falls with the velocity it has.
    Ground,
    /// `delta` slid through the world as it is, with no gravity and no
    /// ground.
    Slide,
    /// `delta` across, gravity and the ground setting the height.
    Across,
}

/// One actor move.
#[derive(Clone, Copy, Debug)]
pub struct ActorPhysicsMove {
    pub origin: [f32; 3],
    /// The velocity the actor's last move left it with.
    pub velocity: [f32; 3],
    /// The step this frame asks for.
    pub delta: [f32; 3],
    pub kind: ActorPhysicsKind,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    /// The actor's clipmask.
    pub mask: u32,
    /// The actor's own entity number, which its sweeps pass through.
    pub entity: i32,
    /// What the actor stood on after its last move.
    pub ground_entity: Option<i32>,
    pub gravity: f32,
    pub seconds: f32,
    /// The actor is prone.
    pub prone: bool,
}

/// What a move hit first beside the ground, and where.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActorPhysicsHit {
    pub entity: i32,
    pub origin: [f32; 3],
    pub normal: [f32; 3],
}

/// Where the move ended.
#[derive(Clone, Copy, Debug)]
pub struct ActorPhysicsResult {
    pub origin: [f32; 3],
    pub velocity: [f32; 3],
    /// What the actor stands on.
    pub ground_entity: Option<i32>,
    /// The plane the actor walks on; `None` while it does not.
    pub walking: Option<[f32; 3]>,
    pub hit: Option<ActorPhysicsHit>,
    /// The move met something on its way.
    pub deflected: bool,
}

/// The match around a moving actor.
pub trait ActorWorld: SolidSource {
    /// Whether the entity is moving (a mover on its way).
    fn moving(&self, entity: i32) -> bool;
    /// A dvar's value, as the match keeps it.
    fn dvar(&self, name: &str) -> Option<&str>;
}

/// How a game moves its actors through the world.
pub trait ActorPhysics: Sync {
    /// Runs one move; `Known(None)` when the game moves this one by the
    /// runtime's trace slide.
    fn physics(
        &self,
        world: &dyn ActorWorld,
        actor_move: &ActorPhysicsMove,
    ) -> Rule<Option<ActorPhysicsResult>>;
}

#[derive(Clone, Copy)]
pub struct ActorPhysicsRules(pub &'static dyn ActorPhysics);

impl core::fmt::Debug for ActorPhysicsRules {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ActorPhysicsRules")
    }
}
