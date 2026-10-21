mod key_types;
mod startup;

use game_api::{HudRules, LibraryMode, ModeRules, Rule, ScriptProgram, ScriptRequest, unknown};

pub struct T7;

pub static GAME: T7 = T7;

const ZOMBIES: &str = "zclassic";

/// The module that holds the code callbacks the engine calls.
const CALLBACKS: &str = "scripts/shared/callbacks_shared";

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
        // Black Ops 3's executable looks up its code callbacks by hash; the
        // order is read from what the scripts need: the systems registered by
        // `autoexec` are set up in pre-initialization before the mains use
        // them, and finalize-initialization runs the post-systems after.
        let mut entries = built.autoexec;
        entries.push(format!("{CALLBACKS}::codecallback_preinitialization"));
        entries.push(format!("{gametype}::main"));
        entries.push(format!("{map}::main"));
        entries.push(format!("{CALLBACKS}::codecallback_finalizeinitialization"));
        entries.push(format!("{CALLBACKS}::codecallback_startgametype"));
        let settings = gametype_settings(request.config, ZOMBIES);
        diag::info!(
            Sim,
            "t7 gametype settings: {} for {ZOMBIES}",
            settings.len()
        );
        Rule::Known(ScriptProgram {
            catalog: gsc::Catalog::from_list(asset_core::FamilyId::T7, &[]),
            roots,
            entries,
            built: Some(built.program),
            settings,
            key_types: entity_key_types(request.entities),
            entity_fields: Vec::new(),
            dvars: Vec::new(),
        })
    }

    fn engine_dvars(&self, _gametype: &str) -> &'static [(&'static str, &'static str)] {
        &[]
    }

    fn config_defaults(&self) -> &'static [(&'static str, &'static str)] {
        &[]
    }
}

/// The types of the keys the map's entities use, as Black Ops 3's
/// executable lists them by the key's hash.
fn entity_key_types(entities: &str) -> Vec<(String, game_api::KeyType)> {
    let mut keys = std::collections::BTreeSet::new();
    for line in entities.lines() {
        let line = line.trim_start();
        if let Some(rest) = line.strip_prefix('"')
            && let Some(end) = rest.find('"')
        {
            keys.insert(rest[..end].to_ascii_lowercase());
        }
    }
    keys.into_iter()
        .filter_map(|key| {
            let hash = gsc_t7::hash(&key);
            key_types::KEY_TYPES
                .binary_search_by_key(&hash, |&(known, _)| known)
                .ok()
                .map(|index| (key, key_types::KEY_TYPES[index].1))
        })
        .collect()
}

/// A zombies gametype's settings: `gamedata/gamesettings/zm/
/// gamesettings_default.cfg`, then the gametype's own file over it. Each line
/// is `gametype_setting <name> <value>`, `//` starting a comment.
fn gametype_settings<'a>(
    config: &dyn Fn(&str) -> Option<&'a str>,
    gametype: &str,
) -> Vec<(String, String)> {
    let mut settings: Vec<(String, String)> = Vec::new();
    for file in ["default", gametype] {
        let Some(text) = config(&format!("gamedata/gamesettings/zm/gamesettings_{file}.cfg"))
        else {
            continue;
        };
        for line in text.lines() {
            let line = line.split("//").next().unwrap_or("");
            let mut words = line.split_whitespace();
            let (Some("gametype_setting"), Some(name), Some(value)) =
                (words.next(), words.next(), words.next())
            else {
                continue;
            };
            let name = name.to_ascii_lowercase();
            match settings.iter_mut().find(|(set, _)| *set == name) {
                Some((_, set)) => value.clone_into(set),
                None => settings.push((name, value.to_owned())),
            }
        }
    }
    settings
}

impl game_api::GameVision for T7 {
    fn shellshock(&self) -> Rule<game_api::ShockFormat> {
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

    fn ingame_menu(&self) -> Rule<game_api::InGameMenu> {
        Rule::Unknown(unknown!(
            "t7.hud.ingame_menu",
            "which menu Black Ops 3's in-game menu key opens",
            "Black Ops 3's in-game menu rule"
        ))
    }
}

/// Classic zombies: the match is run by the gametype and map scripts; the
/// engine imposes no limit and spawns no one on its own.
const ZOMBIES_MODE: ModeRules = ModeRules {
    play_starts_on: Some("all_players_connected"),
    every_player_downs: Rule::Unknown(unknown!(
        "t7.match.last_stand",
        "when a dying Black Ops 3 zombies player goes into last stand, and how a downed player moves and sees",
        "Black Ops 3's down rule"
    )),
    downed: game_api::DownedRules::BRIEFLY_IMMUNE,
    level_failed: None,
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
    bullet_mask: None,
    melee_mask: None,
    missiles: None,
    spawn_health: game_api::SpawnHealth::Default,
    aitype_spawners: false,
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
    host_rules: false,
    hud: HudRules {
        code_hud: Rule::Unknown(unknown!(
            "t7.hud.code_hud",
            "Black Ops 3's code-drawn HUD",
            "Black Ops 3's HUD rules"
        )),
        game_hud_menus: false,
        script_hud: false,
        scoreboard: false,
        scorebar: false,
        compass: false,
        script_text: game_api::ScriptText::Off,
        material_font_floor: false,
        zombies: true,
        font_scales_sent: None,
    },
    client_fire: None,
    top_down: None,
    viewmodel: None,
    script_sounds_hashed: false,
    turrets: None,
    post_chain: None,
    overlays: None,
    frame_fog: None,
    dyn_ent_physics: None,
    ammo_numbers: None,
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
