/// The zombie mode starts from the map's own main, which sets up `_zombiemode`;
/// the code callbacks live in the singleplayer `_callbacksetup`.
pub struct T5ZombieStartup {
    pub roots: Vec<String>,
    pub entries: Vec<String>,
}
impl T5ZombieStartup {
    pub fn new(map: &str) -> Self {
        let map = format!("maps/{map}");
        let callbacks = "maps/_callbacksetup";
        Self {
            roots: vec![
                "codescripts/delete".to_owned(),
                "codescripts/struct".to_owned(),
                callbacks.to_owned(),
                map.clone(),
            ],
            entries: vec![
                format!("{map}::main"),
                format!("{callbacks}::codecallback_startgametype"),
            ],
        }
    }
}
