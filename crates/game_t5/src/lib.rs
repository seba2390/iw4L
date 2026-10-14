mod catalog;
mod fire;
mod fire_sound;
mod grenade;
mod menu_expression;
mod missile;
mod movement;
mod radius;
mod turret;
mod viewmodel;
mod zombie_catalog;
mod zombie_startup;

pub use menu_expression::parse_menu_expression;

use game_api::{
    DownedRules, FailedLevelRules, HudRules, LibraryMode, ModeRules, MovementRules, Rule,
    ScriptProgram, ScriptRequest, WeaponRules, unknown,
};

pub struct T5;

pub static GAME: T5 = T5;

const ZOMBIES: &str = "zom";

/// Engine dvars the zombie scripts read without setting: the mode itself and
/// the AI locomotion tuning (run-weight updates each server frame, no lean or
/// turn slowdown).
const ZOMBIE_ENGINE_DVARS: &[(&str, &str)] = &[
    ("zombiemode", "1"),
    // The zombies executable is the singleplayer one; menus test this to
    // hide the multiplayer rows.
    ("ui_multiplayer", "0"),
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
            built: None,
            settings: Vec::new(),
            key_types: Vec::new(),
            entity_fields: Vec::new(),
            dvars: Vec::new(),
        })
    }

    fn engine_dvars(&self, gametype: &str) -> &'static [(&'static str, &'static str)] {
        if gametype == ZOMBIES {
            ZOMBIE_ENGINE_DVARS
        } else {
            &[]
        }
    }

    fn config_defaults(&self) -> &'static [(&'static str, &'static str)] {
        &[]
    }
}

const ZOMBIE_MODE: ModeRules = ModeRules {
    play_starts_on: "all_players_connected",
    movement: Rule::Known(MovementRules::Game(&movement::ZOMBIES)),
    weapons: Rule::Known(WeaponRules::Game),
    // In zombie mode every lethal hit downs the player: the scripts decide
    // when a downed player dies.
    every_player_downs: Rule::Known(true),
    downed: DownedRules {
        immune_ms: 0,
        hurt_calls_back: true,
        sets_view_height: false,
        revive_restores_health: true,
        unrun: None,
    },
    // Solo game over: every client is told the level failed, and a wall-clock
    // second plus `g_deathDelay` (4000) later the level is loaded again.
    level_failed: Some(FailedLevelRules {
        client_dvar: ("hud_missionFailed", "1"),
        reload_delay_ms: 1000,
        death_delay_ms: 4000,
        blur_target: 32.0,
        deadquote_dvar: "ui_deadquote",
        death_sound: "death",
        unrun: Some(unknown!(
            "t5.match.level_failed",
            "a failed level loads afresh instead of restoring its level-start save, counts its delay on the match clock without ramping com_codeTimeScale from 1 to 0.1, and does not fade out the sound mixer over 2 s",
            "save games in the runtime (Black Ops saves the level at its start), and Black Ops' client timescale ramp and sound-queue fade"
        )),
    }),
    bullet_mask: None,
    melee_mask: None,
    missiles: None,
    spawn_at_default_health: true,
    connect_team: Some("allies"),
    scripts_spawn_players: true,
    spawn_classnames: None,
    unlimited: Rule::Known(true),
    limits_from_config: false,
    default_score_limit: None,
    zombie_zone_scripts: true,
    report_builtin_gaps: true,
    waits_for_lobby: true,
    binds_account: false,
    binds_objectives: false,
    host_rules: false,
    hud: HudRules {
        code_hud: Rule::Unknown(unknown!(
            "t5.hud.code_hud",
            "Black Ops zombies' code-drawn HUD: hud elem placement, blood, reticle, killfeed, names, hints, flash",
            "Black Ops' HUD rules from its executable, or reference shots of each piece"
        )),
        game_hud_menus: true,
        script_hud: true,
        scoreboard: false,
        scorebar: false,
        compass: false,
        script_text: game_api::ScriptText::MenuFonts(hud_elem_font),
        material_font_floor: false,
        zombies: true,
        font_scales_sent: Some(sent_font_scales),
    },
    client_fire: Some(fire_sound::CLIENT_FIRE),
    viewmodel: Some(viewmodel::VIEWMODEL),
    script_sounds_hashed: true,
    turrets: Some(turret::TURRETS),
};

/// Black Ops' hud elem fonts, read from its executable: the menu font each
/// draws with and its base scale. `bigfixed` draws bold at half scale,
/// `smallfixed` the console font at a third; every other font a quarter.
/// Black Ops has no `hudbig` or `hudsmall`; unknown fonts draw as `default`.
fn hud_elem_font(font: i32) -> (i32, f32) {
    match font {
        1 => (4, 0.5),
        2 => (5, 0.333_333_34),
        3 => (1, 0.25),
        4 => (2, 0.25),
        5 => (3, 0.25),
        8 => (6, 0.25),
        _ => (0, 0.25),
    }
}

/// Black Ops sends an elem's font scale in 6 bits as tenths above 1
/// (`(scale - 1) x 10`, rounded to nearest), keeping only the low bits, so a
/// script's 8 reaches the client as 1.6 and its 32 as 6.4. The from-font
/// scale is not sent, so clients keep it at 0.
fn sent_font_scales(font_scale: f32, _from_font_scale: f32) -> (f32, f32) {
    let tenths = (font_scale - 1.0) * 10.0;
    let rounded = (f64::from(tenths) + f64::from(2.0f32.powi(-30))).round_ties_even();
    let sent = if rounded.is_finite() && rounded.abs() < 2_147_483_648.0 {
        rounded as i32 & 0x3f
    } else {
        0
    };
    (sent as f32 * 0.1 + 1.0, 0.0)
}

impl game_api::GameVision for T5 {
    fn shellshock(&self) -> Rule<()> {
        Rule::Unknown(unknown!(
            "t5.vision.shellshock",
            "Black Ops' shellshock files and how its screen draws a shellshock",
            "Black Ops' .shock format and shellshock screen rules"
        ))
    }
}

impl game_api::GameMenus for T5 {
    /// Black Ops' menu font picker: the fixed fonts by number, else by the
    /// item's text scale on screen (`placement_scale` × `text_scale`) against
    /// `ui_smallFont` (0.25) and `ui_bigFont` (0.4). Bold picks small, bold or
    /// normal; the default small, normal or big (`ui_extraBigFont` is not
    /// registered, so its extra-big step never applies).
    fn font(&self, font_enum: i32, placement_scale: f32, text_scale: f32) -> Rule<&'static str> {
        const UI_SMALL_FONT: f32 = 0.25;
        const UI_BIG_FONT: f32 = 0.4;
        Rule::Known(match font_enum {
            2 => "fonts/bigfont",
            3 => "fonts/smallfont",
            1 => "fonts/normalfont",
            5 => "fonts/consolefont",
            6 => "fonts/extrabigfont",
            _ => {
                let scale = text_scale * placement_scale;
                if scale <= UI_SMALL_FONT {
                    "fonts/smallfont"
                } else if font_enum == 4 {
                    if UI_BIG_FONT <= scale {
                        "fonts/boldfont"
                    } else {
                        "fonts/normalfont"
                    }
                } else if scale < UI_BIG_FONT {
                    "fonts/normalfont"
                } else {
                    "fonts/bigfont"
                }
            }
        })
    }

    /// `UI_SetActiveMenu(UIMENU_INGAME)` in zombies: the pause menu, only
    /// once the in-game options are loaded; offline it pauses the game
    /// (`CL_PauseGame`) until no menu is up (`UIMENU_NONE`).
    fn ingame_menu(&self) -> Rule<game_api::InGameMenu> {
        Rule::Known(game_api::InGameMenu::Named {
            menu: "pausedmenu",
            requires: Some("ingameoptions"),
            pauses_offline: true,
        })
    }

    fn layout(&self) -> Rule<&'static dyn game_api::MenuLayout> {
        Rule::Known(&MenuLayout)
    }
}

/// Black Ops' menu engine placement and text measures, read from its
/// executable. Its menus have no scale transition (no `lerp` menu command),
/// so a menu's scale never changes an item.
struct MenuLayout;

/// Black Ops rounds a measured text size to the nearest whole pixel, halves up.
fn round_half_up(value: f32) -> f32 {
    (value + 0.5).floor()
}

impl game_api::MenuLayout for MenuLayout {
    /// `text_scale` × 48 over the font's pixel height, worked in double
    /// precision and stored as a float.
    fn text_scale(&self, pixel_height: i32, text_scale: f32) -> f32 {
        (f64::from(text_scale) * 48.0 / f64::from(pixel_height)) as f32
    }

    fn text_height(&self, pixel_height: i32, text_scale: f32) -> f32 {
        round_half_up(pixel_height as f32 * self.text_scale(pixel_height, text_scale))
    }

    fn text_width(&self, glyph_width: i32, pixel_height: i32, text_scale: f32) -> f32 {
        round_half_up(glyph_width as f32 * self.text_scale(pixel_height, text_scale))
    }

    /// Left (0), centred (1) or right (2) in the rect's width, and on top
    /// (0), below the text's height (4), centred (8) or at the bottom (12)
    /// of its height, after the item's text offset.
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

    /// Black Ops' `ScrPlace_SetupFloatViewport` passes `1.0` for both the
    /// viewable and the adjustable safe areas.
    fn safe_area(&self) -> f32 {
        1.0
    }
}

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
