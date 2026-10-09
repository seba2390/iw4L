//! The one place that names games: which game's rules run a match.

use asset_core::FamilyId;
use game_api::{GameScripts, Rule, ScriptProgram, ScriptRequest};

/// The scripts of the game the map belongs to.
pub(crate) fn scripts(family: FamilyId) -> &'static dyn GameScripts {
    match family {
        FamilyId::Iw4 => &game_iw4::GAME,
        FamilyId::T5 => &game_t5::GAME,
        FamilyId::Iw5 => &game_iw5::GAME,
        FamilyId::T6 => &T6_ON_IW4_BUILTINS,
    }
}

/// Black Ops 2's program compiled against Modern Warfare 2's builtin list.
struct T6OnIw4Builtins;

static T6_ON_IW4_BUILTINS: T6OnIw4Builtins = T6OnIw4Builtins;

impl GameScripts for T6OnIw4Builtins {
    fn program(
        &self,
        request: &ScriptRequest<'_>,
        sources: &dyn gsc::SourceResolver,
    ) -> Rule<ScriptProgram> {
        match game_t6::GAME.program(request, sources) {
            Rule::Known(mut program) => {
                program.catalog = program.catalog.extended(game_iw4::BUILTINS.iter().cloned());
                Rule::Known(program)
            }
            unknown => unknown,
        }
    }

    fn engine_dvars(&self, gametype: &str) -> &'static [(&'static str, &'static str)] {
        game_t6::GAME.engine_dvars(gametype)
    }
}
