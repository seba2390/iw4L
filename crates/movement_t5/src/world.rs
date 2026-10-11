use fastfile_t5::weapon_def::WeaponDefView;

/// A swept-bounds collision result.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Trace {
    pub normal: [f32; 3],
    pub fraction: f32,
    pub surface_flags: u32,
    pub contents: u32,
    pub entity: i32,
    pub allsolid: bool,
    pub startsolid: bool,
    pub walkable: bool,
}

pub const SURF_SLICK: u32 = 0x2;
pub const SURF_NOFALLDAMAGE: u32 = 0x1;
pub const SURF_NOSTEPS: u32 = 0x2000;
pub const SURF_LADDER: u32 = 0x8;

impl Trace {
    pub fn surface_type(&self) -> i32 {
        ((self.surface_flags as i32) >> 20) & 0x3f
    }
}

/// A weapon as Black Ops reads it: its definition and variant, and what the
/// game resolves when it loads them.
#[derive(Clone, Copy, Debug)]
pub struct Weapon<'a> {
    pub def: WeaponDefView<'a>,
    /// Weapons with one ammo name share this number.
    pub ammo_index: i32,
    /// Weapons with one clip name share this number.
    pub clip_index: i32,
    /// The alternate weapon; 0 when none.
    pub alt_weapon: u32,
    /// The left-hand weapon when dual wielding; 0 when none.
    pub dual_wield_weapon: u32,
}

/// The match around one player's movement.
pub trait MoveWorld {
    /// Sweeps `mins`..`maxs` from `start` to `end`, ignoring `pass_entity`.
    fn trace(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        pass_entity: i32,
        contentmask: u32,
    ) -> Trace;

    /// Whether the player may stand on `entity`.
    fn can_stand_on(&self, entity: i32) -> bool;

    /// The weapon's definition; weapon 0 is the game's weapon named none.
    fn weapon(&self, weapon: u32) -> Weapon<'_>;

    /// The height of the water surface over `origin`, looking from `up`
    /// above to `down` below it.
    fn water_surface(&self, origin: [f32; 3], up: f32, down: f32) -> Option<f32>;
}
