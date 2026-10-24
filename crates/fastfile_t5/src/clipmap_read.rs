//! A Black Ops clip map's collision tables read out of the zone: brushes,
//! the collision mesh and the BSP, row by row.

use alloc::string::String;
use alloc::vec::Vec;

use crate::size as sz;
use crate::zone::{ClipMapGeometry, Ptr, ZonePtr, ZoneStream};

const BRUSH: usize = 96;
const BRUSH_MINS: usize = 0;
const BRUSH_CONTENTS: usize = 12;
const BRUSH_MAXS: usize = 16;
const BRUSH_NUMSIDES: usize = 28;
const BRUSH_SIDES: usize = 32;
const BRUSH_AXIAL_SFLAGS: usize = 0x3c;
const CBRUSH_SIDE: usize = 12;
const CBRUSH_SIDE_SFLAGS: usize = 8;

/// Why a table did not read: a field ran past the zone, or a table the
/// collision needs is not there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipReadError {
    Truncated,
    MissingTables,
}

use ClipReadError::{MissingTables, Truncated};

/// A `cbrush_t`: its box, contents, the axial sides' surface flags (in the
/// zone's order), and its other sides that have a plane.
#[derive(Clone, Debug)]
pub struct ClipBrushRead {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub contents: u32,
    pub axial_sflags: [u32; 6],
    pub sides: Vec<([f32; 4], u32)>,
}

#[derive(Clone, Copy, Debug)]
pub struct ClipAabbRead {
    pub origin: [f32; 3],
    pub half_size: [f32; 3],
    pub material_index: u16,
    pub child_count: u16,
    pub u: i32,
}

/// The collision mesh's tables.
#[derive(Clone, Debug, Default)]
pub struct ClipMeshRead {
    pub verts: Vec<[f32; 3]>,
    pub tri_indices: Vec<u16>,
    pub tri_edge_is_walkable: Vec<u8>,
    /// Each material's name, surface flags and content flags.
    pub materials: Vec<(String, u32, u32)>,
    /// Each partition's triangle count and first triangle.
    pub partitions: Vec<(u8, i32)>,
    pub aabb_trees: Vec<ClipAabbRead>,
}

/// A `cLeaf_t`: its collision AABB range and its leaf-brush node.
#[derive(Clone, Copy, Debug)]
pub struct ClipLeafRead {
    pub first_coll_aabb_index: u16,
    pub coll_aabb_count: u16,
    pub leaf_brush_node: i32,
}

/// A `cmodel_t`.
#[derive(Clone, Copy, Debug)]
pub struct ClipModelRead {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub radius: f32,
    pub leaf_brush_node: i32,
}

/// A `cLeafBrushNode_t`: its brushes when it holds any, else its two child
/// offsets.
#[derive(Clone, Debug)]
pub struct ClipLeafBrushNodeRead {
    pub count: i16,
    pub brushes: Vec<u16>,
    pub children: [u16; 2],
}

#[derive(Clone, Copy)]
pub struct ClipMapView<'s, 'z> {
    s: &'s ZoneStream<'z>,
    g: ClipMapGeometry,
}

impl<'z> ZoneStream<'z> {
    pub fn clip_map_view<'s>(&'s self, g: ClipMapGeometry) -> ClipMapView<'s, 'z> {
        ClipMapView { s: self, g }
    }
}

impl ClipMapView<'_, '_> {
    pub fn geometry(&self) -> &ClipMapGeometry {
        &self.g
    }

    fn f32(&self, p: Ptr, off: usize) -> Result<f32, ClipReadError> {
        self.s.f32_at(p, off).map_err(|_| Truncated)
    }

    fn u32(&self, p: Ptr, off: usize) -> Result<u32, ClipReadError> {
        self.s.u32_at(p, off).map_err(|_| Truncated)
    }

    fn u16(&self, p: Ptr, off: usize) -> Result<u16, ClipReadError> {
        self.s.u16_at(p, off).map_err(|_| Truncated)
    }

    fn i32(&self, p: Ptr, off: usize) -> Result<i32, ClipReadError> {
        self.s.i32_at(p, off).map_err(|_| Truncated)
    }

    fn i16(&self, p: Ptr, off: usize) -> Result<i16, ClipReadError> {
        self.s.i16_at(p, off).map_err(|_| Truncated)
    }

    fn vec3(&self, p: Ptr, off: usize) -> Result<[f32; 3], ClipReadError> {
        Ok([
            self.f32(p, off)?,
            self.f32(p, off + 4)?,
            self.f32(p, off + 8)?,
        ])
    }

    fn plane(&self, plane: Ptr) -> Result<[f32; 4], ClipReadError> {
        Ok([
            self.f32(plane, 0)?,
            self.f32(plane, 4)?,
            self.f32(plane, 8)?,
            self.f32(plane, 12)?,
        ])
    }

    /// Brush `i`; `MissingTables` when the map has no brush table.
    pub fn brush(&self, i: usize) -> Result<ClipBrushRead, ClipReadError> {
        let brush = self.g.brushes.ok_or(MissingTables)?.at(i * BRUSH);
        let mins = self.vec3(brush, BRUSH_MINS)?;
        let maxs = self.vec3(brush, BRUSH_MAXS)?;
        let contents = self.u32(brush, BRUSH_CONTENTS)?;
        let mut axial_sflags = [0u32; 6];
        for (i, slot) in axial_sflags.iter_mut().enumerate() {
            *slot = self.u32(brush, BRUSH_AXIAL_SFLAGS + i * 4)?;
        }
        let numsides = self.u32(brush, BRUSH_NUMSIDES)? as usize;
        let mut sides = Vec::new();
        if let Ok(ZonePtr::Offset(table)) = self.s.ptr_at(brush, BRUSH_SIDES) {
            for j in 0..numsides {
                let side = table.at(j * CBRUSH_SIDE);
                if let ZonePtr::Offset(plane) = self.s.ptr_at(side, 0).map_err(|_| Truncated)? {
                    sides.push((self.plane(plane)?, self.u32(side, CBRUSH_SIDE_SFLAGS)?));
                }
            }
        }
        Ok(ClipBrushRead {
            mins,
            maxs,
            contents,
            axial_sflags,
            sides,
        })
    }

    /// The collision mesh's tables; `None` when the map has no vertices.
    pub fn mesh(&self) -> Result<Option<ClipMeshRead>, ClipReadError> {
        let g = &self.g;
        let Some(verts) = g.verts else {
            return Ok(None);
        };
        let mut mesh = ClipMeshRead::default();
        for i in 0..g.vert_count {
            mesh.verts.push(self.vec3(verts.at(i * 12), 0)?);
        }
        if let Some(indices) = g.tri_indices {
            for i in 0..g.tri_count.saturating_mul(3) {
                mesh.tri_indices.push(self.u16(indices, i * 2)?);
            }
        }
        if let Some(walk) = g.tri_edge_is_walkable {
            for i in 0..g.tri_count.saturating_mul(3).div_ceil(32) * 4 {
                mesh.tri_edge_is_walkable
                    .push(self.s.u8_at(walk, i).map_err(|_| Truncated)?);
            }
        }
        if let Some(materials) = g.materials {
            for i in 0..g.material_count {
                let p = materials.at(i * sz::DMATERIAL);
                let bytes = self.s.slice_at(p, 0, 64).map_err(|_| Truncated)?;
                let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                mesh.materials.push((
                    String::from_utf8_lossy(&bytes[..end]).into_owned(),
                    self.u32(p, 64)?,
                    self.u32(p, 68)?,
                ));
            }
        }
        if let Some(parts) = g.collision_partitions {
            for i in 0..g.partition_count {
                let p = parts.at(i * sz::COLLISION_PARTITION);
                mesh.partitions
                    .push((self.s.u8_at(p, 0).map_err(|_| Truncated)?, self.i32(p, 4)?));
            }
        }
        if let Some(trees) = g.collision_aabb_trees {
            for i in 0..g.aabb_tree_count {
                let p = trees.at(i * sz::COLLISION_AABB_TREE);
                mesh.aabb_trees.push(ClipAabbRead {
                    origin: self.vec3(p, 0)?,
                    half_size: self.vec3(p, 16)?,
                    material_index: self.u16(p, 12)?,
                    child_count: self.u16(p, 14)?,
                    u: self.i32(p, 28)?,
                });
            }
        }
        Ok(Some(mesh))
    }

    /// `MissingTables` unless the map has its nodes, leaves and leaf-brush
    /// nodes.
    pub fn has_bsp(&self) -> Result<(), ClipReadError> {
        match (self.g.nodes, self.g.leaves, self.g.leafbrush_nodes) {
            (Some(_), Some(_), Some(_)) => Ok(()),
            _ => Err(MissingTables),
        }
    }

    /// Node `i`'s plane and children.
    pub fn node(&self, i: usize) -> Result<([f32; 4], [i16; 2]), ClipReadError> {
        let p = self.g.nodes.ok_or(MissingTables)?.at(i * sz::C_NODE);
        let ZonePtr::Offset(plane) = self.s.ptr_at(p, 0).map_err(|_| Truncated)? else {
            return Err(MissingTables);
        };
        Ok((self.plane(plane)?, [self.i16(p, 4)?, self.i16(p, 6)?]))
    }

    pub fn leaf(&self, i: usize) -> Result<ClipLeafRead, ClipReadError> {
        let p = self.g.leaves.ok_or(MissingTables)?.at(i * sz::C_LEAF);
        Ok(ClipLeafRead {
            leaf_brush_node: self.i32(p, 36)?,
            first_coll_aabb_index: self.u16(p, 0)?,
            coll_aabb_count: self.u16(p, 2)?,
        })
    }

    pub fn model(&self, i: usize) -> Result<ClipModelRead, ClipReadError> {
        let p = self.g.cmodels.ok_or(MissingTables)?.at(i * sz::C_MODEL);
        Ok(ClipModelRead {
            leaf_brush_node: self.i32(p, 64)?,
            mins: self.vec3(p, 0)?,
            maxs: self.vec3(p, 12)?,
            radius: self.f32(p, 24)?,
        })
    }

    pub fn leaf_brush_node(&self, index: usize) -> Result<ClipLeafBrushNodeRead, ClipReadError> {
        let base = self.g.leafbrush_nodes.ok_or(MissingTables)?;
        let p = base.at(index * sz::C_LEAF_BRUSH_NODE);
        let count = self.i16(p, 2)?;
        if count > 0 {
            let ZonePtr::Offset(brushes) = self.s.ptr_at(p, 8).map_err(|_| Truncated)? else {
                return Err(MissingTables);
            };
            let brushes = (0..count as usize)
                .map(|i| self.u16(brushes, i * 2))
                .collect::<Result<_, _>>()?;
            Ok(ClipLeafBrushNodeRead {
                count,
                brushes,
                children: [0; 2],
            })
        } else {
            Ok(ClipLeafBrushNodeRead {
                count,
                brushes: Vec::new(),
                children: [self.u16(p, 16)?, self.u16(p, 18)?],
            })
        }
    }
}
