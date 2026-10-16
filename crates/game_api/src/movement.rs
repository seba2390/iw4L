//! A game's own player movement, behind one trait: the simulation hands it
//! the player and the command and keeps whatever the game's movement keeps
//! for itself in [`MovePlayer::game`].

use crate::Unknown;

/// Bytes a game's movement keeps per player beyond the shared fields. The
/// simulation stores, replicates and rolls them back with the player.
pub const GAME_MOVE_BYTES: usize = 576;

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

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MoveCommand {
    pub server_time: i32,
    pub buttons: u32,
    pub angles: [i32; 3],
    pub forwardmove: i8,
    pub rightmove: i8,
    /// The weapon the player asks to hold.
    pub weapon: u32,
    /// The offhand the player asks to throw.
    pub offhand: u32,
    /// The alternate weapon the player switched from.
    pub alt_weapon: u32,
    /// Where a melee lunge aims, found by the client: yaw and distance.
    pub melee_charge_yaw: f32,
    pub melee_charge_dist: u8,
}

/// The weapon slots a player has.
pub const HELD_WEAPONS: usize = 15;

/// A held weapon and its rounds: in its clip and beside it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeldWeapon {
    pub weapon: u32,
    pub clip: i32,
    pub stock: i32,
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
    /// A spectator may fly free (`allowspectateteam("freelook", …)`).
    pub free_look: bool,
    /// Set by movement: the eye height above the origin.
    pub view_height: f32,
    /// Set by movement: the eye height the stance settles at.
    pub view_height_target: i32,
    pub bob_cycle: i32,
    pub leanf: f32,
    pub movement_dir: i32,
    pub stance: Stance,
    pub sprinting: bool,
    /// The weapons held, by slot; the game's weapon rules spend, load and
    /// drop them.
    pub held: [HeldWeapon; HELD_WEAPONS],
    /// Set by the weapon rules: how far the sights are up (0 hip, 1 aimed).
    pub weapon_pos_frac: f32,
    /// Set by the weapon rules: hip fire spread, 0 to 255.
    pub aim_spread_scale: f32,
    /// Set by the weapon rules, in the game's numbering.
    pub weapon_state: i32,
    /// Set by the weapon rules, in the game's numbering.
    pub weap_anim: u32,
    pub offhand: u32,
    pub view_weapon: Option<u32>,
    pub grenade_time_left: i32,
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
            free_look: false,
            view_height: 0.0,
            view_height_target: 0,
            bob_cycle: 0,
            leanf: 0.0,
            movement_dir: 0,
            stance: Stance::Stand,
            sprinting: false,
            held: [HeldWeapon::default(); HELD_WEAPONS],
            weapon_pos_frac: 0.0,
            aim_spread_scale: 0.0,
            weapon_state: 0,
            weap_anim: 0,
            offhand: 0,
            view_weapon: None,
            grenade_time_left: 0,
            game: [0; GAME_MOVE_BYTES],
        }
    }
}

/// What a player's weapon is doing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WeaponActivity {
    pub firing: bool,
    pub meleeing: bool,
    pub throwing_grenade: bool,
    /// Raising or dropping a weapon.
    pub switching: bool,
}

/// A jump, landing or foliage step, with the surface's name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JumpLand {
    Jump(&'static str),
    /// A landing, and whether it hurt.
    Land(&'static str, bool),
    Foliage,
}

/// What a player's weapon notifies compare against, kept between frames.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WeaponNotifyMemory {
    pub weapon: u32,
    pub raise_pending: bool,
    pub firing: bool,
    pub night_vision: bool,
    pub dive: bool,
    pub sprint: bool,
}

/// A notify a server frame raises on a player for their weapon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponNotify {
    /// The held weapon changed: the new one, then the old.
    Change(u32, u32),
    /// The weapon being raised is up.
    ChangeComplete(u32),
    BeginFiring,
    EndFiring,
    NightVisionOn,
    NightVisionOff,
    /// A weapon starts coming up.
    Raising(u32),
    DiveStart,
    DiveEnd,
    SprintBegin,
    SprintEnd,
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
    /// What was hit is a piece of breakable glass.
    pub glass: bool,
}

/// A weapon as its own game's player movement reads it: the definition and
/// its variant in the game's own layout, and what the game resolves when it
/// loads them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameWeapon {
    pub def: Box<[u8]>,
    pub variant: Box<[u8]>,
    /// Weapons with one ammo name share this number.
    pub ammo_index: i32,
    /// Weapons with one clip name share this number.
    pub clip_index: i32,
    /// The alternate weapon; 0 when none.
    pub alt_weapon: u32,
    /// The weapon in the left hand when dual wielding; 0 when none.
    pub dual_wield_weapon: u32,
    /// The weapon whose alternate or dual-wield chain loads this variant.
    pub variant_parent: Option<u32>,
    /// The variant's animation slots that name an animation, by slot.
    pub named_anims: u128,
    /// The weapon's name in its game.
    pub name: String,
    /// The definition's damage multipliers by hit location, when it has its
    /// own.
    pub location_damage: Option<Box<[f32]>>,
    /// The definition's bounce coefficients along and across a surface, by
    /// surface type, when it has them.
    pub parallel_bounce: Option<Box<[f32]>>,
    pub perpendicular_bounce: Option<Box<[f32]>>,
    /// The effect a shot flashes in the world (not the first person's).
    pub world_flash: Option<String>,
    /// The fire loop and fire stop sound aliases, the world's then a
    /// player's, when the definition names them.
    pub fire_loop_sounds: [Option<String>; 2],
    pub fire_stop_sounds: [Option<String>; 2],
}

/// The water over a point, as far as the match knows it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WaterSurface {
    Dry,
    At(f32),
    /// The match cannot tell.
    Unknown,
}

/// Where a point's vertical line lies against one class of the map's volume
/// entities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VolumeColumn {
    /// The map places no such volume.
    NoVolumes,
    Inside,
    Outside,
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
    /// The weapon as its game defines it; `None` when the game's definition
    /// was not loaded for it.
    fn weapon(&self, weapon: u32) -> Option<&GameWeapon>;
    /// The water surface over `origin`, searched from `up` above to `down`
    /// below it.
    fn water_surface(&self, origin: [f32; 3], up: f32, down: f32) -> WaterSurface;
    /// The length in milliseconds of the game's animation `name`; `None`
    /// when it is not loaded.
    fn anim_length_ms(&self, _name: &str) -> Option<i32> {
        None
    }
    /// The root translation animation `name` has made by `frac` (0 to 1) of
    /// its length; `None` when it is not loaded.
    fn anim_root_delta(&self, _name: &str, _frac: f32) -> Option<[f32; 3]> {
        None
    }
    /// A line from `start` to `end` against the world's own collision alone
    /// (no entity), stopping at what `mask` names; `None` when the match
    /// cannot trace it.
    fn world_line(&self, _start: [f32; 3], _end: [f32; 3], _mask: u32) -> Option<MoveTrace> {
        None
    }
    /// Whether the vertical line `reach` above and below `point` crosses the
    /// brush model of one of the first `limit` map entities of `classname`
    /// (one per model).
    fn volume_column(
        &self,
        _classname: &str,
        _limit: usize,
        _point: [f32; 3],
        _reach: f32,
    ) -> VolumeColumn {
        VolumeColumn::Unknown
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MoveContext {
    /// A client predicts its own player.
    pub predicting: bool,
}

/// What a movement step tells the match, in the game's own numbering.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MoveSignal {
    Event {
        event: i32,
        parm: i32,
    },
    Anim {
        event: i32,
    },
    LegsMove {
        movetype: i32,
    },
    /// A mantle began: where it ends and how long it takes.
    MantleStart {
        end: [f32; 3],
        duration_ms: i32,
    },
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

/// Where the game's movement has the player, as script builtins ask.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MoveStatus {
    pub on_ladder: bool,
    pub mantling: bool,
    pub sprinting: bool,
    /// What is left of the damage timer, in milliseconds.
    pub damage_timer: i32,
    pub no_melee: bool,
    pub no_weapon_cycling: bool,
    /// Downed, with someone reviving him.
    pub being_revived: bool,
    /// The game's own state of the weapon in hand, for a test player's log.
    pub weapon_state: i32,
}

/// A player flag a script builtin sets in the game's own movement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptFlag {
    /// `allowmelee(false)`.
    NoMelee,
    /// `disableweaponcycling`.
    NoWeaponCycling,
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

    /// The game's own movement flies a free spectator.
    fn moves_spectators(&self) -> bool {
        false
    }

    /// A fresh player at spawn.
    fn spawn(&self, player: &mut MovePlayer);

    fn restrict(&self, player: &mut MovePlayer, what: MoveRestriction, allowed: bool);

    fn set_stance(&self, player: &mut MovePlayer, stance: Stance) -> MoveOutcome;

    /// What entering last stand does to the player in the game's movement:
    /// raises its down animation event and detaches a turret or vehicle.
    fn enter_last_stand(&self, _player: &mut MovePlayer) -> MoveOutcome {
        MoveOutcome::default()
    }

    /// What `reviveplayer` does to the player in the game's movement: clears
    /// `being_revived` and raises its get-up animation event.
    fn revive(&self, _player: &mut MovePlayer) -> MoveOutcome {
        MoveOutcome::default()
    }

    /// What `undolaststand` does to the player in the game's movement.
    fn undo_last_stand(&self, _player: &mut MovePlayer) {}

    /// What `startrevive` (`true`) and `stoprevive` (`false`) set on a
    /// downed player.
    fn set_being_revived(&self, _player: &mut MovePlayer, _being_revived: bool) {}

    /// Whether the player is on a ladder or mantling; `None` when the game's
    /// movement keeps neither.
    fn status(&self, _player: &MovePlayer) -> Option<MoveStatus> {
        None
    }

    /// Sets (`on`) or clears a script's player flag; answers whether it was
    /// set before, `None` when the game's movement has no such flag.
    fn set_script_flag(
        &self,
        _player: &mut MovePlayer,
        _flag: ScriptFlag,
        _on: bool,
    ) -> Option<bool> {
        None
    }

    /// A hit of `damage` points on the player's damage timer, which slows
    /// him while it runs down; `None` clears the timer.
    fn damage_timer(&self, _player: &mut MovePlayer, _damage: Option<i32>) {}

    /// The cursor hint names `setcursorhint` accepts, in their `ps.cursorHint`
    /// order starting at `0` (`HINT_NONE`); weapon hints start right after the
    /// last entry.
    fn cursor_hints(&self) -> &'static [&'static str] {
        &[
            "HINT_NONE",
            "HINT_NOICON",
            "HINT_ACTIVATE",
            "HINT_HEALTH",
            "HINT_FRIENDLY",
        ]
    }

    /// Sets or clears a perk; false when the game has no perk of that name.
    fn set_perk(&self, player: &mut MovePlayer, perk: &str, on: bool) -> bool;

    fn has_perk(&self, player: &MovePlayer, perk: &str) -> bool;

    /// What the player's weapon is doing, as the game's script builtins
    /// test it.
    fn weapon_activity(&self, player: &MovePlayer) -> WeaponActivity;

    /// A footstep a movement signal is, as the game's client scripts are
    /// told of it: its movement type and the surface's name.
    fn footstep(&self, _signal: &MoveSignal) -> Option<(&'static str, &'static str)> {
        None
    }

    /// A jump or a landing a movement signal is, as the game's client
    /// scripts are told of it.
    fn jump_land(&self, _signal: &MoveSignal) -> Option<JumpLand> {
        None
    }

    /// The weapon notifies a server frame raises on the player.
    fn weapon_notifies(
        &self,
        player: &MovePlayer,
        memory: &mut WeaponNotifyMemory,
    ) -> Vec<WeaponNotify>;

    fn clear_perks(&self, player: &mut MovePlayer);

    fn weapon_spin(&self, _player: &MovePlayer) -> f32 {
        0.0
    }

    fn set_weapon_spin(&self, _player: &mut MovePlayer, _spin: f32) {}

    fn weapon_heat(&self, _player: &MovePlayer, _weapon: u32) -> (f32, bool) {
        (0.0, false)
    }

    fn set_weapon_heat(
        &self,
        _player: &mut MovePlayer,
        _weapon: u32,
        _overheating: bool,
        _heat: f32,
        _end_val: f32,
        _overheat_weapon: bool,
    ) {
    }

    /// What the server does with the events a command raised: the shots
    /// they fire. `before` is the player as the command found it, `player`
    /// as it left it. Returns the parts the game reached that are not run.
    fn server_events(
        &self,
        _before: &MovePlayer,
        _player: &MovePlayer,
        _signals: &[MoveSignal],
        _world: &mut dyn crate::fire::FireWorld,
    ) -> Vec<&'static Unknown> {
        Vec::new()
    }

    /// A blast by the game's own rules; `None` when the game has none and
    /// the match applies its own.
    fn radius_damage(
        &self,
        _blast: &crate::fire::RadiusDamage,
        _world: &mut dyn crate::fire::FireWorld,
    ) -> Option<Vec<&'static Unknown>> {
        None
    }

    /// One frame of a missile the game spawned.
    fn run_missile(
        &self,
        _missile: &mut crate::fire::GameMissile,
        _world: &mut dyn crate::fire::FireWorld,
    ) -> (crate::fire::MissileRun, Vec<&'static Unknown>) {
        (crate::fire::MissileRun::Gone, Vec::new())
    }

    /// Resets a missile's detonation time (`resetmissiledetonationtime`):
    /// `after_ms` from now when given, else its weapon's fuse time when it
    /// has timed detonation.
    fn reset_missile_detonation_time(
        &self,
        _missile: &mut crate::fire::GameMissile,
        _after_ms: Option<i32>,
        _world: &dyn crate::fire::FireWorld,
    ) {
    }
}

/// Whose weapon rules run a match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponRules {
    /// The simulation's own, Modern Warfare 2's.
    Simulation,
    /// The game's own, run by its player movement: the match fires what its
    /// fire events ask for.
    Game,
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
