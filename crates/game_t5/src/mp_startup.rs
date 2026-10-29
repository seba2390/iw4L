use gsc::SourceResolver;

/// Black Ops' multiplayer level start, as its server runs it: the gametype
/// script's `main`, then the map's `main`, then `CodeCallback_StartGameType`
/// from the multiplayer `_callbacksetup`.
pub struct T5MpStartup {
    pub roots: Vec<String>,
    pub entries: Vec<String>,
}
impl T5MpStartup {
    pub fn new(resolver: &dyn SourceResolver, gametype: &str, map: &str) -> Self {
        let gametype = format!("maps/mp/gametypes/{gametype}");
        let map = format!("maps/mp/{map}");
        let callbacks = "maps/mp/gametypes/_callbacksetup";
        let mut roots = vec![
            "codescripts/delete".to_owned(),
            "codescripts/struct".to_owned(),
            callbacks.to_owned(),
            gametype.clone(),
        ];
        let mut entries = vec![format!("{gametype}::main")];
        if resolver.read_bytes(&map).is_ok() {
            entries.push(format!("{map}::main"));
            roots.push(map);
        }
        entries.push(format!("{callbacks}::codecallback_startgametype"));
        Self { roots, entries }
    }
}
