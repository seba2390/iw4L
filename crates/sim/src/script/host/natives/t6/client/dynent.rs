//! Black Ops 2's dynamic entities as client scripts see them:
//! `getdynentarray` hands the script each of the map's dynamic entities,
//! `setdynentenabled` turns one off or on.

use super::super::{int, type_name};
use crate::script::host::arrays::new_array;
use crate::script::{MapDynEnt, Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};
use std::collections::{BTreeMap, BTreeSet};

/// The map's dynamic entities and what the client's scripts did with them.
#[derive(Resource, Default)]
pub(crate) struct ClientDynEnts {
    /// Model ones, then brush ones, as the engine walks its two pools.
    pub(crate) map: Vec<MapDynEnt>,
    /// The script entity of each one handed out, by its place in `map`.
    objects: BTreeMap<usize, u64>,
    /// The ones the scripts turned off, by place in `map`.
    pub(crate) disabled: BTreeSet<usize>,
}

impl ClientDynEnts {
    pub(crate) fn new(map: Vec<MapDynEnt>) -> Self {
        Self {
            map,
            ..Default::default()
        }
    }
}

/// The script entity standing for the dynamic entity at `place`, made the
/// first time it is asked for.
fn object(world: &mut World, place: usize) -> Result<u64, String> {
    if let Some(&id) = world.resource::<ClientDynEnts>().objects.get(&place) {
        return Ok(id);
    }
    let origin = world.resource::<ClientDynEnts>().map[place].origin;
    let id = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let id = runtime.new_object()?;
        runtime.set_object_field(id, "origin", Value::Vector(origin));
        id
    };
    world
        .resource_mut::<ClientDynEnts>()
        .objects
        .insert(place, id);
    Ok(id)
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::Function;

    // Every dynamic entity of both pools, in pool order. A name narrows the
    // walk to those whose definition names it, a field IW4L does not read
    // from the map (t6.csc.dynent.name).
    registry.register(Function, "getdynentarray", |world, _, args| {
        if !args.is_empty() {
            return Err(
                "t6.csc.dynent.name: the name a dynamic entity's definition carries is not read"
                    .into(),
            );
        }
        let count = world.resource::<ClientDynEnts>().map.len();
        let objects = (0..count)
            .map(|place| object(world, place).map(Value::Object))
            .collect::<Result<Vec<_>, _>>()?;
        new_array(world, objects)
    });
    // One argument turns the entity off; a second turns it on when not 0.
    // The renderer does not read the state yet: the entity still draws
    // and collides (t6.csc.dynent.enabled).
    registry.register(Function, "setdynentenabled", |world, _, args| {
        let id = match args.first() {
            Some(Value::Object(id)) => *id,
            Some(other) => return Err(format!("{} is not a dynamic entity", type_name(other))),
            None => return Err("setdynentenabled: a dynamic entity is needed".into()),
        };
        let enabled = args.len() > 1 && int(args, 1)? != 0;
        let mut dyn_ents = world.resource_mut::<ClientDynEnts>();
        let place = dyn_ents
            .objects
            .iter()
            .find_map(|(&place, &object)| (object == id).then_some(place))
            .ok_or("setdynentenabled: not a dynamic entity")?;
        if enabled {
            dyn_ents.disabled.remove(&place);
        } else {
            dyn_ents.disabled.insert(place);
        }
        Ok(Value::Undefined)
    });
}
