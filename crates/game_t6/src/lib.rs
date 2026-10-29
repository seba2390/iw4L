mod automelee;
mod builtins;
mod client;
mod client_builtins;
pub mod client_screen;
mod fire;
mod fire_autosim;
mod fire_sound;
pub mod fog;
mod grenade;
mod keys;
mod menu_expression;
mod missile;
mod movement;
mod overlay;
mod player_damage;
mod revive;
mod script_zones;
pub mod scrplace;
mod shock;
mod startup;
pub mod ui;
mod view_kick;
mod viewmodel;

use game_api::{
    HudRules, LibraryMode, LibraryStart, ModeRules, MovementRules, Rule, ScriptProgram,
    ScriptRequest, WeaponRules, unknown,
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
        if own_gametype(request.gametype) && own_scripts(request.map) {
            return own_zombies(request, sources);
        }
        match (zombies_map, request.gametype) {
            (false, "dm" | "war") | (true, "zclassic") => {}
            (true, _) => {
                return Rule::Unknown(unknown!(
                    "t6.scripts.zm_gametypes",
                    "Black Ops 2's zombies gametypes are Classic, Survival, Grief and Turned (zclassic, zstandard, zgrief, zcleansed)",
                    "one of Black Ops 2's zombies gametypes"
                ));
            }
            (false, "zclassic" | "zstandard" | "zgrief" | "zcleansed") => {
                return Rule::Unknown(unknown!(
                    "t6.scripts.zclassic_on_mp",
                    "Black Ops 2's zombies gametypes need a zombies map",
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
        if own_gametype(request.gametype) && own_scripts(request.map) {
            return Some(client::program(request, sources));
        }
        // Black Ops 2's multiplayer client scripts run on the multiplayer
        // maps under the Free for All and Team Deathmatch that IW4L's rules
        // still host (docs/fidelity/t6-mp.md).
        (!request.map.starts_with("zm_") && matches!(request.gametype, "dm" | "war"))
            .then(|| client::multiplayer_program(request, sources))
    }

    fn engine_dvars(&self, _gametype: &str) -> &'static [(&'static str, &'static str)] {
        // The client game registers it on; server scripts then leave
        // createfx effects and sounds to client scripts. The engine
        // registers the turret placement dvar on (`_zm_equipment` reads it),
        // and the actor corpse slots as the machine-spec table
        // (`configure.csv`) sets them on any machine but a Core 2 or
        // Phenom: 5 (its catch-all row's 16 is past the dvar's most, 8).
        &[
            ("cg_usingClientScripts", "1"),
            ("tu11_zombie_turret_placement_ignores_bodies", "1"),
            ("ai_corpseCount", "5"),
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

/// The zombies gametypes Black Ops 2's own scripts run here: Classic,
/// Survival, Grief and Turned (the gametype rows of `zm/gametypestable.csv`).
fn own_gametype(gametype: &str) -> bool {
    matches!(gametype, "zclassic" | "zstandard" | "zgrief" | "zcleansed")
}

/// A multiplayer gametype's script name: the lobby's `war` is Black Ops 2's
/// Team Deathmatch, `tdm` (`maps/mp/gametypes/tdm`).
fn script_gametype(gametype: &str) -> &str {
    match gametype {
        "war" => "tdm",
        other => other,
    }
}

/// The map whose Classic still runs IW4L's rules.
const ORIGINS: &str = "zm_tomb";

fn own_zombies(
    request: &ScriptRequest<'_>,
    sources: &dyn gsc::SourceResolver,
) -> Rule<ScriptProgram> {
    let Some(start) = startup::ZombiesStart::of(
        request.map,
        request.gametype,
        request.location,
        request.table,
    ) else {
        return Rule::Unknown(unknown!(
            "t6.scripts.zm_start",
            "where a Black Ops 2 zombies map starts: its row in zm/gametypestable.csv for the gametype and location",
            "the map's start location and gametype"
        ));
    };
    let gametype = format!("maps/mp/gametypes_zm/{}", start.gametype);
    // A gametype whose own script the map's zones lack (Turned on TranZit's
    // Diner: only Buried's patch zone carries one here) has nothing to run.
    if sources.read_module(&gametype).is_err() {
        return Rule::Unknown(unknown!(
            "t6.scripts.zm_gametype_zone",
            "a Black Ops 2 zombies gametype runs its own script, which the map's zones must carry",
            "the zone carrying that gametype's script for this map"
        ));
    }
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
        gametype,
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
    play_starts_on: Some("prematch_over"),
    every_player_downs: Rule::Known(false),
    downed: game_api::DownedRules::BRIEFLY_IMMUNE,
    level_failed: None,
    movement: Rule::Known(MovementRules::Game(&movement::MULTIPLAYER)),
    weapons: Rule::Known(WeaponRules::Game),
    // The mask the engine's bullet trace takes; actors are in it.
    bullet_mask: Some(0x0280_e833),
    melee_mask: Some(0x0280_e893),
    missiles: Some(game_api::fire::MissileRules(&missile::MISSILES)),
    actor_physics: None,
    players_meet_actors: false,
    spawn_health: game_api::SpawnHealth::Stored,
    aitype_spawners: false,
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
    top_down: None,
    camera_shake: None,
    viewmodel: Some(viewmodel::VIEWMODEL),
    // gsc_playsoundatposition sends the alias's hash; nothing checks it.
    script_sounds_hashed: true,
    turrets: None,
    post_chain: None,
    overlays: Some(overlay::OVERLAYS),
    frame_fog: Some(fog::FRAME_FOG),
    dyn_ent_physics: None,
    ammo_numbers: None,
    extra_camera: None,
    menu_dvars_reach_server: false,
    ui3d: None,
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
    /// Black Ops 2's shellshock files, drawn by the shared screen's rules,
    /// which its client runs.
    fn shellshock(&self) -> Rule<game_api::ShockFormat> {
        Rule::Known(game_api::ShockFormat::Game {
            parse: shock::parse,
            fallback: shock::FALLBACK,
        })
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
            "zclassic" | "zstandard" | "zgrief" | "zcleansed" => Rule::Known(OWN_ZOMBIES_MODE),
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
    front_end: None,
    intro: None,
    maps: &[
        ("zm_nuked", "NUKETOWN ZOMBIES"),
        ("zm_transit", "TRANZIT"),
        ("zm_highrise", "DIE RISE"),
        ("zm_prison", "MOB OF THE DEAD"),
        ("zm_buried", "BURIED"),
        ("zm_tomb", "ORIGINS"),
    ],
    // Black Ops 2's own zombies scripts import its `_globallogic`, which only
    // the patch zone the game downloads at launch carries
    // (`t6.scripts.ffotd`); Origins runs IW4L's rules without it.
    capture: Some(game_api::LibraryCapture {
        suffix: "_zm",
        exempt: &[ORIGINS],
        note: "Black Ops II's zombies scripts need the patch zone the game downloads at launch (ffotd_*_zm*.ff), captured from your own running game. Put it in",
    }),
    // TranZit's Survival and Grief and Buried's Grief and Turned starts, as
    // `zm/gametypestable.csv` lists them (TranZit's Diner Turned needs a zone
    // the install lacks: `t6.scripts.zm_gametype_zone`).
    starts: &[
        LibraryStart {
            zone: "zm_transit",
            gametype: "zstandard",
            location: "transit",
            title: "BUS DEPOT (SURVIVAL)",
        },
        LibraryStart {
            zone: "zm_transit",
            gametype: "zstandard",
            location: "town",
            title: "TOWN (SURVIVAL)",
        },
        LibraryStart {
            zone: "zm_transit",
            gametype: "zstandard",
            location: "farm",
            title: "FARM (SURVIVAL)",
        },
        LibraryStart {
            zone: "zm_transit",
            gametype: "zgrief",
            location: "town",
            title: "TOWN (GRIEF)",
        },
        LibraryStart {
            zone: "zm_transit",
            gametype: "zgrief",
            location: "farm",
            title: "FARM (GRIEF)",
        },
        LibraryStart {
            zone: "zm_buried",
            gametype: "zgrief",
            location: "street",
            title: "BOROUGH (GRIEF)",
        },
        LibraryStart {
            zone: "zm_buried",
            gametype: "zcleansed",
            location: "street",
            title: "BOROUGH (TURNED)",
        },
    ],
};
