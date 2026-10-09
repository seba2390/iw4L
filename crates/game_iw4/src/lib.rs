mod catalog;
mod startup;

use game_api::{HudRules, ModeRules, Rule, ScriptProgram, ScriptRequest};

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
        &[("sv_maxclients", "18")]
    }

    fn config_defaults(&self) -> &'static [(&'static str, &'static str)] {
        &[("onlinegame", "1")]
    }
}

/// Modern Warfare 2's modes: a prematch countdown, deaths, code-drawn HUD.
const MODE: ModeRules = ModeRules {
    play_starts_on: "prematch_over",
    every_player_downs: Rule::Known(false),
    spawn_at_default_health: false,
    connect_team: None,
    scripts_spawn_players: false,
    spawn_classnames: None,
    unlimited: false,
    zombie_zone_scripts: false,
    report_builtin_gaps: false,
    waits_for_lobby: false,
    binds_account: true,
    binds_objectives: true,
    hud: HudRules {
        game_hud_menus: false,
        scoreboard: true,
        scorebar: true,
        compass: true,
        script_text: true,
        material_font_floor: true,
    },
};

impl game_api::GameModes for Iw4 {
    fn mode(&self, _gametype: &str) -> Rule<ModeRules> {
        Rule::Known(MODE)
    }

    fn zombies(&self) -> Option<&'static game_api::LibraryMode> {
        None
    }
}
