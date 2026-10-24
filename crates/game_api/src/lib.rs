//! What a game provides to a match. A rule the game's own data does not give
//! is [`Rule::Unknown`]: the caller reports it as a gap and applies nothing in
//! its place — never another game's rule.

pub mod fire;
pub mod fog;
pub mod hud;
pub mod movement;
pub mod post;
pub mod top_down;
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
    /// The start location the host picked, for games whose zombies maps
    /// start at one of several (`ui_zm_mapstartlocation`).
    pub location: Option<&'a str>,
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
    /// The local player moved through foliage: `(local client, player,
    /// first person, quiet)`.
    pub player_foliage: &'static str,
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
    /// An entity plays its death effect for a weapon (`playweapondeatheffects`):
    /// `self = entity, (local client, weapon, userdata)`.
    pub play_weapon_death_effects: &'static str,
    /// An entity was gibbed (`gib`):
    /// `self = entity, (local client, type, locations)`.
    pub gib_event: &'static str,
    /// A one-shot client trigger was touched: `self = trigger`.
    pub trigger_delete: &'static str,
    /// A level notify the client's engine raises (a viewmodel animation's
    /// note: `("notetrack", local client, note)`).
    pub level_notify: &'static str,
    /// An actor's foot struck the ground, when the game's client sounds it.
    pub ai_footstep: Option<AiFootstep>,
}

/// How a game's client sounds an actor's footstep (its `footstep_` or
/// `dogstep_` notetrack): `callback(local client, entity, position, surface,
/// on fire[, true for a dog])` on the level.
#[derive(Clone, Copy, Debug)]
pub struct AiFootstep {
    pub callback: &'static str,
    /// The left and right foot tags; the step sounds at its foot when the
    /// actor is within `tag_range` of the view, else at the actor's origin.
    pub tags: [&'static str; 2],
    pub tag_range: f32,
    /// The trace for the ground under the step: this far above and below
    /// it, stopping on these contents.
    pub trace_up: f32,
    pub trace_down: f32,
    pub trace_mask: u32,
    /// The surface name a trace's surface flags give.
    pub surface: fn(surface_flags: u32) -> &'static str,
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
    /// Mounted DLC package names (`dlc0`, `dlczm0`, `dlc3`, ...) in the install.
    pub dlc_packages: Vec<String>,
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

    /// The config file a match executes before its scripts, when the game
    /// executes one.
    fn match_config(&self) -> Option<&'static str> {
        Some("default_xboxlive.cfg")
    }
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
    /// Time slows over the death delay, when the game slows it.
    pub slow_motion: Option<FailedSlowMotion>,
    /// Every client fades its whole mix out (and stops its streams).
    pub fade_out_sound: bool,
    /// What failing does that is not run.
    pub unrun: Option<&'static Unknown>,
}

/// The game's time scale ramped from 1 to `to` over the death delay of
/// real time and held until the level loads again; not while any of
/// `unless_dvars` is set, nor in the game type `unless_gametype`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FailedSlowMotion {
    pub to: f32,
    pub unless_dvars: &'static [&'static str],
    pub unless_gametype: &'static str,
}

/// What a game's mode asks of the code around its scripts. Staged for the
/// match: the simulation reads it from its bootstrap, the HUD as a resource.
/// No `PartialEq` here or on the rules it holds: several hold function
/// pointers, and Rust does not promise a function one address.
#[derive(bevy_ecs::resource::Resource, Clone, Copy, Debug)]
pub struct ModeRules {
    /// The level notify that ends the warmup; `None` runs the match from its
    /// first frame, with no warmup.
    pub play_starts_on: Option<&'static str>,
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
    /// The health a spawning player starts at.
    pub spawn_health: SpawnHealth,
    /// Each actor spawner runs its aitype script's `spawner()` when the level
    /// loads, before the level's own scripts start.
    pub aitype_spawners: bool,
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
    /// The game's camera from above the players, which its client dvars
    /// turn on; `None` keeps every client in its player's eye.
    pub top_down: Option<top_down::TopDownRules>,
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
    /// The game's own screen overlays, drawn over the frame after its post
    /// passes; `None` draws none.
    pub overlays: Option<post::OverlayRules>,
    /// The game's own frame fog; `None` keeps the renderer's.
    pub frame_fog: Option<fog::FrameFogRules>,
    /// The game's own rules for what bullets, explosions and players do to
    /// the map's dynamic entities; `None` runs Modern Warfare 2's.
    pub dyn_ent_physics: Option<Rule<DynEntPhysicsRules>>,
    /// How the game numbers its weapons' ammo and clips; the match keeps a
    /// player's rounds by these numbers. `None` numbers them by the weapons'
    /// ammo and clip names for the game's own rules, and keeps the match's
    /// rounds by the weapons' combat facts.
    pub ammo_numbers: Option<movement::AmmoNumbers>,
    /// How the game draws a client's extra camera, the second view of the
    /// world its materials sample; `None` draws none.
    pub extra_camera: Option<ExtraCameraRules>,
    /// The dvars the host's menus set are the server's: the game runs its
    /// menus and its level in one process with one dvar table.
    pub menu_dvars_reach_server: bool,
}

/// The game's second view of the world: a client script's camera entity
/// seen into a target its materials sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtraCameraRules {
    /// The target's size in pixels; its aspect is the view's.
    pub size: [u32; 2],
}

/// Where a client's extra camera stands and looks this frame, and its
/// vertical field of view in degrees.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ExtraCamera {
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub fov: f32,
}

/// The health a spawning player starts at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnHealth {
    /// The player's stored max health.
    Stored,
    /// The default full health, whatever max health is stored.
    Default,
    /// Health and max health are the dvar's value: its registered default
    /// when the match does not set it, clamped to its range.
    Dvar {
        name: &'static str,
        default: i32,
        range: (i32, i32),
    },
}

/// A game's dynamic entity physics parameters (`dynEnt_*` and `phys_bullet*`
/// dvars) and type properties.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DynEntPhysicsRules {
    pub bullet_force: f32,
    pub bullet_upbias: f32,
    pub bullet_spin_scale: f32,
    pub explode_force: f32,
    pub explode_upbias: f32,
    pub explode_spin_scale: f32,
    pub explode_min_force: f32,
    pub explode_max_ents: usize,
    pub spawned_limit: usize,
    pub sentient_auto_activate: bool,
    pub damage_radius_scale: f32,
    pub destruct_uses_physics: bool,
}

/// How a client's viewmodel plays a player's weapon animation number.
#[derive(Clone, Copy, Debug)]
pub struct ViewmodelRules {
    /// What a change of the weapon animation number plays; `named(i)` tells
    /// whether the weapon names its animation `i` (in the game's own order).
    /// The second value is what the change does that is not run.
    pub anim: ViewmodelAnimFn,
    /// The rate the game starts an animation at; `None` takes it from the
    /// weapon's timers as the runtime's viewmodel does.
    pub rate: Option<ViewmodelRateFn>,
    /// What a change of a dual-wield weapon's left-hand animation number
    /// plays (the left animations in the game's own order); `None` leaves
    /// the left hand to the runtime's viewmodel.
    pub left: Option<ViewmodelLeftFn>,
    /// Whether the player's own fields hide the viewmodel (it still
    /// animates); `None` never hides it.
    pub hidden: Option<fn(&[u8; movement::GAME_MOVE_BYTES]) -> bool>,
    /// The game's camera animations; `None` plays none and keeps the view
    /// at the player's eye.
    pub camera: Option<ViewmodelCamera>,
    /// How the game starts an animation that the runtime's viewmodel does
    /// not run, when the game has no rate rule.
    pub playback: Option<&'static Unknown>,
    /// What an animation the runtime's viewmodel holds no slot for does not
    /// do; `None` when it holds them all.
    pub unslotted: Option<&'static Unknown>,
    /// What a change of the weapon animation number plays, read with the
    /// weapon's and the player's own fields and a random draw; when set it
    /// stands in for `anim`.
    pub pick: Option<ViewmodelPickFn>,
    /// The seconds the game blends an animation it starts in over (the
    /// others out); `None` cuts to it.
    pub blend: Option<fn(anim: usize) -> f32>,
    /// A dual-wield weapon's left gun idles empty while its clip is.
    pub left_empty_idle: bool,
}

/// What [`ViewmodelPickFn`] reads.
pub struct ViewmodelPick<'a> {
    /// Whether the weapon names its animation `i` (in the game's own order).
    pub named: &'a dyn Fn(usize) -> bool,
    /// The weapon's definition and variant, in the game's own layout.
    pub def: &'a [u8],
    pub variant: &'a [u8],
    /// The player's own fields ([`movement::MovePlayer::game`]).
    pub game: &'a [u8; movement::GAME_MOVE_BYTES],
    /// A random draw for the game's own random pick.
    pub random: u32,
}

pub type ViewmodelPickFn =
    fn(weap_anim: u32, pick: &ViewmodelPick<'_>) -> (ViewmodelAnim, Option<&'static Unknown>);

pub type ViewmodelAnimFn =
    fn(weap_anim: u32, named: &dyn Fn(usize) -> bool) -> (ViewmodelAnim, Option<&'static Unknown>);

/// The left hand's animation for its number, as [`ViewmodelAnimFn`].
pub type ViewmodelLeftFn = ViewmodelAnimFn;

/// A game's camera animations: played on the viewmodel beside its
/// animations, they move a bone of it that places the first-person view.
#[derive(Clone, Copy, Debug)]
pub struct ViewmodelCamera {
    /// The bone whose pose (from the viewmodel's root at the eye) is the
    /// view.
    pub tag: &'static str,
    /// What a weapon animation number does to the camera animations after
    /// it starts its own (`named` as for [`ViewmodelAnimFn`]); any
    /// animation's start stops them as well; `game` is the player's own
    /// fields ([`movement::MovePlayer::game`]).
    pub anim: fn(
        weap_anim: u32,
        named: &dyn Fn(usize) -> bool,
        game: &[u8; movement::GAME_MOVE_BYTES],
    ) -> Option<CameraAnim>,
}

/// What a weapon animation number does to the camera animations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraAnim {
    /// Starts this one (in the game's own order).
    Start(usize),
    /// Stops them.
    Stop,
}

/// What the game's client knows when it starts a weapon animation.
#[derive(Clone, Copy, Debug)]
pub struct ViewmodelStart<'a> {
    /// The animation, in the game's own order.
    pub anim: usize,
    /// The animation's length, in msec, as the game's animation system
    /// reads it.
    pub length_ms: i32,
    /// The weapon's definition and variant, in the game's own layout.
    pub def: &'a [u8],
    pub variant: &'a [u8],
    /// The player's own fields ([`movement::MovePlayer::game`]).
    pub game: &'a [u8; movement::GAME_MOVE_BYTES],
}

pub type ViewmodelRateFn = fn(&ViewmodelStart<'_>) -> f32;

/// What a viewmodel does on a weapon animation change.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ViewmodelAnim {
    /// Idle once the animations playing have finished.
    Idle,
    /// Idle now.
    IdleNow,
    /// Idle now, blending in over this many seconds.
    IdleOver(f32),
    /// The weapon's animation by its index in the game's own order.
    Play(usize),
    /// What plays goes on.
    Keep,
}

/// Which of the code-drawn HUD pieces a mode shows, and how.
#[derive(Clone, Copy, Debug)]
pub struct HudRules {
    /// The code-drawn HUD (blood, reticle, damage icons, killfeed, names,
    /// hints, splash, playercard, flash, compass, score bar, scoreboard):
    /// Modern Warfare 2's pieces or the game's own.
    pub code_hud: Rule<hud::CodeHud>,
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
#[derive(Clone, Copy, Debug)]
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
    /// How the game's shellshock files are read.
    fn shellshock(&self) -> Rule<ShockFormat>;
}

/// How a game reads its `shock/<name>.shock` files.
#[derive(Clone, Copy)]
pub enum ShockFormat {
    /// Modern Warfare 2's `.shock` format, drawn by its shellshock screen.
    ModernWarfare2,
    /// The game's own reader.
    Game {
        parse: fn(&str) -> Result<ShockFile, String>,
        /// The shock a precached name without a file of its own plays.
        fallback: &'static str,
    },
}

/// A shellshock file in the shared screen's terms, as a game's own reader
/// gives it.
#[derive(Clone, Debug, PartialEq)]
pub struct ShockFile {
    pub screen: ShockScreen,
    /// The blur's blend time and the time it fades over at the end, in
    /// milliseconds.
    pub blur_blend_ms: i32,
    pub blur_fade_ms: i32,
    /// A flash's white and screen-shot fade times, in milliseconds.
    pub white_fade_ms: i32,
    pub shot_fade_ms: i32,
    pub look: ShockLook,
    /// The fraction of their speed a shocked player moves at.
    pub movement_scale: f32,
    /// A shocked player's torso plays the shellshock animation and the
    /// weapon's fire animation is skipped.
    pub animation: bool,
    /// The view kick, when the game's shocks kick the view.
    pub kick: Option<ShockKick>,
    /// The shock's own timed sound, when the game times it from the file.
    pub sound: Option<ShockSound>,
    /// The vision the shock blends the naked vision to.
    pub vision: Option<ShockVision>,
    /// The parts of the shock the runtime does not run.
    pub unrun: Vec<&'static Unknown>,
}

/// A shock's view kick: it fades over the shock's last `fade_ms`, walks the
/// kick noise `rate` steps per msec and is `radius` at full strength.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShockKick {
    pub fade_ms: i32,
    pub rate: f32,
    pub radius: f32,
    /// The view axis is turned with every length and product summed in
    /// index order (`hud_iw4::shellshock_perturb_axis`).
    pub ordered_sums: bool,
}

/// A shock's own sound: a loop on the shocked player faded in over
/// `fade_in_ms`, a reverb room (`room`, dry and wet levels) and a mix
/// snapshot while it plays, then an end sound once the shock is within
/// `-mod_end_delay_ms` of its end, the loop and room fading out over
/// `fade_out_ms`.
#[derive(Clone, Debug, PartialEq)]
pub struct ShockSound {
    pub loop_alias: String,
    pub end_alias: String,
    pub fade_in_ms: i32,
    pub fade_out_ms: i32,
    pub mod_end_delay_ms: i32,
    pub room: String,
    pub dry: f32,
    pub wet: f32,
    pub snapshot: String,
}

/// The vision a shock blends the naked vision to over `in_ms`, and back
/// from over `out_ms`.
#[derive(Clone, Debug, PartialEq)]
pub struct ShockVision {
    pub name: String,
    pub in_ms: i32,
    pub out_ms: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShockScreen {
    Blurred,
    Flashed,
    None,
}

/// How a shock slows the player's look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShockLook {
    pub affect: bool,
    pub fade_ms: i32,
    pub mouse_sensitivity: f32,
    pub max_pitch_speed: f32,
    pub max_yaw_speed: f32,
    /// While fading, the max speeds are multiplied by the fade's reciprocal
    /// instead of divided by the fade.
    pub reciprocal_fade: bool,
}

/// How a game's menus are drawn.
pub trait GameMenus: Sync {
    fn script_ui(&self, _modules: &[ui::Module]) -> Result<Option<Box<dyn ui::ScriptUi>>, String> {
        Ok(None)
    }

    fn front_end_ui(
        &self,
        _modules: &[ui::Module],
    ) -> Result<Option<Box<dyn ui::ScriptUi>>, String> {
        Ok(None)
    }

    /// The font a menu item's `textfont` enum names, at the item's scales.
    fn font(&self, font_enum: i32, placement_scale: f32, text_scale: f32) -> Rule<&'static str>;

    /// How the game's menu engine places and scales an item.
    fn layout(&self) -> Rule<&'static dyn MenuLayout>;

    /// The menu the in-game menu key (Escape) opens.
    fn ingame_menu(&self) -> Rule<InGameMenu>;

    /// The game's own dvars behind its menus' settings, which the runtime
    /// does not register.
    fn menu_dvars(&self) -> &'static [MenuDvar] {
        &[]
    }
}

/// A runtime setting a game's menus show and set through a dvar of their own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuSetting {
    MasterVolume,
    MouseSensitivity,
    FieldOfView,
}

/// One of a game's dvars behind its menus: its registered default, which the
/// menus show until set, and the runtime setting it stands for, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuDvar {
    pub name: &'static str,
    pub default: &'static str,
    pub setting: Option<MenuSetting>,
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

/// A picture of a game's own item paint, in the item's coordinates (its
/// rect's alignment places it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MenuQuad {
    /// `[x, y, w, h]`.
    pub rect: [f32; 4],
    pub color: [f32; 4],
    pub material: &'static str,
    /// The degrees the game's rotated picture turns about its centre
    /// (negative turns it counter-clockwise on screen); 0 for none.
    pub turn: f32,
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
    /// Aligned text of a dvar item of `item_type` leaves room in its rect
    /// for the width of the dvar's value.
    fn value_room(&self, _item_type: i32) -> bool {
        false
    }
    /// How many characters after a `^` a text's width skips as a colour or
    /// formatting code (0: the `^` is a glyph of its own); `next` and
    /// `after` are the two characters that follow it.
    fn text_code_len(&self, next: char, _after: Option<char>) -> usize {
        usize::from(next.is_ascii_digit())
    }
    /// The lines an autowrapped text breaks into in a rect `rect_w` wide,
    /// whose whole text measures `text_w`; `None` keeps the runtime's own
    /// line breaker. `glyph(c)` is a character's advance in font units and
    /// `scale` the font's glyph scale.
    fn wrap_lines(
        &self,
        _text: &str,
        _rect_w: f32,
        _align_mode: i32,
        _text_w: f32,
        _glyph: &dyn Fn(char) -> i32,
        _scale: f32,
    ) -> Option<Vec<String>> {
        None
    }
    /// What a slider item in `rect` `[x, y, w, h]` paints with its value
    /// `fraction` of the way through its range, its text aligned by
    /// `align_mode` and offset by `align_x`, `align_y`; `None` keeps the
    /// runtime's own bar and value text.
    fn slider(
        &self,
        _rect: [f32; 4],
        _align_mode: i32,
        _align_x: f32,
        _align_y: f32,
        _fraction: f32,
    ) -> Option<Vec<MenuQuad>> {
        None
    }
}

/// A mode the game library offers beside multiplayer, with the maps it runs on.
pub struct LibraryMode {
    pub gametype: &'static str,
    /// What the mode does and does not do yet, shown above its maps.
    pub note: &'static str,
    /// `(zone, title)`; a map is offered when its zone is installed.
    pub maps: &'static [(&'static str, &'static str)],
    /// The zone of the game's own front end: picking the game in the
    /// library starts it, and its menus choose the map (`None`: the
    /// library's own pages do).
    pub front_end: Option<&'static str>,
    /// Other gametypes and start locations offered on those maps.
    pub starts: &'static [LibraryStart],
    /// A patch zone the game downloads at launch that the mode's own
    /// scripts need, captured by the user (`None`: no such need).
    pub capture: Option<LibraryCapture>,
}

/// A downloaded patch zone a library mode needs captured: the files
/// `ffotd_*<suffix>*.ff`, needed by every map of the mode but the exempt.
pub struct LibraryCapture {
    pub suffix: &'static str,
    pub exempt: &'static [&'static str],
    /// What is missing, shown in the library.
    pub note: &'static str,
}

/// A gametype on one of a map's start locations, offered beside the map.
pub struct LibraryStart {
    pub zone: &'static str,
    pub gametype: &'static str,
    pub location: &'static str,
    pub title: &'static str,
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
