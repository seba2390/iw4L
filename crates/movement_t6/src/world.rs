use fastfile_t6::weapon::WeaponDefView;

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
    pub glass: bool,
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

/// A weapon as Black Ops 2 reads it: its definition and variant, and what the
/// game resolves when it loads them.
#[derive(Clone, Copy, Debug)]
pub struct Weapon<'a> {
    pub def: WeaponDefView<'a>,
    pub ammo_index: i32,
    pub clip_index: i32,
    pub alt_weapon: u32,
    pub dual_wield_weapon: u32,
    pub named_anims: u128,
    pub name: &'a str,
}

impl Weapon<'_> {
    pub fn has_melee_anim(&self) -> bool {
        self.named_anims & (1 << fastfile_t6::weapon::weap_anim::MELEE) != 0
    }
}

/// The match around one player's movement.
pub trait MoveWorld {
    fn trace(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        pass_entity: i32,
        contentmask: u32,
    ) -> Trace;

    fn can_stand_on(&self, entity: i32) -> bool;

    fn weapon(&self, weapon: u32) -> Weapon<'_>;

    fn weapon_exists(&self, weapon: u32) -> bool;

    /// The weapon whose alternate or dual-wield chain loaded `weapon`'s
    /// variant (Black Ops 2's negative variant count); none for a weapon
    /// loaded on its own.
    fn variant_parent(&self, weapon: u32) -> Option<u32>;

    fn water_surface(&self, origin: [f32; 3], up: f32, down: f32) -> Option<f32>;

    /// The length in milliseconds of the player animation `name`; `None`
    /// when it is not loaded.
    fn anim_length_ms(&self, name: &str) -> Option<i32>;

    /// The root translation player animation `name` has made by `frac` of
    /// its length; `None` when it is not loaded.
    fn anim_root_delta(&self, name: &str, frac: f32) -> Option<[f32; 3]>;

    /// Whether the vertical line `reach` above and below `point` crosses one
    /// of the map's height-lock volumes; `None` when the map has none.
    fn height_lock(&self, point: [f32; 3], reach: f32) -> Option<bool>;

    /// Where the entity numbered `entity` is, as this side of the match
    /// keeps it; `None` when it is not in use.
    fn entity_origin(&self, entity: i32) -> Option<[f32; 3]>;

    /// The entity's velocity (its state's trajectory delta); `None` when
    /// the match cannot tell.
    fn entity_velocity(&self, entity: i32) -> Option<[f32; 3]>;

    /// The fraction of their speed a player shocked by the shellshock
    /// numbered `index` moves at (its file's `bg_shock_movement`); `None`
    /// when the match does not know it.
    fn shock_movement(&self, index: i32) -> Option<f32>;
}
