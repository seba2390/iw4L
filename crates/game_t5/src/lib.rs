mod catalog;
mod zombie_catalog;
mod zombie_startup;

use game_api::{HudRules, LibraryMode, ModeRules, Rule, ScriptProgram, ScriptRequest, unknown};

pub struct T5;

pub static GAME: T5 = T5;

const ZOMBIES: &str = "zom";

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
        if request.gametype != ZOMBIES {
            return Rule::Unknown(unknown!(
                "t5.scripts.mp_gametypes",
                "Black Ops multiplayer gametypes: their scripts need Black Ops' multiplayer natives and match flow",
                "Black Ops' multiplayer builtins bound to Black Ops natives, and Black Ops' match phases"
            ));
        }
        let startup = zombie_startup::T5ZombieStartup::new(sources, request.map, request.entities);
        Rule::Known(ScriptProgram {
            catalog: gsc::Catalog::from_list(asset_core::FamilyId::T5, catalog::T5)
                .extended(zombie_catalog::T5_ZOMBIE.iter().cloned()),
            roots: startup.roots,
            entries: startup.entries,
        })
    }

    fn engine_dvars(&self, gametype: &str) -> &'static [(&'static str, &'static str)] {
        if gametype == ZOMBIES {
            ZOMBIE_ENGINE_DVARS
        } else {
            &[]
        }
    }
}

const ZOMBIE_MODE: ModeRules = ModeRules {
    play_starts_on: "all_players_connected",
    every_player_downs: Rule::Unknown(unknown!(
        "t5.match.last_stand",
        "when a dying Black Ops zombies player goes into last stand, and how a downed player moves and sees",
        "Black Ops' down rule from its executable or a reference capture (laststand dvars, callback order)"
    )),
    spawn_at_default_health: true,
    connect_team: Some("allies"),
    scripts_spawn_players: true,
    spawn_classnames: None,
    unlimited: false,
    zombie_zone_scripts: true,
    report_builtin_gaps: true,
    waits_for_lobby: true,
    binds_account: false,
    binds_objectives: false,
    hud: HudRules {
        game_hud_menus: true,
        scoreboard: false,
        scorebar: false,
        compass: false,
        script_text: false,
        material_font_floor: false,
    },
};

impl game_api::GameModes for T5 {
    fn mode(&self, gametype: &str) -> Rule<ModeRules> {
        if gametype != ZOMBIES {
            return Rule::Unknown(unknown!(
                "t5.scripts.mp_gametypes",
                "Black Ops multiplayer gametypes: their scripts need Black Ops' multiplayer natives and match flow",
                "Black Ops' multiplayer builtins bound to Black Ops natives, and Black Ops' match phases"
            ));
        }
        Rule::Known(ZOMBIE_MODE)
    }

    fn zombies(&self) -> Option<&'static LibraryMode> {
        Some(&ZOMBIES_LIBRARY)
    }
}

static ZOMBIES_LIBRARY: LibraryMode = LibraryMode {
    gametype: ZOMBIES,
    note: "Runs the map's own zombie scripts. Kino der Toten is the map played so far; gaps are listed in docs/fidelity/t5.md.",
    maps: &[
        ("zombie_theater", "KINO DER TOTEN"),
        ("zombie_pentagon", "\"FIVE\""),
        ("zombie_cod5_prototype", "NACHT DER UNTOTEN"),
        ("zombie_cod5_asylum", "VERRÜCKT"),
        ("zombie_cod5_sumpf", "SHI NO NUMA"),
        ("zombie_cod5_factory", "DER RIESE"),
        ("zombie_cosmodrome", "ASCENSION"),
        ("zombie_coast", "CALL OF THE DEAD"),
        ("zombie_temple", "SHANGRI-LA"),
        ("zombie_moon", "MOON"),
    ],
};
