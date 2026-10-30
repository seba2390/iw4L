/// The startup IW4L writes for a Black Ops 2 map: the generated map main.
pub(crate) struct T6Startup {
    pub(crate) roots: Vec<String>,
    pub(crate) entries: Vec<String>,
}

impl T6Startup {
    pub(crate) fn new(map: &str) -> Self {
        let module = format!("maps/mp/{map}");
        Self {
            entries: vec![format!("{module}::main")],
            roots: vec![module],
        }
    }
}

/// Where a zombies map starts: its gametype, start location and mode group,
/// a start row of `zm/gametypestable.csv`.
pub(crate) struct ZombiesStart {
    pub(crate) gametype: String,
    pub(crate) location: String,
    pub(crate) group: String,
}

pub(crate) const GAMETYPES_TABLE: &str = "zm/gametypestable.csv";
/// The table's sections, by first cell: gametypes and start locations per map.
const GAMETYPE_ROW: &str = "0";
const START_ROW: &str = "6";

/// A gametype's mode group (`ui_zm_gamemodegroup`): the last column of its
/// gametype row.
pub(crate) fn mode_group(columns: usize, cells: &[String], gametype: &str) -> Option<String> {
    cells
        .chunks_exact(columns.max(1))
        .find(|row| {
            row.first().is_some_and(|c| c == GAMETYPE_ROW)
                && row.get(1).is_some_and(|c| c == gametype)
        })?
        .get(9)
        .filter(|group| !group.is_empty())
        .cloned()
}

impl ZombiesStart {
    /// The start row for the map, gametype and location: with no location,
    /// the gametype's row marked `YES`, else its first. A map with no row for
    /// the gametype starts at its own default (the `YES` row of any
    /// gametype), as Nuketown, which has only Survival, does under Classic.
    pub(crate) fn of<'a>(
        map: &str,
        gametype: &str,
        location: Option<&str>,
        table: &game_api::TableLookup<'a>,
    ) -> Option<Self> {
        let (columns, cells) = table(GAMETYPES_TABLE)?;
        let starts = || {
            cells.chunks_exact(columns.max(1)).filter(|row| {
                row.first().is_some_and(|c| c == START_ROW) && row.get(2).is_some_and(|c| c == map)
            })
        };
        let default = |row: &&[String]| row.get(7).is_some_and(|c| c == "YES");
        let start = match location {
            Some(location) => starts().find(|row| {
                row.get(4).is_some_and(|c| c == gametype)
                    && row.get(3).is_some_and(|c| c == location)
            })?,
            None => {
                let of_gametype =
                    || starts().filter(|row| row.get(4).is_some_and(|c| c == gametype));
                of_gametype()
                    .find(default)
                    .or_else(|| of_gametype().next())
                    .or_else(|| starts().find(default))?
            }
        };
        let gametype = start.get(4)?.clone();
        Some(Self {
            location: start.get(3)?.clone(),
            group: mode_group(columns, cells, &gametype)?,
            gametype,
        })
    }

    /// The scripts the engine runs as a zombies level starts, in its order.
    pub(crate) fn entries(&self, map: &str, program: &gsc::Program) -> Vec<String> {
        let callbacks = "maps/mp/gametypes_zm/_callbacksetup";
        [
            format!("maps/mp/{map}::gamemode_callback_setup"),
            format!("maps/mp/gametypes_zm/{}::main", self.gametype),
            format!("maps/mp/{map}::main"),
            format!("{callbacks}::codecallback_startgametype"),
            format!("{callbacks}::codecallback_finalizeinitialization"),
        ]
        .into_iter()
        .filter(|entry| program.names.contains_key(entry))
        .collect()
    }
}

/// The animscripts the engine loads for its zombie and zombie dog actors.
const ANIMSCRIPTS: [&str; 17] = [
    "zm_combat",
    "zm_death",
    "zm_init",
    "zm_pain",
    "zm_move",
    "zm_scripted",
    "zm_stop",
    "zm_flashed",
    "zm_jump",
    "zm_turn",
    "zm_dog_combat",
    "zm_dog_death",
    "zm_dog_init",
    "zm_dog_pain",
    "zm_dog_move",
    "zm_dog_stop",
    "zm_dog_flashed",
];

/// The actor scripts the engine loads with a level: the animscripts, the
/// traversal each negotiation begin node names, then the aitype of each
/// actor spawner the map places (`actor_<aitype>`).
pub(crate) fn actor_scripts(entities: &str, traversals: &[String]) -> Vec<String> {
    let mut scripts: Vec<String> = ANIMSCRIPTS
        .iter()
        .map(|name| format!("maps/mp/animscripts/{name}"))
        .chain(
            traversals
                .iter()
                .map(|name| format!("maps/mp/animscripts/traverse/{name}")),
        )
        .collect();
    scripts.extend(aitype_scripts(entities));
    scripts
}

/// The script of each actor type the map's spawners name, once each.
pub(crate) fn aitype_scripts(entities: &str) -> Vec<String> {
    let mut scripts: Vec<String> = Vec::new();
    let mut tokens = entities.split('"').skip(1).step_by(2);
    while let Some(key) = tokens.next() {
        let Some(value) = tokens.next() else {
            break;
        };
        if key == "classname"
            && let Some(aitype) = value.strip_prefix("actor_")
        {
            let script = format!("aitype/{aitype}");
            if !scripts.contains(&script) {
                scripts.push(script);
            }
        }
    }
    scripts
}

/// The multiplayer actors' animscripts (the K9 Unit's dogs).
pub(crate) const MULTIPLAYER_ANIMSCRIPTS: [&str; 11] = [
    "dog_combat",
    "dog_death",
    "dog_flashed",
    "dog_init",
    "dog_jump",
    "dog_move",
    "dog_pain",
    "dog_stop",
    "dog_turn",
    "shared",
    "utility",
];

/// A multiplayer map's factions, allies then axis: its row of
/// `mp/mapstable.csv`, map name in the first column.
pub(crate) fn map_factions(table: &game_api::TableLookup<'_>, map: &str) -> Vec<String> {
    let Some((columns, cells)) = table("mp/mapstable.csv").filter(|(columns, _)| *columns > 2)
    else {
        return Vec::new();
    };
    cells
        .chunks_exact(columns)
        .find(|row| row[0].eq_ignore_ascii_case(map))
        .map_or_else(Vec::new, |row| vec![row[1].clone(), row[2].clone()])
}
