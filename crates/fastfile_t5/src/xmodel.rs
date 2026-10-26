//! A model read out of the zone into plain data: its bones and bind pose,
//! its surfaces' vertices, triangles and skinning, its collision and its
//! movement brushes.

use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;

use crate::content::ScriptStrings;
use crate::size;
use crate::zone::{Ptr, XModelGeometry, ZonePtr, ZoneStream};

const XSURFACE: usize = 68;
const DOBJ_ANIM_MAT: usize = 32;
pub const GFX_PACKED_VERTEX: usize = 32;
const XRIGID_VERT_LIST: usize = 12;

/// A model as the zone stores it, read out.
#[derive(Clone, Debug)]
pub struct XModelRead {
    pub name: String,
    pub num_bones: usize,
    pub num_root_bones: usize,
    pub bone_names: Vec<String>,
    /// Each bone's bind pose: its rotation quaternion and translation.
    pub base_mat: Vec<([f32; 4], [f32; 3])>,
    /// `None` when the child bones' pose arrays do not read.
    pub pose: Option<XModelPoseRead>,
    pub surfaces: Vec<XSurfaceRead>,
    /// Where the surfaces' material handles start, one 4-byte slot each.
    pub material_handles: Option<Ptr>,
    /// `None` when the bone or surface collision is malformed.
    pub collision: Option<XModelCollisionRead>,
    /// `None` when the collision maps' brushes are malformed.
    pub movement_brushes: Option<Vec<XModelBrushRead>>,
    pub radius: Option<f32>,
    pub contents: u32,
    pub coll_lod: i16,
    pub num_lods: i16,
    pub lod_dist: [f32; 4],
    pub lod_surf_span: [(u16, u16); 4],
}

/// The child bones' parents, rotations and translations.
#[derive(Clone, Debug, Default)]
pub struct XModelPoseRead {
    pub parent_list: Vec<u8>,
    pub quats: Vec<[i16; 4]>,
    pub trans: Vec<[f32; 3]>,
}

#[derive(Clone, Debug)]
pub struct XSurfaceRead {
    pub deformed: bool,
    pub vert_list_count: u32,
    pub vertices: Vec<[u8; GFX_PACKED_VERTEX]>,
    /// Three per triangle, as stored: not checked against the vertices.
    pub indices: Vec<u16>,
    pub part_bits: [u32; 6],
    pub skin: XSurfaceSkinRead,
}

/// A surface's skinning as stored: bone offsets, not bone indices.
#[derive(Clone, Debug, Default)]
pub struct XSurfaceSkinRead {
    /// Rigid runs `(bone offset, vertex count)`; empty when the surface has
    /// none.
    pub rigid: Vec<(u16, u16)>,
    /// The blended vertices, read when the rigid runs do not cover every
    /// vertex.
    pub blend: Option<XSurfaceBlendRead>,
}

#[derive(Clone, Debug, Default)]
pub struct XSurfaceBlendRead {
    /// How many vertices take one, two, three and four bones.
    pub counts: [usize; 4],
    /// Their words, a bone offset then a bone offset and weight per extra
    /// bone; `None` for a null list with every count zero.
    pub words: Option<Vec<u16>>,
}

#[derive(Clone, Debug, Default)]
pub struct XModelCollisionRead {
    /// One per bone when the model has bone info; `None` for a bone without
    /// a hit volume.
    pub bones: Vec<Option<XBoneCollisionRead>>,
    pub surfaces: Vec<XCollSurfRead>,
}

#[derive(Clone, Copy, Debug)]
pub struct XBoneCollisionRead {
    pub midpoint: [f32; 3],
    pub half_size: [f32; 3],
    pub radius_sq: f32,
    pub part_classification: u8,
}

#[derive(Clone, Debug)]
pub struct XCollSurfRead {
    pub bone: u16,
    pub contents: u32,
    pub surf_flags: u32,
    pub midpoint: [f32; 3],
    pub half_size: [f32; 3],
    pub tris: Vec<XCollTriRead>,
}

#[derive(Clone, Copy, Debug)]
pub struct XCollTriRead {
    pub plane: [f32; 4],
    pub svec: [f32; 4],
    pub tvec: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct XModelBrushRead {
    pub planes: Vec<[f32; 4]>,
    pub contents: u32,
    pub plane_surface_flags: Vec<u32>,
    pub verts: Vec<[f32; 3]>,
}

impl XModelGeometry {
    pub fn name_in<'s>(&self, s: &'s ZoneStream<'_>) -> Option<&'s str> {
        self.name.and_then(|p| s.cstr(p).ok())
    }

    /// The model read out; `None` without a name, surfaces, bone names or
    /// bind pose, or when a bone or surface does not read.
    pub fn read(&self, s: &ZoneStream<'_>, strings: &ScriptStrings) -> Option<XModelRead> {
        let name = self.name_in(s)?.to_owned();
        let surfaces = self.surfaces?;
        let bone_names = self.bone_names?;
        let base_mat = self.base_mat?;

        let mut names = Vec::with_capacity(self.num_bones);
        let mut mats = Vec::with_capacity(self.num_bones);
        for i in 0..self.num_bones {
            let id = s.u16_at(bone_names, i * 2).ok()?;
            names.push(strings.get(s, id).unwrap_or("").to_owned());
            let m = base_mat.at(i * DOBJ_ANIM_MAT);
            mats.push((
                [
                    s.f32_at(m, 0).ok()?,
                    s.f32_at(m, 4).ok()?,
                    s.f32_at(m, 8).ok()?,
                    s.f32_at(m, 12).ok()?,
                ],
                [
                    s.f32_at(m, 16).ok()?,
                    s.f32_at(m, 20).ok()?,
                    s.f32_at(m, 24).ok()?,
                ],
            ));
        }

        let num_child = self.num_bones.saturating_sub(self.num_root_bones);
        let pose = self.read_pose(s, num_child);

        let mut surface_reads = Vec::with_capacity(self.surface_count);
        for surface_index in 0..self.surface_count {
            surface_reads.push(read_surface(s, surfaces.at(surface_index * XSURFACE))?);
        }

        Some(XModelRead {
            name,
            num_bones: self.num_bones,
            num_root_bones: self.num_root_bones,
            bone_names: names,
            base_mat: mats,
            pose,
            surfaces: surface_reads,
            material_handles: self.material_handles,
            collision: self.read_collision(s),
            movement_brushes: self.read_movement_brushes(s),
            radius: self.radius,
            contents: self.contents,
            coll_lod: self.coll_lod,
            num_lods: self.num_lods,
            lod_dist: self.lod_dist,
            lod_surf_span: self.lod_surf_span,
        })
    }

    fn read_pose(&self, s: &ZoneStream<'_>, num_child: usize) -> Option<XModelPoseRead> {
        if num_child == 0 {
            return Some(XModelPoseRead::default());
        }
        let arr = self.parent_list?;
        let parent_list = (0..num_child)
            .map(|i| s.u8_at(arr, i).ok())
            .collect::<Option<Vec<_>>>()?;
        let arr = self.quats?;
        let quats = (0..num_child)
            .map(|i| {
                let o = i * size::XMODEL_QUAT;
                Some([
                    s.i16_at(arr, o).ok()?,
                    s.i16_at(arr, o + 2).ok()?,
                    s.i16_at(arr, o + 4).ok()?,
                    s.i16_at(arr, o + 6).ok()?,
                ])
            })
            .collect::<Option<Vec<_>>>()?;
        let arr = self.trans?;
        let trans = (0..num_child)
            .map(|i| {
                let o = i * size::XMODEL_TRANS_STRIDE;
                Some([
                    s.f32_at(arr, o).ok()?,
                    s.f32_at(arr, o + 4).ok()?,
                    s.f32_at(arr, o + 8).ok()?,
                ])
            })
            .collect::<Option<Vec<_>>>()?;
        Some(XModelPoseRead {
            parent_list,
            quats,
            trans,
        })
    }

    fn read_movement_brushes(&self, s: &ZoneStream<'_>) -> Option<Vec<XModelBrushRead>> {
        let pointer = |p: Ptr, offset| match s.ptr_at(p, offset).ok()? {
            ZonePtr::Offset(p) => Some(s.resolve_alias(p)),
            _ => None,
        };
        if self.collision_map_count == 0 {
            return Some(Vec::new());
        }
        let maps = self.collision_maps?;
        if s.ptr_at(maps, 0).ok()? == ZonePtr::Null {
            return Some(Vec::new());
        }
        let list = pointer(maps, 0)?;
        let count = s.u32_at(list, 0).ok()? as usize;
        if count == 0 {
            return Some(Vec::new());
        }
        let geoms = pointer(list, 4)?;
        let mut brushes = Vec::new();
        for i in 0..count {
            let geom = geoms.at(i * size::PHYS_GEOM_INFO);
            if s.ptr_at(geom, 0).ok()? == ZonePtr::Null {
                continue;
            }
            let brush = pointer(geom, 0)?;
            let mut planes = Vec::new();
            let mut plane_surface_flags = Vec::new();
            for side in 0..2 {
                for axis in 0..3 {
                    let mut plane = [0.0; 4];
                    plane[axis] = if side == 0 { -1.0 } else { 1.0 };
                    plane[3] = s.f32_at(brush, side * 16 + axis * 4).ok()? * plane[axis];
                    planes.push(plane);
                    plane_surface_flags.push(s.u32_at(brush, 60 + (side * 3 + axis) * 4).ok()?);
                }
            }
            let side_count = s.u32_at(brush, size::BRUSH_WRAPPER_NUMSIDES_OFF).ok()? as usize;
            if side_count != 0 {
                let sides = pointer(brush, size::BRUSH_WRAPPER_SIDES_OFF)?;
                for j in 0..side_count {
                    let side = sides.at(j * size::CBRUSH_SIDE);
                    let plane = pointer(side, 0)?;
                    planes.push([
                        s.f32_at(plane, 0).ok()?,
                        s.f32_at(plane, 4).ok()?,
                        s.f32_at(plane, 8).ok()?,
                        s.f32_at(plane, 12).ok()?,
                    ]);
                    plane_surface_flags.push(s.u32_at(side, 8).ok()?);
                }
            }
            let vert_count = s.u32_at(brush, size::BRUSH_WRAPPER_NUMVERTS_OFF).ok()? as usize;
            let mut verts = Vec::with_capacity(vert_count);
            if vert_count != 0 {
                let table = pointer(brush, size::BRUSH_WRAPPER_VERTS_OFF)?;
                for j in 0..vert_count {
                    verts.push([
                        s.f32_at(table, j * 12).ok()?,
                        s.f32_at(table, j * 12 + 4).ok()?,
                        s.f32_at(table, j * 12 + 8).ok()?,
                    ]);
                }
            }
            brushes.push(XModelBrushRead {
                planes,
                contents: s.u32_at(brush, 12).ok()?,
                plane_surface_flags,
                verts,
            });
        }
        Some(brushes)
    }

    fn read_collision(&self, s: &ZoneStream<'_>) -> Option<XModelCollisionRead> {
        let bounds = |row: Ptr, off| {
            let mut midpoint = [0.0; 3];
            let mut half_size = [0.0; 3];
            for axis in 0..3 {
                let min = s.f32_at(row, off + axis * 4).ok()?;
                let max = s.f32_at(row, off + 12 + axis * 4).ok()?;
                if !min.is_finite() || !max.is_finite() || min > max {
                    return None;
                }
                midpoint[axis] = (min + max) * 0.5;
                half_size[axis] = (max - min) * 0.5;
            }
            Some((midpoint, half_size))
        };
        let mut bones = Vec::new();
        if let Some(info) = self.bone_info {
            let classes = self.part_classification?;
            for bone in 0..self.num_bones {
                let row = info.at(bone * size::XBONE_INFO);

                let radius_sq = s.f32_at(row, 36).ok()?;
                if radius_sq == 0.0 {
                    bones.push(None);
                    continue;
                }
                if !radius_sq.is_finite() || radius_sq < 0.0 {
                    return None;
                }
                let (midpoint, half_size) = bounds(row, 0)?;
                bones.push(Some(XBoneCollisionRead {
                    midpoint,
                    half_size,
                    radius_sq,
                    part_classification: s.u8_at(classes, bone).ok()?,
                }));
            }
        }
        let mut surfaces = Vec::with_capacity(self.num_coll_surfs);
        if self.num_coll_surfs != 0 {
            let arr = self.coll_surfs?;
            for i in 0..self.num_coll_surfs {
                let row = arr.at(i * size::XMODEL_COLL_SURF);
                let (midpoint, half_size) = bounds(row, 8)?;
                let bone = u16::try_from(s.i32_at(row, 32).ok()?).ok()?;
                if usize::from(bone) >= self.num_bones {
                    return None;
                }
                let num = usize::try_from(s.i32_at(row, 4).ok()?).ok()?;
                let mut tris = Vec::with_capacity(num);
                if num != 0 {
                    let ZonePtr::Offset(arr) = s.ptr_at(row, 0).ok()? else {
                        return None;
                    };
                    let arr = s.resolve_alias(arr);
                    for i in 0..num {
                        let tri = arr.at(i * size::XMODEL_COLL_TRI);
                        let vec4 = |off| {
                            let mut v = [0.0; 4];
                            for (j, value) in v.iter_mut().enumerate() {
                                *value = s.f32_at(tri, off + j * 4).ok()?;
                                if !value.is_finite() {
                                    return None;
                                }
                            }
                            Some(v)
                        };
                        tris.push(XCollTriRead {
                            plane: vec4(0)?,
                            svec: vec4(16)?,
                            tvec: vec4(32)?,
                        });
                    }
                }
                surfaces.push(XCollSurfRead {
                    bone,
                    contents: s.u32_at(row, 36).ok()?,
                    surf_flags: s.u32_at(row, 40).ok()?,
                    midpoint,
                    half_size,
                    tris,
                });
            }
        }
        Some(XModelCollisionRead { bones, surfaces })
    }
}

fn read_surface(s: &ZoneStream<'_>, surface: Ptr) -> Option<XSurfaceRead> {
    let vert_list_count = s.u8_at(surface, 1).ok()? as usize;
    let deformed = s.u16_at(surface, 2).ok()? & 0x80 != 0;
    let vertex_count = s.u16_at(surface, 4).ok()? as usize;
    let tri_count = s.u16_at(surface, 6).ok()? as usize;
    let vertices = match s.ptr_at(surface, 32).ok()? {
        ZonePtr::Offset(p) => s.resolve_alias(p),
        _ => return None,
    };
    let triangles = match s.ptr_at(surface, 12).ok()? {
        ZonePtr::Offset(p) => s.resolve_alias(p),
        _ => return None,
    };
    let mut part_bits = [0u32; 6];
    for (i, word) in part_bits
        .iter_mut()
        .enumerate()
        .take(size::XSURFACE_PART_BITS_WORDS)
    {
        *word = s
            .u32_at(surface, size::XSURFACE_PART_BITS_OFF + i * 4)
            .ok()?;
    }
    let skin = read_surface_skin(s, surface, vertex_count, vert_list_count)?;

    let mut packed = Vec::with_capacity(vertex_count);
    for vertex_index in 0..vertex_count {
        let vertex = vertices.at(vertex_index * GFX_PACKED_VERTEX);
        let bytes = s.slice_at(vertex, 0, GFX_PACKED_VERTEX).ok()?;
        let mut copy = [0u8; GFX_PACKED_VERTEX];
        copy.copy_from_slice(bytes);
        packed.push(copy);
    }
    let indices = (0..tri_count * 3)
        .map(|i| s.u16_at(triangles, i * 2).ok())
        .collect::<Option<Vec<_>>>()?;
    Some(XSurfaceRead {
        deformed,
        vert_list_count: vert_list_count as u32,
        vertices: packed,
        indices,
        part_bits,
        skin,
    })
}

fn read_surface_skin(
    s: &ZoneStream<'_>,
    surface: Ptr,
    vertex_count: usize,
    vert_list_count: usize,
) -> Option<XSurfaceSkinRead> {
    let mut rigid = Vec::new();
    if vert_list_count > 0
        && let Ok(ZonePtr::Offset(list_ptr)) = s.ptr_at(surface, 40)
    {
        let list = s.resolve_alias(list_ptr);
        for i in 0..vert_list_count {
            let entry = list.at(i * XRIGID_VERT_LIST);
            rigid.push((s.u16_at(entry, 0).ok()?, s.u16_at(entry, 2).ok()?));
        }
        let covered = rigid
            .iter()
            .fold(0usize, |n, &(_, run)| n.saturating_add(usize::from(run)));
        if covered >= vertex_count {
            return Some(XSurfaceSkinRead { rigid, blend: None });
        }
    }

    let vi = surface.at(16);
    let counts = [
        s.i16_at(vi, 0).ok()?.max(0) as usize,
        s.i16_at(vi, 2).ok()?.max(0) as usize,
        s.i16_at(vi, 4).ok()?.max(0) as usize,
        s.i16_at(vi, 6).ok()?.max(0) as usize,
    ];
    let words = match s.ptr_at(vi, 8).ok()? {
        ZonePtr::Offset(p) => {
            let blend = s.resolve_alias(p);
            let total: usize = counts
                .iter()
                .enumerate()
                .map(|(bucket, count)| count * (1 + bucket * 2))
                .sum();
            Some(
                (0..total)
                    .map(|i| s.u16_at(blend, i * 2).ok())
                    .collect::<Option<Vec<_>>>()?,
            )
        }
        ZonePtr::Null if counts.iter().all(|&c| c == 0) => None,
        _ => return None,
    };
    Some(XSurfaceSkinRead {
        rigid,
        blend: Some(XSurfaceBlendRead { counts, words }),
    })
}

/// One `XSurface` as a draw mesh reads it.
#[derive(Clone, Debug)]
pub struct XSurfaceMeshRead {
    /// The surface's `flags` word.
    pub flags: u16,
    pub vertices: Vec<[u8; GFX_PACKED_VERTEX]>,
    /// Three a triangle, as the zone stores them (not yet checked against
    /// the vertex count).
    pub indices: Vec<u16>,
    pub vert_lists: Vec<XRigidVertListRead>,
}

/// An `XRigidVertList`'s triangle range and its collision tree.
#[derive(Clone, Debug)]
pub struct XRigidVertListRead {
    pub tri_offset: u16,
    pub tri_count: u16,
    pub tree: Option<XSurfaceCollisionTreeRead>,
}

#[derive(Clone, Debug)]
pub struct XSurfaceCollisionTreeRead {
    pub trans: [f32; 3],
    pub scale: [f32; 3],
    /// Each node's mins, maxs, child begin index and child count.
    pub nodes: Vec<[u16; 8]>,
    /// Each leaf's triangle begin index.
    pub leafs: Vec<u16>,
}

/// Why a surface did not read: a pointer the mesh needs is not there, or the
/// zone ran out under a field.
#[derive(Clone, Copy, Debug)]
pub enum XSurfaceMeshError {
    Missing,
    Zone(crate::ZoneError),
}

impl From<crate::ZoneError> for XSurfaceMeshError {
    fn from(error: crate::ZoneError) -> Self {
        Self::Zone(error)
    }
}

impl ZoneStream<'_> {
    /// Surface `index` of the `XSurface` array at `surfaces`.
    pub fn read_xsurface_mesh(
        &self,
        surfaces: Ptr,
        index: usize,
    ) -> Result<XSurfaceMeshRead, XSurfaceMeshError> {
        use XSurfaceMeshError::Missing;
        let surface = surfaces.at(index * XSURFACE);
        let vertex_count = self.u16_at(surface, 4).map_err(|_| Missing)? as usize;
        let tri_count = self.u16_at(surface, 6).map_err(|_| Missing)? as usize;
        let vertices = self.resolved(surface, 32)?;
        let triangles = self.resolved(surface, 12)?;
        let vertices = (0..vertex_count)
            .map(|i| {
                let mut packed = [0u8; GFX_PACKED_VERTEX];
                let bytes = self
                    .slice_at(vertices.at(i * GFX_PACKED_VERTEX), 0, GFX_PACKED_VERTEX)
                    .map_err(|_| Missing)?;
                packed.copy_from_slice(bytes);
                Ok(packed)
            })
            .collect::<Result<Vec<_>, XSurfaceMeshError>>()?;
        let indices = (0..tri_count * 3)
            .map(|i| self.u16_at(triangles, i * 2).map_err(|_| Missing))
            .collect::<Result<Vec<_>, _>>()?;
        let flags = self.u16_at(surface, 2).map_err(|_| Missing)?;
        Ok(XSurfaceMeshRead {
            flags,
            vertices,
            indices,
            vert_lists: self.read_rigid_vert_lists(surface)?,
        })
    }

    fn resolved(&self, parent: Ptr, field: usize) -> Result<Ptr, XSurfaceMeshError> {
        match self
            .ptr_at(parent, field)
            .map_err(|_| XSurfaceMeshError::Missing)?
        {
            ZonePtr::Offset(ptr) => Ok(self.resolve_alias(ptr)),
            _ => Err(XSurfaceMeshError::Missing),
        }
    }

    fn read_rigid_vert_lists(
        &self,
        surface: Ptr,
    ) -> Result<Vec<XRigidVertListRead>, XSurfaceMeshError> {
        let count = self.u8_at(surface, 1)? as usize;
        if count == 0 {
            return Ok(Vec::new());
        }
        let lists = self.resolved(surface, 40)?;
        self.slice_at(lists, 0, count * size::XRIGID_VERT_LIST)?;
        let mut out = Vec::with_capacity(count);
        for list_index in 0..count {
            let list = lists.at(list_index * size::XRIGID_VERT_LIST);
            let tri_offset = self.u16_at(list, 4)?;
            let tri_count = self.u16_at(list, 6)?;
            let tree = match self.ptr_at(list, 8)? {
                ZonePtr::Null => None,
                ZonePtr::Offset(tree) => Some(self.read_collision_tree(self.resolve_alias(tree))?),
                ZonePtr::Following | ZonePtr::Insert => return Err(XSurfaceMeshError::Missing),
            };
            out.push(XRigidVertListRead {
                tri_offset,
                tri_count,
                tree,
            });
        }
        Ok(out)
    }

    fn read_collision_tree(
        &self,
        tree: Ptr,
    ) -> Result<XSurfaceCollisionTreeRead, XSurfaceMeshError> {
        let mut trans = [0.0; 3];
        let mut scale = [0.0; 3];
        for axis in 0..3 {
            trans[axis] = self.f32_at(tree, axis * 4)?;
            scale[axis] = self.f32_at(tree, 12 + axis * 4)?;
        }
        let node_count = self.u32_at(tree, 24)? as usize;
        let leaf_count = self.u32_at(tree, 32)? as usize;
        let nodes_ptr = if node_count == 0 {
            None
        } else {
            let rows = self.resolved(tree, 28)?;
            self.slice_at(rows, 0, node_count * size::XSURFACE_COLLISION_NODE)?;
            Some(rows)
        };
        let leafs_ptr = if leaf_count == 0 {
            None
        } else {
            let rows = self.resolved(tree, 36)?;
            self.slice_at(rows, 0, leaf_count * size::XSURFACE_COLLISION_LEAF)?;
            Some(rows)
        };
        let mut nodes = Vec::with_capacity(node_count);
        if let Some(rows) = nodes_ptr {
            for index in 0..node_count {
                let row = rows.at(index * size::XSURFACE_COLLISION_NODE);
                let mut node = [0u16; 8];
                for (word, value) in node.iter_mut().enumerate() {
                    *value = self.u16_at(row, word * 2)?;
                }
                nodes.push(node);
            }
        }
        let mut leafs = Vec::with_capacity(leaf_count);
        if let Some(rows) = leafs_ptr {
            for index in 0..leaf_count {
                leafs.push(self.u16_at(rows, index * size::XSURFACE_COLLISION_LEAF)?);
            }
        }
        Ok(XSurfaceCollisionTreeRead {
            trans,
            scale,
            nodes,
            leafs,
        })
    }
}
