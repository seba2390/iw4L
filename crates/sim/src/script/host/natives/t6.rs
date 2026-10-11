//! Black Ops 2's builtins, each as its handler in the executable reads its
//! arguments and answers. Where the handler reads engine state IW4L keeps
//! differently, the choice is listed in `docs/fidelity/t6.md`.

use super::super::args::arg;
use super::super::arrays::new_array;
use super::super::entities::EntityKind;
use crate::script::{ArrayKey, Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

mod actor;
mod anim;
mod attach;
mod barrier;
mod effect;
mod movement;
mod mover;
mod sound;
mod stats;
mod weapon;

/// How Black Ops 2 finds a dvar: djb2 over the lowercased name.
fn dvar_hash(name: &str) -> u32 {
    name.bytes().fold(5381u32, |hash, byte| {
        hash.wrapping_mul(33)
            .wrapping_add(u32::from(byte.to_ascii_lowercase()))
    })
}

/// The value of the dvar an argument names, a hash (an int is taken as one)
/// or a name; a missing dvar reads as empty.
fn dvar(world: &World, args: &[Value]) -> Result<String, String> {
    let dvars = &world.resource::<crate::script::MatchScript>().dvars;
    let found = match arg(args, 0)? {
        Value::Int(hash) => {
            let hash = *hash as u32;
            dvars
                .iter()
                .find(|(name, _)| dvar_hash(name) == hash)
                .map(|(_, value)| value.clone())
        }
        Value::String(name) => dvars.get(&name.to_string().to_ascii_lowercase()).cloned(),
        other => return Err(format!("{} is not a dvar name", type_name(other))),
    };
    Ok(found.unwrap_or_else(String::new))
}

/// `atoi`: the leading decimal integer, 0 when there is none.
fn atoi(text: &str) -> i32 {
    let text = text.trim_start();
    let (sign, digits) = match text.as_bytes().first() {
        Some(b'-') => (-1i64, &text[1..]),
        Some(b'+') => (1, &text[1..]),
        _ => (1, text),
    };
    let value = digits
        .bytes()
        .take_while(u8::is_ascii_digit)
        .fold(0i64, |n, d| {
            (n * 10 + i64::from(d - b'0')).min(i64::from(u32::MAX))
        });
    (sign * value) as i32
}

/// `atof`: the leading decimal number, 0 when there is none.
fn atof(text: &str) -> f32 {
    let text = text.trim_start();
    let end = text
        .char_indices()
        .take_while(|&(i, c)| c.is_ascii_digit() || c == '.' || (i == 0 && (c == '-' || c == '+')))
        .last()
        .map_or(0, |(i, c)| i + c.len_utf8());
    text[..end].parse().unwrap_or(0.0)
}

/// Black Ops 2's name for a value's type.
fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Undefined => "undefined",
        Value::Int(_) => "int",
        Value::Float(_) => "float",
        Value::String(_) => "string",
        Value::LocalizedString(_) => "localized string",
        Value::Vector(_) => "vector",
        Value::Object(_) => "object",
        Value::Array(_) => "array",
        Value::Function(_) | Value::Builtin(_) => "function",
        Value::Animation { .. } | Value::AnimationTree(_) => "animation",
    }
}

/// A builtin's int parameter: only an int is one.
fn int(args: &[Value], index: usize) -> Result<i32, String> {
    match arg(args, index)? {
        Value::Int(n) => Ok(*n),
        other => Err(format!("type {} is not an int", type_name(other))),
    }
}

/// A builtin's string parameter: a string, or a number or vector as text.
fn string(args: &[Value], index: usize) -> Result<String, String> {
    match arg(args, index)? {
        Value::String(text) => Ok(text.to_string()),
        other => crate::script::runtime::to_text(other)
            .ok_or_else(|| format!("cannot cast {} to string", type_name(other))),
    }
}

/// A builtin's localized string parameter: only a localized string is one.
fn istring(args: &[Value], index: usize) -> Result<String, String> {
    match arg(args, index)? {
        Value::LocalizedString(text) => Ok(text.to_string()),
        other => Err(format!(
            "type {} is not a localized string",
            type_name(other)
        )),
    }
}

/// The session and game mode of the solo Classic match the owner started in
/// Black Ops 2: an online public session (`docs/fidelity/t6.md`).
const SESSION_FLAGS: u32 = 0x14;
const GAME_MODE_FLAGS: u32 = 0x1;

/// The names `clientsysregister` gave out, in order; the engine keeps eight.
#[derive(Resource, Default)]
struct ClientSystems(Vec<String>);

pub(crate) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    registry.register(Function, "isdefined", |world, _, args| {
        let defined = match arg(args, 0)? {
            Value::Undefined => false,
            Value::Object(id) => world.resource::<RoundScript>().script_is_defined(id),
            _ => true,
        };
        Ok(Value::Int(defined.into()))
    });
    registry.register(Function, "gettime", |world, _, _| {
        let tick = world.resource::<crate::step::StepRequest>().tick;
        let ms = u64::from(tick.0) * u64::from(crate::MATCH_TICK_MS);
        i32::try_from(ms)
            .map(Value::Int)
            .map_err(|_| "gettime overflow".into())
    });

    // The GetDvar opcodes: a missing dvar reads as "" (0, 0.0).
    registry.register(Function, "getdvar", |world, _, args| {
        Ok(Value::String(dvar(world, args)?.into()))
    });
    registry.register(Function, "getdvarint", |world, _, args| {
        Ok(Value::Int(atoi(&dvar(world, args)?)))
    });
    registry.register(Function, "getdvarfloat", |world, _, args| {
        Ok(Value::Float(atof(&dvar(world, args)?)))
    });
    registry.register(Function, "setdvar", |world, _, args| {
        let name = string(args, 0)?;
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err(format!("Dvar {name} has an invalid dvar name"));
        }
        let value = match arg(args, 1)? {
            Value::LocalizedString(text) => text.to_string(),
            _ => string(args, 1)?,
        };
        crate::script::set_dvar(world, &name, &value.replace('"', "'"));
        Ok(Value::Undefined)
    });

    // Session and game mode flags.
    registry.register(Function, "sessionmodeisonlinegame", |_, _, _| {
        Ok(Value::Int((SESSION_FLAGS >> 2 & 1) as i32))
    });
    registry.register(Function, "sessionmodeissystemlink", |_, _, _| {
        Ok(Value::Int((SESSION_FLAGS >> 1 & 1) as i32))
    });
    registry.register(Function, "sessionmodeisprivate", |_, _, _| {
        Ok(Value::Int((SESSION_FLAGS >> 3 & 1) as i32))
    });
    registry.register(Function, "sessionmodeisprivateonlinegame", |_, _, _| {
        Ok(Value::Int(
            (SESSION_FLAGS >> 2 & SESSION_FLAGS >> 3 & 1) as i32,
        ))
    });
    registry.register(Function, "sessionmodeiszombiesgame", |_, _, _| {
        Ok(Value::Int(1))
    });
    registry.register(Function, "gamemodeisusingxp", |_, _, _| {
        Ok(Value::Int((GAME_MODE_FLAGS & 1) as i32))
    });
    registry.register(Function, "gamemodeisusingstats", |_, _, _| {
        Ok(Value::Int((GAME_MODE_FLAGS & 0x41 != 0).into()))
    });
    registry.register(Function, "gamemodeismode", |_, _, args| {
        let mode = int(args, 0)?;
        if !(0..=7).contains(&mode) {
            return Err(format!("Bad game mode {mode}"));
        }
        Ok(Value::Int((GAME_MODE_FLAGS >> mode & 1) as i32))
    });

    registry.register(Function, "getaitriggerflags", |_, _, _| Ok(Value::Int(7)));
    registry.register(Function, "getvehicletriggerflags", |_, _, _| {
        Ok(Value::Int(0x10))
    });

    registry.register(Function, "clientsysregister", |world, _, args| {
        let name = string(args, 0)?;
        let systems = &mut world.get_resource_or_insert_with(ClientSystems::default).0;
        let index = match systems.iter().position(|known| *known == name) {
            Some(index) => index,
            None if systems.len() < 8 => {
                systems.push(name);
                systems.len() - 1
            }
            None => return Ok(Value::Int(-1)),
        };
        Ok(Value::Int(index as i32))
    });

    registry.register(Function, "isstring", |_, _, args| {
        Ok(Value::Int(matches!(arg(args, 0)?, Value::String(_)).into()))
    });
    // Split screen needs an offline, not system link session.
    registry.register(Function, "issplitscreen", |_, _, _| Ok(Value::Int(0)));
    registry.register(Function, "tolower", |_, _, args| {
        let text = string(args, 0)?;
        if text.len() >= 1024 {
            return Err("string too long".into());
        }
        Ok(Value::string(&text.to_ascii_lowercase()))
    });
    registry.register(Function, "spawnstruct", |world, _, _| {
        let mut runtime = world.resource_mut::<RoundScript>();
        let id = runtime.next_object;
        runtime.next_object = id.checked_add(1).ok_or("object identifier exhausted")?;
        runtime.objects.insert(id, BTreeMap::new());
        Ok(Value::Object(id))
    });

    // Precaching: only while the level loads, before any wait.
    registry.register(Function, "precachemodel", |world, _, args| {
        let name = string(args, 0)?;
        if name.is_empty() {
            return Err("Model name string is empty".into());
        }
        super::iw4::precache(world, "model", name).map_err(|_| String::from("precacheModel must be called before any wait statements in the gametype or level script"))?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "precacheshellshock", |world, _, args| {
        let name = string(args, 0)?;
        super::iw4::precache(world, "shellshock", name).map_err(|_| String::from("precacheShellShock must be called before any wait statements in the gametype or level script"))?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "precacherumble", |world, _, args| {
        let name = string(args, 0)?;
        super::iw4::precache(world, "rumble", name).map_err(|_| {
            String::from(
                "precacheRumble must be called before any wait statements in the level script",
            )
        })?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "loadfx", |world, _, args| {
        let name = string(args, 0)?;
        if name.get(..3).is_some_and(|prefix| prefix == "fx/") {
            return Err("effect name should start after the 'fx' folder.".into());
        }
        super::iw4::precache(world, "fx", name).map(Value::Int).map_err(|_| String::from("loadFx must be called before any wait statements in the level script, or on an already loaded effect"))
    });

    // Entities: a lookup by one of the entity's string keys; more than one
    // match is an error.
    registry.register(
        Function,
        "getent",
        |world, _, args| match entities_by_key(world, args)?.as_slice() {
            [] => Ok(Value::Undefined),
            [id] => Ok(Value::Object(*id)),
            _ => Err("getent used with more than one entity".into()),
        },
    );
    registry.register(Function, "getentarray", |world, _, args| {
        let ids = if args.is_empty() {
            world
                .resource::<RoundScript>()
                .entities
                .iter()
                .filter(|(_, entity)| entity.kind != EntityKind::HudElem)
                .map(|(id, _)| *id)
                .collect()
        } else {
            entities_by_key(world, args)?
        };
        new_array(world, ids.into_iter().map(Value::Object).collect())
    });
    // The connected players by client number; a team other than "all" keeps
    // that team's.
    registry.register(Function, "getplayers", |world, _, args| {
        let team = match args.first() {
            None => None,
            Some(Value::String(team)) if team.as_bytes() == b"all" => None,
            Some(_) => Some(string(args, 0)?),
        };
        let mut players: Vec<(i32, u64)> = world
            .resource::<RoundScript>()
            .entities
            .iter()
            .filter(|(_, entity)| entity.kind == EntityKind::Player)
            .map(|(id, entity)| (entity.number, *id))
            .collect();
        players.sort_unstable();
        let mut listed = Vec::new();
        for (_, id) in players {
            let keep = team.as_deref().is_none_or(|team| {
                match world.resource_mut::<RoundScript>().object_field(id, "team") {
                    Value::String(own) => own.as_bytes() == team.as_bytes(),
                    _ => false,
                }
            });
            if keep {
                listed.push(Value::Object(id));
            }
        }
        new_array(world, listed)
    });

    // A gametype setting the configs name; a whole number reads as an int.
    registry.register(Function, "getgametypesetting", |world, _, args| {
        let name = string(args, 0)?.to_ascii_lowercase();
        let Some(value) = world
            .get_resource::<crate::script::GametypeSettings>()
            .and_then(|settings| settings.0.get(&name).cloned())
        else {
            return Ok(Value::Undefined);
        };
        Ok(match value.parse::<i32>() {
            Ok(n) => Value::Int(n),
            Err(_) => Value::Float(atof(&value)),
        })
    });

    registry.register(Function, "precachestring", |world, _, args| {
        if !world.resource::<RoundScript>().loading {
            return Err("precacheString must be called before any wait statements in the gametype or level script\n".into());
        }
        let text = istring(args, 0)?;
        if !text.is_empty() {
            super::iw4::precache(world, "string", text)?;
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "precacheshader", |world, _, args| {
        if !world.resource::<RoundScript>().loading {
            return Err("precacheShader must be called before any wait statements in the gametype or level script\n".into());
        }
        let name = string(args, 0)?;
        if name.is_empty() {
            return Err("Shader name string is empty".into());
        }
        super::iw4::precache(world, "shader", name)?;
        Ok(Value::Undefined)
    });
    // Status icons and menus each fill a fixed run of config strings; a
    // second precache of a name only prints.
    registry.register(Function, "precachestatusicon", |world, _, args| {
        let name = string(args, 0)?;
        let icons = &mut world
            .get_resource_or_insert_with(Precached::default)
            .status_icons;
        if !icons.iter().any(|known| known.eq_ignore_ascii_case(&name)) {
            if icons.len() == 8 {
                return Err("Too many player status icons precached. Max allowed is 8".into());
            }
            icons.push(name);
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "precachemenu", |world, _, args| {
        let name = string(args, 0)?;
        let menus = &mut world.get_resource_or_insert_with(Precached::default).menus;
        if !menus.iter().any(|known| known.eq_ignore_ascii_case(&name)) {
            if menus.len() == 32 {
                return Err("Too many menus precached. Max allowed menus is 32".into());
            }
            menus.push(name);
        }
        Ok(Value::Undefined)
    });

    // An existing dvar only becomes server info; a new one takes the value
    // with its double quotes made single.
    registry.register(Function, "makedvarserverinfo", |world, _, args| {
        let name = string(args, 0)?;
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err(format!("Dvar {name} has an invalid dvar name"));
        }
        let name = name.to_ascii_lowercase();
        world
            .resource_mut::<RoundScript>()
            .server_info
            .insert(name.clone());
        if world
            .resource::<crate::script::MatchScript>()
            .dvars
            .contains_key(&name)
        {
            return Ok(Value::Undefined);
        }
        let value = match arg(args, 1)? {
            Value::LocalizedString(text) => text.to_string(),
            _ => string(args, 1)?,
        };
        crate::script::set_dvar(world, &name, &value.replace('"', "'"));
        Ok(Value::Undefined)
    });
    registry.register(Function, "setmatchflag", |world, _, args| {
        let name = string(args, 0)?;
        let on = int(args, 1)? != 0;
        let (word, bit) = MATCH_FLAGS
            .iter()
            .find(|(flag, _, _)| flag.eq_ignore_ascii_case(&name))
            .map(|&(_, word, bit)| (word, bit))
            .ok_or_else(|| {
                format!("GScr_SetMatchFlag: Couldn't find a matching bit for flag name : {name} ")
            })?;
        let flags = &mut world
            .get_resource_or_insert_with(MatchFlags::default)
            .match_words[word];
        let mask = (1u32 << (bit & 31)) >> 4;
        if on {
            *flags |= mask;
        } else {
            *flags &= !mask;
        }
        Ok(Value::Undefined)
    });
    // An unknown column name stays as the invalid column and only prints.
    registry.register(Function, "setscoreboardcolumns", |world, _, args| {
        let mut columns = [0; 5];
        for (index, column) in columns.iter_mut().enumerate() {
            let name = string(args, index)?;
            *column = SCOREBOARD_COLUMNS
                .iter()
                .position(|known| **known == *name)
                .unwrap_or(0);
        }
        world.insert_resource(ScoreboardColumns(columns));
        Ok(Value::Undefined)
    });
    registry.register(Function, "setroundsplayed", |world, _, args| {
        world.insert_resource(RoundsPlayed(int(args, 0)?));
        Ok(Value::Undefined)
    });

    registry.register(Function, "randomint", |world, _, args| {
        let max = int(args, 0)?;
        if max < 1 {
            return Err("RandomInt parm must be positive integer.\n".into());
        }
        Ok(Value::Int(irand(world, 0, max)))
    });
    registry.register(Function, "int", |_, _, args| match arg(args, 0)? {
        Value::String(text) => Ok(Value::Int(atoi(text))),
        Value::Float(f) => Ok(Value::Int(*f as i32)),
        Value::Int(n) => Ok(Value::Int(*n)),
        other => Err(format!("cannot cast {} to int", type_name(other))),
    });

    registry.register(Function, "float", |_, _, args| match arg(args, 0)? {
        Value::String(text) => Ok(Value::Float(atof(text))),
        Value::Float(f) => Ok(Value::Float(*f)),
        Value::Int(n) => Ok(Value::Float(*n as f32)),
        other => Err(format!("cannot cast {} to float", type_name(other))),
    });
    registry.register(Function, "randomintrange", |world, _, args| {
        let (min, max) = (int(args, 0)?, int(args, 1)?);
        if max <= min {
            return Err(
                "RandomIntRange's second parameter must be greater than the first.\n".into(),
            );
        }
        Ok(Value::Int(irand(world, min, max)))
    });
    registry.register(Function, "randomfloat", |world, _, args| {
        let max = float(args, 0)?;
        Ok(Value::Float(frand(world, 0.0, max)))
    });
    registry.register(Function, "randomfloatrange", |world, _, args| {
        let (min, max) = (float(args, 0)?, float(args, 1)?);
        if max <= min {
            return Err(
                "Scr_RandomFloatRange's second parameter must be greater than the first.\n".into(),
            );
        }
        Ok(Value::Float(frand(world, min, max)))
    });
    registry.register(Function, "getminbitcountfornum", |_, _, args| {
        let n = int(args, 0)?;
        if n < 1 {
            return Err("GScr_GetMinBitCountForNum: parm must be positive integer.\n".into());
        }
        Ok(Value::Int(32 - n.leading_zeros() as i32))
    });
    registry.register(Function, "ismp", |_, _, _| Ok(Value::Int(1)));
    // One online client, the owner's.
    registry.register(Function, "getnumexpectedplayers", |_, _, _| {
        Ok(Value::Int(1))
    });

    // The math opcodes.
    registry.register(Function, "anglestoforward", |_, _, args| {
        angles_to(args, "anglestoforward", 0)
    });
    registry.register(Function, "anglestoright", |_, _, args| {
        angles_to(args, "anglestoright", 1)
    });
    registry.register(Function, "anglestoup", |_, _, args| {
        angles_to(args, "anglestoup", 2)
    });
    registry.register(Function, "vectortoangles", |_, _, args| {
        match arg(args, 0)? {
            Value::Vector(v) => Ok(Value::Vector(math_iw4::vect_to_angles(*v))),
            other => Err(format!(
                "vectorstoangle expecting vector, given {})",
                type_name(other)
            )),
        }
    });
    registry.register(Function, "angleclamp180", |_, _, args| {
        let angle = match arg(args, 0)? {
            Value::Float(f) => *f,
            Value::Int(n) => *n as f32,
            other => {
                return Err(format!(
                    "angleclamp180 expecting float, given {})",
                    type_name(other)
                ));
            }
        };
        let turns = angle * (1.0 / 360.0);
        let clamped = (turns - turns.floor()) * 360.0;
        Ok(Value::Float(if clamped > 180.0 {
            clamped - 360.0
        } else {
            clamped
        }))
    });
    registry.register(Function, "abs", |_, _, args| match arg(args, 0)? {
        Value::Float(f) => Ok(Value::Float(if *f < 0.0 { -f } else { *f })),
        Value::Int(n) => Ok(Value::Int(n.wrapping_abs())),
        other => Err(format!(
            "abs expecting float or int, given {})",
            type_name(other)
        )),
    });

    // Arrays: values match when their type and value do.
    registry.register(Function, "array", |world, _, args| {
        let values = args
            .iter()
            .take_while(|value| !matches!(value, Value::Undefined))
            .cloned()
            .collect();
        new_array(world, values)
    });
    // Without keeping keys the remaining entries are numbered from 0.
    registry.register(Function, "arrayremovevalue", |world, _, args| {
        let id = array_id(args, 0)?;
        let wanted = arg(args, 1)?.clone();
        let keep_keys = args.len() == 3 && int(args, 2)? != 0;
        let mut runtime = world.resource_mut::<RoundScript>();
        let array = runtime
            .arrays
            .get_mut(&id)
            .ok_or("invalid array reference")?;
        array.retain(|_, value| *value != wanted);
        if !keep_keys {
            let values = std::mem::take(array).into_values();
            *array = values
                .enumerate()
                .map(|(index, value)| (ArrayKey::Integer(index as i32), value))
                .collect();
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "arrayremoveindex", |world, _, args| {
        let id = array_id(args, 0)?;
        let index = int(args, 1)?;
        let keep_keys = args.len() == 3 && int(args, 2)? != 0;
        let mut runtime = world.resource_mut::<RoundScript>();
        let array = runtime
            .arrays
            .get_mut(&id)
            .ok_or("invalid array reference")?;
        array.remove(&ArrayKey::Integer(index));
        if !keep_keys {
            let values = std::mem::take(array).into_values();
            *array = values
                .enumerate()
                .map(|(index, value)| (ArrayKey::Integer(index as i32), value))
                .collect();
        }
        Ok(Value::Undefined)
    });
    // An index inside the array moves the entries from it up by one; any
    // other appends.
    registry.register(Function, "arrayinsert", |world, _, args| {
        let id = array_id(args, 0)?;
        let value = arg(args, 1)?.clone();
        let index = int(args, 2)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        let array = runtime
            .arrays
            .get_mut(&id)
            .ok_or("invalid array reference")?;
        let size = array.len() as i32;
        if (0..size).contains(&index) {
            for key in (index..size).rev() {
                if let Some(moved) = array.remove(&ArrayKey::Integer(key)) {
                    array.insert(ArrayKey::Integer(key + 1), moved);
                }
            }
            array.insert(ArrayKey::Integer(index), value);
        } else {
            array.insert(ArrayKey::Integer(size), value);
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "isinarray", |world, _, args| {
        let values = array(world, args, 0)?;
        let wanted = arg(args, 1)?;
        Ok(Value::Int(
            values.values().any(|value| value == wanted).into(),
        ))
    });
    registry.register(Function, "getarraykeys", |world, _, args| {
        let keys = array(world, args, 0)?
            .into_keys()
            .rev()
            .map(|key| match key {
                ArrayKey::Integer(n) => Value::Int(n),
                ArrayKey::String(s) => Value::String(s),
            })
            .collect();
        new_array(world, keys)
    });
    // Keeping keys needs both arrays keyed by strings, judged by their first
    // entry, and no key in both; without duplicates the second array's values
    // already in the first are left out.
    registry.register(Function, "arraycombine", |world, _, args| {
        let first = array(world, args, 0)?;
        let second = array(world, args, 1)?;
        let duplicates = int(args, 2)? != 0;
        let string_keyed = |array: &BTreeMap<ArrayKey, Value>| {
            array
                .keys()
                .next_back()
                .is_none_or(|key| matches!(key, ArrayKey::String(_)))
        };
        let keep_keys = int(args, 3)? != 0 && string_keyed(&first) && string_keyed(&second);
        if keep_keys && second.keys().any(|key| first.contains_key(key)) {
            return Err("Duplicate key found when merging arrays".into());
        }
        let mut combined: Vec<(ArrayKey, Value)> = first.clone().into_iter().collect();
        combined.extend(
            second
                .into_iter()
                .filter(|(_, value)| duplicates || !first.values().any(|known| known == value)),
        );
        if !keep_keys {
            return new_array(
                world,
                combined.into_iter().map(|(_, value)| value).collect(),
            );
        }
        let mut runtime = world.resource_mut::<RoundScript>();
        let id = runtime.next_object;
        runtime.next_object = id.checked_add(1).ok_or("object identifier exhausted")?;
        runtime.arrays.insert(id, combined.into_iter().collect());
        Ok(Value::Array(id))
    });

    registry.register(Function, "tablelookup", |world, _, args| {
        Ok(Value::String(table_lookup(world, args)?.into()))
    });

    registry.register(Function, "isarray", |_, _, args| {
        Ok(Value::Int(matches!(arg(args, 0)?, Value::Array(_)).into()))
    });
    registry.register(Function, "setteamscore", |world, _, args| {
        let team = team(args, 0)?;
        let score = int(args, 1)?;
        world.get_resource_or_insert_with(TeamScores::default).0[team] = score;
        Ok(Value::Undefined)
    });
    registry.register(Function, "getteamscore", |world, _, args| {
        let team = team(args, 0)?;
        Ok(Value::Int(
            world
                .get_resource::<TeamScores>()
                .map_or(0, |scores| scores.0[team]),
        ))
    });
    registry.register(Function, "setmatchtalkflag", |world, _, args| {
        let name = string(args, 0)?;
        let on = int(args, 1)? != 0;
        let bit = TALK_FLAGS
            .iter()
            .position(|flag| flag.eq_ignore_ascii_case(&name))
            .ok_or_else(|| {
                format!(
                    "GScr_SetMatchTalkFlag: Couldn't find a matching bit for flag name : {name} "
                )
            })?;
        let flags = &mut world.get_resource_or_insert_with(MatchFlags::default).talk;
        if on {
            *flags |= 1 << bit;
        } else {
            *flags &= !(1 << bit);
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "setgameendtime", |world, _, args| {
        let time = int(args, 0)?;
        world.resource_mut::<RoundScript>().engine.game_end_time = time;
        Ok(Value::Undefined)
    });
    // The level's vision set, eased in over the time given (a second by
    // default); other argument counts do nothing.
    registry.register(Function, "visionsetnaked", |world, _, args| {
        let seconds = match args.len() {
            1 => 1.0,
            2 => float(args, 1)?,
            _ => return Ok(Value::Undefined),
        };
        let name = string(args, 0)?;
        let set_ms = crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick);
        world.resource_mut::<RoundScript>().engine.naked_vision = Some(crate::VisionChange {
            name,
            duration_ms: (seconds * 1000.0).round() as i32,
            set_ms,
        });
        Ok(Value::Undefined)
    });
    // Reports the match start to the online backend only.
    registry.register(Function, "recordmatchbegin", |_, _, _| Ok(Value::Undefined));

    // The classes a script can spawn.
    registry.register(Function, "spawn", |world, receiver, args| {
        let classname = string(args, 0)?;
        match classname.as_str() {
            "script_origin" | "script_model" | "info_notnull" | "info_notnull_big" => {
                let spawn = NativeRegistry::engine_services()
                    .get(Function, "spawn")
                    .ok_or("spawn: no engine spawn")?;
                spawn(world, receiver, args)
            }
            "trigger_radius" | "trigger_radius_use" | "trigger_box" | "trigger_box_use" => {
                spawn_trigger(world, &classname, args)
            }
            "info_volume" | "trigger_damage" => {
                Err(format!("spawn {classname}: not implemented yet"))
            }
            _ if classname.starts_with("weapon_")
                || (classname.starts_with("mp_") && classname.contains("_spawn")) =>
            {
                Err(format!("spawn {classname}: not implemented yet"))
            }
            _ => Err(format!("unable to spawn \"{classname}\" entity")),
        }
    });
    registry.register(Method, "getmins", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        Ok(Value::Vector(bounds(world, id).0))
    });
    registry.register(Method, "getmaxs", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        Ok(Value::Vector(bounds(world, id).1))
    });
    registry.register(Method, "getabsmins", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        Ok(Value::Vector(absolute_bounds(world, id).0))
    });
    registry.register(Method, "getabsmaxs", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        Ok(Value::Vector(absolute_bounds(world, id).1))
    });

    registry.register(Method, "fadeovertime", |world, receiver, args| {
        let seconds = float(args, 0)?;
        if seconds <= 0.0 {
            return Err(format!("fade time {} <= 0", g(seconds)));
        }
        if seconds > 60.0 {
            return Err(format!("fade time {} > 60", g(seconds)));
        }
        let fade = NativeRegistry::engine_services()
            .get(Method, "fadeovertime")
            .ok_or("fadeovertime: no engine fade")?;
        fade(world, receiver, args)
    });

    // Path nodes, in map order; a lookup names one of the node's string
    // fields and an empty field never matches.
    registry.register(Function, "getallnodes", |world, _, _| {
        let nodes = super::super::actor_nav::nodes_where(world, |_| true)?;
        new_array(world, nodes)
    });
    registry.register(Function, "getnodearray", |world, _, args| {
        let value = string(args, 0)?;
        let key = string(args, 1)?;
        let field = node_string_field(&key)?;
        let nodes = super::super::actor_nav::nodes_where(world, |node| {
            let text = field(node);
            !text.is_empty() && *text == *value
        })?;
        new_array(world, nodes)
    });

    registry.register(Function, "cos", |_, _, args| {
        let degrees = float(args, 0)?;
        Ok(Value::Float(
            (f64::from(degrees) * f64::from(std::f32::consts::PI / 180.0)).cos() as f32,
        ))
    });
    registry.register(Function, "issubstr", |_, _, args| {
        let (text, part) = (string(args, 0)?, string(args, 1)?);
        Ok(Value::Int(text.contains(&part).into()))
    });
    // Tokens between any of the delimiter characters; empty ones are skipped.
    registry.register(Function, "strtok", |world, _, args| {
        let (text, delimiters) = (string(args, 0)?, string(args, 1)?);
        let mut tokens = Vec::new();
        for token in text.split(|c| delimiters.contains(c)) {
            if token.len() > 1023 {
                return Err("string too long".into());
            }
            if !token.is_empty() {
                tokens.push(Value::string(token));
            }
        }
        new_array(world, tokens)
    });
    registry.register(Function, "tablelookuprownum", |world, _, args| {
        let name = table_name(args)?;
        let column = int(args, 1)?;
        let value = string(args, 2)?;
        let tables = world.resource::<RoundScript>().tables.clone();
        let row = crate::script::host::tables::table(&tables, &name).and_then(|table| {
            let column = usize::try_from(column).ok()?;
            (0..table.rows).find(|&row| {
                table
                    .cell(row, column)
                    .is_some_and(|cell| cell.eq_ignore_ascii_case(&value))
            })
        });
        Ok(Value::Int(row.map_or(-1, |row| row as i32)))
    });
    // Black Ops 2's logstring handler is empty, as function and as method.
    registry.register(Function, "logstring", |_, _, _| Ok(Value::Undefined));
    registry.register(Method, "logstring", |_, _, _| Ok(Value::Undefined));
    // The online counters and the demo camera are outside the match.
    registry.register(Function, "incrementcounter", |_, _, args| {
        string(args, 0)?;
        int(args, 1)?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "setdemointermissionpoint", |_, _, _| {
        Ok(Value::Undefined)
    });
    registry.register(Function, "getnumconnectedplayers", |world, _, _| {
        let players = world
            .resource::<RoundScript>()
            .entities
            .values()
            .filter(|entity| entity.kind == EntityKind::Player)
            .count();
        Ok(Value::Int(players as i32))
    });
    registry.register(Function, "precacheleaderboards", |world, _, args| {
        if !world.resource::<RoundScript>().loading {
            return Err("precacheLeaderboards must be called before any wait statements in the gametype or level script\n".into());
        }
        string(args, 0)?;
        Ok(Value::Undefined)
    });
    // Client fields by pool; the bit count is 1 to 32, the type int or float.
    registry.register(Function, "registerclientfield", |world, _, args| {
        let pool = string(args, 0)?;
        let name = string(args, 1)?;
        let version = int(args, 2)?;
        let bits = int(args, 3)?;
        let kind = string(args, 4)?;
        if !CLIENT_FIELD_POOLS.iter().any(|known| known.eq_ignore_ascii_case(&pool)) {
            return Err(format!("{pool} not a valid name for a clientfield set."));
        }
        if !(1..=32).contains(&bits) {
            return Err(format!(
                "RegisterClientField can only accept bit ranges between 1 and 32.  {bits} requested."
            ));
        }
        let float = match kind.as_str() {
            "int" => false,
            "float" => true,
            _ => return Err("Unknown type passed to BG_GetClientFieldTypeFromTypeName".into()),
        };
        world
            .get_resource_or_insert_with(ClientFields::default)
            .0
            .push(ClientField {
                pool: pool.to_ascii_lowercase(),
                name,
                version,
                bits,
                float,
            });
        Ok(Value::Undefined)
    });
    registry.register(Function, "scriptmodelsuseanimtree", |world, _, args| {
        let tree = arg(args, 0)?.clone();
        world.insert_resource(ScriptModelAnimTree(tree));
        Ok(Value::Undefined)
    });

    // Use triggers.
    registry.register(Method, "sethintstring", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        let classname = world.resource::<RoundScript>().entities[&id].classname.clone();
        if !HINT_CLASSES.contains(&&*classname) && !SCRIPT_MOVERS.contains(&&*classname) {
            return Err("The setHintString command only works on trigger_use, trigger_radius, trigger_box, trigger_radius_use, trigger_use_touch, or script mover entities.\n".into());
        }
        let hint = match args {
            [Value::String(text)] if text.is_empty() => 0,
            _ => super::super::hud::string_index_with_args(world, args)?,
        };
        super::super::physics::usable(world, receiver)?.hint = hint;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setcursorhint", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        let name = string(args, 0)?;
        let classname = world.resource::<RoundScript>().entities[&id]
            .classname
            .clone();
        if HINT_CLASSES.contains(&&*classname) && name.eq_ignore_ascii_case("HINT_INHERIT") {
            super::super::physics::usable(world, receiver)?.cursor = -1;
            return Ok(Value::Undefined);
        }
        let cursor = CURSOR_HINTS
            .iter()
            .position(|hint| hint.eq_ignore_ascii_case(&name))
            .ok_or_else(|| {
                format!("{name} is not a valid hint type. See above for list of valid hint types\n")
            })?;
        super::super::physics::usable(world, receiver)?.cursor = cursor as i32 + 1;
        Ok(Value::Undefined)
    });
    registry.register(Method, "sethintlowpriority", |world, receiver, args| {
        let low = int(args, 0)? != 0;
        let id = super::engine::entity_id(world, receiver)?;
        world
            .get_resource_or_insert_with(EntityFlags::default)
            .low_priority_hint
            .insert(id, low);
        Ok(Value::Undefined)
    });
    registry.register(Method, "triggerignoreteam", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        let classname = world.resource::<RoundScript>().entities[&id].classname.clone();
        if !matches!(&*classname, "trigger_radius" | "trigger_box" | "trigger_radius_use") {
            return Err("triggerignoreteam: trigger entity must be of type trigger_radius, trigger_box or trigger_radius_use".into());
        }
        world
            .get_resource_or_insert_with(EntityFlags::default)
            .ignore_team
            .insert(id);
        Ok(Value::Undefined)
    });

    registry.register(Function, "sin", |_, _, args| {
        let degrees = float(args, 0)?;
        Ok(Value::Float(
            (f64::from(degrees) * f64::from(std::f32::consts::PI / 180.0)).sin() as f32,
        ))
    });
    registry.register(Function, "atan", |_, _, args| {
        let value = float(args, 0)?;
        Ok(Value::Float(
            (f64::from(value).atan() * f64::from(180.0f32 / std::f32::consts::PI)) as f32,
        ))
    });
    registry.register(Function, "sqrt", |_, _, args| {
        Ok(Value::Float(f64::from(float(args, 0)?).sqrt() as f32))
    });
    registry.register(Function, "log", |_, _, args| {
        Ok(Value::Float(f64::from(float(args, 0)?).ln() as f32))
    });
    registry.register(Function, "floor", |_, _, args| {
        Ok(Value::Float(float(args, 0)?.floor()))
    });
    registry.register(Function, "ceil", |_, _, args| {
        Ok(Value::Float(f64::from(float(args, 0)?).ceil() as f32))
    });
    // Always a float; the first unless the second is smaller (larger).
    registry.register(Function, "min", |_, _, args| {
        let (a, b) = (float(args, 0)?, float(args, 1)?);
        Ok(Value::Float(if f64::from(b) - f64::from(a) >= 0.0 {
            a
        } else {
            b
        }))
    });
    registry.register(Function, "max", |_, _, args| {
        let (a, b) = (float(args, 0)?, float(args, 1)?);
        Ok(Value::Float(if f64::from(a) - f64::from(b) >= 0.0 {
            a
        } else {
            b
        }))
    });
    registry.register(Function, "vectordot", |_, _, args| {
        let (a, b) = (vector(args, 0)?, vector(args, 1)?);
        Ok(Value::Float(a[1] * b[1] + a[0] * b[0] + a[2] * b[2]))
    });
    registry.register(Function, "vectorcross", |_, _, args| {
        let (a, b) = (vector(args, 0)?, vector(args, 1)?);
        Ok(Value::Vector([
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]))
    });
    registry.register(Function, "vectornormalize", |_, _, args| {
        let v = vector(args, 0)?;
        let mut length = (v[1] * v[1] + v[0] * v[0] + v[2] * v[2]).sqrt();
        if 0.0 <= -length {
            length = 1.0;
        }
        let scale = 1.0 / length;
        Ok(Value::Vector([v[0] * scale, v[1] * scale, v[2] * scale]))
    });
    registry.register(Function, "length", |_, _, args| {
        let v = vector(args, 0)?;
        Ok(Value::Float(
            (v[1] * v[1] + v[0] * v[0] + v[2] * v[2]).sqrt(),
        ))
    });
    registry.register(Function, "lengthsquared", |_, _, args| {
        let v = vector(args, 0)?;
        Ok(Value::Float(v[1] * v[1] + v[0] * v[0] + v[2] * v[2]))
    });
    registry.register(Function, "distance", |_, _, args| {
        Ok(Value::Float(distance_squared(args, 3)?.sqrt()))
    });
    registry.register(Function, "distancesquared", |_, _, args| {
        Ok(Value::Float(distance_squared(args, 3)?))
    });
    registry.register(Function, "distance2d", |_, _, args| {
        Ok(Value::Float(distance_squared(args, 2)?.sqrt()))
    });
    registry.register(Function, "distance2dsquared", |_, _, args| {
        Ok(Value::Float(distance_squared(args, 2)?))
    });
    registry.register(Function, "isint", |_, _, args| {
        Ok(Value::Int(matches!(arg(args, 0)?, Value::Int(_)).into()))
    });
    registry.register(Function, "getsubstr", |_, _, args| {
        let text = string(args, 0)?;
        let start = int(args, 1)?;
        let end = if args.len() < 3 {
            i32::MAX
        } else {
            int(args, 2)?
        };
        let bytes = text.as_bytes();
        let mut part = Vec::new();
        let mut at = start;
        while at < end {
            if part.len() > 0x3ff {
                return Err("string too long".into());
            }
            let Some(&byte) = usize::try_from(at).ok().and_then(|at| bytes.get(at)) else {
                break;
            };
            part.push(byte);
            at += 1;
        }
        Ok(Value::string(&String::from_utf8_lossy(&part)))
    });
    // The minimap's material and corners, turned to the map's north.
    registry.register(Function, "setminimap", |world, _, args| {
        let material = string(args, 0)?;
        let (x1, y1) = (float(args, 1)?, float(args, 2)?);
        let (x2, y2) = (float(args, 3)?, float(args, 4)?);
        let north = world
            .resource::<RoundScript>()
            .engine
            .worldspawn
            .get("northyaw")
            .map_or(0.0, |v| atof(v));
        let radians = f64::from((f64::from(north) * f64::from(std::f32::consts::PI / 180.0)) as f32);
        let (sin, cos) = (radians.sin() as f32, radians.cos() as f32);
        let (dx, dy) = (x2 - x1, y2 - y1);
        let width = sin * dx - cos * dy;
        let height = -(cos * dx) - sin * dy;
        if width < 0.0 || height < 0.0 {
            return Err("lower-right X and Y coordinates must be both south and east of upper-left X and Y coordinates in terms of the northyaw".into());
        }
        world.insert_resource(Minimap {
            material,
            corners: [x1, y1, x2, y2],
        });
        Ok(Value::Undefined)
    });
    registry.register(Method, "ghost", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        if let Some(entity) = world.resource_mut::<RoundScript>().entities.get_mut(&id) {
            entity.hidden = true;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "setteamfortrigger", |world, receiver, args| {
        let id = super::engine::entity_id(world, receiver)?;
        let classname = world.resource::<RoundScript>().entities[&id].classname.clone();
        if !HINT_CLASSES.contains(&&*classname) {
            return Err("setteamfortrigger: trigger entity must be of type trigger_use, trigger_use_touch, trigger_radius, trigger_box or trigger_radius_use".into());
        }
        let team = team(args, 0)?;
        let mut flags = world.get_resource_or_insert_with(EntityFlags::default);
        flags.trigger_team.insert(id, team);
        flags.ignore_team.remove(&id);
        Ok(Value::Undefined)
    });
    registry.register(Function, "codesetworldclientfield", |world, _, args| {
        let name = string(args, 0)?;
        let fields = world.get_resource_or_insert_with(ClientFields::default);
        let field = fields
            .0
            .iter()
            .position(|field| field.pool == "world" && field.name == name)
            .ok_or_else(|| format!("clientfield {name} is not registered"))?;
        let value = if fields.0[field].float {
            Value::Float(float(args, 1)?)
        } else {
            Value::Int(int(args, 1)?)
        };
        world
            .get_resource_or_insert_with(ClientFields::default)
            .1
            .insert(("world".to_owned(), 0, field), value);
        Ok(Value::Undefined)
    });

    // Living actors whose team is one of those named; none names every team
    // but the free one. Dogs are left out.
    registry.register(Function, "getaiarray", |world, _, args| {
        let mut mask = 0u32;
        for index in 0..args.len() {
            mask |= match arg(args, index)? {
                Value::String(name) if name.as_bytes() == b"all" => 0x1fe,
                _ => 1 << team(args, index)?,
            };
        }
        if mask == 0 {
            mask = 0x1fe;
        }
        let actors = living_actors(world, mask, |species| species != 0 && species != 3);
        new_array(world, actors)
    });
    // Living actors of a team (or all) and a species (or all).
    registry.register(Function, "getaispeciesarray", |world, _, args| {
        let mask = match args.first() {
            None => 0x1fe,
            Some(Value::String(name)) if name.as_bytes() == b"all" => 0x1fe,
            Some(_) => 1 << team(args, 0)?,
        };
        let species = if args.len() < 2 {
            4
        } else {
            let name = string(args, 1)?;
            match name.as_str() {
                "all" => 4,
                _ => SPECIES
                    .iter()
                    .position(|known| *known == name)
                    .ok_or_else(|| {
                        format!("unknown species '{name}' (should be human, dog, bigdog or all)")
                    })?,
            }
        };
        let actors = living_actors(world, mask, |own| species == 4 || own == species);
        new_array(world, actors)
    });
    // Path changes wait for Black Ops 2's path finding (`docs/fidelity/t6.md`).
    registry.register(Method, "disconnectpaths", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        world
            .get_resource_or_insert_with(EntityFlags::default)
            .disconnects_paths
            .insert(id);
        Ok(Value::Undefined)
    });
    registry.register(Method, "connectpaths", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        world
            .get_resource_or_insert_with(EntityFlags::default)
            .disconnects_paths
            .remove(&id);
        Ok(Value::Undefined)
    });
    registry.register(Method, "useweaponhidetags", |world, receiver, args| {
        let name = string(args, 0)?;
        if weapon::weapon_index(world, &name)? == 0 {
            return Err(format!(
                "useweaponhidetags called with unknown weapon name {name}\n"
            ));
        }
        let id = super::engine::entity_id(world, receiver)?;
        world
            .get_resource_or_insert_with(EntityFlags::default)
            .weapon_hide_tags
            .insert(id, name);
        Ok(Value::Undefined)
    });

    // Player settings the client reads; each is a client dvar by number.
    registry.register(
        Method,
        "setclientaimlockonpitchstrength",
        |world, receiver, args| set_client_dvar(world, receiver, args, 0),
    );
    registry.register(
        Method,
        "setclientammocounterhide",
        |world, receiver, args| set_client_dvar(world, receiver, args, 1),
    );
    registry.register(
        Method,
        "setclientcgobjectivetext",
        |world, receiver, args| set_client_dvar(world, receiver, args, 2),
    );
    registry.register(Method, "setclientcompass", |world, receiver, args| {
        set_client_dvar(world, receiver, args, 3)
    });
    registry.register(Method, "setclienthudhardcore", |world, receiver, args| {
        set_client_dvar(world, receiver, args, 6)
    });
    registry.register(
        Method,
        "setclientplayersprinttime",
        |world, receiver, args| set_client_dvar(world, receiver, args, 7),
    );
    registry.register(
        Method,
        "setclientminiscoreboardhide",
        |world, receiver, args| set_client_dvar(world, receiver, args, 8),
    );
    registry.register(Method, "setclientnumlives", |world, receiver, args| {
        set_client_dvar(world, receiver, args, 9)
    });
    registry.register(
        Method,
        "setclientplayerpushamount",
        |world, receiver, args| set_client_dvar(world, receiver, args, 10),
    );
    registry.register(
        Method,
        "setclientscriptmainmenu",
        |world, receiver, args| set_client_dvar(world, receiver, args, 12),
    );
    registry.register(
        Method,
        "setclientthirdpersonangle",
        |world, receiver, args| set_client_dvar(world, receiver, args, 14),
    );
    registry.register(Method, "setclientthirdperson", |world, receiver, args| {
        let client = client_of(world, receiver)?;
        let on = int(args, 0)? != 0;
        player_state(world, client).third_person = on;
        Ok(Value::Undefined)
    });
    registry.register(
        Method,
        "setclientuivisibilityflag",
        |world, receiver, args| {
            let client = client_of(world, receiver)?;
            let name = string(args, 0)?;
            let on = int(args, 1)? != 0;
            let bit = [
                "hud_visible",
                "g_compassShowEnemies",
                "radar_client",
                "killcam_nemesis",
            ]
            .iter()
            .position(|flag| flag.eq_ignore_ascii_case(&name))
            .ok_or_else(|| {
                format!(
                    "PlayerCmd_SetClientFlag: Couldn't find a matching bit for flag name : {name} "
                )
            })?;
            let flags = &mut player_state(world, client).ui_visibility;
            if on {
                *flags |= 1 << bit;
            } else {
                *flags &= !(1 << bit);
            }
            Ok(Value::Undefined)
        },
    );
    registry.register(Method, "setplayercurrentstreak", |world, receiver, args| {
        let client = client_of(world, receiver)?;
        player_state(world, client).current_streak = int(args, 0)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "allowspectateteam", |world, receiver, args| {
        let client = client_of(world, receiver)?;
        let name = string(args, 0)?;
        let bit = match name.as_str() {
            "none" => 1,
            "freelook" => 0x400,
            "localplayers" => 0x800,
            _ => 1 << team(args, 0)?,
        };
        let allow = int(args, 1)? != 0;
        let mask = &mut player_state(world, client).spectate_denied;
        if allow {
            *mask &= !bit;
        } else {
            *mask |= bit;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "enableusability", |world, receiver, _| {
        let client = client_of(world, receiver)?;
        player_state(world, client).usability_disabled = false;
        Ok(Value::Undefined)
    });
    registry.register(Method, "disabledeathstreak", |world, receiver, _| {
        client_of(world, receiver)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "getentitynumber", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        Ok(Value::Int(
            world.resource::<RoundScript>().entities[&id].number,
        ))
    });
    // The owner hosts the solo match on this machine.
    registry.register(Method, "ishost", |world, receiver, _| {
        client_of(world, receiver)?;
        Ok(Value::Int(1))
    });
    registry.register(Method, "islocaltohost", |world, receiver, _| {
        client_of(world, receiver)?;
        Ok(Value::Int(1))
    });
    registry.register(Method, "issplitscreen", |world, receiver, _| {
        client_of(world, receiver)?;
        Ok(Value::Int(0))
    });
    registry.register(Function, "getgametypeenumfromname", |_, _, args| {
        let name = string(args, 0)?;
        int(args, 1)?;
        let index = ["zgrief", "zclassic", "zstandard", "zcleansed"]
            .iter()
            .position(|known| known.eq_ignore_ascii_case(&name))
            .ok_or_else(|| {
                format!("GetGameTypeEnumFromName: Invalid gametype parameter: '{name}' supplied to GetGameTypeEnumFromName")
            })?;
        Ok(Value::Int(index as i32))
    });
    registry.register(Function, "setinitialplayersconnected", |world, _, _| {
        world.insert_resource(InitialPlayersConnected);
        Ok(Value::Undefined)
    });
    registry.register(Function, "aretexturesloaded", |_, _, _| Ok(Value::Int(1)));
    registry.register(Method, "usetriggerrequirelookat", |world, receiver, _| {
        let id = super::engine::entity_id(world, receiver)?;
        let classname = world.resource::<RoundScript>().entities[&id]
            .classname
            .clone();
        if !matches!(
            &*classname,
            "trigger_use" | "trigger_radius_use" | "trigger_box"
        ) {
            return Err(
                "The UseTriggerRequireLookAt command only works on trigger_use entities.\n".into(),
            );
        }
        world
            .get_resource_or_insert_with(EntityFlags::default)
            .require_look_at
            .insert(id);
        Ok(Value::Undefined)
    });
    // Profiling, performance markers, logs, the matchmaking backend's game
    // history and stats upload are outside the match.
    registry.register(Function, "profilelog_begintiming", |_, _, _| {
        Ok(Value::Undefined)
    });
    registry.register(Function, "profilelog_endtiming", |_, _, _| {
        Ok(Value::Undefined)
    });
    registry.register(Function, "pixbeginevent", |_, _, _| Ok(Value::Undefined));
    registry.register(Function, "pixendevent", |_, _, _| Ok(Value::Undefined));
    registry.register(Function, "pixmarker", |_, _, _| Ok(Value::Undefined));
    registry.register(Function, "logprint", |_, _, _| Ok(Value::Undefined));
    registry.register(Function, "bbprint", |_, _, _| Ok(Value::Undefined));
    registry.register(Function, "resettimeout", |_, _, _| Ok(Value::Undefined));
    registry.register(Function, "uploadstats", |_, _, _| Ok(Value::Undefined));
    registry.register(Method, "gamehistorystartmatch", |world, receiver, _| {
        client_of(world, receiver)?;
        Ok(Value::Undefined)
    });

    // A player's stats, in the zombies stats layout; a new profile's are all
    // zero (`docs/fidelity/t6.md`).
    registry.register(Method, "getdstat", |world, receiver, args| {
        let client = client_of(world, receiver)
            .map_err(|_| String::from("getdstat: entity must be a player entity"))?;
        let (layout, at) =
            stat_path(world, args, args.len())?.ok_or("getdstat: can't find stat path")?;
        let stats = player_stats(world, client, &layout);
        let raw = read_bits(stats, at.offset, at.bits);
        Ok(match at.kind {
            0..=3 => Value::Int(raw as u32 as i32),
            4 => Value::string(&raw.to_string()),
            5 | 6 => Value::string(&format!("{:.6}", f32::from_bits(raw as u32))),
            7 => {
                let bytes: Vec<u8> = (0..at.bits / 8)
                    .map(|i| read_bits(stats, at.offset + i * 8, 8) as u8)
                    .take_while(|&b| b != 0)
                    .collect();
                Value::string(&String::from_utf8_lossy(&bytes))
            }
            _ => return Err("getdstat: stat type undefined".into()),
        })
    });
    registry.register(Method, "setdstat", |world, receiver, args| {
        let client = client_of(world, receiver)
            .map_err(|_| String::from("setdstat: entity must be a player entity"))?;
        let path = args.len().saturating_sub(1);
        let Some((layout, at)) = stat_path(world, args, path)? else {
            return Ok(Value::Undefined);
        };
        let value = match (arg(args, path)?, at.kind) {
            (Value::Int(n), 0..=3) => *n as u32 as u64,
            (Value::Int(n), 4) => *n as i64 as u64,
            (Value::Float(f), 5 | 6) => u64::from(f.to_bits()),
            (Value::String(text), 4) => text.parse::<u64>().unwrap_or(0),
            (Value::String(text), 7) => {
                let stats = player_stats(world, client, &layout);
                let bytes = text.as_bytes();
                for i in 0..at.bits / 8 {
                    let byte = bytes.get(i as usize).copied().unwrap_or(0);
                    write_bits(stats, at.offset + i * 8, 8, u64::from(byte));
                }
                return Ok(Value::Undefined);
            }
            _ => return Err("setdstat: the value's type does not match the stat's".into()),
        };
        write_bits(
            player_stats(world, client, &layout),
            at.offset,
            at.bits,
            value,
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "adddstat", |world, receiver, args| {
        let client = client_of(world, receiver)
            .map_err(|_| String::from("AddDStat: entity must be a player entity"))?;
        let path = args.len().saturating_sub(1);
        let Some((layout, at)) = stat_path(world, args, path)? else {
            return Ok(Value::Undefined);
        };
        let delta = int(args, path)?;
        let stats = player_stats(world, client, &layout);
        let value = read_bits(stats, at.offset, at.bits) as i64 + i64::from(delta);
        write_bits(stats, at.offset, at.bits, value as u64);
        Ok(Value::Undefined)
    });
    // Counts for the online match record only.
    registry.register(Method, "incrementplayerstat", |world, receiver, args| {
        client_of(world, receiver)?;
        string(args, 0)?;
        int(args, 1)?;
        Ok(Value::Undefined)
    });
    // The reviver goes in the online match record only.
    registry.register(
        Method,
        "recordplayerrevivezombies",
        |world, receiver, args| {
            client_of(world, receiver)?;
            super::engine::entity_id(world, arg(args, 0)?)?;
            Ok(Value::Undefined)
        },
    );
    // Its handler is empty.
    registry.register(Function, "getplayerspawnid", |_, _, _| Ok(Value::Undefined));

    registry.register(Function, "playsoundatposition", |world, _, args| {
        let origin = vector(args, 1)?;
        let alias = string(args, 0)?;
        super::engine::play_sound_at(world, origin, &alias)?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "disablegrenadesuicide", |world, _, _| {
        world.insert_resource(GrenadeSuicideDisabled);
        Ok(Value::Undefined)
    });
    // Demo bookmarks only matter while a demo records.
    registry.register(Function, "adddemobookmark", |_, _, _| Ok(Value::Undefined));
    // Clients beyond the host's own: none in a solo match.
    registry.register(Function, "numremoteclients", |_, _, _| Ok(Value::Int(0)));
    // Each client's last snapshot number plus one (undefined for one not
    // active), then the server's time plus 1000.
    registry.register(Function, "getsnapshotindexarray", |world, _, _| {
        let tick = world.resource::<crate::step::StepRequest>().tick;
        let now = i32::try_from(u64::from(tick.0) * u64::from(crate::MATCH_TICK_MS))
            .map_err(|_| "time overflow")?;
        let mut players: Vec<i32> = world
            .resource::<RoundScript>()
            .entities
            .values()
            .filter(|entity| entity.kind == EntityKind::Player)
            .map(|entity| entity.number)
            .collect();
        players.sort_unstable();
        let mut values: Vec<Value> = Vec::new();
        let count = players.last().map_or(0, |n| n + 1);
        for number in 0..count {
            values.push(if players.contains(&number) {
                Value::Int(tick.0 as i32 + 1)
            } else {
                Value::Undefined
            });
        }
        values.push(Value::Int(now + 1000));
        new_array(world, values)
    });
    registry.register(Function, "clientsyssetstate", |world, _, args| {
        let index = int(args, 0)?;
        if !(0..=8).contains(&index) {
            return Err(format!("ClientSysSetState - state index ({index}) out of bounds (0 - 8)"));
        }
        let registered = world
            .get_resource::<ClientSystems>()
            .is_some_and(|systems| (index as usize) < systems.0.len());
        if !registered {
            return Err(format!(
                "ClientSysSetState - state index ({index}) unregistered.  Use ClientSysRegister first."
            ));
        }
        let state = match args.get(1) {
            Some(_) => string(args, 1)?.replace(' ', "!"),
            None => String::new(),
        };
        world
            .get_resource_or_insert_with(ClientSystemStates::default)
            .0
            .insert(index as usize, state);
        Ok(Value::Undefined)
    });
    // A client field on an entity, in the pool of its kind.
    registry.register(Function, "codesetclientfield", |world, _, args| {
        let id = super::engine::entity_id(world, arg(args, 0)?)
            .map_err(|_| String::from("Entity does not exist."))?;
        let pool = {
            let runtime = world.resource::<RoundScript>();
            let entity = &runtime.entities[&id];
            match entity.kind {
                EntityKind::Actor => "actor",
                EntityKind::Player => "allplayers",
                _ if SCRIPT_MOVERS.contains(&&*entity.classname) => "scriptmover",
                _ if entity.classname.starts_with("zbarrier") => "zbarrier",
                _ => return Err("Unknown client field set.".into()),
            }
        };
        set_client_field(world, pool, id, args)
    });
    registry.register(
        Function,
        "codesetplayerstateclientfield",
        |world, _, args| {
            let receiver = arg(args, 0)?.clone();
            let client = client_of(world, &receiver).map_err(|_| {
                String::from("GScr_CodeSetPlayerStateClientField: must be called on a player.")
            })?;
            set_client_field(world, "toplayer", u64::from(client), args)
        },
    );
    registry.register(Method, "resetfov", |world, receiver, _| {
        client_of(world, receiver)?;
        Ok(Value::Undefined)
    });

    // IW4L's own builtins whose state and checks match Black Ops 2's handler
    // on the arguments it accepts.
    let shared = NativeRegistry::engine_services();
    for (namespace, name) in [
        (Function, "newhudelem"),
        (Method, "setshader"),
        (Method, "setvalue"),
        (Method, "freezecontrols"),
        (Function, "visionsetnight"),
        (Method, "setmodel"),
        (Method, "hide"),
        (Method, "show"),
        (Method, "delete"),
        (Method, "istouching"),
        (Method, "rotateroll"),
        (Method, "rotatepitch"),
        (Method, "rotateyaw"),
        (Method, "rotateto"),
        (Method, "moveto"),
        (Method, "linkto"),
        (Method, "unlink"),
        (Method, "playsound"),
        (Method, "playloopsound"),
        (Method, "stoploopsound"),
        (Method, "solid"),
        (Method, "notsolid"),
        (Function, "getnorthyaw"),
        (Method, "usebuttonpressed"),
        (Method, "getplayerangles"),
        (Function, "isalive"),
        // An entity object with a client.
        (Function, "isplayer"),
        (Function, "newclienthudelem"),
        (Method, "setdepthoffield"),
        (Method, "stopshellshock"),
        (Method, "stoprumble"),
        (Method, "closemenu"),
        (Method, "closeingamemenu"),
        (Function, "iprintln"),
        (Method, "getguid"),
        (Method, "settext"),
        (Method, "destroy"),
        (Function, "playfxontag"),
        // The player's weapons and movement: IW4L's, as Black Ops 2's own
        // movement is not yet read (`docs/fidelity/t6.md`).
        (Method, "spawn"),
        (Method, "enableweapons"),
        (Method, "disableweapons"),
        (Method, "setviewmodel"),
        (Method, "setmovespeedscale"),
        (Method, "giveweapon"),
        (Method, "givestartammo"),
        (Method, "givemaxammo"),
        (Method, "switchtoweapon"),
        (Method, "setmaxhealth"),
        (Method, "hasweapon"),
        (Method, "setweaponammoclip"),
        (Method, "setweaponammostock"),
        (Method, "getweaponammoclip"),
        (Method, "getweaponammostock"),
        (Method, "setactionslot"),
        (Method, "hasperk"),
        (Method, "setperk"),
        (Method, "unsetperk"),
        (Method, "getweaponslistprimaries"),
        (Method, "getweaponslist"),
        (Method, "takeweapon"),
        (Method, "takeallweapons"),
        (Method, "getcurrentweapon"),
        (Method, "isonground"),
        (Function, "bullettrace"),
    ] {
        if let Some(native) = shared.get(namespace, name) {
            registry.register(namespace, name, native);
        }
    }
    // Black Ops' own player builtins of the same name.
    let black_ops = NativeRegistry::engine_services().with_black_ops();
    for (namespace, name) in [
        (Method, "stopburning"),
        (Method, "setsprintduration"),
        (Method, "setsprintcooldown"),
        (Method, "playerknockback"),
        (Method, "cameraactivate"),
        (Method, "setblur"),
        (Method, "isinvehicle"),
        (Method, "isthrowinggrenade"),
        (Method, "setinvisibletoplayer"),
        (Method, "setvisibletoplayer"),
    ] {
        if let Some(native) = black_ops.get(namespace, name) {
            registry.register(namespace, name, native);
        }
    }
    actor::register(registry);
    anim::register(registry);
    movement::register(registry);
    attach::register(registry);
    barrier::register(registry);
    weapon::register_natives(registry);
    effect::register(registry);
    mover::register(registry);
    sound::register(registry);
    stats::register(registry);
}

/// The node field a node lookup key names; it has to hold a string.
fn node_string_field(key: &str) -> Result<fn(&crate::script::NavNode) -> &str, String> {
    Ok(match key {
        "targetname" => |node| &node.targetname,
        "target" => |node| &node.target,
        "animscript" => |node| &node.animscript,
        "script_linkname" => |node| node.fields.as_ref().map_or("", |f| &f.script_linkname),
        "script_noteworthy" => |node| node.fields.as_ref().map_or("", |f| &f.script_noteworthy),
        "origin" | "angles" | "radius" | "minusedistsq" | "spawnflags" | "type" | "suspended" => {
            return Err("key is not internally a string".into());
        }
        _ => return Err(format!("key '{key}' does not internally belong to nodes")),
    })
}

/// The entity fields Black Ops 2 looks an entity up by: its string keys.
const ENTITY_STRING_KEYS: [&str; 4] = ["classname", "target", "targetname", "script_noteworthy"];

/// Entity bounds the engine keeps that IW4L's entity does not.
#[derive(Resource, Default)]
struct EntityBounds(BTreeMap<u64, ([f32; 3], [f32; 3])>);

/// A trigger spawned by script: a radius trigger stands on its origin, a box
/// is centred on it; a use radius trigger is at least 72 wide and 96 high,
/// and a use box keeps the box's class name.
fn spawn_trigger(world: &mut World, classname: &str, args: &[Value]) -> Result<Value, String> {
    let origin = vector(args, 1)?;
    let flags = if args.len() >= 3 { int(args, 2)? } else { 0 };
    let (stored, cylinder, mins, maxs) = match classname {
        "trigger_radius" => {
            if args.len() < 5 {
                return Err("USAGE: spawn( \"trigger_radius\", <origin>, <spawnflags>, <radius>, <height> )".into());
            }
            let (r, h) = (float(args, 3)?, float(args, 4)?);
            ("trigger_radius", Some((r, h)), [-r, -r, 0.0], [r, r, h])
        }
        "trigger_radius_use" => {
            let r = if args.len() >= 4 {
                float(args, 3)?
            } else {
                72.0
            };
            let h = if args.len() >= 5 {
                float(args, 4)?
            } else {
                96.0
            };
            let r = if r - 72.0 < 0.0 { 72.0 } else { r };
            let h = if h - 96.0 < 0.0 { 96.0 } else { h };
            ("trigger_radius_use", Some((r, h)), [-r, -r, 0.0], [r, r, h])
        }
        _ => {
            if args.len() < 5 {
                return Err("USAGE: spawn( \"trigger_box\", <origin>, <spawnflags>, <width>, <length>, <height> )".into());
            }
            let half = [
                float(args, 3)? * 0.5,
                float(args, 4)? * 0.5,
                float(args, 5)? * 0.5,
            ];
            ("trigger_box", None, half.map(|h| -h), half)
        }
    };
    let mut runtime = world.resource_mut::<RoundScript>();
    let id = runtime.create_entity(EntityKind::Spawned, stored)?;
    runtime.set_object_field(id, "origin", Value::Vector(origin));
    runtime.set_object_field(id, "angles", Value::Vector([0.0; 3]));
    runtime.set_object_field(id, "spawnflags", Value::Int(flags));
    if let Some(entity) = runtime.entities.get_mut(&id) {
        entity.cylinder = cylinder;
    }
    world
        .get_resource_or_insert_with(EntityBounds::default)
        .0
        .insert(id, (mins, maxs));
    Ok(Value::Object(id))
}

fn bounds(world: &World, id: u64) -> ([f32; 3], [f32; 3]) {
    world
        .get_resource::<EntityBounds>()
        .and_then(|bounds| bounds.0.get(&id).copied())
        .unwrap_or(([0.0; 3], [0.0; 3]))
}

/// Linked bounds: a sentient's box on its origin, anything else the sphere
/// around its box; both an inch wider each way.
fn absolute_bounds(world: &mut World, id: u64) -> ([f32; 3], [f32; 3]) {
    let (mins, maxs) = bounds(world, id);
    let mut runtime = world.resource_mut::<RoundScript>();
    let origin = match runtime.object_field(id, "origin") {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    };
    let sentient = runtime
        .entities
        .get(&id)
        .is_some_and(|entity| matches!(entity.kind, EntityKind::Player | EntityKind::Actor));
    let (low, high) = if sentient {
        (
            [0, 1, 2].map(|i| origin[i] + mins[i]),
            [0, 1, 2].map(|i| origin[i] + maxs[i]),
        )
    } else {
        let radius = [0, 1, 2]
            .map(|i| mins[i].abs().max(maxs[i].abs()))
            .iter()
            .map(|a| a * a)
            .sum::<f32>()
            .sqrt();
        (origin.map(|o| o - radius), origin.map(|o| o + radius))
    };
    (low.map(|v| v - 1.0), high.map(|v| v + 1.0))
}

/// `disablegrenadesuicide` was called.
#[derive(Resource)]
struct GrenadeSuicideDisabled;

/// Each registered client system's state.
#[derive(Resource, Default)]
struct ClientSystemStates(BTreeMap<usize, String>);

/// A registered client field's value (`args` 1 names it, 2 is the value).
fn set_client_field(
    world: &mut World,
    pool: &str,
    owner: u64,
    args: &[Value],
) -> Result<Value, String> {
    let name = string(args, 1)?;
    let fields = world.get_resource_or_insert_with(ClientFields::default);
    let field = fields
        .0
        .iter()
        .position(|field| field.pool == pool && field.name == name)
        .ok_or_else(|| format!("clientfield {name} is not registered for {pool}"))?;
    let value = if fields.0[field].float {
        Value::Float(float(args, 2)?)
    } else {
        Value::Int(int(args, 2)?)
    };
    world
        .get_resource_or_insert_with(ClientFields::default)
        .1
        .insert((pool.to_owned(), owner, field), value);
    Ok(Value::Undefined)
}

/// The layout of a zombies player's stats.
const STATS_LAYOUT: &str = "ddl_zm/stats.ddl";

/// Where a stat path ends: its bits and the member's type.
struct StatAt {
    offset: u32,
    bits: u32,
    kind: u32,
    /// Not a whole array.
    leaf: bool,
}

/// Each player's stats buffer, by layout.
#[derive(Resource, Default)]
struct PlayerStats(BTreeMap<(u32, String), Vec<u8>>);

/// The first `count` arguments as a path into the stats layout: member
/// names, and array indices as numbers or the names of the array's enum.
/// `None` when a step does not resolve (the engine prints and goes on).
fn stat_path(
    world: &World,
    args: &[Value],
    count: usize,
) -> Result<Option<(String, StatAt)>, String> {
    if count > 8 {
        return Err("get/setdstat: path is too deep.".into());
    }
    let layout = world
        .resource::<RoundScript>()
        .restart
        .as_ref()
        .and_then(|plan| plan.layouts.get(STATS_LAYOUT).cloned())
        .ok_or("the zombies stats layout is not in the zones")?;
    let Some(root) = layout.structs.first() else {
        return Ok(None);
    };
    let mut offset = 0u32;
    let mut member: Option<&asset_core::BitMember> = None;
    let mut indexed = false;
    for index in 0..count {
        if !matches!(arg(args, index)?, Value::String(_) | Value::Int(_)) {
            return Err("get/setdstat: Expected strings or integers only.".into());
        }
        let name = string(args, index)?.to_ascii_lowercase();
        let find = |s| member_named(s, &name);
        match member {
            None => {
                let Some(found) = find(root) else {
                    return Ok(None);
                };
                offset = found.offset;
                member = Some(found);
                indexed = false;
            }
            Some(array) if array.array_size > 1 && !indexed => {
                let position = if array.enum_index < 0 {
                    atoi(&name)
                } else {
                    let Some(names) = layout.enums.get(array.enum_index as usize) else {
                        return Ok(None);
                    };
                    match names.iter().position(|n| n.eq_ignore_ascii_case(&name)) {
                        Some(position) => position as i32,
                        None => return Ok(None),
                    }
                };
                if position < 0 || position >= array.array_size {
                    return Ok(None);
                }
                offset += array.bits / array.array_size as u32 * position as u32;
                indexed = true;
            }
            Some(holder) if matches!(holder.kind, 8 | 9) => {
                let Some(found) = layout.structs.get(holder.external as usize).and_then(find)
                else {
                    return Ok(None);
                };
                offset += found.offset;
                member = Some(found);
                indexed = false;
            }
            Some(_) => return Ok(None),
        }
    }
    let Some(member) = member else {
        return Ok(None);
    };
    let bits = if member.array_size > 1 && indexed {
        member.bits / member.array_size as u32
    } else {
        member.bits
    };
    Ok(Some((
        layout.name.clone(),
        StatAt {
            offset,
            bits,
            kind: member.kind,
            leaf: member.array_size <= 1 || indexed,
        },
    )))
}

fn member_named<'a>(s: &'a asset_core::BitStruct, name: &str) -> Option<&'a asset_core::BitMember> {
    s.members.iter().find(|m| m.name.eq_ignore_ascii_case(name))
}

fn player_stats<'w>(world: &'w mut World, client: u32, layout: &str) -> &'w mut Vec<u8> {
    let bits = world
        .resource::<RoundScript>()
        .restart
        .as_ref()
        .and_then(|plan| plan.layouts.get(layout).map(|l| l.bits))
        .unwrap_or(0);
    world
        .get_resource_or_insert_with(PlayerStats::default)
        .into_inner()
        .0
        .entry((client, layout.to_owned()))
        .or_insert_with(|| vec![0; bits.div_ceil(8) as usize])
}

/// Bits from the least significant up, at most 64.
fn read_bits(buffer: &[u8], offset: u32, bits: u32) -> u64 {
    (0..bits.min(64)).fold(0u64, |value, i| {
        let at = offset + i;
        let bit = buffer
            .get((at / 8) as usize)
            .map_or(0, |byte| (byte >> (at % 8)) & 1);
        value | (u64::from(bit) << i)
    })
}

fn write_bits(buffer: &mut [u8], offset: u32, bits: u32, value: u64) {
    for i in 0..bits.min(64) {
        let at = offset + i;
        if let Some(byte) = buffer.get_mut((at / 8) as usize) {
            let mask = 1 << (at % 8);
            if value >> i & 1 != 0 {
                *byte |= mask;
            } else {
                *byte &= !mask;
            }
        }
    }
}

/// AI species by number.
const SPECIES: [&str; 4] = ["dog", "bigdog", "zombie", "zombie_dog"];

/// Each actor's species once its spawner gave it one; an actor without one
/// is taken as a zombie (`docs/fidelity/t6.md`).
#[derive(Resource, Default)]
struct ActorSpecies(BTreeMap<u64, usize>);

/// Living actors on a team of `mask` whose species `keep` admits, in spawn
/// order.
fn living_actors(world: &mut World, mask: u32, keep: impl Fn(usize) -> bool) -> Vec<Value> {
    let actors: Vec<u64> = world
        .resource::<RoundScript>()
        .entities
        .iter()
        .filter(|(_, entity)| entity.kind == EntityKind::Actor)
        .map(|(id, _)| *id)
        .collect();
    let mut listed = Vec::new();
    for id in actors {
        let species = world
            .get_resource::<ActorSpecies>()
            .and_then(|known| known.0.get(&id).copied())
            .unwrap_or(2);
        let mut runtime = world.resource_mut::<RoundScript>();
        let alive = match runtime.object_field(id, "health") {
            Value::Int(n) => n > 0,
            Value::Float(f) => f > 0.0,
            _ => false,
        };
        let team = match runtime.object_field(id, "team") {
            Value::String(name) => team(&[Value::String(name)], 0).unwrap_or(0),
            _ => 0,
        };
        if alive && mask & (1 << team) != 0 && keep(species) {
            listed.push(Value::Object(id));
        }
    }
    listed
}

/// Player state the engine keeps per client that IW4L's player does not.
#[derive(Default)]
struct PlayerState {
    client_dvars: BTreeMap<u8, String>,
    third_person: bool,
    ui_visibility: u32,
    current_streak: i32,
    spectate_denied: u32,
    usability_disabled: bool,
}

#[derive(Resource, Default)]
struct Players(BTreeMap<u32, PlayerState>);

/// `setinitialplayersconnected` was called.
#[derive(Resource)]
struct InitialPlayersConnected;

/// A player method's receiver as its client number.
fn client_of(world: &World, receiver: &Value) -> Result<u32, String> {
    let Value::Object(id) = receiver else {
        return Err("not an entity".into());
    };
    let runtime = world.resource::<RoundScript>();
    runtime.player_client(*id).ok_or_else(|| {
        runtime
            .entities
            .get(id)
            .map_or("not an entity".to_owned(), |entity| {
                format!("entity {} is not a player", entity.number)
            })
    })
}

fn player_state(world: &mut World, client: u32) -> &mut PlayerState {
    world
        .get_resource_or_insert_with(Players::default)
        .into_inner()
        .0
        .entry(client)
        .or_default()
}

/// A client dvar: a localized string or any value as text.
fn set_client_dvar(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    index: u8,
) -> Result<Value, String> {
    let client = client_of(world, receiver)?;
    let value = match arg(args, 0)? {
        Value::LocalizedString(text) => text.to_string(),
        _ => string(args, 0)?,
    };
    player_state(world, client)
        .client_dvars
        .insert(index, value);
    Ok(Value::Undefined)
}

/// The pools a client field belongs to.
const CLIENT_FIELD_POOLS: [&str; 11] = [
    "world",
    "actor",
    "vehicle",
    "allplayers",
    "toplayer",
    "scriptmover",
    "helicopter",
    "plane",
    "turret",
    "missile",
    "zbarrier",
];

#[derive(Clone, Debug)]
#[expect(
    dead_code,
    reason = "setclientfield reads them once it is Black Ops 2's"
)]
struct ClientField {
    pool: String,
    name: String,
    version: i32,
    bits: i32,
    float: bool,
}

/// Registered client fields, and the values set on them by pool, owner and
/// field.
#[derive(Resource, Default)]
#[expect(clippy::type_complexity, reason = "one map")]
struct ClientFields(Vec<ClientField>, BTreeMap<(String, u64, usize), Value>);

/// The minimap `setminimap` set.
#[derive(Resource)]
#[expect(dead_code, reason = "the compass reads it once it is Black Ops 2's")]
struct Minimap {
    material: String,
    corners: [f32; 4],
}

/// A vector parameter: only a vector is one.
fn vector(args: &[Value], index: usize) -> Result<[f32; 3], String> {
    match arg(args, index)? {
        Value::Vector(v) => Ok(*v),
        other => Err(format!("type {} is not a vector", type_name(other))),
    }
}

/// The squared distance between two vector parameters over their first
/// `axes` axes, summed as the engine sums them.
fn distance_squared(args: &[Value], axes: usize) -> Result<f32, String> {
    let (a, b) = (vector(args, 0)?, vector(args, 1)?);
    let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let planar = d[1] * d[1] + d[0] * d[0];
    Ok(if axes == 3 {
        planar + d[2] * d[2]
    } else {
        planar
    })
}

/// The animation tree `scriptmodelsuseanimtree` gave every script model.
#[derive(Resource)]
#[expect(
    dead_code,
    reason = "script model animation reads it once it is Black Ops 2's"
)]
struct ScriptModelAnimTree(Value);

/// The trigger classes that take a hint, and the script movers.
const HINT_CLASSES: [&str; 5] = [
    "trigger_use",
    "trigger_use_touch",
    "trigger_radius",
    "trigger_box",
    "trigger_radius_use",
];
const SCRIPT_MOVERS: [&str; 3] = ["script_model", "script_brushmodel", "script_origin"];

/// The cursor hints from 1.
const CURSOR_HINTS: [&str; 6] = [
    "HINT_NOICON",
    "HINT_SEAT",
    "HINT_ACTIVATE",
    "HINT_HEALTH",
    "HINT_SPECTATOR",
    "HINT_TEAMPOT",
];

/// Entity flags no IW4L state holds yet.
#[derive(Resource, Default)]
struct EntityFlags {
    low_priority_hint: BTreeMap<u64, bool>,
    ignore_team: std::collections::BTreeSet<u64>,
    trigger_team: BTreeMap<u64, usize>,
    disconnects_paths: std::collections::BTreeSet<u64>,
    weapon_hide_tags: BTreeMap<u64, String>,
    require_look_at: std::collections::BTreeSet<u64>,
}

/// The tables a lookup can name by number.
const NUMBERED_TABLES: [&str; 17] = [
    "mp/statstable.csv",
    "mp/statsmilestones1.csv",
    "mp/statsmilestones2.csv",
    "mp/statsmilestones3.csv",
    "mp/statsmilestones4.csv",
    "mp/globalcounters.csv",
    "mp/globalCounterRefs.csv",
    "mp/trackercolumns.csv",
    "sp/statstable.csv",
    "mp/rankicontable.csv",
    "mp/scoreinfo.csv",
    "mp/gametypesTable.csv",
    "zm/gamemodesTable.csv",
    "mp/rankicontable_zm.csv",
    "mp/gunlevels.csv",
    "zm/zm_statstable.csv",
    "mp/mtxitems.csv",
];

/// `setmatchflag`'s names: the flag word and the bit shifted down by four.
const MATCH_FLAGS: [(&str, usize, u32); 16] = [
    ("radar_allies", 0, 6),
    ("radar_axis", 0, 7),
    ("bomb_timer", 0, 9),
    ("bomb_timer_a", 0, 10),
    ("bomb_timer_b", 0, 11),
    ("ammocounterhide", 0, 12),
    ("enable_popups", 1, 8),
    ("hud_hardcore", 1, 13),
    ("pregame", 1, 14),
    ("final_killcam", 1, 4),
    ("round_end_killcam", 1, 5),
    ("cg_drawSpectatorMessages", 1, 15),
    ("disableIngameMenu", 1, 16),
    ("hud_zombie", 1, 0x36),
    ("game_ended", 1, 17),
    ("overtime", 0, 18),
];

const SCOREBOARD_COLUMNS: [&str; 26] = [
    "_invalid",
    "none",
    "score",
    "kills",
    "deaths",
    "assists",
    "defends",
    "plants",
    "defuses",
    "returns",
    "captures",
    "destructions",
    "kdratio",
    "survived",
    "stabs",
    "tomahawks",
    "humiliated",
    "x2score",
    "headshots",
    "downs",
    "revives",
    "agrkills",
    "hacks",
    "pointstowin",
    "killsconfirmed",
    "killsdenied",
];

#[derive(Resource, Default)]
struct Precached {
    status_icons: Vec<String>,
    menus: Vec<String>,
}

/// The match flag words the HUD reads, and the voice chat rules.
#[derive(Resource, Default)]
pub(crate) struct MatchFlags {
    pub(crate) match_words: [u32; 2],
    pub(crate) talk: u32,
}

/// Scores by team number.
#[derive(Resource, Default)]
struct TeamScores([i32; 10]);

/// `setmatchtalkflag`'s names, by bit.
const TALK_FLAGS: [&str; 7] = [
    "EveryoneHearsEveryone",
    "DeadChatWithDead",
    "DeadChatWithTeam",
    "DeadHearTeamLiving",
    "DeadHearAllLiving",
    "DeadHearKiller",
    "KillersHearVictim",
];

/// A team parameter as its number: allies 1, axis 2, team3 to team8, and
/// spectator 9; none, free and neutral are 0.
fn team(args: &[Value], index: usize) -> Result<usize, String> {
    let name = string(args, index)?;
    Ok(match name.as_str() {
        "allies" => 1,
        "axis" => 2,
        "team3" => 3,
        "team4" => 4,
        "team5" => 5,
        "team6" => 6,
        "team7" => 7,
        "team8" => 8,
        "spectator" => 9,
        "none" | "free" | "neutral" => 0,
        _ => {
            return Err(format!(
                "'{name}' is an illegal team string. Must be allies, axis, team3, team4, team5, team6, team7, team8, none, or spectator."
            ));
        }
    })
}

/// A builtin's float parameter: a float or an int.
fn float(args: &[Value], index: usize) -> Result<f32, String> {
    match arg(args, index)? {
        Value::Float(f) => Ok(*f),
        Value::Int(n) => Ok(*n as f32),
        other => Err(format!("type {} is not a float", type_name(other))),
    }
}

/// `%g`.
fn g(value: f32) -> String {
    crate::script::runtime::to_text(&Value::Float(value)).unwrap_or_else(|| value.to_string())
}

#[derive(Resource)]
#[expect(
    dead_code,
    reason = "the scoreboard reads it once it draws Black Ops 2's"
)]
pub(crate) struct ScoreboardColumns(pub(crate) [usize; 5]);

#[derive(Resource)]
#[expect(
    dead_code,
    reason = "the scoreboard reads it once it draws Black Ops 2's"
)]
pub(crate) struct RoundsPlayed(pub(crate) i32);

/// The game's one random state; the engine seeds it from the clock when the
/// server spawns, IW4L from its match random.
#[derive(Resource)]
struct HoldRand(u32);

fn irand(world: &mut World, min: i32, max: i32) -> i32 {
    let state = next_rand(world);
    let span = i64::from(max.wrapping_sub(min));
    (((i64::from((state >> 17) as i32) * span) >> 15) as i32).wrapping_add(min)
}

fn frand(world: &mut World, min: f32, max: f32) -> f32 {
    let state = next_rand(world);
    (f64::from(state >> 17) * (f64::from(max) - f64::from(min)) * f64::from(1.0f32 / 32768.0)
        + f64::from(min)) as f32
}

fn next_rand(world: &mut World) -> u32 {
    if !world.contains_resource::<HoldRand>() {
        let seed = super::math::random(world);
        world.insert_resource(HoldRand(seed));
    }
    let mut state = world.resource_mut::<HoldRand>();
    state.0 = state.0.wrapping_mul(0x343fd).wrapping_add(0x269ec3);
    state.0
}

/// The opcodes turning angles into one of their axes take only a vector.
fn angles_to(args: &[Value], name: &str, axis: usize) -> Result<Value, String> {
    let angles = match arg(args, 0)? {
        Value::Vector(v) => *v,
        other => {
            return Err(format!(
                "{name} expecting vector, given {})",
                type_name(other)
            ));
        }
    };
    let (forward, right, up) = math_iw4::angle_vectors(angles);
    Ok(Value::Vector([forward, right, up][axis]))
}

/// An array parameter's id.
fn array_id(args: &[Value], index: usize) -> Result<u64, String> {
    match arg(args, index)? {
        Value::Array(id) => Ok(*id),
        other => Err(format!("Parameter ({}) must be an array", type_name(other))),
    }
}

/// An array parameter's entries.
fn array(world: &World, args: &[Value], index: usize) -> Result<BTreeMap<ArrayKey, Value>, String> {
    match arg(args, index)? {
        Value::Array(id) => world
            .resource::<RoundScript>()
            .arrays
            .get(id)
            .cloned()
            .ok_or_else(|| "invalid array reference".into()),
        other => Err(format!("Parameter ({}) must be an array", type_name(other))),
    }
}

/// A lookup's table, by name or by number.
fn table_name(args: &[Value]) -> Result<String, String> {
    match arg(args, 0)? {
        Value::Int(n) => Ok(usize::try_from(*n)
            .ok()
            .and_then(|n| NUMBERED_TABLES.get(n))
            .ok_or("table number out of range")?
            .to_string()),
        Value::String(name) => Ok(name.to_string()),
        _ => Err("Invalid argument to stringtable lookup - either a filename or an ID\n".into()),
    }
}

/// `tablelookup`: the table by name or number, then one to three column and
/// value pairs and the column to answer; any miss answers "".
fn table_lookup(world: &World, args: &[Value]) -> Result<String, String> {
    let name = match arg(args, 0)? {
        Value::Int(_) | Value::String(_) => table_name(args)?,
        _ => return Ok(String::new()),
    };
    if !matches!(args.len(), 4 | 6 | 8) {
        return Err(format!(
            "Incorrect number of arguments ({}) passed to TableLookup!\n",
            args.len()
        ));
    }
    let mut keys = Vec::new();
    for pair in (1..args.len() - 1).step_by(2) {
        keys.push((int(args, pair)?, string(args, pair + 1)?));
    }
    let answer = int(args, args.len() - 1)?;
    let tables = world.resource::<RoundScript>().tables.clone();
    let Some(table) = crate::script::host::tables::table(&tables, &name) else {
        return Ok(String::new());
    };
    let row = (0..table.rows).find(|&row| {
        keys.iter().all(|(column, value)| {
            usize::try_from(*column)
                .ok()
                .and_then(|column| table.cell(row, column))
                .is_some_and(|cell| cell.eq_ignore_ascii_case(value))
        })
    });
    Ok(row
        .zip(usize::try_from(answer).ok())
        .and_then(|(row, column)| table.cell(row, column))
        .unwrap_or("")
        .to_owned())
}

fn entities_by_key(world: &mut World, args: &[Value]) -> Result<Vec<u64>, String> {
    let value = string(args, 0)?;
    let key = string(args, 1)?;
    if !ENTITY_STRING_KEYS.contains(&key.as_str()) {
        return Ok(Vec::new());
    }
    let mut runtime = world.resource_mut::<RoundScript>();
    let field = runtime.symbol(&key);
    let runtime = &*runtime;
    Ok(runtime
        .entities
        .iter()
        .filter(|(_, entity)| entity.kind != EntityKind::HudElem)
        .map(|(id, _)| *id)
        .filter(|id| {
            matches!(
                runtime.objects.get(id).and_then(|fields| fields.get(&field)),
                Some(Value::String(text)) if **text == *value
            )
        })
        .collect())
}
