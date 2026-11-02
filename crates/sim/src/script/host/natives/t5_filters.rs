//! Black Ops' client filter and overlay builtins (`clientscripts/_filter`):
//! the client keeps the script's materials by slot, three filters of
//! sixteen passes (material, target, two samplers, quads, sixteen
//! constants, enabled) and three overlays (material, sort, sixteen
//! constants, enabled), which its renderer and its 2D draw read each frame.
//! Neither draw is ported, so only what the builtins themselves check is
//! kept (the mapped materials, each pass's and overlay's material); a pass
//! or overlay a script enables logs `t5.csc.filters`.

use super::super::args::{float, int, string};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};

const MATERIALS: usize = 64;
const FILTERS: usize = 3;
const PASSES: usize = 16;
const CONSTANTS: usize = 16;
const OVERLAYS: usize = 3;
/// The pass tables are one array of `FILTERS * PASSES` in the client;
/// `set_filter_pass_enabled` and `_quads` take pass 16, which is the next
/// filter's pass 0 (the last filter's lands one past the end).
const LAST_FLAT_PASS: usize = PASSES;

/// The local client's filter state.
#[derive(Resource, Clone, Debug)]
pub(crate) struct ClientFilters {
    materials: [Option<String>; MATERIALS],
    /// Each pass's material, by filter and pass.
    passes: [Option<String>; FILTERS * PASSES],
    /// Each overlay's material.
    overlays: [Option<String>; OVERLAYS],
    /// The passes and overlays already reported as not drawn.
    reported: std::collections::BTreeSet<(bool, usize)>,
}

impl Default for ClientFilters {
    fn default() -> Self {
        Self {
            materials: std::array::from_fn(|_| None),
            passes: std::array::from_fn(|_| None),
            overlays: std::array::from_fn(|_| None),
            reported: Default::default(),
        }
    }
}

fn filters(world: &mut World) -> &mut ClientFilters {
    world
        .get_resource_or_insert_with(ClientFilters::default)
        .into_inner()
}

/// The receiver must be an entity.
fn entity(world: &World, receiver: &Value) -> Result<(), String> {
    world
        .resource::<RoundScript>()
        .entity(receiver)
        .map(|_| ())
        .ok_or_else(|| "not an entity".into())
}

fn within(value: i32, last: usize) -> bool {
    usize::try_from(value).is_ok_and(|value| value <= last)
}

fn report(world: &mut World, overlay: bool, index: usize, what: String) {
    if filters(world).reported.insert((overlay, index)) {
        diag::warn!(
            Sim,
            "t5.csc.filters: {what} is enabled, but Black Ops' filter and overlay draws are not ported"
        );
    }
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    use Namespace::Method;
    registry.register(Method, "map_material", |world, receiver, args| {
        entity(world, receiver)?;
        let index = int(args, 0)?;
        let name = string(args, 1)?;
        if !within(index, MATERIALS - 1) || args.len() != 2 {
            return Err(
                "USAGE: ent map_material( <material index 0-63 >, <material name>)\n".into(),
            );
        }
        filters(world).materials[index as usize] = Some(name);
        Ok(Value::Undefined)
    });
    registry.register(Method, "set_filter_pass_material", |world, receiver, args| {
        const USAGE: &str = "USAGE: ent set_filter_pass_material( <filterindex 0-3>, <passindex 0-15>, <materialindex 0-63>, <targetindex 0-4>, <sampler0index 0-4>, <sampler0index 0-4> )";
        entity(world, receiver)?;
        let filter = int(args, 0)?;
        let pass = int(args, 1)?;
        let material = int(args, 2)?;
        let target = int(args, 3)?;
        let sampler0 = int(args, 4)?;
        let sampler1 = int(args, 5)?;
        if args.len() != 6
            || !within(filter, FILTERS - 1)
            || !within(pass, PASSES - 1)
            || !within(material, MATERIALS - 1)
            || !within(target, 5)
            || !within(sampler0, 5)
            || !within(sampler1, 5)
        {
            return Err(format!("{USAGE}\n"));
        }
        let state = filters(world);
        let Some(name) = state.materials[material as usize].clone() else {
            return Err(format!("{USAGE} - no material at that index\n"));
        };
        state.passes[filter as usize * PASSES + pass as usize] = Some(name);
        Ok(Value::Undefined)
    });
    registry.register(Method, "set_filter_pass_enabled", |world, receiver, args| {
        entity(world, receiver)?;
        let filter = int(args, 0)?;
        let pass = int(args, 1)?;
        let enabled = int(args, 2)? != 0;
        if args.len() != 3 || !within(filter, FILTERS - 1) || !within(pass, LAST_FLAT_PASS) {
            return Err(
                "USAGE: ent set_filter_pass_enabled( <filterindex 0-3>, <passindex 0-16>, <enabled true/false> )\n"
                    .into(),
            );
        }
        let at = filter as usize * PASSES + pass as usize;
        if enabled {
            let material = filters(world)
                .passes
                .get(at)
                .cloned()
                .flatten()
                .unwrap_or_default();
            report(
                world,
                false,
                at,
                format!("filter {filter} pass {pass} ({material})"),
            );
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "set_filter_pass_quads", |world, receiver, args| {
        entity(world, receiver)?;
        let filter = int(args, 0)?;
        let pass = int(args, 1)?;
        let quads = int(args, 2)?;
        if args.len() != 3
            || !within(filter, FILTERS - 1)
            || !within(pass, LAST_FLAT_PASS)
            || !within(quads, 400)
        {
            return Err(
                "USAGE: ent set_filter_pass_quads( <filterindex 0-3>, <passindex 0-15>, <passquads 0-400> )\n"
                    .into(),
            );
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "set_filter_pass_constant", |world, receiver, args| {
        entity(world, receiver)?;
        let filter = int(args, 0)?;
        let pass = int(args, 1)?;
        let constant = int(args, 2)?;
        float(args, 3)?;
        if args.len() != 4
            || !within(filter, FILTERS - 1)
            || !within(pass, PASSES - 1)
            || !within(constant, CONSTANTS - 1)
        {
            return Err(
                "USAGE: ent set_filter_pass_constant( <filterindex 0-3>, <passindex 0-16>, <constantindex 0-16>, <value> )\n"
                    .into(),
            );
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "set_overlay_enabled", |world, receiver, args| {
        entity(world, receiver)?;
        let overlay = int(args, 0)?;
        let enabled = int(args, 1)? != 0;
        if args.len() != 2 || !within(overlay, OVERLAYS - 1) {
            return Err(
                "USAGE: ent set_overlay_enabled( <overlayindex 0-3>, <enable true/false> )\n"
                    .into(),
            );
        }
        if enabled {
            let material = filters(world).overlays[overlay as usize]
                .clone()
                .unwrap_or_default();
            report(
                world,
                true,
                overlay as usize,
                format!("overlay {overlay} ({material})"),
            );
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "set_overlay_material", |world, receiver, args| {
        const USAGE: &str = "USAGE: ent set_overlay_material( <overlayIndex 0-3>, <materialindex 0-63>, <sortindex 0-2> )";
        entity(world, receiver)?;
        let overlay = int(args, 0)?;
        let material = int(args, 1)?;
        let sort = int(args, 2)?;
        if args.len() != 3
            || !within(overlay, OVERLAYS - 1)
            || !within(material, MATERIALS - 1)
            || !within(sort, 2)
        {
            return Err(format!("{USAGE}\n"));
        }
        let state = filters(world);
        let Some(name) = state.materials[material as usize].clone() else {
            return Err(format!("{USAGE} - no material at that index\n"));
        };
        state.overlays[overlay as usize] = Some(name);
        Ok(Value::Undefined)
    });
    registry.register(Method, "set_overlay_constant", |world, receiver, args| {
        entity(world, receiver)?;
        let overlay = int(args, 0)?;
        let constant = int(args, 1)?;
        float(args, 2)?;
        if args.len() != 3 || !within(overlay, OVERLAYS - 1) || !within(constant, CONSTANTS - 1)
        {
            return Err(
                "USAGE: ent set_overlay_constant( <overlayindex 0-3>, <constantindex 0-16>, <value> )\n"
                    .into(),
            );
        }
        Ok(Value::Undefined)
    });
    // `player HasTacticalMaskOverlay()`: Black Ops' PC multiplayer client
    // answers false for every player, so `_tacticalmask.csc` never enables
    // its filter.
    registry.register(Method, "hastacticalmaskoverlay", |_, _, _| {
        Ok(Value::Int(0))
    });
}
