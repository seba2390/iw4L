//! The actor builtins of Black Ops' multiplayer server that its dogs use and
//! its zombies server has no version of, from BlackOpsMP.exe's method table.

use super::super::actor_dog_states;
use super::super::args::{arg, float, string};
use super::engine::entity_id;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};

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
