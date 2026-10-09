//! What a game provides to a match. A rule the game's own data does not give
//! is [`Rule::Unknown`]: the caller reports it as a gap and applies nothing in
//! its place — never another game's rule.

/// A rule this game does not have yet. `id` is listed in
/// `docs/fidelity/<game>.md` (`cargo xtask boundary` checks it).
#[derive(Debug)]
pub struct Unknown {
    pub id: &'static str,
    pub what: &'static str,
    pub needs: &'static str,
}

#[derive(Debug)]
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
}

/// What a match compiles and where it starts.
pub struct ScriptProgram {
    pub catalog: gsc::Catalog,
    pub roots: Vec<String>,
    pub entries: Vec<String>,
}

pub trait GameScripts: Sync {
    fn program(
        &self,
        request: &ScriptRequest<'_>,
        sources: &dyn gsc::SourceResolver,
    ) -> Rule<ScriptProgram>;

    /// Engine dvars this game's code sets for `gametype` before scripts run.
    fn engine_dvars(&self, gametype: &str) -> &'static [(&'static str, &'static str)];
}

/// What a game's mode asks of the code around its scripts. Staged for the
/// match: the simulation reads it from its bootstrap, the HUD as a resource.
#[derive(bevy_ecs::resource::Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModeRules {
    /// The level notify that ends the warmup.
    pub play_starts_on: &'static str,
    /// Every player goes down into last stand instead of dying.
    pub every_player_downs: bool,
    /// A spawn starts at the default full health, not the stored max health.
    pub spawn_at_default_health: bool,
    /// The team every player joins on connect, when the scripts pick none.
    pub connect_team: Option<&'static str>,
    /// A connected player is held in place until the scripts spawn them.
    pub scripts_spawn_players: bool,
    /// The only spawn classnames players use, when the mode names them.
    pub spawn_classnames: Option<&'static [&'static str]>,
    /// The match has no score or time limit.
    pub unlimited: bool,
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
    pub hud: HudRules,
}

/// Which of the code-drawn HUD pieces a mode shows, and how.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HudRules {
    /// The game's own HUD menus draw the weapon info in place of the code weaponbar.
    pub game_hud_menus: bool,
    pub scoreboard: bool,
    pub scorebar: bool,
    pub compass: bool,
    /// Script text elems are drawn (their font and size rule is known).
    pub script_text: bool,
    /// A script material elem grows to the elem's font height.
    pub material_font_floor: bool,
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
