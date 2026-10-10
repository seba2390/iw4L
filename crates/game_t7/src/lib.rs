mod startup;

use game_api::{HudRules, LibraryMode, ModeRules, Rule, ScriptProgram, ScriptRequest, unknown};

pub struct T7;

pub static GAME: T7 = T7;

const ZOMBIES: &str = "zclassic";

impl game_api::GameScripts for T7 {
    fn program(
        &self,
        request: &ScriptRequest<'_>,
        sources: &dyn gsc::SourceResolver,
    ) -> Rule<ScriptProgram> {
        if request.gametype != ZOMBIES || !request.map.starts_with("zm_") {
            return Rule::Unknown(unknown!(
                "t7.scripts.gametypes",
                "Black Ops 3 modes other than Classic zombies on a zombies map",
                "Black Ops 3's rules for that mode"
            ));
        }
        let gametype = format!("scripts/zm/gametypes/{ZOMBIES}");
        let map = format!("scripts/zm/{}", request.map);
        let roots = vec![gametype.clone(), map.clone()];
        let modules = startup::load(sources, &roots);
        let built = gsc_t7::build(&modules, asset_core::FamilyId::T7);
        for line in &built.report {
            diag::info!(Sim, "{line}");
        }
        let mut entries = built.autoexec;
        entries.push(format!("{gametype}::main"));
        entries.push(format!("{map}::main"));
        Rule::Known(ScriptProgram {
            catalog: gsc::Catalog::from_list(asset_core::FamilyId::T7, &[]),
            roots,
            entries,
            built: Some(built.program),
        })
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

/// Classic zombies: the match is run by the gametype and map scripts; the
/// engine imposes no limit and spawns no one on its own.
const ZOMBIES_MODE: ModeRules = ModeRules {
    play_starts_on: "all_players_connected",
    every_player_downs: Rule::Unknown(unknown!(
        "t7.match.last_stand",
        "when a dying Black Ops 3 zombies player goes into last stand, and how a downed player moves and sees",
        "Black Ops 3's down rule"
    )),
    movement: Rule::Unknown(unknown!(
        "t7.movement.player",
        "Black Ops 3's player movement: look, walk, sprint, jump, slide, stances, gravity, collision",
        "Black Ops 3's player movement rules"
    )),
    weapons: Rule::Unknown(unknown!(
        "t7.weapons.state_machine",
        "Black Ops 3's weapon state machine: fire, reload, switch, ADS, melee, offhands",
        "Black Ops 3's weapon rules"
    )),
    spawn_at_default_health: true,
    connect_team: Some("allies"),
    scripts_spawn_players: true,
    spawn_classnames: None,
    unlimited: Rule::Known(true),
    limits_from_config: false,
    default_score_limit: None,
    zombie_zone_scripts: false,
    report_builtin_gaps: true,
    waits_for_lobby: true,
    binds_account: false,
    binds_objectives: false,
    hud: HudRules {
        code_hud: Rule::Unknown(unknown!(
            "t7.hud.code_hud",
            "Black Ops 3's code-drawn HUD",
            "Black Ops 3's HUD rules"
        )),
        game_hud_menus: false,
        scoreboard: false,
        scorebar: false,
        compass: false,
        script_text: false,
        material_font_floor: false,
    },
};

impl game_api::GameModes for T7 {
    fn mode(&self, gametype: &str) -> Rule<ModeRules> {
        if gametype == ZOMBIES {
            return Rule::Known(ZOMBIES_MODE);
        }
        Rule::Unknown(unknown!(
            "t7.scripts.gametypes",
            "Black Ops 3 modes other than Classic zombies on a zombies map",
            "Black Ops 3's rules for that mode"
        ))
    }

    fn zombies(&self) -> Option<&'static game_api::LibraryMode> {
        Some(&ZOMBIES_LIBRARY)
    }
}

static ZOMBIES_LIBRARY: LibraryMode = LibraryMode {
    gametype: "zclassic",
    note: "Runs the map's own compiled scripts; the map's world, weapons and movement are not read yet (docs/fidelity/t7.md).",
    maps: &[("zm_zod", "SHADOWS OF EVIL")],
};
