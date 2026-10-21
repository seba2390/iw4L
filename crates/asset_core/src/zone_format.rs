use crate::FamilyId;

/// How a game's zone file is recognised and turned into its content stream,
/// for a game whose zones `asset_transport` does not open itself.
#[derive(Clone, Copy)]
pub struct ZoneFormat {
    pub game: FamilyId,
    /// The bytes the file starts with.
    pub magic: &'static [u8],
    /// The little-endian word at offset 8.
    pub version: u32,
    pub decode: fn(&[u8]) -> Result<Vec<u8>, String>,
    /// The compiled script modules a decoded zone carries.
    pub scripts: fn(&[u8]) -> ZoneScripts,
    /// The zones, beside a map's own, whose script modules the map runs, in
    /// load order: a later zone's module replaces an earlier one's.
    pub script_zones: fn(&str) -> Vec<String>,
    /// The map entities a decoded zone carries (a map's own zone).
    pub entities: fn(&[u8]) -> ZoneEntities,
    /// The drawn world a decoded zone carries (a map's own zone).
    pub world: fn(&[u8]) -> ZoneWorld,
}

#[derive(Clone, Debug, Default)]
pub struct ZoneWorld {
    pub geometry: Option<WorldGeometry>,
    pub collision: Option<WorldCollision>,
    pub report: Vec<String>,
}

/// A map's collision: brushes found through a BSP tree, and triangles found
/// through bounding-box trees.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldCollision {
    /// Normal and distance.
    pub planes: Vec<[f32; 4]>,
    pub materials: Vec<CollisionMaterial>,
    pub brushes: Vec<CollisionBrush>,
    pub nodes: Vec<CollisionNode>,
    pub leaves: Vec<CollisionLeaf>,
    /// The brushes of every leaf and model, by `CollisionLeaf::first_brush`.
    pub leaf_brushes: Vec<u32>,
    pub models: Vec<CollisionModel>,
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
    /// Three bits per triangle, one per edge.
    pub walkable_edges: Vec<u8>,
    pub partitions: Vec<CollisionPartition>,
    pub boxes: Vec<CollisionBox>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CollisionMaterial {
    /// `None` when the zone refers to a name loaded elsewhere.
    pub name: Option<String>,
    pub surface_flags: u32,
    pub contents: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CollisionBrush {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub contents: u32,
    /// The six axial sides' surface flags: the minimum side of x, y and z,
    /// then the maximum side.
    pub axial_surface_flags: [u32; 6],
    /// The other sides: a plane index and its surface flags.
    pub sides: Vec<(u32, u32)>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CollisionNode {
    pub plane: u32,
    /// A child below zero is leaf `-1 - child`.
    pub children: [i32; 2],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CollisionLeaf {
    pub first_box: u16,
    pub box_count: u16,
    pub brush_contents: u32,
    pub terrain_contents: u32,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub first_brush: u32,
    pub brush_count: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CollisionModel {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub radius: f32,
    pub leaf: CollisionLeaf,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CollisionPartition {
    pub first_triangle: u32,
    pub triangle_count: u32,
}

/// A node of a triangle tree: a leaf (`child_count` 0) holds partition
/// `index`, an inner node its children from box `index`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CollisionBox {
    pub origin: [f32; 3],
    pub half_size: [f32; 3],
    pub material: u16,
    pub child_count: u16,
    pub index: u32,
}

/// One draw of the world: a run of triangles in the index buffer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldSurface {
    /// The surface's first vertex, as a byte offset into the position array.
    pub position_offset: u32,
    /// The same vertex as a byte offset into the attribute array.
    pub attribute_offset: u32,
    pub triangle_count: u16,
    pub first_index: u32,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    /// The surface's material, as the zone refers to it.
    pub material: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldGeometry {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Tangent and the bitangent sign.
    pub tangents: Vec<[f32; 4]>,
    pub colors: Vec<[u8; 4]>,
    pub texture_uvs: Vec<[f32; 2]>,
    /// Absolute vertex numbers, three per triangle, in surface order.
    pub indices: Vec<u32>,
    pub surfaces: Vec<WorldSurface>,
}

#[derive(Clone, Debug, Default)]
pub struct ZoneEntities {
    /// The entity text (`{ "classname" "worldspawn" … }`).
    pub text: Option<String>,
    pub report: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ZoneScripts {
    /// Module name (`scripts/…/x.gsc`) and its compiled bytes.
    pub modules: Vec<(String, Vec<u8>)>,
    /// Config raw files (`….cfg`) and their text.
    pub configs: Vec<(String, String)>,
    pub tables: Vec<ZoneTable>,
    /// Layouts of bit-packed buffers (a player's stats), by name.
    pub layouts: Vec<BitLayout>,
    /// Zombies barrier types.
    pub barriers: Vec<BarrierType>,
    pub weapons: Vec<ZoneWeapon>,
    pub report: Vec<String>,
}

/// A weapon as a zone defines it, for the game's script weapon table.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ZoneWeapon {
    pub name: String,
    pub display_name: String,
    /// The weapons the game registers along with this one, in its order.
    pub companions: Vec<String>,
    pub fire_sound: String,
    pub fire_sound_player: String,
    pub pickup_sound: String,
    pub pickup_sound_player: String,
    pub retrievable: bool,
    /// Listed among the game's watcher weapons.
    pub watcher: bool,
    pub clip_size: i32,
    pub start_ammo: i32,
    pub max_ammo: i32,
    /// Start and max ammo count clips, not rounds.
    pub ammo_count_clip_relative: bool,
    /// Holds no stock: all its ammo is in the clip.
    pub clip_only: bool,
    pub tank_life_time: i32,
    pub melee_damage: i32,
    /// The game's weapon type, class, inventory type, fire type and offhand
    /// slot, by their numbers.
    pub weap_type: i32,
    pub weap_class: i32,
    pub inventory_type: i32,
    pub fire_type: i32,
    pub offhand_slot: i32,
    pub alt_weapon: String,
    pub dual_wield: bool,
    pub dual_wield_weapon: String,
    /// World models by index; empty where none is set.
    pub world_models: Vec<String>,
    /// View models (`gunXModel`) by index; empty where none is set.
    pub view_models: Vec<String>,
    /// The tags its models hide.
    pub hide_tags: Vec<String>,
    /// Damage scales by hit location, in the game's order; empty when the
    /// weapon has none.
    pub location_damage: Vec<f32>,
    /// A turret weapon's pitch and yaw convergence times, seconds.
    pub turret_convergence: [f32; 2],
    /// A turret weapon's arcs in degrees: left, right, top, bottom.
    pub turret_arcs: [f32; 4],
    /// A turret weapon's spread in degrees for an AI's and a player's shots.
    pub turret_spread: [f32; 2],
    /// A turret weapon's turn rates in degrees a second: least pitch, least
    /// yaw, most pitch, most yaw.
    pub turret_turn_rates: [f32; 4],
    /// A turret weapon's range; 0 for none.
    pub turret_max_range: f32,
    /// A sentry's scan: speed in degrees a second, acceleration, and the
    /// pause at each end in ms.
    pub turret_scan: (f32, f32, i32),
    /// Damage by distance: six steps of damage and the range it holds to.
    pub damage_curve: [(i32, f32); 6],
    /// How a fire uses the barrels (the game's number).
    pub fire_barrels: i32,
    pub penetrate_type: i32,
    /// A bullet weapon's rifle (else pistol) bullets.
    pub rifle_bullet: bool,
    /// Its bullets' hits carry damage flag 2.
    pub armor_piercing: bool,
    pub bullet_bounce: bool,
    pub bullet_impact_explode: bool,
    /// Pellets laid on a level line; a single shot goes unspread.
    pub flat_spread: bool,
    /// Its projectile's impact revives a downed player.
    pub revives_on_impact: bool,
    /// How far a fired grenade flies before it arms; 0 for a thrown one.
    pub projectile_activate_dist: i32,
    pub overheat_weapon: bool,
    pub overheat_rate: f32,
    pub cooldown_rate: f32,
    pub overheat_end_val: f32,
    pub cool_while_firing: bool,
    pub spin_up_time: i32,
    pub spin_down_time: i32,
    /// Whether the variant's attachment table has a slot-0 entry (`BG_WeaponUsesAttachmentTable`).
    pub has_attachment_table: bool,
    /// Attachments supported by this weapon variant.
    pub attachments: Vec<ZoneWeaponAttachment>,
}

/// An attachment supported on a `ZoneWeapon`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ZoneWeaponAttachment {
    pub kind: u32,
    pub clip_size_scale: f32,
    pub clip_size: i32,
    pub fire_type: i32,
    pub alt_weapon: String,
    pub dual_wield_weapon: String,
}

/// A zombies barrier type: pieces that open and close by animation, and the
/// zombie animation states it offers. Flags are the game's words.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BarrierType {
    pub name: String,
    pub piece_count: i32,
    pub auto_hide_open_pieces: u32,
    pub taunts: u32,
    pub reach_through_attacks: u32,
    pub taunt_anim_state: String,
    pub reach_through_anim_state: String,
    pub attack_slots: i32,
    pub attack_slot_horz_offset: f32,
    pub pieces: Vec<BarrierPiece>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BarrierPiece {
    /// The default, alternate and upgraded models.
    pub models: [Option<String>; 3],
    /// The animations a piece plays opening and closing.
    pub open_anim: Option<BarrierAnim>,
    pub close_anim: Option<BarrierAnim>,
    pub anim_state: String,
    pub anim_substate: String,
    pub upgraded_lives: u32,
}

/// An animation by name, with its frame count and rate when known.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BarrierAnim {
    pub name: String,
    pub timing: Option<(u16, f32)>,
}

/// A string table a zone carries: its name and cells, row by row.
#[derive(Clone, Debug, Default)]
pub struct ZoneTable {
    pub name: String,
    pub columns: usize,
    pub rows: usize,
    pub cells: Vec<String>,
}

/// The layout of a bit-packed buffer: structs of members at bit offsets; the
/// first struct is the root.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BitLayout {
    pub name: String,
    /// The buffer's size in bits.
    pub bits: u32,
    pub structs: Vec<BitStruct>,
    /// Each enum's names, in order.
    pub enums: Vec<Vec<String>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BitStruct {
    pub name: String,
    pub members: Vec<BitMember>,
}

/// A member: its size in bits (all of an array), its offset in bits, the
/// game's type number, the struct it holds, the fraction bits of a fixed
/// point number, its array length and the enum that names its indices.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BitMember {
    pub name: String,
    pub bits: u32,
    pub offset: u32,
    pub kind: u32,
    pub external: i32,
    pub range: u32,
    pub array_size: i32,
    pub enum_index: i32,
}

/// Where a game whose zones `asset_transport` opens itself keeps the compiled
/// scripts its maps run as they are.
#[derive(Clone, Copy)]
pub struct ScriptZoneFormat {
    pub game: FamilyId,
    /// The zones whose scripts a map runs, the map's own among them, in load
    /// order: a later zone's module replaces an earlier one's.
    pub script_zones: fn(&str) -> Vec<String>,
    /// The compiled modules, configs and tables a decoded zone carries.
    pub scripts: fn(&[u8]) -> ZoneScripts,
}

impl ZoneFormat {
    pub fn recognises(&self, file: &[u8]) -> bool {
        file.starts_with(self.magic) && file.get(8..12) == Some(&self.version.to_le_bytes()[..])
    }
}

impl core::fmt::Debug for ZoneFormat {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ZoneFormat")
            .field("game", &self.game)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}
