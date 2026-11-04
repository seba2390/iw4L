//! Black Ops multiplayer's popup commands: each builtin sends its player a
//! command, and the client fills the HUD's local variables from its tables and
//! moves a popup's items to `Start`.

use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeSet;
use std::sync::Arc;

use super::super::args::{float, int};
use crate::frame::FrameWorld;
use crate::script::Namespace::Method;
use crate::script::host::tables::{table, table_search};
use crate::script::{NativeRegistry, Value};
use crate::world::ClientId;
use crate::{MenuCommandKind, MenuPopUp, PopUpText};

/// The ids of the popups not shown that have been logged.
#[derive(Resource, Default)]
struct PopUpsLogged(BTreeSet<&'static str>);

const POPUP_MENU: &str = "InGamePopUps";
const MEDAL_TABLE: &str = "mp/medaltable.csv";
const RANK_TABLE: &str = "mp/rankTable.csv";
const RANK_ICON_TABLE: &str = "mp/rankIconTable.csv";
const STATS_TABLE: &str = "mp/statstable.csv";
/// The rank table's column of the CoD Points a rank pays.
const RANK_COD_POINTS_COLUMN: usize = 17;
/// The rank table's short rank number, as a popup shows it.
const RANK_SHORT_COLUMN: usize = 14;
/// `displayWagerPopup`'s sound, which the client always plays.
const WAGER_SOUND: &str = "uin_challenge_repeatable";

fn dvar_on(world: &World, name: &str) -> bool {
    crate::script::host::actor_nav::dvar_f32(world, name, 0.0) != 0.0
}

fn optional_text(args: &[Value], at: usize) -> String {
    args.get(at)
        .and_then(|value| super::player::text(value).ok())
        .map_or_else(String::new, |text: Arc<str>| text.to_string())
}

/// A cell of a loaded table, by the row whose first column is `key`.
fn cell(world: &World, name: &str, key: &str, column: usize) -> Option<String> {
    let runtime = world.resource::<crate::script::RoundScript>();
    let table = table(&runtime.tables, name)?;
    let row = table_search(table, 0, key)?;
    table
        .cell(row, column)
        .filter(|cell| !cell.is_empty())
        .map(str::to_owned)
}

fn center_lines(a: PopUpText, b: PopUpText, c: Option<PopUpText>) -> Vec<(String, PopUpText)> {
    let mut locals = vec![
        ("popUpCenterStringA".to_owned(), a),
        ("popUpCenterStringB".to_owned(), b),
    ];
    if let Some(c) = c {
        locals.push(("popUpCenterStringC".to_owned(), c));
    }
    locals
}

fn send(world: &mut World, receiver: &Value, popup: MenuPopUp) -> Result<Value, String> {
    let client = super::player::player(world, receiver)?;
    super::player::send_menu_command(world, client, MenuCommandKind::PopUp(popup));
    Ok(Value::Undefined)
}

fn center_popup(item: &str, locals: Vec<(String, PopUpText)>, sound: String) -> MenuPopUp {
    MenuPopUp {
        menu: POPUP_MENU.to_owned(),
        item: item.to_owned(),
        locals,
        hide_others: true,
        sound,
    }
}

/// `MEDAL_XP_POPUP` when the medal pays anything, or
/// nothing.
fn xp_line(points: i32) -> PopUpText {
    if points > 0 {
        PopUpText::with("MEDAL_XP_POPUP", vec![PopUpText::plain(points.to_string())])
    } else {
        PopUpText::default()
    }
}

/// `displayMedal(index, teambased, xpscale, sound)`. A wager match shows the
/// CoD Points it pays; a match that pays XP folds the XP into its second line.
fn display_medal(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let index = int(args, 0)?.to_string();
    let team_based = int(args, 1)? != 0;
    let scale = float(args, 2)?;
    let sound = optional_text(args, 3);
    let (Some(name), Some(desc)) = (
        cell(world, MEDAL_TABLE, &index, 2),
        cell(world, MEDAL_TABLE, &index, 3),
    ) else {
        return Ok(Value::Undefined);
    };
    let pay_column = if team_based { 4 } else { 5 };
    let pay = cell(world, MEDAL_TABLE, &index, pay_column).map_or(0, |pay| super::iw4::atoi(&pay));
    let name = PopUpText::plain(name);
    let desc = PopUpText::plain(desc);
    let lines = if dvar_on(world, "xblive_wagermatch") {
        let bet = crate::script::host::actor_nav::dvar_f32(world, "scr_wagerBet", 0.0) as i32;
        let points = ((bet * pay) as f32 * 0.01) as i32;
        let points = PopUpText::with("MEDAL_CP_POPUP", vec![PopUpText::plain(points.to_string())]);
        center_lines(name, desc, Some(points))
    } else if dvar_on(world, "onlinegame") || dvar_on(world, "xblive_basictraining") {
        let xp = xp_line((pay as f32 * scale) as i32);
        let second = PopUpText::with("MEDAL_DESC_POPUP", vec![desc, xp.clone()]);
        center_lines(name, second, Some(xp))
    } else {
        center_lines(name, desc, None)
    };
    send(world, receiver, center_popup("MedalPopUp", lines, sound))
}

/// `displayRankUp(rank, prestige, sound)`: the rank's icon and the CoD Points
/// it pays come from the rank tables, one row below the rank sent.
fn display_rank_up(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let rank = int(args, 0)?;
    let prestige = int(args, 1)?;
    let sound = optional_text(args, 2);
    let row = (rank - 1).to_string();
    let Some(icon) = cell(world, RANK_ICON_TABLE, &row, (prestige + 1) as usize) else {
        return Ok(Value::Undefined);
    };
    let Some(points) = cell(world, RANK_TABLE, &row, RANK_COD_POINTS_COLUMN) else {
        return Ok(Value::Undefined);
    };
    let promoted = if (0..999).contains(&rank) {
        PopUpText::with("RANK_PROMOTED_TO", vec![PopUpText::plain(rank.to_string())])
    } else {
        PopUpText::plain("RANK_PROMOTED_TO")
    };
    let points = PopUpText::with("RANK_COD_POINTS", vec![PopUpText::plain(points)]);
    let mut locals = center_lines(promoted, points, None);
    locals.push((
        "popUpCenterMaterial".to_owned(),
        PopUpText::name(icon.clone()),
    ));
    locals.push(("rankUpPopUpIcon0".to_owned(), PopUpText::name(icon)));
    send(world, receiver, center_popup("RankPopUp", locals, sound))
}

/// `displayWagerPopup(message, points[, subMessage])`.
fn display_wager_popup(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let message = optional_text(args, 0);
    let points = int(args, 1)?;
    let sub = optional_text(args, 2);
    let xp = (points > 0).then(|| xp_line(points));
    let lines = center_lines(PopUpText::plain(message), PopUpText::plain(sub), xp);
    send(
        world,
        receiver,
        center_popup("WagerPopUp", lines, WAGER_SOUND.to_owned()),
    )
}

/// `displayKillstreak(count, tableNumber)`: the killstreak's icon and name
/// come from the stats table; its first line is the kill count, or its name
/// when it was bought and not earned.
fn display_killstreak(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let count = int(args, 0)?;
    let number = int(args, 1)?.to_string();
    let (Some(icon), Some(name)) = (
        cell(world, STATS_TABLE, &number, 6),
        cell(world, STATS_TABLE, &number, 3),
    ) else {
        return Ok(Value::Undefined);
    };
    let first = if count == 0 {
        PopUpText::plain(name)
    } else {
        PopUpText::with(
            "KILLSTREAK_KILLCOUNT_DISPLAY",
            vec![PopUpText::plain(count.to_string())],
        )
    };
    let press = PopUpText::with(
        "KILLSTREAK_PRESS_TO_KILLSTREAK",
        vec![PopUpText::plain("[{+actionslot 4}]")],
    );
    let mut locals = center_lines(first, press, None);
    locals.push(("popUpCenterMaterial".to_owned(), PopUpText::name(icon)));
    send(
        world,
        receiver,
        center_popup("KillstreakPopUp", locals, String::new()),
    )
}

/// `displayGameModeMessage(message[, sound])`: its slide-out, which leaves the
/// centre popups be.
fn display_game_mode_message(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let popup = MenuPopUp {
        menu: "GameMessageSlideOut".to_owned(),
        item: "SlideOut".to_owned(),
        locals: vec![(
            "gameModeMessage0".to_owned(),
            PopUpText::plain(optional_text(args, 0)),
        )],
        hide_others: false,
        sound: optional_text(args, 1),
    };
    send(world, receiver, popup)
}

/// A popup that does not reach the HUD: logs its id once and keeps the args,
/// as the other presented builtins do.
fn not_shown(
    world: &mut World,
    receiver: &Value,
    key: &'static str,
    id: &'static str,
    args: &[Value],
) -> Result<Value, String> {
    if world
        .get_resource_or_insert_with(PopUpsLogged::default)
        .0
        .insert(id)
    {
        diag::warn!(Sim, "{id}: {key} does not reach the client");
    }
    super::player::present(world, receiver, key, args)
}

/// A rank's icon, by its prestige, and its short rank.
fn rank_row(world: &World, rank: i32, prestige: i32) -> Option<(String, String)> {
    let row = rank.to_string();
    Some((
        cell(world, RANK_ICON_TABLE, &row, prestige as usize + 1)?,
        cell(world, RANK_TABLE, &row, RANK_SHORT_COLUMN)?,
    ))
}

/// `displayTeamMessage(message, player[, sound])`: the slide-out naming the
/// player, with their rank. A player past the top of the table shows none.
fn display_team_message(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let about = super::player::player(world, super::super::args::arg(args, 1)?)?;
    let (name, rank, prestige) = {
        let frame = FrameWorld::from_world(world);
        let Some(meta) = frame.client_meta(ClientId(about)) else {
            return Ok(Value::Undefined);
        };
        let end = meta.name.iter().position(|byte| *byte == 0);
        let name = String::from_utf8_lossy(&meta.name[..end.unwrap_or(meta.name.len())]);
        (name.into_owned(), meta.rank, meta.prestige)
    };
    let mut locals = vec![
        (
            "teamMessage".to_owned(),
            PopUpText::plain(optional_text(args, 0)),
        ),
        ("teamMessageName".to_owned(), PopUpText::name(name)),
        ("teamClanTagPopUp".to_owned(), PopUpText::name("")),
    ];
    if let Some((icon, short)) = rank_row(world, rank, prestige) {
        locals.push(("teamMessageRankIcon".to_owned(), PopUpText::name(icon)));
        locals.push(("teamMessageRank".to_owned(), PopUpText::plain(short)));
    }
    let popup = MenuPopUp {
        menu: "teamMessageSlideOut".to_owned(),
        item: "SlideOut".to_owned(),
        locals,
        hide_others: false,
        sound: optional_text(args, 2),
    };
    send(world, receiver, popup)
}

/// The player card an obituary puts up for the one who killed and the one who
/// died (the client's obituary): the other player's name and rank, under the
/// kill's line. A revive and a suicide put up none.
pub(super) fn obituary_cards(
    world: &mut World,
    victim: Option<ClientId>,
    attacker: Option<ClientId>,
    weapon: &str,
) {
    let (Some(victim), Some(attacker)) = (victim, attacker) else {
        return;
    };
    if victim == attacker || weapon == "syrette_mp" {
        return;
    }
    let card = |world: &mut World, about: ClientId, line: &str, team_line: &str| {
        let (team_kill, name, rank, prestige) = {
            let frame = FrameWorld::from_world(world);
            let (killer, killed) = (frame.client_meta(attacker)?, frame.client_meta(victim)?);
            let meta = frame.client_meta(about)?;
            (
                killer.client_state_team != 0
                    && killer.client_state_team == killed.client_state_team,
                {
                    let end = meta.name.iter().position(|byte| *byte == 0);
                    String::from_utf8_lossy(&meta.name[..end.unwrap_or(meta.name.len())])
                        .into_owned()
                },
                meta.rank,
                meta.prestige,
            )
        };
        let mut locals = vec![
            (
                "killStringPopUp".to_owned(),
                PopUpText::plain(if team_kill { team_line } else { line }),
            ),
            ("namePopUp".to_owned(), PopUpText::name(name)),
            ("clanTagPopUp".to_owned(), PopUpText::name("")),
        ];
        if let Some((icon, short)) = rank_row(world, rank, prestige) {
            locals.push(("rankIconPopUp".to_owned(), PopUpText::name(icon)));
            locals.push(("rankPopUp".to_owned(), PopUpText::plain(short)));
        }
        Some(locals)
    };
    for (viewer, about, line, team_line) in [
        (attacker, victim, "CGAME_KILLED", "CGAME_TEAMKILLED"),
        (victim, attacker, "CGAME_KILLEDBY", "CGAME_TEAMKILLEDBY"),
    ] {
        let Some(locals) = card(world, about, line, team_line) else {
            continue;
        };
        super::player::send_menu_command(
            world,
            viewer.0,
            MenuCommandKind::PopUp(MenuPopUp {
                menu: "playerCard".to_owned(),
                item: "playerCardPopUp".to_owned(),
                locals,
                hide_others: false,
                sound: String::new(),
            }),
        );
        super::player::send_menu_command(
            world,
            viewer.0,
            MenuCommandKind::Animate {
                menu: "playerKilled".to_owned(),
                item: "playerCardPopUp".to_owned(),
                state: "Start".to_owned(),
                ms: 0,
            },
        );
    }
}

/// `clearCenterPopups()`.
fn clear_center_popups(
    world: &mut World,
    receiver: &Value,
    _args: &[Value],
) -> Result<Value, String> {
    let client = super::player::player(world, receiver)?;
    super::player::send_menu_command(world, client, MenuCommandKind::HidePopUps);
    Ok(Value::Undefined)
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Method, "displaymedal", display_medal);
    registry.register(Method, "displayrankup", display_rank_up);
    registry.register(Method, "displaywagerpopup", display_wager_popup);
    registry.register(Method, "displaykillstreak", display_killstreak);
    registry.register(Method, "displaygamemodemessage", display_game_mode_message);
    registry.register(Method, "clearcenterpopups", clear_center_popups);
    registry.register(
        Method,
        "displaychallengecomplete",
        |world, receiver, args| {
            not_shown(
                world,
                receiver,
                "displaychallengecomplete",
                "t5.mp.challenge_popup",
                args,
            )
        },
    );
    registry.register(Method, "displaycontract", |world, receiver, args| {
        not_shown(
            world,
            receiver,
            "displaycontract",
            "t5.mp.contract_popup",
            args,
        )
    });
    registry.register(Method, "displayteammessage", display_team_message);
}
