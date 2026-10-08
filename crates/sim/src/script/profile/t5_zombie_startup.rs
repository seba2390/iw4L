use crate::script::source::SourceResolver;

/// The zombie mode starts from the map's own main, which sets up `_zombiemode`;
/// the code callbacks live in the singleplayer `_callbacksetup`.
pub struct T5ZombieStartup {
    pub roots: Vec<String>,
    pub entries: Vec<String>,
}
impl T5ZombieStartup {
    /// `entities` is the map's entity string; each actor spawner names the
    /// aitype script that dresses the AI it spawns.
    pub fn new(resolver: &impl SourceResolver, map: &str, entities: &str) -> Self {
        let map = format!("maps/{map}");
        let callbacks = "maps/_callbacksetup";
        let aitypes = aitype_modules(entities);
        let mut roots = vec![
            "codescripts/delete".to_owned(),
            "codescripts/struct".to_owned(),
            callbacks.to_owned(),
            map.clone(),
        ];
        roots.extend(aitypes.iter().cloned());
        roots.extend(
            crate::script::host::actor_brain::ANIMSCRIPT_MODULES
                .iter()
                .filter(|module| resolver.read_bytes(module).is_ok())
                .map(|module| (*module).to_owned()),
        );
        let mut entries: Vec<String> = aitypes
            .iter()
            .map(|module| format!("{module}::precache"))
            .collect();
        entries.push(format!("{map}::main"));
        entries.push(format!("{callbacks}::codecallback_startgametype"));
        Self { roots, entries }
    }
}

fn aitype_modules(entities: &str) -> Vec<String> {
    let mut modules: Vec<String> = entities
        .lines()
        .filter_map(|line| {
            let mut quoted = line.split('"').skip(1).step_by(2);
            match (quoted.next(), quoted.next()) {
                (Some("classname"), Some(class)) => class.strip_prefix("actor_"),
                _ => None,
            }
        })
        .map(|aitype| format!("aitype/{}", aitype.to_ascii_lowercase()))
        .collect();
    modules.sort_unstable();
    modules.dedup();
    modules
}
