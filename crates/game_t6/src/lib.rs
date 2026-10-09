use game_api::{Rule, ScriptProgram, ScriptRequest, unknown};

pub struct T6;

pub static GAME: T6 = T6;

impl game_api::GameScripts for T6 {
    fn program(
        &self,
        _request: &ScriptRequest<'_>,
        _sources: &dyn gsc::SourceResolver,
    ) -> Rule<ScriptProgram> {
        Rule::Unknown(unknown!(
            "t6.scripts.gametypes",
            "Black Ops 2 gametype scripts need Black Ops 2's builtin catalog, natives and match flow",
            "Black Ops 2's builtin list and the natives behind it"
        ))
    }

    fn engine_dvars(&self, _gametype: &str) -> &'static [(&'static str, &'static str)] {
        &[]
    }
}
