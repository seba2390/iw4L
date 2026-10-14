//! Black Ops 2's client builtins for client fields: the client's own
//! registrations (with the callbacks its game runs when a field changes) and
//! the values its snapshot carries.

use super::game::{ClientGame, FieldDef, client_entity, set_of_type};
use crate::script::host::args::{arg, float, int};
use crate::script::{Namespace, NativeRegistry, Value};
use bevy_ecs::prelude::World;

/// The client field sets by the index the engine gives each name; the client
/// game numbers `allplayers` 4.
const SETS: [(&str, i32); 11] = [
    ("world", 0),
    ("actor", 1),
    ("vehicle", 2),
    ("allplayers", 4),
    ("toplayer", 5),
    ("scriptmover", 6),
    ("helicopter", 7),
    ("plane", 8),
    ("turret", 9),
    ("missile", 10),
    ("zbarrier", 11),
];

/// A set's name as the client game knows it (case as registered), or none.
fn set_named(name: &str) -> Option<&'static str> {
    SETS.iter()
        .find(|(set, _)| set.eq_ignore_ascii_case(name))
        .map(|(set, _)| *set)
}

fn string(args: &[Value], index: usize) -> Result<String, String> {
    match arg(args, index)? {
        Value::String(text) => Ok(text.to_string()),
        Value::LocalizedString(text) => Ok(text.to_string()),
        other => Err(format!(
            "type {} is not a string",
            crate::script::host::args::kind(other)
        )),
    }
}

fn game(world: &mut World) -> bevy_ecs::prelude::Mut<'_, ClientGame> {
    world.get_resource_or_insert_with(ClientGame::default)
}

fn field_index(world: &mut World, set: &str, name: &str) -> Option<usize> {
    game(world)
        .fields
        .iter()
        .position(|field| field.pool == set && field.name == name)
}

/// The value the client's snapshot holds for a field, 0 before any.
fn snapshot_value(
    world: &mut World,
    set: &str,
    owner: Option<i32>,
    name: &str,
) -> Result<Value, String> {
    // The engine reads the field's record unchecked; a missing field is a
    // null record there.
    let index = field_index(world, set, name).ok_or_else(|| {
        format!("t6.csc.clientfield.missing: client field {set}:{name} is not registered")
    })?;
    let game = game(world);
    let field = &game.fields[index];
    let zero = if field.float {
        Value::Float(0.0)
    } else {
        Value::Int(0)
    };
    Ok(game.value(set, owner, index).unwrap_or(zero))
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::Function;

    // (set, name, version, bits, type, callback, host only, callback on zero,
    // float min, float max): a set the engine does not know and a bit count
    // outside 1..32 are parameter errors; registering after the client game
    // finalised its fields fails.
    registry.register(Function, "registerclientfield", |world, _, args| {
        let set = string(args, 0)?;
        let name = string(args, 1)?;
        let version = int(args, 2)?;
        let bits = int(args, 3)?;
        let kind = string(args, 4)?;
        let callback = match args.get(5) {
            None | Some(Value::Undefined) => Value::Undefined,
            Some(value @ Value::Function(_)) => value.clone(),
            Some(other) => {
                return Err(format!(
                    "type {} is not a function",
                    crate::script::host::args::kind(other)
                ));
            }
        };
        let Some(pool) = set_named(&set) else {
            return Err(format!("{set} not a valid name for a clientfield set."));
        };
        if !(1..=32).contains(&bits) {
            return Err(format!(
                "RegisterClientField can only accept bit ranges between 1 and 32.  {bits} requested."
            ));
        }
        let host_only = args.len() > 6 && int(args, 6)? == 1;
        let zero_callback = args.len() > 7 && int(args, 7)? == 1;
        let min = if args.len() > 8 { float(args, 8)? } else { 0.0 };
        let max = if args.len() > 9 { float(args, 9)? } else { 1.0 };
        let float_type = match kind.as_str() {
            "int" => false,
            "float" => true,
            _ => return Err("Unknown type passed to BG_GetClientFieldTypeFromTypeName".into()),
        };
        let mut game = game(world);
        if game.fields_final {
            return Err(format!(
                "Attempt to register Client Field {name} post finalization of Client Field registration period failed."
            ));
        }
        let def = FieldDef {
            pool: pool.to_owned(),
            name: name.clone(),
            version: version.abs(),
            bits,
            float: float_type,
            callback,
            host_only,
            zero_callback,
            min,
            max,
            code: false,
        };
        match game
            .fields
            .iter()
            .position(|field| field.pool == pool && field.name == name)
        {
            Some(index) => game.fields[index] = def,
            None => {
                if game.fields.iter().filter(|field| field.pool == pool).count() >= 0xa0 {
                    return Err(format!(
                        "Failed to alloc client field {pool}:{name} - MAX_CLIENTFIELD_FIELDS_IN_SET exceeded."
                    ));
                }
                game.fields.push(def);
            }
        }
        Ok(Value::Undefined)
    });

    // (entity, name): the field of the entity's set as its snapshot holds it.
    registry.register(Function, "codegetclientfield", |world, _, args| {
        let entity = arg(args, 0)?.clone();
        let Some((number, found)) = client_entity(world, &entity) else {
            return Err("not an entity".into());
        };
        let Some(set) = set_of_type(found.etype) else {
            return Err("Unknown client field set.".into());
        };
        let name = string(args, 1)?;
        snapshot_value(world, set, Some(number), &name)
    });
    registry.register(Function, "codegetworldclientfield", |world, _, args| {
        let name = string(args, 0)?;
        snapshot_value(world, "world", None, &name)
    });
    // (player, name): the player's own fields, from the local client's
    // player state.
    registry.register(
        Function,
        "codegetplayerstateclientfield",
        |world, _, args| {
            let entity = arg(args, 0)?.clone();
            let number = client_entity(world, &entity).map(|(number, _)| number);
            let name = string(args, 1)?;
            snapshot_value(world, "toplayer", number, &name)
        },
    );
    // (set, name): the version the field registered with, 0 for none.
    registry.register(Function, "getclientfieldversion", |world, _, args| {
        let set = string(args, 0)?;
        let name = string(args, 1)?;
        let Some(pool) = set_named(&set) else {
            return Ok(Value::Int(0));
        };
        let game = game(world);
        Ok(Value::Int(
            game.fields
                .iter()
                .find(|field| field.pool == pool && field.name == name)
                .map_or(0, |field| field.version),
        ))
    });
    // The highest client field version the server's game state carries.
    registry.register(
        Function,
        "getserverhighestclientfieldversion",
        |world, _, _| {
            let version = crate::script::host::client_scripts::with_server(world, |server| {
                super::super::highest_client_field_version(server)
            })
            .unwrap_or(0);
            Ok(Value::Int(version))
        },
    );
    // (set, count, names…): the named fields' changes go to the client's
    // menus (LUI) as events with the old and new values, instead of a script
    // callback.
    for name in [
        "setupclientfieldcodecallbacks",
        "setupclientfieldanimspeedcallbacks",
    ] {
        registry.register(Function, name, |world, _, args| {
            let set = string(args, 0)?;
            let count = int(args, 1)?;
            let pool = set_named(&set);
            for at in 0..count.max(0) as usize {
                let field = string(args, at + 2)?;
                let Some(pool) = pool else { continue };
                if let Some(index) = field_index(world, pool, &field) {
                    game(world).fields[index].code = true;
                }
            }
            Ok(Value::Undefined)
        });
    }
    // On an entity: runs its field callbacks again as for a new entity.
    registry.register(
        Namespace::Method,
        "processclientfieldsasifnew",
        |world, receiver, _| {
            let Some((number, _)) = client_entity(world, receiver) else {
                return Err("not an entity".into());
            };
            game(world).renew.push(number);
            Ok(Value::Undefined)
        },
    );
}
