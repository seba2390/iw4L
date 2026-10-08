use bevy_ecs::prelude::World;

use crate::script::runtime::{raise, run_now};
use crate::script::{Runtime, Value};

/// The animscript an actor runs; the engine picks it, the script plays it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AnimScript {
    Stop,
    Move,
    Combat,
}

impl AnimScript {
    fn module(self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::Move => "move",
            Self::Combat => "combat",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ActorBrain {
    /// Animscript family, `zombie_` or `zombie_dog_`.
    prefix: &'static str,
    script: Option<AnimScript>,
}

/// An enemy this close is fought rather than approached.
const MELEE_RANGE: f32 = 64.0;

/// Animscript modules the zombie actors may run.
pub const ANIMSCRIPT_MODULES: &[&str] = &[
    "animscripts/zombie_init",
    "animscripts/zombie_stop",
    "animscripts/zombie_move",
    "animscripts/zombie_combat",
    "animscripts/zombie_death",
    "animscripts/zombie_pain",
    "animscripts/zombie_dog_init",
    "animscripts/zombie_dog_stop",
    "animscripts/zombie_dog_move",
    "animscripts/zombie_dog_combat",
    "animscripts/zombie_dog_death",
    "animscripts/zombie_dog_pain",
];

/// Runs the actor's animscript init and takes charge of its animscripts.
pub(crate) fn begin(world: &mut World, actor: u64) -> Result<(), String> {
    let prefix = match world.resource_mut::<Runtime>().object_field(actor, "type") {
        Value::String(kind) if kind.as_bytes().starts_with(b"zombie_dog") => "zombie_dog_",
        _ => "zombie_",
    };
    let now = super::players::now_ms(world);
    let init = format!("animscripts/{prefix}init::main");
    run_now(world, &init, Value::Object(actor), Vec::new(), now)
        .map_err(|fault| format!("{init}: {fault:?}"))?;
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.actor_brains.insert(
        actor,
        ActorBrain {
            prefix,
            script: None,
        },
    );
    runtime.actor_moves.entry(actor).or_default();
    Ok(())
}

/// Switches each actor to the animscript its situation calls for, ending
/// the previous one with `killanimscript`.
pub(crate) fn think(world: &mut World) {
    let actors: Vec<(u64, ActorBrain)> = world
        .resource::<Runtime>()
        .actor_brains
        .iter()
        .map(|(actor, brain)| (*actor, brain.clone()))
        .collect();
    for (actor, brain) in actors {
        let wanted = if super::actor_nav::melee_range_enemy(world, actor, MELEE_RANGE) {
            AnimScript::Combat
        } else if world
            .resource::<Runtime>()
            .actor_moves
            .get(&actor)
            .is_some_and(super::actor_nav::ActorMove::has_path)
        {
            AnimScript::Move
        } else {
            AnimScript::Stop
        };
        if brain.script == Some(wanted) {
            continue;
        }
        if brain.script.is_some() {
            raise(world, Value::Object(actor), "killanimscript", Vec::new());
        }
        let main = format!("animscripts/{}{}::main", brain.prefix, wanted.module());
        if let Err(fault) = crate::script::start(world, &main, Value::Object(actor), Vec::new()) {
            diag::warn!(Sim, "actor animscript {main}: {fault:?}");
        }
        if let Some(brain) = world.resource_mut::<Runtime>().actor_brains.get_mut(&actor) {
            brain.script = Some(wanted);
        }
    }
}
