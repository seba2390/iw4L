//! Black Ops multiplayer's target list: 32 entities the lock-on weapons
//! may pick (`Target_Set`) and whether turrets may acquire each. The aim
//! offset `Target_Set` takes is read but not kept: nothing that aims at a
//! target is ported yet (ledger `t5.mp.target_offset`).

use super::super::args::vector;
use crate::script::Namespace::Function;
use crate::script::{NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};

const SLOTS: usize = 32;
/// Turrets do not acquire the target (`Target_SetTurretAquire(ent, false)`).
const NO_TURRET_ACQUIRE: u32 = 4;

#[derive(Clone, Copy, Debug)]
struct Target {
    entity: u64,
    flags: u32,
}

#[derive(Resource, Debug, Default)]
pub(crate) struct TargetList([Option<Target>; SLOTS]);

impl TargetList {
    fn slot(&self, entity: u64) -> Option<usize> {
        self.0
            .iter()
            .position(|t| t.is_some_and(|t| t.entity == entity))
    }
}

fn list(world: &mut World) -> &mut TargetList {
    world
        .get_resource_or_insert_with(TargetList::default)
        .into_inner()
}

fn entity(world: &World, args: &[Value]) -> Result<u64, String> {
    let value = args.first().ok_or("Too few arguments\n")?;
    super::engine::entity_id(world, value)
}

fn set(world: &mut World, _: &Value, args: &[Value]) -> Result<Value, String> {
    let entity = entity(world, args)?;
    if args.len() >= 2 {
        vector(args, 1)?;
    }
    let targets = list(world);
    let index = match targets.slot(entity) {
        Some(index) => index,
        None => {
            if targets.0.iter().flatten().count() >= SLOTS {
                return Err("Maximum number of targets exceeded".into());
            }
            let index = targets.0.iter().position(Option::is_none).unwrap();
            targets.0[index] = Some(Target { entity, flags: 0 });
            index
        }
    };
    targets.0[index].as_mut().unwrap().flags &= !NO_TURRET_ACQUIRE;
    Ok(Value::Undefined)
}

fn remove(world: &mut World, _: &Value, args: &[Value]) -> Result<Value, String> {
    let entity = entity(world, args)?;
    let targets = list(world);
    match targets.slot(entity) {
        Some(index) => {
            targets.0[index] = None;
            Ok(Value::Undefined)
        }
        None => {
            let runtime = world.resource::<RoundScript>();
            let number = runtime.entities.get(&entity).map_or(-1, |e| e.number);
            Err(format!("Entity {number} is not a target"))
        }
    }
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Function, "target_set", set);
    registry.register(Function, "target_remove", remove);
    registry.register(Function, "target_istarget", |world, _, args| {
        let entity = entity(world, args)?;
        Ok(Value::Int(list(world).slot(entity).is_some().into()))
    });
    // Targets whose entity is still in use.
    registry.register(Function, "target_getarray", |world, _, _| {
        let entities: Vec<u64> = list(world).0.iter().flatten().map(|t| t.entity).collect();
        let runtime = world.resource::<RoundScript>();
        let live: Vec<Value> = entities
            .into_iter()
            .filter(|id| runtime.live(id))
            .map(Value::Object)
            .collect();
        super::super::arrays::new_array(world, live)
    });
    registry.register(Function, "target_setturretaquire", |world, _, args| {
        let entity = entity(world, args)?;
        let acquire = args.len() < 2 || super::super::args::int(args, 1)? != 0;
        let targets = list(world);
        if let Some(target) = targets.slot(entity).and_then(|i| targets.0[i].as_mut()) {
            if acquire {
                target.flags &= !NO_TURRET_ACQUIRE;
            } else {
                target.flags |= NO_TURRET_ACQUIRE;
            }
        }
        Ok(Value::Undefined)
    });
}
