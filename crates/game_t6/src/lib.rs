mod startup;

use game_api::{HudRules, ModeRules, Rule, ScriptProgram, ScriptRequest, unknown};

pub struct T6;

pub static GAME: T6 = T6;

impl game_api::GameScripts for T6 {
    fn program(
        &self,
        request: &ScriptRequest<'_>,
        _sources: &dyn gsc::SourceResolver,
    ) -> Rule<ScriptProgram> {
        let zombies_map = request.map.starts_with("zm_");
        match (zombies_map, request.gametype) {
            (false, "dm" | "war") | (true, "zclassic") => {}
            (true, _) => {
                return Rule::Unknown(unknown!(
                    "t6.scripts.zm_gametypes",
                    "Black Ops 2 zombies maps run only Classic (zclassic); Grief, Turned and the TranZit locations have no rules",
                    "Black Ops 2's zombies submode and location rules"
                ));
            }
            (false, "zclassic") => {
                return Rule::Unknown(unknown!(
                    "t6.scripts.zclassic_on_mp",
                    "Classic zombies needs a Black Ops 2 zombies map",
                    "a zombies map (zm_*)"
                ));
            }
            (false, _) => {
                return Rule::Unknown(unknown!(
                    "t6.scripts.gametypes",
                    "Black Ops 2 multiplayer modes other than Free for All and Team Deathmatch have no rules",
                    "Black Ops 2's rules for that mode"
                ));
            }
        }
        let startup = startup::T6Startup::new(request.map);
        Rule::Known(ScriptProgram {
            catalog: gsc::Catalog::from_list(asset_core::FamilyId::T6, &[]),
            roots: startup.roots,
            entries: startup.entries,
        })
    }

    fn engine_dvars(&self, _gametype: &str) -> &'static [(&'static str, &'static str)] {
        &[]
    }
}

const MULTIPLAYER_MODE: ModeRules = ModeRules {
    play_starts_on: "prematch_over",
    every_player_downs: false,
    spawn_at_default_health: false,
    connect_team: None,
    scripts_spawn_players: false,
    spawn_classnames: None,
    unlimited: false,
    zombie_zone_scripts: false,
    report_builtin_gaps: false,
    waits_for_lobby: false,
    binds_account: false,
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

const ZCLASSIC_MODE: ModeRules = ModeRules {
    spawn_classnames: Some(&["initial_spawn_points", "info_player_start"]),
    unlimited: true,
    ..MULTIPLAYER_MODE
};

impl game_api::GameModes for T6 {
    fn mode(&self, gametype: &str) -> Rule<ModeRules> {
        match gametype {
            "dm" | "war" => Rule::Known(MULTIPLAYER_MODE),
            "zclassic" => Rule::Known(ZCLASSIC_MODE),
            _ => Rule::Unknown(unknown!(
                "t6.scripts.gametypes",
                "Black Ops 2 multiplayer modes other than Free for All and Team Deathmatch have no rules",
                "Black Ops 2's rules for that mode"
            )),
        }
    }
}
