//! What a game's server does with its players' fire events: the game runs
//! its own shot (spread, bullets, penetration, damage) and the match traces
//! and applies it through [`FireWorld`].

use crate::Rule;
use crate::movement::{GameWeapon, MovePlayer};

/// What a bullet trace ran into, in the game's trace terms.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BulletHitType {
    /// Nothing: the bullet reached its end.
    #[default]
    None,
    /// An entity, or the world itself as [`crate::movement::ENTITY_WORLD`].
    Entity(i32),
    /// A breakable glass piece, by number.
    Glass(i32),
}

/// One bullet trace.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BulletTrace {
    pub fraction: f32,
    pub normal: [f32; 3],
    /// The surface's flags, the surface type among them, in the game's bits.
    pub surface_flags: u32,
    /// The contents of what it hit, in the game's bits.
    pub contents: u32,
    pub hit: BulletHitType,
    /// The hit location the trace reports, in the game's numbering (0 none).
    pub hit_location: i32,
    /// The bone hit, as the match numbers it on the entity's model; handed
    /// back with the damage.
    pub bone: Option<u16>,
    pub start_solid: bool,
    pub all_solid: bool,
}

/// A team, as the match keeps one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FireTeam {
    Axis,
    Allies,
    Neutral,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FireEntityKind {
    Player,
    Actor,
    Other,
}

/// An entity a shot or blast reaches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FireEntity {
    pub kind: FireEntityKind,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    /// What it is solid to, in the game's bits.
    pub contents: u32,
    /// Its linked bounds in the world, as the engine links it (sentients
    /// are linked one unit wider); `None` when it is not linked by bounds.
    pub bounds: Option<([f32; 3], [f32; 3])>,
    /// A brush model's own bounds, in its model space.
    pub model_bounds: Option<([f32; 3], [f32; 3])>,
    /// An actor's eye.
    pub eye: Option<[f32; 3]>,
    /// A script model: a blast reaches it by its model's bounds.
    pub script_model: bool,
    /// It has its own blast damage rule (a destructible).
    pub blast_handler: bool,
    pub takes_damage: bool,
    pub health: i32,
    pub max_health: i32,
    pub team: Option<FireTeam>,
    /// An actor's `ignorelocationaldamage`.
    pub ignore_locational_damage: bool,
    /// An actor's `allowdeath`.
    pub allow_death: bool,
    /// God mode: no damage at all.
    pub god: bool,
    /// Demigod: damage never takes the last point of health.
    pub demigod: bool,
    /// An actor's `type`, when it is one the game names.
    pub ai_type: Option<&'static str>,
}

/// Damage a shot does to one entity, as the game settled it.
#[derive(Clone, Debug, PartialEq)]
pub struct FireDamage {
    pub target: i32,
    pub inflictor: i32,
    pub attacker: i32,
    pub dir: [f32; 3],
    pub point: [f32; 3],
    pub amount: i32,
    pub flags: i32,
    /// The means of death, by its script name.
    pub means: &'static str,
    pub weapon: u32,
    /// The hit location, by its script name and its number.
    pub hit_location: &'static str,
    pub location: i32,
    pub bone: Option<u16>,
}

/// A bullet's mark on what it hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BulletImpact {
    pub attacker: i32,
    pub weapon: u32,
    pub start: [f32; 3],
    pub point: [f32; 3],
    pub normal: [f32; 3],
    pub surface_type: u8,
    pub surface_flags: u32,
    /// The entity hit; [`crate::movement::ENTITY_NONE`] for none.
    pub entity: i32,
    /// The game's impact flags (a head hit, a kill, an exit).
    pub flags: u8,
    /// Where the bullet left a surface it went through.
    pub exit: bool,
    /// The game's server sends this impact; otherwise its client draws it.
    pub from_server: bool,
}

/// The match around a shot.
pub trait FireWorld {
    /// The match's time, in milliseconds.
    fn level_time(&self) -> i32;

    /// A dvar's value; `None` when the match does not keep it.
    fn dvar(&self, name: &str) -> Option<String>;

    /// The server's shared random draw, 0 to 32767: the C library's
    /// generator over the match's state.
    fn rand(&mut self) -> i32;

    fn weapon(&self, weapon: u32) -> Option<&GameWeapon>;

    /// Traces a bullet from `start` to `end`, passing through `pass_entity`.
    fn bullet_trace(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        pass_entity: i32,
        mask: u32,
    ) -> BulletTrace;

    fn entity(&mut self, entity: i32) -> Option<FireEntity>;

    /// How deep a bullet of `penetrate_type` goes into `surface_type`, from
    /// the game's penetration table; `None` when the match has no table.
    fn penetration_depth(&self, penetrate_type: i32, surface_type: i32) -> Option<f32>;

    /// The damage multiplier at a hit location, from the game's table; `None`
    /// when the match has no table.
    fn hit_location_multiplier(&self, hit_location: i32) -> Option<f32>;

    /// Hands damage to the entity: an actor's damage callback, a player's,
    /// or the entity's own health and notifies.
    fn damage(&mut self, damage: &FireDamage);

    /// Damage triggers the bullet's path crosses.
    fn damage_triggers(
        &mut self,
        attacker: i32,
        start: [f32; 3],
        end: [f32; 3],
        amount: i32,
        means: &'static str,
    );

    fn impact(&mut self, impact: &BulletImpact);

    /// The entities whose bounds touch the box.
    fn entities_in_box(&mut self, mins: [f32; 3], maxs: [f32; 3]) -> Vec<i32>;

    /// True when a sight trace from `start` to `end` through what `mask`
    /// names reaches it, passing through the two entities.
    fn sight_clear(&mut self, start: [f32; 3], end: [f32; 3], pass: [i32; 2], mask: u32) -> bool;

    /// A player, as its movement keeps it.
    fn player(&self, entity: i32) -> Option<MovePlayer>;

    /// A script asked that script blasts pass players by.
    fn players_ignore_radius_damage(&self) -> bool;

    /// A script turned off grenade suicide: a grenade held past its fuse
    /// falls at the thrower's feet and hurts them instead of killing them
    /// outright.
    fn grenade_suicide_disabled(&self) -> bool;

    /// Puts a game missile in the match; returns its entity number, or
    /// `None` when the match has no room for it.
    fn spawn_missile(&mut self, missile: GameMissile) -> Option<i32>;

    /// Adds to a player's velocity.
    fn push_player(&mut self, entity: i32, velocity: [f32; 3]);

    /// Stores a player as its movement keeps it, after the game changed it.
    fn store_player(&mut self, entity: i32, player: &MovePlayer);

    /// A missile's explosion, as the clients show it.
    fn explosion(&mut self, explosion: &MissileExplosion);

    /// A grenade's bounce sound off a surface type, as clients play it.
    fn bounce(&mut self, missile: i32, surface_type: u8);

    /// The match's time one frame ago.
    fn previous_level_time(&self) -> i32;

    /// Notifies `name` on an entity with these arguments.
    fn notify(&mut self, entity: i32, name: &str, args: &[NotifyArg]);

    /// The map's centre a script set; missiles too far from it are gone.
    fn map_center(&self) -> [f32; 3];

    /// The contents at a point, of the world and the entities there.
    fn point_contents(&mut self, point: [f32; 3], mask: u32) -> u32;

    /// A player's weapon event, sent to every client: the player's entity,
    /// the weapon in its hand when the event is sent.
    fn weapon_event(&mut self, entity: i32, weapon: u32, event: WeaponEvent);

    /// Updates an entity's script origin and angles before a notify wakes
    /// threads that read them.
    fn set_entity_pose(&mut self, _entity: i32, _origin: [f32; 3], _angles: [f32; 3]) {}
}

/// How a game launches, moves and detonates its missiles, even while a mode
/// borrows the simulation's weapon state machine for the throw itself.
pub trait MissilePhysics: Sync {
    /// Launches a grenade from `origin` aimed along `angles` (`launcher` when
    /// fired from a grenade launcher, `remaining_fuse` when cooked in hand);
    /// returns the entity number `FireWorld::spawn_missile` gave it.
    #[allow(clippy::too_many_arguments)]
    fn launch_grenade(
        &self,
        owner: i32,
        weapon: u32,
        origin: [f32; 3],
        angles: [f32; 3],
        launcher: bool,
        remaining_fuse: Option<i32>,
        world: &mut dyn FireWorld,
    ) -> (Option<i32>, Vec<&'static crate::Unknown>);

    /// Spawns a grenade at `origin` with an explicit initial `velocity` and
    /// `fuse` in milliseconds (`MagicGrenadeType`).
    fn spawn_grenade(
        &self,
        owner: i32,
        weapon: u32,
        origin: [f32; 3],
        velocity: [f32; 3],
        fuse: i32,
        world: &mut dyn FireWorld,
    ) -> (Option<i32>, Vec<&'static crate::Unknown>);

    /// Launches a projectile weapon's rocket from `origin` aimed along
    /// `angles` with `spread` degrees and the shooter's `velocity`.
    #[allow(clippy::too_many_arguments)]
    fn launch_rocket(
        &self,
        owner: i32,
        weapon: u32,
        origin: [f32; 3],
        angles: [f32; 3],
        spread: f32,
        velocity: [f32; 3],
        world: &mut dyn FireWorld,
    ) -> (Option<i32>, Vec<&'static crate::Unknown>);

    /// Runs one frame of a missile.
    fn run(
        &self,
        missile: &mut GameMissile,
        world: &mut dyn FireWorld,
    ) -> (MissileRun, Vec<&'static crate::Unknown>);

    /// Detonates a missile (`detonate`), with an optional new owner.
    fn detonate(
        &self,
        missile: &mut GameMissile,
        attacker: Option<i32>,
        world: &mut dyn FireWorld,
    ) -> (MissileRun, Vec<&'static crate::Unknown>);

    /// Marks a grenade a dud (`makegrenadedud`).
    fn make_dud(&self, missile: &mut GameMissile);

    /// Resets a missile's detonation time (`resetmissiledetonationtime`):
    /// `after_ms` from now when given, else its weapon's fuse time when it
    /// has timed detonation.
    fn reset_detonation_time(
        &self,
        missile: &mut GameMissile,
        after_ms: Option<i32>,
        world: &dyn FireWorld,
    );

    /// The entity number a resting missile sits on (`getgroundent`), or
    /// `ENTITY_NONE` while flying.
    fn ground_entity(&self, missile: &GameMissile) -> i32 {
        let _ = missile;
        crate::movement::ENTITY_NONE
    }
}

#[derive(Clone, Copy)]
pub struct MissileRules(pub &'static dyn MissilePhysics);

impl core::fmt::Debug for MissileRules {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("MissileRules")
    }
}

impl PartialEq for MissileRules {
    fn eq(&self, other: &Self) -> bool {
        core::ptr::addr_eq(self.0, other.0)
    }
}

impl Eq for MissileRules {}

/// A player's weapon event the clients present with the weapon's sounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponEvent {
    /// A shot, from the left hand or the right; the clip's last round.
    Fire {
        left: bool,
        last_shot: bool,
    },
    Reload,
    ReloadFromEmpty,
    ReloadStart,
    ReloadEnd,
    Raise,
    FirstRaise,
    Putaway,
    /// The switch to the weapon's alternate mode.
    Alt,
    /// A grenade's pin is pulled.
    Pullback,
    Rechamber,
}

/// What a client does when a player fires, by the game's own rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClientFireRules {
    /// The fire sound a shot plays, from the weapon's definition and
    /// variant in the game's layout; `None` when the shot plays none.
    pub sound: fn(def: &[u8], variant: &[u8], shot: &FireSoundShot) -> Rule<Option<FireSound>>,
    /// The muzzle flash, shell and tracer a shot shows.
    pub effects: Rule<()>,
    /// A left-hand shot sounds the left-hand weapon of the weapon the
    /// client's own player holds, not the shooter's.
    pub left_from_own_weapon: bool,
}

/// A shot as the client that plays its sound sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FireSoundShot {
    /// The client views the shooter in first person.
    pub player_view: bool,
    pub left: bool,
    pub last_shot: bool,
    /// The client is watching a killcam.
    pub killcam: bool,
    /// The weapon names a last shot sound for this view.
    pub last_shot_sound: bool,
}

/// Which of a weapon's fire sounds a shot plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FireSound {
    Fire,
    FirePlayer,
    LastShot,
    LastShotPlayer,
}

/// A blast: damage falling off with distance from its origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadiusDamage {
    pub origin: [f32; 3],
    pub inflictor: i32,
    pub attacker: i32,
    /// Damage at the origin and at the edge.
    pub inner: f32,
    pub outer: f32,
    pub radius: f32,
    /// Only what lies within this cone (cosine, direction).
    pub cone: Option<(f32, [f32; 3])>,
    /// The entity the blast passes by.
    pub ignore: i32,
    /// The means of death, by its script name.
    pub means: &'static str,
    pub weapon: u32,
    /// The blast comes from a script's RadiusDamage.
    pub script: bool,
}

/// How a trajectory moves.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum TrajectoryKind {
    #[default]
    Stationary,
    Linear,
    /// Falling at this many units per second squared.
    Gravity(f32),
}

/// A position (or angles) over time: from `base` at `time`, moving by
/// `delta` per second as `kind` says.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GameTrajectory {
    pub kind: TrajectoryKind,
    pub time: i32,
    pub duration: i32,
    pub base: [f32; 3],
    pub delta: [f32; 3],
}

/// Bytes a game keeps per missile beyond the shared fields.
pub const GAME_MISSILE_BYTES: usize = 64;

/// A missile the game's rules move and explode; the match keeps it, shows
/// it and runs it every frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameMissile {
    /// Its entity number; set by the match when it spawns.
    pub entity: i32,
    /// The player that fired it.
    pub owner: i32,
    pub weapon: u32,
    pub position: GameTrajectory,
    pub angles: GameTrajectory,
    /// Where it is now.
    pub origin: [f32; 3],
    /// Clients draw it from this time on.
    pub visible_at: i32,
    pub game: [u8; GAME_MISSILE_BYTES],
}

/// What a missile's frame did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MissileRun {
    Flying,
    /// It exploded here (the match tells scripts) and is gone.
    Exploded([f32; 3]),
    /// It is gone without an explosion scripts hear of.
    Gone,
}

/// A missile's explosion effect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MissileExplosion {
    pub missile: i32,
    pub owner: i32,
    pub weapon: u32,
    pub origin: [f32; 3],
    pub normal: [f32; 3],
    pub surface_type: u8,
}

/// An argument a game's notify passes to scripts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NotifyArg {
    /// A weapon, by its script name.
    Weapon(u32),
    Entity(i32),
    Vector([f32; 3]),
    Float(f32),
    Int(i32),
    String(&'static str),
}
