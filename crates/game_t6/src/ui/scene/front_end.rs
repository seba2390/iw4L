//! Black Ops 2's front end: what the engine's LUI init runs outside a match.

use super::{Host, Scene, key, text};
use crate::ui::Axis;
use crate::ui::dvars::{DVARS, Kind};
use crate::ui::script::tables::Tables;
use crate::ui::script::{Environment, FaultKind, Value};
use game_api::ui::{Input, Module, UiEvent, UiRequest};
use std::collections::BTreeMap;

/// The modules the engine requires outside a match, in order.
const FRONT_END_MODULES: [&str; 3] = ["T6.CoDRoot", "T6.main", "T6.Zombie.ffotdzm"];

/// The root the front end opens its first menu on.
const FRONT_END_ROOT: &str = "UIRootFull";

/// The menu the front end opens first.
const FRONT_END_MENU: &str = "main";

/// The menu `UI_SetActiveMenu(UIMENU_MAIN)` opens.
const MAIN_MENU: &str = "MainMenu";

/// The controller of the primary local client on PC.
const PRIMARY_CONTROLLER: f32 = 0.0;

/// `Engine` functions the front end calls beyond the match HUD's.
const ENGINE: &[&str] = &[];

/// `UIElement` methods the front end calls beyond the match HUD's.
const METHODS: &[&str] = &["setupGlobe", "setupVoipImage"];

/// `UIExpression` functions the front end calls.
const EXPRESSIONS: &[&str] = &[
    "GetMaxControllerCount",
    "IsInGame",
    "IsSplitscreen",
    "DvarString",
    "DvarInt",
    "DvarBool",
    "DvarFloat",
];

/// The gamer profile settings kept in the common profile layout; the rest
/// are in the game's.
const COMMON_PROFILE_SETTINGS: usize = 0x34;

/// A DDL string member.
const DDL_STRING: u32 = 7;

/// The zombies gametypes table (gametypes, start locations).
const GAMETYPES_TABLE: &str = "zm/gametypesTable.csv";

/// The zombies maps table.
const MAPS_TABLE: &str = "zm/mapsTable.csv";

/// `Com_GameMode`'s theater bit.
const GAMEMODE_THEATER: u32 = 5;

/// `Com_SessionMode`'s zombies bit.
const SESSION_ZOMBIES: u32 = 4;

/// A mode argument's bit: its number, 0 when it is not one.
fn mode_bit(value: Option<&Value>) -> u32 {
    match value {
        Some(Value::Number(mode)) => (*mode as i32).clamp(0, 31) as u32,
        _ => 0,
    }
}

/// C's `atoi`: leading blanks, a sign, then digits; 0 when none.
fn atoi(text: &str) -> i32 {
    let text = text.trim_start();
    let (sign, digits) = match text.as_bytes().first() {
        Some(b'-') => (-1, &text[1..]),
        Some(b'+') => (1, &text[1..]),
        _ => (1, text),
    };
    let digits: String = digits.chars().take_while(char::is_ascii_digit).collect();
    sign * digits.parse::<i32>().unwrap_or(0)
}

/// The engine state the front end's functions read.
#[derive(Debug, Default)]
pub(super) struct FrontEnd {
    /// The scene runs outside a match.
    pub(super) active: bool,
    /// Whether the in-game pause menu is open.
    pub(super) ingame_menu_open: bool,
    /// The dvars the scripts read and write, by name without case: their
    /// type and value as text.
    pub(super) dvars: BTreeMap<String, (Kind, String)>,
    /// The material the engine draws the mouse cursor with
    /// (`Engine.SetMouseCursor`); the cursor itself is not drawn
    /// (`t6.ui.render`).
    cursor: Option<Value>,
    /// The local client's flags the client-usage commands set: in use,
    /// primary, active.
    pub(super) client_used: bool,
    pub(super) client_primary: bool,
    client_active: bool,
    /// Console text the scripts queued (`Engine.Exec`), run before the next
    /// frame as `Cbuf_Execute` does.
    queued: Vec<String>,
    /// The local gamer profile's settings, by name without case, as text:
    /// a new profile's (`crate::ui::profile`).
    profile: BTreeMap<String, String>,
    /// The private party: IW4L's local session, the local player alone and
    /// its host once `xstartprivateparty` ran (`t6.ui.party`).
    pub(super) party: bool,
    /// The gametype `Engine.SetGametype` chose, and its settings
    /// (`zm/gamesettings_default.cfg`, then the gametype's file).
    pub(super) gametype: String,
    pub(super) gametype_settings: Vec<(String, String)>,
    /// The UI state the host shows its parties (`PartyHostSetUIState`).
    host_ui_state: i32,
    /// The active party's player limit (`Engine.PartySetMaxPlayerCount`).
    party_max_players: i32,
    /// The game lobby, hosted by the local player once one runs.
    pub(super) game_lobby: bool,
    /// The game lobby is ready to start its match.
    game_lobby_ready: bool,
    /// Whether the game lobby needs a `gamelobby_update` event on the next frame.
    pending_lobby_update: bool,
    /// The selected solo playlist ID (`Engine.SetPlaylistID`).
    playlist_id: i32,
    /// The millisecond timestamp when the match countdown finishes.
    pub(super) countdown_end: Option<i32>,
    pub(super) session: u32,
    /// `Com_GameMode`'s bits (`CoD.GAMEMODE_*`).
    game_modes: u32,
    /// The player's stats and the DDL states the scripts hold.
    pub(super) stats: super::stats::Stats,
    /// The DDL objects' metatables, by shape.
    pub(super) ddl_metatables: BTreeMap<String, Value>,
    /// The front end's root, for the events the engine sends it.
    pub(super) root: Option<Value>,
    /// Events the engine sent from inside a script's call, run when the
    /// call returns.
    pub(super) callbacks: std::collections::VecDeque<super::Callback>,
    /// What the scripts asked of the engine since the host last took them.
    pub(super) requests: Vec<UiRequest>,
}

impl Host {
    /// A binding the scripts can call, by the name `call` dispatches on.
    pub(super) fn binding(&mut self, name: String) -> Value {
        let identity = self.functions.len() as u32 + 100;
        self.functions.push(name);
        Value::Reference { kind: 10, identity }
    }

    pub(super) fn install_front_end(&mut self, tables: &mut Tables) -> Result<(), FaultKind> {
        // The zombies executable starts in a zombies session and no game
        // mode.
        self.front_end.session = 1 << SESSION_ZOMBIES;
        self.front_end.profile = crate::ui::profile::SETTINGS
            .iter()
            .map(|name| (name.to_ascii_lowercase(), String::new()))
            .chain(
                crate::ui::profile::DEFAULTS
                    .iter()
                    .map(|(name, value)| (name.to_ascii_lowercase(), (*value).to_owned())),
            )
            .collect();
        self.front_end.dvars = DVARS
            .iter()
            .map(|(name, kind, value)| (name.to_ascii_lowercase(), (*kind, (*value).to_owned())))
            .collect();
        // `Dvar.<name>` finds the dvar by name (`LUI_CoD_LuaCall_DvarIndex`)
        // and keeps its object in `Dvar`.
        let globals = tables.globals();
        let dvar = tables.allocate()?;
        let metatable = tables.allocate()?;
        let index = self.binding("DvarIndex".into());
        tables.raw_set(&metatable, &key("__index"), index)?;
        tables.set_metatable(&dvar, Some(metatable))?;
        tables.raw_set(&globals, &key("Dvar"), dvar)?;
        let engine = tables.global(&key("Engine"))?;
        for name in ENGINE {
            self.bind(tables, &engine, name)?;
        }
        let container = tables.global(&key("UIContainer"))?;
        let lui = tables.global(&key("LUI"))?;
        let class = tables.field(&lui, &key("UIElement"))?;
        for name in METHODS {
            self.bind(tables, &container, name)?;
            if matches!(
                tables.raw_get(&class, &key(name))?,
                Value::Nil | Value::Reference { kind: 10, .. }
            ) {
                let method = tables.raw_get(&container, &key(name))?;
                tables.raw_set(&class, &key(name), method)?;
            }
        }
        let expression = tables.global(&key("UIExpression"))?;
        for name in EXPRESSIONS {
            let function = self.binding(format!("uiexpr:{name}"));
            tables.raw_set(&expression, &key(name), function)?;
        }
        Ok(())
    }

    /// `Dvar.<name>`: the dvar's object with `get`, `set`, `getString` and
    /// its name, kept in `Dvar`; a name no dvar has is a script error.
    fn dvar_index(&mut self, args: &[Value], tables: &mut Tables) -> Result<Vec<Value>, FaultKind> {
        let table = args.first().ok_or(FaultKind::Argument(1))?;
        let name = text(args.get(1).ok_or(FaultKind::Argument(2))?)?;
        if !self
            .front_end
            .dvars
            .contains_key(&name.to_ascii_lowercase())
        {
            return Err(FaultKind::ScriptError(
                format!("Dvar {name} doesn't exist").into(),
            ));
        }
        let object = tables.allocate()?;
        for method in ["get", "set", "getString"] {
            let function = self.binding(format!("dvarobj:{method}"));
            tables.raw_set(&object, &key(method), function)?;
        }
        tables.raw_set(&object, &key("dvar_name"), key(&name))?;
        tables.raw_set(table, &key(&name), object.clone())?;
        Ok(vec![object])
    }

    /// The dvar a dvar object names.
    fn dvar_of(&self, object: Option<&Value>, tables: &mut Tables) -> Result<String, FaultKind> {
        let object = object.ok_or(FaultKind::Argument(1))?;
        Ok(text(&tables.raw_get(object, &key("dvar_name"))?)?.to_ascii_lowercase())
    }

    /// A dvar's value as `get` returns it, by its type: a bool, a number
    /// (an enum's index), one number per component, `rrggbbaa` as four
    /// numbers in 0..1, or text.
    fn dvar_value(&self, name: &str) -> Result<Vec<Value>, FaultKind> {
        let (kind, value) =
            self.front_end.dvars.get(name).ok_or_else(|| {
                FaultKind::ScriptError(format!("Dvar {name} doesn't exist").into())
            })?;
        let numbers = || {
            value
                .split_whitespace()
                .map(|part| Value::Number(part.parse().unwrap_or(0.0)))
                .collect::<Vec<_>>()
        };
        Ok(match kind {
            Kind::Bool => vec![Value::Boolean((value != "0").into())],
            Kind::Float
            | Kind::Int
            | Kind::Enum(_)
            | Kind::Vec2
            | Kind::Vec3
            | Kind::Vec4
            | Kind::LinearColorRgb
            | Kind::ColorXyz => numbers(),
            Kind::Color => (0..4)
                .map(|channel| {
                    let byte = value
                        .get(channel * 2..channel * 2 + 2)
                        .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                        .unwrap_or(0);
                    Value::Number(byte as f32 / 255.0)
                })
                .collect(),
            Kind::String | Kind::Int64 => vec![key(value)],
        })
    }

    /// `Dvar_DisplayableValue`: the text a dvar shows. An enum shows its
    /// domain's string for its index.
    fn dvar_display(&self, name: &str) -> Result<Option<String>, FaultKind> {
        let Some((kind, value)) = self.front_end.dvars.get(name) else {
            return Ok(None);
        };
        Ok(Some(match kind {
            Kind::Enum(domain) => {
                let index = value.parse::<usize>().unwrap_or(0);
                domain
                    .get(index)
                    .copied()
                    .unwrap_or(value.as_str())
                    .to_owned()
            }
            _ => value.clone(),
        }))
    }

    /// The `UIExpression` dvar operators (`DvarString`, `DvarInt`,
    /// `DvarBool`, `DvarFloat`): a name no dvar has reads as `""` or 0;
    /// `DvarInt` and `DvarBool` read a bool as 0 or 1, a float truncated, an
    /// int (and for `DvarInt` an enum) as it is, and anything else by `atoi`
    /// of its text; `DvarFloat` reads a bool, float or int as a float and
    /// anything else by `atof` of its text.
    fn expression_dvar(&self, operator: &str, args: &[Value]) -> Result<Vec<Value>, FaultKind> {
        let name = text(args.last().ok_or(FaultKind::Argument(2))?)?.to_ascii_lowercase();
        if operator == "DvarString" {
            return Ok(vec![key(&self
                .dvar_display(&name)?
                .unwrap_or_else(String::new))]);
        }
        let Some((kind, value)) = self.front_end.dvars.get(&name) else {
            return Ok(vec![Value::Number(0.0)]);
        };
        if operator == "DvarFloat" {
            let number = match kind {
                Kind::Bool => f32::from(u8::from(value != "0")),
                Kind::Float | Kind::Int => value.parse::<f32>().unwrap_or(0.0),
                _ => self
                    .dvar_display(&name)?
                    .unwrap_or_else(String::new)
                    .trim()
                    .parse::<f32>()
                    .unwrap_or(0.0),
            };
            return Ok(vec![Value::Number(number)]);
        }
        let number = match kind {
            Kind::Bool => i32::from(value != "0"),
            Kind::Float => value.parse::<f32>().unwrap_or(0.0) as i32,
            Kind::Int => value.parse().unwrap_or(0),
            Kind::Enum(_) if operator == "DvarInt" => value.parse().unwrap_or(0),
            _ => atoi(&self.dvar_display(&name)?.unwrap_or_else(String::new)),
        };
        Ok(vec![Value::Number(number as f32)])
    }

    /// A new profile's settings without a default are their DDL member's
    /// zero: the first 52 in the common profile layout, the rest in the
    /// game's (`ddl_mp/profile_mp.ddl`); only a string's is empty.
    pub(super) fn zero_profile_settings(&mut self) {
        for (index, name) in crate::ui::profile::SETTINGS.iter().enumerate() {
            let layout = if index < COMMON_PROFILE_SETTINGS {
                "ddl/profile_common.ddl"
            } else {
                "ddl_mp/profile_mp.ddl"
            };
            let kind = self
                .input
                .layouts
                .iter()
                .find(|candidate| candidate.name.eq_ignore_ascii_case(layout))
                .and_then(|layout| layout.structs.first())
                .and_then(|root| {
                    root.members
                        .iter()
                        .find(|member| member.name.eq_ignore_ascii_case(name))
                })
                .map(|member| member.kind);
            if let Some(value) = self.front_end.profile.get_mut(&name.to_ascii_lowercase())
                && value.is_empty()
                && !crate::ui::profile::DEFAULTS
                    .iter()
                    .any(|(set, _)| set.eq_ignore_ascii_case(name))
                && kind.is_some_and(|kind| kind != DDL_STRING)
            {
                *value = "0".into();
            }
        }
    }

    /// A front-end dvar's value as an int (`Dvar_GetInt`).
    fn dvar_int(&self, name: &str) -> i32 {
        self.front_end
            .dvars
            .get(&name.to_ascii_lowercase())
            .map_or(0, |(_, value)| {
                value
                    .parse::<f32>()
                    .map_or_else(|_| atoi(value), |v| v as i32)
            })
    }

    /// A gamer profile setting as an int.
    fn profile_int(&self, name: &str) -> i32 {
        self.front_end
            .profile
            .get(&name.to_ascii_lowercase())
            .map_or(0, |value| {
                value
                    .parse::<f32>()
                    .map_or_else(|_| atoi(value), |v| v as i32)
            })
    }

    /// Sets a dvar from a script value: a number in its type's text, a
    /// bool as `1` or `0`, text as it is (or an enum's index when the text
    /// names one of its domain strings).
    pub(super) fn set_dvar(&mut self, name: &str, value: &Value) -> Result<(), FaultKind> {
        let (kind, stored) =
            self.front_end.dvars.get_mut(name).ok_or_else(|| {
                FaultKind::ScriptError(format!("Dvar {name} doesn't exist").into())
            })?;
        *stored = match (value, *kind) {
            (Value::Number(number), Kind::Int | Kind::Enum(_)) => (*number as i32).to_string(),
            (Value::Number(number), Kind::Bool) => u8::from(*number != 0.0).to_string(),
            (Value::Boolean(flag), _) => u8::from(*flag != 0).to_string(),
            (value, Kind::Enum(domain)) => {
                let text = text(value)?;
                domain
                    .iter()
                    .position(|entry| entry.eq_ignore_ascii_case(&text))
                    .map_or_else(|| atoi(&text).to_string(), |index| index.to_string())
            }
            (value, _) => text(value)?,
        };
        Ok(())
    }

    /// An event the engine sends the root from inside a call
    /// (`LUI_BeginEvent` ... `LUI_EndEvent`): `root:processEvent(event)`.
    fn post_event(
        &mut self,
        tables: &mut Tables,
        name: &str,
        fields: &[(&str, Value)],
    ) -> Result<(), FaultKind> {
        let root = self
            .front_end
            .root
            .clone()
            .ok_or(FaultKind::BindingRequired)?;
        let event = tables.allocate()?;
        tables.raw_set(&event, &key("name"), key(name))?;
        for (field, value) in fields {
            tables.raw_set(&event, &key(field), value.clone())?;
        }
        let callee = tables.field(&root, &key("processEvent"))?;
        self.front_end.callbacks.push_back(super::Callback {
            callee,
            arguments: vec![root, event],
        });
        Ok(())
    }

    /// A localized string by its key; the key itself when the zones have
    /// none (`SEH_LocalizeTextMessage`'s fallback).
    fn localized(&self, name: &str) -> String {
        self.input
            .localized
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map_or(name, |(_, value)| value.as_str())
            .to_owned()
    }

    /// `StringTable_Lookup`: the first row of a table whose `column` is
    /// `value` (without case), its `result` cell; empty when none.
    fn table_lookup(&self, table: &str, column: usize, value: &str, result: usize) -> &str {
        self.input
            .tables
            .iter()
            .find(|candidate| candidate.name.eq_ignore_ascii_case(table))
            .filter(|table| table.columns > column.max(result))
            .and_then(|table| {
                table
                    .cells
                    .chunks_exact(table.columns)
                    .find(|row| row[column].eq_ignore_ascii_case(value))
                    .map(|row| row[result].as_str())
            })
            .unwrap_or("")
    }

    /// The first row of a table whose cells match every `(column, value)`
    /// without case.
    fn table_row(&self, table: &str, keys: &[(usize, &str)]) -> Option<Vec<String>> {
        let table = self
            .input
            .tables
            .iter()
            .find(|candidate| candidate.name.eq_ignore_ascii_case(table))?;
        table
            .cells
            .chunks_exact(table.columns)
            .find(|row| {
                keys.iter().all(|(column, value)| {
                    row.get(*column)
                        .is_some_and(|cell| cell.eq_ignore_ascii_case(value))
                })
            })
            .map(<[String]>::to_vec)
    }

    /// The maps table the engine loads (`zm/mapsTable.csv`): per map, its
    /// zone, name key and map pack, in table order.
    fn maps_table(&self) -> Vec<(String, String, i32)> {
        let count = atoi(self.table_lookup(MAPS_TABLE, 0, "maxnum_map", 1));
        (0..count.max(0))
            .map(|index| {
                let index = index.to_string();
                (
                    self.table_lookup(MAPS_TABLE, 5, &index, 0).to_owned(),
                    self.table_lookup(MAPS_TABLE, 5, &index, 3).to_owned(),
                    atoi(self.table_lookup(MAPS_TABLE, 5, &index, 11)),
                )
            })
            .collect()
    }

    /// Whether a map pack's content is there: IW4L counts a pack as there
    /// when the zone of a map in it is installed.
    fn map_pack_available(&self, pack: i32) -> bool {
        self.maps_table().iter().any(|(map, _, map_pack)| {
            *map_pack == pack
                && self
                    .input
                    .installed_maps
                    .iter()
                    .any(|zone| zone.eq_ignore_ascii_case(map))
        })
    }

    /// Solo playlists synthesized from `zm/mapsTable.csv` and
    /// `zm/gametypesTable.csv` for installed maps:
    /// `(playlist_id, super_cat, load_name, map_name, loc_ref, loc_name, gametype, gametype_name, gametype_icon, group)`.
    #[allow(clippy::type_complexity)]
    fn solo_playlists(
        &self,
    ) -> Vec<(
        i32,
        i32,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    )> {
        let mut out = Vec::new();
        let loc_count = atoi(self.table_lookup(GAMETYPES_TABLE, 0, "maxnum_startloc", 1));
        let mode_rows = atoi(self.table_lookup(GAMETYPES_TABLE, 0, "startloc_gamemode_map", 1));
        for (map_idx, (load_name, map_name, pack)) in self.maps_table().into_iter().enumerate() {
            if pack != 0 && !self.map_pack_available(pack) {
                continue;
            }
            if load_name.contains("_dr") {
                continue;
            }
            let super_cat = map_idx as i32 + 1;
            let mut loc_seq = 0;
            for loc_i in 0..loc_count.max(0) {
                let Some(loc_row) =
                    self.table_row(GAMETYPES_TABLE, &[(0, "5"), (1, &loc_i.to_string())])
                else {
                    continue;
                };
                if !loc_row
                    .get(2)
                    .is_some_and(|cell| cell.eq_ignore_ascii_case(&load_name))
                {
                    continue;
                }
                let loc_ref = loc_row.get(3).cloned().unwrap_or_else(String::new);
                let loc_name = loc_row.get(4).cloned().unwrap_or_else(String::new);
                let limit = atoi(loc_row.get(7).map_or("", String::as_str));
                loc_seq += 1;
                let mut found = 0;
                let mut mode_seq = 0;
                for row_i in 0..mode_rows.max(0) {
                    if found >= limit {
                        break;
                    }
                    let Some(row) = self.table_row(
                        GAMETYPES_TABLE,
                        &[
                            (0, "6"),
                            (1, &row_i.to_string()),
                            (2, &load_name),
                            (3, &loc_ref),
                        ],
                    ) else {
                        continue;
                    };
                    found += 1;
                    let gametype = row.get(4).cloned().unwrap_or_else(String::new);
                    if gametype != "zclassic" && gametype != "zstandard" {
                        continue;
                    }
                    mode_seq += 1;
                    let gt_row = self.table_row(GAMETYPES_TABLE, &[(0, "0"), (1, &gametype)]);
                    let gametype_name = gt_row
                        .as_ref()
                        .and_then(|r| r.get(2).cloned())
                        .unwrap_or_else(String::new);
                    let gametype_icon = gt_row
                        .as_ref()
                        .and_then(|r| r.get(4).cloned())
                        .unwrap_or_else(String::new);
                    let group = gt_row
                        .as_ref()
                        .and_then(|r| r.get(9).cloned())
                        .unwrap_or_else(|| "zclassic".into());
                    let playlist_id = super_cat * 100 + loc_seq * 10 + mode_seq;
                    out.push((
                        playlist_id,
                        super_cat,
                        load_name.clone(),
                        map_name.clone(),
                        loc_ref.clone(),
                        loc_name.clone(),
                        gametype,
                        gametype_name,
                        gametype_icon,
                        group,
                    ));
                }
            }
        }
        out
    }

    /// Runs console text: the commands whose state the menus read back run
    /// here; the rest go to the host as console text.
    pub(super) fn execute(&mut self, text: &str) {
        for raw_line in text.lines() {
            let uncommented = raw_line.split("//").next().unwrap_or("");
            for line in uncommented.split(';') {
                let line = line.trim();
                let Some(command) = line.split_whitespace().next() else {
                    continue;
                };
                if command.starts_with('/') {
                    continue;
                }
                let lower = command.to_ascii_lowercase();
                match lower.as_str() {
                    // The invite a boot left to accept; IW4L starts with none.
                    "checkforinvites"
                    | "xsessionupdate"
                    | "session_rejoinsession"
                    | "freedemomemory"
                    | "party_statechanged"
                    | "party_updateactivemenu"
                    | "restoredvars"
                    | "xpartyveto"
                    | "loadcommonff"
                    | "xupdatepartystate"
                    | "ui_keyboard_cancel" => {}
                    "updategamerprofile" => {
                        if self.front_end.game_lobby {
                            self.front_end.pending_lobby_update = true;
                        }
                    }
                    "reset" => {
                        if let Some(name) = line.split_whitespace().nth(1)
                            && let Some(&(_, kind, default)) = DVARS
                                .iter()
                                .find(|&&(dvar, _, _)| dvar.eq_ignore_ascii_case(name))
                        {
                            self.front_end
                                .dvars
                                .insert(name.to_ascii_lowercase(), (kind, default.to_owned()));
                        }
                    }
                    "exec" => {
                        if let Some(file) = line.split_whitespace().nth(1)
                            && let Some(cfg) = self.configs.get(&file.to_ascii_lowercase()).cloned()
                        {
                            self.execute(&cfg);
                        }
                    }
                    "set" | "seta" => {
                        let mut words = line.split_whitespace().skip(1);
                        if let (Some(name), Some(value)) = (words.next(), words.next()) {
                            let key_name = name.to_ascii_lowercase();
                            let value = value.trim_matches('"');
                            // `set_dvar` takes `&mut self`: no entry can be held.
                            #[allow(clippy::map_entry)]
                            if self.front_end.dvars.contains_key(&key_name) {
                                let _ = self.set_dvar(&key_name, &key(value));
                            } else {
                                self.front_end
                                    .dvars
                                    .insert(key_name, (Kind::String, value.to_owned()));
                            }
                        }
                    }
                    "fast_restart" => {
                        self.front_end.ingame_menu_open = false;
                        self.front_end
                            .requests
                            .push(UiRequest::Exec("map_restart".into()));
                    }
                    "xpartygo" => {
                        if self.front_end.game_lobby {
                            self.front_end.game_lobby_ready = true;
                            self.front_end.countdown_end = Some(self.input.time);
                        }
                    }
                    "xpartyready" => {
                        if self.front_end.game_lobby && line.split_whitespace().nth(1) == Some("1")
                        {
                            self.front_end.game_lobby_ready = true;
                            let seconds = self.dvar_int("party_gameStartTimerLengthPrivate")
                                + self.dvar_int("party_pregameStartTimerLengthPrivate");
                            self.front_end.countdown_end =
                                Some(self.input.time + seconds.max(0) * 1000);
                            self.front_end.pending_lobby_update = true;
                        }
                    }
                    "xstartprivateparty" | "xstartlocalprivateparty" => self.front_end.party = true,
                    "xstartpartyhost" | "xstartparty" => {
                        self.front_end.game_lobby = true;
                        self.front_end.game_lobby_ready = false;
                        self.front_end.countdown_end = None;
                        self.front_end.pending_lobby_update = true;
                    }
                    "xstopparty" | "xstoppartykeeptogether" => {
                        self.front_end.game_lobby = false;
                        self.front_end.game_lobby_ready = false;
                        self.front_end.countdown_end = None;
                    }
                    "xstopprivateparty" => self.front_end.party = false,
                    "xstopallparties" => {
                        self.front_end.party = false;
                        self.front_end.game_lobby = false;
                        self.front_end.game_lobby_ready = false;
                        self.front_end.countdown_end = None;
                    }
                    "resetcustomgametype" => {
                        let configs = &self.configs;
                        self.front_end.gametype_settings = crate::gametype_settings(
                            &|name: &str| {
                                configs.get(&name.to_ascii_lowercase()).map(String::as_str)
                            },
                            &self.front_end.gametype,
                        );
                    }
                    "gametype_setting" => {
                        let mut words = line.split_whitespace().skip(1);
                        if let (Some(name), Some(value)) = (words.next(), words.next()) {
                            let name = name.to_ascii_lowercase();
                            match self
                                .front_end
                                .gametype_settings
                                .iter_mut()
                                .find(|(set, _)| *set == name)
                            {
                                Some((_, set)) => value.clone_into(set),
                                None => self
                                    .front_end
                                    .gametype_settings
                                    .push((name, value.to_owned())),
                            }
                        }
                    }
                    // Client 0's flags (the only local client on PC).
                    "disableallclients" | "setclientnotbeingused" => {
                        self.front_end.client_active = false;
                        self.front_end.client_used = false;
                    }
                    "disableallbutprimaryclients" if !self.front_end.client_primary => {
                        self.front_end.client_active = false;
                        self.front_end.client_used = false;
                    }
                    "disableallbutprimaryclients" => {}
                    "setclientbeingusedandprimary" => {
                        self.front_end.client_used = true;
                        self.front_end.client_primary = true;
                    }
                    "setclientbeingusedandprimaryandactive" => {
                        self.front_end.client_used = true;
                        self.front_end.client_primary = true;
                        self.front_end.client_active = true;
                    }
                    "setclientbeingused" => self.front_end.client_used = true,
                    "setclientbeingusedandactive" => {
                        self.front_end.client_used = true;
                        self.front_end.client_active = true;
                    }
                    "setclientprimary" => self.front_end.client_primary = true,
                    _ if self.front_end.dvars.contains_key(&lower) => {
                        if let Some(value) = line.split_whitespace().nth(1) {
                            let _ = self.set_dvar(&lower, &key(value.trim_matches('"')));
                        }
                    }
                    _ => self
                        .front_end
                        .requests
                        .push(UiRequest::Exec(line.to_owned())),
                }
            }
        }
    }

    /// Runs the console text queued since the last frame.
    pub(super) fn run_queued(&mut self) {
        for text in std::mem::take(&mut self.front_end.queued) {
            self.execute(&text);
        }
    }

    /// The front end's engine functions, and the ones whose answer depends
    /// on whether a match runs; `None` for the rest.
    pub(super) fn front_end_call(
        &mut self,
        name: &str,
        args: &[Value],
        tables: &mut Tables,
    ) -> Option<Result<Vec<Value>, FaultKind>> {
        if let Some(method) = name.strip_prefix("ddl:") {
            return Some(self.ddl_call(method, args, tables));
        }
        match name {
            "missing:Engine.GetPlayerStats" => {
                return Some(self.get_player_stats(args, tables));
            }
            "DvarIndex" => return Some(self.dvar_index(args, tables)),
            "dvarobj:get" => {
                return Some(
                    self.dvar_of(args.first(), tables)
                        .and_then(|name| self.dvar_value(&name)),
                );
            }
            "dvarobj:getString" => {
                return Some(
                    self.dvar_of(args.first(), tables)
                        .map(|name| vec![key(&self.front_end.dvars[&name].1)]),
                );
            }
            "dvarobj:set" => {
                return Some(self.dvar_of(args.first(), tables).and_then(|name| {
                    self.set_dvar(&name, args.get(1).unwrap_or(&Value::Nil))?;
                    Ok(Vec::new())
                }));
            }
            "uiexpr:IsInGame" | "missing:UIExpression.IsInGame" => {
                return Some(Ok(vec![Value::Number(f32::from(u8::from(
                    !self.front_end.active,
                )))]));
            }
            "uiexpr:IsSplitscreen" | "missing:UIExpression.IsSplitscreen" => {
                return Some(Ok(vec![Value::Number(0.0)]));
            }
            _ => {}
        }
        if let Some(operator @ ("DvarString" | "DvarInt" | "DvarBool" | "DvarFloat")) = name
            .strip_prefix("uiexpr:")
            .or_else(|| name.strip_prefix("missing:UIExpression."))
            .or(Some(name))
        {
            return Some(self.expression_dvar(operator, args));
        }
        let name = name
            .strip_prefix("uiexpr:")
            .or_else(|| name.strip_prefix("missing:UIExpression."))
            .or_else(|| name.strip_prefix("missing:Engine."))
            .or_else(|| name.strip_prefix("missing:UIElement."))
            .unwrap_or(name);
        Some(Ok(match name {
            // `Dvar_SetFromStringByName`: sets the dvar (or creates a string
            // dvar) from the argument's text; nothing when the name is not
            // text or a number.
            "SetDvar" => {
                if let Some(Ok(name)) = args.first().map(text)
                    && !name.is_empty()
                {
                    let key = name.to_ascii_lowercase();
                    let value = args
                        .get(1)
                        .and_then(|v| text(v).ok())
                        .unwrap_or_else(String::new);
                    if key == "cl_paused" && (value == "0" || value == "0.0") {
                        self.front_end.ingame_menu_open = false;
                    }
                    let value = super::key(&value);
                    // `set_dvar` takes `&mut self`: no entry can be held.
                    #[allow(clippy::map_entry)]
                    if self.front_end.dvars.contains_key(&key) {
                        if let Err(fault) = self.set_dvar(&key, &value) {
                            return Some(Err(fault));
                        }
                    } else if let Ok(value) = text(&value) {
                        self.front_end.dvars.insert(key, (Kind::String, value));
                    }
                }
                Vec::new()
            }
            // One controller on PC.
            "GetMaxControllerCount" | "SplitscreenNum" | "GetActiveLocalClientsCount" => {
                vec![Value::Number(1.0)]
            }
            // Whether the client is in a match.
            "IsInGame" => vec![Value::Boolean((!self.front_end.active).into())],
            "CanPauseZombiesGame" => vec![Value::Boolean(1)],
            "ProbationCheckIfPenalizedForQuit" => vec![Value::Boolean(0)],
            "UpdateStatsForQuit" => Vec::new(),
            "SendMenuResponse" => {
                if args.get(1).and_then(|v| text(v).ok()).as_deref() == Some("popup_leavegame")
                    && args.get(2).and_then(|v| text(v).ok()).as_deref() == Some("endround")
                {
                    self.front_end.ingame_menu_open = false;
                    self.front_end
                        .requests
                        .push(UiRequest::Exec("disconnect".into()));
                }
                Vec::new()
            }
            "SetActiveMenu" => {
                let menu = super::number(args.get(1)).map_or(0, |m| m as i32);
                self.front_end.ingame_menu_open = menu == 3;
                Vec::new()
            }
            "GametypeName" => {
                let gametype = self
                    .front_end
                    .dvars
                    .get("ui_gametype")
                    .map_or("zclassic", |(_, v)| v.as_str())
                    .to_owned();
                let key_name = self
                    .table_row(GAMETYPES_TABLE, &[(0, "0"), (1, &gametype)])
                    .and_then(|row| row.get(2).cloned())
                    .unwrap_or(gametype);
                vec![key(&self.localized(&key_name))]
            }
            "GametypeDescription" => vec![key("")],
            // The sound thread's music command, by the alias's name; a
            // name that is not a string stops the music.
            "PlayMenuMusic" => {
                let alias = match args.first() {
                    Some(Value::Bytes(name)) if !name.is_empty() => {
                        Some(String::from_utf8_lossy(name).into_owned())
                    }
                    _ => None,
                };
                self.front_end.requests.push(UiRequest::MenuMusic(alias));
                Vec::new()
            }
            // `Cbuf_AddText` for the controller (or the active one) with the
            // text and a newline, run before the next frame; `ExecNow` runs
            // it at once.
            "Exec" | "ExecNow" => {
                let command = match args.get(1) {
                    Some(Value::Bytes(text)) => String::from_utf8_lossy(text).into_owned(),
                    _ => String::new(),
                };
                if name == "Exec" {
                    self.front_end.queued.push(command);
                } else {
                    self.execute(&command);
                }
                Vec::new()
            }
            "SetMouseCursor" => {
                if let Some(material @ Value::Reference { kind: 2, .. }) = args.first() {
                    self.front_end.cursor = Some(material.clone());
                }
                Vec::new()
            }
            // IW4L's local profile is the signed-in online user, connected
            // to an online service that has fetched its data, bans nobody
            // and has no message of the day (`t6.ui.live`).
            "AnySignedInToLive" | "IsSignedInToLive" | "IsSignedIn" | "IsDemonwareFetchingDone" => {
                vec![Value::Number(1.0)]
            }
            "IsFeatureBanned" | "IsVacBanned" | "ShouldShowMOTD" | "IsLivestreamEnabled" => {
                vec![Value::Boolean(0)]
            }
            "CheckNetConnection" | "CanViewContent" | "PartyShowTruePlayerInfo" => {
                vec![Value::Boolean(1)]
            }
            // Constant on PC.
            "IsContentRatingAllowed" => vec![Value::Number(1.0)],
            "IsAnyControllerMPRestricted" => vec![Value::Number(0.0)],
            // `Com_SessionMode_SetMode` for one mode; reset clears offline,
            // system link, online and private.
            "SessionModeSetOffline"
            | "SessionModeSetSystemLink"
            | "SessionModeSetOnlineGame"
            | "SessionModeSetPrivate" => {
                let bit = match name {
                    "SessionModeSetOffline" => 0,
                    "SessionModeSetSystemLink" => 1,
                    "SessionModeSetOnlineGame" => 2,
                    _ => 3,
                };
                if args.first().is_some_and(Value::truthy) {
                    self.front_end.session |= 1 << bit;
                } else {
                    self.front_end.session &= !(1 << bit);
                }
                Vec::new()
            }
            "SessionModeResetModes" => {
                self.front_end.session &= !0b1111;
                Vec::new()
            }
            "SessionModeIsMode" => {
                let bit = mode_bit(args.first());
                vec![Value::Boolean(self.front_end.session >> bit & 1)]
            }
            "SessionMode_IsOnlineGame"
            | "SessionMode_IsSystemlinkGame"
            | "SessionMode_IsZombiesGame" => {
                let bit = match name {
                    "SessionMode_IsOnlineGame" => 2,
                    "SessionMode_IsSystemlinkGame" => 1,
                    _ => SESSION_ZOMBIES,
                };
                vec![Value::Number((self.front_end.session >> bit & 1) as f32)]
            }
            "SessionMode_IsPublicOnlineGame" => {
                let public = self.front_end.session & 0b1100 == 0b0100;
                vec![Value::Number(f32::from(u8::from(public)))]
            }
            // `Com_GameMode`, a set of bits like the session's.
            "GameModeSetMode" => {
                let bit = mode_bit(args.first());
                if args.get(1).is_some_and(Value::truthy) {
                    self.front_end.game_modes |= 1 << bit;
                } else {
                    self.front_end.game_modes &= !(1 << bit);
                }
                Vec::new()
            }
            "GameModeResetModes" => {
                self.front_end.game_modes = 0;
                Vec::new()
            }
            "GameModeIsMode" => {
                let bit = mode_bit(args.first());
                vec![Value::Boolean(self.front_end.game_modes >> bit & 1)]
            }
            // `CL_PlayLocalSoundAliasByName` for the primary local client.
            "PlaySound" => {
                if let Some(Value::Bytes(alias)) = args.first() {
                    self.front_end.requests.push(UiRequest::Sound(
                        String::from_utf8_lossy(alias).into_owned(),
                    ));
                }
                Vec::new()
            }
            // The blur of the 3D scene behind the menus; the front end draws
            // no 3D scene (`t6.ui.render`).
            "BlurWorld" => Vec::new(),
            // Clears the engine's "event in flight" flag; nothing here reads it.
            "EventProcessed" => Vec::new(),
            // `CL_GetPrimaryController`: controller 0 on PC.
            "GetPrimaryController" => vec![Value::Number(PRIMARY_CONTROLLER)],
            // Whether the local player hosts the game lobby, and its
            // private party.
            "GameHost" => vec![Value::Number(f32::from(u8::from(
                self.front_end.game_lobby,
            )))],
            "PrivatePartyHost" => vec![Value::Number(f32::from(u8::from(self.front_end.party)))],
            // Gamer profile settings by name: an int reads 0 for a name the
            // profile has no setting for; a set takes text.
            "GetProfileVarInt" | "ProfileInt" | "ProfileBool" => {
                let name =
                    text(args.get(1).unwrap_or(&Value::Nil)).unwrap_or_else(|_| String::new());
                let value = self
                    .front_end
                    .profile
                    .get(&name.to_ascii_lowercase())
                    .map_or(0, |value| {
                        value
                            .parse::<f32>()
                            .map_or_else(|_| atoi(value), |v| v as i32)
                    });
                vec![Value::Number(value as f32)]
            }
            "ProfileValueAsString" => {
                let name =
                    text(args.get(1).unwrap_or(&Value::Nil)).unwrap_or_else(|_| String::new());
                vec![key(self
                    .front_end
                    .profile
                    .get(&name.to_ascii_lowercase())
                    .map_or("", String::as_str))]
            }
            "SetProfileVar" => {
                let name =
                    text(args.get(1).unwrap_or(&Value::Nil)).unwrap_or_else(|_| String::new());
                let value = args
                    .get(2)
                    .map(text)
                    .transpose()
                    .ok()
                    .flatten()
                    .unwrap_or_else(String::new);
                if let Some(setting) = self.front_end.profile.get_mut(&name.to_ascii_lowercase()) {
                    *setting = value;
                }
                Vec::new()
            }
            // Writes the profile to storage; IW4L keeps it for the session.
            "CommitProfileChanges"
            | "SyncHardwareProfileWithDvars"
            | "SetHardwareProfileValue"
            | "SaveHardwareProfile"
            | "ApplyHardwareProfileSettings" => Vec::new(),
            // `Sys_GetSystemInfo`: a label with the value in its `&&1`. IW4L
            // detects no NAT type, which reads as not available.
            "GetSystemInfo" => {
                let label = match args.get(1) {
                    Some(Value::Number(2.0)) => "MENU_SYSINFO_NAT_TYPE",
                    Some(Value::Number(10.0)) => "MENU_SYSINFO_NAT_TYPE_LOBBY",
                    _ => return Some(Err(FaultKind::Unknown(super::super::script::ENGINE_API))),
                };
                let value = self.localized("MENU_SYSINFO_NOT_AVAILABLE");
                vec![key(&self.localized(label).replace("&&1", &value))]
            }
            // The game lobby runs, the local player hosts it, and it is
            // ready to start.
            "PartyHostIsReadyToStart" | "PartyIsReadyToStart" => vec![Value::Boolean(
                (self.front_end.game_lobby && self.front_end.game_lobby_ready).into(),
            )],
            "PartyIsPostGame" => vec![Value::Boolean(0)],
            "PartyShowTeams" => vec![Value::Boolean(
                (self.front_end.session & 0b1100 != 0b0100 || self.front_end.game_lobby_ready)
                    .into(),
            )],
            "AloneInPartyIgnoreSplitscreen" => vec![Value::Number(1.0)],
            "IsXuidPrivatePartyHost" => {
                let xuid = args
                    .first()
                    .and_then(|v| text(v).ok())
                    .unwrap_or_else(String::new);
                vec![Value::Boolean((self.front_end.party && xuid == "0").into())]
            }
            // Client 0's usage flags.
            "GetUsedControllerCount" | "IsControllerBeingUsed" => {
                vec![Value::Number(f32::from(u8::from(
                    self.front_end.client_used,
                )))]
            }
            "GetNonUsedControllerCount" => {
                vec![Value::Number(f32::from(u8::from(
                    !self.front_end.client_used,
                )))]
            }
            "IsPrimaryLocalClient" => {
                vec![Value::Number(f32::from(u8::from(
                    self.front_end.client_primary,
                )))]
            }
            // A retail build.
            "IsBetaBuild" => vec![Value::Boolean(0)],
            // The online service's player group counts, never fetched here:
            // empty text.
            "GetPlayerGroupCount" => vec![key("")],
            "InPrivateParty" => vec![Value::Number(f32::from(u8::from(self.front_end.party)))],
            // The title needs no update: no results (a failed check would
            // raise a script error).
            "SystemNeedsUpdate" => Vec::new(),
            // The element's current shader vector 2, when one was set.
            "getShaderVector2" => {
                let element = match self
                    .elements
                    .element_mut(args.first().unwrap_or(&Value::Nil))
                {
                    Ok(element) => element,
                    Err(fault) => return Some(Err(fault)),
                };
                element.current.shader_vectors[2].map_or_else(Vec::new, |vector| {
                    vector.into_iter().map(Value::Number).collect()
                })
            }
            // No invite is being accepted (IW4L has none).
            "AcceptingInvite" => vec![Value::Number(0.0)],
            // Neither the party nor the game lobby is joining another
            // session.
            "IsJoiningAnotherParty" => vec![Value::Boolean(0)],
            "IsGameLobbyRunning" => vec![Value::Boolean(self.front_end.game_lobby.into())],
            "IsPartyLobbyRunning" => vec![Value::Boolean(self.front_end.party.into())],
            "InLobby" => vec![Value::Number(f32::from(u8::from(
                self.front_end.game_lobby,
            )))],
            "PartyGetPlayerCount" | "PartyGetMemberCount" if self.front_end.active => {
                vec![Value::Number(f32::from(u8::from(
                    self.front_end.game_lobby || self.front_end.party,
                )))]
            }
            "PartyGetTeamMemberCount" if self.front_end.active => {
                let team = super::number(args.first()).map_or(0, |team| team as i32);
                let present = (self.front_end.game_lobby || self.front_end.party) && team == 1;
                vec![Value::Number(f32::from(u8::from(present)))]
            }
            "DoesPartyHaveDLCForMap" => {
                let map = args
                    .first()
                    .and_then(|v| text(v).ok())
                    .unwrap_or_else(String::new);
                let available = self
                    .maps_table()
                    .into_iter()
                    .find(|(zone, _, _)| zone.eq_ignore_ascii_case(&map))
                    .is_none_or(|(_, _, pack)| pack == 0 || self.map_pack_available(pack));
                vec![Value::Boolean(available.into())]
            }
            "IsMapValid" => {
                let map = args
                    .first()
                    .and_then(|v| text(v).ok())
                    .unwrap_or_else(String::new);
                let valid = !map.is_empty()
                    && self.maps_table().into_iter().any(|(zone, _, pack)| {
                        zone.eq_ignore_ascii_case(&map)
                            && (pack == 0 || self.map_pack_available(pack))
                    });
                vec![Value::Boolean(valid.into())]
            }
            "GetGameLobbyStatus" => {
                if let Some(end) = self.front_end.countdown_end {
                    let seconds = ((end - self.input.time) / 1000).max(0);
                    let status = self
                        .localized("MENU_GAME_STARTING_IN")
                        .replace("&&1", &seconds.to_string());
                    vec![key(&status), Value::Number(-1.0)]
                } else {
                    vec![key(""), Value::Number(-1.0)]
                }
            }
            "PartyGetHostUIState" => {
                if self.front_end.game_lobby {
                    vec![Value::Number(self.front_end.host_ui_state as f32)]
                } else {
                    Vec::new()
                }
            }
            "PartyHostToggleStart" => {
                if self.front_end.game_lobby {
                    self.front_end.game_lobby_ready = !self.front_end.game_lobby_ready;
                    self.front_end.countdown_end = self.front_end.game_lobby_ready.then(|| {
                        let seconds = self.dvar_int("party_gameStartTimerLengthPrivate")
                            + self.dvar_int("party_pregameStartTimerLengthPrivate");
                        self.input.time + seconds.max(0) * 1000
                    });
                }
                Vec::new()
            }
            "GetPlayersInLobby" => {
                let list = match tables.allocate() {
                    Ok(list) => list,
                    Err(fault) => return Some(Err(fault)),
                };
                if self.front_end.game_lobby || self.front_end.party {
                    let entry = match tables.allocate() {
                        Ok(entry) => entry,
                        Err(fault) => return Some(Err(fault)),
                    };
                    let gamertag = self
                        .input
                        .players
                        .first()
                        .map_or("Player", |player| player.name.as_str());
                    let fields = [
                        ("clientNum", Value::Number(0.0)),
                        ("xuid", key("0")),
                        ("tagprefix", key("")),
                        ("clantag", key("")),
                        ("gamertag", key(gamertag)),
                        ("clean_gamertag", key(gamertag)),
                        ("rank", Value::Number(1.0)),
                        ("prestige", Value::Number(0.0)),
                        ("isLocal", Value::Number(1.0)),
                        ("controller", Value::Number(PRIMARY_CONTROLLER)),
                        ("isHost", Value::Number(1.0)),
                        ("isGuest", Value::Number(0.0)),
                        ("isInParty", Value::Boolean(0)),
                        ("subparty", Value::Number(-1.0)),
                        ("team", Value::Number(1.0)),
                        ("isReady", Value::Boolean(0)),
                        ("score", key("")),
                        ("leagueTeamMemberCount", Value::Number(0.0)),
                        ("mapPackFlags", Value::Number(0.0)),
                        ("missingMapPacks", Value::Number(0.0)),
                    ];
                    for (field, value) in fields {
                        if let Err(fault) = tables.raw_set(&entry, &key(field), value) {
                            return Some(Err(fault));
                        }
                    }
                    if let Err(fault) = tables.raw_set(&list, &Value::Number(1.0), entry) {
                        return Some(Err(fault));
                    }
                }
                vec![list]
            }
            // The profile's `party_maxplayers`; in theater, or in a zombies
            // session that is not a public online one, the dvar's, at most 8
            // for zombies.
            "GetMaxUserPlayerCount" => {
                let mut count = self.profile_int("party_maxplayers");
                if self.front_end.game_modes >> GAMEMODE_THEATER & 1 != 0 {
                    count = self.dvar_int("party_maxplayers");
                } else if self.front_end.session >> SESSION_ZOMBIES & 1 != 0 {
                    if self.front_end.session & 0b1100 != 0b0100 {
                        count = self.dvar_int("party_maxplayers");
                    }
                    count = count.min(8);
                }
                vec![Value::Number(count as f32)]
            }
            // The active party's player limit.
            "PartySetMaxPlayerCount" => {
                self.front_end.party_max_players = match args.first() {
                    Some(Value::Number(count)) => *count as i32,
                    _ => 0,
                };
                Vec::new()
            }
            // `Com_SetGametype`: sets `ui_gametype` and `g_gametype`, loads
            // the gametype's settings from its config files, and tells the
            // UI (`gametype_update`, not modified).
            "SetGametype" => {
                let Some(Value::Bytes(gametype)) = args.first() else {
                    return Some(Ok(Vec::new()));
                };
                let gametype = String::from_utf8_lossy(gametype).into_owned();
                for dvar in ["ui_gametype", "g_gametype"] {
                    if let Some((_, value)) = self.front_end.dvars.get_mut(dvar) {
                        value.clone_from(&gametype);
                    }
                }
                let configs = &self.configs;
                self.front_end.gametype_settings = crate::gametype_settings(
                    &|name: &str| configs.get(&name.to_ascii_lowercase()).map(String::as_str),
                    &gametype,
                );
                self.front_end.gametype = gametype.clone();
                if let Err(fault) = self.post_event(
                    tables,
                    "gametype_update",
                    &[
                        ("gametype", key(&gametype)),
                        ("isModified", Value::Boolean(0)),
                    ],
                ) {
                    return Some(Err(fault));
                }
                Vec::new()
            }
            "GetGametypeSetting" => {
                let name = args
                    .first()
                    .and_then(|v| text(v).ok())
                    .unwrap_or_else(String::new);
                let in_ddl = self
                    .input
                    .layouts
                    .iter()
                    .find(|l| l.name.eq_ignore_ascii_case("ddl_mp/gametype_settings.ddl"))
                    .and_then(|l| l.structs.first())
                    .is_none_or(|s| s.members.iter().any(|m| m.name.eq_ignore_ascii_case(&name)));
                if !in_ddl {
                    return Some(Ok(Vec::new()));
                }
                let default = args.get(1).is_some_and(Value::truthy);
                let defaults;
                let list = if default {
                    let configs = &self.configs;
                    defaults = crate::gametype_settings(
                        &|n: &str| configs.get(&n.to_ascii_lowercase()).map(String::as_str),
                        &self.front_end.gametype,
                    );
                    &defaults
                } else {
                    &self.front_end.gametype_settings
                };
                let value = list
                    .iter()
                    .find(|(set, _)| set.eq_ignore_ascii_case(&name))
                    .and_then(|(_, v)| v.parse::<f32>().ok())
                    .unwrap_or_else(|| match name.as_str() {
                        "teamCount" if self.front_end.gametype != "dm" => 2.0,
                        _ => 0.0,
                    });
                vec![Value::Number(value)]
            }
            "SetGametypeSetting" => {
                let name = args
                    .first()
                    .and_then(|v| text(v).ok())
                    .unwrap_or_else(String::new)
                    .to_ascii_lowercase();
                let value = super::number(args.get(1)).unwrap_or(0.0);
                let formatted = if value.fract() == 0.0 {
                    (value as i32).to_string()
                } else {
                    value.to_string()
                };
                match self
                    .front_end
                    .gametype_settings
                    .iter_mut()
                    .find(|(set, _)| *set == name)
                {
                    Some((_, set)) => *set = formatted,
                    None => self.front_end.gametype_settings.push((name, formatted)),
                }
                Vec::new()
            }
            "IsGametypeSettingDefault" => {
                let name = args
                    .first()
                    .and_then(|v| text(v).ok())
                    .unwrap_or_else(String::new);
                let configs = &self.configs;
                let defaults = crate::gametype_settings(
                    &|n: &str| configs.get(&n.to_ascii_lowercase()).map(String::as_str),
                    &self.front_end.gametype,
                );
                let find = |list: &[(String, String)]| {
                    list.iter()
                        .find(|(set, _)| set.eq_ignore_ascii_case(&name))
                        .and_then(|(_, v)| v.parse::<f32>().ok())
                        .unwrap_or(0.0)
                };
                vec![Value::Boolean(
                    (find(&self.front_end.gametype_settings) == find(&defaults)).into(),
                )]
            }
            // The UI state the host shows the parties it runs and hosts.
            "PartyHostSetUIState" | "PartyHostClearUIState" => {
                if self.front_end.party || self.front_end.game_lobby {
                    self.front_end.host_ui_state = match args.first() {
                        Some(Value::Number(state)) => *state as i32,
                        _ => 0,
                    };
                }
                Vec::new()
            }
            // Reassigns the party's members across teams; the local player
            // alone stays on their team.
            "PartyHostReassignTeams" => Vec::new(),
            "PartyIsWaiting" => vec![Value::Boolean(
                (self.front_end.game_lobby && !self.front_end.game_lobby_ready).into(),
            )],
            // Solo Play (`solomatch`) synthesizes super categories and
            // playlists from `zm/mapsTable.csv` and `zm/gametypesTable.csv`
            // for installed maps; online matchmaking has none.
            "GetPlaylistSuperCategories" => {
                let list = match tables.allocate() {
                    Ok(list) => list,
                    Err(fault) => return Some(Err(fault)),
                };
                let filter = tables
                    .global(&key("CoD"))
                    .and_then(|cod| tables.field(&cod, &key("PlaylistCategoryFilter")))
                    .ok()
                    .and_then(|v| text(&v).ok())
                    .unwrap_or_else(|| "solomatch".into());
                let mut seen = Vec::new();
                for (_, super_cat, load_name, map_name, ..) in self.solo_playlists() {
                    if seen.contains(&super_cat) {
                        continue;
                    }
                    seen.push(super_cat);
                    let entry = match tables.allocate() {
                        Ok(entry) => entry,
                        Err(fault) => return Some(Err(fault)),
                    };
                    for (field, value) in [
                        ("index", Value::Number(super_cat as f32)),
                        ("loadName", key(&load_name)),
                        ("name", key(&format!("{map_name}_CAPS"))),
                        ("filter", key(&filter)),
                        ("playerCount", Value::Number(1.0)),
                    ] {
                        if let Err(fault) = tables.raw_set(&entry, &key(field), value) {
                            return Some(Err(fault));
                        }
                    }
                    if let Err(fault) =
                        tables.raw_set(&list, &Value::Number(seen.len() as f32), entry)
                    {
                        return Some(Err(fault));
                    }
                }
                vec![list]
            }
            "GetPlaylistSuperCategoryID" => {
                let playlist_id = super::number(args.first()).unwrap_or(0.0) as i32;
                let playlists = self.solo_playlists();
                let super_cat = playlists
                    .iter()
                    .find(|(id, ..)| *id == playlist_id)
                    .or_else(|| playlists.first())
                    .map_or(1, |(_, cat, ..)| *cat);
                vec![Value::Number(super_cat as f32)]
            }
            "GetPlaylistCategories" => {
                let super_cat = super::number(args.first()).unwrap_or(1.0) as i32;
                let filter = tables
                    .global(&key("CoD"))
                    .and_then(|cod| tables.field(&cod, &key("PlaylistCategoryFilter")))
                    .ok()
                    .and_then(|v| text(&v).ok())
                    .unwrap_or_else(|| "solomatch".into());
                let list = match tables.allocate() {
                    Ok(list) => list,
                    Err(fault) => return Some(Err(fault)),
                };
                let all: Vec<_> = self
                    .solo_playlists()
                    .into_iter()
                    .filter(|(_, cat, ..)| *cat == super_cat)
                    .collect();
                let mut locs: Vec<(String, String)> = Vec::new();
                for (_, _, _, _, loc_ref, loc_name, ..) in &all {
                    if !locs.iter().any(|(r, _)| r == loc_ref) {
                        locs.push((loc_ref.clone(), loc_name.clone()));
                    }
                }
                for (loc_idx, (loc_ref, loc_name)) in locs.into_iter().enumerate() {
                    let entry = match tables.allocate() {
                        Ok(entry) => entry,
                        Err(fault) => return Some(Err(fault)),
                    };
                    let playlists = match tables.allocate() {
                        Ok(playlists) => playlists,
                        Err(fault) => return Some(Err(fault)),
                    };
                    let mut mode_idx = 0;
                    for (id, _, _, _, r, _, gt, gt_name, gt_icon, _) in &all {
                        if *r != loc_ref {
                            continue;
                        }
                        mode_idx += 1;
                        let pl = match tables.allocate() {
                            Ok(pl) => pl,
                            Err(fault) => return Some(Err(fault)),
                        };
                        let desc_prefix = self
                            .table_row(GAMETYPES_TABLE, &[(0, "0"), (1, gt)])
                            .and_then(|row| row.get(3).cloned())
                            .unwrap_or_else(String::new);
                        let desc = format!("{desc_prefix}_{loc_ref}");
                        let icon = self.asset(gt_icon);
                        for (field, value) in [
                            ("index", Value::Number(*id as f32)),
                            ("ref", key(gt)),
                            ("name", key(gt_name)),
                            ("description", key(&desc)),
                            ("icon", icon),
                            ("playerCount", Value::Number(1.0)),
                        ] {
                            if let Err(fault) = tables.raw_set(&pl, &key(field), value) {
                                return Some(Err(fault));
                            }
                        }
                        if let Err(fault) =
                            tables.raw_set(&playlists, &Value::Number(mode_idx as f32), pl)
                        {
                            return Some(Err(fault));
                        }
                    }
                    for (field, value) in [
                        ("ref", key(&loc_ref)),
                        ("name", key(&loc_name)),
                        ("filter", key(&filter)),
                        ("playerCount", Value::Number(1.0)),
                        ("playlists", playlists),
                    ] {
                        if let Err(fault) = tables.raw_set(&entry, &key(field), value) {
                            return Some(Err(fault));
                        }
                    }
                    if let Err(fault) =
                        tables.raw_set(&list, &Value::Number(loc_idx as f32 + 1.0), entry)
                    {
                        return Some(Err(fault));
                    }
                }
                vec![list]
            }
            "GetPlaylistGameType" => {
                let playlist_id = super::number(args.get(1)).unwrap_or(0.0) as i32;
                let gametype = self
                    .solo_playlists()
                    .into_iter()
                    .find(|(id, ..)| *id == playlist_id)
                    .map_or_else(|| "zclassic".into(), |(_, _, _, _, _, _, gt, ..)| gt);
                vec![key(&gametype)]
            }
            "SetPlaylistID" => {
                let playlist_id = super::number(args.first()).unwrap_or(0.0) as i32;
                self.front_end.playlist_id = playlist_id;
                if let Some((_, _, load_name, _, loc_ref, _, gametype, _, _, group)) = self
                    .solo_playlists()
                    .into_iter()
                    .find(|(id, ..)| *id == playlist_id)
                {
                    let _ = self.set_dvar("ui_mapname", &key(&load_name));
                    let _ = self.set_dvar("ui_zm_mapstartlocation", &key(&loc_ref));
                    let _ = self.set_dvar("ui_gametype", &key(&gametype));
                    let _ = self.set_dvar("g_gametype", &key(&gametype));
                    let _ = self.set_dvar("ui_zm_gamemodegroup", &key(&group));
                    let _ = self.set_dvar("party_readypercentrequired", &key("1"));
                    let _ = self.set_dvar("party_readybuttonvisible", &key("1"));
                    let configs = &self.configs;
                    self.front_end.gametype_settings = crate::gametype_settings(
                        &|n: &str| configs.get(&n.to_ascii_lowercase()).map(String::as_str),
                        &gametype,
                    );
                    self.front_end.gametype = gametype;
                }
                Vec::new()
            }
            "GetPlaylistID" => vec![Value::Number(self.front_end.playlist_id as f32)],
            "GetPlaylistCategoryFilter" => {
                let filter = tables
                    .global(&key("CoD"))
                    .and_then(|cod| tables.field(&cod, &key("PlaylistCategoryFilter")))
                    .ok()
                    .and_then(|v| text(&v).ok())
                    .unwrap_or_else(|| "solomatch".into());
                vec![key(&filter)]
            }
            "GetPlaylistName" => {
                let name = self
                    .solo_playlists()
                    .into_iter()
                    .find(|(id, ..)| *id == self.front_end.playlist_id)
                    .map_or_else(
                        String::new,
                        |(_, _, load_name, _, _, _, gt, gt_name, ..)| {
                            let key_name = if gt == "zclassic" {
                                format!("ZMUI_ZCLASSIC_{}_CAPS", load_name.to_ascii_uppercase())
                            } else {
                                gt_name
                            };
                            self.localized(&key_name)
                        },
                    );
                vec![key(&name)]
            }
            "GetPlaylistCategoryName" => {
                let name = self
                    .solo_playlists()
                    .into_iter()
                    .find(|(id, ..)| *id == self.front_end.playlist_id)
                    .map_or_else(String::new, |(_, _, _, _, _, loc_name, ..)| {
                        self.localized(&loc_name)
                    });
                vec![key(&name)]
            }
            // The controller's gamepad motor speeds, left and right.
            "PlayRumble" => {
                let speed = |index: usize| match args.get(index) {
                    Some(Value::Number(speed)) => *speed,
                    _ => 0.0,
                };
                self.front_end.requests.push(UiRequest::Rumble {
                    left: speed(1),
                    right: speed(2),
                });
                Vec::new()
            }
            // The maps table's maps whose pack is the base game's (0) or is
            // there, each at its table index plus one: `loadName`, `name`
            // (its key and `_CAPS`) and `mapPackTypeIndex`.
            "GetMaps" => {
                let list = match tables.allocate() {
                    Ok(list) => list,
                    Err(fault) => return Some(Err(fault)),
                };
                for (index, (load_name, name, pack)) in self.maps_table().into_iter().enumerate() {
                    if pack != 0 && !self.map_pack_available(pack) {
                        continue;
                    }
                    let entry = match tables.allocate() {
                        Ok(entry) => entry,
                        Err(fault) => return Some(Err(fault)),
                    };
                    let set = [
                        ("loadName", key(&load_name)),
                        ("name", key(&format!("{name}_CAPS"))),
                        ("mapPackTypeIndex", Value::Number(pack as f32)),
                    ];
                    for (field, value) in set {
                        if let Err(fault) = tables.raw_set(&entry, &key(field), value) {
                            return Some(Err(fault));
                        }
                    }
                    if let Err(fault) =
                        tables.raw_set(&list, &Value::Number(index as f32 + 1.0), entry)
                    {
                        return Some(Err(fault));
                    }
                }
                vec![list]
            }
            // The zombies executable's maps table.
            "GetCurrentMapTableName" => vec![key(MAPS_TABLE)],
            "GetDStat" => return Some(self.get_dstat(args)),
            // The gametypes table's start locations (`maxnum_startloc` rows
            // `5 <index> <map> <ref> <name>`) on the map, each at its index
            // plus one.
            "GetStartLocsZombie" => {
                let map = match args.first() {
                    Some(Value::Bytes(map)) if !map.is_empty() => {
                        String::from_utf8_lossy(map).into_owned()
                    }
                    _ => return Some(Ok(Vec::new())),
                };
                let list = match tables.allocate() {
                    Ok(list) => list,
                    Err(fault) => return Some(Err(fault)),
                };
                let count = atoi(self.table_lookup(GAMETYPES_TABLE, 0, "maxnum_startloc", 1));
                for index in 0..count.max(0) {
                    let Some(row) =
                        self.table_row(GAMETYPES_TABLE, &[(0, "5"), (1, &index.to_string())])
                    else {
                        continue;
                    };
                    if !row
                        .get(2)
                        .is_some_and(|cell| cell.eq_ignore_ascii_case(&map))
                    {
                        continue;
                    }
                    let entry = match tables.allocate() {
                        Ok(entry) => entry,
                        Err(fault) => return Some(Err(fault)),
                    };
                    for (field, column) in [("ref", 3), ("name", 4)] {
                        let cell = row.get(column).map_or("", String::as_str);
                        if let Err(fault) = tables.raw_set(&entry, &key(field), key(cell)) {
                            return Some(Err(fault));
                        }
                    }
                    if let Err(fault) =
                        tables.raw_set(&list, &Value::Number(index as f32 + 1.0), entry)
                    {
                        return Some(Err(fault));
                    }
                }
                vec![list]
            }
            // The gamemodes of the start location `ref` on the map: the
            // `startloc_gamemode_map` rows `6 <index> <map> <ref> <gametype>`
            // in order, at most the location's count (its row's column 7),
            // each `{ ref = gametype, name = its name key }`; nothing for a
            // location the table does not have.
            "GetGamemodesZombie" => {
                let text_arg = |index: usize| match args.get(index) {
                    Some(Value::Bytes(value)) => String::from_utf8_lossy(value).into_owned(),
                    _ => String::new(),
                };
                let (map, location) = (text_arg(0), text_arg(1));
                if map.is_empty() || location.is_empty() {
                    return Some(Ok(Vec::new()));
                }
                let locations = atoi(self.table_lookup(GAMETYPES_TABLE, 0, "maxnum_startloc", 1));
                let Some(start) = (0..locations.max(0)).find_map(|index| {
                    self.table_row(
                        GAMETYPES_TABLE,
                        &[(0, "5"), (1, &index.to_string()), (2, &map), (3, &location)],
                    )
                }) else {
                    return Some(Ok(Vec::new()));
                };
                let limit = atoi(start.get(7).map_or("", String::as_str));
                let rows = atoi(self.table_lookup(GAMETYPES_TABLE, 0, "startloc_gamemode_map", 1));
                let list = match tables.allocate() {
                    Ok(list) => list,
                    Err(fault) => return Some(Err(fault)),
                };
                let mut count = 0;
                for index in 0..rows.max(0) {
                    if count >= limit {
                        break;
                    }
                    let Some(row) = self.table_row(
                        GAMETYPES_TABLE,
                        &[(0, "6"), (1, &index.to_string()), (2, &map), (3, &location)],
                    ) else {
                        continue;
                    };
                    let gametype = row.get(4).cloned().unwrap_or_else(String::new);
                    let name = self
                        .table_row(GAMETYPES_TABLE, &[(0, "0"), (1, &gametype)])
                        .and_then(|row| row.get(2).cloned())
                        .unwrap_or_else(String::new);
                    let entry = match tables.allocate() {
                        Ok(entry) => entry,
                        Err(fault) => return Some(Err(fault)),
                    };
                    for (field, value) in [("ref", gametype), ("name", name)] {
                        if let Err(fault) = tables.raw_set(&entry, &key(field), key(&value)) {
                            return Some(Err(fault));
                        }
                    }
                    count += 1;
                    if let Err(fault) = tables.raw_set(&list, &Value::Number(count as f32), entry) {
                        return Some(Err(fault));
                    }
                }
                vec![list]
            }
            // Whether a map pack's content is there (the base game's always).
            "HasDLCContent" => {
                let pack = super::number(args.first()).map_or(0, |pack| pack as i32);
                vec![Value::Boolean(
                    (pack == 0 || self.map_pack_available(pack)).into(),
                )]
            }
            // The globe's 3D render and VOIP indicator draw nothing yet
            // (`t6.ui.render`).
            "setupGlobe" | "setupVoipImage" => Vec::new(),
            _ => return None,
        }))
    }
}

impl Scene {
    /// A scene that runs Black Ops 2's front end instead of the match HUD.
    pub fn front_end(modules: &[Module]) -> Result<Self, String> {
        let mut scene = Self::new(modules)?;
        scene.front_end = true;
        scene.bindings.front_end.active = true;
        scene
            .bindings
            .install_front_end(&mut scene.runtime.tables)
            .map_err(|e| format!("t6.ui.engine_api: {e:?}"))?;
        Ok(scene)
    }

    /// Runs `require` for a module named as the scripts name it
    /// (`T6.main`), once.
    fn require_module(&mut self, name: &str) -> Result<(), String> {
        let suffix = name.replace('.', "/").to_ascii_lowercase() + ".lua";
        let path = [format!("ui/{suffix}"), format!("ui_mp/{suffix}")]
            .into_iter()
            .find(|path| self.bindings.modules.contains_key(path))
            .ok_or_else(|| format!("t6.ui.bootstrap: module {name} missing"))?;
        if self.bindings.loaded.contains(&path) {
            return Ok(());
        }
        self.module(&path)
    }

    /// The engine's front-end LUI init: `T6.CoDRoot`, the full-screen root
    /// `LUI.CoDRoot.new("UIRootFull")`, then `T6.main` and
    /// `T6.Zombie.ffotdzm`, then an immediate `addmenu` event naming `main`
    /// (no controller) on that root.
    pub(super) fn bootstrap_front_end(&mut self, input: &Input) -> Result<(), String> {
        self.bindings.input = input.clone();
        self.bindings.zero_profile_settings();
        self.require_module(FRONT_END_MODULES[0])?;
        let root = self
            .invoke(&["LUI", "CoDRoot", "new"], &[key(FRONT_END_ROOT)])?
            .first()
            .cloned()
            .ok_or("t6.ui.render: root missing")?;
        let state = &mut self
            .bindings
            .elements
            .element_mut(&root)
            .map_err(|e| format!("{e:?}"))?
            .current;
        state.horizontal = Axis {
            anchor_start: true,
            anchor_end: true,
            start: 0.0,
            end: 0.0,
        };
        state.vertical = state.horizontal;
        for name in &FRONT_END_MODULES[1..] {
            self.require_module(name)?;
        }
        let event = self
            .runtime
            .tables
            .allocate()
            .map_err(|e| format!("{e:?}"))?;
        for (field, value) in [
            ("name", key("addmenu")),
            ("menu", key(FRONT_END_MENU)),
            ("immediate", Value::Boolean(1)),
        ] {
            self.runtime
                .tables
                .raw_set(&event, &key(field), value)
                .map_err(|e| format!("{e:?}"))?;
        }
        let opened = self.dispatch_event(&root, "processEvent", "addmenu", event);
        self.report_errors();
        opened?;
        self.root = Some(root.clone());
        self.bindings.front_end.root = Some(root.clone());
        // Com_Init's `UI_SetActiveMenu(UIMENU_MAIN)` for the primary local
        // client: `open_menu` naming `MainMenu`, with its controller.
        self.send(
            &root,
            "open_menu",
            &[
                ("menuName", key(MAIN_MENU)),
                ("controller", Value::Number(PRIMARY_CONTROLLER)),
            ],
        )
    }

    /// Sends the root an event with these fields, as the engine's
    /// `LUI_BeginEvent`/`LUI_EndEvent` do (`LUI.roots[root]:processEvent`).
    pub(super) fn send(
        &mut self,
        root: &Value,
        name: &str,
        fields: &[(&str, Value)],
    ) -> Result<(), String> {
        let event = self
            .runtime
            .tables
            .allocate()
            .map_err(|e| format!("{e:?}"))?;
        self.runtime
            .tables
            .raw_set(&event, &key("name"), key(name))
            .map_err(|e| format!("{e:?}"))?;
        for (field, value) in fields {
            self.runtime
                .tables
                .raw_set(&event, &key(field), value.clone())
                .map_err(|e| format!("{e:?}"))?;
        }
        let sent = self.dispatch_event(root, "processEvent", name, event);
        self.report_errors();
        sent
    }
}

impl Scene {
    /// One front-end frame: the frame's draws.
    pub(super) fn update_front_end(
        &mut self,
        root: &Value,
        input: &Input,
    ) -> Result<Vec<game_api::ui::Draw>, String> {
        self.bindings.run_queued();
        if self.bindings.front_end.pending_lobby_update {
            self.bindings.front_end.pending_lobby_update = false;
            self.send(
                root,
                "gamelobby_update",
                &[("controller", Value::Number(PRIMARY_CONTROLLER))],
            )?;
        }
        self.send_input(root, input)?;
        if let Some(end) = self.bindings.front_end.countdown_end
            && input.time >= end
        {
            self.bindings.front_end.countdown_end = None;
            self.bindings.front_end.game_lobby_ready = false;
            if let Some((_, map)) = self.bindings.front_end.dvars.get("ui_mapname")
                && !map.is_empty()
            {
                let map = map.strip_prefix("t6:").unwrap_or(map).to_owned();
                let dvar = |name: &str| {
                    self.bindings
                        .front_end
                        .dvars
                        .get(name)
                        .map_or_else(String::new, |(_, value)| value.clone())
                };
                let start = UiRequest::ZombiesStart {
                    gametype: dvar("ui_gametype"),
                    location: dvar("ui_zm_mapstartlocation"),
                };
                let requests = &mut self.bindings.front_end.requests;
                requests.push(start);
                requests.push(UiRequest::Exec(format!("map t6:{map}")));
            }
        }
        // Each frame the engine has every root process its queued events
        // (`process_events`, immediate), then runs its animations.
        self.send(root, "process_events", &[("immediate", Value::Boolean(1))])?;
        self.paint(root, input)
    }

    /// The frame's keys and mouse, as `LUI_KeyEvent` and the mouse handlers
    /// send them: `gamepad_button` with the button, `down`, a `qualifier`
    /// when there is one, immediate; `mousemove`, `mousedown` and `mouseup`
    /// with the root's name and the cursor in its units.
    pub(super) fn send_input(&mut self, root: &Value, input: &Input) -> Result<(), String> {
        let controller = Value::Number(input.local_client as f32);
        for event in &input.events {
            match event {
                UiEvent::Button {
                    button,
                    qualifier,
                    down,
                } => {
                    let mut fields = vec![
                        ("controller", controller.clone()),
                        ("button", key(button)),
                        ("down", Value::Boolean((*down).into())),
                    ];
                    if let Some(qualifier) = qualifier {
                        fields.push(("qualifier", key(qualifier)));
                    }
                    fields.push(("immediate", Value::Boolean(1)));
                    self.send(root, "gamepad_button", &fields)?;
                }
                UiEvent::MouseMove { x, y } => self.send(
                    root,
                    "mousemove",
                    &[
                        ("rootName", key(FRONT_END_ROOT)),
                        ("x", Value::Number(*x)),
                        ("y", Value::Number(*y)),
                        ("controller", controller.clone()),
                    ],
                )?,
                UiEvent::Mouse { button, down, x, y } => self.send(
                    root,
                    if *down { "mousedown" } else { "mouseup" },
                    &[
                        ("rootName", key(FRONT_END_ROOT)),
                        ("x", Value::Number(*x)),
                        ("y", Value::Number(*y)),
                        ("controller", controller.clone()),
                        ("button", key(button)),
                    ],
                )?,
                UiEvent::InGameMenu { menu } => {
                    if !self.bindings.front_end.ingame_menu_open {
                        self.bindings.front_end.ingame_menu_open = true;
                        if let Some((_, value)) = self.bindings.front_end.dvars.get_mut("cl_paused")
                        {
                            *value = "1".into();
                        }
                        self.send(
                            root,
                            "open_ingame_menu",
                            &[
                                ("menuName", key(menu)),
                                ("controller", controller.clone()),
                                ("unpausable", Value::Number(1.0)),
                                ("immediate", Value::Boolean(1)),
                            ],
                        )?;
                    } else {
                        for down in [1, 0] {
                            self.send(
                                root,
                                "gamepad_button",
                                &[
                                    ("controller", controller.clone()),
                                    ("button", key("secondary")),
                                    ("down", Value::Boolean(down)),
                                    ("immediate", Value::Boolean(1)),
                                ],
                            )?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
