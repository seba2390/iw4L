mod actor_physics;
mod ammo;
mod catalog;
mod client_catalog;
mod code_hud;
mod crosshair_scan;
mod fire;
mod fire_autosim;
mod fire_sound;
mod front_end_catalog;
mod grenade;
mod grenade_indicator;
mod menu_expression;
mod missile;
mod movement;
mod overlay;
mod physics;
mod post_chain;
mod radius;
mod shake;
mod shock;
mod surface;
mod top_down;
mod tracer;
mod turret;
mod view_kick;
mod viewmodel;
mod vision;
mod vision_channels;
mod vision_file;
mod weapon_info;
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

/// Black Ops' front-end level: the main menu draws over it.
const FRONT_END_LEVEL: &str = "frontend";

/// Engine dvars the zombie scripts read without setting: the mode itself and
/// the AI locomotion tuning the run animscript reads, at the defaults the
/// executable registers (no config a zombies match loads sets them).
const ZOMBIE_ENGINE_DVARS: &[(&str, &str)] = &[
    ("zombiemode", "1"),
    // The zombies executable is the singleplayer one; menus test this to
    // hide the multiplayer rows.
    ("ui_multiplayer", "0"),
    ("ai_runAnimUpdateFrequency", "0.05"),
    ("ai_useLeanRunAnimations", "1"),
    ("ai_slowdownRateBlendFactor", "1"),
    ("ai_slowdownMinRate", "1"),
    ("ai_slowdownMinYawDiff", "0"),
    ("ai_slowdownMaxYawDiff", "90"),
    ("ai_meleeRange", "64"),
    ("ai_turnRate", "220"),
    // Actors move through the physics world (registered 1, 0 to 1).
    ("phys_ai_collision_mode", "1"),
    // The hellhound's turn test and turn animscript, and its weave.
    ("dog_turn90_angle", "70"),
    ("dog_turn180_angle", "135"),
    ("dog_turn_min_goal_dist", "40"),
    ("ai_pathMomentum", "0.78"),
    // Black Ops registers it true; `_zombiemode_load` sets
    // `level.clientscripts` from it, which `clientNotify` reads.
    ("cg_usingClientScripts", "1"),
    // Engine dvars the HUD menus read, at the defaults the executable
    // registers them with (no config a zombies match loads sets them).
    ("actionSlotsHide", "0"),
    ("ammoCounterHide", "0"),
    ("hud_missionFailed", "0"),
    ("cg_hideWeaponHeat", "0"),
    ("cg_drawWeaponHeatVertical", "0"),
    ("dec20_Enabled", "0"),
    ("g_gameskill", "1"),
    ("gpad_enabled", "0"),
    ("arcademode", "0"),
    // A solo game is offline: neither a system link nor an online game.
    ("systemlink", "0"),
    ("onlinegame", "0"),
    // Registered from the executable's content flag, which this build
    // sets: the zombie scripts gib zombies (`is_mature`) only with it.
    ("cg_mature", "1"),
    ("cg_blood", "1"),
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
                .extended(zombie_catalog::T5_ZOMBIE.iter().cloned())
                .extended(front_end_catalog::T5_FRONT_END.iter().cloned()),
            roots: startup.roots,
            entries: startup.entries,
            built: None,
            settings: Vec::new(),
            key_types: Vec::new(),
            entity_fields: Vec::new(),
            dvars: if request.map == FRONT_END_LEVEL {
                // The front end is a singleplayer level, not a zombies one.
                vec![("zombiemode".to_owned(), "0".to_owned())]
            } else {
                Vec::new()
            },
        })
    }

    /// Black Ops' client loads `clientscripts/<map>.csc` and runs its `main`,
    /// with the map's structs in `level.struct` (`codescripts/struct`) and
    /// `codescripts/delete` for its one-shot triggers.
    fn client_program(
        &self,
        request: &ScriptRequest<'_>,
        _sources: &dyn gsc::SourceResolver,
    ) -> Option<ScriptProgram> {
        (request.gametype == ZOMBIES).then(|| ScriptProgram {
            catalog: gsc::Catalog::from_list(asset_core::FamilyId::T5, client_catalog::T5_CLIENT),
            roots: vec![
                "codescripts/delete".to_owned(),
                "codescripts/struct".to_owned(),
                format!("clientscripts/{}", request.map),
            ],
            entries: vec![format!("clientscripts/{}::main", request.map)],
            built: None,
            settings: Vec::new(),
            key_types: Vec::new(),
            entity_fields: Vec::new(),
            dvars: Vec::new(),
        })
    }

    /// Black Ops' client engine calls `clientscripts/_callbacks`.
    fn client_callbacks(&self) -> Option<game_api::ClientCallbacks> {
        Some(game_api::ClientCallbacks {
            local_client_connect: "clientscripts/_callbacks::localclientconnect",
            state_change: "clientscripts/_callbacks::statechange",
            activate_exploder: "clientscripts/_callbacks::callback_activate_exploder",
            deactivate_exploder: "clientscripts/_callbacks::callback_deactivate_exploder",
            footstep: "clientscripts/_callbacks::codecallback_playerfootstep",
            player_jump: "clientscripts/_callbacks::codecallback_playerjump",
            player_land: "clientscripts/_callbacks::codecallback_playerland",
            player_foliage: "clientscripts/_callbacks::codecallback_playerfoliage",
            entity_spawned: "clientscripts/_callbacks::entityspawned",
            player_spawned: "clientscripts/_callbacks::playerspawned",
            entity_shutdown: "clientscripts/_callbacks::entityshutdown_callback",
            client_flag: "clientscripts/_callbacks::client_flag_callback",
            client_flag_as_val: "clientscripts/_callbacks::client_flagasval_callback",
            zombie_eye: "clientscripts/_zombiemode::zombie_eye_callback",
            play_weapon_death_effects: "clientscripts/_callbacks::codecallback_playweapondeatheffects",
            gib_event: "clientscripts/_callbacks::codecallback_gibevent",
            trigger_delete: "codescripts/delete::main",
            level_notify: "clientscripts/_callbacks::level_notify",
            // Black Ops' client turns an actor's `footstep_` / `dogstep_`
            // note into a step at `j_ball_le` (a note whose tenth letter is
            // `l`) or `j_ball_ri` within 1024 units of the view, the ground
            // traced 15 units either side of it.
            ai_footstep: Some(game_api::AiFootstep {
                callback: "clientscripts/_footsteps::playaifootstep",
                tags: ["j_ball_le", "j_ball_ri"],
                tag_range: 1024.0,
                trace_up: 15.0,
                trace_down: 15.0,
                trace_mask: 0x0282_0011,
                surface: surface::name_of_flags,
            }),
        })
    }

    fn engine_dvars(&self, gametype: &str) -> &'static [(&'static str, &'static str)] {
        if gametype == ZOMBIES {
            ZOMBIE_ENGINE_DVARS
        } else {
            &[]
        }
    }

    /// A solo zombies match is offline and executes no match config; its
    /// dvars are the engine's registered defaults.
    fn match_config(&self) -> Option<&'static str> {
        None
    }

    fn config_defaults(&self) -> &'static [(&'static str, &'static str)] {
        &[]
    }
}

const ZOMBIE_MODE: ModeRules = ModeRules {
    // Black Ops' level runs its entities from the first frame: no warmup
    // (`all_players_connected` is the scripts' own flag).
    play_starts_on: None,
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
        // The client's `X` command ramps `com_codeTimeScale` from 1 to 0.1
        // over `g_deathDelay` of real time (not in the "vs" game type), held
        // until the reload; the time scale stays 1 in system link and
        // online games.
        slow_motion: Some(game_api::FailedSlowMotion {
            to: 0.1,
            unless_dvars: &["systemlink", "onlinegame"],
            unless_gametype: "vs",
        }),
        // The client's `q 0` command.
        fade_out_sound: true,
        // `loadgame_continue`: a zombies game loads its save in memory, or
        // stops.
        missing_save: Some("Unable to find save."),
        unrun: Some(unknown!(
            "t5.match.level_failed",
            "a failed level loads its zones again before its save takes its place; Black Ops restarts in place",
            "a restart that keeps the level's zones loaded"
        )),
    }),
    bullet_mask: None,
    melee_mask: None,
    missiles: None,
    // Black Ops moves its actors through its physics world
    // (`phys_ai_collision_mode` 1, its default).
    actor_physics: Some(game_api::actor::ActorPhysicsRules(
        &actor_physics::ACTOR_PHYSICS,
    )),
    // A live actor's contents are `CONTENTS_ACTOR` (0x4000), which the
    // player's clip mask (0x0281c011) names: zombies block the player.
    players_meet_actors: true,
    // A player spawn sets health and max health to `g_player_maxhealth`
    // (registered 100, 10 to 2000; no script sets it), never the stored
    // max health.
    spawn_health: game_api::SpawnHealth::Dvar {
        name: "g_player_maxhealth",
        default: 100,
        range: (10, 2000),
    },
    // The level's start runs `spawner()` on every actor spawner before the
    // aitypes' `precache()` and the level's `main()`.
    aitype_spawners: true,
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
        code_hud: Rule::Known(game_api::hud::CodeHud::Game(&code_hud::CODE_HUD)),
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
    top_down: Some(top_down::RULES),
    camera_shake: Some(shake::RULES),
    viewmodel: Some(viewmodel::VIEWMODEL),
    script_sounds_hashed: true,
    turrets: Some(turret::TURRETS),
    post_chain: Some(post_chain::POST_CHAIN),
    overlays: Some(overlay::OVERLAYS),
    frame_fog: None,
    dyn_ent_physics: Some(Rule::Known(game_api::DynEntPhysicsRules {
        bullet_force: 800.0,
        bullet_upbias: 0.5,
        bullet_spin_scale: 3.0,
        explode_force: 12500.0,
        explode_upbias: 0.5,
        explode_spin_scale: 4.0,
        explode_min_force: 40.0,
        explode_max_ents: 20,
        spawned_limit: 50,
        sentient_auto_activate: true,
        damage_radius_scale: 1.0,
        destruct_uses_physics: false,
    })),
    ammo_numbers: Some(crate::ammo::numbers),
    // The extra camera's target: no `xGlobals` asset in Kino's or Five's
    // zones leaves the default mode, 256 by 256 (aspect 1, with
    // `r_extracam_custom_aspectratio` at its -1 default).
    extra_camera: Some(game_api::ExtraCameraRules { size: [256, 256] }),
    // Black Ops' singleplayer and zombies run the menus and the level in
    // one process: a menu's `setDvar` is what `GetDvar` reads (the front
    // end's map select sets `ui_load_level` for `ChangeLevel`).
    menu_dvars_reach_server: true,
    // A zombies map's 3D UI texture; the front end's is square
    // (`ui3d_size`).
    ui3d: Some(game_api::Ui3dRules { size: [1024, 512] }),
};

/// Black Ops' 3D UI texture: 1024 by 1024 on the front end and the `menu_`
/// levels, 1024 by 512 on every other map.
fn ui3d_size(map: &str) -> [u32; 2] {
    if map == FRONT_END_LEVEL || map.starts_with("menu_") {
        [1024, 1024]
    } else {
        [1024, 512]
    }
}

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
    /// Black Ops' shellshock files, whose screen blur and flash, look
    /// control and timing are Modern Warfare 2's.
    fn shellshock(&self) -> Rule<game_api::ShockFormat> {
        Rule::Known(game_api::ShockFormat::Game {
            parse: shock::parse,
            fallback: shock::FALLBACK,
        })
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

    fn menu_dvars(&self) -> &'static [game_api::MenuDvar] {
        &MENU_DVARS
    }

    /// Black Ops' table by `snd_menu_speaker_setup` (TV, surround,
    /// headphones) and `snd_menu_listen_level` (quiet, loud), each held to
    /// its dvar's range.
    fn master_preset(&self, dvar: &dyn Fn(&str) -> Option<String>) -> Option<&'static str> {
        const PRESETS: [[&str; 2]; 3] = [
            ["tv_quiet", "tv_loud"],
            ["surround_quiet", "surround_loud"],
            ["headphone_quiet", "headphone_loud"],
        ];
        let int = |name: &str, max: i32| {
            dvar(name)
                .and_then(|value| value.trim().parse::<f32>().ok())
                .filter(|value| value.is_finite())
                .map_or(0, |value| (value as i32).clamp(0, max))
        };
        let speakers = int("snd_menu_speaker_setup", 2) as usize;
        let level = int("snd_menu_listen_level", 1) as usize;
        Some(PRESETS[speakers][level])
    }

    /// `default.cfg` runs `default_controls.cfg` (the localized
    /// `code_pre_gfx`): `unbindall`, then Black Ops' own keyboard binds.
    fn default_controls(&self) -> Option<&'static str> {
        Some("default_controls.cfg")
    }
}

/// The client dvars Black Ops' options menus show, with their registered
/// defaults. `sensitivity` scales the mouse by `m_yaw` / `m_pitch` (0.022)
/// as the runtime's does; `cg_fov_default` is the player's field of view;
/// every sound plays at the square of `snd_menu_master`.
/// `r_gamma` sets the display's gamma ramp; a sound plays at the square of
/// its mixer group category's volume: `snd_menu_sfx` for categories 0 and
/// 3, `snd_menu_music` for 1, `snd_menu_voice` for 2. `snd_menu_cinematic`
/// is the cinematics' (docs/fidelity/t5.md). `snd_menu_speaker_setup` and
/// `snd_menu_listen_level` pick the sound driver's master preset.
const MENU_DVARS: [game_api::MenuDvar; 16] = {
    use game_api::{MenuDvar, MenuSetting};
    const fn shown(name: &'static str, default: &'static str) -> MenuDvar {
        MenuDvar {
            name,
            default,
            setting: None,
        }
    }
    [
        MenuDvar {
            name: "sensitivity",
            default: "5",
            setting: Some(MenuSetting::MouseSensitivity),
        },
        MenuDvar {
            name: "cg_fov_default",
            default: "65",
            setting: Some(MenuSetting::FieldOfView),
        },
        MenuDvar {
            name: "snd_menu_master",
            default: "1",
            setting: Some(MenuSetting::SquaredMasterVolume),
        },
        MenuDvar {
            name: "snd_menu_music",
            default: "1",
            setting: Some(MenuSetting::CategoryVolume(&[1])),
        },
        MenuDvar {
            name: "snd_menu_sfx",
            default: "1",
            setting: Some(MenuSetting::CategoryVolume(&[0, 3])),
        },
        MenuDvar {
            name: "snd_menu_voice",
            default: "1",
            setting: Some(MenuSetting::CategoryVolume(&[2])),
        },
        shown("snd_menu_cinematic", "1"),
        shown("snd_menu_speaker_setup", "0"),
        shown("snd_menu_listen_level", "0"),
        MenuDvar {
            name: "r_gamma",
            default: "0.9",
            setting: Some(MenuSetting::DisplayGamma),
        },
        shown("r_texFilterAnisoMin", "1"),
        shown("cl_freelook", "1"),
        // The front end's main menu opens its lockout first, and its
        // buttons test the language.
        shown("ui_skipMainLockout", "0"),
        shown("loc_language", "0"),
        // The zombie map select lists Five and Dead Ops Arcade while these
        // (archived booleans, registered 1) are on.
        shown("zombiefive_discovered", "1"),
        shown("zombietron_discovered", "1"),
    ]
};

/// Black Ops' menu engine placement and text measures, read from its
/// executable. Its menus have no scale transition (no `lerp` menu command),
/// so a menu's scale never changes an item.
struct MenuLayout;

/// Where Black Ops' wrapped lines start and end in `chars` at `width`
/// (its line breaker): each `(start, end)`; a line that cannot hold one
/// glyph stops the text.
fn wrap_line_ends(
    chars: &[char],
    width: i32,
    glyph: &dyn Fn(char) -> i32,
    scale: f32,
) -> Vec<(usize, usize)> {
    let mut ends = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let end = wrap_line_end(chars, start, width, glyph, scale);
        if end <= start {
            break;
        }
        ends.push((start, end));
        start = end;
        while start < chars.len() && chars[start] != '\n' && chars[start].is_whitespace() {
            start += 1;
        }
    }
    ends
}

/// One wrapped line of Black Ops' text from `start`: where it ends.
fn wrap_line_end(
    chars: &[char],
    start: usize,
    width: i32,
    glyph: &dyn Fn(char) -> i32,
    scale: f32,
) -> usize {
    let limit = width as f32;
    let mut advance = 0;
    let mut space_break = None;
    let mut at = start;
    while at < chars.len() {
        let c = chars[at];
        let mut next = at + 1;
        match c {
            '\r' => advance = 0,
            '\n' => return next,
            '^' if chars
                .get(next)
                .is_some_and(|&code| ('0'..='@').contains(&code) || code == 'F') =>
            {
                next += 1;
            }
            _ => {
                advance += glyph(c);
                if at != start && c.is_whitespace() {
                    space_break = Some(next);
                }
                let wide = limit < advance as f32 * scale;
                if let Some(end) = space_break.filter(|_| wide) {
                    return end;
                }
                if wide {
                    return at;
                }
            }
        }
        at = next;
    }
    chars.len()
}

/// Black Ops rounds a measured text size to the nearest whole pixel, halves up.
fn round_half_up(value: f32) -> f32 {
    (value + 0.5).floor()
}

impl game_api::MenuLayout for MenuLayout {
    /// `Menu_Paint` draws a fullscreen menu's background over the 640×480
    /// screen in the menu's alignment, then `Window_Paint`s the menu window.
    fn paints_menu_window(&self) -> bool {
        true
    }

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

    /// Black Ops' text extents take the width of the dvar's value (as
    /// stored, not as the item shows it) off the rect of these item types.
    fn value_room(&self, item_type: i32) -> bool {
        matches!(item_type, 5 | 7 | 13 | 14 | 22 | 30)
    }

    /// Black Ops' slider: no text and no value, only ten 9 × 9 notches
    /// across 100 units (placed by the text alignment), lit up to the value
    /// (the notch at the value split in two halves of 4), and the
    /// `ui_arrow_right` marker turned to point up at the value from below.
    fn slider(
        &self,
        rect: [f32; 4],
        align_mode: i32,
        align_x: f32,
        align_y: f32,
        fraction: f32,
    ) -> Option<Vec<game_api::MenuQuad>> {
        const LIT: [f32; 4] = [0.96, 0.58, 0.11, 1.0];
        const DIM: [f32; 4] = [0.96, 0.58, 0.11, 0.2];
        let [x, y, w, h] = rect;
        let mut x0 = x + align_x;
        match align_mode & 3 {
            0 => {}
            1 => x0 += (w - 100.0) * 0.5,
            _ => x0 += w - 100.0,
        }
        let mut y0 = y + align_y;
        match align_mode & 0xc {
            0xc => y0 += h - 9.0,
            8 => y0 += (h - 9.0) * 0.5,
            _ => {}
        }
        let at = x0 + fraction * 100.0;
        let quad = |x: f32, w: f32, color| game_api::MenuQuad {
            rect: [x, y0, w, 9.0],
            color,
            material: "white",
            turn: 0.0,
        };
        let mut quads = Vec::with_capacity(12);
        for step in (0..100).step_by(10) {
            let sx = step as f32 + x0;
            if at + 2.0 > sx + 10.0 {
                quads.push(quad(sx, 9.0, LIT));
            } else if sx > at - 2.0 {
                quads.push(quad(sx, 9.0, DIM));
            } else {
                quads.push(quad(sx, 4.0, LIT));
                quads.push(quad(sx + 4.0, 4.0, DIM));
            }
        }
        quads.push(game_api::MenuQuad {
            rect: [at - 3.0 - 1.0, y0 + 9.0 - 1.0, 7.0, 7.0],
            color: LIT,
            material: "ui_arrow_right",
            turn: -90.0,
        });
        Some(quads)
    }

    /// Black Ops' text width skips `^` with `0` to `@` or `F`, `^C` with a
    /// `0` to `@` after it, and `^L`; any other `^` is a glyph.
    fn text_code_len(&self, next: char, after: Option<char>) -> usize {
        let code = |c: char| ('0'..='@').contains(&c);
        match next {
            c if code(c) || c == 'F' => 1,
            'C' if after.is_some_and(code) => 2,
            'L' => 1,
            _ => 0,
        }
    }

    /// Black Ops' wrapped text: each line ends after the last space before
    /// the glyphs pass the rect's width (or mid-word when none), a `\n`
    /// ends a line and `\r` restarts the count; spaces after a break are
    /// skipped. Centred text that overflows is re-wrapped to as few lines,
    /// at the narrowest width (from its even share, in steps of 8) that
    /// keeps that many.
    fn wrap_lines(
        &self,
        text: &str,
        rect_w: f32,
        align_mode: i32,
        text_w: f32,
        glyph: &dyn Fn(char) -> i32,
        scale: f32,
    ) -> Option<Vec<String>> {
        let chars: Vec<char> = text.chars().collect();
        let mut width = rect_w as i32;
        if rect_w != 0.0 && rect_w < text_w && align_mode & 3 == 1 {
            let lines = (f64::from(text_w) / f64::from(rect_w)).ceil() as i32;
            let mut wrap = (f64::from(text_w) / f64::from(lines)).ceil() as i32;
            let full = width;
            width = loop {
                if wrap >= full {
                    break full;
                }
                if wrap_line_ends(&chars, wrap, glyph, scale).len() as i32 == lines {
                    break wrap;
                }
                wrap += 8;
            };
        }
        let ends = wrap_line_ends(&chars, width, glyph, scale);
        let mut lines = Vec::with_capacity(ends.len());
        for (start, end) in ends {
            let line: String = chars[start..end].iter().collect();
            lines.push(line.trim_end_matches(['\n', '\r']).to_owned());
        }
        Some(lines)
    }

    /// `UI_Refresh` draws `ui_cursor` 32 by 32 (virtual, in the full screen
    /// placement's scale) centred on the cursor while the cursor is in the
    /// window and the focused menu has no 3D window, in no colour.
    fn cursor(&self) -> Option<(&'static str, f32)> {
        Some(("ui_cursor", 32.0))
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

    fn mode_on_map(&self, map: &str, gametype: &str) -> Rule<ModeRules> {
        match self.mode(gametype) {
            Rule::Known(mode) => Rule::Known(ModeRules {
                ui3d: Some(game_api::Ui3dRules {
                    size: ui3d_size(map),
                }),
                ..mode
            }),
            unknown => unknown,
        }
    }

    fn zombies(&self) -> Option<&'static LibraryMode> {
        Some(&ZOMBIES_LIBRARY)
    }
}

static ZOMBIES_LIBRARY: LibraryMode = LibraryMode {
    gametype: ZOMBIES,
    note: "Runs the map's own zombie scripts. Kino der Toten and Five play solo; gaps are listed in docs/fidelity/t5.md.",
    front_end: Some(FRONT_END_LEVEL),
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
    starts: &[],
    capture: None,
};
