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

/// What a weapon tells movement (its weapon file fields).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponMove {
    pub move_speed_scale: f32,
    pub ads_move_speed_scale: f32,
    pub sprint_duration_scale: f32,
    pub sprint_scale: f32,
    pub ducked_sprint_scale: f32,
    pub dtp_scale: f32,
    pub blocks_prone: bool,
    pub freeze_movement_when_firing: bool,
    pub dual_wield: bool,
    /// `adsOverlayReticle` is set: aiming looks through a scope overlay.
    pub ads_overlay: bool,
    pub offhand_slot: i32,
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

    fn weapon(&self, weapon: u32) -> WeaponMove;

    /// The height of the water surface over `origin`, looking from `up`
    /// above to `down` below it.
    fn water_surface(&self, origin: [f32; 3], up: f32, down: f32) -> Option<f32>;
}
