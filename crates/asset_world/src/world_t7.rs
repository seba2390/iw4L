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
use asset_core::WorldGeometry;
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
