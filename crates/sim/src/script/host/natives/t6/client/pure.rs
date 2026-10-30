//! Black Ops 2's client builtins that read no client game state: the
//! language helpers, math, strings, arrays, dvars and session queries.

use crate::script::host::args::arg;
use crate::script::host::arrays::new_array;
use crate::script::runtime::to_text;
use crate::script::{ArrayKey, Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

/// The session flags the client reads are the process's, the same session
/// the server's scripts see (`natives::t6`): online and public.
const SESSION_FLAGS: u32 = 0x14;

/// The packages `hasdlcavailable` knows, by name (case aside).
const DLC_PACKAGES: [&str; 10] = [
    "dlc0",
    "dlczm0",
    "dlc1",
    "dlc2",
    "dlc3",
    "dlc4",
    "dlc5",
    "dlc0dd",
    "seasonpass",
    "textures",
];

/// Mounted DLC packages (`dlc0`, `dlczm0`, `dlc3`, ...) in the install.
#[derive(Resource, Default)]
pub(crate) struct MountedDlc(pub(crate) Vec<String>);

/// The client game's random state, seeded from the server's
/// match seed (`G_InitGame` -> `BG_srand`) and shared with client sound
/// randoms (`SND_UpdateRandoms`).
#[derive(Resource)]
pub(crate) struct HoldRand(pub(crate) u32);

static SYS_TIME_BASE: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

/// `Sys_Milliseconds`: milliseconds elapsed since its first read.
pub(crate) fn sys_milliseconds() -> i32 {
    SYS_TIME_BASE
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_millis() as i32
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::Function;

    // Debug output and geometry: the arguments are read as the handler
    // reads them, for as many as were passed; what they draw is nothing in
    // a release build.
    registry.register(Function, "println", |_, _, _| Ok(Value::Undefined));
    registry.register(Function, "print3d", |_, _, args| {
        let count = args.len();
        if (2..=6).contains(&count) {
            if count >= 6 {
                int(args, 5)?;
            }
            if count >= 5 {
                float(args, 4)?;
            }
            if count >= 4 {
                float(args, 3)?;
            }
            if count >= 3 {
                vector(args, 2)?;
            }
            text(args, 1)?;
            vector(args, 0)?;
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "line", |_, _, args| {
        let count = args.len();
        if (2..=6).contains(&count) {
            if count >= 6 {
                int(args, 5)?;
            }
            if count >= 5 {
                int(args, 4)?;
            }
            if count >= 4 {
                float(args, 3)?;
            }
            if count >= 3 {
                vector(args, 2)?;
            }
            vector(args, 1)?;
            vector(args, 0)?;
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "box", |_, _, args| {
        let count = args.len();
        if (1..=8).contains(&count) {
            if count >= 8 {
                int(args, 7)?;
            }
            if count >= 7 {
                int(args, 6)?;
            }
            if count >= 6 {
                float(args, 5)?;
            }
            if count >= 5 {
                vector(args, 4)?;
            }
            if count >= 4 {
                float(args, 3)?;
            }
            if count >= 3 {
                vector(args, 2)?;
            }
            if count >= 2 {
                vector(args, 1)?;
            }
            vector(args, 0)?;
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "debugstar", |_, _, args| {
        let count = args.len();
        if (1..=3).contains(&count) {
            if count >= 3 {
                vector(args, 2)?;
            }
            if count >= 2 {
                int(args, 1)?;
            }
            vector(args, 0)?;
        }
        Ok(Value::Undefined)
    });
    // A failed assert is a terminal error for the client's scripts.
    registry.register(Function, "assert", |_, _, args| {
        if int(args, 0)? != 0 {
            return Ok(Value::Undefined);
        }
        if args.len() > 1 {
            let message = text(args, 1)?;
            return Err(format!(
                "assert fail: {}",
                String::from_utf8_lossy(&message)
            ));
        }
        Err("assert fail".into())
    });
    registry.register(Function, "assertmsg", |_, _, args| {
        let message = text(args, 0)?;
        Err(format!(
            "assert fail: {}",
            String::from_utf8_lossy(&message)
        ))
    });

    // The game's generator over the client's state; the ranges are checked
    // as the client's handlers check them.
    registry.register(Function, "randomint", |world, _, args| {
        let max = int(args, 0)?;
        if max <= 0 {
            return Err("RandomInt parm must be positive integer.\n".into());
        }
        Ok(Value::Int(irand(world, 0, max)))
    });
    registry.register(Function, "randomintrange", |world, _, args| {
        let (min, max) = (int(args, 0)?, int(args, 1)?);
        if max <= min {
            return Err("RandomIntRange range must be positive integer.\n".into());
        }
        Ok(Value::Int(irand(world, min, max)))
    });
    // An empty range gives its end.
    registry.register(Function, "randomfloatrange", |world, _, args| {
        let (min, max) = (float(args, 0)?, float(args, 1)?);
        if min > max {
            return Err("Scr_RandomFloatRange range must be positive float.\n".into());
        }
        Ok(Value::Float(frand(world, min, max)))
    });

    // `randomfloat(max)` is `randomfloatrange(0, max)` without its range check.
    registry.register(Function, "randomfloat", |world, _, args| {
        let max = float(args, 0)?;
        Ok(Value::Float(frand(world, 0.0, max)))
    });

    // Casts: a string is read as `atoi` and `atof` read it.
    registry.register(Function, "int", |_, _, args| match arg(args, 0)? {
        Value::String(text) => Ok(Value::Int(atoi(text.as_bytes()))),
        Value::Float(f) => Ok(Value::Int(truncate(*f))),
        Value::Int(n) => Ok(Value::Int(*n)),
        other => Err(format!("cannot cast {} to int", type_name(other))),
    });
    registry.register(Function, "float", |_, _, args| match arg(args, 0)? {
        Value::String(text) => Ok(Value::Float(atof(text.as_bytes()))),
        Value::Float(f) => Ok(Value::Float(*f)),
        Value::Int(n) => Ok(Value::Float(*n as f32)),
        other => Err(format!("cannot cast {} to float", type_name(other))),
    });
    registry.register(Function, "isarray", |_, _, args| {
        Ok(Value::Int(matches!(arg(args, 0)?, Value::Array(_)).into()))
    });

    // Math, each summed in the order the handler sums it.
    registry.register(Function, "distance", |_, _, args| {
        Ok(Value::Float(distance_squared(args)?.sqrt()))
    });
    registry.register(Function, "distancesquared", |_, _, args| {
        Ok(Value::Float(distance_squared(args)?))
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
    registry.register(Function, "vectordot", |_, _, args| {
        let (a, b) = (vector(args, 0)?, vector(args, 1)?);
        Ok(Value::Float(a[1] * b[1] + a[0] * b[0] + a[2] * b[2]))
    });
    // A zero vector stays zero.
    registry.register(Function, "vectornormalize", |_, _, args| {
        let v = vector(args, 0)?;
        let mut length = (v[1] * v[1] + v[0] * v[0] + v[2] * v[2]).sqrt();
        if -length >= 0.0 {
            length = 1.0;
        }
        let scale = 1.0 / length;
        Ok(Value::Vector([v[0] * scale, v[1] * scale, v[2] * scale]))
    });
    // The first unless the second is greater (or either is NaN).
    registry.register(Function, "max", |_, _, args| {
        let (a, b) = (float(args, 0)?, float(args, 1)?);
        Ok(Value::Float(if f64::from(a) - f64::from(b) >= 0.0 {
            a
        } else {
            b
        }))
    });
    // `b - a` not below zero gives `a`; otherwise, NaN too, `b`.
    registry.register(Function, "min", |_, _, args| {
        let (a, b) = (float(args, 0)?, float(args, 1)?);
        Ok(Value::Float(if f64::from(b) - f64::from(a) >= 0.0 {
            a
        } else {
            b
        }))
    });
    // Degrees, turned to radians by the engine's single-precision factor
    // and taken in the x87's precision.
    registry.register(Function, "cos", |_, _, args| {
        Ok(Value::Float(radians(float(args, 0)?).cos() as f32))
    });
    registry.register(Function, "sin", |_, _, args| {
        Ok(Value::Float(radians(float(args, 0)?).sin() as f32))
    });
    registry.register(Function, "sqrt", |_, _, args| {
        Ok(Value::Float(f64::from(float(args, 0)?).sqrt() as f32))
    });
    // The fraction is held to 0..1 (NaN stays NaN).
    registry.register(Function, "lerpfloat", |_, _, args| {
        let (from, to, fraction) = (float(args, 0)?, float(args, 1)?, float(args, 2)?);
        let upper = if f64::from(fraction) - 1.0 >= 0.0 {
            1.0
        } else {
            fraction
        };
        let fraction = if -fraction >= 0.0 { 0.0 } else { upper };
        Ok(Value::Float((to - from) * fraction + from))
    });
    registry.register(Function, "getminbitcountfornum", |_, _, args| {
        let n = int(args, 0)?;
        if n <= 0 {
            return Err("GScr_GetMinBitCountForNum: parm must be positive integer.\n".into());
        }
        Ok(Value::Int(32 - n.leading_zeros() as i32))
    });

    // Strings, as bytes; a built string holds at most 1023 of them.
    registry.register(Function, "tolower", |_, _, args| {
        let text = text(args, 0)?;
        if text.len() >= 1024 {
            return Err("string too long".into());
        }
        Ok(Value::byte_string(&text.to_ascii_lowercase()))
    });
    registry.register(Function, "getsubstr", |_, _, args| {
        let text = text(args, 0)?;
        let start = int(args, 1)?;
        let end = if args.len() >= 3 {
            int(args, 2)?
        } else {
            i32::MAX
        };
        let mut part = Vec::new();
        let mut at = start;
        while at < end {
            if part.len() >= 1024 {
                return Err("string too long".into());
            }
            let Some(&byte) = usize::try_from(at).ok().and_then(|at| text.get(at)) else {
                break;
            };
            part.push(byte);
            at += 1;
        }
        Ok(Value::byte_string(&part))
    });
    registry.register(Function, "issubstr", |_, _, args| {
        let (text, part) = (text(args, 0)?, text(args, 1)?);
        let found = part.is_empty() || text.windows(part.len()).any(|window| window == part);
        Ok(Value::Int(found.into()))
    });
    // Tokens between any of the delimiter bytes; empty ones are skipped.
    registry.register(Function, "strtok", |world, _, args| {
        let (text, delimiters) = (text(args, 0)?, text(args, 1)?);
        let mut tokens = Vec::new();
        let mut token = Vec::new();
        for &byte in &text {
            if delimiters.contains(&byte) {
                if !token.is_empty() {
                    tokens.push(Value::byte_string(&token));
                    token.clear();
                }
                continue;
            }
            token.push(byte);
            if token.len() >= 1024 {
                return Err("string too long".into());
            }
        }
        if !token.is_empty() {
            tokens.push(Value::byte_string(&token));
        }
        new_array(world, tokens)
    });
    // The first 1023 bytes split at each space; the part after the last
    // space is always kept, empty or not.
    registry.register(Function, "splitargs", |world, _, args| {
        let mut text = text(args, 0)?;
        text.truncate(1023);
        let parts = text
            .split(|&byte| byte == b' ')
            .map(Value::byte_string)
            .collect();
        new_array(world, parts)
    });

    // Arrays. Values match when their type and stored value do: an object
    // by identity, a float by its bits, a vector by its allocation.
    registry.register(Function, "array", |world, _, args| {
        let values = args
            .iter()
            .take_while(|value| !matches!(value, Value::Undefined))
            .cloned()
            .collect();
        new_array(world, values)
    });
    registry.register(Function, "spawnstruct", |world, _, _| {
        let mut runtime = world.resource_mut::<RoundScript>();
        let id = runtime.next_object;
        runtime.next_object = id.checked_add(1).ok_or("object identifier exhausted")?;
        runtime.objects.insert(id, BTreeMap::new());
        Ok(Value::Object(id))
    });
    // Newest key first.
    registry.register(Function, "getarraykeys", |world, _, args| {
        let id = array_param(args, 0)?;
        let keys = entries(world, id)?
            .into_keys()
            .rev()
            .map(|key| match key {
                ArrayKey::Integer(n) => Value::Int(n),
                ArrayKey::String(s) => Value::String(s),
            })
            .collect();
        new_array(world, keys)
    });
    registry.register(Function, "isinarray", |world, _, args| {
        let id = array_param(args, 0)?;
        let wanted = arg(args, 1)?;
        Ok(Value::Int(contains(&entries(world, id)?, wanted)?.into()))
    });
    // Without keeping keys the remaining entries are numbered from 0.
    registry.register(Function, "arrayremovevalue", |world, _, args| {
        let id = array_param(args, 0)?;
        let wanted = arg(args, 1)?.clone();
        let keep_keys = args.len() == 3 && int(args, 2)? != 0;
        let mut kept = BTreeMap::new();
        for (key, value) in entries(world, id)? {
            if !same_value(&value, &wanted)? {
                kept.insert(key, value);
            }
        }
        if !keep_keys {
            kept = renumbered(kept.into_values());
        }
        set_entries(world, id, kept)?;
        Ok(Value::Undefined)
    });
    // An index inside the array moves the entries from it up by one; any
    // other appends.
    registry.register(Function, "arrayinsert", |world, _, args| {
        let id = array_param(args, 0)?;
        let value = arg(args, 1)?.clone();
        let index = int(args, 2)?;
        let mut array = entries(world, id)?;
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
        set_entries(world, id, array)?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "arraycopy", |world, _, args| {
        let id = array_param(args, 0)?;
        let copy = entries(world, id)?;
        keyed_array(world, copy)
    });
    // Keeping keys needs each array's oldest key to be a string. The
    // engine's check for keys in both compares only the two arrays' oldest
    // values. Without duplicates the second array's values already in the
    // first are left out.
    registry.register(Function, "arraycombine", |world, _, args| {
        let first_id = array_param(args, 0)?;
        let second_id = array_param(args, 1)?;
        let duplicates = int(args, 2)? != 0;
        let keep_requested = int(args, 3)? != 0;
        let first = entries(world, first_id)?;
        let second = entries(world, second_id)?;
        let string_keyed = |array: &BTreeMap<ArrayKey, Value>| {
            array
                .keys()
                .next()
                .is_none_or(|key| matches!(key, ArrayKey::String(_)))
        };
        let keep_keys = keep_requested && string_keyed(&first) && string_keyed(&second);
        if keep_keys
            && let (Some((_, oldest)), Some((_, other))) =
                (first.first_key_value(), second.first_key_value())
            && same_value(oldest, other)?
        {
            return Err("Duplicate key found when merging arrays".into());
        }
        let mut combined: Vec<(ArrayKey, Value)> = first.clone().into_iter().collect();
        for (key, value) in second {
            if !duplicates && contains(&first, &value)? {
                continue;
            }
            combined.push((key, value));
        }
        if keep_keys {
            keyed_array(world, combined.into_iter().collect())
        } else {
            new_array(
                world,
                combined.into_iter().map(|(_, value)| value).collect(),
            )
        }
    });

    // Time, from the host's clocks.
    registry.register(Function, "getsystemtime", |_, _, args| {
        let utc = args.len() == 1 && int(args, 0)? != 0;
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "system time precedes the Unix epoch")?
            .as_secs();
        // The engine keeps the time's low 32 bits, signed.
        let seconds = i64::from(seconds as u32 as i32);
        if seconds < 0 {
            return Err(
                "t6.csc.time.utc_range: a clock past 2038 reads as negative, which the \
                 engine's gmtime refuses"
                    .into(),
            );
        }
        use chrono::Timelike;
        let (h, m, s) = if utc {
            let Some(dt) = chrono::DateTime::from_timestamp(seconds, 0) else {
                return Ok(Value::Vector([0.0, 0.0, 0.0]));
            };
            (dt.hour(), dt.minute(), dt.second())
        } else {
            let Some(dt) = chrono::TimeZone::timestamp_opt(&chrono::Local, seconds, 0).single()
            else {
                return Ok(Value::Vector([0.0, 0.0, 0.0]));
            };
            (dt.hour(), dt.minute(), dt.second())
        };
        Ok(Value::Vector([h as f32, m as f32, s as f32]))
    });
    registry.register(Function, "getrealtime", |_, _, _| {
        Ok(Value::Int(sys_milliseconds()))
    });

    // Dvars. Each handler checks its arguments, then reads whether the
    // engine registered the dvar and its flags: setdvar sets only a dvar
    // scripts may set (or makes one), setsaveddvar only a saved one, and
    // setdvarfloat holds the value to the dvar's domain (or makes one).
    registry.register(Function, "setdvar", |world, _, args| {
        let name = text(args, 0)?;
        let value = text(args, 1)?;
        check_dvar_name(&name)?;
        let name_str = String::from_utf8_lossy(&name);
        if let Some(spec) = super::super::dvars::lookup(&name_str)
            && spec.flags & 0x4000 == 0
        {
            return Err(format!(
                "Invalid Dvar set: {name_str} - Internal Dvars cannot be changed by script. Use 'setsaveddvar' to alter SAVED internal dvars\n"
            ));
        }
        let val_str: String = value
            .iter()
            .take(0x3ff)
            .map(|&b| if b == b'"' { '\'' } else { char::from(b) })
            .collect();
        diag::info!(Sim, "csc: setdvar {name_str} = {val_str}");
        crate::script::set_dvar(world, &name_str, &val_str);
        Ok(Value::Undefined)
    });
    registry.register(Function, "setsaveddvar", |world, _, args| {
        let name = text(args, 0)?;
        if matches!(arg(args, 1)?, Value::LocalizedString(_)) {
            return Err("Cannot set saved dvar on client side with a string value".into());
        }
        let value = text(args, 1)?;
        check_dvar_name(&name)?;
        let name_str = String::from_utf8_lossy(&name);
        let Some(spec) = super::super::dvars::lookup(&name_str) else {
            return Err(format!(
                "SetSavedDvar(): The dvar \"{name_str}\" does not exist."
            ));
        };
        if spec.flags & 0x1000 == 0 {
            return Err("SetSavedDvar can only be called on dvars with the SAVED flag set".into());
        }
        let val_str: String = value
            .iter()
            .take(0x3ff)
            .map(|&b| if b == b'"' { '\'' } else { char::from(b) })
            .collect();
        diag::info!(Sim, "csc: setsaveddvar {name_str} = {val_str}");
        crate::script::set_dvar(world, &name_str, &val_str);
        Ok(Value::Undefined)
    });
    registry.register(Function, "setdvarfloat", |world, _, args| {
        let name = text(args, 0)?;
        let mut val = float(args, 1)?;
        let name_str = String::from_utf8_lossy(&name);
        if let Some(spec) = super::super::dvars::lookup(&name_str)
            && let Some((lo, hi)) = spec.float_domain
        {
            val = val.clamp(lo, hi);
        }
        let val_str = val.to_string();
        diag::info!(Sim, "csc: setdvarfloat {name_str} = {val_str}");
        crate::script::set_dvar(world, &name_str, &val_str);
        Ok(Value::Undefined)
    });

    // Session and platform. One handler answers both getmaxlocalclients and
    // sessionmodeiszombiesgame with 1.
    registry.register(Function, "sessionmodeisonlinegame", |_, _, _| {
        Ok(Value::Int((SESSION_FLAGS >> 2 & 1) as i32))
    });
    registry.register(Function, "sessionmodeiszombiesgame", |_, _, _| {
        Ok(Value::Int(1))
    });
    registry.register(Function, "getmaxlocalclients", |_, _, _| Ok(Value::Int(1)));
    registry.register(Function, "isps3", |_, _, _| Ok(Value::Int(0)));
    // This build's split screen query is always false.
    registry.register(Function, "issplitscreen", |_, _, _| Ok(Value::Int(0)));
    // The host is the primary local client, the owner's: 0 (-1 is none).
    registry.register(Function, "issplitscreenhost", |_, _, args| {
        Ok(Value::Int((int(args, 0)? == 0).into()))
    });
    registry.register(Function, "ismaturecontentenabled", |_, _, _| {
        Ok(Value::Int(1))
    });
    registry.register(Function, "hasdlcavailable", |world, _, args| {
        let name = text(args, 0)?;
        if !DLC_PACKAGES
            .iter()
            .any(|package| package.as_bytes().eq_ignore_ascii_case(&name))
        {
            return Err(format!(
                "Unknown DLC package: {}\n",
                String::from_utf8_lossy(&name)
            ));
        }
        let available = world.get_resource::<MountedDlc>().is_some_and(|dlc| {
            dlc.0
                .iter()
                .any(|pkg| pkg.as_bytes().eq_ignore_ascii_case(&name))
        });
        Ok(Value::Int(available.into()))
    });
    // No demo plays, so no demo camera is in use.
    registry.register(Function, "isdemoplaying", |_, _, _| Ok(Value::Int(0)));
    for name in [
        "demoisanyfreemovecamera",
        "demoismoviecamera",
        "demoiseditcamera",
        "demoisdollycamera",
    ] {
        registry.register(Function, name, |_, _, _| Ok(Value::Int(0)));
    }
}

/// Black Ops 2's name for a value's type on the script stack.
fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Undefined => "undefined",
        Value::Int(_) => "int",
        Value::Float(_) => "float",
        Value::String(_) => "string",
        Value::LocalizedString(_) => "localized string",
        Value::Vector(_) => "vector",
        Value::Object(_) | Value::Array(_) => "object",
        Value::Function(_) | Value::Builtin(_) => "function",
        Value::Animation { .. } | Value::AnimationTree(_) => "animation",
    }
}

/// An int parameter: only an int is one.
fn int(args: &[Value], index: usize) -> Result<i32, String> {
    match arg(args, index)? {
        Value::Int(n) => Ok(*n),
        other => Err(format!("type {} is not an int", type_name(other))),
    }
}

/// A float parameter: a float or an int.
fn float(args: &[Value], index: usize) -> Result<f32, String> {
    match arg(args, index)? {
        Value::Float(f) => Ok(*f),
        Value::Int(n) => Ok(*n as f32),
        other => Err(format!("type {} is not a float", type_name(other))),
    }
}

/// A vector parameter: only a vector is one.
fn vector(args: &[Value], index: usize) -> Result<[f32; 3], String> {
    match arg(args, index)? {
        Value::Vector(v) => Ok(*v),
        other => Err(format!("type {} is not a vector", type_name(other))),
    }
}

/// A string parameter's bytes: a string, or a number or vector as text.
fn text(args: &[Value], index: usize) -> Result<Vec<u8>, String> {
    match arg(args, index)? {
        Value::String(text) => Ok(text.as_bytes().to_vec()),
        other => to_text(other)
            .map(String::into_bytes)
            .ok_or_else(|| format!("cannot cast {} to string", type_name(other))),
    }
}

/// An array parameter: a value that is no object fails as an object read.
fn array_param(args: &[Value], index: usize) -> Result<u64, String> {
    match arg(args, index)? {
        Value::Array(id) => Ok(*id),
        Value::Object(_) => Err("Parameter (object) must be an array".into()),
        other => Err(format!("type {} is not an object", type_name(other))),
    }
}

fn entries(world: &World, id: u64) -> Result<BTreeMap<ArrayKey, Value>, String> {
    world
        .resource::<RoundScript>()
        .arrays
        .get(&id)
        .cloned()
        .ok_or_else(|| "invalid array reference".into())
}

fn set_entries(world: &mut World, id: u64, array: BTreeMap<ArrayKey, Value>) -> Result<(), String> {
    let mut runtime = world.resource_mut::<RoundScript>();
    let slot = runtime
        .arrays
        .get_mut(&id)
        .ok_or("invalid array reference")?;
    *slot = array;
    Ok(())
}

/// A new array holding these keys and values.
fn keyed_array(world: &mut World, array: BTreeMap<ArrayKey, Value>) -> Result<Value, String> {
    let mut runtime = world.resource_mut::<RoundScript>();
    let id = runtime.next_object;
    runtime.next_object = id.checked_add(1).ok_or("object identifier exhausted")?;
    runtime.arrays.insert(id, array);
    Ok(Value::Array(id))
}

fn renumbered(values: impl Iterator<Item = Value>) -> BTreeMap<ArrayKey, Value> {
    values
        .enumerate()
        .map(|(index, value)| (ArrayKey::Integer(index as i32), value))
        .collect()
}

/// Whether an array holds the value; a match decides over a comparison
/// IW4L cannot make.
fn contains(array: &BTreeMap<ArrayKey, Value>, wanted: &Value) -> Result<bool, String> {
    let mut unknown = None;
    for value in array.values() {
        match same_value(value, wanted) {
            Ok(true) => return Ok(true),
            Ok(false) => {}
            Err(error) => unknown = Some(error),
        }
    }
    unknown.map_or(Ok(false), Err)
}

/// Whether two values are the same as the engine's array helpers compare
/// them: the same type and the same stored value.
fn same_value(a: &Value, b: &Value) -> Result<bool, String> {
    Ok(match (a, b) {
        (Value::Undefined, Value::Undefined) => true,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x.to_bits() == y.to_bits(),
        (Value::String(x), Value::String(y)) => x == y,
        (Value::LocalizedString(x), Value::LocalizedString(y)) => x == y,
        (Value::Object(x), Value::Object(y)) | (Value::Array(x), Value::Array(y)) => x == y,
        (Value::Function(x), Value::Function(y)) | (Value::Builtin(x), Value::Builtin(y)) => x == y,
        (Value::Animation { .. }, Value::Animation { .. })
        | (Value::AnimationTree(_), Value::AnimationTree(_)) => a == b,
        (Value::Vector(x), Value::Vector(y)) => {
            if x.iter().zip(y).any(|(p, q)| p.to_bits() != q.to_bits()) {
                false
            } else {
                return Err(
                    "t6.csc.array.vector_identity: the engine compares vectors by their \
                     allocation, which IW4L's vectors do not have"
                        .into(),
                );
            }
        }
        _ => false,
    })
}

/// The squared distance between two vector parameters, summed as the
/// handler sums it.
fn distance_squared(args: &[Value]) -> Result<f32, String> {
    let (a, b) = (vector(args, 0)?, vector(args, 1)?);
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    Ok(d[1] * d[1] + d[0] * d[0] + d[2] * d[2])
}

/// `cvttsd2si`: out of range or NaN reads as the lowest int.
fn truncate(f: f32) -> i32 {
    if f.is_nan() || f >= 2_147_483_648.0 || f < -2_147_483_648.0 {
        i32::MIN
    } else {
        f as i32
    }
}

fn is_c_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// `atoi`, which is `strtol` in base 10: out of range holds at the ends.
fn atoi(text: &[u8]) -> i32 {
    let mut at = text.iter().take_while(|&&b| is_c_space(b)).count();
    let negative = match text.get(at) {
        Some(b'-') => {
            at += 1;
            true
        }
        Some(b'+') => {
            at += 1;
            false
        }
        _ => false,
    };
    let magnitude = text[at..]
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .fold(0i64, |n, d| (n * 10 + i64::from(d - b'0')).min(1 << 32));
    let value = if negative { -magnitude } else { magnitude };
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// `atof`: the leading decimal number (its exponent written with `e` or
/// `d`), 0 when there is none.
fn atof(text: &[u8]) -> f32 {
    let start = text.iter().take_while(|&&b| is_c_space(b)).count();
    let rest = &text[start..];
    let digits = |at: usize| {
        rest.get(at..).map_or(0, |tail| {
            tail.iter().take_while(|b| b.is_ascii_digit()).count()
        })
    };
    let mut number = String::new();
    let mut at = 0;
    if let Some(&sign @ (b'-' | b'+')) = rest.first() {
        number.push(char::from(sign));
        at += 1;
    }
    let whole = digits(at);
    number.extend(rest[at..at + whole].iter().map(|&b| char::from(b)));
    at += whole;
    let mut fraction = 0;
    if rest.get(at) == Some(&b'.') {
        fraction = digits(at + 1);
        number.push('.');
        number.extend(
            rest[at + 1..at + 1 + fraction]
                .iter()
                .map(|&b| char::from(b)),
        );
        at += 1 + fraction;
    }
    if whole + fraction == 0 {
        return 0.0;
    }
    if let Some(b'e' | b'E' | b'd' | b'D') = rest.get(at) {
        let mut exponent = at + 1;
        let mut sign = None;
        if let Some(&s @ (b'-' | b'+')) = rest.get(exponent) {
            sign = Some(char::from(s));
            exponent += 1;
        }
        let count = digits(exponent);
        if count > 0 {
            number.push('e');
            number.extend(sign);
            number.extend(
                rest[exponent..exponent + count]
                    .iter()
                    .map(|&b| char::from(b)),
            );
        }
    }
    number.parse::<f64>().map_or(0.0, |n| n as f32)
}

/// `Dvar_IsValidName`: letters, digits and underscores (an empty name
/// passes).
fn check_dvar_name(name: &[u8]) -> Result<(), String> {
    if name.iter().all(|&b| b.is_ascii_alphanumeric() || b == b'_') {
        Ok(())
    } else {
        Err(format!(
            "Dvar {} has an invalid dvar name",
            String::from_utf8_lossy(name)
        ))
    }
}

fn next_rand(world: &mut World) -> u32 {
    if !world.contains_resource::<HoldRand>() {
        let seed = crate::script::host::natives::math::random(world);
        world.insert_resource(HoldRand(seed));
    }
    let mut state = world.resource_mut::<HoldRand>();
    state.0 = state.0.wrapping_mul(0x343fd).wrapping_add(0x269ec3);
    state.0
}

/// A random int in `min..max`: the state's top 15 bits scale the span.
pub(super) fn irand(world: &mut World, min: i32, max: i32) -> i32 {
    let state = next_rand(world);
    let span = i64::from(max.wrapping_sub(min));
    (((i64::from(state >> 17) * span) >> 15) as i32).wrapping_add(min)
}

/// A random float in `min..max`, the state's top 15 bits scaling the span.
/// Degrees to radians as the client's `cos` and `sin` take them.
fn radians(degrees: f32) -> f64 {
    f64::from(degrees) * f64::from(0.017_453_292_f32)
}

fn frand(world: &mut World, min: f32, max: f32) -> f32 {
    frand_wide(world, min, max) as f32
}

/// `frand` as the engine's x87 code leaves it before a store.
pub(super) fn frand_wide(world: &mut World, min: f32, max: f32) -> f64 {
    let state = next_rand(world);
    f64::from(state >> 17) * (f64::from(max) - f64::from(min)) * f64::from(1.0f32 / 32768.0)
        + f64::from(min)
}
