//! A game's extra camera: the second view's colour lane, prepared with its
//! own scratch, constants and texture tables and drawn into a target of its
//! own before the main view, whose materials sample it as a code texture
//! ([`CODE_TEXTURE_EXTRA_CAM`]).

use bevy::render::render_resource::{
    Extent3d, Texture, TextureDescriptor, TextureDimension, TextureUsages,
};
use bevy::render::renderer::RenderAdapter;
use bevy::render::texture::TextureCache;

use super::encode::{ColourPassInputs, RecordMesh, TargetLoad, record_colour_pass_into};
use super::indirect::ExactIndirectDraws;
use super::prepare_camera::{
    CameraLane, CameraTargetSpec, InstalledColourPass, open_scene_table_epoch,
    prepare_camera_colour,
};
use super::*;

/// The extra view's sealed frame, while a game's extra camera is on, and its
/// target's size.
#[derive(Resource, Clone, Default)]
pub struct PublishedExtraView {
    pub frame: Option<PublishedRenderFrame>,
    pub size: [u32; 2],
}

struct ExtraCamTarget {
    size: [u32; 2],
    format: TextureFormat,
    _texture: Texture,
    views: ExactColourTargetViews,
    sampled: TextureView,
}

/// The extra camera's colour lane: everything the main camera's lane keeps
/// for itself, kept apart.
#[derive(Resource, Default)]
pub(super) struct ExtraCamLane {
    target: Option<ExtraCamTarget>,
    depth: Option<SceneDepthTexture>,
    binding_cache: ExactColourBindingCache,
    constant_arena: ExactConstantArena,
    census: ExactColourSubmitCensus,
    scratch: ColourSubmitScratch,
    texture_table: SceneTextureTables,
    cam: CameraPrepareState,
    pretess: CameraWorldPretess,
    indirect: ExactIndirectDraws,
}

fn extra_cam_target(
    device: &RenderDevice,
    size: [u32; 2],
    format: TextureFormat,
) -> ExtraCamTarget {
    let mut view_formats = Vec::new();
    for variant in [format.add_srgb_suffix(), format.remove_srgb_suffix()] {
        if variant != format && !view_formats.contains(&variant) {
            view_formats.push(variant);
        }
    }
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("exact_extra_cam"),
        size: Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
        view_formats: &view_formats,
    });
    // Sampled as the resolved scene is: its colour bytes as they were written.
    let sampled = texture.create_view(&TextureViewDescriptor {
        label: Some("exact_extra_cam_sampled"),
        format: Some(format.remove_srgb_suffix()),
        ..Default::default()
    });
    ExtraCamTarget {
        size,
        format,
        views: ExactColourTargetViews::of_texture(&texture),
        _texture: texture,
        sampled,
    }
}

/// Prepares the extra camera's draws before the main camera's, and publishes
/// its target for the main camera's materials. The target stays published
/// once drawn: a screen shows the last view its camera saw.
pub(super) fn prepare_extra_cam_colour(
    views: Query<&ViewTarget, With<Camera3d>>,
    extra: Res<PublishedExtraView>,
    (geometry, pipeline, registry, device, queue): (
        Res<ExactColourGeometry>,
        Res<ExactColourPipeline>,
        Res<ExactPipelineRegistry>,
        Res<RenderDevice>,
        Res<RenderQueue>,
    ),
    mut uploaded: ResMut<RuntimeUploadedImageRegistry>,
    mut smodel_cache_gpu: ResMut<SmodelCacheGpu>,
    mut cache: ResMut<TextureCache>,
    mut lane: ResMut<ExtraCamLane>,
) {
    let lane = lane.as_mut();
    lane.cam = CameraPrepareState::default();
    lane.depth = None;
    let Some(frame) = extra.frame.as_ref() else {
        return;
    };
    let Some(main) = views.iter().next() else {
        return;
    };
    let format = main.main_texture_format();
    let size = extra.size;
    if size[0] == 0 || size[1] == 0 {
        return;
    }
    if lane
        .target
        .as_ref()
        .is_none_or(|target| target.size != size || target.format != format)
    {
        lane.target = Some(extra_cam_target(&device, size, format));
    }
    let target = lane.target.as_ref().expect("target made above");
    let depth = cache.get(
        &device,
        TextureDescriptor {
            label: Some("exact_extra_cam_depth"),
            size: Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: SCENE_DEPTH_FORMAT,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
    );
    lane.depth = Some(SceneDepthTexture::of_cached(depth, 0.0));
    let sampled = target.sampled.clone();

    let generation = frame.world().generation;
    if lane.constant_arena.generation != generation {
        lane.constant_arena.generation = generation;
        for gpu in &mut lane.constant_arena.gpu {
            gpu.bind_group = None;
        }
    }
    // The extra camera never samples its own target: its draws that would
    // are refused for want of it.
    uploaded.extra_cam = None;
    open_scene_table_epoch(
        &mut lane.binding_cache,
        &mut lane.texture_table,
        &mut lane.scratch,
        &uploaded,
        generation,
    );
    let installed = InstalledColourPass::extra_camera(!pipeline.ports.is_empty());
    prepare_camera_colour(CameraLane {
        view: Some(CameraTargetSpec { format, samples: 1 }),
        extra_views: 0,
        frame,
        installed: &installed,
        geometry: &geometry,
        pipeline: &pipeline,
        registry: &registry,
        device: &device,
        queue: &queue,
        uploaded: &uploaded,
        smodel_cache_gpu: &mut smodel_cache_gpu,
        binding_cache: &mut lane.binding_cache,
        constant_arena: &mut lane.constant_arena,
        census: &mut lane.census,
        scratch: &mut lane.scratch,
        texture_table: &mut lane.texture_table,
        cam: &mut lane.cam,
        pretess: &mut lane.pretess,
    });
    uploaded.publish_frame_target(|registry| &mut registry.extra_cam, Some(sampled));
}

/// Draws the extra camera's view into its target, cleared to black first,
/// before the main camera's colour pass samples it.
pub(super) fn draw_extra_cam_colour(
    (geometry, smodel_cache_gpu, pipeline, registry): (
        Res<ExactColourGeometry>,
        Res<SmodelCacheGpu>,
        Res<ExactColourPipeline>,
        Res<ExactPipelineRegistry>,
    ),
    (device, queue, adapter): (Res<RenderDevice>, Res<RenderQueue>, Res<RenderAdapter>),
    mut lane: ResMut<ExtraCamLane>,
    mut context: RenderContext,
) {
    let ExtraCamLane {
        target,
        depth,
        constant_arena,
        scratch,
        texture_table,
        cam,
        pretess,
        indirect,
        ..
    } = lane.as_mut();
    if !cam.active {
        return;
    }
    cam.active = false;
    let (Some(target), Some(depth)) = (target.as_ref(), depth.as_ref()) else {
        return;
    };
    let Some(table_layout) = texture_table_layout(&pipeline) else {
        return;
    };
    let mut prepared = std::mem::take(&mut scratch.prepared);
    coalesce_exact_draws(&mut prepared);
    indirect.publish(&mut prepared, &device, &adapter, &queue);
    let tess = scratch.upload_skinned(&device, &queue);
    let input = ColourPassInputs {
        device: &device,
        depth,
        viewport: [0.0, 0.0, target.size[0] as f32, target.size[1] as f32],
        geometry: geometry::record_geometry(
            &geometry,
            &smodel_cache_gpu,
            RecordMesh {
                vertex: tess.vertex.as_ref(),
                lighting: tess.lighting.as_ref(),
                index: tess.index.as_ref(),
            },
            pretess,
        ),
        indirect_args: indirect.buffer(),
        registry: &registry,
        constants: constant_arena
            .gpu
            .each_ref()
            .map(|arena| arena.bind_group.as_ref()),
        focused_object_id: None,
    };
    let table_binds = texture_table
        .0
        .each_mut()
        .map(|table| &table.binds(&device, &registry, table_layout).scene);
    let encoder = context.command_encoder();
    record_colour_pass_into(
        &input,
        encoder,
        &target.views,
        TargetLoad::Clear(LinearRgba::BLACK),
        [table_binds[0], table_binds[1]],
        prepared.iter().filter(|draw| !draw.after_scene_resolve),
        "iw4_exact_extra_cam_pass",
    );
    record_colour_pass_into(
        &input,
        encoder,
        &target.views,
        TargetLoad::Load,
        [table_binds[2], table_binds[3]],
        prepared.iter().filter(|draw| draw.after_scene_resolve),
        "iw4_exact_extra_cam_late_pass",
    );
    scratch.prepared = prepared;
}
