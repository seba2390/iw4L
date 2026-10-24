//! The display's gamma ramp a game sets from its menus: the last pass over
//! the frame, after the HUD, mapping each level the frame holds through the
//! ramp as the display would.

use bevy::core_pipeline::{Core3d, Core3dSystems};
use bevy::prelude::*;
use bevy::render::render_resource::binding_types::{texture_2d, uniform_buffer_sized};
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery};
use bevy::render::view::ViewTarget;
use bevy::render::{Extract, ExtractSchedule, RenderApp, RenderStartup};
use std::{collections::HashMap, num::NonZeroU64};

/// The ramp's exponent this frame, when the game sets one other than 1.
#[derive(Resource, Default)]
struct ExtractedDisplayGamma(Option<f32>);

fn extract(
    mut extracted: ResMut<ExtractedDisplayGamma>,
    gamma: Extract<Option<Res<frame::DisplayGamma>>>,
) {
    // The game's ramp is each level to the power of one over the gamma; a
    // gamma of 1 leaves the levels as they are.
    extracted.0 = gamma
        .as_ref()
        .and_then(|gamma| gamma.0)
        .map(|gamma| 1.0 / gamma)
        .filter(|exponent| exponent.is_finite() && *exponent != 1.0);
}

#[derive(Resource)]
struct DisplayGammaPipeline {
    shader: Handle<Shader>,
    layout: BindGroupLayoutDescriptor,
    ramp: Buffer,
    pipelines: HashMap<TextureFormat, CachedRenderPipelineId>,
}

impl DisplayGammaPipeline {
    fn pipeline(&mut self, format: TextureFormat, cache: &PipelineCache) -> CachedRenderPipelineId {
        *self.pipelines.entry(format).or_insert_with(|| {
            cache.queue_render_pipeline(RenderPipelineDescriptor {
                label: Some("display_gamma".into()),
                layout: vec![self.layout.clone()],
                vertex: VertexState {
                    shader: self.shader.clone(),
                    entry_point: Some("vertex".into()),
                    ..default()
                },
                fragment: Some(FragmentState {
                    shader_defs: Vec::new(),
                    constants: Vec::new(),
                    shader: self.shader.clone(),
                    entry_point: Some("fragment".into()),
                    targets: vec![Some(ColorTargetState {
                        format,
                        blend: None,
                        write_mask: ColorWrites::ALL,
                    })],
                }),
                ..default()
            })
        })
    }
}

fn init(mut commands: Commands, assets: Res<AssetServer>, device: Res<RenderDevice>) {
    commands.insert_resource(DisplayGammaPipeline {
        shader: assets.load("embedded://render_gpu/drawsurf/display_gamma.wgsl"),
        layout: BindGroupLayoutDescriptor::new(
            "display_gamma",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (
                    texture_2d(TextureSampleType::Float { filterable: false }),
                    uniform_buffer_sized(false, NonZeroU64::new(16)),
                ),
            ),
        ),
        ramp: device.create_buffer(&BufferDescriptor {
            label: Some("display_gamma_ramp"),
            size: 16,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }),
        pipelines: HashMap::new(),
    });
}

/// The frame's levels as stored, whatever view format the frame is drawn
/// through.
fn level_view(texture: &Texture) -> TextureView {
    texture.create_view(&TextureViewDescriptor {
        format: Some(texture.format().remove_srgb_suffix()),
        ..default()
    })
}

fn draw_display_gamma(
    view: ViewQuery<&ViewTarget>,
    extracted: Res<ExtractedDisplayGamma>,
    mut pipeline: ResMut<DisplayGammaPipeline>,
    cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    mut context: RenderContext,
) {
    let Some(exponent) = extracted.0 else {
        return;
    };
    let target = view.into_inner();
    let format = target.main_texture_format().remove_srgb_suffix();
    let id = pipeline.pipeline(format, &cache);
    let Some(render_pipeline) = cache.get_render_pipeline(id) else {
        return;
    };
    queue.write_buffer(
        &pipeline.ramp,
        0,
        bytemuck::cast_slice(&[exponent, 0.0, 0.0, 0.0]),
    );
    let post = target.post_process_write();
    let source = level_view(post.source_texture);
    let destination = level_view(post.destination_texture);
    let bind = device.create_bind_group(
        "display_gamma",
        &cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::sequential((&source, pipeline.ramp.as_entire_buffer_binding())),
    );
    let mut pass = context.begin_tracked_render_pass(RenderPassDescriptor {
        label: Some("display_gamma"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: &destination,
            resolve_target: None,
            depth_slice: None,
            ops: Operations {
                load: LoadOp::Load,
                store: StoreOp::Store,
            },
        })],
        ..default()
    });
    pass.set_render_pipeline(render_pipeline);
    pass.set_bind_group(0, &bind, &[]);
    pass.draw(0..3, 0..1);
}

pub(super) fn register(app: &mut App) {
    bevy::asset::embedded_asset!(app, "display_gamma.wgsl");
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render_app
        .init_resource::<ExtractedDisplayGamma>()
        .add_systems(RenderStartup, init)
        .add_systems(ExtractSchedule, extract)
        .add_systems(
            Core3d,
            draw_display_gamma
                .in_set(Core3dSystems::PostProcess)
                .after(super::iw_tess::HudTessSet),
        );
}
