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
    pub report: Vec<String>,
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
