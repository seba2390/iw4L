use game_api::{ModeRules, Rule, ScriptProgram, ScriptRequest, unknown};

pub struct T7;

pub static GAME: T7 = T7;

impl game_api::GameScripts for T7 {
    fn program(
        &self,
        _request: &ScriptRequest<'_>,
        _sources: &dyn gsc::SourceResolver,
    ) -> Rule<ScriptProgram> {
        Rule::Unknown(unknown!(
            "t7.scripts.compiled",
            "Black Ops 3 ships its scripts compiled; IW4L has no loader or VM for them",
            "Black Ops 3's compiled script format, a VM for it, and its builtins"
        ))
    }

    fn engine_dvars(&self, _gametype: &str) -> &'static [(&'static str, &'static str)] {
        &[]
    }

    fn config_defaults(&self) -> &'static [(&'static str, &'static str)] {
        &[]
    }
}

impl game_api::GameVision for T7 {
    fn shellshock(&self) -> Rule<()> {
        Rule::Unknown(unknown!(
            "t7.vision.shellshock",
            "Black Ops 3's shellshock files and how its screen draws a shellshock",
            "Black Ops 3's shellshock format and screen rules"
        ))
    }
}

impl game_api::GameMenus for T7 {
    fn font(&self, _font_enum: i32, _placement_scale: f32, _text_scale: f32) -> Rule<&'static str> {
        Rule::Unknown(unknown!(
            "t7.hud.menu_font",
            "the font a Black Ops 3 menu names",
            "Black Ops 3's menu font table"
        ))
    }

    fn layout(&self) -> Rule<&'static dyn game_api::MenuLayout> {
        Rule::Unknown(unknown!(
            "t7.hud.menu_layout",
            "how Black Ops 3's menus place an item and scale its text",
            "Black Ops 3's menu rules"
        ))
    }
}

impl game_api::GameModes for T7 {
    fn mode(&self, _gametype: &str) -> Rule<ModeRules> {
        Rule::Unknown(unknown!(
            "t7.scripts.gametypes",
            "Black Ops 3's match flow lives in its compiled gametype scripts, which IW4L does not run yet",
            "Black Ops 3's compiled script loader, VM and builtins"
        ))
    }

    fn zombies(&self) -> Option<&'static game_api::LibraryMode> {
        None
    }
}
