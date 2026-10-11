//! What a game provides to a match. A rule the game's own data does not give
//! is [`Rule::Unknown`]: the caller reports it as a gap and applies nothing in
//! its place — never another game's rule.

pub mod fire;
pub mod movement;

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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FailedLevelRules {
    /// The client dvar every client is sent, with its value.
    pub client_dvar: (&'static str, &'static str),
    /// The delay from the failure to the reload, before the death delay.
    pub reload_delay_ms: i64,
    /// The death delay added to it.
    pub death_delay_ms: i64,
    /// What failing does that is not run.
    pub unrun: Option<&'static Unknown>,
}

/// What a game's mode asks of the code around its scripts. Staged for the
/// match: the simulation reads it from its bootstrap, the HUD as a resource.
#[derive(bevy_ecs::resource::Resource, Clone, Copy, Debug, PartialEq, Eq)]
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
    /// The font a menu item's `textfont` enum names, at the item's scales.
    fn font(&self, font_enum: i32, placement_scale: f32, text_scale: f32) -> Rule<&'static str>;

    /// How the game's menu engine places and scales an item.
    fn layout(&self) -> Rule<&'static dyn MenuLayout>;
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

    /// The game's zombies mode, when it has one the runtime can start.
    fn zombies(&self) -> Option<&'static LibraryMode>;
}
