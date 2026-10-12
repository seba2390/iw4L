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

/// Where a zombies map starts by default, as `zm/gametypestable.csv` lists it.
pub(crate) struct ZombiesStart {
    pub(crate) gametype: String,
    pub(crate) location: String,
    pub(crate) group: String,
}

const GAMETYPES_TABLE: &str = "zm/gametypestable.csv";
/// The table's sections, by first cell: gametypes and start locations per map.
const GAMETYPE_ROW: &str = "0";
const START_ROW: &str = "6";

impl ZombiesStart {
    pub(crate) fn of<'a>(map: &str, table: &game_api::TableLookup<'a>) -> Option<Self> {
        let (columns, cells) = table(GAMETYPES_TABLE)?;
        let rows = || cells.chunks_exact(columns.max(1));
        let start = rows().find(|row| {
            row.first().is_some_and(|c| c == START_ROW)
                && row.get(2).is_some_and(|c| c == map)
                && row.get(7).is_some_and(|c| c == "YES")
        })?;
        let gametype = start.get(4)?.clone();
        let group = rows()
            .find(|row| {
                row.first().is_some_and(|c| c == GAMETYPE_ROW) && row.get(1) == Some(&gametype)
            })?
            .get(9)?
            .clone();
        Some(Self {
            location: start.get(3)?.clone(),
            gametype,
            group,
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
