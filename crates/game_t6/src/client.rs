//! Black Ops 2's client scripts (`.csc`): the modules its client game loads,
//! the map entities it makes into structs and the functions it runs as it
//! starts, in its loader's order.

use game_api::{ClientScriptCallbacks, ClientScriptProgram, KeyType, Rule, ScriptRequest, unknown};

use crate::{client_builtins, keys, startup};

const ZOMBIES_CALLBACKS: &str = "clientscripts/mp/zombies/_callbacks";
const MULTIPLAYER_CALLBACKS: &str = "clientscripts/mp/_callbacks";

/// The engine's own client modules the loader asks for after the code
/// scripts and the callbacks, before the map's.
const ENGINE_MODULES: [&str; 3] = [
    "clientscripts/mp/_dogs",
    "clientscripts/mp/_fx",
    "clientscripts/mp/_footsteps",
];

pub(crate) fn program(
    request: &ScriptRequest<'_>,
    sources: &dyn gsc::SourceResolver,
) -> Rule<ClientScriptProgram> {
    let Some(start) = startup::ZombiesStart::of(
        request.map,
        request.gametype,
        request.location,
        request.table,
    ) else {
        return Rule::Unknown(unknown!(
            "t6.scripts.zm_start",
            "where a Black Ops 2 zombies map starts: its default row in zm/gametypestable.csv",
            "the map's start location and gametype"
        ));
    };
    build(request, sources, ZOMBIES_CALLBACKS, &start.gametype)
}

/// A multiplayer map's client scripts: `clientscripts/mp/_callbacks` and
/// the gametype's own client module (`clientscripts/mp/gametypes/<g>`).
/// `t6mp.exe`'s loader is not read: it is taken to ask for the same engine
/// modules as `t6zm.exe`'s with the multiplayer callbacks in place of the
/// zombies ones (`t6.csc.mp_loader`).
pub(crate) fn multiplayer_program(
    request: &ScriptRequest<'_>,
    sources: &dyn gsc::SourceResolver,
) -> Rule<ClientScriptProgram> {
    build(
        request,
        sources,
        MULTIPLAYER_CALLBACKS,
        crate::script_gametype(request.gametype),
    )
}

fn build(
    request: &ScriptRequest<'_>,
    sources: &dyn gsc::SourceResolver,
    callbacks_module: &str,
    gametype: &str,
) -> Rule<ClientScriptProgram> {
    let entities = entity_pairs(request.entities);
    let aitypes = aitypes(&entities);
    let gametype = format!("clientscripts/mp/gametypes/{gametype}");
    let map = format!("clientscripts/mp/{}", request.map);
    let roots: Vec<String> = ["codescripts/delete", "codescripts/struct", callbacks_module]
        .into_iter()
        .chain(ENGINE_MODULES)
        .map(str::to_owned)
        .chain(
            aitypes
                .iter()
                .map(|aitype| format!("aitype/clientscripts/{aitype}")),
        )
        .chain([gametype.clone(), map.clone()])
        .collect();
    let read = |name: &str| {
        let module = format!("{name}.csc");
        sources
            .read_module(&module)
            .ok()
            .map(|bytes| gsc_t6::Source {
                name: name.to_owned(),
                bytes,
                origin: sources.origin(&module),
            })
    };
    let (modules, missing) = gsc_t6::load(&read, &roots);
    let builtins = gsc_t6::Builtins {
        functions: &client_builtins::CLIENT_FUNCTIONS,
        methods: &client_builtins::CLIENT_METHODS,
    };
    let built = gsc_t6::build(&modules, &builtins, asset_core::FamilyId::T6);
    for line in missing.iter().chain(&built.report) {
        diag::info!(Sim, "client {line}");
    }
    if !built.unresolved.is_empty() {
        return Rule::Unknown(unknown!(
            "t6.csc.unresolved",
            "Black Ops 2's client scripts call into modules or builtins the install does not carry",
            "the client modules the loader links them with"
        ));
    }
    let program = built.program;
    let key_types = (request.config)("radiant/keys.txt").map_or_else(Vec::new, keys::parse_client);
    let has = |name: &str| program.names.contains_key(name).then(|| name.to_owned());
    // The engine runs each actor type's precache once, in map order, with
    // the type's index (from one, in the order the loader met the types).
    let precaches = aitypes
        .iter()
        .zip(1..)
        .filter_map(|(aitype, index)| {
            has(&format!("aitype/clientscripts/{aitype}::precache")).map(|name| (name, index))
        })
        .collect();
    // The level's structs, then the actor precaches, then the gametype's
    // main, the map's main, the callbacks' precache and finalize, and the
    // gametype's start.
    let starts = [
        format!("{gametype}::main"),
        format!("{map}::main"),
        format!("{callbacks_module}::codecallback_precachegametype"),
        format!("{callbacks_module}::codecallback_finalizeinitialization"),
        format!("{callbacks_module}::codecallback_startgametype"),
    ]
    .iter()
    .filter_map(|name| has(name))
    .collect();
    let callbacks = ClientScriptCallbacks {
        local_client_connect: has(&format!("{callbacks_module}::localclientconnect")),
        player_spawned: has(&format!("{callbacks_module}::playerspawned")),
        entity_spawned: has(&format!("{callbacks_module}::entityspawned")),
        entity_shutdown: has(&format!("{callbacks_module}::entityshutdown_callback")),
        state_change: has(&format!("{callbacks_module}::statechange")),
        activate_exploder: has(&format!("{callbacks_module}::callback_activate_exploder")),
        deactivate_exploder: has(&format!("{callbacks_module}::callback_deactivate_exploder")),
        gib_event: has(&format!("{callbacks_module}::codecallback_gibevent")),
        player_jump: has(&format!("{callbacks_module}::codecallback_playerjump")),
        player_land: has(&format!("{callbacks_module}::codecallback_playerland")),
        player_foliage: has(&format!("{callbacks_module}::codecallback_playerfoliage")),
        sound_set_ambient_state: has(&format!("{callbacks_module}::soundsetambientstate")),
    };
    let (client_ents, entity_spawnflags) = client_entities(&entities, &key_types)
        .into_iter()
        .map(|(etype, fields, flags)| ((etype, fields), flags))
        .unzip();
    let dlc_packages = (request.config)("dlc/mounted.txt")
        .map(|text| {
            text.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_else(Vec::new);
    Rule::Known(ClientScriptProgram {
        entities: client_ents,
        entity_spawnflags,
        structs: structs(&entities, &key_types),
        precaches,
        starts,
        callbacks,
        dlc_packages,
        program,
    })
}

/// The map's entities, each as its key and value pairs.
fn entity_pairs(text: &str) -> Vec<Vec<(String, String)>> {
    let mut entities = Vec::new();
    let mut current: Option<Vec<(String, String)>> = None;
    let mut key: Option<String> = None;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                current = Some(Vec::new());
                key = None;
            }
            '}' => {
                entities.extend(current.take());
                key = None;
            }
            '"' => {
                let token: String = chars.by_ref().take_while(|&c| c != '"').collect();
                match key.take() {
                    None => key = Some(token),
                    Some(name) => {
                        if let Some(pairs) = current.as_mut() {
                            pairs.push((name, token));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    entities
}

fn value<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(key))
        .map(|(_, value)| value.as_str())
}

/// The actor types the map's spawners name (`actor_<aitype>`), each once, in
/// the order the loader meets them.
fn aitypes(entities: &[Vec<(String, String)>]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for pairs in entities {
        if let Some(aitype) = value(pairs, "classname").and_then(|c| c.strip_prefix("actor_"))
            && !found.iter().any(|known| known == aitype)
        {
            found.push(aitype.to_owned());
        }
    }
    found
}

/// The client entity field table (`CG_ParseEntityFields`), before the
/// client's key table (`radiant/keys.txt`).
const CLIENT_ENTITY_FIELDS: [(&str, KeyType); 5] = [
    ("classname", KeyType::String),
    ("origin", KeyType::Vector),
    ("angles", KeyType::Vector),
    ("model", KeyType::String),
    ("targetname", KeyType::String),
];

/// A client entity: its `eType`, typed fields and spawnflags.
type ClientEntity = (i32, Vec<(String, KeyType, String)>, i32);

/// The map entities the client spawns itself (`CG_SpawnEntities` ->
/// `CG_CallSpawn` -> `CG_ParseEntityFields`): client-side `script_model`,
/// `script_origin` and `script_brushmodel` entities without a
/// `destructibledef` (`eType` 6), and client triggers with spawnflags
/// `0x880` (`eType` 0), each with its spawnflags.
fn client_entities(
    entities: &[Vec<(String, String)>],
    key_types: &[(String, KeyType)],
) -> Vec<ClientEntity> {
    let mut out = Vec::new();
    for pairs in entities.iter().skip(1) {
        let classname = value(pairs, "classname").unwrap_or("");
        let flags = value(pairs, "spawnflags").map_or(0, atoi);
        let clientside =
            flags & 2 != 0 || value(pairs, "clientside").is_some_and(|v| atof(v) == 1.0);
        let mover = ["script_model", "script_origin", "script_brushmodel"]
            .iter()
            .any(|class| classname.eq_ignore_ascii_case(class))
            && value(pairs, "destructibledef").is_none()
            && clientside;
        let trigger = [
            "trigger_multiple",
            "trigger_radius",
            "trigger_box",
            "trigger_once",
        ]
        .iter()
        .any(|class| classname.eq_ignore_ascii_case(class))
            && flags & 0x880 != 0;
        let etype = if mover {
            6
        } else if trigger {
            0
        } else {
            continue;
        };
        let fields = pairs
            .iter()
            .filter_map(|(key, val)| {
                let (name, ty) = CLIENT_ENTITY_FIELDS
                    .iter()
                    .find(|(name, _)| name.eq_ignore_ascii_case(key))
                    .map(|(name, ty)| ((*name).to_owned(), *ty))
                    .or_else(|| {
                        key_types
                            .iter()
                            .find(|(name, _)| name.eq_ignore_ascii_case(key))
                            .cloned()
                    })?;
                Some((name, ty, val.clone()))
            })
            .collect();
        out.push((etype, fields, flags));
    }
    out
}

/// The entities the client makes into structs: script structs, models and
/// brush models, lights without spawnflag 16 (whose `pl#` key it reads as
/// `pl`), and path nodes that carry a `script_noteworthy`. Each keeps the keys
/// the client's key table lists, with their types.
fn structs(
    entities: &[Vec<(String, String)>],
    key_types: &[(String, KeyType)],
) -> Vec<Vec<(String, KeyType, String)>> {
    let typed = |pairs: &[(String, String)], light: bool| {
        pairs
            .iter()
            .filter_map(|(key, value)| {
                let key = if light && key.eq_ignore_ascii_case("pl#") {
                    "pl"
                } else {
                    key.as_str()
                };
                let (name, ty) = key_types
                    .iter()
                    .find(|(name, _)| name.eq_ignore_ascii_case(key))?;
                Some((name.clone(), *ty, value.clone()))
            })
            .collect()
    };
    let mut out = Vec::new();
    for pairs in entities {
        let classname = value(pairs, "classname").unwrap_or("");
        if ["script_struct", "script_brushmodel", "script_model"]
            .iter()
            .any(|class| classname.eq_ignore_ascii_case(class))
        {
            out.push(typed(pairs, false));
        } else if classname.eq_ignore_ascii_case("light") {
            let flags = value(pairs, "spawnflags").map_or(0, atoi);
            if flags & 0x10 == 0 {
                out.push(typed(pairs, true));
            }
        } else if classname.eq_ignore_ascii_case("node_pathnode")
            && value(pairs, "script_noteworthy").is_some()
        {
            out.push(typed(pairs, false));
        }
    }
    out
}

/// C's `atoi`: leading white space, a sign, then digits.
fn atoi(text: &str) -> i32 {
    let text = text.trim_start();
    let (sign, digits) = match text.as_bytes().first() {
        Some(b'-') => (-1, &text[1..]),
        Some(b'+') => (1, &text[1..]),
        _ => (1, text),
    };
    let value = digits
        .bytes()
        .take_while(u8::is_ascii_digit)
        .fold(0i32, |n, d| {
            n.wrapping_mul(10).wrapping_add(i32::from(d - b'0'))
        });
    sign * value
}

fn atof(text: &str) -> f64 {
    let text = text.trim_start();
    text.find(|c: char| !c.is_ascii_digit() && !matches!(c, '+' | '-' | '.' | 'e' | 'E'))
        .map_or(text, |end| &text[..end])
        .parse::<f64>()
        .unwrap_or(0.0)
}
