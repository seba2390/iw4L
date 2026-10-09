mod catalog;
mod startup;

pub use catalog::IW4 as BUILTINS;

use game_api::{Rule, ScriptProgram, ScriptRequest};

pub struct Iw4;

pub static GAME: Iw4 = Iw4;

impl game_api::GameScripts for Iw4 {
    fn program(
        &self,
        request: &ScriptRequest<'_>,
        sources: &dyn gsc::SourceResolver,
    ) -> Rule<ScriptProgram> {
        let startup = startup::Iw4Startup::new(sources, request.gametype, request.map);
        Rule::Known(ScriptProgram {
            catalog: gsc::Catalog::from_list(asset_core::FamilyId::Iw4, catalog::IW4),
            roots: startup.roots,
            entries: startup.entries,
        })
    }

    fn engine_dvars(&self, _gametype: &str) -> &'static [(&'static str, &'static str)] {
        &[]
    }
}
