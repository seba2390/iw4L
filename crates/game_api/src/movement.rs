//! A game's own player movement, behind one trait: the simulation hands it
//! the player and the command and keeps whatever the game's movement keeps
//! for itself in [`MovePlayer::game`].

use crate::Unknown;

/// Bytes a game's movement keeps per player beyond the shared fields. The
/// simulation stores, replicates and rolls them back with the player.
pub const GAME_MOVE_BYTES: usize = 256;

/// The entity number for no entity: a player standing on nothing.
pub const ENTITY_NONE: i32 = -1;
/// The entity number for the world itself.
pub const ENTITY_WORLD: i32 = -2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MoveType {
    #[default]
    Normal,
    NormalLinked,
    Noclip,
    Ufo,
    Spectator,
    Intermission,
    LastStand,
    Dead,
    DeadLinked,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stance {
    #[default]
    Stand,
    Crouch,
    Prone,
}

/// The command buttons by what the player does, not by any game's bits.
pub mod buttons {
    pub const ATTACK: u32 = 1 << 0;
    pub const SPRINT: u32 = 1 << 1;
    pub const MELEE: u32 = 1 << 2;
    pub const USE: u32 = 1 << 3;
    pub const RELOAD: u32 = 1 << 4;
    pub const USE_RELOAD: u32 = 1 << 5;
    pub const LEAN_LEFT: u32 = 1 << 6;
    pub const LEAN_RIGHT: u32 = 1 << 7;
    pub const PRONE: u32 = 1 << 8;
    pub const CROUCH: u32 = 1 << 9;
    pub const JUMP: u32 = 1 << 10;
    pub const ADS: u32 = 1 << 11;
    pub const STANCE_HELD: u32 = 1 << 12;
    pub const HOLD_BREATH: u32 = 1 << 13;
    pub const FRAG: u32 = 1 << 14;
    pub const SMOKE: u32 = 1 << 15;
    pub const THROW: u32 = 1 << 16;
    pub const TALKING: u32 = 1 << 17;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MoveCommand {
    pub server_time: i32,
    pub buttons: u32,
    pub angles: [i32; 3],
    pub forwardmove: i8,
    pub rightmove: i8,
}

/// One player as movement sees it: the fields the rest of the match shares,
/// and the bytes the game's movement keeps for itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovePlayer {
    pub client_num: i32,
    pub command_time: i32,
    pub move_type: MoveType,
    pub origin: [f32; 3],
    pub velocity: [f32; 3],
    pub viewangles: [f32; 3],
    pub delta_angles: [f32; 3],
    pub gravity: i32,
    pub speed: i32,
    /// [`ENTITY_NONE`], [`ENTITY_WORLD`] or the entity's number.
    pub ground_entity: i32,
    pub weapon: u32,
    pub move_speed_scale: f32,
    /// A script froze the player's controls.
    pub frozen: bool,
    /// Set by movement: the eye height above the origin.
    pub view_height: f32,
    /// Set by movement: the eye height the stance settles at.
    pub view_height_target: i32,
    pub bob_cycle: i32,
    pub leanf: f32,
    pub movement_dir: i32,
    pub stance: Stance,
    pub sprinting: bool,
    pub game: [u8; GAME_MOVE_BYTES],
}

impl Default for MovePlayer {
    fn default() -> Self {
        Self {
            client_num: 0,
            command_time: 0,
            move_type: MoveType::Normal,
            origin: [0.0; 3],
            velocity: [0.0; 3],
            viewangles: [0.0; 3],
            delta_angles: [0.0; 3],
            gravity: 0,
            speed: 0,
            ground_entity: ENTITY_NONE,
            weapon: 0,
            move_speed_scale: 1.0,
            frozen: false,
            view_height: 0.0,
            view_height_target: 0,
            bob_cycle: 0,
            leanf: 0.0,
            movement_dir: 0,
            stance: Stance::Stand,
            sprinting: false,
            game: [0; GAME_MOVE_BYTES],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MoveTrace {
    pub normal: [f32; 3],
    pub fraction: f32,
    pub surface_flags: u32,
    pub contents: u32,
    /// The entity hit: [`ENTITY_WORLD`] when nothing else, [`ENTITY_NONE`]
    /// when nothing was hit.
    pub entity: i32,
    pub allsolid: bool,
    pub startsolid: bool,
    pub walkable: bool,
}

/// A weapon's fields movement reads, by their weapon file names.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MoveWeapon {
    pub move_speed_scale: f32,
    pub ads_move_speed_scale: f32,
    pub sprint_duration_scale: f32,
    pub sprint_scale: f32,
    pub ducked_sprint_scale: f32,
    pub dtp_scale: f32,
    pub blocks_prone: bool,
    pub freeze_movement_when_firing: bool,
    pub dual_wield: bool,
    pub ads_overlay_reticle: bool,
    pub offhand_slot: i32,
}

/// The water over a point, as far as the match knows it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WaterSurface {
    Dry,
    At(f32),
    /// The match cannot tell.
    Unknown,
}

/// The match around a moving player.
pub trait MoveWorld {
    /// Sweeps `mins`..`maxs` from `start` to `end` against what `mask`
    /// names, ignoring `pass_entity`.
    fn trace(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        pass_entity: i32,
        mask: u32,
    ) -> MoveTrace;
    fn is_player(&self, entity: i32) -> bool;
    /// The weapon's movement fields; `None` when its game's fields were not
    /// loaded for it.
    fn weapon(&self, weapon: u32) -> Option<MoveWeapon>;
    /// The water surface over `origin`, searched from `up` above to `down`
    /// below it.
    fn water_surface(&self, origin: [f32; 3], up: f32, down: f32) -> WaterSurface;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MoveContext {
    /// A client predicts its own player.
    pub predicting: bool,
}

/// What a movement step tells the match, in the game's own numbering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveSignal {
    Event { event: i32, parm: i32 },
    Anim { event: i32 },
    LegsMove { movetype: i32 },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MoveOutcome {
    pub walking: bool,
    /// The player's bounds after the move; `None` when the move did not run.
    pub bounds: Option<([f32; 3], [f32; 3])>,
    pub touched: Vec<i32>,
    pub signals: Vec<MoveSignal>,
    /// Parts of the game's movement the player reached that are not run.
    pub gaps: Vec<&'static Unknown>,
}

/// What a script may forbid a player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveRestriction {
    Jump,
    Sprint,
    Stand,
    Crouch,
    Prone,
    Lean,
    Ads,
}

/// A game's player movement and the player state it alone keeps.
pub trait PlayerMovement: Sync {
    /// Runs one command.
    fn pmove(
        &self,
        player: &mut MovePlayer,
        cmd: &MoveCommand,
        oldcmd: &MoveCommand,
        world: &dyn MoveWorld,
        context: MoveContext,
    ) -> MoveOutcome;

    /// What the server sets on the player before each command, from the
    /// match's dvars.
    fn think(&self, player: &mut MovePlayer, dvar: &dyn Fn(&str) -> Option<String>);

    /// A fresh player at spawn.
    fn spawn(&self, player: &mut MovePlayer);

    fn restrict(&self, player: &mut MovePlayer, what: MoveRestriction, allowed: bool);

    fn set_stance(&self, player: &mut MovePlayer, stance: Stance) -> MoveOutcome;

    /// Sets or clears a perk; false when the game has no perk of that name.
    fn set_perk(&self, player: &mut MovePlayer, perk: &str, on: bool) -> bool;

    fn has_perk(&self, player: &MovePlayer, perk: &str) -> bool;

    fn clear_perks(&self, player: &mut MovePlayer);
}

/// Whose player movement runs a match.
#[derive(Clone, Copy)]
pub enum MovementRules {
    /// The simulation's own movement, Modern Warfare 2's.
    Simulation,
    /// The game's own movement.
    Game(&'static dyn PlayerMovement),
}

impl core::fmt::Debug for MovementRules {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Simulation => f.write_str("Simulation"),
            Self::Game(_) => f.write_str("Game"),
        }
    }
}

impl PartialEq for MovementRules {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Simulation, Self::Simulation) => true,
            (Self::Game(a), Self::Game(b)) => core::ptr::addr_eq(*a, *b),
            _ => false,
        }
    }
}

impl Eq for MovementRules {}
