//! A game's mounted turrets: what its spawn keeps for a map turret, read
//! from the turret's spawn keys and its weapon, and the turret's own think
//! (targets, aim, firing), run through [`TurretWorld`].

use crate::Unknown;
use crate::fire::{FireTeam, FireWorld};
use crate::movement::GameWeapon;

/// How the game spawns and runs its map turrets.
#[derive(Clone, Copy, Debug)]
pub struct TurretRules {
    /// The map entity classname the game spawns as a turret.
    pub classname: &'static str,
    /// The turret its spawn keeps, from its spawn keys (`key` answers a
    /// float key the map sets, matched without case and read as C's `atof`)
    /// and its `weaponinfo` weapon (`None` when the match has none of that
    /// name), or the game's error.
    pub spawn: TurretSpawnFn,
    /// How many turrets a level can spawn.
    pub max: usize,
    /// How often the turret thinks, ms.
    pub think_ms: i32,
    /// The size of the game's sentient pool, whose slot order a turret's
    /// search goes in.
    pub sentients: usize,
    /// The turret's think: overheat, targets, aim and its firing state.
    pub think: TurretThinkFn,
    /// One shot of the turret's weapon (`shootturret`).
    pub shoot: TurretThinkFn,
    /// The bone angles the turret's gun angles pose its model with, as the
    /// game's turret controller sets them on the server and the clients.
    pub bone_controls: TurretBonesFn,
    /// What `restoredefaultdroppitch` does that is not run.
    pub restore_drop_pitch: &'static Unknown,
    /// What a player using a turret does that is not run.
    pub player_use: &'static Unknown,
}

pub type TurretSpawnFn = fn(
    weapon: Option<&GameWeapon>,
    weapon_name: &str,
    key: &dyn Fn(&str) -> Option<f32>,
    has_tag: &dyn Fn(&str) -> bool,
) -> Result<TurretSpawn, String>;

/// Runs the turret against the match; returns what it reached that the game
/// rules do not run.
pub type TurretThinkFn =
    fn(turret: &mut GameTurret, world: &mut dyn TurretWorld) -> Vec<&'static Unknown>;

/// The bones the game's turret controller turns and their control angles
/// (degrees), for these gun angles on a model with these tags.
pub type TurretBonesFn =
    fn(gun_angles: [f32; 3], has_tag: &dyn Fn(&str) -> bool) -> Vec<(&'static str, [f32; 3])>;

/// Rules are told apart by what they spawn and leave unrun: function
/// addresses are not stable to compare.
impl PartialEq for TurretRules {
    fn eq(&self, other: &Self) -> bool {
        self.classname == other.classname
            && self.max == other.max
            && self.think_ms == other.think_ms
            && self.sentients == other.sentients
            && self.restore_drop_pitch == other.restore_drop_pitch
            && self.player_use == other.player_use
    }
}

/// A turret as the game's spawn leaves it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurretSpawn {
    /// The game's own turret flag bits (mode, usable, firing, …).
    pub flags: u32,
    /// The game's team number.
    pub team: i32,
    /// Pitch up and yaw right limits (degrees, at most 0) and pitch down and
    /// yaw left limits (at least 0).
    pub arcs: TurretArcs,
    /// The yaw limits as spawned, which the arc methods do not change.
    pub spawned_yaw: [f32; 2],
    /// The cosine of the widest angle off its gun a shot may aim.
    pub aim_cone_cos: f32,
    /// The pitch the turret drops to when unmanned.
    pub drop_pitch: f32,
    /// Pitch and yaw convergence times, ms.
    pub convergence_ms: [i32; 2],
    pub suppression_ms: i32,
    /// The square of the turret's range.
    pub max_range_squared: f32,
    pub accuracy: f32,
    pub ai_spread: f32,
    pub player_spread: f32,
    /// The entity's health when the map gave it none.
    pub default_health: i32,
}

/// A turret's aim limits, degrees.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TurretArcs {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

/// A turret the match keeps and the game's rules run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameTurret {
    /// Its entity number.
    pub entity: i32,
    /// Its `weaponinfo` weapon.
    pub weapon: u32,
    /// Its spawn, as its methods changed it since.
    pub spawn: TurretSpawn,
    /// What its think keeps.
    pub state: TurretState,
}

/// What a turret's think keeps between thinks, in the game's terms.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TurretState {
    /// The firing state; nonzero while the turret fires (`isfiringturret`).
    pub firing: i32,
    /// `settargetentity`'s entity.
    pub manual_target: Option<i32>,
    /// The entity the think aims at.
    pub target: Option<i32>,
    /// When it took that target, ms.
    pub target_time: i32,
    /// The sentient slot its search last looked at; negative before the
    /// first.
    pub cursor: i32,
    /// The direction its converging aim starts off the target by.
    pub miss: [f32; 3],
    /// The point it last aimed at.
    pub aim_point: [f32; 3],
    pub heat: f32,
    pub overheating: bool,
    /// How long its fire loop sound has left, ms.
    pub fire_sound_ms: i32,
    /// The barrel a two-barrelled turret fires next.
    pub barrel: i32,
    /// The entity number the game keeps as the turret's user; a search
    /// passes it by.
    pub user: i32,
    /// `setturretowner`'s entity.
    pub owner: Option<i32>,
    /// Its gun's pitch, yaw and pitch offset, relative to its base angles.
    pub gun_angles: [f32; 3],
}

/// A tag on a posed model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurretTag {
    pub origin: [f32; 3],
    /// Forward, left and up.
    pub axis: [[f32; 3]; 3],
}

/// An entity that sees and is seen (a player or an actor), as the game's
/// sentient pool keeps it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sentient {
    pub entity: i32,
    pub team: Option<FireTeam>,
    /// `ignoreme`.
    pub ignore_me: bool,
    pub origin: [f32; 3],
    pub eye: [f32; 3],
    /// Its bounds about its origin.
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    /// A player's movement type; `None` for an actor.
    pub pm_type: Option<i32>,
}

/// The match around a turret's think.
pub trait TurretWorld: FireWorld {
    /// The turret's origin and base angles.
    fn turret_pose(&mut self, turret: i32) -> ([f32; 3], [f32; 3]);

    /// A tag on the turret's model with its gun at these angles, posed by
    /// [`TurretRules::bone_controls`]; `None` when the model lacks it.
    fn turret_tag(&mut self, turret: i32, tag: &str, gun_angles: [f32; 3]) -> Option<TurretTag>;

    /// The sentient in a slot of the game's pool
    /// ([`TurretRules::sentients`]); `None` for a free slot.
    fn sentient(&mut self, slot: usize) -> Option<Sentient>;

    /// The sentient an entity is.
    fn sentient_of(&mut self, entity: i32) -> Option<Sentient>;

    /// The turret entity's loop sound, an alias, or none.
    fn turret_loop_sound(&mut self, turret: i32, alias: Option<&str>);

    /// A sound alias played at the turret entity.
    fn turret_sound(&mut self, turret: i32, alias: &str);

    /// An effect the clients play on a tag of the turret's model.
    fn turret_tag_effect(&mut self, turret: i32, effect: &str, tag: &str);

    /// A turret's shot from one of its barrels (0 or 1) at `origin`, for
    /// the clients' fire sound, which their fire rules pick and time.
    fn turret_fire_sound(&mut self, turret: i32, weapon: u32, barrel: i32, origin: [f32; 3]);

    /// A camera shake the clients near `origin` feel: its scale, how long
    /// it lasts (ms) and how far it reaches.
    fn shake_camera(&mut self, origin: [f32; 3], scale: f32, duration_ms: i32, radius: f32);
}
