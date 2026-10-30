//! Black Ops 2's stat builtins: what they add to the player's stats
//! (`ddl_zm/stats.ddl`), and the online match record, whose builtins read
//! their arguments only (`t6.stats.match_record`).

use super::super::super::args::arg;
use super::{
    SESSION_FLAGS, client_of, int, player_stats, read_bits, stat_path, string, write_bits,
};
use crate::script::{MatchScript, Namespace, NativeRegistry, Value};
use bevy_ecs::prelude::World;

/// The session is online: the match recorder runs while the server does.
const RECORDING: bool = SESSION_FLAGS & 0x4 != 0;
/// An online session that is not a private one keeps stats.
const STATS_KEPT: bool = SESSION_FLAGS & 0x4 != 0 && SESSION_FLAGS & 0x8 == 0;

/// Whether this match keeps stats: a multiplayer match here is a private one.
fn stats_kept(world: &World) -> bool {
    STATS_KEPT && !super::multiplayer::is_multiplayer(world)
}

/// A bool dvar, `default` when the match has not set it.
fn dvar_on(world: &World, name: &str, default: bool) -> bool {
    world
        .resource::<MatchScript>()
        .dvars
        .get(name)
        .map_or(default, |value| super::atoi(value) != 0)
}

fn dvar_text(world: &World, name: &str) -> String {
    world
        .resource::<MatchScript>()
        .dvars
        .get(name)
        .cloned()
        .unwrap_or_else(String::new)
}

/// Stats are written: tracking is not turned off and the session keeps them.
fn tracking(world: &World) -> bool {
    !dvar_on(world, "scr_disablestattracking", false) && stats_kept(world)
}

/// The amount, the last argument: a float or an int.
fn amount(args: &[Value]) -> Result<f32, String> {
    match arg(args, args.len().wrapping_sub(1))? {
        Value::Float(f) => Ok(*f),
        Value::Int(n) => Ok(*n as f32),
        _ => Err("addweaponstat: Expected floats or integers only.".into()),
    }
}

/// The stat's name, the argument before the amount: a string or an int.
fn stat_name(args: &[Value], builtin: &str) -> Result<String, String> {
    let index = args.len().wrapping_sub(2);
    match arg(args, index)? {
        Value::String(_) | Value::Int(_) => string(args, index),
        _ => Err(format!("{builtin}: Expected strings or integers only.")),
    }
}

/// Adds `amount` to the stat a path names; false when the path does not resolve.
fn add(world: &mut World, client: u32, path: &[&str], amount: f32) -> Result<bool, String> {
    let steps: Vec<Value> = path.iter().map(|step| Value::string(step)).collect();
    let Some((layout, at)) = stat_path(world, &steps, steps.len())? else {
        return Ok(false);
    };
    if !at.leaf {
        return Err("AddWeaponStat: trying to set a non leaf member of the ddl".into());
    }
    let fix_negative = dvar_on(world, "fixnegativelosses", true);
    let stats = player_stats(world, client, &layout);
    let current = read_bits(stats, at.offset, at.bits);
    let value = match at.kind {
        0..=2 => {
            let current = current as u32;
            let delta = amount as i32;
            if fix_negative && delta < 0 && delta.unsigned_abs() > current {
                0
            } else {
                u64::from(current.wrapping_add_signed(delta))
            }
        }
        3 => u64::from((current as u32).wrapping_add_signed(amount as i32)),
        5 | 6 => u64::from((f32::from_bits(current as u32) + amount).to_bits()),
        _ => return Err("AddWeaponStat: stat type undefined".into()),
    };
    write_bits(stats, at.offset, at.bits, value);
    Ok(true)
}

fn steps(path: &[String]) -> Vec<Value> {
    path.iter().map(|step| Value::string(step)).collect()
}

/// A stat's value as a number; `None` when the path does not resolve.
fn get(world: &mut World, client: u32, path: &[String]) -> Result<Option<f32>, String> {
    let steps = steps(path);
    let Some((layout, at)) = stat_path(world, &steps, steps.len())? else {
        return Ok(None);
    };
    let raw = read_bits(player_stats(world, client, &layout), at.offset, at.bits) as u32;
    Ok(Some(match at.kind {
        0..=2 => raw as f32,
        3 => raw as i32 as f32,
        5 | 6 => f32::from_bits(raw),
        _ => return Err("GetStatValue: stat type undefined".into()),
    }))
}

/// Sets a stat from a number; false when the path does not resolve.
fn set(world: &mut World, client: u32, path: &[String], value: f32) -> Result<bool, String> {
    let steps = steps(path);
    let Some((layout, at)) = stat_path(world, &steps, steps.len())? else {
        return Ok(false);
    };
    let raw = match at.kind {
        0..=2 => u64::from(value.round_ties_even() as i64 as u32),
        3 => u64::from(value as i32 as u32),
        5 | 6 => u64::from(value.to_bits()),
        _ => return Err("SetStatFromState: stat type undefined".into()),
    };
    write_bits(
        player_stats(world, client, &layout),
        at.offset,
        at.bits,
        raw,
    );
    Ok(true)
}

/// `addplayerstatwithgametype` (`with_list`) and `addgametypestat`: the stat
/// for the match's gametype and for its gametype group, and with the list's.
fn add_gametype_stat(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    with_list: bool,
) -> Result<Value, String> {
    const BUILTIN: &str = "AddPlayerStatWithGameType";
    let client = client_of(world, receiver)
        .map_err(|_| format!("{BUILTIN}: entity must be a player entity"))?;
    if !tracking(world) {
        return Ok(Value::Undefined);
    }
    let amount = amount(args)?;
    let name = stat_name(args, BUILTIN)?;
    if with_list {
        add_list_stat(world, client, &name, amount)?;
    }
    for (root, dvar) in [
        ("playerstatsbygametype", "g_gametype"),
        ("playerstatsbygametypegroup", "ui_zm_gamemodegroup"),
    ] {
        let gametype = dvar_text(world, dvar);
        if !add(
            world,
            client,
            &[root, &gametype, &name, "StatValue"],
            amount,
        )? {
            return Err(format!("{BUILTIN}: path not found."));
        }
    }
    Ok(Value::Undefined)
}

/// The stat in the player's stats list; the challenges it counts toward are
/// not kept (`t6.stats.challenges`).
fn add_list_stat(world: &mut World, client: u32, name: &str, amount: f32) -> Result<(), String> {
    if !add(
        world,
        client,
        &["playerstatslist", name, "StatValue"],
        amount,
    )? {
        return Err("AddPlayerStat: path not found.".into());
    }
    Ok(())
}

/// A match record builtin's player receiver.
fn recorded_player(world: &World, receiver: &Value) -> Result<(), String> {
    client_of(world, receiver).map(|_| ())
}

/// The match recorder runs.
fn recording(world: &World) -> bool {
    RECORDING && dvar_on(world, "sv_running", true)
}

/// A function builtin's player argument: an entity, then one with a client.
fn player_param(world: &World, args: &[Value], builtin: &str) -> Result<(), String> {
    let entity = arg(args, 0)?;
    if !matches!(entity, Value::Object(_)) {
        return Err(format!("{builtin} Error: param 0 is not an entity."));
    }
    client_of(world, entity)
        .map(|_| ())
        .map_err(|_| format!("{builtin} Error: param 0 is not an player."))
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    registry.register(Method, "addplayerstat", |world, receiver, args| {
        let client = client_of(world, receiver)
            .map_err(|_| String::from("AddPlayerStat: entity must be a player entity"))?;
        if !tracking(world) {
            return Ok(Value::Undefined);
        }
        let amount = amount(args)?;
        let name = stat_name(args, "AddPlayerStat")?;
        add_list_stat(world, client, &name, amount)?;
        Ok(Value::Undefined)
    });
    registry.register(
        Method,
        "addplayerstatwithgametype",
        |world, receiver, args| add_gametype_stat(world, receiver, args, true),
    );
    registry.register(Method, "addgametypestat", |world, receiver, args| {
        add_gametype_stat(world, receiver, args, false)
    });
    // Weapon stats are not kept (`t6.stats.match_record`).
    registry.register(Method, "addweaponstat", |world, receiver, args| {
        client_of(world, receiver)
            .map_err(|_| String::from("AddWeaponStat: entity must be a player entity"))?;
        if !RECORDING {
            return Ok(Value::Undefined);
        }
        let Value::String(_) = arg(args, 0)? else {
            return Err("addweaponstat: Expected strings or integers only.".into());
        };
        match arg(args, 1)? {
            Value::String(_) | Value::Int(_) => {}
            _ => return Err("addweaponstat: Expected strings or integers only.".into()),
        }
        amount(args)?;
        if args.len() > 3 {
            int(args, 3)?;
        }
        if args.len() > 4 {
            int(args, 4)?;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "updatestatratio", |world, receiver, args| {
        let client = client_of(world, receiver)
            .map_err(|_| String::from("UpdateStatRatio: entity must be a player entity"))?;
        if !tracking(world) {
            return Ok(Value::Undefined);
        }
        let mut names = Vec::with_capacity(3);
        for back in [3, 2, 1] {
            let index = args.len().wrapping_sub(back);
            match arg(args, index)? {
                Value::String(_) | Value::Int(_) => names.push(string(args, index)?),
                _ => {
                    return Err("UpdateStatRatio: Expected strings or integers only.".into());
                }
            }
        }
        let [ratio, numerator, denominator] = &names[..] else {
            return Ok(Value::Undefined);
        };
        let gametype = dvar_text(world, "g_gametype");
        for (root, missing) in [
            (None, "GetStatValue: path not found."),
            (
                Some(gametype.as_str()),
                "GetGameTypeStatValue: path not found.",
            ),
        ] {
            let path = |name: &str| -> Vec<String> {
                match root {
                    None => vec!["playerstatslist".into(), name.into(), "StatValue".into()],
                    Some(gametype) => vec![
                        "playerstatsbygametype".into(),
                        gametype.into(),
                        name.into(),
                        "StatValue".into(),
                    ],
                }
            };
            let over = get(world, client, &path(numerator))?.ok_or(missing)?;
            let under = get(world, client, &path(denominator))?.ok_or(missing)?;
            let under = if under <= 1.0 { 1.0 } else { under };
            if !set(world, client, &path(ratio), over * 1000.0 / under)? {
                return Err("GetStatValue: path not found.".into());
            }
        }
        Ok(Value::Undefined)
    });

    registry.register(Function, "recordzombieroundstart", |_, _, _| {
        Ok(Value::Undefined)
    });
    registry.register(Function, "recordzombieroundend", |_, _, _| {
        Ok(Value::Undefined)
    });
    registry.register(Function, "recordmatchinit", |_, _, _| Ok(Value::Undefined));
    registry.register(Function, "recordnumzombierounds", |world, _, args| {
        if recording(world) {
            int(args, 0)?;
        }
        Ok(Value::Undefined)
    });
    registry.register(
        Function,
        "matchrecorderincrementheaderstat",
        |world, _, args| {
            if recording(world) {
                string(args, 0)?;
                int(args, 1)?;
            }
            Ok(Value::Undefined)
        },
    );
    registry.register(
        Function,
        "recordmatchsummaryzombieendgamedata",
        |world, _, args| {
            if recording(world) {
                string(args, 0)?;
                string(args, 1)?;
                string(args, 2)?;
                int(args, 3)?;
            }
            Ok(Value::Undefined)
        },
    );
    registry.register(Method, "recordzombiezone", |world, receiver, args| {
        recorded_player(world, receiver)?;
        if recording(world) {
            string(args, 0)?;
            string(args, 1)?;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "recordplayerdeathzombies", |world, receiver, _| {
        recorded_player(world, receiver)?;
        Ok(Value::Undefined)
    });
    registry.register(
        Method,
        "recordplayerdownzombies",
        |world, receiver, args| {
            recorded_player(world, receiver)?;
            string(args, 0)?;
            Ok(Value::Undefined)
        },
    );
    registry.register(Function, "recordplayerstats", |world, _, args| {
        player_param(world, args, "recordPlayerStats")?;
        if !matches!(arg(args, 1)?, Value::String(_)) {
            return Err("recordPlayerStats Error: param 1 is not a string.".into());
        }
        int(args, 2)?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "recordplayermatchend", |world, _, args| {
        player_param(world, args, "recordplayermatchend")?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "recordgameresult", |_, _, args| {
        string(args, 0)?;
        Ok(Value::Undefined)
    });
    // League play only; the owner's match is not one.
    registry.register(Function, "recordleaguewinner", |_, _, _| {
        Ok(Value::Undefined)
    });
    registry.register(Function, "bbpostdemostreamstatsforround", |_, _, args| {
        if !matches!(arg(args, 0)?, Value::Int(_)) {
            return Err(
                "GScr_BBPostDemoStreamStatsForRound: Invalid round number, must be an integer"
                    .into(),
            );
        }
        Ok(Value::Undefined)
    });
    // Its handler is empty.
    registry.register(Function, "reportmtu", |_, _, _| Ok(Value::Undefined));
    registry.register(Method, "uploadleaderboards", |world, receiver, _| {
        client_of(world, receiver)
            .map_err(|_| String::from("uploadLeaderboards: entity must be a player entity"))?;
        Ok(Value::Undefined)
    });
    // The game history is not kept (`t6.stats.match_record`).
    registry.register(Method, "gamehistoryfinishmatch", |world, receiver, args| {
        client_of(world, receiver)
            .map_err(|_| String::from("setdstat: entity must be a player entity"))?;
        if dvar_on(world, "scr_disablesetdstat", false) || !stats_kept(world) {
            return Ok(Value::Undefined);
        }
        for index in 0..5 {
            int(args, index)?;
        }
        super::float(args, 5)?;
        Ok(Value::Undefined)
    });
}
