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
    /// What was hit is a piece of breakable glass.
    pub glass: bool,
}

/// One slide through the physics world.
#[derive(Clone, Copy, Debug)]
pub struct PhysicsSlide {
    pub origin: [f32; 3],
    pub velocity: [f32; 3],
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub contentmask: u32,
    pub frametime: f32,
    /// The gravity pulling the player through the slide, when it does.
    pub gravity: Option<f32>,
    /// A player walking on the ground stays on it.
    pub stick_to_ground: bool,
}

/// Where the slide ended.
#[derive(Clone, Copy, Debug)]
pub struct PhysicsSlid {
    pub origin: [f32; 3],
    pub velocity: [f32; 3],
    /// The slide met something.
    pub contacted: bool,
    /// What it met, in the order met.
    pub touched: [i32; 32],
    pub touched_count: usize,
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
    /// The variant's animation slots that name an animation, by slot.
    pub named_anims: u128,
    pub name: &'a str,
}

impl Weapon<'_> {
    /// The variant names a melee animation.
    pub fn has_melee_anim(&self) -> bool {
        self.named_anims & 1 << fastfile_t5::size::weap_anim::MELEE != 0
    }
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

    /// Whether the player sweeps through Black Ops' physics world, its
    /// default collision, rather than the clip map.
    fn physics_collision(&self) -> bool {
        false
    }

    /// The player's sweep through the physics world, ignoring
    /// `pass_entity`.
    fn physics_trace(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        pass_entity: i32,
        contentmask: u32,
    ) -> Trace {
        self.trace(start, end, mins, maxs, pass_entity, contentmask)
    }

    /// The ground sweep through the physics world: of the contacts near
    /// the earliest, a walkable one is preferred.
    fn physics_ground_trace(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        pass_entity: i32,
        contentmask: u32,
    ) -> Trace {
        self.physics_trace(start, end, mins, maxs, pass_entity, contentmask)
    }

    /// The player's slide through the physics world; `None` when the match
    /// cannot run it.
    fn physics_slide(&self, _slide: &PhysicsSlide, _pass_entity: i32) -> Option<PhysicsSlid> {
        None
    }

    /// Whether the player may stand on `entity`.
    fn can_stand_on(&self, entity: i32) -> bool;

    /// The weapon's definition; weapon 0 is the game's weapon named none.
    fn weapon(&self, weapon: u32) -> Weapon<'_>;

    /// The weapon number names a weapon the game loaded.
    fn weapon_exists(&self, weapon: u32) -> bool;

    /// The height of the water surface over `origin`, looking from `up`
    /// above to `down` below it.
    fn water_surface(&self, origin: [f32; 3], up: f32, down: f32) -> Option<f32>;

    /// What a precached shellshock does to movement.
    fn shellshock(&self, index: i32) -> Shellshock;

    /// The length of the named animation, in msec; `None` when it is not
    /// loaded.
    fn anim_length_ms(&self, name: &str) -> Option<i32>;

    /// The named animation's root translation `frac` of the way through.
    fn anim_root_delta(&self, name: &str, frac: f32) -> Option<[f32; 3]>;
}

/// The parts of a shellshock file movement reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shellshock {
    /// The fraction of their speed a shocked player moves at.
    pub movement: f32,
    /// The torso plays the shellshock animation and the weapon's fire
    /// animation is skipped.
    pub animation: bool,
}
