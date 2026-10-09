mod catalog;
mod zombie_catalog;
mod zombie_startup;

use game_api::{Rule, ScriptProgram, ScriptRequest};

pub struct T5;

pub static GAME: T5 = T5;

/// Engine dvars the zombie scripts read without setting: the mode itself and
/// the AI locomotion tuning (run-weight updates each server frame, no lean or
/// turn slowdown).
const ZOMBIE_ENGINE_DVARS: &[(&str, &str)] = &[
    ("zombiemode", "1"),
    ("ai_runAnimUpdateFrequency", "0.05"),
    ("ai_useLeanRunAnimations", "0"),
    ("ai_slowdownRateBlendFactor", "1"),
    ("ai_slowdownMinRate", "1"),
    ("ai_slowdownMinYawDiff", "180"),
    ("ai_slowdownMaxYawDiff", "180"),
    ("ai_meleeRange", "64"),
];

impl game_api::GameScripts for T5 {
    fn program(
        &self,
        request: &ScriptRequest<'_>,
        sources: &dyn gsc::SourceResolver,
    ) -> Rule<ScriptProgram> {
        let startup = zombie_startup::T5ZombieStartup::new(sources, request.map, request.entities);
        Rule::Known(ScriptProgram {
            catalog: gsc::Catalog::from_list(asset_core::FamilyId::T5, catalog::T5)
                .extended(zombie_catalog::T5_ZOMBIE.iter().cloned()),
            roots: startup.roots,
            entries: startup.entries,
        })
    }

    fn engine_dvars(&self) -> &'static [(&'static str, &'static str)] {
        ZOMBIE_ENGINE_DVARS
    }
}
