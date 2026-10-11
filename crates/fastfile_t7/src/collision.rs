//! A map's collision (`col_map`): brushes found through a BSP tree, and
//! triangles found through bounding-box trees.
//!
//! The header (0x2d0 bytes) holds a count and a pointer per array. An inline
//! pointer's data follows the header in field order, each array whole and then
//! the inline data its elements point to. The planes are the drawn world's: the
//! header refers to them, and they are found in the `gfx_map` data by shape. The
//! result is accepted only when every brush corner lies on or behind each of
//! its brush's planes, every brush of a leaf touches the leaf's bounds, and
//! every triangle lies inside the bounding box that holds it.

use asset_core::{
    CollisionBox, CollisionBrush, CollisionLeaf, CollisionMaterial, CollisionModel, CollisionNode,
    CollisionPartition, WorldCollision,
};

use crate::content::AssetList;
use crate::world::{self, f32_at, i64_at, u16_at, u32_at, u64_at, vec3_at};

/// The asset type of a map's collision (`col_map`).
pub const COL_MAP_ASSET_TYPE: u32 = 12;

const HEADER_LEN: usize = 0x2d0;
/// Both headers start with the map's name; the plane count follows at 0x10.
const PLANE_COUNT_AT: usize = 0x10;
const PLANES_AT: usize = 0x18;
const MATERIALS_AT: usize = 0x20;
const SIDES_AT: usize = 0x30;
const LEAF_BRUSH_NODES_AT: usize = 0x40;
const LEAF_BRUSH_COUNT_AT: usize = 0x50;
const BRUSH_VERTICES_AT: usize = 0x60;
const VERTEX_LISTS_AT: usize = 0x70;
const BRUSHES_AT: usize = 0x80;
/// Fields between the brushes and the static models; empty in every map read.
const UNREAD_AT: [usize; 3] = [0x90, 0x98, 0xa0];
const STATIC_MODELS_AT: usize = 0xa8;
const NODES_AT: usize = 0xb8;
const LEAVES_AT: usize = 0xc8;
const VERTICES_AT: usize = 0xd8;
const TRIANGLES_AT: usize = 0xe8;
const WALKABLE_AT: usize = 0xf8;
const PARTITIONS_AT: usize = 0x100;
const BOXES_AT: usize = 0x110;
const MODELS_AT: usize = 0x120;

const PLANE_LEN: usize = 20;
const MATERIAL_LEN: usize = 16;
const SIDE_LEN: usize = 16;
const LEAF_BRUSH_NODE_LEN: usize = 24;
const VERTEX_LEN: usize = 12;
const BRUSH_LEN: usize = 112;
const STATIC_MODEL_LEN: usize = 112;
const STATIC_MODEL_PART_LEN: usize = 32;
const NODE_LEN: usize = 16;
const LEAF_LEN: usize = 56;
const TRIANGLE_LEN: usize = 12;
const PARTITION_LEN: usize = 16;
const BOX_LEN: usize = 32;
const MODEL_LEN: usize = 96;
/// A model's leaf, inside the model.
const MODEL_LEAF_AT: usize = 0x28;

const INLINE: i64 = -1;
/// How far a corner may stand in front of its brush's plane, or a triangle
/// outside its box (rounding).
const SLACK: f32 = 0.5;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CollisionError {
    World(world::WorldError),
    /// Collision headers found in the stream do not match the asset list.
    HeaderCount {
        listed: usize,
        found: usize,
    },
    /// No run of planes in the drawn world, or more than one.
    Planes {
        candidates: usize,
    },
    /// A field holds what this reader does not read.
    Unread {
        field: usize,
    },
    /// The stream ends inside the asset.
    Truncated,
    /// A reference or index points outside its array.
    Index(&'static str),
    /// A check of the data against itself failed.
    Check(&'static str),
}

impl From<world::WorldError> for CollisionError {
    fn from(error: world::WorldError) -> Self {
        Self::World(error)
    }
}

/// The zone's collision, if it lists a drawn world and its collision.
pub fn world_collision(
    content: &[u8],
    list: &AssetList,
) -> Result<Option<WorldCollision>, CollisionError> {
    let listed = list
        .asset_types
        .iter()
        .filter(|&&ty| ty == COL_MAP_ASSET_TYPE)
        .count();
    let Some(gfx) = world::gfx_header(content, list)? else {
        return match listed {
            0 => Ok(None),
            _ => Err(CollisionError::HeaderCount { listed, found: 0 }),
        };
    };
    let name = u64_at(content, gfx).ok_or(CollisionError::Truncated)?;
    let plane_count = u32_at(content, gfx + PLANE_COUNT_AT).ok_or(CollisionError::Truncated)?;
    let headers: Vec<usize> = (gfx + world::HEADER_LEN..content.len().saturating_sub(HEADER_LEN))
        .filter(|&at| {
            u64_at(content, at) == Some(name)
                && u32_at(content, at + PLANE_COUNT_AT) == Some(plane_count)
                && u64_at(content, at + PLANES_AT).is_some_and(|planes| planes >> 56 == 0x50)
        })
        .collect();
    if headers.len() != listed {
        return Err(CollisionError::HeaderCount {
            listed,
            found: headers.len(),
        });
    }
    let Some(&header) = headers.first() else {
        return Ok(None);
    };
    let planes = plane_array(
        content,
        gfx + world::HEADER_LEN,
        header,
        plane_count as usize,
    )?;
    let collision = Reader {
        content,
        header,
        at: header + HEADER_LEN,
        planes_ref: u64_at(content, header + PLANES_AT).unwrap(),
        plane_count: planes.len(),
    }
    .read(planes)?;
    check(&collision)?;
    Ok(Some(collision))
}

/// The one run of `count` planes between the drawn world's header and the
/// collision header: unit normals, the axial type, the sign bits.
fn plane_array(
    content: &[u8],
    from: usize,
    to: usize,
    count: usize,
) -> Result<Vec<[f32; 4]>, CollisionError> {
    // One pass per offset modulo the plane length, counting runs of planes;
    // a run longer than `count` holds more than one candidate.
    let mut candidates = Vec::new();
    for start in from..(from + PLANE_LEN).min(to) {
        let mut run = 0usize;
        let mut at = start;
        loop {
            let valid = at + PLANE_LEN <= to && plane_at(content, at);
            if valid {
                run += 1;
            } else {
                if run >= count {
                    let first = at - PLANE_LEN * run;
                    candidates.extend((0..=run - count).map(|i| first + PLANE_LEN * i));
                }
                run = 0;
            }
            if at + PLANE_LEN > to {
                break;
            }
            at += PLANE_LEN;
        }
    }
    let [at] = candidates[..] else {
        return Err(CollisionError::Planes {
            candidates: candidates.len(),
        });
    };
    Ok((0..count)
        .map(|i| {
            let [x, y, z] = vec3_at(content, at + PLANE_LEN * i).unwrap();
            [x, y, z, f32_at(content, at + PLANE_LEN * i + 12).unwrap()]
        })
        .collect())
}

/// Normal, distance, type (0..2 along that axis, 3 otherwise), the normal's
/// sign bits, two bytes of padding.
fn plane_at(content: &[u8], at: usize) -> bool {
    let Some(tail) = content.get(at + 16..at + 20) else {
        return false;
    };
    if tail[0] > 3 || tail[1] > 7 || tail[2] != 0 || tail[3] != 0 {
        return false;
    }
    let normal = vec3_at(content, at).unwrap();
    let length = normal.iter().map(|c| c * c).sum::<f32>();
    let signs = (0..3).fold(0u8, |bits, axis| {
        bits | u8::from(normal[axis] < 0.0) << axis
    });
    let axis = normal.iter().position(|&c| c == 1.0);
    (length - 1.0).abs() < 1e-3
        && signs == tail[1]
        && axis.map_or(tail[0] == 3, |axis| usize::from(tail[0]) == axis)
}

struct Reader<'a> {
    content: &'a [u8],
    header: usize,
    /// Where the next inline data starts.
    at: usize,
    planes_ref: u64,
    plane_count: usize,
}

struct LeafBrushNode {
    count: i16,
    children: [u16; 2],
    brushes: Vec<u32>,
}

impl Reader<'_> {
    fn read(mut self, planes: Vec<[f32; 4]>) -> Result<WorldCollision, CollisionError> {
        for field in UNREAD_AT {
            if self.field(field)? != 0 {
                return Err(CollisionError::Unread { field });
            }
        }
        let mut out = WorldCollision {
            planes,
            ..Default::default()
        };
        let material_at = self.array(MATERIALS_AT, MATERIAL_LEN)?;
        let mut named = Vec::new();
        for (i, at) in material_at.iter().enumerate() {
            let name = self.i64(*at)?;
            out.materials.push(CollisionMaterial {
                name: None,
                surface_flags: self.u32(at + 8)?,
                contents: self.u32(at + 12)?,
            });
            if name == INLINE {
                named.push(i);
            }
        }
        for i in named {
            out.materials[i].name = Some(self.string()?);
        }
        let sides: Vec<(u32, u32)> = self
            .array(SIDES_AT, SIDE_LEN)?
            .into_iter()
            .map(|at| Ok((self.plane(at)?, self.u32(at + 12)?)))
            .collect::<Result<_, CollisionError>>()?;
        let leaf_brush_nodes = self.leaf_brush_nodes()?;
        let brush_vertices: Vec<[f32; 3]> = self
            .array(BRUSH_VERTICES_AT, VERTEX_LEN)?
            .into_iter()
            .map(|at| self.vec3(at))
            .collect::<Result<_, _>>()?;
        self.array(VERTEX_LISTS_AT, 4)?;
        let brush_at = self.array(BRUSHES_AT, BRUSH_LEN)?;
        let mut corners = Vec::with_capacity(brush_at.len());
        let (mut first_sides, mut first_corners) = (None, None);
        let (mut next_side, mut next_corner) = (0usize, 0usize);
        for at in brush_at {
            let side_count = self.u32(at + 28)? as usize;
            let side_ref = self.u64(at + 32)?;
            let corner_count = self.u32(at + 88)? as usize;
            let corner_ref = self.u64(at + 96)?;
            // A brush refers to its sides and corners inside the arrays read
            // above, one brush after another.
            let first_side =
                consecutive(&mut first_sides, side_ref, side_count, SIDE_LEN, next_side)?;
            let first_corner = consecutive(
                &mut first_corners,
                corner_ref,
                corner_count,
                VERTEX_LEN,
                next_corner,
            )?;
            next_side = first_side + side_count;
            next_corner = first_corner + corner_count;
            let mut axial_surface_flags = [0; 6];
            for (i, flags) in axial_surface_flags.iter_mut().enumerate() {
                *flags = self.u32(at + 64 + 4 * i)?;
            }
            out.brushes.push(CollisionBrush {
                mins: self.vec3(at)?,
                maxs: self.vec3(at + 16)?,
                contents: self.u32(at + 12)?,
                axial_surface_flags,
                sides: sides
                    .get(first_side..next_side)
                    .ok_or(CollisionError::Index("brush sides"))?
                    .to_vec(),
            });
            corners.push(
                brush_vertices
                    .get(first_corner..next_corner)
                    .ok_or(CollisionError::Index("brush corners"))?
                    .to_vec(),
            );
        }
        if next_side != sides.len() || next_corner != brush_vertices.len() {
            return Err(CollisionError::Check(
                "brush sides and corners cover their arrays",
            ));
        }
        for (brush, corners) in out.brushes.iter().zip(&corners) {
            for &(plane, _) in &brush.sides {
                let [x, y, z, dist] = out.planes[plane as usize];
                if corners
                    .iter()
                    .any(|c| x * c[0] + y * c[1] + z * c[2] - dist > SLACK)
                {
                    return Err(CollisionError::Check("brush corners behind their planes"));
                }
            }
        }
        for at in self.array(STATIC_MODELS_AT, STATIC_MODEL_LEN)? {
            // Static models are not read yet; their inline parts are skipped.
            match self.i64(at + 96)? {
                0 => {}
                INLINE => {
                    let count = self.u32(at + 104)? as usize;
                    self.skip(count * STATIC_MODEL_PART_LEN)?;
                }
                _ => {
                    return Err(CollisionError::Unread {
                        field: STATIC_MODELS_AT,
                    });
                }
            }
        }
        for at in self.array(NODES_AT, NODE_LEN)? {
            out.nodes.push(CollisionNode {
                plane: self.plane(at)?,
                children: [self.u32(at + 8)? as i32, self.u32(at + 12)? as i32],
            });
        }
        for at in self.array(LEAVES_AT, LEAF_LEN)? {
            let leaf = self.leaf(at, &leaf_brush_nodes, &mut out.leaf_brushes)?;
            out.leaves.push(leaf);
        }
        let world_leaf_brushes = out.leaf_brushes.len();
        if world_leaf_brushes != self.field(LEAF_BRUSH_COUNT_AT)? as usize {
            return Err(CollisionError::Check(
                "leaf brushes match the header's count",
            ));
        }
        for at in self.array(VERTICES_AT, VERTEX_LEN)? {
            out.vertices.push(self.vec3(at)?);
        }
        for at in self.array(TRIANGLES_AT, TRIANGLE_LEN)? {
            out.triangles
                .push([self.u32(at)?, self.u32(at + 4)?, self.u32(at + 8)?]);
        }
        let walkable_len = (3 * out.triangles.len()).div_ceil(32) * 4;
        match self.i64(self.header + WALKABLE_AT)? {
            INLINE => {
                let at = self.skip(walkable_len)?;
                out.walkable_edges = self.content[at..at + walkable_len].to_vec();
            }
            0 if out.triangles.is_empty() => {}
            _ => return Err(CollisionError::Unread { field: WALKABLE_AT }),
        }
        for at in self.array(PARTITIONS_AT, PARTITION_LEN)? {
            out.partitions.push(CollisionPartition {
                triangle_count: self.u32(at)?,
                first_triangle: self.u32(at + 4)?,
            });
        }
        for at in self.array(BOXES_AT, BOX_LEN)? {
            out.boxes.push(CollisionBox {
                origin: self.vec3(at)?,
                material: self.u16(at + 12)?,
                child_count: self.u16(at + 14)?,
                half_size: self.vec3(at + 16)?,
                index: self.u32(at + 28)?,
            });
        }
        for at in self.array(MODELS_AT, MODEL_LEN)? {
            out.models.push(CollisionModel {
                mins: self.vec3(at)?,
                maxs: self.vec3(at + 12)?,
                radius: self.f32(at + 24)?,
                leaf: self.leaf(at + MODEL_LEAF_AT, &leaf_brush_nodes, &mut out.leaf_brushes)?,
            });
        }
        Ok(out)
    }

    /// The leaf brush trees: each node lists brushes, or splits into the
    /// node after it (a negative count) and the nodes its child offsets name.
    /// The lists follow the nodes, in node order.
    fn leaf_brush_nodes(&mut self) -> Result<Vec<LeafBrushNode>, CollisionError> {
        let mut nodes = Vec::new();
        for at in self.array(LEAF_BRUSH_NODES_AT, LEAF_BRUSH_NODE_LEN)? {
            let count = self.u16(at + 2)? as i16;
            if count > 0 && self.i64(at + 8)? != INLINE {
                return Err(CollisionError::Unread {
                    field: LEAF_BRUSH_NODES_AT,
                });
            }
            nodes.push(LeafBrushNode {
                count,
                children: [self.u16(at + 16)?, self.u16(at + 18)?],
                brushes: Vec::new(),
            });
        }
        for node in &mut nodes {
            if node.count > 0 {
                let at = self.skip(2 * node.count as usize)?;
                node.brushes = (0..node.count as usize)
                    .map(|i| self.u16(at + 2 * i).map(u32::from))
                    .collect::<Result<_, _>>()?;
            }
        }
        Ok(nodes)
    }

    /// A leaf, its brushes appended to `leaf_brushes`.
    fn leaf(
        &self,
        at: usize,
        nodes: &[LeafBrushNode],
        leaf_brushes: &mut Vec<u32>,
    ) -> Result<CollisionLeaf, CollisionError> {
        let first_brush = leaf_brushes.len();
        let root = self.u32(at + 40)? as usize;
        if root != 0 {
            let mut stack = vec![root];
            let mut budget = nodes.len();
            while let Some(index) = stack.pop() {
                let node = nodes
                    .get(index)
                    .ok_or(CollisionError::Index("leaf brush node"))?;
                budget = budget
                    .checked_sub(1)
                    .ok_or(CollisionError::Check("leaf brush trees end"))?;
                if node.count > 0 {
                    leaf_brushes.extend(&node.brushes);
                    continue;
                }
                for child in node.children.iter().rev() {
                    if *child != 0 {
                        stack.push(index + usize::from(*child));
                    }
                }
                if node.count < 0 {
                    stack.push(index + 1);
                }
            }
        }
        Ok(CollisionLeaf {
            first_box: self.u16(at)?,
            box_count: self.u16(at + 2)?,
            brush_contents: self.u32(at + 8)?,
            terrain_contents: self.u32(at + 12)?,
            mins: self.vec3(at + 16)?,
            maxs: self.vec3(at + 28)?,
            first_brush: first_brush as u32,
            brush_count: (leaf_brushes.len() - first_brush) as u32,
        })
    }

    /// The element offsets of the array a header field counts, with the
    /// pointer after it: inline data is consumed, an absent array is empty.
    fn array(&mut self, field: usize, len: usize) -> Result<Vec<usize>, CollisionError> {
        let count = self.field(field)? as usize;
        match self.i64(self.header + field + 8)? {
            INLINE => {
                let at = self.skip(count * len)?;
                Ok((0..count).map(|i| at + len * i).collect())
            }
            0 if count == 0 => Ok(Vec::new()),
            _ => Err(CollisionError::Unread { field }),
        }
    }

    fn field(&self, field: usize) -> Result<u32, CollisionError> {
        self.u32(self.header + field)
    }

    fn skip(&mut self, len: usize) -> Result<usize, CollisionError> {
        let at = self.at;
        if at + len > self.content.len() {
            return Err(CollisionError::Truncated);
        }
        self.at += len;
        Ok(at)
    }

    fn string(&mut self) -> Result<String, CollisionError> {
        let rest = self
            .content
            .get(self.at..)
            .ok_or(CollisionError::Truncated)?;
        let len = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or(CollisionError::Truncated)?;
        let text = String::from_utf8_lossy(&rest[..len]).into_owned();
        self.at += len + 1;
        Ok(text)
    }

    /// A plane reference, as an index into the drawn world's planes.
    fn plane(&self, at: usize) -> Result<u32, CollisionError> {
        let offset = self
            .u64(at)?
            .checked_sub(self.planes_ref)
            .ok_or(CollisionError::Index("plane"))? as usize;
        let index = offset / PLANE_LEN;
        if !offset.is_multiple_of(PLANE_LEN) || index >= self.plane_count {
            return Err(CollisionError::Index("plane"));
        }
        Ok(index as u32)
    }

    fn u16(&self, at: usize) -> Result<u16, CollisionError> {
        u16_at(self.content, at).ok_or(CollisionError::Truncated)
    }

    fn u32(&self, at: usize) -> Result<u32, CollisionError> {
        u32_at(self.content, at).ok_or(CollisionError::Truncated)
    }

    fn u64(&self, at: usize) -> Result<u64, CollisionError> {
        u64_at(self.content, at).ok_or(CollisionError::Truncated)
    }

    fn i64(&self, at: usize) -> Result<i64, CollisionError> {
        i64_at(self.content, at).ok_or(CollisionError::Truncated)
    }

    fn f32(&self, at: usize) -> Result<f32, CollisionError> {
        f32_at(self.content, at).ok_or(CollisionError::Truncated)
    }

    fn vec3(&self, at: usize) -> Result<[f32; 3], CollisionError> {
        vec3_at(self.content, at).ok_or(CollisionError::Truncated)
    }
}

/// The index of a run of `count` elements that a reference names, given the
/// reference of the first run (set by the first non-empty one) and where the
/// run must start.
fn consecutive(
    first: &mut Option<u64>,
    reference: u64,
    count: usize,
    len: usize,
    expected: usize,
) -> Result<usize, CollisionError> {
    if count == 0 {
        return Ok(expected);
    }
    let base = *first.get_or_insert(reference);
    let offset = reference
        .checked_sub(base)
        .ok_or(CollisionError::Check("brush runs follow one another"))? as usize;
    if offset != expected * len {
        return Err(CollisionError::Check("brush runs follow one another"));
    }
    Ok(expected)
}

/// The tree checks: every brush of a leaf touches the leaf, every triangle of
/// a box lies inside it.
fn check(collision: &WorldCollision) -> Result<(), CollisionError> {
    for leaf in &collision.leaves {
        let first = leaf.first_brush as usize;
        for &brush in &collision.leaf_brushes[first..first + leaf.brush_count as usize] {
            let brush = collision
                .brushes
                .get(brush as usize)
                .ok_or(CollisionError::Index("leaf brush"))?;
            if (0..3).any(|k| {
                brush.maxs[k] < leaf.mins[k] - SLACK || brush.mins[k] > leaf.maxs[k] + SLACK
            }) {
                return Err(CollisionError::Check("leaf brushes touch their leaf"));
            }
        }
    }
    for model in &collision.models {
        let first = model.leaf.first_brush as usize;
        let brushes = &collision.leaf_brushes[first..first + model.leaf.brush_count as usize];
        if brushes
            .iter()
            .any(|&b| b as usize >= collision.brushes.len())
        {
            return Err(CollisionError::Index("model brush"));
        }
    }
    for node in &collision.nodes {
        for child in node.children {
            let fits = match usize::try_from(child) {
                Ok(node) => node < collision.nodes.len(),
                Err(_) => ((-1 - child) as usize) < collision.leaves.len(),
            };
            if !fits {
                return Err(CollisionError::Index("node child"));
            }
        }
    }
    for b in &collision.boxes {
        if b.child_count == 0 {
            let partition = collision
                .partitions
                .get(b.index as usize)
                .ok_or(CollisionError::Index("box partition"))?;
            let first = partition.first_triangle as usize;
            let triangles = collision
                .triangles
                .get(first..first + partition.triangle_count as usize)
                .ok_or(CollisionError::Index("partition triangles"))?;
            for &vertex in triangles.iter().flatten() {
                let v = collision
                    .vertices
                    .get(vertex as usize)
                    .ok_or(CollisionError::Index("triangle vertex"))?;
                if (0..3).any(|k| (v[k] - b.origin[k]).abs() > b.half_size[k] + SLACK) {
                    return Err(CollisionError::Check("triangles inside their box"));
                }
            }
        } else if b.index as usize + usize::from(b.child_count) > collision.boxes.len() {
            return Err(CollisionError::Index("box children"));
        }
        if usize::from(b.material) >= collision.materials.len() {
            return Err(CollisionError::Index("box material"));
        }
    }
    for leaf in collision
        .leaves
        .iter()
        .chain(collision.models.iter().map(|m| &m.leaf))
    {
        if usize::from(leaf.first_box) + usize::from(leaf.box_count) > collision.boxes.len() {
            return Err(CollisionError::Index("leaf boxes"));
        }
    }
    Ok(())
}
