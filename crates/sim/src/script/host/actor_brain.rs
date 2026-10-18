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
/// A sight check is reused for this long (ms).
const SIGHT_CACHE_MS: i64 = 250;
/// The stop-to-fight rule shrinks the goal by half its radius, at most this.
const FIGHT_GOAL_SHRINK: f32 = 64.0;
/// Far from its path's end by at least this, a non-combat actor runs.
const WALK_DIST: f32 = 128.0;

/// `movemode`'s values.
const MOVE_STOP: u8 = 0;
const MOVE_WALK: u8 = 2;
const MOVE_RUN: u8 = 3;

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

/// Runs the actor's animscript init and takes charge of it: exposed at the
/// bottom of its state stack, started by nothing (its substate is none
/// until its enemy is first missing or found).
pub(crate) fn begin(world: &mut World, actor: u64) -> Result<(), String> {
    let (prefix, dog) = match world
        .resource_mut::<RoundScript>()
        .object_field(actor, "type")
    {
        Value::String(kind) if kind.as_bytes().starts_with(b"zombie_dog") => ("zombie_dog_", true),
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
    runtime.actor_moves.entry(actor).or_default();
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
    for actor in actors {
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
            let mut runtime = world.resource_mut::<RoundScript>();
            runtime.actor_brains.remove(&actor);
            runtime.request_delete(actor);
        }
    }
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
    with_movement(world, actor, |movement| movement.drop_path());
}

fn safe_to_change_script(world: &mut World, actor: u64) -> bool {
    !matches!(
        world
            .resource_mut::<RoundScript>()
            .object_field(actor, "safetochangescript"),
        Value::Int(0)
    )
}

/// The stop animscript, walking mode.
fn start_stop(world: &mut World, actor: u64) {
    set_anim_script(
        world,
        actor,
        AnimScript::Species(SpeciesScript::Stop),
        MOVE_STOP,
        AnimMode::Normal,
    );
}

/// The move animscript along the path, running or walking; the stop
/// animscript with no path. An actor not yet moving starts from stop and
/// the normal movement mode.
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
    let movemode = if run { MOVE_RUN } else { MOVE_WALK };
    set_anim_script(
        world,
        actor,
        AnimScript::Species(SpeciesScript::Move),
        movemode,
        AnimMode::Normal,
    );
}

/// The combat animscript, gravity movement.
fn start_combat(world: &mut World, actor: u64) {
    set_anim_script(
        world,
        actor,
        AnimScript::Species(SpeciesScript::Combat),
        MOVE_STOP,
        AnimMode::Gravity,
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

/// Whether the actor sees its enemy, a check reused for a quarter second.
fn can_see_enemy(world: &mut World, actor: u64) -> bool {
    let Some(enemy) = enemy(world, actor) else {
        return false;
    };
    let now = super::players::now_ms(world);
    let cached = brain(world, actor)
        .and_then(|brain| brain.sightings.get(&enemy).copied())
        .filter(|sighting| {
            sighting
                .checked
                .is_some_and(|at| at + SIGHT_CACHE_MS >= now)
        });
    if let Some(sighting) = cached {
        return sighting.visible;
    }
    let visible = super::actor_nav::sees(world, actor, enemy);
    with_brain(world, actor, |brain| {
        let sighting = brain.sightings.entry(enemy).or_default();
        sighting.visible = visible;
        sighting.checked = Some(now);
        if visible {
            sighting.seen = Some(now);
        }
    });
    visible
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
    set_code_orient(world, actor, Orient::Enemy);
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
        // Noclip moves as nogravity does: actors collide with nothing here.
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
        with_movement(world, actor, |movement| {
            movement.velocity = [0.0; 3];
            movement.finish_traversal();
        });
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

/// Lets go of a dead actor: its path ends. (Black Ops also takes it off the
/// other AI's minds and its links, not ported.)
fn release_dead(world: &mut World, actor: u64) {
    clear_path(world, actor);
    with_brain(world, actor, |brain| brain.set_substate(DEATH_RELEASED));
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
