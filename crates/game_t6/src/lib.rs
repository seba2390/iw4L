mod builtins;
mod client;
mod client_builtins;
pub mod client_screen;
mod fire;
mod fire_sound;
pub mod fog;
mod grenade;
mod keys;
mod menu_expression;
mod missile;
mod movement;
mod overlay;
mod script_zones;
pub mod scrplace;
mod startup;
pub mod ui;
mod viewmodel;

use game_api::{
    HudRules, LibraryMode, ModeRules, MovementRules, Rule, ScriptProgram, ScriptRequest,
    WeaponRules, unknown,
};

pub struct T6;

pub static GAME: T6 = T6;

pub use menu_expression::parse_menu_expression;
pub use script_zones::SCRIPT_ZONE_FORMAT;

impl game_api::GameScripts for T6 {
    fn program(
        &self,
        request: &ScriptRequest<'_>,
        sources: &dyn gsc::SourceResolver,
    ) -> Rule<ScriptProgram> {
        let zombies_map = request.map.starts_with("zm_");
        if request.gametype == "zclassic" && own_scripts(request.map) {
            return own_zombies(request, sources);
        }
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
            catalog: gsc::Catalog::from_list(asset_core::FamilyId::T6, &catalog())
                .with_ordered_strings(),
            roots: startup.roots,
            entries: startup.entries,
            built: None,
            settings: Vec::new(),
            key_types: Vec::new(),
            entity_fields: Vec::new(),
            dvars: Vec::new(),
        })
    }

    fn compiled_client_program(
        &self,
        request: &ScriptRequest<'_>,
        sources: &dyn gsc::SourceResolver,
    ) -> Option<Rule<game_api::ClientScriptProgram>> {
        (request.gametype == "zclassic" && own_scripts(request.map))
            .then(|| client::program(request, sources))
    }

    fn engine_dvars(&self, _gametype: &str) -> &'static [(&'static str, &'static str)] {
        // The client game registers it on; server scripts then leave
        // createfx effects and sounds to client scripts. The engine
        // registers the turret placement dvar on (`_zm_equipment` reads it),
        // and the actor corpse slots at their most (`ai_corpseCount`).
        &[
            ("cg_usingClientScripts", "1"),
            ("tu11_zombie_turret_placement_ignores_bodies", "1"),
            ("ai_corpseCount", "8"),
        ]
    }

    fn config_defaults(&self) -> &'static [(&'static str, &'static str)] {
        &[]
    }
}

/// Black Ops 2's own zombies scripts run Classic on every zombies map but
/// Origins, which keeps IW4L's rules (docs/fidelity/t6.md).
fn own_scripts(map: &str) -> bool {
    map.starts_with("zm_") && map != ORIGINS
}

/// The map whose Classic still runs IW4L's rules.
const ORIGINS: &str = "zm_tomb";

fn own_zombies(
    request: &ScriptRequest<'_>,
    sources: &dyn gsc::SourceResolver,
) -> Rule<ScriptProgram> {
    let Some(start) = startup::ZombiesStart::of(request.map, request.table) else {
        return Rule::Unknown(unknown!(
            "t6.scripts.zm_start",
            "where a Black Ops 2 zombies map starts: its default row in zm/gametypestable.csv",
            "the map's start location and gametype"
        ));
    };
    let map = format!("maps/mp/{}", request.map);
    // The scripts the engine loads itself, then the gametype's and the map's.
    let roots = [
        "codescripts/delete".to_owned(),
        "codescripts/struct".to_owned(),
        "maps/mp/gametypes_zm/_callbacksetup".to_owned(),
        "maps/mp/_medals".to_owned(),
        "maps/mp/gametypes_zm/_rank".to_owned(),
        "maps/mp/gametypes_zm/_persistence".to_owned(),
        "maps/mp/gametypes_zm/_spawning".to_owned(),
        "maps/mp/zombies/_zm".to_owned(),
        format!("maps/mp/gametypes_zm/{}", start.gametype),
        map,
    ]
    .into_iter()
    .chain(startup::actor_scripts(request.entities, request.traversals))
    .collect::<Vec<_>>();
    let read = |name: &str| {
        sources.read_module(name).ok().map(|bytes| gsc_t6::Source {
            name: name.to_owned(),
            bytes,
            origin: sources.origin(name),
        })
    };
    let (modules, missing) = gsc_t6::load(&read, &roots);
    let builtins = gsc_t6::Builtins {
        functions: &builtins::FUNCTIONS,
        methods: &builtins::METHODS,
    };
    let built = gsc_t6::build(&modules, &builtins, asset_core::FamilyId::T6);
    for line in missing.iter().chain(&built.report) {
        diag::info!(Sim, "{line}");
    }
    if !built.unresolved.is_empty() {
        return Rule::Unknown(unknown!(
            "t6.scripts.ffotd",
            "Black Ops 2's zombies scripts call into modules the install does not carry (maps/mp/gametypes_zm/_globallogic is in the patch zone the game downloads at launch)",
            "the downloaded patch zone, captured into ~/.config/iw4l/captured/ (or <artifacts>/captured/)"
        ));
    }
    let entries = start.entries(request.map, &built.program);
    let dvars = vec![
        ("g_gametype".to_owned(), start.gametype.clone()),
        ("ui_gametype".to_owned(), start.gametype.clone()),
        ("ui_zm_mapstartlocation".to_owned(), start.location.clone()),
        ("ui_zm_gamemodegroup".to_owned(), start.group.clone()),
    ];
    Rule::Known(ScriptProgram {
        catalog: gsc::Catalog::from_list(asset_core::FamilyId::T6, &catalog())
            .with_ordered_strings(),
        roots,
        entries,
        built: Some(built.program),
        settings: gametype_settings(request.config, &start.gametype),
        key_types: (request.config)("radiant/keys.txt").map_or_else(Vec::new, keys::parse),
        entity_fields: keys::ENTITY_FIELDS
            .iter()
            .map(|&(name, ty)| (name.to_owned(), ty))
            .collect(),
        dvars,
    })
}

/// A zombies gametype's settings: `zm/gamesettings_default.cfg`, then the
/// gametype's own file over it; each line is `gametype_setting <name> <value>`.
pub(crate) fn gametype_settings<'a>(
    config: &dyn Fn(&str) -> Option<&'a str>,
    gametype: &str,
) -> Vec<(String, String)> {
    let mut settings: Vec<(String, String)> = Vec::new();
    for file in ["default", gametype] {
        let Some(text) = config(&format!("zm/gamesettings_{file}.cfg")) else {
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

/// Black Ops 2's builtins, the first of each name as its loader finds them.
fn catalog() -> Vec<gsc::Builtin> {
    let list = |namespace, specs: &'static [gsc_t6::BuiltinSpec]| {
        specs.iter().map(move |spec| {
            gsc::Builtin::new(namespace, spec.name, gsc::Owner::Script, spec.developer)
        })
    };
    list(gsc::Namespace::Function, &builtins::FUNCTIONS)
        .chain(list(gsc::Namespace::Method, &builtins::METHODS))
        .collect()
}

/// IW4L's Free for All rules for Black Ops 2 (docs/fidelity/t6.md).
const FREE_FOR_ALL_MODE: ModeRules = ModeRules {
    default_score_limit: Some(30),
    ..MULTIPLAYER_MODE
};

/// IW4L's Team Deathmatch rules for Black Ops 2 (docs/fidelity/t6.md).
const TEAM_DEATHMATCH_MODE: ModeRules = ModeRules {
    default_score_limit: Some(75),
    ..MULTIPLAYER_MODE
};

const MULTIPLAYER_MODE: ModeRules = ModeRules {
    play_starts_on: "prematch_over",
    every_player_downs: Rule::Known(false),
    downed: game_api::DownedRules::BRIEFLY_IMMUNE,
    level_failed: None,
    movement: Rule::Known(MovementRules::Game(&movement::MULTIPLAYER)),
    weapons: Rule::Known(WeaponRules::Game),
    // The mask the engine's bullet trace takes; actors are in it.
    bullet_mask: Some(0x0280_e833),
    melee_mask: Some(0x0280_e893),
    missiles: Some(game_api::fire::MissileRules(&missile::MISSILES)),
    spawn_at_default_health: false,
    connect_team: None,
    scripts_spawn_players: false,
    spawn_classnames: None,
    unlimited: Rule::Known(false),
    limits_from_config: true,
    default_score_limit: None,
    zombie_zone_scripts: false,
    report_builtin_gaps: false,
    waits_for_lobby: false,
    binds_account: false,
    binds_objectives: true,
    host_rules: true,
    hud: HudRules {
        code_hud: Rule::Unknown(unknown!(
            "t6.hud.code_hud",
            "Black Ops 2's code-drawn HUD; IW4L's own Black Ops 2 HUD (ui/src/t6_hud.rs) draws instead",
            "Black Ops 2's HUD rules"
        )),
        game_hud_menus: false,
        script_hud: false,
        scoreboard: true,
        scorebar: true,
        compass: true,
        script_text: game_api::ScriptText::Engine,
        material_font_floor: true,
        zombies: false,
        font_scales_sent: None,
    },
    client_fire: Some(fire_sound::CLIENT_FIRE),
    viewmodel: Some(viewmodel::VIEWMODEL),
    // gsc_playsoundatposition sends the alias's hash; nothing checks it.
    script_sounds_hashed: true,
    turrets: None,
    post_chain: None,
    overlays: Some(overlay::OVERLAYS),
    frame_fog: Some(fog::FRAME_FOG),
    dyn_ent_physics: None,
};

const ZCLASSIC_MODE: ModeRules = ModeRules {
    spawn_classnames: Some(&["initial_spawn_points", "info_player_start"]),
    unlimited: Rule::Known(true),
    hud: HudRules {
        zombies: true,
        ..MULTIPLAYER_MODE.hud
    },
    movement: Rule::Known(MovementRules::Game(&movement::ZOMBIES)),
    ..MULTIPLAYER_MODE
};

/// Black Ops 2's own zombies scripts connect, spawn and score the players.
const OWN_ZOMBIES_MODE: ModeRules = ModeRules {
    connect_team: Some("allies"),
    scripts_spawn_players: true,
    spawn_classnames: None,
    limits_from_config: false,
    report_builtin_gaps: true,
    binds_objectives: false,
    host_rules: false,
    ..ZCLASSIC_MODE
};

impl game_api::GameVision for T6 {
    fn shellshock(&self) -> Rule<game_api::ShockFormat> {
        Rule::Unknown(unknown!(
            "t6.vision.shellshock",
            "Black Ops 2's shellshock files and how its screen draws a shellshock",
            "Black Ops 2's .shock format and shellshock screen rules"
        ))
    }
}

impl game_api::GameMenus for T6 {
    fn script_ui(
        &self,
        modules: &[game_api::ui::Module],
    ) -> Result<Option<Box<dyn game_api::ui::ScriptUi>>, String> {
        Ok(Some(Box::new(ui::scene::Scene::new(modules)?)))
    }

    fn front_end_ui(
        &self,
        modules: &[game_api::ui::Module],
    ) -> Result<Option<Box<dyn game_api::ui::ScriptUi>>, String> {
        Ok(Some(Box::new(ui::scene::Scene::front_end(modules)?)))
    }

    fn font(&self, font_enum: i32, placement_scale: f32, text_scale: f32) -> Rule<&'static str> {
        Rule::Known(scrplace::menu_font_at(
            font_enum,
            placement_scale * text_scale,
            0.563,
            0.688,
        ))
    }

    fn layout(&self) -> Rule<&'static dyn game_api::MenuLayout> {
        Rule::Known(&MenuLayout)
    }

    fn ingame_menu(&self) -> Rule<game_api::InGameMenu> {
        Rule::Known(game_api::InGameMenu::ScriptMainMenu)
    }
}

/// Black Ops 2's legacy menu placement and text measures (`UI_TextWidth`,
/// `UI_TextHeight`, `Item_Text_AutoWrapped_Paint`).
struct MenuLayout;

fn round_half_up(value: f32) -> f32 {
    (value + 0.5).floor()
}

impl game_api::MenuLayout for MenuLayout {
    fn text_scale(&self, pixel_height: i32, text_scale: f32) -> f32 {
        (f64::from(text_scale) * 48.0 / f64::from(pixel_height)) as f32
    }

    fn text_height(&self, pixel_height: i32, text_scale: f32) -> f32 {
        round_half_up(pixel_height as f32 * self.text_scale(pixel_height, text_scale))
    }

    fn text_width(&self, glyph_width: i32, pixel_height: i32, text_scale: f32) -> f32 {
        round_half_up(glyph_width as f32 * self.text_scale(pixel_height, text_scale))
    }

    fn text_origin(
        &self,
        [x, y, w, h]: [f32; 4],
        align_mode: i32,
        align_x: f32,
        align_y: f32,
        measured_w: f32,
        measured_h: f32,
    ) -> (f32, f32) {
        let mut offset_x = align_x;
        let horizontal = align_mode & 3;
        if horizontal != 0 {
            let mut room = w - measured_w;
            if horizontal == 1 {
                room *= 0.5;
            }
            offset_x = room + align_x;
        }
        let offset_y = match align_mode & 0xc {
            0 => align_y,
            4 => measured_h + align_y,
            8 => (h + measured_h) * 0.5 + align_y,
            _ => h + align_y,
        };
        (x + offset_x, y + offset_y)
    }

    fn text_paint_scale(&self, text_scale: f32, _menu_scale: f32) -> f32 {
        text_scale
    }

    fn window_rect(&self, rect: [f32; 4], _menu_scale: f32) -> [f32; 4] {
        rect
    }

    fn safe_area(&self) -> f32 {
        1.0
    }
}

impl game_api::GameModes for T6 {
    fn mode(&self, gametype: &str) -> Rule<ModeRules> {
        match gametype {
            "dm" => Rule::Known(FREE_FOR_ALL_MODE),
            "war" => Rule::Known(TEAM_DEATHMATCH_MODE),
            "zclassic" => Rule::Known(OWN_ZOMBIES_MODE),
            _ => Rule::Unknown(unknown!(
                "t6.scripts.gametypes",
                "Black Ops 2 multiplayer modes other than Free for All and Team Deathmatch have no rules",
                "Black Ops 2's rules for that mode"
            )),
        }
    }

    fn mode_on_map(&self, map: &str, gametype: &str) -> Rule<ModeRules> {
        match gametype {
            "zclassic" if map == ORIGINS => Rule::Known(ZCLASSIC_MODE),
            _ => self.mode(gametype),
        }
    }

    fn zombies(&self) -> Option<&'static LibraryMode> {
        Some(&ZOMBIES_LIBRARY)
    }
}

static ZOMBIES_LIBRARY: LibraryMode = LibraryMode {
    gametype: "zclassic",
    note: "Black Ops II's own zombies scripts; Origins runs survival rules written for IW4L (docs/fidelity/t6.md).",
    maps: &[
        ("zm_nuked", "NUKETOWN ZOMBIES"),
        ("zm_transit", "TRANZIT"),
        ("zm_highrise", "DIE RISE"),
        ("zm_prison", "MOB OF THE DEAD"),
        ("zm_buried", "BURIED"),
        ("zm_tomb", "ORIGINS"),
    ],
};
