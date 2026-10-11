//! Black Ops 3's drawn world, from the geometry `fastfile_t7` reads: positions,
//! normals, tangents, colours, texture coordinates, indices and surfaces.
//! Materials, lightmaps, lights, probes and visibility are not read yet: every
//! surface draws without a material and nothing is culled.

use crate::capture::world_capture_from_casters;
use crate::caster::SurfaceCastsSunShadow;
use crate::world_draw::{
    CameraRangeKind, CameraSurfRange, CameraSurfRanges, DpvsWorldData, HostVertex,
    SurfaceDrawFields, WorldDraw, WorldLightmapGap, WorldVertexPayload,
};
use crate::world_mesh::WorldMeshStats;
use asset_core::{WorldCollision, WorldGeometry};
use asset_model::{pack_unit_vec, unpack_color};

pub fn build_world_draw(geometry: &WorldGeometry) -> WorldDraw {
    let vertex_count = geometry.positions.len();
    let mut packed_vertices = Vec::with_capacity(vertex_count);
    let mut colors = Vec::with_capacity(vertex_count);
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for vertex in 0..vertex_count {
        let position = geometry.positions[vertex];
        for axis in 0..3 {
            min[axis] = min[axis].min(position[axis]);
            max[axis] = max[axis].max(position[axis]);
        }
        let tangent = geometry.tangents[vertex];
        let [r, g, b, a] = geometry.colors[vertex];
        // The host vertex keeps the colour blue first.
        let color = u32::from_le_bytes([b, g, r, a]);
        let uv = geometry.texture_uvs[vertex];
        let mut host: HostVertex = [0u8; core::mem::size_of::<HostVertex>()];
        for (axis, value) in position.iter().enumerate() {
            host[4 * axis..4 * axis + 4].copy_from_slice(&value.to_le_bytes());
        }
        host[12..16].copy_from_slice(&tangent[3].to_le_bytes());
        host[16..20].copy_from_slice(&color.to_le_bytes());
        host[20..24].copy_from_slice(&uv[0].to_le_bytes());
        host[24..28].copy_from_slice(&uv[1].to_le_bytes());
        host[36..40].copy_from_slice(&pack_unit_vec(geometry.normals[vertex]).to_le_bytes());
        host[40..44]
            .copy_from_slice(&pack_unit_vec([tangent[0], tangent[1], tangent[2]]).to_le_bytes());
        packed_vertices.push(host);
        colors.push(unpack_color(color));
    }
    let surface_count = geometry.surfaces.len();
    let mut surface_index_ranges = Vec::with_capacity(surface_count);
    let mut surface_draw_fields = Vec::with_capacity(surface_count);
    let mut start = 0u32;
    for surface in &geometry.surfaces {
        let count = 3 * u32::from(surface.triangle_count);
        surface_index_ranges.push((start, count));
        surface_draw_fields.push(SurfaceDrawFields {
            first_vertex: 0,
            tri_count: surface.triangle_count,
            base_index: start,
            lightmap_index: 0,
            reflection_probe_index: 0,
            primary_light_index: 0,
        });
        start += count;
    }
    let surface_materials = vec![None; surface_count];
    let surface_lightmapped = vec![false; surface_count];
    let surface_bytes = vec![0u8; surface_count];
    let lightmap_uvs = vec![[0.0f32; 2]; vertex_count];
    let (batches, surface_batch_ranges) = crate::world_t5::make_material_batches(
        &geometry.positions,
        &geometry.normals,
        &geometry.tangents,
        &colors,
        &geometry.texture_uvs,
        &lightmap_uvs,
        &geometry.indices,
        &surface_index_ranges,
        &surface_materials,
        &surface_lightmapped,
        &surface_bytes,
        &surface_bytes,
        &surface_bytes,
    );
    let stats = WorldMeshStats {
        vertices: vertex_count,
        triangles: geometry.indices.len() / 3,
        surfaces: surface_count,
        min,
        max,
        bounds: Some([min[0], min[1], min[2], max[0], max[1], max[2]]),
        unrouted_surfaces: surface_count,
        ..Default::default()
    };
    WorldDraw {
        batches,
        sky_model: None,
        lightmap: Err(WorldLightmapGap::Missing),
        stats,
        packed_vertices: WorldVertexPayload::Host(packed_vertices),
        vertex_layer: Vec::new(),
        surface_vertex_layer: Vec::new(),
        surface_first_vertex: vec![0; surface_count],
        surface_draw_fields,
        positions: geometry.positions.clone(),
        normals: geometry.normals.clone(),
        tangents: geometry.tangents.clone(),
        colors,
        texture_uvs: geometry.texture_uvs.clone(),
        lightmap_uvs,
        packed_indices: geometry.indices.clone(),
        surface_index_ranges,
        surface_batch_ranges,
        surface_lightmapped,
        surface_lightmap_indices: surface_bytes.clone(),
        surface_reflection_probes: surface_bytes.clone(),
        surface_primary_lights: surface_bytes,
        sort_key_distortion: None,
        capture: world_capture_from_casters(SurfaceCastsSunShadow::with_len(surface_count)),
        brush_models: Vec::new(),
        brush_model_bounds: Vec::new(),
        surface_materials,
        primary_lights: Vec::new(),
        light_defs: Vec::new(),
        sun_primary_light_count: 0,
        sun_stages: Vec::new(),
        light_region_hulls: None,
        shadow_geometry: Vec::new(),
        reflection_probes: Vec::new(),
        dpvs: DpvsWorldData::new(CameraSurfRanges::new(
            CameraSurfRange {
                kind: CameraRangeKind::LitOpaque,
                begin: 0,
                end: surface_count as u32,
            },
            Vec::new(),
        )),
        outdoor_image_name: None,
        outdoor_image: None,
        outdoor_lookup: [0; 16],
        sun_effects: None,
        t5_sun_parse_exposure: None,
        t6_exposure: None,
        sky_dynamic_intensity: None,
        t5_sun_light: None,
        t5_tree_scatter_intensity: None,
        t5_tree_scatter_amount: None,
        t5_exposure_volume_count: 0,
    }
}

/// Black Ops 3's collision in the shape the simulation traces: brushes as
/// planes (the six axial sides first, then the others), leaves with their
/// brush lists, and the triangle trees. Static models and the brush models
/// of triggers are not read yet.
pub fn build_clip_collision(collision: &WorldCollision) -> Result<crate::ClipCollision, String> {
    use crate::clip_collision::{ClipMesh, ClipMeshBox, ClipMeshPartition};
    use crate::{ClipBrush, ClipBspLeaf, ClipBspNode, ClipCmodel, ClipMapMaterial};
    let mut out = crate::ClipCollision {
        materials: collision
            .materials
            .iter()
            .map(|m| ClipMapMaterial {
                // A name the zone refers to elsewhere is not resolved yet.
                name: m.name.clone().unwrap_or(String::new()),
                surface_flags: m.surface_flags,
                content_flags: m.contents,
            })
            .collect(),
        ..Default::default()
    };
    let plane = |index: u32| collision.planes[index as usize];
    for brush in &collision.brushes {
        let mut planes = Vec::with_capacity(6 + brush.sides.len());
        for side in 0..2 {
            for axis in 0..3 {
                let mut plane = [0.0; 4];
                plane[axis] = if side == 0 { -1.0 } else { 1.0 };
                plane[3] = if side == 0 {
                    -brush.mins[axis]
                } else {
                    brush.maxs[axis]
                };
                planes.push(plane);
            }
        }
        let mut flags = brush.axial_surface_flags.to_vec();
        for &(index, surface_flags) in &brush.sides {
            planes.push(plane(index));
            flags.push(surface_flags);
        }
        out.brushes.push(ClipBrush {
            planes,
            contents: brush.contents,
            plane_surface_flags: flags,
            glass_encoded: 0,
        });
    }
    out.nodes = collision
        .nodes
        .iter()
        .map(|node| ClipBspNode {
            plane: plane(node.plane),
            children: node.children,
        })
        .collect();
    out.leafbrushes = collision
        .leaf_brushes
        .iter()
        .map(|&brush| u16::try_from(brush).map_err(|_| "t7 brush index beyond 16 bits"))
        .collect::<Result<_, _>>()?;
    let leaf = |leaf: &asset_core::CollisionLeaf| -> Result<ClipBspLeaf, String> {
        Ok(ClipBspLeaf {
            first_brush: leaf.first_brush,
            num_brushes: u16::try_from(leaf.brush_count)
                .map_err(|_| "t7 leaf has too many brushes")?,
            first_coll_aabb_index: leaf.first_box,
            coll_aabb_count: leaf.box_count,
        })
    };
    out.leaves = collision
        .leaves
        .iter()
        .map(leaf)
        .collect::<Result<_, _>>()?;
    for model in &collision.models {
        let leaf = leaf(&model.leaf)?;
        out.cmodels.push(ClipCmodel {
            mins: model.mins,
            maxs: model.maxs,
            radius: model.radius,
            first_brush: leaf.first_brush,
            num_brushes: leaf.num_brushes,
        });
    }
    let triangle_count = collision.triangles.len();
    let mut mesh = ClipMesh {
        verts: collision.vertices.clone(),
        tri_indices: collision.triangles.iter().flatten().copied().collect(),
        ..Default::default()
    };
    let bits = &collision.walkable_edges;
    mesh.tri_edge_is_walkable = (0..triangle_count)
        .map(|i| {
            (0..3).fold(0u8, |value, e| {
                let bit = 3 * i + e;
                value | ((bits[bit / 8] >> (bit % 8)) & 1) << e
            })
        })
        .collect();
    for partition in &collision.partitions {
        mesh.partitions.push(ClipMeshPartition {
            tri_count: u8::try_from(partition.triangle_count)
                .map_err(|_| "t7 partition has too many triangles")?,
            first_tri: i32::try_from(partition.first_triangle)
                .map_err(|_| "t7 triangle index beyond 31 bits")?,
            ..Default::default()
        });
    }
    for node in &collision.boxes {
        mesh.aabb_trees.push(ClipMeshBox {
            origin: node.origin,
            half_size: node.half_size,
            material_index: node.material,
            child_count: node.child_count,
            u: i32::try_from(node.index).map_err(|_| "t7 box index beyond 31 bits")?,
        });
    }
    mesh.aabb_roots = collision
        .leaves
        .iter()
        .flat_map(|l| l.first_box..l.first_box.saturating_add(l.box_count))
        .collect();
    mesh.aabb_roots.sort_unstable();
    mesh.aabb_roots.dedup();
    mesh.tri_surface_flags.resize(triangle_count, 0);
    mesh.tri_content_flags.resize(triangle_count, 0);
    out.tri_material_index.resize(triangle_count, 0);
    // `fastfile_t7` checked every leaf box's partition and triangles.
    for node in collision.boxes.iter().filter(|b| b.child_count == 0) {
        let partition = collision.partitions[node.index as usize];
        let material = &collision.materials[usize::from(node.material)];
        let first = partition.first_triangle as usize;
        for i in first..first + partition.triangle_count as usize {
            mesh.tri_surface_flags[i] = material.surface_flags;
            mesh.tri_content_flags[i] = material.contents;
            out.tri_material_index[i] = node.material;
        }
    }
    out.mesh = std::sync::Arc::new(mesh);
    Ok(out)
}
