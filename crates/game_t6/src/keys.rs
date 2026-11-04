use game_api::KeyType;

/// The keys the engine reads into an entity itself, with their types.
pub(crate) const ENTITY_FIELDS: [(&str, KeyType); 15] = [
    ("classname", KeyType::String),
    ("origin", KeyType::Vector),
    ("model", KeyType::String),
    ("spawnflags", KeyType::Int),
    ("target", KeyType::String),
    ("targetname", KeyType::String),
    ("script_noteworthy", KeyType::String),
    ("count", KeyType::Int),
    ("health", KeyType::Int),
    ("dmg", KeyType::Int),
    ("angles", KeyType::Vector),
    ("birthtime", KeyType::Int),
    ("index", KeyType::Int),
    ("lerp_to_lighter", KeyType::Float),
    ("lerp_to_darker", KeyType::Float),
];

/// The keys the engine's vehicle node spawn reads, with their types: its own
/// table, which also names the nodes' script fields.
const VEHICLE_NODE_FIELDS: [(&str, KeyType); 12] = [
    ("targetname", KeyType::String),
    ("target", KeyType::String),
    ("target2", KeyType::String),
    ("script_linkname", KeyType::String),
    ("script_noteworthy", KeyType::String),
    ("origin", KeyType::Vector),
    ("angles", KeyType::Vector),
    ("speed", KeyType::Float),
    ("radius", KeyType::Float),
    ("lookahead", KeyType::Float),
    ("tension", KeyType::Float),
    ("spawnflags", KeyType::Int),
];

/// The classes spawned as vehicle nodes.
const VEHICLE_NODE_CLASSES: [&str; 2] = ["info_vehicle_node", "info_vehicle_node_rotate"];

/// The entity fields, then each vehicle node class's own keys qualified by
/// the class (`info_vehicle_node.lookahead`).
pub(crate) fn entity_fields() -> Vec<(String, KeyType)> {
    ENTITY_FIELDS
        .iter()
        .map(|&(name, ty)| (name.to_owned(), ty))
        .chain(VEHICLE_NODE_CLASSES.iter().flat_map(|class| {
            VEHICLE_NODE_FIELDS
                .iter()
                .map(move |&(name, ty)| (format!("{class}.{name}"), ty))
        }))
        .collect()
}

/// `radiant/keys.txt`: `<type> <key>` per entry after `//` comments are
/// dropped; a `client` entry is the client's alone. The first entry of a key
/// is the one the engine finds.
pub(crate) fn parse(text: &str) -> Vec<(String, KeyType)> {
    parse_for(text, false)
}

/// The client's key table: the same file, with its `client` entries.
pub(crate) fn parse_client(text: &str) -> Vec<(String, KeyType)> {
    parse_for(text, true)
}

fn parse_for(text: &str, client: bool) -> Vec<(String, KeyType)> {
    let mut keys: Vec<(String, KeyType)> = Vec::new();
    for line in text.lines() {
        let line = line.split("//").next().unwrap_or("");
        let mut words = line.split_whitespace().peekable();
        if client && words.peek() == Some(&"client") {
            words.next();
        }
        let ty = match words.next() {
            Some("int") => KeyType::Int,
            Some("float") => KeyType::Float,
            Some("string") => KeyType::String,
            Some("vector") => KeyType::Vector,
            _ => continue,
        };
        let Some(key) = words.next() else { continue };
        let key = key.to_ascii_lowercase();
        if !keys.iter().any(|(known, _)| *known == key) {
            keys.push((key, ty));
        }
    }
    keys
}
