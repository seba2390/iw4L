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
            // The animscripts the executable loads for its dogs.
            "maps/mp/animscripts/dog_combat".to_owned(),
            "maps/mp/animscripts/dog_death".to_owned(),
            "maps/mp/animscripts/dog_init".to_owned(),
            "maps/mp/animscripts/dog_pain".to_owned(),
            "maps/mp/animscripts/dog_move".to_owned(),
            "maps/mp/animscripts/dog_stop".to_owned(),
            "maps/mp/animscripts/dog_flashed".to_owned(),
            "maps/mp/animscripts/dog_jump".to_owned(),
            "maps/mp/animscripts/dog_turn".to_owned(),
            // And the traversals of the map's nodes.
            "maps/mp/animscripts/traverse/jump_down_40".to_owned(),
            "maps/mp/animscripts/traverse/jump_down_56".to_owned(),
            "maps/mp/animscripts/traverse/jump_down_96".to_owned(),
            "maps/mp/animscripts/traverse/mantle_on_40".to_owned(),
            "maps/mp/animscripts/traverse/mantle_on_56".to_owned(),
            "maps/mp/animscripts/traverse/mantle_on_80".to_owned(),
            "maps/mp/animscripts/traverse/mantle_on_96".to_owned(),
            "maps/mp/animscripts/traverse/mantle_over_40".to_owned(),
            "maps/mp/animscripts/traverse/mantle_window_36".to_owned(),
            "maps/mp/animscripts/traverse/through_hole_42".to_owned(),
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
