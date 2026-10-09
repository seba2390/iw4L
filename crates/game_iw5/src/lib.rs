use game_api::{ModeRules, Rule, ScriptProgram, ScriptRequest, unknown};

pub struct Iw5;

pub static GAME: Iw5 = Iw5;

impl game_api::GameScripts for Iw5 {
    fn program(
        &self,
        _request: &ScriptRequest<'_>,
        _sources: &dyn gsc::SourceResolver,
    ) -> Rule<ScriptProgram> {
        Rule::Unknown(unknown!(
            "iw5.scripts.gametypes",
            "Modern Warfare 3 gametype scripts need Modern Warfare 3's builtin catalog, natives and match flow",
            "Modern Warfare 3's builtin list and the natives behind it"
        ))
    }

    fn engine_dvars(&self, _gametype: &str) -> &'static [(&'static str, &'static str)] {
        &[]
    }
}

impl game_api::GameModes for Iw5 {
    fn mode(&self, _gametype: &str) -> Rule<ModeRules> {
        Rule::Unknown(unknown!(
            "iw5.scripts.gametypes",
            "Modern Warfare 3 gametype scripts need Modern Warfare 3's builtin catalog, natives and match flow",
            "Modern Warfare 3's builtin list and the natives behind it"
        ))
    }

    fn zombies(&self) -> Option<&'static game_api::LibraryMode> {
        None
    }
}
