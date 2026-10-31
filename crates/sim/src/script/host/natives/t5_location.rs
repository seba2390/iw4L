//! Black Ops multiplayer's location selector: `beginLocation…Selection`
//! opens the client's full map with a selector of a type (generic,
//! artillery, airstrike, mortar, napalm, comlink) and a radius, which the
//! client sends back as `confirm_location`; `precacheLocationSelector`
//! names the selector materials.

use super::super::args::{float, string};
use crate::frame::FrameWorld;
use crate::script::Namespace::{Function, Method};
use crate::script::{NativeRegistry, RoundScript, Value};
use crate::world::ClientId;
use bevy_ecs::prelude::{Resource, World};

const SELECTORS: usize = 15;
/// The radius without one given, as a share of the map's height.
const DEFAULT_RADIUS: f32 = 0.15;
/// The client gets the radius in fifteenths.
const RADIUS_STEPS: f32 = 15.0;

/// The precached selector materials.
#[derive(Resource, Debug, Default)]
pub(crate) struct LocationSelectors(Vec<String>);

fn begin(world: &mut World, receiver: &Value, args: &[Value], kind: u8) -> Result<Value, String> {
    let id = super::engine::entity_id(world, receiver).map_err(|_| "not an entity".to_owned())?;
    let runtime = world.resource::<RoundScript>();
    let client = runtime.player_client(id).ok_or_else(|| {
        let number = runtime.entities.get(&id).map_or(-1, |e| e.number);
        format!("entity {number} is not a player")
    })?;
    let material = string(args, 0)?.to_string();
    let share = if args.len() > 1 {
        let radius = float(args, 1)?;
        if radius <= 0.0 {
            return Err("Radius of location selector must be greater than zero\n".into());
        }
        let height = runtime.engine.minimap.map_or(0.0, |map| map.size[1]);
        let share = if height > 0.0 {
            radius / height
        } else {
            radius * 0.001
        };
        share.clamp(0.0, 1.0)
    } else {
        DEFAULT_RADIUS
    };
    let radius = (share * RADIUS_STEPS + 0.5).floor() / RADIUS_STEPS;
    let aspect = runtime
        .engine
        .minimap
        .map_or(1.0, |map| map.size[0] / map.size[1]);
    let mut frame = FrameWorld::from_world(world);
    if frame.client_meta(ClientId(client)).is_some() {
        frame.client_meta_mut(ClientId(client)).location_selection =
            Some(crate::LocationSelection {
                material,
                choose_direction: matches!(kind, 2 | 4),
                radius,
                kind: Some(kind),
                aspect,
            });
    }
    Ok(Value::Undefined)
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    macro_rules! selectors {
        ($($name:literal => $kind:literal),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, args| {
                begin(world, receiver, args, $kind)
            });
        )*};
    }
    selectors!(
        "beginlocationselection" => 0,
        "beginlocationartilleryselection" => 1,
        "beginlocationairstrikeselection" => 2,
        "beginlocationmortarselection" => 3,
        "beginlocationnapalmselection" => 4,
        "beginlocationcomlinkselection" => 5,
    );
    registry.register(Function, "precachelocationselector", |world, _, args| {
        let name = string(args, 0)?.to_string();
        let selectors = &mut world
            .get_resource_or_insert_with(LocationSelectors::default)
            .into_inner()
            .0;
        if selectors.contains(&name) {
            diag::warn!(
                Sim,
                "Script tried to precache the location selector '{name}' more than once"
            );
            return Ok(Value::Undefined);
        }
        if selectors.len() >= SELECTORS {
            return Err(format!(
                "Too many location selectors precached. Max allowed is {SELECTORS}"
            ));
        }
        selectors.push(name);
        Ok(Value::Undefined)
    });
}
