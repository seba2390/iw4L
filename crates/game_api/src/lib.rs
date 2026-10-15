//! What a game provides to a match. A rule the game's own data does not give
//! is [`Rule::Unknown`]: the caller reports it as a gap and applies nothing in
//! its place — never another game's rule.

pub mod fire;
pub mod movement;
pub mod post;
pub mod turret;
pub mod ui;

pub use movement::{MovementRules, WeaponRules};

/// A rule this game does not have yet. `id` is listed in
/// `docs/fidelity/<game>.md` (`cargo xtask boundary` checks it).
#[derive(Debug, PartialEq, Eq)]
pub struct Unknown {
    pub id: &'static str,
    pub what: &'static str,
    pub needs: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule<T> {
    Known(T),
    Unknown(&'static Unknown),
}

/// `unknown!("t5.area.rule", "what is missing", "what would recover it")`.
#[macro_export]
macro_rules! unknown {
    ($id:literal, $what:literal, $needs:literal $(,)?) => {
        &$crate::Unknown {
            id: $id,
            what: $what,
            needs: $needs,
        }
    };
}

pub struct ScriptRequest<'a> {
    pub map: &'a str,
    pub gametype: &'a str,
    /// The map's entity string.
    pub entities: &'a str,
    /// The traversal animscripts the map's negotiation begin nodes name.
    pub traversals: &'a [String],
    /// A config file the map's zones carry (`gamedata/…/x.cfg`), by name.
    pub config: &'a dyn Fn(&str) -> Option<&'a str>,
    /// A string table the map's zones carry, by name: its column count and cells.
    pub table: &'a TableLookup<'a>,
}

/// A string table by name: its column count and cells, row by row.
pub type TableLookup<'a> = dyn Fn(&str) -> Option<(usize, &'a [String])> + 'a;

/// The functions a game's client engine calls in its client scripts.
#[derive(Clone, Copy, Debug)]
pub struct ClientCallbacks {
    /// The local client connected, after the level's main started:
    /// `(local client)`.
    pub local_client_connect: &'static str,
    /// A client system's state changed: `(local client, system, state)`.
    pub state_change: &'static str,
    /// The server started a client exploder: `(exploder id)`.
    pub activate_exploder: &'static str,
    /// The server stopped a client exploder: `(exploder id)`.
    pub deactivate_exploder: &'static str,
    /// A local player stepped: `(local client, player, movement type,
    /// surface, first person, quiet)`.
    pub footstep: &'static str,
    /// The local player jumped: `(local client, player, surface, first
    /// person, quiet)`.
    pub player_jump: &'static str,
    /// The local player landed: `(local client, player, surface, first
    /// person, quiet, hurt)`.
    pub player_land: &'static str,
    /// A snapshot entity (actor, missile, vehicle) spawned on the client:
    /// `self = entity, (local client)`.
    pub entity_spawned: &'static str,
    /// A player entity spawned on the client: `self = player, (local client)`.
    pub player_spawned: &'static str,
    /// A snapshot entity that had spawned left the snapshot:
    /// `(local client, entity)`.
    pub entity_shutdown: &'static str,
    /// A client flag changed on a snapshot entity:
    /// `self = entity, (local client, flag, set, new_ent)`.
    pub client_flag: &'static str,
    /// The 16-bit client flags changed on a snapshot entity:
    /// `self = entity, (local client, flags)`.
    pub client_flag_as_val: &'static str,
    /// A non-dog zombie actor's eye-glow flag (`haseyes`) changed:
    /// `self = actor, (local client, has_eyes)`.
    pub zombie_eye: &'static str,
}

/// What a match compiles and where it starts.
pub struct ScriptProgram {
    pub catalog: gsc::Catalog,
    pub roots: Vec<String>,
    pub entries: Vec<String>,
    /// A program the game built itself (from compiled scripts); the roots are
    /// then only reported, not compiled.
    pub built: Option<gsc::Program>,
    /// The gametype's settings (`GetGametypeSetting`), by name.
    pub settings: Vec<(String, String)>,
    /// The type a map entity key's value takes, by key; a key not listed
    /// stays as the runtime reads it otherwise.
    pub key_types: Vec<(String, KeyType)>,
    /// The keys the engine reads into an entity itself. When given, a map
    /// entity keeps only these and the keys of `key_types`, and a struct only
    /// the keys of `key_types`.
    pub entity_fields: Vec<(String, KeyType)>,
    /// Dvars the game sets for this match over the config and engine defaults.
    pub dvars: Vec<(String, String)>,
}

/// The scripts a map's local clients run (Black Ops 2's `.csc`), and how the
/// client game starts them.
pub struct ClientScriptProgram {
    pub program: gsc::Program,
    /// The map entities the client spawns itself (`CG_SpawnEntities`), each
    /// with its entity type (`eType`) and its typed fields, in map order.
    pub entities: Vec<(i32, Vec<(String, KeyType, String)>)>,
    /// Each of those entities' spawnflags, in the same order: a client
    /// trigger's say what touches it (`CG_SetTriggerFlags`).
    pub entity_spawnflags: Vec<i32>,
    /// The map entities the client makes into structs (`createstruct`), each
    /// with the keys it sets and their types, in map order.
    pub structs: Vec<Vec<(String, KeyType, String)>>,
    /// The actor types' precache functions, each with the index the engine
    /// passes it, run as the client game starts.
    pub precaches: Vec<(String, i32)>,
    /// The functions the client game runs on level as it starts, in order.
    pub starts: Vec<String>,
    /// The callbacks the client game calls itself.
    pub callbacks: ClientScriptCallbacks,
}

/// The script functions a client game calls on its own events.
#[derive(Default)]
pub struct ClientScriptCallbacks {
    /// `(clientnum)` on level when a local client connects.
    pub local_client_connect: Option<String>,
    /// `(localclientnum)` on a player entity the client first sees.
    pub player_spawned: Option<String>,
    /// `(localclientnum)` on another entity the client first sees.
    pub entity_spawned: Option<String>,
    /// `(localclientnum, entity)` on level when an entity leaves the client's view.
    pub entity_shutdown: Option<String>,
    /// `(clientnum, system, state)` on level when a client system's state changes.
    pub state_change: Option<String>,
    /// `(number)` on level when the server starts an exploder on the clients.
    pub activate_exploder: Option<String>,
    /// `(number)` on level when the server stops one.
    pub deactivate_exploder: Option<String>,
    /// `(localclientnum, type, locations)` on an entity when `gib` fires.
    pub gib_event: Option<String>,
    /// `(localclientnum, player, surface, firstperson, quiet, islouder)` on level when the local player jumps.
    pub player_jump: Option<String>,
    /// `(localclientnum, player, surface, firstperson, quiet, damageplayer, islouder)` on level when the local player lands.
    pub player_land: Option<String>,
    /// `(localclientnum, player, firstperson, quiet)` on level when the local player moves through foliage.
    pub player_foliage: Option<String>,
    /// `(room, package, roomcollider, packagecollider, defaultroom)` on level
    /// when the local player's ambient room or package changes.
    pub sound_set_ambient_state: Option<String>,
}

/// The type of a map entity key's value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyType {
    Int,
    Float,
    String,
    Vector,
}

pub trait GameScripts: Sync {
    fn program(
        &self,
        request: &ScriptRequest<'_>,
        sources: &dyn gsc::SourceResolver,
    ) -> Rule<ScriptProgram>;

    /// The game's client scripts for the map, when its client runs scripts
    /// of its own (Black Ops' CSC): their roots and its client builtins.
    fn client_program(
        &self,
        _request: &ScriptRequest<'_>,
        _sources: &dyn gsc::SourceResolver,
    ) -> Option<ScriptProgram> {
        None
    }

    /// The functions the game's client engine calls in its client scripts.
    fn client_callbacks(&self) -> Option<ClientCallbacks> {
        None
    }

    /// The compiled client scripts a map's local clients run, when the game
    /// has them (Black Ops 2's `.csc`).
    fn compiled_client_program(
        &self,
        _request: &ScriptRequest<'_>,
        _sources: &dyn gsc::SourceResolver,
    ) -> Option<Rule<ClientScriptProgram>> {
        None
    }

    /// Engine dvars this game's code sets for `gametype` before scripts run.
    fn engine_dvars(&self, gametype: &str) -> &'static [(&'static str, &'static str)];

    /// Engine dvars this game's code sets when the match config does not.
    fn config_defaults(&self) -> &'static [(&'static str, &'static str)];
}

/// What being in last stand does to a player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DownedRules {
    /// How long a player who just went down takes no damage.
    pub immune_ms: i64,
    /// Damage to a downed player leaves their health and raises the last
    /// stand callback again, instead of killing them.
    pub hurt_calls_back: bool,
    /// Going down sets the shared movement's last stand view height.
    pub sets_view_height: bool,
    /// A revive gives the player back their max health.
    pub revive_restores_health: bool,
    /// What going down or being revived does that is not run.
    pub unrun: Option<&'static Unknown>,
}

impl DownedRules {
    /// A downed player is briefly immune, then dies when hurt; a revive
    /// leaves their health.
    pub const BRIEFLY_IMMUNE: Self = Self {
        immune_ms: 500,
        hurt_calls_back: false,
        sets_view_height: true,
        revive_restores_health: false,
        unrun: None,
    };
}

/// What a failed level does: `missionfailed()` tells every client and, once
/// a delay has passed, loads the level again.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FailedLevelRules {
    /// The client dvar every client is sent, with its value.
    pub client_dvar: (&'static str, &'static str),
    /// The delay from the failure to the reload, before the death delay.
    pub reload_delay_ms: i64,
    /// The death delay added to it.
    pub death_delay_ms: i64,
    /// The screen blur target ramped over `death_delay_ms`.
    pub blur_target: f32,
    /// The server dvar whose value is sent to every client as the dead
    /// quote, when set.
    pub deadquote_dvar: &'static str,
    /// The local sound played on every client when a dead quote is sent.
    pub death_sound: &'static str,
    /// What failing does that is not run.
    pub unrun: Option<&'static Unknown>,
}

/// What a game's mode asks of the code around its scripts. Staged for the
/// match: the simulation reads it from its bootstrap, the HUD as a resource.
#[derive(bevy_ecs::resource::Resource, Clone, Copy, Debug, PartialEq)]
pub struct ModeRules {
    /// The level notify that ends the warmup.
    pub play_starts_on: &'static str,
    /// Every player goes down into last stand instead of dying.
    pub every_player_downs: Rule<bool>,
    /// What being down does to a player.
    pub downed: DownedRules,
    /// What a failed level does, when the mode's scripts can fail it.
    pub level_failed: Option<FailedLevelRules>,
    /// Whose player movement (look, walk, jump, stances, gravity, collision)
    /// moves the players.
    pub movement: Rule<MovementRules>,
    /// Players' weapons run Modern Warfare 2's weapon state machine.
    pub weapons: Rule<WeaponRules>,
    /// The contents a player's bullet stops on, where the game's differ from
    /// the simulation's own.
    pub bullet_mask: Option<u32>,
    /// The contents a player's melee trace stops on, where the game's differ
    /// from the simulation's own.
    pub melee_mask: Option<u32>,
    /// How the game launches, moves and detonates its missiles.
    pub missiles: Option<fire::MissileRules>,
    /// A spawn starts at the default full health, not the stored max health.
    pub spawn_at_default_health: bool,
    /// The team every player joins on connect, when the scripts pick none.
    pub connect_team: Option<&'static str>,
    /// A connected player is held in place until the scripts spawn them.
    pub scripts_spawn_players: bool,
    /// The only spawn classnames players use, when the mode names them.
    pub spawn_classnames: Option<&'static [&'static str]>,
    /// The match has no score or time limit.
    pub unlimited: Rule<bool>,
    /// Score and time limits come from the match config's
    /// `scr_<gametype>_scorelimit` / `_timelimit` dvars.
    pub limits_from_config: bool,
    /// The score limit when neither the host nor the config sets one.
    pub default_score_limit: Option<i32>,
    /// The match's scripts come from the zombie zones beside the map.
    pub zombie_zone_scripts: bool,
    /// Builtins the scripts bind but the runtime lacks are listed at start.
    pub report_builtin_gaps: bool,
    /// The host waits for every lobby member before play starts.
    pub waits_for_lobby: bool,
    /// The local account's persistent data is bound to the match.
    pub binds_account: bool,
    /// Objective models, effects and weapons are bound for non-deathmatch modes.
    pub binds_objectives: bool,
    /// Rules IW4L wrote for this game's mode run the match beside its
    /// scripts (the game's ledger lists them).
    pub host_rules: bool,
    pub hud: HudRules,
    /// What a client does when a player fires; `None` runs Modern Warfare
    /// 2's.
    pub client_fire: Option<fire::ClientFireRules>,
    /// How a client's viewmodel plays the game's own weapon animation
    /// numbers; `None` reads them as Modern Warfare 2's.
    pub viewmodel: Option<ViewmodelRules>,
    /// A script's sound goes out as its name's hash alone: the server checks
    /// nothing about the alias, so an unknown one plays nothing instead of
    /// failing the call.
    pub script_sounds_hashed: bool,
    /// How the game spawns and runs its map turrets; `None` leaves them
    /// plain entities.
    pub turrets: Option<turret::TurretRules>,
    /// The game's own post chain, drawn over every 3D frame in place of the
    /// renderer's; `None` keeps the renderer's.
    pub post_chain: Option<post::PostChainRules>,
}

/// How a client's viewmodel plays a player's weapon animation number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewmodelRules {
    /// What a change of the weapon animation number plays; `named(i)` tells
    /// whether the weapon names its animation `i` (in the game's own order).
    /// The second value is what the change does that is not run.
    pub anim: ViewmodelAnimFn,
    /// How the game starts an animation (its blend and rate), which the
    /// runtime's viewmodel does not run.
    pub playback: &'static Unknown,
    /// What an animation the runtime's viewmodel holds no slot for does not
    /// do.
    pub unslotted: &'static Unknown,
}

pub type ViewmodelAnimFn =
    fn(weap_anim: u32, named: &dyn Fn(usize) -> bool) -> (ViewmodelAnim, Option<&'static Unknown>);

/// What a viewmodel does on a weapon animation change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewmodelAnim {
    /// Idle once the animations playing have finished.
    Idle,
    /// Idle now.
    IdleNow,
    /// The weapon's animation by its index in the game's own order.
    Play(usize),
    /// What plays goes on.
    Keep,
}

/// Which of the code-drawn HUD pieces a mode shows, and how.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HudRules {
    /// The code-drawn HUD (hud elems, blood, reticle, killfeed, names, hints,
    /// splash, playercard, flash, compass, score bar, scoreboard, the menus it
    /// paints) applies to this mode.
    pub code_hud: Rule<()>,
    /// The game's own HUD menus draw the weapon info in place of the code weaponbar.
    pub game_hud_menus: bool,
    /// While the code-drawn HUD is unknown, the game's HUD menus, script hud
    /// elems and script menus still draw; only the code-drawn pieces hide.
    pub script_hud: bool,
    pub scoreboard: bool,
    pub scorebar: bool,
    pub compass: bool,
    /// How script text elems are drawn.
    pub script_text: ScriptText,
    /// A script material elem grows to the elem's font height.
    pub material_font_floor: bool,
    /// The mode is zombies: a player's own points, the round, perks and
    /// power-ups stand where match scores and limits would.
    pub zombies: bool,
    /// The font scale and from-font scale a client receives for an elem's
    /// scripted ones, when the game's network encodes them.
    pub font_scales_sent: Option<fn(f32, f32) -> (f32, f32)>,
}

/// How a mode's script text hud elems are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptText {
    /// Not drawn: the game's rule is not known.
    Off,
    /// Modern Warfare 2's hud elem text.
    Engine,
    /// With the game's menu fonts and menu text measures. The function takes
    /// an elem's font number in the shared list (`default`, `bigfixed`,
    /// `smallfixed`, `objective`, `big`, `small`, `hudbig`, `hudsmall`,
    /// `extrabig`) and gives the menu font number it draws with and its base
    /// scale. The elem's scale is taken to screen pixels before the font is
    /// picked and the text measured.
    MenuFonts(fn(i32) -> (i32, f32)),
}

/// How a game's screen reacts to its match: shellshocks.
pub trait GameVision: Sync {
    /// The game's shellshock files are read with Modern Warfare 2's `.shock`
    /// format and drawn by its shellshock screen.
    fn shellshock(&self) -> Rule<()>;
}

/// How a game's menus are drawn.
pub trait GameMenus: Sync {
    fn script_ui(&self, _modules: &[ui::Module]) -> Result<Option<Box<dyn ui::ScriptUi>>, String> {
        Ok(None)
    }

    /// The font a menu item's `textfont` enum names, at the item's scales.
    fn font(&self, font_enum: i32, placement_scale: f32, text_scale: f32) -> Rule<&'static str>;

    /// How the game's menu engine places and scales an item.
    fn layout(&self) -> Rule<&'static dyn MenuLayout>;

    /// The menu the in-game menu key (Escape) opens.
    fn ingame_menu(&self) -> Rule<InGameMenu>;
}

/// What the in-game menu key opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InGameMenu {
    /// The menu the `g_scriptMainMenu` dvar names.
    ScriptMainMenu,
    /// `menu`, once `requires` is loaded too; an offline match pauses while
    /// a menu is up when `pauses_offline`.
    Named {
        menu: &'static str,
        requires: Option<&'static str>,
        pauses_offline: bool,
    },
}

/// How a game's menu engine places and scales an item's rect and text.
pub trait MenuLayout: Sync {
    /// The glyph scale of a font of `pixel_height` drawn at `text_scale`.
    fn text_scale(&self, pixel_height: i32, text_scale: f32) -> f32;
    /// The height a line of text in a font of `pixel_height` takes at
    /// `text_scale`.
    fn text_height(&self, pixel_height: i32, text_scale: f32) -> f32;
    /// The width of text whose glyphs advance `glyph_width` in a font of
    /// `pixel_height`, at `text_scale`.
    fn text_width(&self, glyph_width: i32, pixel_height: i32, text_scale: f32) -> f32;
    /// Where an item's text starts inside its rect `[x, y, w, h]`.
    #[allow(clippy::too_many_arguments)]
    fn text_origin(
        &self,
        rect: [f32; 4],
        align_mode: i32,
        align_x: f32,
        align_y: f32,
        measured_w: f32,
        measured_h: f32,
    ) -> (f32, f32);
    /// An item's text scale while its menu is scaled by `menu_scale`.
    fn text_paint_scale(&self, text_scale: f32, menu_scale: f32) -> f32;
    /// A rect `[x, y, w, h]` while its menu is scaled by `menu_scale`.
    fn window_rect(&self, rect: [f32; 4], menu_scale: f32) -> [f32; 4];
    /// The safe area fraction `ScrPlace` uses for the viewable area (`left`,
    /// `right`, `top`, `bottom`, `center_safearea`).
    fn safe_area(&self) -> f32 {
        0.85
    }
}

/// A mode the game library offers beside multiplayer, with the maps it runs on.
pub struct LibraryMode {
    pub gametype: &'static str,
    /// What the mode does and does not do yet, shown above its maps.
    pub note: &'static str,
    /// `(zone, title)`; a map is offered when its zone is installed.
    pub maps: &'static [(&'static str, &'static str)],
}

pub trait GameModes: Sync {
    fn mode(&self, gametype: &str) -> Rule<ModeRules>;

    /// The mode's rules on one map, for games whose rules differ by map.
    fn mode_on_map(&self, _map: &str, gametype: &str) -> Rule<ModeRules> {
        self.mode(gametype)
    }

    /// The game's zombies mode, when it has one the runtime can start.
    fn zombies(&self) -> Option<&'static LibraryMode>;
}
