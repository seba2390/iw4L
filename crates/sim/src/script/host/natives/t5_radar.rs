//! Black Ops multiplayer's radar: each team's spy plane and satellite
//! levels (`setTeamSpyplane`, `setTeamSatellite`), which a player's own
//! `hasSpyplane` and `hasSatellite` override. The client's compass sweeps
//! for enemies by the player's spy plane level and shows them live by its
//! satellite level. `setClientUIVisibilityFlag` sets the player's UI
//! flags and `setMatchFlag` the match's flags: the client's HUD visibility
//! bits 0–3 and 4–16, which its menus and compass read.

use super::super::args::{int, string};
use crate::frame::FrameWorld;
use crate::script::Namespace::{Function, Method};
use crate::script::{NativeRegistry, RoundScript, Value};
use crate::world::ClientId;
use bevy_ecs::prelude::{Resource, World};

/// The teams' levels by team number: none, axis, allies.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub(crate) struct TeamRadar {
    spyplane: [i32; 3],
    satellite: [i32; 3],
}

/// The players' UI flags by client number.
#[derive(Resource, Clone, Debug, Default)]
pub(crate) struct UiFlags(std::collections::BTreeMap<u32, u8>);

/// The match flags as visibility bits (`setMatchFlag`).
#[derive(Resource, Clone, Copy, Debug, Default)]
pub(crate) struct MatchFlags(u32);

/// `setMatchFlag`'s names and the visibility bits they set.
const MATCH_FLAGS: [(&str, u32); 13] = [
    ("radar_allies", 6),
    ("radar_axis", 7),
    ("bomb_timer", 9),
    ("bomb_timer_a", 10),
    ("bomb_timer_b", 11),
    ("ammocounterhide", 12),
    ("enable_popups", 8),
    ("hud_hardcore", 13),
    ("pregame", 14),
    ("final_killcam", 4),
    ("round_end_killcam", 5),
    ("cg_drawSpectatorMessages", 15),
    ("disableIngameMenu", 16),
];

fn set_match_flag(world: &mut World, _: &Value, args: &[Value]) -> Result<Value, String> {
    // Any other count of arguments sets nothing.
    if args.len() != 2 {
        return Ok(Value::Undefined);
    }
    let name = string(args, 0)?;
    let on = int(args, 1)? != 0;
    let bit = MATCH_FLAGS
        .iter()
        .find(|(flag, _)| *flag == name)
        .map(|(_, bit)| *bit)
        .ok_or_else(|| {
            format!("GScr_SetMatchFlag: Couldn't find a matching bit for flag name : {name} ")
        })?;
    let flags = &mut world
        .get_resource_or_insert_with(MatchFlags::default)
        .into_inner()
        .0;
    if on {
        *flags |= 1 << bit;
    } else {
        *flags &= !(1 << bit);
    }
    Ok(Value::Undefined)
}

/// The UI flags by bit.
const UI_FLAGS: [&str; 4] = [
    "hud_visible",
    "g_compassShowEnemies",
    "radar_client",
    "killcam_nemesis",
];

fn set_ui_flag(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let id = super::engine::entity_id(world, receiver).map_err(|_| "not an entity".to_owned())?;
    let runtime = world.resource::<RoundScript>();
    let client = runtime.player_client(id).ok_or_else(|| {
        let number = runtime.entities.get(&id).map_or(-1, |e| e.number);
        format!("entity {number} is not a player")
    })?;
    // Any other count of arguments sets nothing.
    if args.len() != 2 {
        return Ok(Value::Undefined);
    }
    let name = string(args, 0)?;
    let on = int(args, 1)? != 0;
    let bit = UI_FLAGS
        .iter()
        .position(|flag| *flag == name)
        .ok_or_else(|| {
            format!("PlayerCmd_SetClientFlag: Couldn't find a matching bit for flag name : {name} ")
        })?;
    let flags = world
        .get_resource_or_insert_with(UiFlags::default)
        .into_inner()
        .0
        .entry(client)
        .or_default();
    if on {
        *flags |= 1 << bit;
    } else {
        *flags &= !(1 << bit);
    }
    Ok(Value::Undefined)
}

fn team_index(args: &[Value]) -> Result<usize, String> {
    let team = string(args, 0)?;
    match team.as_str() {
        "none" => Ok(0),
        "axis" => Ok(1),
        "allies" => Ok(2),
        _ => Err(format!(
            "Illegal team string '{team}'. Must be allies, axis, or none."
        )),
    }
}

fn levels(world: &mut World) -> &mut TeamRadar {
    world
        .get_resource_or_insert_with(TeamRadar::default)
        .into_inner()
}

/// A player's script field as the game's client field reads it: an int.
fn player_int(world: &World, client: u32, name: &str) -> i32 {
    let runtime = world.resource::<RoundScript>();
    let Some(object) = runtime.players.get(&client).map(|slot| slot.object) else {
        return 0;
    };
    let symbol = runtime
        .program
        .as_ref()
        .and_then(|program| program.symbol_ids.get(name))
        .or_else(|| runtime.dynamic_symbols.get(name));
    match symbol.and_then(|symbol| runtime.objects.get(&object)?.get(symbol)) {
        Some(Value::Int(n)) => *n,
        Some(Value::Float(f)) => *f as i32,
        _ => 0,
    }
}

/// Each frame's end gives every player its radar levels: its own
/// `hasSpyplane` / `hasSatellite` when set, else its team's.
/// Only Black Ops' builtins keep these, so another game's match has none.
pub(crate) fn publish(world: &mut World) {
    let (teams, ui_flags, match_flags) = match (
        world.get_resource::<TeamRadar>(),
        world.get_resource::<UiFlags>(),
        world.get_resource::<MatchFlags>(),
    ) {
        (None, None, None) => return,
        (teams, flags, match_flags) => (
            teams.copied().unwrap_or(TeamRadar {
                spyplane: [0; 3],
                satellite: [0; 3],
            }),
            flags.cloned().unwrap_or(UiFlags(Default::default())),
            match_flags.copied().unwrap_or(MatchFlags(0)),
        ),
    };
    let clients: Vec<u32> = world
        .resource::<RoundScript>()
        .players
        .keys()
        .copied()
        .collect();
    for client in clients {
        let team = match super::super::players::load_field(world, client, "sessionteam") {
            Some(Value::String(team)) => match &*team {
                "axis" => Some(1),
                "allies" => Some(2),
                "spectator" => None,
                _ => Some(0),
            },
            _ => Some(0),
        };
        let team_level = |by_team: [i32; 3]| team.map_or(0, |t| by_team[t]);
        let own = |name| player_int(world, client, name);
        let spyplane = match own("hasspyplane") {
            0 => team_level(teams.spyplane),
            own => own,
        };
        let satellite = match own("hassatellite") {
            0 => team_level(teams.satellite),
            own => own,
        };
        let mut frame = FrameWorld::from_world(world);
        if frame.client_meta(ClientId(client)).is_some() {
            let meta = frame.client_meta_mut(ClientId(client));
            meta.radar_levels = [spyplane, satellite];
            meta.ui_flags =
                u32::from(ui_flags.0.get(&client).copied().unwrap_or(0)) | match_flags.0;
        }
    }
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Method, "setclientuivisibilityflag", set_ui_flag);
    registry.register(Function, "setmatchflag", set_match_flag);
    registry.register(Function, "setteamspyplane", |world, _, args| {
        let team = team_index(args)?;
        let level = int(args, 1)?;
        levels(world).spyplane[team] = level;
        Ok(Value::Undefined)
    });
    registry.register(Function, "getteamspyplane", |world, _, args| {
        let team = team_index(args)?;
        Ok(Value::Int(levels(world).spyplane[team]))
    });
    registry.register(Function, "setteamsatellite", |world, _, args| {
        let team = team_index(args)?;
        let level = int(args, 1)?;
        levels(world).satellite[team] = level;
        Ok(Value::Undefined)
    });
    registry.register(Function, "getteamsatellite", |world, _, args| {
        let team = team_index(args)?;
        Ok(Value::Int(levels(world).satellite[team]))
    });
}
