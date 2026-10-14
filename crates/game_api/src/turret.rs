//! A game's mounted turrets: what its spawn keeps for a map turret, read
//! from the turret's spawn keys and its weapon.

use crate::Unknown;
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
    /// What the turret's own think (aiming, targets, firing, overheating)
    /// does that is not run.
    pub think: &'static Unknown,
    /// What a player using a turret does that is not run.
    pub player_use: &'static Unknown,
}

pub type TurretSpawnFn = fn(
    weapon: Option<&GameWeapon>,
    weapon_name: &str,
    key: &dyn Fn(&str) -> Option<f32>,
) -> Result<TurretSpawn, String>;

/// Rules are told apart by what they spawn and leave unrun: function
/// addresses are not stable to compare.
impl PartialEq for TurretRules {
    fn eq(&self, other: &Self) -> bool {
        self.classname == other.classname
            && self.max == other.max
            && self.think == other.think
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
