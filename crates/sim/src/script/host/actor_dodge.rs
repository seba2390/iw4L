//! Black Ops' move decision for an actor starting or keeping a move along
//! its path (the path step): the nearest sentient in its way is looked
//! for; the actor tries a dodge path around it on either side, and failing
//! that stops and waits behind it, joining the wait of the actor it waits
//! on (the follow chain). It answers whether the move goes on, stops, or
//! walks. What is not ported is listed under "AI and actors" in
//! `docs/fidelity/t5.md`.

use super::actor_nav::{self, ActorMove, AnimMode};
use super::actor_path::{DodgeProbe, Probe, Sweep};
use crate::script::{RoundScript, Value};
use bevy_ecs::prelude::World;

/// The path step's answers.
pub(crate) const MOVE_ON: u8 = 0;
pub(crate) const STOP: u8 = 1;
pub(crate) const WALK: u8 = 2;

/// The step is not taken again for this long after it stopped the actor,
/// nor for one frame while it holds a blocker.
const RECHECK_MS: i64 = 500;
const BLOCKED_RECHECK_MS: i64 = 50;
/// A dodge made, or a wait begun, holds this long.
const HOLD_MS: i64 = 1000;
/// Every this many rechecks a waiting actor gives up its dodge.
const GIVE_UP_MS: i64 = 2000;
/// A sentient this far up or down is not in the way.
const IN_WAY_HEIGHT: f32 = 80.0;
/// Off the line of motion by less than this (squared) is in the way.
const SIDE_SQ: f32 = 900.0;
/// The dodge's reach and the nearness it waits at (37.5, squared).
const NEAR: f32 = 37.5;
const NEAR_SQ: f32 = 1406.25;
/// Beyond this (squared) a body ahead is not worth waiting for.
const FAR_SQ: f32 = 3600.0;
/// A lookahead this long reaches past what is in the way.
const LOOK_REACH: f32 = 60.0;
/// The dodge's points: `NEAR` off the body's side, from `NEAR` behind it to
/// `NEAR` ahead.
const DODGE_SPAN: f32 = 75.0;
/// A moving actor looks this share of its speed (squared) ahead.
const LEAD_SHARE: f32 = 0.01;
/// Another sentient counts as moving when five times its speed beats the
/// actor's.
const MOVING_SHARE_SQ: f32 = 25.0;
/// The ranges a wide step (`interval`) never drops below.
const MIN_INTERVAL: f32 = 30.0;
const DEFAULT_INTERVAL: f32 = 96.0;
/// The actor's wish delta is per 50 ms frame.
const FRAMES_PER_SECOND: f32 = 20.0;
/// The scripted states a body in the way waits through.
const SCRIPTED_STATES: [super::actor_brain::AiState; 2] = [
    super::actor_brain::ANIM_SCRIPTED,
    super::actor_brain::ANIM_CUSTOM,
];
/// The AI state that never joins a follow chain.
const NO_CHAIN_STATE: super::actor_brain::AiState = 3;
/// Black Ops' teams, and the one each fights.
const TEAM_AXIS: u8 = 1;
const TEAM_ALLIES: u8 = 2;
const TEAM_NEUTRAL: u8 = 3;
const OPPOSING: [u8; 4] = [0, TEAM_ALLIES, TEAM_AXIS, 0];
/// A follow chain longer than this is broken (Black Ops walks it without
/// a bound; a loop never forms there).
const CHAIN_LIMIT: usize = 64;

/// A sentient the step looks at: a player or a live actor.
struct Sentient {
    id: u64,
    entnum: u16,
    at: [f32; 3],
    team: u8,
    actor: Option<ActorView>,
    /// A player's velocity.
    velocity: [f32; 2],
}

/// What the step reads of another actor.
struct ActorView {
    chain: Option<u64>,
    chain_ent: Option<u16>,
    held: bool,
    wish: [f32; 2],
    hit_body: bool,
    anim_mode: AnimMode,
    movemode: u8,
}

/// The step's working state (the actor's move and what it found).
struct Scan {
    /// Where the actor stands, and where its dodge starts from.
    home: [f32; 3],
    origin: [f32; 3],
    vel: [f32; 2],
    dir: [f32; 2],
    speed_sq: f32,
    lead: f32,
    near_sq: f32,
    wide_sq: f32,
    reach_sq: f32,
    spent: bool,
    at_goal: bool,
    /// The actors the dodge sweeps watch for.
    list: Vec<u16>,
    best: Option<usize>,
    best_sq: f32,
    /// The nearest moving sentient ahead going the actor's way.
    follow: Option<usize>,
    follow_sq: f32,
    follow_vel: [f32; 2],
}

/// The actor itself, as the step reads it.
struct Me {
    id: u64,
    entnum: u16,
    team: u8,
    speed_sq: f32,
    meets_players: bool,
}

/// One sentient's place relative to the actor.
struct Sub {
    at: [f32; 3],
    d: [f32; 2],
    dist_sq: f32,
    along: f32,
}

/// Black Ops' path step for `actor` about to move along its path: `wide`
/// takes its `interval` for the ranges, `goal_check` lets an actor at its
/// goal count as arrived.
pub(crate) fn path_step(world: &mut World, actor: u64, wide: bool, goal_check: bool) -> u8 {
    let now = super::players::now_ms(world);
    let Some(mut movement) = world
        .resource::<RoundScript>()
        .actor_moves
        .get(&actor)
        .cloned()
    else {
        return MOVE_ON;
    };
    let step = decide(world, actor, (wide, goal_check), now, &mut movement);
    if let Some(slot) = world
        .resource_mut::<RoundScript>()
        .actor_moves
        .get_mut(&actor)
    {
        *slot = movement;
    }
    step
}

fn decide(
    world: &mut World,
    actor: u64,
    (wide, goal_check): (bool, bool),
    now: i64,
    movement: &mut ActorMove,
) -> u8 {
    if now < movement.path_wait {
        return STOP;
    }
    if movement.blocker.is_some() {
        movement.path_wait = now + BLOCKED_RECHECK_MS;
        return STOP;
    }
    let origin = actor_nav::origin_of(world, actor);
    let mut result = MOVE_ON;
    // The way the actor moves: its velocity, or along its lookahead at
    // its species' run speed while nearly still.
    let my_vel = [movement.velocity[0], movement.velocity[1]];
    let my_speed = flat_len(my_vel);
    let mut dir = scale(my_vel, 1.0 / safe(my_speed));
    let mut vel = my_vel;
    let mut speed = my_speed;
    if my_speed < 1.0 {
        dir = [movement.path.look_dir[0], movement.path.look_dir[1]];
        if dir == [0.0; 2] {
            if movement.path.negotiation_start > 0 {
                return MOVE_ON;
            }
            movement.clear_path(origin);
            return STOP;
        }
        speed = actor_nav::run_speed(world, actor);
        vel = scale(dir, speed);
    }
    let (near_sq, wide_sq) = if wide {
        let mut near = actor_nav::float_field(world, actor, "interval").unwrap_or(DEFAULT_INTERVAL);
        let mut half = near * 0.5;
        if half < NEAR {
            half = NEAR;
            if near < MIN_INTERVAL {
                near = MIN_INTERVAL;
            }
        }
        (near * near, half * half)
    } else {
        (SIDE_SQ, NEAR_SQ)
    };
    let speed_sq = speed * speed;
    let movemode = super::actor_brain::movemode(world, actor);
    let at_goal =
        movement.chain.is_some() || (goal_check && super::actor_brain::in_goal(world, actor));
    let reach_sq = if near_sq - speed_sq < 0.0 {
        speed_sq
    } else {
        near_sq
    };
    let mut scan = Scan {
        home: origin,
        origin,
        vel,
        dir,
        speed_sq,
        lead: if movemode != 0 {
            speed_sq * LEAD_SHARE
        } else {
            0.0
        },
        near_sq,
        wide_sq,
        reach_sq,
        spent: movement.path.amount_spent(),
        at_goal,
        list: Vec::new(),
        best: None,
        best_sq: reach_sq,
        follow: None,
        follow_sq: near_sq,
        follow_vel: [0.0; 2],
    };
    let me = Me {
        id: actor,
        entnum: actor_nav::entity_number(world, actor),
        team: team_of(world, actor),
        speed_sq: my_speed * my_speed,
        meets_players: !movement.push_player,
    };
    let sentients = gather(world);
    for (index, sentient) in sentients.iter().enumerate() {
        let Some(sub) = consider(&me, index, sentient, &mut scan, movement) else {
            continue;
        };
        if in_way(&scan, &sub, sentient.entnum, movement, now, movemode) {
            scan.best_sq = sub.dist_sq;
            scan.best = Some(index);
        }
    }
    if let Some(follow) = scan.follow
        && scan.follow_sq < scan.best_sq
    {
        let ahead = &sentients[follow];
        let Some(view) = ahead.actor.as_ref() else {
            movement.path_wait = now + RECHECK_MS;
            return STOP;
        };
        if dot(scan.vel, scan.follow_vel) < 0.0 {
            movement.path_wait = now + RECHECK_MS;
            return STOP;
        }
        let walking_far = view.movemode == 2 && scan.follow_sq >= scan.wide_sq;
        if view.anim_mode != AnimMode::Normal || (view.movemode != 3 && !walking_far) {
            scan.best = Some(follow);
            scan.best_sq = scan.follow_sq;
        } else {
            result = WALK;
        }
    }
    movement.chain = None;
    movement.chain_ent = None;
    let no_dodge = !matches!(
        world
            .resource_mut::<RoundScript>()
            .object_field(actor, "nododgemove"),
        Value::Int(0) | Value::Undefined
    ) || actor_nav::dvar_f32(world, "ai_noDodge", 0.0) != 0.0;
    let best = match scan.best {
        Some(best) if !no_dodge => best,
        _ => return nothing_in_way(world, &scan, &sentients, movement, (movemode, now), result),
    };
    let body = &sentients[best];
    if movement.path.dodge_entity != Some(body.entnum) {
        movement.dodge_time = 0;
        if movement.path.dodge_count() < 0 {
            movement.path.set_dodge_count(0);
        }
        movement.path.dodge_entity = Some(body.entnum);
    }
    if is_enemy(me.team, body.team) {
        if scan.best_sq >= NEAR_SQ {
            return result;
        }
        if super::actor_brain::enemy(world, actor) == Some(body.id) {
            movement.clear_path(origin);
        }
        movement.path_wait = now + RECHECK_MS;
        return STOP;
    }
    let other = body.actor.as_ref().map(|_| body.id);
    let short = short_lookahead(movement, movemode);
    let (depth, top, substates) = super::actor_brain::stack_view(world, actor).unwrap_or((
        0,
        0,
        [0; super::actor_brain::STACK_LEVELS],
    ));
    let mut join_first = short;
    loop {
        if !join_first {
            if now < movement.dodge_time {
                return result;
            }
            let index = movement.path.rejoin_index(scan.origin, scan.best_sq);
            let fallback = movement.path.to_next() != 0.0
                && movement.path.look_dist >= scan.best_sq.sqrt() + NEAR;
            if fallback || index >= movement.path.negotiation_start as isize {
                let actors_met = other.is_none();
                back_off(world, actor, &mut scan, body.at, actors_met);
                if try_sides(
                    world,
                    actor,
                    &scan,
                    body.at,
                    (index, fallback),
                    actors_met,
                    now,
                    movement,
                ) {
                    movement.dodge_time = now + HOLD_MS;
                    return if movemode != 0 { result } else { STOP };
                }
            }
            match wait_or_hold(
                world,
                &scan,
                other,
                movement,
                (depth, substates[depth]),
                movemode,
                now,
            ) {
                Wait::Keep => return result,
                Wait::Chain => {
                    if other.is_none_or(|other| chain_ent_of(world, other) != Some(me.entnum)) {
                        movement.chain = Some(me.id);
                        movement.chain_ent = Some(body.entnum);
                    }
                    movement.path_wait = now + RECHECK_MS;
                    return STOP;
                }
                Wait::Hold => {
                    movement.dodge_time = now + HOLD_MS;
                    if movemode != 0 {
                        return result;
                    }
                    if short {
                        return STOP;
                    }
                    result = STOP;
                }
            }
        }
        join_first = false;
        if join_chain(world, &me, other, top, now, movement) {
            return STOP;
        }
    }
}

/// What the actor does about the body in its way it could not dodge.
enum Wait {
    /// Move on as decided so far.
    Keep,
    /// Wait behind it, heading a follow chain of its own.
    Chain,
    /// Hold for a while (and join the body's chain).
    Hold,
}

fn wait_or_hold(
    world: &mut World,
    scan: &Scan,
    other: Option<u64>,
    movement: &mut ActorMove,
    (depth, substate): (usize, i32),
    movemode: u8,
    now: i64,
) -> Wait {
    let other_view = other.map(|other| {
        let (_, top, substates) = super::actor_brain::stack_view(world, other).unwrap_or((
            0,
            0,
            [0; super::actor_brain::STACK_LEVELS],
        ));
        (other, top, substates[depth])
    });
    if let Some((other, _, other_substate)) = other_view
        && substate == super::actor_brain::EXPOSED_COMBAT
        && (other_substate != super::actor_brain::EXPOSED_COMBAT
            || !actor_nav::has_path_of(world, other))
    {
        return Wait::Hold;
    }
    let look_dist = movement.path.look_dist;
    let at_end = movement.path.at_end();
    if scan.best_sq >= NEAR_SQ {
        if movemode != 0 {
            return Wait::Keep;
        }
        if scan.best_sq >= FAR_SQ && (look_dist >= LOOK_REACH || at_end) {
            return Wait::Hold;
        }
    }
    let count = movement.path.dodge_count();
    // Backing off a bumped body with a short lookahead resets the dodge at
    // once; otherwise every fourth recheck gives the dodge up.
    let reset_now = if count < 0 {
        look_dist < LOOK_REACH && !at_end
    } else {
        if count == 0
            && let Some((other, top, _)) = other_view
        {
            let anim = actor_nav::anim_mode_of(world, other);
            if matches!(anim, Some(AnimMode::Normal | AnimMode::ZOnlyPhysics))
                && super::actor_brain::movemode(world, other) == 0
                && super::actor_brain::in_goal(world, other)
            {
                return Wait::Hold;
            }
            if SCRIPTED_STATES.contains(&top) {
                return Wait::Hold;
            }
        }
        false
    };
    if !reset_now {
        if (now / RECHECK_MS) % (GIVE_UP_MS / RECHECK_MS) != 0 {
            return Wait::Chain;
        }
        if count >= 0 {
            if count != 0 || scan.spent {
                movement.clear_path(scan.home);
            } else {
                movement.path.set_dodge_count(-1);
            }
            return Wait::Chain;
        }
    }
    if let Some((other, top, _)) = other_view
        && (actor_nav::anim_mode_of(world, other) == Some(AnimMode::Normal)
            || SCRIPTED_STATES.contains(&top))
    {
        return Wait::Hold;
    }
    movement.path.set_dodge_count(0);
    Wait::Chain
}

/// With no body in the way the dodge ends: a slower actor ahead going the
/// same way is dodged from now on, a dodge entity still near is kept.
fn nothing_in_way(
    world: &mut World,
    scan: &Scan,
    sentients: &[Sentient],
    movement: &mut ActorMove,
    (movemode, now): (u8, i64),
    result: u8,
) -> u8 {
    if !scan.spent
        && let Some(follow) = scan.follow
        && dot(scan.follow_vel, scan.follow_vel) < scan.speed_sq
    {
        if movement.path.dodge_count() == 0 {
            movement.path.set_dodge_count(-1);
        }
        movement.path.dodge_entity = Some(sentients[follow].entnum);
        return MOVE_ON;
    }
    let count = movement.path.dodge_count();
    if count != 0 {
        if let Some(entnum) = movement.path.dodge_entity
            && (count >= 0 || NEAR <= movement.path.look_dist)
            && let Some(at) = actor_nav::entity_origin_by_number(world, entnum)
            && (scan.origin[1] - at[1]).powi(2) + (scan.origin[0] - at[0]).powi(2) < FAR_SQ
        {
            return result;
        }
        if count < 0 {
            movement.path.set_dodge_count(0);
        }
    }
    if movement.path.dodge_entity.take().is_some() && movemode == 0 && movement.dodge_time <= now {
        return STOP;
    }
    movement.dodge_time = 0;
    result
}

/// Whether the lookahead is too short to dodge by: stopped, it did not
/// grow; moving, it did not, and ends within 60 short of the path's end.
fn short_lookahead(movement: &ActorMove, movemode: u8) -> bool {
    if movemode == 0 {
        if !movement.path.grew() {
            return true;
        }
    } else if movement.path.grew() {
        return false;
    }
    movement.path.look_dist < LOOK_REACH && !movement.path.at_end()
}

/// Close to the body, the dodge starts from where the actor can back off
/// to, `NEAR` from it.
fn back_off(world: &mut World, actor: u64, scan: &mut Scan, at: [f32; 3], actors_met: bool) {
    if scan.best_sq >= NEAR_SQ {
        return;
    }
    let d = [at[0] - scan.origin[0], at[1] - scan.origin[1]];
    let len = flat_len(d);
    if len >= NEAR {
        return;
    }
    let unit = scale(d, 1.0 / safe(len));
    let to = [at[0] - unit[0] * NEAR, at[1] - unit[1] * NEAR, at[2]];
    let out = Sweeps {
        world,
        actor,
        list: &scan.list,
        actors_met,
    }
    .sweep(scan.origin, to)
    .1;
    scan.origin = out;
}

/// The dodge around the body on its right, then on its left.
#[allow(clippy::too_many_arguments)]
fn try_sides(
    world: &mut World,
    actor: u64,
    scan: &Scan,
    at: [f32; 3],
    (index, fallback): (isize, bool),
    actors_met: bool,
    now: i64,
    movement: &mut ActorMove,
) -> bool {
    let [dx, dy] = scan.dir;
    let side = [dy * NEAR, dx * -NEAR];
    let mut beside = [
        at[0] - dx * NEAR + side[0],
        at[1] - dy * NEAR + side[1],
        at[2],
    ];
    let mut past = [
        dx * DODGE_SPAN + beside[0],
        dy * DODGE_SPAN + beside[1],
        at[2],
    ];
    let mut probe = Sweeps {
        world,
        actor,
        list: &scan.list,
        actors_met,
    };
    for turn in 0..2 {
        if turn == 1 {
            beside[0] -= side[0] * 2.0;
            beside[1] -= side[1] * 2.0;
            past[0] -= side[0] * 2.0;
            past[1] -= side[1] * 2.0;
        }
        if movement
            .path
            .dodge_around(scan.origin, beside, past, index, fallback, now, &mut probe)
        {
            return true;
        }
    }
    false
}

/// The actor joins the follow chain of the body it waits on.
fn join_chain(
    world: &mut World,
    me: &Me,
    other: Option<u64>,
    top: super::actor_brain::AiState,
    now: i64,
    movement: &mut ActorMove,
) -> bool {
    let Some(other) = other else {
        return false;
    };
    update_chain(world, other, now);
    let Some((chain, chain_ent)) = world
        .resource::<RoundScript>()
        .actor_moves
        .get(&other)
        .map(|other| (other.chain, other.chain_ent))
    else {
        return false;
    };
    if chain.is_none_or(|root| root == me.id)
        || chain_ent == Some(me.entnum)
        || top == NO_CHAIN_STATE
    {
        return false;
    }
    if movement.path.dodge_count() < 0 {
        movement.path.set_dodge_count(0);
    }
    movement.chain = chain;
    movement.chain_ent = chain_ent;
    movement.path_wait = now + RECHECK_MS;
    movement.dodge_time = 0;
    true
}

/// An actor's follow chain, followed to its head: kept while the head
/// waits (its step's recheck not yet due), else dropped.
pub(crate) fn update_chain(world: &mut World, actor: u64, now: i64) {
    let entnum = actor_nav::entity_number(world, actor);
    let joined = {
        let runtime = world.resource::<RoundScript>();
        let Some(movement) = runtime.actor_moves.get(&actor) else {
            return;
        };
        if movement.anim_mode != AnimMode::Normal {
            None
        } else if movement.chain == Some(actor) {
            return;
        } else {
            let mut link = movement.chain;
            let mut head = None;
            for _ in 0..CHAIN_LIMIT {
                let Some(id) = link else {
                    break;
                };
                let Some(next) = runtime.actor_moves.get(&id) else {
                    break;
                };
                if next.chain == Some(id) {
                    head = Some((id, next));
                    break;
                }
                link = next.chain;
            }
            head.and_then(|(id, head)| {
                let mut until = head.path_wait;
                if head.next_think <= now {
                    until += BLOCKED_RECHECK_MS;
                }
                (now < until && head.chain_ent != Some(entnum)).then_some((id, head.chain_ent))
            })
        }
    };
    if let Some(movement) = world
        .resource_mut::<RoundScript>()
        .actor_moves
        .get_mut(&actor)
    {
        (movement.chain, movement.chain_ent) = match joined {
            Some((head, chain_ent)) => (Some(head), chain_ent),
            None => (None, None),
        };
    }
}

/// Whether `other` is the actor's to consider (Black Ops answers it by the
/// sentient), and where it lies relative to the move.
fn consider(
    me: &Me,
    index: usize,
    sentient: &Sentient,
    scan: &mut Scan,
    movement: &mut ActorMove,
) -> Option<Sub> {
    let enemy = is_enemy(me.team, sentient.team);
    if let Some(view) = &sentient.actor
        && (enemy || sentient.id == me.id || view.chain_ent == Some(me.entnum))
    {
        return None;
    }
    let at = sentient.at;
    if (scan.origin[2] - at[2]).abs() >= IN_WAY_HEIGHT {
        return None;
    }
    let d = [at[0] - scan.origin[0], at[1] - scan.origin[1]];
    let dist_sq = d[0] * d[0] + d[1] * d[1];
    if scan.reach_sq <= dist_sq {
        return None;
    }
    let along = scan.vel[1] * d[1] + scan.vel[0] * d[0] - scan.lead;
    let forward = movement.path.forward_dir;
    if along <= 0.0 && forward[1] * d[1] + d[0] * forward[0] <= 0.0 && !scan.spent {
        return None;
    }
    let sub = Sub {
        at,
        d,
        dist_sq,
        along,
    };
    let near = dist_sq < SIDE_SQ;
    let me_moving = 1.0 <= me.speed_sq;
    let other_vel = match &sentient.actor {
        None => {
            if scan.best_sq <= dist_sq || !me.meets_players {
                return None;
            }
            if enemy {
                return Some(sub);
            }
            sentient.velocity
        }
        Some(view) => {
            if near && movement.blocker.is_none() && view.hit_body {
                movement.blocker = Some(sentient.id);
            }
            if view.chain.is_none() && !view.held {
                scale(view.wish, FRAMES_PER_SECOND)
            } else {
                [0.0; 2]
            }
        }
    };
    let other_sq = dot(other_vel, other_vel);
    let moving = !(other_sq < 1.0 || other_sq * MOVING_SHARE_SQ <= me.speed_sq);
    let heads = movement.chain == Some(me.id);
    let mut same_way = true;
    if !heads && moving {
        if scan.near_sq <= dist_sq {
            return None;
        }
        if scan.vel[1] * other_vel[1] + scan.vel[0] * other_vel[0] < 0.0 {
            if !scan.at_goal && (me_moving || !near) {
                if me.entnum <= sentient.entnum {
                    return None;
                }
                same_way = false;
            }
        } else {
            if scan.lead + along <= 0.0 {
                return None;
            }
            if sentient.actor.is_some()
                && sentient.entnum < me.entnum
                && other_vel[1] * d[1] + other_vel[0] * d[0] < scan.lead
            {
                return None;
            }
        }
    }
    if sentient.actor.is_some() {
        scan.list.push(sentient.entnum);
    }
    if dist_sq < scan.best_sq {
        if heads || !moving {
            return Some(sub);
        }
        if dist_sq < scan.follow_sq {
            if !same_way {
                return Some(sub);
            }
            scan.follow_sq = dist_sq;
            scan.follow = Some(index);
            scan.follow_vel = other_vel;
        }
    }
    None
}

/// Whether the sentient stands in the way of the move: within 30 of its
/// line and short of the lookahead's reach, where the actor's own way
/// meets it.
fn in_way(
    scan: &Scan,
    sub: &Sub,
    entnum: u16,
    movement: &ActorMove,
    now: i64,
    movemode: u8,
) -> bool {
    let cross = sub.d[1] * scan.dir[0] - scan.dir[1] * sub.d[0];
    let room = SIDE_SQ - cross * cross;
    if room <= 0.0 {
        return false;
    }
    let path = &movement.path;
    let ahead = scan.dir[1] * sub.d[1] + scan.dir[0] * sub.d[0] - path.look_dist;
    if 0.0 < ahead && (room + NEAR_SQ) - SIDE_SQ <= ahead * ahead {
        return false;
    }
    if scan.spent {
        return true;
    }
    let t = sub.along / scan.speed_sq;
    let rel = [scan.vel[0] * -t + sub.d[0], scan.vel[1] * -t + sub.d[1]];
    let rel_sq = rel[0] * rel[0] + rel[1] * rel[1];
    let mut look_first = false;
    let mut general = true;
    if path.dodge_entity == Some(entnum) {
        if scan.at_goal {
            if (path.dodge_count() != 0 || path.grew())
                && !path.at_end()
                && path.look_dist < sub.dist_sq.sqrt() + NEAR
            {
                return true;
            }
        } else if movemode != 0 && now < movement.dodge_time {
            if t <= 0.0 || SIDE_SQ <= rel_sq {
                return false;
            }
            look_first = true;
            general = false;
        }
    }
    let mut d = sub.d;
    if general {
        if rel_sq < NEAR_SQ {
            if 0.0 < t {
                look_first = true;
            } else {
                if FAR_SQ <= sub.dist_sq {
                    return false;
                }
                let Some(point) = path.look_point(scan.origin) else {
                    return true;
                };
                d = [sub.at[0] - point[0], sub.at[1] - point[1]];
            }
        } else {
            if FAR_SQ <= sub.dist_sq {
                return false;
            }
            let Some(point) = path.look_point(scan.origin) else {
                return false;
            };
            d = [sub.at[0] - point[0], sub.at[1] - point[1]];
        }
    }
    if look_first {
        let Some(point) = path.look_point(scan.origin) else {
            return true;
        };
        d = [sub.at[0] - point[0], sub.at[1] - point[1]];
        if scan.vel[1] * d[1] + scan.vel[0] * d[0] <= 0.0 {
            return true;
        }
    }
    let forward = path.forward_dir;
    let a = forward[1] * d[1] + forward[0] * d[0];
    if 0.0 < a {
        let perp = [-a * forward[0] + d[0], forward[1] * -a + d[1]];
        if perp[0] * perp[0] + perp[1] * perp[1] < SIDE_SQ {
            return true;
        }
    }
    false
}

/// The players, then the live actors, as Black Ops keeps its sentients.
fn gather(world: &mut World) -> Vec<Sentient> {
    let players: Vec<(u32, u64)> = world
        .resource::<RoundScript>()
        .players
        .iter()
        .map(|(&client, slot)| (client, slot.object))
        .collect();
    let mut out = Vec::new();
    for (client, id) in players {
        if !world.resource::<RoundScript>().live(&id) {
            continue;
        }
        let Some(ps) = crate::frame::player_ref(world, crate::ClientId(client)) else {
            continue;
        };
        let (at, velocity) = (ps.origin, [ps.velocity[0], ps.velocity[1]]);
        out.push(Sentient {
            id,
            entnum: actor_nav::entity_number(world, id),
            at,
            team: team_of(world, id),
            actor: None,
            velocity,
        });
    }
    let actors: Vec<u64> = world
        .resource::<RoundScript>()
        .actor_moves
        .keys()
        .copied()
        .collect();
    for id in actors {
        if !matches!(super::players::entity_field(world, id, "health"), Value::Int(n) if n > 0) {
            continue;
        }
        let at = actor_nav::origin_of(world, id);
        let movemode = super::actor_brain::movemode(world, id);
        let Some(view) = world
            .resource::<RoundScript>()
            .actor_moves
            .get(&id)
            .map(|movement| ActorView {
                chain: movement.chain,
                chain_ent: movement.chain_ent,
                held: movement.blocker.is_some(),
                wish: movement.wish_delta,
                hit_body: movement.hit_body.is_some(),
                anim_mode: movement.anim_mode,
                movemode,
            })
        else {
            continue;
        };
        out.push(Sentient {
            id,
            entnum: actor_nav::entity_number(world, id),
            at,
            team: team_of(world, id),
            actor: Some(view),
            velocity: [0.0; 2],
        });
    }
    out
}

/// The entity number of the actor whose follow chain an actor heads.
fn chain_ent_of(world: &World, actor: u64) -> Option<u16> {
    world
        .resource::<RoundScript>()
        .actor_moves
        .get(&actor)
        .and_then(|movement| movement.chain_ent)
}

/// Black Ops' team number for a sentient (a player without one plays for
/// the allies).
fn team_of(world: &mut World, id: u64) -> u8 {
    let player = world.resource::<RoundScript>().player_client(id).is_some();
    match super::players::entity_field(world, id, "team") {
        Value::String(team) => match &*team {
            "axis" => TEAM_AXIS,
            "allies" => TEAM_ALLIES,
            "neutral" => TEAM_NEUTRAL,
            _ => 0,
        },
        _ if player => TEAM_ALLIES,
        _ => 0,
    }
}

/// `other` is on the team `team` fights.
fn is_enemy(team: u8, other: u8) -> bool {
    OPPOSING.get(usize::from(team)).copied().unwrap_or(0) == other
}

/// The dodge's sweeps: the watched actors' boxes first, then the actor's
/// step trace with its own clipmask.
struct Sweeps<'w, 'l> {
    world: &'w mut World,
    actor: u64,
    list: &'l [u16],
    actors_met: bool,
}

impl Probe for Sweeps<'_, '_> {
    fn clear(&mut self, from: [f32; 3], to: [f32; 3], dodge: Option<u16>) -> bool {
        actor_nav::lookahead_clear(self.world, self.actor, from, to, dodge)
    }

    fn linked(&mut self, node: u16) -> bool {
        actor_nav::node_linked(self.world, node)
    }
}

impl DodgeProbe for Sweeps<'_, '_> {
    fn sweep(&mut self, from: [f32; 3], to: [f32; 3]) -> (Sweep, [f32; 3]) {
        for &entnum in self.list {
            if let Some(other) = actor_nav::actor_by_entnum(self.world, entnum)
                && actor_nav::dodge_actor_blocks(self.world, other, from, to)
            {
                return (Sweep::Body, from);
            }
        }
        let (clear, out) =
            actor_nav::dodge_step_trace(self.world, self.actor, from, to, self.actors_met);
        (if clear { Sweep::Clear } else { Sweep::Blocked }, out)
    }
}

fn flat_len(v: [f32; 2]) -> f32 {
    (v[0] * v[0] + v[1] * v[1]).sqrt()
}

fn scale(v: [f32; 2], s: f32) -> [f32; 2] {
    [v[0] * s, v[1] * s]
}

fn dot(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[1] * b[1] + a[0] * b[0]
}

/// The engine's guard: a length that is not positive divides as 1.
fn safe(len: f32) -> f32 {
    if -len >= 0.0 { 1.0 } else { len }
}
