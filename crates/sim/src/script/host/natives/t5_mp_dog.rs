//! The actor builtins of Black Ops' multiplayer server that its dogs use and
//! its zombies server has no version of, from BlackOpsMP.exe's method table.

use super::super::actor_dog_states;
use super::super::args::{arg, float, int, string, vector};
use super::engine::entity_id;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};

/// `heliTurretDogTrace`'s aim above the dog's origin, and what blocks it.
const DOG_TRACE_HEIGHT: f32 = 24.0;
const DOG_TRACE_CONTENTS: u32 = 0x3801;

pub(crate) fn register(registry: &mut NativeRegistry) {
    use Namespace::Method;
    // `spawner spawnactor()`: a spawn from an actor spawner, at once whatever
    // stands there.
    registry.register(Method, "spawnactor", |world, receiver, _| {
        super::super::actors::spawn_from(world, receiver, true)
    });
    // `setentityowner(owner)`: its owner is no target of its own.
    registry.register(Method, "setentityowner", |world, receiver, args| {
        let actor = entity_id(world, receiver)?;
        let owner = arg(args, 0)?.clone();
        world
            .resource_mut::<RoundScript>()
            .set_object_field(actor, "script_owner", owner);
        Ok(Value::Undefined)
    });
    registry.register(Method, "clearentityowner", |world, receiver, _| {
        let actor = entity_id(world, receiver)?;
        world.resource_mut::<RoundScript>().set_object_field(
            actor,
            "script_owner",
            Value::Undefined,
        );
        Ok(Value::Undefined)
    });
    // `clearentitytarget()`: its handler drops the script's entity target, which
    // nothing in the multiplayer scripts sets.
    registry.register(Method, "clearentitytarget", |world, receiver, _| {
        entity_id(world, receiver)?;
        Ok(Value::Undefined)
    });
    // `finishactordamage(inflictor, attacker, damage, flags, means, weapon,
    // point, dir, hitLoc, timeOffset)`: the same as the zombies server's,
    // less the model index.
    registry.register(Method, "finishactordamage", |world, receiver, args| {
        let actor = entity_id(world, receiver)?;
        let mut args = args.to_vec();
        args.truncate(9);
        args.push(Value::Int(0));
        super::super::actors::finish_damage(world, actor, &args)
    });
    // `heli heliTurretDogTrace(start, dog, lastHit)`: 0 when nothing in the
    // world (contents 0x3801) stands between the start and 24 units above
    // the dog's origin, the helicopter itself passed through. The handler
    // first retries what blocked last time (`lastHit`), a cache; this
    // answers 1 for any block, which the scripts only test against 0.
    registry.register(Method, "heliturretdogtrace", |world, receiver, args| {
        let heli = entity_id(world, receiver)?;
        let start = vector(args, 0)?;
        let dog = entity_id(world, arg(args, 1)?)?;
        int(args, 2)?;
        let Value::Vector(origin) = world
            .resource_mut::<RoundScript>()
            .object_field(dog, "origin")
        else {
            return Err("heliturretdogtrace: the dog has no origin".into());
        };
        let end = [origin[0], origin[1], origin[2] + DOG_TRACE_HEIGHT];
        let tick = world.resource::<crate::step::StepRequest>().tick;
        let clear = crate::game_fire::sight_clear(
            &mut crate::frame::FrameWorld::from_world(world),
            tick,
            start,
            end,
            [heli, heli],
            DOG_TRACE_CONTENTS,
        );
        Ok(Value::Int(i32::from(!clear)))
    });
    registry.register(Method, "setanimstate", |world, receiver, args| {
        let actor = entity_id(world, receiver)?;
        let name = string(args, 0)?;
        actor_dog_states::set_state(&mut world.resource_mut::<RoundScript>(), actor, &name);
        Ok(Value::Undefined)
    });
    registry.register(Method, "setaimanimweights", |world, receiver, args| {
        let actor = entity_id(world, receiver)?;
        let aim = [float(args, 0)?, float(args, 1)?];
        actor_dog_states::set_aim(&mut world.resource_mut::<RoundScript>(), actor, aim);
        Ok(Value::Undefined)
    });
}

/// The dog builtins of its client VM.
pub(crate) fn register_client(registry: &mut NativeRegistry) {
    use Namespace::Method;
    // `GetAnimStateCategory()`: the category of the animation state the
    // actor's entity state carries; an actor that never set one is in state
    // 0, `init`.
    registry.register(Method, "getanimstatecategory", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        let actor = world
            .resource::<RoundScript>()
            .entities
            .get(&id)
            .is_some_and(|entity| {
                matches!(
                    entity.kind,
                    super::super::entities::EntityKind::Actor
                        | super::super::entities::EntityKind::ActorCorpse
                )
            });
        if !actor {
            return Err("GetAnimStateCategory() can only be called on actors".into());
        }
        let state = world
            .get_resource::<crate::script::client::ClientAnimStates>()
            .and_then(|states| states.0.get(&id).copied())
            .unwrap_or(0);
        Ok(Value::string(actor_dog_states::category(state)))
    });
}
