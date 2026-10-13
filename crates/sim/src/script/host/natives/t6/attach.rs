//! Black Ops 2's model attachments: 19 slots per entity, kept packed (a
//! detach moves the later ones down).

use super::{int, string};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;
use std::sync::Arc;

const SLOTS: usize = 19;

fn entity(world: &World, receiver: &Value) -> Result<u64, String> {
    super::super::engine::entity_id(world, receiver).map_err(|_| "not an entity".into())
}

/// The model and the tag (`""` when not given).
fn model_and_tag(args: &[Value]) -> Result<(Arc<str>, Arc<str>), String> {
    let model = string(args, 0)?;
    let tag = if args.len() < 2 {
        String::new()
    } else {
        string(args, 1)?
    };
    Ok((model.into(), tag.into()))
}

fn attached(attachments: &[(Arc<str>, Arc<str>)], model: &str, tag: &str) -> Option<usize> {
    if model.is_empty() {
        return None;
    }
    attachments
        .iter()
        .position(|(m, t)| m.eq_ignore_ascii_case(model) && t.eq_ignore_ascii_case(tag))
}

fn slot(world: &World, receiver: &Value, args: &[Value]) -> Result<(Arc<str>, Arc<str>), String> {
    let id = entity(world, receiver)?;
    let index = int(args, 0)?;
    world.resource::<RoundScript>().entities[&id]
        .attachments
        .get(usize::try_from(index).unwrap_or(SLOTS))
        .filter(|_| (index as u32 as usize) < SLOTS)
        .cloned()
        .ok_or_else(|| "bad index".into())
}

/// `attach(model, [tag], [ignore collision])`: the tag it went on.
pub(super) fn attach(world: &mut World, id: u64, args: &[Value]) -> Result<Arc<str>, String> {
    let (model, tag) = model_and_tag(args)?;
    if args.len() > 2 {
        int(args, 2)?;
    }
    let mut runtime = world.resource_mut::<RoundScript>();
    let attachments = &mut runtime
        .entities
        .get_mut(&id)
        .ok_or("not an entity")?
        .attachments;
    if attached(attachments, &model, &tag).is_some() {
        return Err(format!("model '{model}' already attached to tag '{tag}'"));
    }
    if model.is_empty() || attachments.len() >= SLOTS {
        return Err(format!("failed to attach model '{model}' to tag '{tag}'"));
    }
    attachments.push((model, tag.clone()));
    Ok(tag)
}

/// A model that is not attached is reported on the console only.
pub(super) fn detach(world: &mut World, id: u64, args: &[Value]) -> Result<Value, String> {
    let (model, tag) = model_and_tag(args)?;
    let mut runtime = world.resource_mut::<RoundScript>();
    let attachments = &mut runtime
        .entities
        .get_mut(&id)
        .ok_or("not an entity")?
        .attachments;
    match attached(attachments, &model, &tag) {
        Some(index) => {
            attachments.remove(index);
        }
        None => diag::info!(
            Sim,
            "detach: model '{model}' is not attached to tag '{tag}'"
        ),
    }
    Ok(Value::Undefined)
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::Method;

    registry.register(Method, "attach", |world, receiver, args| {
        let id = entity(world, receiver)?;
        attach(world, id, args).map(|_| Value::Undefined)
    });
    registry.register(Method, "detach", |world, receiver, args| {
        let id = entity(world, receiver)?;
        detach(world, id, args)
    });
    registry.register(Method, "detachall", |world, receiver, _| {
        let id = entity(world, receiver)?;
        if let Some(entity) = world.resource_mut::<RoundScript>().entities.get_mut(&id) {
            entity.attachments.clear();
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "getattachsize", |world, receiver, _| {
        let id = entity(world, receiver)?;
        let count = world.resource::<RoundScript>().entities[&id]
            .attachments
            .len();
        Ok(Value::Int(count.min(SLOTS) as i32))
    });
    registry.register(Method, "getattachmodelname", |world, receiver, args| {
        let (model, _) = slot(world, receiver, args)?;
        Ok(Value::String(model.into()))
    });
    registry.register(Method, "getattachtagname", |world, receiver, args| {
        let (_, tag) = slot(world, receiver, args)?;
        Ok(Value::String(tag.into()))
    });
}
