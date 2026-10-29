//! Black Ops multiplayer's objective builtins: 32 slots, each a state, a
//! whole-unit position or an entity it follows, a team and an icon.

use super::super::args::{arg, int, string, vector};
use super::super::objectives::{Team, objective};
use crate::ObjectiveState;
use crate::script::Namespace::Function;
use crate::script::{NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;

const SLOTS: i32 = 32;
const ICON_NAME_MAX: usize = 63;

fn index(args: &[Value]) -> Result<u8, String> {
    let index = int(args, 0)?;
    if !(0..SLOTS).contains(&index) {
        return Err(format!(
            "index {index} is an illegal objective index. Valid indexes are 0 to {}\n",
            SLOTS - 1
        ));
    }
    Ok(index as u8)
}

fn state(args: &[Value]) -> Result<ObjectiveState, String> {
    let name = string(args, 1)?;
    Ok(match name.as_str() {
        "empty" => ObjectiveState::Empty,
        "invisible" => ObjectiveState::Invisible,
        "current" => ObjectiveState::Current,
        "active" => ObjectiveState::Active,
        _ => {
            return Err(format!(
                "Illegal objective state \"{name}\". Valid states are \"empty\", \"invisible\", \"current\", \"active\"\n"
            ));
        }
    })
}

fn icon(args: &[Value], at: usize) -> Result<String, String> {
    let name = string(args, at)?;
    if let Some(c) = name.chars().find(|c| !(' '..='~').contains(c)) {
        return Err(format!(
            "Illegal character '{c}'(ascii {}) in objective icon name: {name}\n",
            c as u32
        ));
    }
    if name.len() > ICON_NAME_MAX {
        return Err(format!(
            "Objective icon name is too long (> {ICON_NAME_MAX}): {name}\n"
        ));
    }
    Ok(name)
}

/// A position is kept in whole units, each axis cut toward zero.
fn whole_units(args: &[Value], at: usize) -> Result<[f32; 3], String> {
    Ok(vector(args, at)?.map(|v| v as i32 as f32))
}

fn detach(world: &mut World, index: u8) {
    objective(world, index).entity = None;
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    // `objective_add(index, state, [position], [icon])` keeps the slot's
    // position and icon unless it is given new ones.
    registry.register(Function, "objective_add", |world, _, args| {
        if args.len() < 2 {
            return Err("objective_add needs at least the first two parameters out of its parameter list of: index state [string] [position]\n".into());
        }
        let index = index(args)?;
        {
            let mut row = objective(world, index);
            row.viewer = None;
            row.entity = None;
        }
        let state = state(args)?;
        objective(world, index).state = state;
        if args.len() > 2 {
            let origin = whole_units(args, 2)?;
            objective(world, index).origin = origin;
            if args.len() > 3 {
                let icon = icon(args, 3)?;
                objective(world, index).icon = icon;
            }
        }
        objective(world, index).team = Team::Free;
        Ok(Value::Undefined)
    });
    registry.register(Function, "objective_delete", |world, _, args| {
        let index = index(args)?;
        world
            .resource_mut::<RoundScript>()
            .engine
            .objectives
            .remove(&index);
        Ok(Value::Undefined)
    });
    // An empty or invisible objective stops following its entity.
    registry.register(Function, "objective_state", |world, _, args| {
        let index = index(args)?;
        let state = state(args)?;
        objective(world, index).state = state;
        if matches!(state, ObjectiveState::Empty | ObjectiveState::Invisible) {
            detach(world, index);
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "objective_icon", |world, _, args| {
        let index = index(args)?;
        let icon = icon(args, 1)?;
        objective(world, index).icon = icon;
        Ok(Value::Undefined)
    });
    registry.register(Function, "objective_position", |world, _, args| {
        let index = index(args)?;
        detach(world, index);
        let origin = whole_units(args, 1)?;
        objective(world, index).origin = origin;
        Ok(Value::Undefined)
    });
    registry.register(Function, "objective_onentity", |world, _, args| {
        let index = index(args)?;
        detach(world, index);
        let target = arg(args, 1)?.clone();
        super::engine::entity_id(world, &target)?;
        objective(world, index).entity = Some(target);
        Ok(Value::Undefined)
    });
    registry.register(Function, "objective_team", |world, _, args| {
        let index = index(args)?;
        let team = match string(args, 1)?.as_str() {
            "allies" => Team::Allies,
            "axis" => Team::Axis,
            "none" => Team::Free,
            other => {
                return Err(format!(
                    "Illegal team string '{other}'. Must be allies, axis, or none."
                ));
            }
        };
        objective(world, index).team = team;
        Ok(Value::Undefined)
    });
}
