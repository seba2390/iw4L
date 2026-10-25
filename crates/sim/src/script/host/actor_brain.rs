//! Black Ops' actor AI, as its server runs the zombie and zombie dog
//! species: a stack of AI states (exposed at the bottom; pain, scripted and
//! custom animation and negotiation pushed above it; death set at the
//! bottom), the transitions damage and scripts queue on that stack, and the
//! animscript each state keeps running.
//!
//! Each think first applies the queued transitions, then runs the top
//! state's think, again while it reports that the state changed. A think
//! keeps its animscript with `set_anim_script`, which only refreshes the
//! movement modes while that script runs and otherwise ends the old one
//! (`killanimscript`) and runs the new one at once.
//!
//! The path planning, path following and turning these states drive stay
//! the runtime's own (`actor_nav`): a state asks whether the actor has a
//! path, clears it, and picks the movement and facing modes.

use bevy_ecs::prelude::World;

use super::actor_nav::{AnimMode, Orient};
use crate::script::runtime::{notify_now, raise, run_now, start_function_now, start_now};
use crate::script::{RoundScript, Value};

/// An AI state, numbered as Black Ops' state table numbers its rows.
pub(crate) type AiState = u8;
pub(crate) const EXPOSED: AiState = 1;
pub(crate) const DEATH: AiState = 6;
pub(crate) const PAIN: AiState = 7;
pub(crate) const ANIM_SCRIPTED: AiState = 9;
pub(crate) const ANIM_CUSTOM: AiState = 10;
pub(crate) const NEGOTIATION: AiState = 11;

/// The levels of the state stack.
const STACK_LEVELS: usize = 6;

/// Exposed's substates: fighting an enemy, and having none.
const EXPOSED_COMBAT: i32 = 100;
const EXPOSED_NONCOMBAT: i32 = 101;
/// Death's substates: the death animscript plays; the actor was let go of.
const DEATH_PLAYING: i32 = 200;
const DEATH_RELEASED: i32 = 201;

/// What a state think reports.
const THINK_DONE: u8 = 0;
const THINK_CHANGED: u8 = 1;
/// The death animscript ended: the actor is done.
const THINK_FINISHED: u8 = 2;

/// Black Ops loops a think while it reports a change, with no bound; this
/// one stops a state cycle from hanging the server.
const THINK_LIMIT: usize = 16;

/// The death think lets go of the actor this long after dying (ms).
const DEATH_RELEASE_MS: i64 = 800;
/// An enemy seen this recently is still fought (ms).
const ENEMY_MEMORY_MS: i64 = 10_000;
/// How old a sight check of the enemy (and `cansee`'s, by default) may be
/// (ms).
pub(crate) const SIGHT_MAX_AGE_MS: i64 = 250;
/// An actor's sight: `fovcosine` and `maxsightdistsqrd` unless set.
const FOV_COSINE: f32 = 0.01;
const MAX_SIGHT_DIST_SQ: f32 = 8192.0 * 8192.0;
/// A sentient's `maxvisibledist` unless set.
const MAX_VISIBLE_DIST: f32 = 8192.0;
/// The fog's limit on sight: Black Ops reads it from the fog with a format
/// its fog no longer matches, so it stays the largest float.
const FOG_SIGHT_DIST_SQ: f32 = f32::MAX;
/// `ai_foliageSeeThroughDist` unless set.
const FOLIAGE_SEE_THROUGH_DIST: f32 = 128.0;
/// What a sight trace stops at, with foliage and through it.
const SIGHT_MASK_FOLIAGE: u32 = 0x0280_d803;
const SIGHT_MASK: u32 = 0x0280_d801;
/// An actor with no `tag_eye` sees from this high over its origin.
const EYE_FALLBACK_HEIGHT: f32 = 64.0;
/// The stop-to-fight rule shrinks the goal by half its radius, at most this.
const FIGHT_GOAL_SHRINK: f32 = 64.0;
/// Far from its path's end by at least this, a non-combat actor runs.
const WALK_DIST: f32 = 128.0;

/// `movemode`'s values.
const MOVE_STOP: u8 = 0;
const MOVE_STOP_SOON: u8 = 1;
const MOVE_WALK: u8 = 2;
const MOVE_RUN: u8 = 3;
/// A blocked actor slower than this, squared, starts to stop.
const BLOCKED_STILL_SQ: f32 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Transition {
    /// Replaces the top state.
    Set,
    Push,
    Pop,
}

/// What the last two queued transitions become: `None` keeps both,
/// `Some(None)` drops both, `Some(Some(kind))` merges them into one of that
/// kind toward the later one's state.
fn collapse(prev: Transition, last: Transition) -> Option<Option<Transition>> {
    use Transition::{Pop, Push, Set};
    match (prev, last) {
        (Set, Set) | (Push, Set) | (Pop, Push) => Some(Some(Set)),
        (Set, Pop) => Some(Some(Pop)),
        (Push, Pop) => Some(None),
        _ => None,
    }
}

/// States on the stack above its bottom that refuse a push of `state`.
/// (Black Ops' table also names state 8, which these species lack.)
fn push_refused_by(state: AiState) -> &'static [AiState] {
    match state {
        PAIN => &[ANIM_SCRIPTED, NEGOTIATION],
        ANIM_CUSTOM => &[NEGOTIATION],
        NEGOTIATION => &[ANIM_SCRIPTED],
        _ => &[],
    }
}

/// States a push of `state` first pops, with every level above them.
fn push_pops(state: AiState) -> &'static [AiState] {
    match state {
        PAIN => &[PAIN, ANIM_CUSTOM],
        ANIM_SCRIPTED | ANIM_CUSTOM | NEGOTIATION => {
            &[PAIN, ANIM_SCRIPTED, ANIM_CUSTOM, NEGOTIATION]
        }
        _ => &[],
    }
}

/// A species animscript, as the species' script table lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SpeciesScript {
    Combat,
    Death,
    Pain,
    Move,
    Scripted,
    Stop,
}

impl SpeciesScript {
    fn module(self) -> &'static str {
        match self {
            Self::Combat => "combat",
            Self::Death => "death",
            Self::Pain => "pain",
            Self::Move => "move",
            Self::Scripted => "scripted",
            Self::Stop => "stop",
        }
    }
}

/// The animscript an actor runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AnimScript {
    Species(SpeciesScript),
    /// The script in the custom slot: `animcustom`'s function or a
    /// traversal's animscript.
    Custom,
}

/// What the custom slot holds.
#[derive(Clone, Debug)]
enum CustomScript {
    Function(usize),
    Traverse(String),
}

/// What an actor knows of seeing one target.
#[derive(Clone, Copy, Debug, Default)]
struct Sighting {
    visible: bool,
    checked: Option<i64>,
    seen: Option<i64>,
}

#[derive(Clone, Debug)]
pub(crate) struct ActorBrain {
    /// Animscript family, `zombie_` or `zombie_dog_`.
    prefix: &'static str,
    dog: bool,
    stack: [AiState; STACK_LEVELS],
    substates: [i32; STACK_LEVELS],
    depth: usize,
    /// When the bottom level last changed state.
    state_time: i64,
    queue: Vec<(Transition, AiState)>,
    /// The stack as it stands once the queue is applied.
    pending: [AiState; STACK_LEVELS],
    pending_depth: usize,
    pain_time: i64,
    alive: bool,
    script: Option<AnimScript>,
    /// The animscript's thread handle: kept after the thread ends, until the
    /// script is replaced; its serial while the thread lives.
    thread: Option<Option<u64>>,
    custom: Option<CustomScript>,
    /// `traversemode`: 1 gravity, 2 nogravity, 3 noclip.
    traverse_mode: i32,
    movemode: u8,
    /// The frame the senses last updated.
    sensed_at: Option<i64>,
    sightings: std::collections::BTreeMap<u64, Sighting>,
}

impl ActorBrain {
    fn top(&self) -> AiState {
        self.stack[self.depth]
    }

    fn substate(&self) -> i32 {
        self.substates[self.depth]
    }

    fn set_substate(&mut self, substate: i32) {
        self.substates[self.depth] = substate;
    }

    /// Queues a transition, collapsing it into the one before when Black
    /// Ops' table says so; a dead actor keeps only its first.
    fn queue(&mut self, kind: Transition, state: AiState) {
        self.queue.push((kind, state));
        if self.queue.len() < 2 {
            return;
        }
        while self.alive {
            let last = self.queue[self.queue.len() - 1];
            let prev = self.queue[self.queue.len() - 2];
            match collapse(prev.0, last.0) {
                None => return,
                Some(None) => {
                    self.queue.truncate(self.queue.len() - 2);
                }
                Some(Some(kind)) => {
                    let at = self.queue.len() - 2;
                    self.queue[at] = (kind, last.1);
                    self.queue.truncate(self.queue.len() - 1);
                }
            }
            if self.queue.len() < 2 {
                return;
            }
        }
        self.queue.truncate(1);
    }

    /// Queues a push of `state` unless a state on the stack refuses it,
    /// first popping what it replaces.
    fn push(&mut self, state: AiState) -> bool {
        let pending = |brain: &Self| brain.pending[1..=brain.pending_depth].to_vec();
        if pending(self)
            .iter()
            .any(|held| push_refused_by(state).contains(held))
        {
            return false;
        }
        if let Some(level) = pending(self)
            .iter()
            .position(|held| push_pops(state).contains(held))
            .map(|index| index + 1)
        {
            while level <= self.pending_depth {
                self.queue(Transition::Pop, 0);
                self.pending_depth -= 1;
            }
        }
        // Black Ops does not bound the stack; nothing here pushes it that
        // deep, and a push past its end is refused.
        if self.pending_depth + 1 >= STACK_LEVELS {
            return false;
        }
        self.queue(Transition::Push, state);
        self.pending_depth += 1;
        self.pending[self.pending_depth] = state;
        true
    }

    fn pop(&mut self) {
        self.queue(Transition::Pop, 0);
        self.pending_depth = self.pending_depth.saturating_sub(1);
    }

    fn pending_top(&self) -> AiState {
        self.pending[self.pending_depth]
    }
}

fn brain(world: &World, actor: u64) -> Option<&ActorBrain> {
    world.resource::<RoundScript>().actor_brains.get(&actor)
}

fn with_brain<T>(
    world: &mut World,
    actor: u64,
    apply: impl FnOnce(&mut ActorBrain) -> T,
) -> Option<T> {
    world
        .resource_mut::<RoundScript>()
        .actor_brains
        .get_mut(&actor)
        .map(apply)
}

fn with_movement(
    world: &mut World,
    actor: u64,
    apply: impl FnOnce(&mut super::actor_nav::ActorMove),
) {
    if let Some(movement) = world
        .resource_mut::<RoundScript>()
        .actor_moves
        .get_mut(&actor)
    {
        apply(movement);
    }
}

/// The actor's animscript family prefix.
pub(crate) fn prefix(world: &World, actor: u64) -> &'static str {
    brain(world, actor).map_or("zombie_", |brain| brain.prefix)
}

/// Whether the actor still lives, as its AI knows it.
pub(crate) fn alive(world: &World, actor: u64) -> bool {
    brain(world, actor).is_some_and(|brain| brain.alive)
}

/// Whether the actor's pending states hold a scripted animation or a
/// negotiation, which hold off a lethal hit while it may not die.
pub(crate) fn delays_death(runtime: &RoundScript, actor: u64) -> bool {
    runtime.actor_brains.get(&actor).is_some_and(|brain| {
        brain.pending[..=brain.pending_depth]
            .iter()
            .any(|state| matches!(*state, ANIM_SCRIPTED | NEGOTIATION))
    })
}

/// Runs the actor's animscript init and takes charge of it: exposed at the
/// bottom of its state stack, started by nothing (its substate is none
/// until its enemy is first missing or found).
pub(crate) fn begin(world: &mut World, actor: u64) -> Result<(), String> {
    let (prefix, dog) = match world
        .resource_mut::<RoundScript>()
        .object_field(actor, "type")
    {
        Value::String(kind) if kind.as_bytes().starts_with(b"zombie_dog") => ("zombie_dog_", true),
        // Black Ops' human species runs the singleplayer animscripts.
        Value::String(kind) if &*kind == "human" => ("", false),
        _ => ("zombie_", false),
    };
    let now = super::players::now_ms(world);
    let init = format!("animscripts/{prefix}init::main");
    run_now(world, &init, Value::Object(actor), Vec::new(), now)
        .map_err(|fault| format!("{init}: {fault:?}"))?;
    let mut runtime = world.resource_mut::<RoundScript>();
    runtime.actor_brains.insert(
        actor,
        ActorBrain {
            prefix,
            dog,
            stack: [EXPOSED; STACK_LEVELS],
            substates: [0; STACK_LEVELS],
            depth: 0,
            state_time: now,
            queue: Vec::new(),
            pending: [EXPOSED; STACK_LEVELS],
            pending_depth: 0,
            pain_time: 0,
            alive: true,
            script: None,
            thread: None,
            custom: None,
            traverse_mode: 2,
            movemode: MOVE_STOP,
            sensed_at: None,
            sightings: Default::default(),
        },
    );
    let movement = runtime.actor_moves.entry(actor).or_default();
    if dog {
        movement.allow_pitch = true;
    }
    Ok(())
}

/// Runs each actor's think: its queued transitions, then its top state's
/// think while that reports a change. An actor whose death animscript
/// ended is removed.
pub(crate) fn think(world: &mut World) {
    let actors: Vec<u64> = world
        .resource::<RoundScript>()
        .actor_brains
        .keys()
        .copied()
        .collect();
    super::actor_nav::install_clip(world);
    for actor in actors {
        super::actor_nav::clip_actor(world, actor);
        super::actor_nav::update_blocker(world, actor);
        let mut result = THINK_DONE;
        for round in 0..THINK_LIMIT {
            if brain(world, actor).is_none() {
                break;
            }
            result = think_once(world, actor);
            if result != THINK_CHANGED {
                break;
            }
            if round + 1 == THINK_LIMIT {
                diag::warn!(
                    Sim,
                    "actor {actor}: state changed {THINK_LIMIT} times in one think"
                );
            }
        }
        if result == THINK_FINISHED {
            finish_death(world, actor);
        } else if brain(world, actor).is_some() {
            super::actor_nav::goal_notify(world, actor);
        }
    }
    super::actor_nav::remove_clip(world);
}

fn think_once(world: &mut World, actor: u64) -> u8 {
    if brain(world, actor).is_some_and(|brain| !brain.queue.is_empty()) {
        apply_transitions(world, actor);
    }
    let Some((state, dog)) = brain(world, actor).map(|brain| (brain.top(), brain.dog)) else {
        return THINK_DONE;
    };
    match state {
        EXPOSED if dog => dog_exposed_think(world, actor),
        EXPOSED => exposed_think(world, actor),
        DEATH => death_think(world, actor),
        PAIN => pain_think(world, actor),
        ANIM_SCRIPTED => scripted_think(world, actor),
        ANIM_CUSTOM => custom_think(world, actor),
        NEGOTIATION => negotiation_think(world, actor),
        _ => THINK_DONE,
    }
}

/// Applies the queued transitions in order, then takes the stack as the
/// pending one.
fn apply_transitions(world: &mut World, actor: u64) {
    let queue =
        with_brain(world, actor, |brain| std::mem::take(&mut brain.queue)).unwrap_or_else(Vec::new);
    let now = super::players::now_ms(world);
    for (kind, state) in queue {
        let Some(top) = brain(world, actor).map(ActorBrain::top) else {
            return;
        };
        match kind {
            Transition::Set => {
                finish(world, actor, top);
                with_brain(world, actor, |brain| {
                    if brain.depth == 0 {
                        brain.state_time = now;
                    }
                    let depth = brain.depth;
                    brain.stack[depth] = state;
                });
                if !start(world, actor, state) {
                    with_brain(world, actor, |brain| {
                        if brain.depth == 0 {
                            brain.state_time = now;
                        }
                        let depth = brain.depth;
                        brain.stack[depth] = EXPOSED;
                    });
                    start(world, actor, EXPOSED);
                }
            }
            Transition::Push => {
                // A state is suspended as it is finished.
                finish(world, actor, top);
                with_brain(world, actor, |brain| {
                    brain.depth += 1;
                    let depth = brain.depth;
                    brain.stack[depth] = state;
                });
                if !start(world, actor, state) {
                    pop_to_resume(world, actor, now);
                }
            }
            Transition::Pop => {
                finish(world, actor, top);
                pop_to_resume(world, actor, now);
            }
        }
    }
    with_brain(world, actor, |brain| {
        brain.pending = brain.stack;
        brain.pending_depth = brain.depth;
    });
}

/// Pops levels until one resumes; past the bottom, exposed starts there.
fn pop_to_resume(world: &mut World, actor: u64, now: i64) {
    loop {
        let Some(depth) = brain(world, actor).map(|brain| brain.depth) else {
            return;
        };
        if depth == 0 {
            with_brain(world, actor, |brain| {
                brain.state_time = now;
                brain.stack[0] = EXPOSED;
            });
            start(world, actor, EXPOSED);
            return;
        }
        let state = with_brain(world, actor, |brain| {
            brain.depth -= 1;
            brain.top()
        })
        .unwrap_or(EXPOSED);
        if resume(world, actor, state) {
            return;
        }
        finish(world, actor, state);
    }
}

fn start(world: &mut World, actor: u64, state: AiState) -> bool {
    match state {
        // Exposed would also start its ground pitch (`allowpitchangle`),
        // which no actor here applies.
        EXPOSED => with_brain(world, actor, |brain| brain.set_substate(EXPOSED_COMBAT)).is_some(),
        DEATH => {
            with_brain(world, actor, |brain| brain.set_substate(DEATH_PLAYING));
            true
        }
        PAIN => pain_start(world, actor),
        ANIM_SCRIPTED => true,
        ANIM_CUSTOM => {
            let now = super::players::now_ms(world);
            notify_now(
                world,
                Value::Object(actor),
                "begin_custom_anim",
                Vec::new(),
                now,
            );
            true
        }
        NEGOTIATION => negotiation_start(world, actor),
        _ => false,
    }
}

fn finish(world: &mut World, actor: u64, state: AiState) {
    if state == ANIM_SCRIPTED {
        // Its scripted animation goes with it.
        with_movement(world, actor, |movement| movement.stop_scripted());
    }
}

/// Zombie exposed resumes as it stands; the dog's restarts, as every other
/// state does.
fn resume(world: &mut World, actor: u64, state: AiState) -> bool {
    let dog = brain(world, actor).is_some_and(|brain| brain.dog);
    if state == EXPOSED && !dog {
        return true;
    }
    start(world, actor, state)
}

// --- Animscripts --------------------------------------------------------

/// Keeps `script` running: while it runs, only the movement modes are
/// refreshed (`movemode` notified on a change); otherwise the old script is
/// ended and `script` runs at once.
fn set_anim_script(
    world: &mut World,
    actor: u64,
    script: AnimScript,
    movemode: u8,
    anim_mode: AnimMode,
) {
    let Some(running) = brain(world, actor).map(|brain| brain.script) else {
        return;
    };
    if running == Some(script) {
        with_movement(world, actor, |movement| {
            movement.anim_mode = movement.anim_mode_override.unwrap_or(anim_mode);
        });
        let changed = with_brain(world, actor, |brain| {
            let changed = brain.movemode != movemode;
            brain.movemode = movemode;
            changed
        })
        .unwrap_or(false);
        if changed {
            set_movemode_field(world, actor, movemode);
            let now = super::players::now_ms(world);
            notify_now(world, Value::Object(actor), "movemode", Vec::new(), now);
        }
        return;
    }
    let now = super::players::now_ms(world);
    kill_anim_script(world, actor, true);
    with_movement(world, actor, |movement| {
        movement.clear_script_orient();
        movement.anim_mode_override = None;
        movement.anim_mode = anim_mode;
    });
    {
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.set_object_field(actor, "safetochangescript", Value::Int(1));
        runtime.set_object_field(actor, "keepclaimednode", Value::Int(0));
        runtime.set_object_field(actor, "keepclaimednodeingoal", Value::Int(0));
        runtime.set_object_field(actor, "requestarrivalnotify", Value::Int(0));
    }
    let custom = with_brain(world, actor, |brain| {
        brain.script = Some(script);
        brain.movemode = movemode;
        brain.custom.clone()
    })
    .flatten();
    set_movemode_field(world, actor, movemode);
    let prefix = prefix(world, actor);
    let started = match script {
        AnimScript::Species(species) => {
            let main = format!("animscripts/{prefix}{}::main", species.module());
            start_now(world, &main, Value::Object(actor), Vec::new(), now)
                .map_err(|fault| format!("{main}: {fault:?}"))
        }
        AnimScript::Custom => match custom {
            Some(CustomScript::Function(function)) => {
                start_function_now(world, function, Value::Object(actor), Vec::new(), now)
                    .map_err(|fault| format!("animcustom: {fault:?}"))
            }
            Some(CustomScript::Traverse(name)) => {
                let main = format!("animscripts/traverse/{name}::main");
                start_now(world, &main, Value::Object(actor), Vec::new(), now)
                    .map_err(|fault| format!("{main}: {fault:?}"))
            }
            None => Err("no custom animscript".into()),
        },
    };
    let thread = match started {
        Ok(serial) => Some(serial),
        Err(fault) => {
            diag::warn!(Sim, "actor {actor} animscript: {fault}");
            None
        }
    };
    with_brain(world, actor, |brain| brain.thread = thread);
}

/// Ends the actor's animscript: it is told `killanimscript` while it has a
/// thread handle. `now` delivers the notify at once (a think, outside any
/// script); otherwise it waits for the running script.
fn kill_anim_script(world: &mut World, actor: u64, now: bool) {
    let had_thread = with_brain(world, actor, |brain| {
        brain.script = None;
        brain.thread.take().is_some()
    })
    .unwrap_or(false);
    if !had_thread {
        return;
    }
    if now {
        let time = super::players::now_ms(world);
        notify_now(
            world,
            Value::Object(actor),
            "killanimscript",
            Vec::new(),
            time,
        );
    } else {
        raise(world, Value::Object(actor), "killanimscript", Vec::new());
    }
}

/// Whether the animscript's thread still runs.
fn script_running(world: &mut World, actor: u64) -> bool {
    match brain(world, actor).and_then(|brain| brain.thread) {
        Some(Some(serial)) => crate::script::runtime::thread_alive(world, serial),
        _ => false,
    }
}

fn set_movemode_field(world: &mut World, actor: u64, movemode: u8) {
    let name = match movemode {
        MOVE_STOP => "stop",
        1 => "stop_soon",
        MOVE_WALK => "walk",
        _ => "run",
    };
    world
        .resource_mut::<RoundScript>()
        .set_object_field(actor, "movemode", Value::string(name));
}

fn set_code_orient(world: &mut World, actor: u64, orient: Orient) {
    with_movement(world, actor, |movement| movement.code_orient = orient);
}

fn has_path(world: &World, actor: u64) -> bool {
    world
        .resource::<RoundScript>()
        .actor_moves
        .get(&actor)
        .is_some_and(super::actor_nav::ActorMove::has_path)
}

fn clear_path(world: &mut World, actor: u64) {
    super::actor_nav::clear_path(world, actor);
}

/// The actor's movemode (0 stop, 1 stop soon, 2 walk, 3 run).
pub(crate) fn movemode(world: &World, actor: u64) -> u8 {
    brain(world, actor).map_or(MOVE_STOP, |brain| brain.movemode)
}

/// Whether the actor is a hellhound.
pub(crate) fn is_dog(world: &World, actor: u64) -> bool {
    brain(world, actor).is_some_and(|brain| brain.dog)
}

/// Where the actor's enemy stands, when the actor sees it.
pub(crate) fn seen_enemy(world: &mut World, actor: u64) -> Option<[f32; 3]> {
    let enemy = enemy(world, actor)?;
    if !can_see_enemy(world, actor) {
        return None;
    }
    match super::players::entity_field(world, enemy, "origin") {
        Value::Vector(at) => Some(at),
        _ => None,
    }
}

fn safe_to_change_script(world: &mut World, actor: u64) -> bool {
    !matches!(
        world
            .resource_mut::<RoundScript>()
            .object_field(actor, "safetochangescript"),
        Value::Int(0)
    )
}

/// The stop animscript, walking mode; it looks for a blocker first, and
/// with one starts as a blocked move does.
fn start_stop(world: &mut World, actor: u64) {
    if super::actor_nav::acquire_blocker(world, actor) {
        start_blocked(world, actor);
        return;
    }
    set_anim_script(
        world,
        actor,
        AnimScript::Species(SpeciesScript::Stop),
        MOVE_STOP,
        AnimMode::Normal,
    );
}

/// The move animscript along the path, running or walking; the stop
/// animscript with no path (or with `stop_soon` right before the path's
/// end). An actor not yet moving starts from stop and the normal movement
/// mode.
fn start_move(world: &mut World, actor: u64, run: bool) {
    let zonly = world
        .resource::<RoundScript>()
        .actor_moves
        .get(&actor)
        .is_some_and(|movement| movement.anim_mode == AnimMode::ZOnlyPhysics);
    if !zonly {
        let moving = brain(world, actor).is_some_and(|brain| brain.movemode != MOVE_STOP)
            && world
                .resource::<RoundScript>()
                .actor_moves
                .get(&actor)
                .is_some_and(|movement| movement.anim_mode == AnimMode::Normal);
        if !moving {
            with_brain(world, actor, |brain| brain.movemode = MOVE_STOP);
            with_movement(world, actor, |movement| {
                movement.anim_mode = AnimMode::Normal
            });
        }
    }
    if !has_path(world, actor) {
        start_stop(world, actor);
        return;
    }
    if super::actor_nav::has_blocker(world, actor) {
        start_blocked(world, actor);
        return;
    }
    // Running, the path's end ease about to end stops it.
    let stop_soon = run && {
        let now = super::players::now_ms(world);
        super::actor_nav::ends_soon(world, actor, now)
    };
    if stop_soon {
        set_anim_script(
            world,
            actor,
            AnimScript::Species(SpeciesScript::Stop),
            MOVE_STOP_SOON,
            AnimMode::Normal,
        );
        return;
    }
    let movemode = if run { MOVE_RUN } else { MOVE_WALK };
    set_anim_script(
        world,
        actor,
        AnimScript::Species(SpeciesScript::Move),
        movemode,
        AnimMode::Normal,
    );
}

/// The combat animscript, gravity movement; it looks for a blocker first,
/// and with one moves normally (stepping away from it).
fn start_combat(world: &mut World, actor: u64) {
    let anim_mode = if super::actor_nav::acquire_blocker(world, actor) {
        AnimMode::Normal
    } else {
        AnimMode::Gravity
    };
    set_anim_script(
        world,
        actor,
        AnimScript::Species(SpeciesScript::Combat),
        MOVE_STOP,
        anim_mode,
    );
}

/// A move start while holding a blocker: nearly still, the stop animscript
/// about to stop; else the move animscript, walking.
fn start_blocked(world: &mut World, actor: u64) {
    let speed_sq = world
        .resource::<RoundScript>()
        .actor_moves
        .get(&actor)
        .map_or(0.0, |movement| {
            movement.velocity[0] * movement.velocity[0]
                + movement.velocity[1] * movement.velocity[1]
        });
    let (script, movemode) = if speed_sq < BLOCKED_STILL_SQ {
        (SpeciesScript::Stop, MOVE_STOP_SOON)
    } else {
        (SpeciesScript::Move, MOVE_WALK)
    };
    set_anim_script(
        world,
        actor,
        AnimScript::Species(script),
        movemode,
        AnimMode::Normal,
    );
}

// --- Senses -------------------------------------------------------------

/// The once-a-frame senses: the enemy, and whether it is seen.
fn sense(world: &mut World, actor: u64) {
    let now = super::players::now_ms(world);
    let fresh = with_brain(world, actor, |brain| {
        let fresh = brain.sensed_at != Some(now);
        brain.sensed_at = Some(now);
        fresh
    })
    .unwrap_or(false);
    if !fresh {
        return;
    }
    super::actor_nav::choose_enemy(world, actor);
    let _ = can_see_enemy(world, actor);
}

fn enemy(world: &mut World, actor: u64) -> Option<u64> {
    match world
        .resource_mut::<RoundScript>()
        .object_field(actor, "enemy")
    {
        Value::Object(enemy) => Some(enemy),
        _ => None,
    }
}

/// Whether the actor sees its enemy: a sentient by a sight check reused for
/// a quarter second, any other entity by a fresh one.
fn can_see_enemy(world: &mut World, actor: u64) -> bool {
    let Some(enemy) = enemy(world, actor) else {
        return false;
    };
    if is_sentient(world, enemy) {
        return can_see_sentient(world, actor, enemy, SIGHT_MAX_AGE_MS);
    }
    let (fov, max_dist_sq) = sight_limits(world, actor);
    sight(world, actor, enemy, fov, max_dist_sq)
}

/// A player's or an actor's.
pub(crate) fn is_sentient(world: &World, object: u64) -> bool {
    let runtime = world.resource::<RoundScript>();
    runtime.player_client(object).is_some()
        || runtime
            .entities
            .get(&object)
            .is_some_and(|entity| entity.kind == super::entities::EntityKind::Actor)
}

/// The actor's own field of view and sight distance (`fovcosine`,
/// `maxsightdistsqrd`).
pub(crate) fn sight_limits(world: &mut World, actor: u64) -> (f32, f32) {
    let mut runtime = world.resource_mut::<RoundScript>();
    (
        float_field(&mut runtime, actor, "fovcosine").unwrap_or(FOV_COSINE),
        float_field(&mut runtime, actor, "maxsightdistsqrd").unwrap_or(MAX_SIGHT_DIST_SQ),
    )
}

/// Whether the actor sees a sentient, by the last sight check when it is no
/// older than `max_age` (ms), else by a new one with its own sight.
pub(crate) fn can_see_sentient(world: &mut World, actor: u64, target: u64, max_age: i64) -> bool {
    let now = super::players::now_ms(world);
    let cached = brain(world, actor)
        .and_then(|brain| brain.sightings.get(&target).copied())
        .filter(|sighting| sighting.checked.is_some_and(|at| at + max_age >= now));
    if let Some(sighting) = cached {
        return sighting.visible;
    }
    let (fov, max_dist_sq) = sight_limits(world, actor);
    sight(world, actor, target, fov, max_dist_sq)
}

/// Black Ops' sight check of a target within a field of view (a cosine; 0
/// for none) and a squared distance (0 for any): its eye, else a
/// non-sentient's `tag_eye` or bounds centre, must be in view and a sight
/// trace from the actor's eye reach it. An enemy or favorite enemy is seen
/// all around, and a sentient no farther than its `maxvisibledist`. A
/// sentient's result is remembered when the check was no wider or longer
/// than the actor's own sight for a sighting, no narrower or shorter for a
/// miss; an actor seen is told whether it sees back, by its own view.
pub(crate) fn sight(
    world: &mut World,
    actor: u64,
    target: u64,
    fov: f32,
    max_dist_sq: f32,
) -> bool {
    let (eye, eye_dir) = eye_view(world, actor);
    if !is_sentient(world, target) {
        let at = super::presence::tag_world(world, target, "tag_eye")
            .map_or_else(|| bounds_centre(world, target), |(at, _)| at);
        return in_view(eye, eye_dir, at, fov, max_dist_sq)
            && sight_trace(world, actor, eye, at, target);
    }
    let Some(at) = super::natives::t5_turret::sentient_view(world, target).map(|view| view.eye)
    else {
        return false;
    };
    let favorite = match world
        .resource_mut::<RoundScript>()
        .object_field(actor, "favoriteenemy")
    {
        Value::Object(favorite) => Some(favorite),
        _ => None,
    };
    let view_fov = if enemy(world, actor) == Some(target) || favorite == Some(target) {
        0.0
    } else {
        fov
    };
    let mut max = max_dist_sq;
    let visible_dist = max_visible_dist(world, target);
    let visible_dist_sq = visible_dist * visible_dist;
    if !(max - visible_dist_sq < 0.0) {
        max = visible_dist_sq;
    }
    let visible =
        in_view(eye, eye_dir, at, view_fov, max) && sight_trace(world, actor, eye, at, target);
    let (own_fov, own_max) = sight_limits(world, actor);
    let remembered = if visible {
        !(fov < own_fov) && !(own_max < max)
    } else {
        !(own_fov < fov) && !(max < own_max)
    };
    if remembered {
        remember(world, actor, target, visible);
    }
    if !visible {
        return false;
    }
    let target_is_actor = world
        .resource::<RoundScript>()
        .entities
        .get(&target)
        .is_some_and(|entity| entity.kind == super::entities::EntityKind::Actor);
    if target_is_actor && brain(world, target).is_some() {
        let (target_fov, target_max) = sight_limits(world, target);
        let mine = max_visible_dist(world, actor);
        let mine_sq = mine * mine;
        let back_max = if !(target_max - mine_sq < 0.0) {
            mine_sq
        } else {
            target_max
        };
        let (_, target_dir) = eye_view(world, target);
        let back = in_view(at, target_dir, eye, target_fov, back_max);
        remember(world, target, actor, back);
    }
    true
}

/// Records a sight check: when it was made, and when the target was last
/// seen; an actor that newly sees its enemy is told `enemy_visible`.
fn remember(world: &mut World, actor: u64, target: u64, visible: bool) {
    let now = super::players::now_ms(world);
    let Some(was) = with_brain(world, actor, |brain| {
        let sighting = brain.sightings.entry(target).or_default();
        let was = sighting.visible;
        sighting.visible = visible;
        sighting.checked = Some(now);
        if visible {
            sighting.seen = Some(now);
        }
        was
    }) else {
        return;
    };
    if visible && !was && enemy(world, actor) == Some(target) {
        raise(world, Value::Object(actor), "enemy_visible", Vec::new());
    }
}

/// A sentient's `maxvisibledist`.
fn max_visible_dist(world: &mut World, object: u64) -> f32 {
    float_field(
        &mut world.resource_mut::<RoundScript>(),
        object,
        "maxvisibledist",
    )
    .unwrap_or(MAX_VISIBLE_DIST)
}

/// The actor's eye and the way it looks: its `tag_eye` and that tag's
/// forward axis, level and normalized; else 64 units over its origin and
/// its facing.
pub(crate) fn eye_view(world: &mut World, actor: u64) -> ([f32; 3], [f32; 3]) {
    if let Some((at, axis)) = super::presence::tag_world(world, actor, "tag_eye") {
        let [x, y, _] = axis[0];
        let len = (x * x + y * y).sqrt();
        let len = if -len >= 0.0 { 1.0 } else { len };
        let inv = 1.0 / len;
        return (at, [x * inv, y * inv, 0.0]);
    }
    let mut runtime = world.resource_mut::<RoundScript>();
    let origin = vector_field(&mut runtime, actor, "origin").unwrap_or([0.0; 3]);
    let angles = vector_field(&mut runtime, actor, "angles").unwrap_or([0.0; 3]);
    let (forward, _, _) = math_iw4::angle_vectors([angles[0], angles[1], 0.0]);
    (
        [origin[0], origin[1], origin[2] + EYE_FALLBACK_HEIGHT],
        forward,
    )
}

/// The middle of an entity's bounds, turned with it.
fn bounds_centre(world: &mut World, object: u64) -> [f32; 3] {
    let (mins, maxs) = super::triggers::entity_bounds(world, object);
    let mut runtime = world.resource_mut::<RoundScript>();
    let origin = vector_field(&mut runtime, object, "origin").unwrap_or([0.0; 3]);
    let angles = vector_field(&mut runtime, object, "angles").unwrap_or([0.0; 3]);
    let axis = math_iw4::angles_to_axis(angles);
    let mid: [f32; 3] = std::array::from_fn(|i| (mins[i] + maxs[i]) * 0.5);
    std::array::from_fn(|i| {
        let turned = (axis[0][i] * mid[0] + axis[1][i] * mid[1]) + axis[2][i] * mid[2];
        origin[i] + turned
    })
}

/// Whether a point is within a squared distance (0 for any) of the eye and
/// within a field of view of the way it looks (0 for all around).
fn in_view(eye: [f32; 3], dir: [f32; 3], at: [f32; 3], fov: f32, max_dist_sq: f32) -> bool {
    let d = [at[0] - eye[0], at[1] - eye[1], at[2] - eye[2]];
    let dist_sq = (d[0] * d[0] + d[2] * d[2]) + d[1] * d[1];
    if max_dist_sq != 0.0 && (dist_sq > max_dist_sq || dist_sq > FOG_SIGHT_DIST_SQ) {
        return false;
    }
    if fov == 0.0 {
        return true;
    }
    let dot = (dir[0] * d[0] + dir[2] * d[2]) + dir[1] * d[1];
    if 0.0 > dot {
        return false;
    }
    !(dist_sq * fov * fov > dot * dot)
}

/// A sight trace from the eye to the point, through the actor and the
/// target: foliage is seen through within `ai_foliageSeeThroughDist`
/// (an actor that `ignoreclosefoliage` sees through it on that stretch of
/// the line only).
fn sight_trace(world: &mut World, actor: u64, eye: [f32; 3], at: [f32; 3], target: u64) -> bool {
    let through = world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get("ai_foliageSeeThroughDist")
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(FOLIAGE_SEE_THROUGH_DIST);
    let close = matches!(
        world
            .resource_mut::<RoundScript>()
            .object_field(actor, "ignoreclosefoliage"),
        Value::Int(n) if n != 0
    );
    let tick = world.resource::<crate::step::StepRequest>().tick;
    let mut frame = crate::frame::FrameWorld::from_world(world);
    let pass = [actor, target];
    if close {
        let d = [at[0] - eye[0], at[1] - eye[1], at[2] - eye[2]];
        let len = ((d[2] * d[2] + d[0] * d[0]) + d[1] * d[1]).sqrt();
        let len = if -len >= 0.0 { 1.0 } else { len };
        let inv = 1.0 / len;
        let mid: [f32; 3] = std::array::from_fn(|i| through * (inv * d[i]) + eye[i]);
        crate::game_fire::sight_clear(&mut frame, tick, mid, at, pass, SIGHT_MASK_FOLIAGE)
            && crate::game_fire::sight_clear(&mut frame, tick, eye, mid, pass, SIGHT_MASK)
    } else {
        let d = [at[0] - eye[0], at[1] - eye[1], at[2] - eye[2]];
        let dist_sq = (d[0] * d[0] + d[2] * d[2]) + d[1] * d[1];
        let mask = if through * through > dist_sq {
            SIGHT_MASK
        } else {
            SIGHT_MASK_FOLIAGE
        };
        crate::game_fire::sight_clear(&mut frame, tick, eye, at, pass, mask)
    }
}

/// Whether the enemy was seen within the last ten seconds.
fn enemy_known(world: &mut World, actor: u64) -> bool {
    let Some(enemy) = enemy(world, actor) else {
        return false;
    };
    let now = super::players::now_ms(world);
    brain(world, actor)
        .and_then(|brain| brain.sightings.get(&enemy))
        .and_then(|sighting| sighting.seen)
        .is_some_and(|seen| now - seen < ENEMY_MEMORY_MS)
}

// --- Exposed ------------------------------------------------------------

/// Exposed's substate: none without an enemy, fighting once a missing
/// enemy is known again.
fn pick_substate(world: &mut World, actor: u64) {
    if enemy(world, actor).is_none() {
        with_brain(world, actor, |brain| brain.set_substate(EXPOSED_NONCOMBAT));
        return;
    }
    if brain(world, actor).is_some_and(|brain| brain.substate() == EXPOSED_NONCOMBAT)
        && enemy_known(world, actor)
    {
        with_brain(world, actor, |brain| brain.set_substate(EXPOSED_COMBAT));
    }
}

/// The zombie's exposed think.
fn exposed_think(world: &mut World, actor: u64) -> u8 {
    sense(world, actor);
    pick_substate(world, actor);
    match brain(world, actor).map(ActorBrain::substate) {
        Some(EXPOSED_COMBAT) => {
            super::actor_nav::update_path(world, actor);
            if has_path(world, actor) && can_see_enemy(world, actor) && stop_to_fight(world, actor)
            {
                clear_path(world, actor);
            }
            if has_path(world, actor) && safe_to_change_script(world, actor) {
                set_code_orient(world, actor, Orient::EnemyOrMotion);
                start_move(world, actor, true);
            } else {
                start_combat(world, actor);
                set_code_orient(world, actor, Orient::Enemy);
            }
        }
        Some(EXPOSED_NONCOMBAT) => noncombat(world, actor),
        _ => {}
    }
    begin_negotiation(world, actor);
    THINK_DONE
}

/// Exposed with no enemy known: along a path it runs, walking within
/// `walkdist` of the path's end, facing its motion (its enemy or motion
/// once at its goal); without one it fights an enemy or stands, facing it.
fn noncombat(world: &mut World, actor: u64) {
    super::actor_nav::update_path(world, actor);
    if has_path(world, actor) {
        let run = {
            let mut runtime = world.resource_mut::<RoundScript>();
            let origin = vector_field(&mut runtime, actor, "origin");
            let walk = float_field(&mut runtime, actor, "walkdist").unwrap_or(WALK_DIST);
            let end = runtime
                .actor_moves
                .get(&actor)
                .and_then(super::actor_nav::ActorMove::path_end);
            match (origin, end) {
                (Some(origin), Some(end)) => {
                    let d = [origin[2] - end[2], origin[0] - end[0], origin[1] - end[1]];
                    d[0] * d[0] + d[1] * d[1] + d[2] * d[2] >= walk * walk
                }
                _ => true,
            }
        };
        start_move(world, actor, run);
        let orient = if in_goal(world, actor) {
            Orient::EnemyOrMotion
        } else {
            Orient::Motion
        };
        set_code_orient(world, actor, orient);
        return;
    }
    if enemy(world, actor).is_some() {
        start_combat(world, actor);
    } else {
        start_stop(world, actor);
    }
    let goal_yaw = if in_goal(world, actor) {
        world
            .resource::<RoundScript>()
            .actor_moves
            .get(&actor)
            .and_then(|movement| movement.goal_yaw)
    } else {
        None
    };
    if let Some(yaw) = goal_yaw {
        with_movement(world, actor, |movement| {
            movement.orient = Some(Orient::Angle(yaw));
        });
    } else {
        set_code_orient(world, actor, Orient::Enemy);
    }
}

/// Whether the actor stands at its goal, and its path (if any) ends there.
fn in_goal(world: &mut World, actor: u64) -> bool {
    let Some(goal) = super::actor_nav::goal_point(world, actor) else {
        return false;
    };
    let mut runtime = world.resource_mut::<RoundScript>();
    let (Some(origin), Some(radius), Some(height)) = (
        vector_field(&mut runtime, actor, "origin"),
        float_field(&mut runtime, actor, "goalradius"),
        float_field(&mut runtime, actor, "goalheight"),
    ) else {
        return false;
    };
    if !at_goal(origin, goal, radius, height, 0.0) {
        return false;
    }
    match runtime
        .actor_moves
        .get(&actor)
        .and_then(super::actor_nav::ActorMove::path_end)
    {
        Some(end) => at_goal(end, goal, radius, height, 0.0),
        None => true,
    }
}

/// Whether a path toward the goal should stop for the actor to fight: the
/// path ends at the goal and the actor stands within the goal shrunk by
/// half its radius (64 at most).
fn stop_to_fight(world: &mut World, actor: u64) -> bool {
    let Some((origin, path_end, goal, radius, height)) = goal_shape(world, actor) else {
        return false;
    };
    if !at_goal(path_end, goal, radius, height, 0.0) {
        return false;
    }
    let half = radius * 0.5;
    let shrink = if half - FIGHT_GOAL_SHRINK >= 0.0 {
        FIGHT_GOAL_SHRINK
    } else {
        half
    };
    at_goal(origin, goal, radius, height, -shrink)
}

/// Black Ops' goal test: within the height, then within the radius (grown
/// by `pad`) on the ground plane.
fn at_goal(point: [f32; 3], goal: [f32; 3], radius: f32, height: f32, pad: f32) -> bool {
    let dz = point[2] - goal[2];
    if dz * dz > height * height {
        return false;
    }
    let dy = goal[1] - point[1];
    let dx = goal[0] - point[0];
    let reach = radius + pad;
    dy * dy + dx * dx <= reach * reach
}

fn goal_shape(world: &mut World, actor: u64) -> Option<([f32; 3], [f32; 3], [f32; 3], f32, f32)> {
    let goal = super::actor_nav::goal_point(world, actor)?;
    let mut runtime = world.resource_mut::<RoundScript>();
    let origin = vector_field(&mut runtime, actor, "origin")?;
    let path_end = runtime
        .actor_moves
        .get(&actor)
        .and_then(super::actor_nav::ActorMove::path_end)?;
    let radius = float_field(&mut runtime, actor, "goalradius")?;
    let height = float_field(&mut runtime, actor, "goalheight")?;
    Some((origin, path_end, goal, radius, height))
}

/// The hellhound's exposed think: it bites (the combat animscript) once
/// its attack spot is reached, or while its bite may not be cut off, and
/// otherwise runs its path or stands.
fn dog_exposed_think(world: &mut World, actor: u64) -> u8 {
    sense(world, actor);
    let combat_running = brain(world, actor)
        .is_some_and(|brain| brain.script == Some(AnimScript::Species(SpeciesScript::Combat)))
        && script_running(world, actor);
    let locked = combat_running && !safe_to_change_script(world, actor);
    let bite = if locked {
        false
    } else {
        super::actor_dog::think(world, actor)
    };
    if locked || bite {
        if enemy(world, actor).is_some() {
            let combat_ended = brain(world, actor).is_some_and(|brain| {
                brain.script == Some(AnimScript::Species(SpeciesScript::Combat))
            }) && !script_running(world, actor);
            if combat_ended {
                kill_anim_script(world, actor, true);
            }
            set_anim_script(
                world,
                actor,
                AnimScript::Species(SpeciesScript::Combat),
                MOVE_STOP,
                AnimMode::Normal,
            );
        }
    } else if has_path(world, actor) {
        set_code_orient(world, actor, Orient::Motion);
        start_move(world, actor, true);
    } else {
        start_stop(world, actor);
    }
    begin_negotiation(world, actor);
    THINK_DONE
}

// --- Negotiation --------------------------------------------------------

/// The path reached a traversal: negotiation is pushed with its animscript.
fn begin_negotiation(world: &mut World, actor: u64) {
    let script = world
        .resource::<RoundScript>()
        .actor_moves
        .get(&actor)
        .and_then(|movement| {
            movement
                .traversal
                .as_ref()
                .map(|traversal| traversal.script.clone())
        });
    let Some(script) = script else {
        return;
    };
    if brain(world, actor).is_some_and(|brain| brain.pending_top() == NEGOTIATION) {
        return;
    }
    with_brain(world, actor, |brain| {
        if brain.push(NEGOTIATION) {
            brain.custom = Some(CustomScript::Traverse(script));
        }
    });
}

fn negotiation_start(world: &mut World, actor: u64) -> bool {
    let traversing = world
        .resource::<RoundScript>()
        .actor_moves
        .get(&actor)
        .is_some_and(|movement| movement.traversal.is_some());
    if !traversing {
        return false;
    }
    set_code_orient(world, actor, Orient::Current);
    with_brain(world, actor, |brain| brain.traverse_mode = 2);
    true
}

fn negotiation_think(world: &mut World, actor: u64) -> u8 {
    let mode = match brain(world, actor).map(|brain| brain.traverse_mode) {
        Some(1) => AnimMode::Gravity,
        Some(3) => AnimMode::NoClip,
        _ => AnimMode::NoGravity,
    };
    let mode = override_or(world, actor, mode);
    set_anim_script(world, actor, AnimScript::Custom, MOVE_STOP, mode);
    if !script_running(world, actor) {
        let traversal = world
            .resource::<RoundScript>()
            .actor_moves
            .get(&actor)
            .and_then(|movement| movement.traversal.clone());
        if let Some(traversal) = traversal
            && let Value::Vector(at) = world
                .resource_mut::<RoundScript>()
                .object_field(actor, "origin")
        {
            diag::info!(
                Sim,
                "actor {actor}: traversed {} ({}->{}) to ({:.0} {:.0} {:.0})",
                traversal.script,
                traversal.begin,
                traversal.end,
                at[0],
                at[1],
                at[2]
            );
        }
        super::actor_nav::finish_traversal(world, actor);
        with_brain(world, actor, ActorBrain::pop);
        return THINK_CHANGED;
    }
    sense(world, actor);
    THINK_DONE
}

/// The script's `animmode` when one is set, else `mode`.
fn override_or(world: &World, actor: u64, mode: AnimMode) -> AnimMode {
    world
        .resource::<RoundScript>()
        .actor_moves
        .get(&actor)
        .and_then(|movement| movement.anim_mode_override)
        .unwrap_or(mode)
}

// --- Pain ---------------------------------------------------------------

/// Black Ops' actor pain: the hit is recorded on the actor, and an actor
/// allowed pain (and not held in a synced melee, which nothing here holds)
/// has its animscript cut off for the pain state.
#[allow(clippy::too_many_arguments)]
pub(crate) fn pain(
    world: &mut World,
    actor: u64,
    attacker: &Value,
    damage: i32,
    mod_name: &Value,
    dir: &Value,
    location: &Value,
    weapon: &Value,
) {
    {
        let mut runtime = world.resource_mut::<RoundScript>();
        let yaw = damage_yaw(&mut runtime, actor, dir);
        runtime.set_object_field(actor, "damagetaken", Value::Int(damage));
        runtime.set_object_field(actor, "damageyaw", Value::Int(yaw));
        runtime.set_object_field(actor, "damagedir", dir.clone());
        runtime.set_object_field(actor, "damagelocation", location.clone());
        runtime.set_object_field(actor, "damagemod", mod_name.clone());
        if *attacker != Value::Undefined {
            runtime.set_object_field(actor, "damageweapon", weapon.clone());
        }
        runtime.set_object_field(actor, "attacker", attacker.clone());
    }
    let allowed = !matches!(
        world
            .resource_mut::<RoundScript>()
            .object_field(actor, "allowpain"),
        Value::Int(0)
    );
    if allowed && with_brain(world, actor, |brain| brain.push(PAIN)).unwrap_or(false) {
        kill_anim_script(world, actor, false);
    }
}

fn pain_start(world: &mut World, actor: u64) -> bool {
    let now = super::players::now_ms(world);
    with_brain(world, actor, |brain| brain.pain_time = now);
    clear_path(world, actor);
    set_anim_script(
        world,
        actor,
        AnimScript::Species(SpeciesScript::Pain),
        MOVE_STOP,
        AnimMode::Gravity,
    );
    notify_now(world, Value::Object(actor), "pain", Vec::new(), now);
    true
}

fn pain_think(world: &mut World, actor: u64) -> u8 {
    let now = super::players::now_ms(world);
    let pain_time = brain(world, actor).map_or(now, |brain| brain.pain_time);
    if now > pain_time && !script_running(world, actor) {
        with_brain(world, actor, ActorBrain::pop);
        return THINK_CHANGED;
    }
    // Half a second in, Black Ops also forgets the pain's attacker and
    // tells the AI around it (not ported).
    sense(world, actor);
    set_code_orient(world, actor, Orient::Current);
    set_anim_script(
        world,
        actor,
        AnimScript::Species(SpeciesScript::Pain),
        MOVE_STOP,
        AnimMode::Gravity,
    );
    THINK_DONE
}

// --- Death --------------------------------------------------------------

/// Black Ops' actor death: the killing hit is recorded on the actor, every
/// state above the bottom finishes, death is set there, and the animscript
/// is cut off.
#[allow(clippy::too_many_arguments)]
pub(crate) fn die(
    world: &mut World,
    actor: u64,
    attacker: &Value,
    mod_name: &Value,
    weapon: &Value,
    dir: &Value,
    location: &Value,
) {
    {
        let mut runtime = world.resource_mut::<RoundScript>();
        let yaw = damage_yaw(&mut runtime, actor, dir);
        runtime.set_object_field(actor, "damagedir", dir.clone());
        runtime.set_object_field(actor, "damageyaw", Value::Int(yaw));
        runtime.set_object_field(actor, "damagelocation", location.clone());
        runtime.set_object_field(actor, "damagemod", mod_name.clone());
        runtime.set_object_field(actor, "damageweapon", weapon.clone());
    }
    let dying = brain(world, actor).is_some_and(|brain| brain.top() != DEATH);
    if !dying {
        return;
    }
    // Every level above the bottom finishes; death replaces the bottom.
    loop {
        let Some((depth, top)) = brain(world, actor).map(|brain| (brain.depth, brain.top())) else {
            return;
        };
        if depth == 0 {
            break;
        }
        finish(world, actor, top);
        with_brain(world, actor, |brain| brain.depth -= 1);
    }
    with_brain(world, actor, |brain| {
        brain.queue.clear();
        brain.pending_depth = 0;
        brain.queue(Transition::Set, DEATH);
        brain.pending[0] = DEATH;
        brain.alive = false;
    });
    world
        .resource_mut::<RoundScript>()
        .set_object_field(actor, "attacker", attacker.clone());
    super::actor_dog::release(&mut world.resource_mut::<RoundScript>(), actor);
    kill_anim_script(world, actor, false);
}

fn death_think(world: &mut World, actor: u64) -> u8 {
    let now = super::players::now_ms(world);
    let (substate, state_time, script) = match brain(world, actor) {
        Some(brain) => (brain.substate(), brain.state_time, brain.script),
        None => return THINK_DONE,
    };
    if substate == DEATH_PLAYING && now - state_time >= DEATH_RELEASE_MS {
        release_dead(world, actor);
    }
    if script == Some(AnimScript::Species(SpeciesScript::Death)) && !script_running(world, actor) {
        if brain(world, actor).is_some_and(|brain| brain.substate() == DEATH_PLAYING) {
            release_dead(world, actor);
        }
        return THINK_FINISHED;
    }
    set_code_orient(world, actor, Orient::Current);
    let mode = override_or(world, actor, AnimMode::Gravity);
    set_anim_script(
        world,
        actor,
        AnimScript::Species(SpeciesScript::Death),
        MOVE_STOP,
        mode,
    );
    THINK_DONE
}

/// Lets go of a dead actor (at `state_time + 500`): its path ends and
/// its contents move to `death_contents` (`0x0400_2000` default, or what
/// `setdeathcontents` set).
fn release_dead(world: &mut World, actor: u64) {
    clear_path(world, actor);
    with_brain(world, actor, |brain| brain.set_substate(DEATH_RELEASED));
    let mut runtime = world.resource_mut::<RoundScript>();
    let contents = runtime
        .t5
        .death_contents
        .get(&actor)
        .copied()
        .unwrap_or(0x0400_2000);
    runtime.set_object_field(actor, "contents", Value::Int(contents));
}

/// Converts a dead actor whose death animscript finished into an
/// `EntityKind::ActorCorpse`, bounded by `ai_corpseCount` (default `10`,
/// max `32`; evicts the corpse least visible to player 0).
fn finish_death(world: &mut World, actor: u64) {
    let corpse_limit = world
        .get_resource::<crate::script::MatchScript>()
        .and_then(|m| m.dvars.get("ai_corpseCount"))
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(10)
        .min(32);
    let viewer = crate::frame::FrameWorld::from_world(world)
        .player(crate::ClientId(0))
        .map(|ps| {
            let eye = [
                ps.origin[0],
                ps.origin[1],
                ps.origin[2] + ps.view_height_current,
            ];
            let fwd = math_iw4::angles_to_axis(ps.viewangles)[0];
            (eye, fwd)
        });
    let runtime = world.resource_mut::<RoundScript>().into_inner();
    runtime.actor_brains.remove(&actor);
    runtime.actor_moves.remove(&actor);
    let entities = &runtime.entities;
    runtime
        .t5
        .corpse_slots
        .retain(|id| entities.contains_key(id));
    let model = match runtime.object_field(actor, "model") {
        Value::String(s) if !s.is_empty() => Some(s),
        _ => None,
    };
    let hidden = runtime
        .entities
        .get(&actor)
        .is_none_or(|entity| entity.hidden);
    if corpse_limit > 0
        && !hidden
        && let Some(model) = model
    {
        while runtime.t5.corpse_slots.len() >= corpse_limit {
            let evicted = evict_corpse_slot(runtime, viewer);
            runtime.t5.corpse_slots.retain(|&id| id != evicted);
            runtime.request_delete(evicted);
        }
        let origin = runtime.object_field(actor, "origin");
        let angles = runtime.object_field(actor, "angles");
        let team = runtime.object_field(actor, "team");
        let (number, presence, attachments, attach_ignore_collision) =
            match runtime.entities.get_mut(&actor) {
                Some(entity) => (
                    entity.number,
                    entity.presence.take(),
                    entity.attachments.clone(),
                    entity.attach_ignore_collision,
                ),
                None => (0, None, Vec::new(), 0),
            };
        let shown = runtime.shown.remove(&actor);
        let anim = runtime.actor_anims.remove(&actor);
        let ragdoll = runtime.t5.ragdolls.remove(&actor);
        let client_flags = runtime.t5.client_flags.remove(&actor);
        let saved_next = runtime.next_entity_number;
        if let Ok(corpse) =
            runtime.create_entity(super::entities::EntityKind::ActorCorpse, "actor_corpse")
        {
            runtime.next_entity_number = saved_next;
            if let Some(entity) = runtime.entities.get_mut(&corpse) {
                entity.number = number;
                entity.presence = presence;
                entity.attachments = attachments;
                entity.attach_ignore_collision = attach_ignore_collision;
            }
            runtime.set_object_field(corpse, "origin", origin);
            runtime.set_object_field(corpse, "angles", angles);
            runtime.set_object_field(corpse, "model", Value::String(model));
            runtime.set_object_field(corpse, "team", team);
            runtime.set_object_field(corpse, "contents", Value::Int(0x0400_0000));
            if let Some(shown) = shown {
                runtime.shown.insert(corpse, shown);
            }
            if let Some(anim) = anim {
                runtime.actor_anims.insert(corpse, anim);
            }
            if let Some(ragdoll) = ragdoll {
                runtime.t5.ragdolls.insert(corpse, ragdoll);
            }
            if let Some(flags) = client_flags {
                runtime.t5.client_flags.insert(corpse, flags);
            }
            runtime.t5.corpse_slots.push(corpse);
            diag::info!(
                Sim,
                "t5 actor_become_corpse actor={actor} corpse={corpse} num={number} active={}",
                runtime.t5.corpse_slots.len()
            );
        }
    }
    runtime.request_delete(actor);
}

fn evict_corpse_slot(runtime: &mut RoundScript, viewer: Option<([f32; 3], [f32; 3])>) -> u64 {
    let Some((eye, fwd)) = viewer else {
        return runtime.t5.corpse_slots[0];
    };
    let slots = runtime.t5.corpse_slots.clone();
    let mut best = (slots[0], 0u8, -1.0f32);
    for id in slots {
        let pos = vector_field(runtime, id, "origin").unwrap_or([0.0; 3]);
        let dx = pos[0] - eye[0];
        let dy = pos[1] - eye[1];
        let dz = pos[2] - eye[2];
        let dist_sq = dx * dx + dy * dy + dz * dz;
        let dist = dist_sq.sqrt().max(1e-4);
        let cos = (dx * fwd[0] + dy * fwd[1] + dz * fwd[2]) / dist;
        let bucket = if cos <= 0.0 {
            3u8
        } else if cos < std::f32::consts::FRAC_1_SQRT_2 {
            2u8
        } else {
            1u8
        };
        if (bucket, dist_sq) > (best.1, best.2) {
            best = (id, bucket, dist_sq);
        }
    }
    best.0
}

// --- Scripted and custom animation ---------------------------------------

/// `animscripted` on an actor: refused once it died, else the scripted
/// state is pushed, its animscript cut off, and the species' scripted init
/// told of the animation.
pub(crate) fn anim_scripted(world: &mut World, actor: u64, args: &[Value]) -> Result<(), String> {
    if !alive(world, actor) {
        let (number, origin) = {
            let mut runtime = world.resource_mut::<RoundScript>();
            let number = runtime
                .entities
                .get(&actor)
                .map_or(0, |entity| entity.number);
            (
                number,
                vector_field(&mut runtime, actor, "origin").unwrap_or([0.0; 3]),
            )
        };
        return Err(format!(
            "tried to play a scripted animation on a dead AI; entity {number} origin {} {} {}",
            origin[0], origin[1], origin[2]
        ));
    }
    with_brain(world, actor, |brain| brain.push(ANIM_SCRIPTED));
    kill_anim_script(world, actor, false);
    with_movement(world, actor, |movement| movement.stop_scripted());
    let init = format!("animscripts/{}scripted::init", prefix(world, actor));
    let now = super::players::now_ms(world);
    run_now(world, &init, Value::Object(actor), args.to_vec(), now)
        .map_err(|fault| format!("{init}: {fault:?}"))
}

/// `stopanimscripted` on an actor: a pending scripted state is popped.
pub(crate) fn stop_anim_scripted(world: &mut World, actor: u64) {
    with_brain(world, actor, |brain| {
        if brain.pending_top() == ANIM_SCRIPTED {
            brain.pop();
        }
    });
}

fn scripted_think(world: &mut World, actor: u64) -> u8 {
    clear_path(world, actor);
    set_anim_script(
        world,
        actor,
        AnimScript::Species(SpeciesScript::Scripted),
        MOVE_STOP,
        AnimMode::Gravity,
    );
    if !script_running(world, actor) {
        with_brain(world, actor, |brain| {
            if brain.pending_top() == ANIM_SCRIPTED {
                brain.pop();
            }
        });
        return THINK_CHANGED;
    }
    let playing = world
        .resource::<RoundScript>()
        .actor_moves
        .get(&actor)
        .is_some_and(super::actor_nav::ActorMove::scripted_playing);
    if !playing {
        with_brain(world, actor, ActorBrain::pop);
        return THINK_CHANGED;
    }
    sense(world, actor);
    with_movement(world, actor, |movement| movement.velocity = [0.0; 3]);
    THINK_DONE
}

/// `animcustom(func)`: the custom state is pushed (unless refused), the
/// animscript cut off, and `func` becomes the custom animscript.
pub(crate) fn anim_custom(world: &mut World, actor: u64, function: usize) {
    let pushed = with_brain(world, actor, |brain| brain.push(ANIM_CUSTOM)).unwrap_or(false);
    if !pushed {
        return;
    }
    kill_anim_script(world, actor, false);
    with_brain(world, actor, |brain| {
        brain.custom = Some(CustomScript::Function(function))
    });
}

fn custom_think(world: &mut World, actor: u64) -> u8 {
    clear_path(world, actor);
    let mode = override_or(world, actor, AnimMode::Gravity);
    set_anim_script(world, actor, AnimScript::Custom, MOVE_STOP, mode);
    if !script_running(world, actor) {
        with_brain(world, actor, ActorBrain::pop);
        return THINK_CHANGED;
    }
    sense(world, actor);
    THINK_DONE
}

/// `traversemode(mode)`.
pub(crate) fn set_traverse_mode(world: &mut World, actor: u64, mode: &str) -> Result<(), String> {
    let mode = match mode {
        "gravity" => 1,
        "nogravity" => 2,
        "noclip" => 3,
        _ => return Err("traverseMode must be 'gravity', 'nogravity', or 'noclip'\n".into()),
    };
    with_brain(world, actor, |brain| brain.traverse_mode = mode);
    Ok(())
}

// --- Fields -------------------------------------------------------------

/// The hit's yaw against the actor's: `AngleNormalize180(yaw(dir) - yaw)`,
/// truncated.
fn damage_yaw(runtime: &mut RoundScript, actor: u64, dir: &Value) -> i32 {
    let yaw = vector_field(runtime, actor, "angles").map_or(0.0, |angles| angles[1]);
    let dir = match dir {
        Value::Vector(v) => *v,
        _ => [0.0; 3],
    };
    let from = if dir[0] == 0.0 && dir[1] == 0.0 {
        0.0
    } else {
        let deg = dir[1].atan2(dir[0]).to_degrees();
        if deg < 0.0 { deg + 360.0 } else { deg }
    };
    let turns = (from - yaw) * (1.0 / 360.0);
    ((turns - (turns + 0.5).floor()) * 360.0) as i32
}

fn vector_field(runtime: &mut RoundScript, id: u64, name: &str) -> Option<[f32; 3]> {
    match runtime.object_field(id, name) {
        Value::Vector(v) => Some(v),
        _ => None,
    }
}

fn float_field(runtime: &mut RoundScript, id: u64, name: &str) -> Option<f32> {
    match runtime.object_field(id, name) {
        Value::Int(n) => Some(n as f32),
        Value::Float(f) => Some(f),
        _ => None,
    }
}
