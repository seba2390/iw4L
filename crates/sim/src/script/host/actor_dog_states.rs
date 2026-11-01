//! Black Ops multiplayer's dog animation states. Its dog scripts never touch
//! the animation tree: `setanimstate` stores a state and a toggle on the
//! entity, and the actor's think then runs that state's rule from the
//! executable's table, on entering it and each frame it stays. The tree is
//! the one `Dog_CreateAnims` builds (`game_t5`'s `DOG_TREE`), by node number.

use bevy_ecs::prelude::World;

use super::actor_anims::{ActorAnim, NodeGoal};
use crate::script::RoundScript;

/// What `setanimstate` flips, so that setting the same state enters it again.
const TOGGLE: u32 = 0x40;

/// The states by index, as `setanimstate` looks them up; any other name is
/// state 0, which has no rule.
const STATES: [&str; 47] = [
    "idle",
    "move_run",
    "move_start",
    "move_stop",
    "move_walk",
    "move_turn_left",
    "move_turn_right",
    "move_run_turn_left",
    "move_run_turn_right",
    "move_turn_around_left",
    "move_turn_around_right",
    "move_run_turn_around_left",
    "move_run_turn_around_right",
    "move_attack",
    "stop_idle",
    "stop_attackidle",
    "stop_attackidle_bark",
    "stop_attackidle_growl",
    "pain_main",
    "pain_front",
    "pain_back",
    "pain_left",
    "pain_right",
    "pain_run_front",
    "pain_run_back",
    "pain_run_left",
    "pain_run_right",
    "death_front",
    "death_back",
    "death_left",
    "death_right",
    "flashed",
    "combat_attackidle",
    "combat_attackidle_bark",
    "combat_attackidle_growl",
    "combat_attack_run",
    "combat_attack_player_close_range",
    "combat_attack_miss",
    "combat_attack_miss_left",
    "combat_attack_miss_right",
    "traverse_jump_up_40",
    "traverse_jump_up_80",
    "traverse_jump_down_40",
    "traverse_jump_down_80",
    "traverse_wallhop",
    "traverse_window",
    "traverse_through_hole_42",
];

const ROOT: u16 = 0;
const IDLE: u16 = 1;
const WALK: u16 = 2;
const RUN_STOP: u16 = 3;
const RUN_TURN_180_LEFT: u16 = 0xa;
const RUN_TURN_180_RIGHT: u16 = 0xb;
/// Every turn, in the order the rule clears them.
const TURNS: [u16; 8] = [8, 9, 6, 7, 0xc, 0xd, 0xa, 0xb];
const TRAVERSES: std::ops::RangeInclusive<u16> = 0x1f..=0x24;
const RUN_START_KNOB: u16 = 0x26;
const RUN_KNOB: u16 = 0x27;
const ATTACKIDLE_KNOB: u16 = 0x28;
const LOOK_KNOBS: [u16; 4] = [0x29, 0x2a, 0x2c, 0x2b];
const RUN_START: u16 = 0x2d;
const RUN: u16 = 0x2e;
const RUN_LEAN_L: u16 = 0x2f;
const RUN_LEAN_R: u16 = 0x30;
const ATTACKIDLE: u16 = 0x31;
/// The look-around additives, one under each of `LOOK_KNOBS`.
const LOOKS: [u16; 4] = [0x36, 0x34, 0x3a, 0x38];
const ATTACK_LOOKS: [u16; 4] = [0x37, 0x35, 0x3b, 0x39];

/// How far the state's lean reaches its full animation.
const LEAN_FULL: f32 = 0.95;
const LEAN_SCALE: f32 = 20.0;

const BLEND: f32 = 0.2;
const QUICK: f32 = 0.1;
const WALK_BLEND: f32 = 0.3;
const AIM_BLEND: f32 = 0.05;

/// One dog's state as the entity carries it.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DogAnimState {
    /// The state index with its toggle.
    state: u32,
    /// The last state whose rule ran, and whether one has.
    applied: u32,
    entered: bool,
    /// `setaimanimweights`: up and left.
    aim: [f32; 2],
}

/// `setanimstate(name)`.
pub(crate) fn set_state(runtime: &mut RoundScript, actor: u64, name: &str) {
    let index = STATES.iter().position(|state| *state == name).unwrap_or(0) as u32;
    let dog = runtime.dog_anim_states.entry(actor).or_default();
    dog.state = (!dog.state & TOGGLE) | index;
}

/// `setaimanimweights(up, left)`.
pub(crate) fn set_aim(runtime: &mut RoundScript, actor: u64, aim: [f32; 2]) {
    runtime.dog_anim_states.entry(actor).or_default().aim = aim;
}

fn play(anim: &mut ActorAnim, node: u16, blend: f32, restart: bool) {
    anim.set_node(
        node,
        NodeGoal {
            weight: 1.0,
            blend,
            flag: Some("done"),
            restart,
            knob: false,
            limited: false,
        },
    );
}

/// The run's three animations by lean. Beyond 0.95 either way the lean
/// animation plays alone, nearer it fades out, and at 0 the run plays alone.
fn run_goals(anim: &mut ActorAnim, lean: f32) {
    let reach = |toward: f32| ((1.0 - toward.min(LEAN_FULL)) * LEAN_SCALE).clamp(0.0, 1.0);
    let (centre, left, right) = if lean > 0.0 {
        let right = reach(lean);
        (1.0 - right, 0.0, right)
    } else if lean < 0.0 {
        let left = reach(-lean);
        (1.0 - left, left, 0.0)
    } else {
        (1.0, 0.0, 0.0)
    };
    anim.clear_node(WALK, WALK_BLEND);
    for (node, weight) in [(RUN, centre), (RUN_LEAN_L, left), (RUN_LEAN_R, right)] {
        anim.set_node(
            node,
            NodeGoal {
                weight,
                blend: BLEND,
                flag: None,
                restart: false,
                knob: false,
                limited: false,
            },
        );
    }
    anim.set_node(
        RUN_KNOB,
        NodeGoal {
            weight: 1.0,
            blend: BLEND,
            flag: Some("done"),
            restart: false,
            knob: true,
            limited: false,
        },
    );
}

fn look_goals(anim: &mut ActorAnim, nodes: [u16; 4]) {
    clear_looks(anim);
    for node in nodes {
        anim.set_node(
            node,
            NodeGoal {
                weight: 1.0,
                blend: 0.0,
                flag: None,
                restart: false,
                knob: false,
                limited: true,
            },
        );
    }
}

fn clear_looks(anim: &mut ActorAnim) {
    for knob in LOOK_KNOBS {
        anim.clear_node(knob, 0.0);
    }
}

/// The aim weights on the four look knobs.
fn aim_goals(anim: &mut ActorAnim, aim: [f32; 2]) {
    let [up, left] = aim;
    let goals = [
        (0x2a, up.max(0.0)),
        (0x29, (-up).max(0.0)),
        (0x2c, left.max(0.0)),
        (0x2b, (-left).max(0.0)),
    ];
    for (node, weight) in goals {
        anim.set_node(
            node,
            NodeGoal {
                weight,
                blend: AIM_BLEND,
                flag: None,
                restart: false,
                knob: false,
                limited: true,
            },
        );
    }
}

/// Leaving a traverse fades its animation out.
fn clear_traverses(anim: &mut ActorAnim, previous: u32) {
    if (40..=46).contains(&previous) {
        for node in TRAVERSES {
            anim.clear_node(node, WALK_BLEND);
        }
    }
}

fn from_stopped(anim: &mut ActorAnim, previous: u32) {
    anim.clear_node(RUN_START, 0.0);
    anim.clear_node(RUN_STOP, 0.0);
    anim.clear_node(RUN_TURN_180_LEFT, BLEND);
    anim.clear_node(RUN_TURN_180_RIGHT, BLEND);
    clear_traverses(anim, previous);
}

fn turn(anim: &mut ActorAnim, node: u16) {
    anim.clear_node(ROOT, BLEND);
    anim.clear_node(RUN_STOP, BLEND);
    for turn in TURNS {
        anim.clear_node(turn, 0.0);
    }
    play(anim, node, BLEND, true);
}

fn stand(anim: &mut ActorAnim) {
    anim.clear_node(ROOT, BLEND);
    anim.clear_node(IDLE, BLEND);
    anim.clear_node(ATTACKIDLE_KNOB, BLEND);
    look_goals(anim, ATTACK_LOOKS);
}

fn stand_idle(anim: &mut ActorAnim, node: u16, previous: u32) {
    if !(14..=17).contains(&previous) {
        stand(anim);
    }
    anim.clear_node(IDLE, BLEND);
    anim.clear_node(ATTACKIDLE_KNOB, QUICK);
    play(anim, node, BLEND, true);
}

fn enter(anim: &mut ActorAnim, state: u32, previous: u32, aim: [f32; 2]) {
    match state {
        1 => {
            if previous != 4 {
                from_stopped(anim, previous);
                look_goals(anim, LOOKS);
            }
            run_goals(anim, 0.0);
            aim_goals(anim, aim);
        }
        2 => {
            anim.clear_node(ROOT, BLEND);
            anim.clear_node(RUN_STOP, 0.0);
            anim.set_node(
                RUN_START,
                NodeGoal {
                    weight: 1.0,
                    blend: BLEND,
                    flag: None,
                    restart: true,
                    knob: false,
                    limited: false,
                },
            );
            anim.set_node(
                RUN_START_KNOB,
                NodeGoal {
                    weight: 1.0,
                    blend: BLEND,
                    flag: Some("done"),
                    restart: true,
                    knob: true,
                    limited: false,
                },
            );
        }
        3 => {
            anim.clear_node(RUN_START, 0.0);
            anim.clear_node(RUN_KNOB, BLEND);
            play(anim, RUN_STOP, BLEND, true);
        }
        4 => {
            if previous != 1 {
                from_stopped(anim, previous);
            }
            play(anim, WALK, BLEND, true);
        }
        5 => turn(anim, 8),
        6 => turn(anim, 9),
        7 => turn(anim, 6),
        8 => turn(anim, 7),
        9 => turn(anim, 0xc),
        10 => turn(anim, 0xd),
        11 => turn(anim, 0xa),
        12 => turn(anim, 0xb),
        14 => {
            if !(14..=17).contains(&previous) {
                stand(anim);
            }
            anim.clear_node(ATTACKIDLE_KNOB, BLEND);
            play(anim, IDLE, BLEND, true);
        }
        15 => stand_idle(anim, ATTACKIDLE, previous),
        16 => stand_idle(anim, ATTACKIDLE + 1, previous),
        17 => stand_idle(anim, ATTACKIDLE + 2, previous),
        18 => one_shot(anim, 4, BLEND),
        // The front, back, left and right pains, then the running four.
        19..=22 => one_shot(anim, 0x12 + (state - 19) as u16, BLEND),
        23..=26 => one_shot(anim, 0xe + (state - 23) as u16, BLEND),
        27..=30 => one_shot(anim, 0x16 + (state - 27) as u16, BLEND),
        31 => one_shot(anim, 0x1a, BLEND),
        32..=34 => one_shot(anim, ATTACKIDLE + (state - 32) as u16, BLEND),
        35 => one_shot(anim, 5, BLEND),
        36 => one_shot(anim, 0x1e, QUICK),
        37 => {
            one_shot(anim, 0x1b, QUICK);
            look_goals(anim, LOOKS);
        }
        38 => one_shot(anim, 0x1c, QUICK),
        39 => one_shot(anim, 0x1d, QUICK),
        40..=45 => one_shot(anim, 0x1f + (state - 40) as u16, BLEND),
        46 => {
            anim.clear_node(ROOT, BLEND);
            run_through_hole(anim);
        }
        _ => {}
    }
}

fn run_through_hole(anim: &mut ActorAnim) {
    anim.set_node(
        RUN,
        NodeGoal {
            weight: 1.0,
            blend: BLEND,
            flag: None,
            restart: false,
            knob: false,
            limited: false,
        },
    );
    anim.set_node(
        RUN_KNOB,
        NodeGoal {
            weight: 1.0,
            blend: BLEND,
            flag: Some("done"),
            restart: false,
            knob: true,
            limited: false,
        },
    );
}

fn one_shot(anim: &mut ActorAnim, node: u16, clear: f32) {
    anim.clear_node(ROOT, clear);
    play(anim, node, BLEND, true);
}

/// The rule of the state while it stays.
fn stay(anim: &mut ActorAnim, state: u32, aim: [f32; 2]) {
    match state {
        1 => {
            run_goals(anim, 0.0);
            clear_looks(anim);
        }
        4 => {
            anim.clear_node(RUN_KNOB, WALK_BLEND);
            play(anim, WALK, BLEND, true);
        }
        14..=17 | 37 => aim_goals(anim, aim),
        _ => {}
    }
}

/// Runs each dog's state rule before its tree advances, as its think does.
pub(crate) fn drive(world: &mut World) {
    let mut runtime = world.resource_mut::<RoundScript>();
    let dogs: Vec<u64> = runtime.dog_anim_states.keys().copied().collect();
    for actor in dogs {
        let Some(mut dog) = runtime.dog_anim_states.get(&actor).copied() else {
            continue;
        };
        let Some(anim) = runtime.actor_anims.get_mut(&actor) else {
            continue;
        };
        let state = dog.state & !TOGGLE;
        if dog.state == dog.applied && (state == 0 || dog.entered) {
            if dog.entered {
                stay(anim, state, dog.aim);
            }
        } else if !matches!(state, 0 | 13) {
            enter(anim, state, dog.applied & !TOGGLE, dog.aim);
            dog.applied = dog.state;
            dog.entered = true;
            runtime.dog_anim_states.insert(actor, dog);
        }
    }
}
