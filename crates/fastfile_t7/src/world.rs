//! A map's drawn world (`gfx_map`): vertex positions, vertex attributes,
//! triangle indices and the surfaces that cut them into draws.
//!
//! The asset's header (0x2040 bytes) holds a draw block: the vertex count with
//! the byte sizes of the position array (12 per vertex) and the attribute array
//! (20 per vertex), then the index count, each followed by an inline pointer.
//! The arrays sit later in the stream after other world data, and so do the
//! surfaces (96 bytes each), which cover the index buffer in order. The draw
//! block, the surface array and the vertex arrays are found by that shape, and
//! the result is accepted only when every triangle of every surface lies inside
//! the surface's own bounds.

use asset_core::{WorldGeometry, WorldSurface};

use crate::content::AssetList;

/// The asset type of a map's drawn world (`gfx_map`).
pub const GFX_MAP_ASSET_TYPE: u32 = 16;

const HEADER_LEN: usize = 0x2040;
/// The draw block's inline-and-registered marker, from the header's start.
const DRAW_AT: usize = 0x260;
const SURFACE_COUNT_AT: usize = 0x18;
const SURFACE_LEN: usize = 96;
const POSITION_LEN: usize = 12;
const ATTRIBUTE_LEN: usize = 20;
const REGISTERED: i64 = -2;
const INLINE: i64 = -1;
/// How far a vertex may sit outside its surface's stored bounds (rounding).
const BOUNDS_SLACK: f32 = 0.05;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorldError {
    /// Draw blocks found in the stream do not match the asset list's count.
    DrawCount { listed: usize, found: usize },
    /// No run of surfaces covers the index buffer.
    NoSurfaces,
    /// No offset puts every surface's triangles inside its bounds, or more
    /// than one does.
    VertexArrays { candidates: usize },
}

/// The zone's drawn world, if it lists one.
pub fn world_geometry(
    content: &[u8],
    list: &AssetList,
) -> Result<Option<WorldGeometry>, WorldError> {
    let listed = list
        .asset_types
        .iter()
        .filter(|&&ty| ty == GFX_MAP_ASSET_TYPE)
        .count();
    let mut headers = Vec::new();
    let mut from = list.data_at;
    while let Some(offset) = find(&content[from..], &REGISTERED.to_le_bytes()) {
        let marker = from + offset;
        from = marker + 1;
        if let Some(header) = marker.checked_sub(DRAW_AT)
            && draw_block(content, marker).is_some()
        {
            headers.push(header);
        }
    }
    if headers.len() != listed {
        return Err(WorldError::DrawCount {
            listed,
            found: headers.len(),
        });
    }
    let Some(&header) = headers.first() else {
        return Ok(None);
    };
    let draw = draw_block(content, header + DRAW_AT).unwrap();
    let surface_count = u32_at(content, header + SURFACE_COUNT_AT).unwrap_or(0) as usize;
    let data_at = header + HEADER_LEN;
    let (surfaces_at, surfaces) = surface_array(content, data_at, surface_count, draw.index_count)
        .ok_or(WorldError::NoSurfaces)?;
    let arrays_len = draw.vertex_count * (POSITION_LEN + ATTRIBUTE_LEN) + 2 * draw.index_count;
    let last = surfaces_at
        .checked_sub(arrays_len)
        .ok_or(WorldError::NoSurfaces)?;
    let candidates: Vec<usize> = (data_at..=last)
        .filter(|&at| fits(content, at, &draw, &surfaces[..surfaces.len().min(4)]))
        .filter(|&at| fits(content, at, &draw, &surfaces))
        .collect();
    let [positions_at] = candidates[..] else {
        return Err(WorldError::VertexArrays {
            candidates: candidates.len(),
        });
    };
    Ok(Some(decode(content, positions_at, &draw, surfaces)))
}

struct Draw {
    vertex_count: usize,
    index_count: usize,
}

/// The draw block at a registered marker: vertex count, position bytes
/// (12 per vertex) and pointer, attribute bytes (20 per vertex) and pointer,
/// index count and pointer.
fn draw_block(content: &[u8], marker: usize) -> Option<Draw> {
    let vertex_count = u32_at(content, marker + 8)? as usize;
    let index_count = u32_at(content, marker + 0x38)? as usize;
    let shaped = vertex_count > 0
        && index_count > 0
        && u32_at(content, marker + 0xc)? as usize == POSITION_LEN * vertex_count
        && i64_at(content, marker + 0x10)? == INLINE
        && u32_at(content, marker + 0x20)? as usize == ATTRIBUTE_LEN * vertex_count
        && i64_at(content, marker + 0x28)? == INLINE
        && i64_at(content, marker + 0x40)? == INLINE;
    shaped.then_some(Draw {
        vertex_count,
        index_count,
    })
}

/// The surface array: `count` records whose index ranges follow one another
/// from index 0 and end exactly at the index buffer's end.
fn surface_array(
    content: &[u8],
    from: usize,
    count: usize,
    index_count: usize,
) -> Option<(usize, Vec<WorldSurface>)> {
    if count == 0 {
        return None;
    }
    let end = content.len().checked_sub(count * SURFACE_LEN)?;
    (from..=end).find_map(|at| {
        let first = surface(content, at)?;
        if first.first_index != 0 || first.triangle_count == 0 {
            return None;
        }
        let mut surfaces = Vec::with_capacity(count);
        let mut next = 0usize;
        for i in 0..count {
            let surface = surface(content, at + i * SURFACE_LEN)?;
            if surface.first_index as usize != next || surface.triangle_count == 0 {
                return None;
            }
            next += 3 * usize::from(surface.triangle_count);
            surfaces.push(surface);
        }
        (next == index_count).then_some((at, surfaces))
    })
}

fn surface(content: &[u8], at: usize) -> Option<WorldSurface> {
    let position_offset = u32_at(content, at + 0x0c)?;
    let attribute_offset = u32_at(content, at + 0x1c)?;
    let ok = position_offset as usize % POSITION_LEN == 0
        && attribute_offset as usize % ATTRIBUTE_LEN == 0
        && position_offset as usize / POSITION_LEN == attribute_offset as usize / ATTRIBUTE_LEN;
    ok.then_some(())?;
    Some(WorldSurface {
        position_offset,
        attribute_offset,
        triangle_count: u16_at(content, at + 0x2a)?,
        first_index: u32_at(content, at + 0x2c)?,
        mins: vec3_at(content, at + 0x30)?,
        maxs: vec3_at(content, at + 0x3c)?,
        material: u64_at(content, at + 0x48)?,
    })
}

/// Whether, with the position array at `at` (attributes and indices right
/// after), every triangle of `surfaces` lies inside the surface's bounds.
fn fits(content: &[u8], at: usize, draw: &Draw, surfaces: &[WorldSurface]) -> bool {
    let indices_at = at + draw.vertex_count * (POSITION_LEN + ATTRIBUTE_LEN);
    surfaces.iter().all(|surface| {
        let base = surface.position_offset as usize / POSITION_LEN;
        let first = surface.first_index as usize;
        (first..first + 3 * usize::from(surface.triangle_count)).all(|i| {
            let Some(local) = u16_at(content, indices_at + 2 * i) else {
                return false;
            };
            let vertex = base + usize::from(local);
            vertex < draw.vertex_count
                && vec3_at(content, at + POSITION_LEN * vertex).is_some_and(|p| {
                    (0..3).all(|axis| {
                        p[axis] >= surface.mins[axis] - BOUNDS_SLACK
                            && p[axis] <= surface.maxs[axis] + BOUNDS_SLACK
                    })
                })
        })
    })
}

fn decode(
    content: &[u8],
    positions_at: usize,
    draw: &Draw,
    surfaces: Vec<WorldSurface>,
) -> WorldGeometry {
    let attributes_at = positions_at + POSITION_LEN * draw.vertex_count;
    let indices_at = attributes_at + ATTRIBUTE_LEN * draw.vertex_count;
    let mut geometry = WorldGeometry {
        positions: Vec::with_capacity(draw.vertex_count),
        normals: Vec::with_capacity(draw.vertex_count),
        tangents: Vec::with_capacity(draw.vertex_count),
        colors: Vec::with_capacity(draw.vertex_count),
        texture_uvs: Vec::with_capacity(draw.vertex_count),
        indices: Vec::with_capacity(draw.index_count),
        surfaces: Vec::new(),
    };
    for vertex in 0..draw.vertex_count {
        // `fits` read every position a surface uses; the arrays end before the
        // surfaces, so every read here is in range.
        geometry
            .positions
            .push(vec3_at(content, positions_at + POSITION_LEN * vertex).unwrap());
        let at = attributes_at + ATTRIBUTE_LEN * vertex;
        geometry
            .colors
            .push(content[at..at + 4].try_into().unwrap());
        geometry.texture_uvs.push([
            f32_at(content, at + 4).unwrap(),
            f32_at(content, at + 8).unwrap(),
        ]);
        geometry
            .normals
            .push(unit_vector(u32_at(content, at + 12).unwrap()));
        let tangent = u32_at(content, at + 16).unwrap();
        let [x, y, z] = unit_vector(tangent);
        geometry
            .tangents
            .push([x, y, z, if tangent >> 30 == 0 { 1.0 } else { -1.0 }]);
    }
    for surface in &surfaces {
        let base = surface.position_offset / POSITION_LEN as u32;
        let first = surface.first_index as usize;
        for i in first..first + 3 * usize::from(surface.triangle_count) {
            geometry
                .indices
                .push(base + u32::from(u16_at(content, indices_at + 2 * i).unwrap()));
        }
    }
    geometry.surfaces = surfaces;
    geometry
}

/// Three unsigned 10-bit components, low bits first, each 0..1023 standing
/// for -1..1.
fn unit_vector(packed: u32) -> [f32; 3] {
    let component = |shift: u32| ((packed >> shift) & 0x3ff) as f32 / 1023.0 * 2.0 - 1.0;
    let v = [component(0), component(10), component(20)];
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if length == 0.0 {
        [0.0, 0.0, 1.0]
    } else {
        [v[0] / length, v[1] / length, v[2] / length]
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(at..at + 2)?.try_into().unwrap(),
    ))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(at..at + 4)?.try_into().unwrap(),
    ))
}

fn u64_at(bytes: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(at..at + 8)?.try_into().unwrap(),
    ))
}

fn i64_at(bytes: &[u8], at: usize) -> Option<i64> {
    u64_at(bytes, at).map(|value| value as i64)
}

fn f32_at(bytes: &[u8], at: usize) -> Option<f32> {
    u32_at(bytes, at).map(f32::from_bits)
}

fn vec3_at(bytes: &[u8], at: usize) -> Option<[f32; 3]> {
    Some([
        f32_at(bytes, at)?,
        f32_at(bytes, at + 4)?,
        f32_at(bytes, at + 8)?,
    ])
}
