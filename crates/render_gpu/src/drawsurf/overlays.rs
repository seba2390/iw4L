//! A game's screen overlays, drawn over the frame after the post passes and
//! before the HUD: each a fullscreen draw of one of the game's post
//! materials with the rows its rule set, reading the frame as drawn so far
//! (the `feedback` code texture) and the material's own textures.

use super::gpu_resources::{DecodedSampler, RuntimeUploadedImageRegistry};
use super::postfx::{
    ExtractedFilm, ExtractedPostFx, PostFxGpuRefusal, PostFxSet, packed_fullscreen_vertices,
    postfx_layouts, queue_postfx_pipeline_into, require_postfx_pipeline, stretch_pic_buffers,
};
use super::postfx_dof::film_sources;
use crate::diag::render_frame_diag::GPU_SPAN_POSTFX;
use bevy::core_pipeline::{Core3d, Core3dSystems, tonemapping::tonemapping};
use bevy::prelude::*;
use bevy::render::diagnostic::RecordDiagnostics;
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery};
use bevy::render::view::{ExtractedView, ViewTarget};
use bevy::render::{Render, RenderApp, RenderSystems};
use game_api::post::{OverlayConstant, OverlayPass};
use render_material::{
    MaterialGenerationId, RuntimeCodeSources, RuntimeImageId, SamplerSource, rebind_stable_material,
};
use std::collections::HashMap;

/// The code texture of the frame as drawn so far (`feedback`).
const CODE_TEXTURE_FEEDBACK: u32 = 8;
/// The game time row.
const CODE_GAMETIME: u16 = 0x07;
/// The render target size row.
const CODE_RENDER_TARGET_SIZE: u16 = 0x16;
/// The sampler state the frame is read with, as the post chain reads it.
const POST_SAMPLER: u8 = 0x62;

/// An overlay material, with its textures by name hash and the samplers
/// their states decode to.
#[derive(Clone, Debug)]
pub struct ExtractedOverlayMaterial {
    pub film: ExtractedFilm,
    pub textures: Vec<(u32, RuntimeImageId, DecodedSampler)>,
}

/// A game's overlay materials and the frame's planned overlays.
#[derive(Clone, Debug, Default)]
pub struct ExtractedOverlays {
    pub materials: Vec<ExtractedOverlayMaterial>,
    pub passes: Vec<OverlayPass>,
}

struct OverlayMaterial {
    constants_arena: Buffer,
    constants: BindGroup,
    textures_layout: BindGroupLayoutDescriptor,
    vertex: Buffer,
    index: Buffer,
    srgb_write: bool,
    blends: bool,
}

#[derive(Resource, Default)]
struct OverlayGpu {
    owner: Option<MaterialGenerationId>,
    materials: Vec<OverlayMaterial>,
    pipelines:
        HashMap<(&'static str, TextureFormat), Result<CachedRenderPipelineId, PostFxGpuRefusal>>,
    samplers: HashMap<u32, Sampler>,
}

fn create_material(
    material: &ExtractedOverlayMaterial,
    device: &RenderDevice,
    cache: &PipelineCache,
) -> OverlayMaterial {
    let film = &material.film;
    let module = film.port.module();
    let total_size = (module.vertex_constant_len + module.pixel_constant_len) * 16
        + d3d9_sm3::texture_slot_rows(module.sampler_count) * 16;
    let (constants_layout, textures_layout) = postfx_layouts(film);
    let constants_arena = device.create_buffer(&BufferDescriptor {
        label: Some("overlay_constant_arena"),
        size: total_size.max(16) as u64,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let constants = device.create_bind_group(
        "overlay_constants",
        &cache.get_bind_group_layout(&constants_layout),
        &[BindGroupEntry {
            binding: 0,
            resource: BindingResource::Buffer(BufferBinding {
                buffer: &constants_arena,
                offset: 0,
                size: None,
            }),
        }],
    );
    let (vertex, index) = stretch_pic_buffers(device, "overlay");
    let state = super::state::GfxPassState::from_prepared(film.shell.passes()[0].state());
    OverlayMaterial {
        constants_arena,
        constants,
        textures_layout,
        vertex,
        index,
        srgb_write: state.srgb_write_enable(),
        blends: state
            .apply_change_state_0_host(AlphaMode::Opaque, false)
            .blend
            .blend_state()
            .is_some(),
    }
}

fn prepare_overlays(
    extracted: Res<ExtractedPostFx>,
    mut gpu: ResMut<OverlayGpu>,
    device: Res<RenderDevice>,
    cache: Res<PipelineCache>,
) {
    let Some(overlays) = extracted.overlays.as_ref() else {
        *gpu = OverlayGpu::default();
        return;
    };
    let generation = overlays.materials.first().map(|m| m.film.generation);
    if gpu.owner == generation {
        return;
    }
    *gpu = OverlayGpu {
        owner: generation,
        materials: overlays
            .materials
            .iter()
            .map(|material| create_material(material, &device, &cache))
            .collect(),
        ..Default::default()
    };
}

/// The code row a planned constant sets, if there is one.
fn code_row(constant: OverlayConstant) -> Option<u16> {
    Some(match constant {
        OverlayConstant::GameTime => CODE_GAMETIME,
        OverlayConstant::FlameDistortion => render_material::CODE_T6_FLAME_DISTORTION,
        OverlayConstant::OverlayTexCoord => render_material::CODE_T6_OVERLAY_TEX_COORD,
        OverlayConstant::FadeEffect => render_material::CODE_T6_FADE_EFFECT,
        OverlayConstant::ViewportDimensions => render_material::CODE_T6_VIEWPORT_DIMENSIONS,
        OverlayConstant::RenderTargetSize => CODE_RENDER_TARGET_SIZE,
        OverlayConstant::ScriptVector(at) => {
            *render_material::CODE_T6_SCRIPT_VECTOR.get(usize::from(at))?
        }
    })
}

/// What a pass binds: the frame, or a material texture's view and sampler.
enum Bound<'a> {
    Frame,
    Material(&'a TextureView, DecodedSampler),
}

struct PlannedDraw {
    material: usize,
    pipeline: CachedRenderPipelineId,
    bytes: Vec<u8>,
    /// What each sampler slot reads, by slot.
    bound: Vec<(Option<RuntimeImageId>, Option<DecodedSampler>)>,
}

#[allow(clippy::too_many_arguments)]
fn draw_overlays(
    view: ViewQuery<(&ViewTarget, &ExtractedView)>,
    mut gpu: ResMut<OverlayGpu>,
    extracted: Res<ExtractedPostFx>,
    uploaded: Res<RuntimeUploadedImageRegistry>,
    cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    mut context: RenderContext,
    mut texture_table: ResMut<super::texture_table::ExactTextureTable>,
    mut reported: Local<Option<String>>,
) {
    let Some(overlays) = extracted.overlays.as_ref() else {
        return;
    };
    if overlays.passes.is_empty() || gpu.materials.len() != overlays.materials.len() {
        return;
    }
    let mut report = |line: String| {
        if reported.as_ref() != Some(&line) {
            diag::info!(World, "{line}");
            *reported = Some(line);
        }
    };
    let (target, _) = view.into_inner();
    if target.main_texture_format() != TextureFormat::Rgba8Unorm {
        report("screen overlays: RED target format".into());
        return;
    }
    let Some(frame_sampler) = extracted.sampler else {
        return;
    };
    let size = UVec2::new(
        target.main_texture().width(),
        target.main_texture().height(),
    );
    // Plan every draw before encoding any: a pipeline still compiling or a
    // material that does not execute leaves the frame as drawn.
    let mut planned = Vec::with_capacity(overlays.passes.len());
    for pass in &overlays.passes {
        let Some(at) = overlays
            .materials
            .iter()
            .position(|m| m.film.name == pass.material)
        else {
            report(format!(
                "screen overlays: RED material={} unknown",
                pass.material
            ));
            return;
        };
        let material = &overlays.materials[at];
        let film = &material.film;
        let format = if gpu.materials[at].srgb_write {
            TextureFormat::Rgba8UnormSrgb
        } else {
            TextureFormat::Rgba8Unorm
        };
        let pipeline = gpu
            .pipelines
            .entry((film.name, format))
            .or_insert_with(|| queue_postfx_pipeline_into(film, &cache, Some(format)))
            .clone();
        let pipeline = match pipeline {
            Ok(pipeline) => pipeline,
            Err(cause) => {
                report(format!(
                    "screen overlays: RED material={} pipeline {cause:?}",
                    pass.material
                ));
                return;
            }
        };
        if let Err(cause) =
            require_postfx_pipeline(cache.get_render_pipeline_state(pipeline), film.name)
        {
            report(format!(
                "screen overlays: waiting material={} {cause:?}",
                pass.material
            ));
            return;
        }
        let Ok(mut sources): Result<RuntimeCodeSources, _> = film_sources(size.x, size.y, None)
        else {
            report("screen overlays: RED film sources".into());
            return;
        };
        for row in std::iter::once(CODE_GAMETIME)
            .chain([
                render_material::CODE_T6_FLAME_DISTORTION,
                render_material::CODE_T6_OVERLAY_TEX_COORD,
                render_material::CODE_T6_FADE_EFFECT,
                render_material::CODE_T6_VIEWPORT_DIMENSIONS,
            ])
            .chain(render_material::CODE_T6_SCRIPT_VECTOR)
        {
            sources.set_constant_rows(row, &[[0; 4]]);
        }
        for &(constant, mut row) in &pass.constants {
            let Some(code) = code_row(constant) else {
                report(format!(
                    "screen overlays: RED material={} constant {constant:?}",
                    pass.material
                ));
                return;
            };
            if constant == OverlayConstant::RenderTargetSize {
                row[0] = size.x as f32;
                row[1] = size.y as f32;
            }
            sources.set_constant_rows(code, &[row.map(f32::to_bits)]);
        }
        if sources
            .set_texture(CODE_TEXTURE_FEEDBACK, POST_SAMPLER)
            .is_err()
        {
            report("screen overlays: RED feedback texture".into());
            return;
        }
        let execution = match rebind_stable_material(&film.shell, &film.catalog, &sources) {
            Ok(execution) => execution,
            Err(cause) => {
                report(format!(
                    "screen overlays: RED material={} execute {cause:?}",
                    pass.material
                ));
                return;
            }
        };
        let Some(executed) = execution.pass(0).filter(|_| execution.pass_count() == 1) else {
            report(format!(
                "screen overlays: RED material={} pass count",
                pass.material
            ));
            return;
        };
        let constants = match film.port.pack_hit(executed) {
            Ok(constants) => constants,
            Err(cause) => {
                report(format!(
                    "screen overlays: RED material={} constants {cause:?}",
                    pass.material
                ));
                return;
            }
        };
        let mut bytes = Vec::new();
        bytes.extend_from_slice(bytemuck::cast_slice(&constants.vertex));
        bytes.extend_from_slice(bytemuck::cast_slice(&constants.pixel));
        let mut bound = Vec::with_capacity(film.port.abi().samplers.len());
        for binding in &film.port.abi().samplers {
            match binding.source {
                SamplerSource::CodeTexture {
                    index: CODE_TEXTURE_FEEDBACK,
                } => bound.push((None, None)),
                SamplerSource::MaterialTexture { name_hash } => {
                    let Some(&(_, image, sampler)) = material
                        .textures
                        .iter()
                        .find(|(hash, ..)| *hash == name_hash)
                    else {
                        report(format!(
                            "screen overlays: RED material={} texture {name_hash:#x} missing",
                            pass.material
                        ));
                        return;
                    };
                    bound.push((Some(image), Some(sampler)));
                }
                other => {
                    report(format!(
                        "screen overlays: RED material={} reads {other:?}",
                        pass.material
                    ));
                    return;
                }
            }
        }
        planned.push(PlannedDraw {
            material: at,
            pipeline,
            bytes,
            bound,
        });
    }
    // Every material texture must be uploaded before anything draws.
    for draw in &planned {
        for (image, _) in &draw.bound {
            let Some(image) = image else {
                continue;
            };
            if !uploaded
                .material_images
                .get(image.0 as usize)
                .is_some_and(|view| matches!(view, Some(Ok(_))))
            {
                report(format!(
                    "screen overlays: waiting image {} of {}",
                    image.0, overlays.materials[draw.material].film.name
                ));
                return;
            }
        }
    }
    let frame_sampler_object = gpu
        .samplers
        .entry(frame_sampler.packed_word())
        .or_insert_with(|| device.create_sampler(&frame_sampler.descriptor()))
        .clone();
    for draw in &planned {
        for (_, sampler) in &draw.bound {
            if let Some(sampler) = sampler {
                gpu.samplers
                    .entry(sampler.packed_word())
                    .or_insert_with(|| device.create_sampler(&sampler.descriptor()));
            }
        }
    }
    let diagnostics = context.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    for draw in &planned {
        let material = &gpu.materials[draw.material];
        let post = target.post_process_write();
        let encoder = context.command_encoder();
        let span = diagnostics.time_span(encoder, GPU_SPAN_POSTFX);
        if material.blends {
            // A blended overlay draws over the frame as drawn so far.
            encoder.copy_texture_to_texture(
                post.source_texture.as_image_copy(),
                post.destination_texture.as_image_copy(),
                Extent3d {
                    width: size.x,
                    height: size.y,
                    depth_or_array_layers: 1,
                },
            );
        }
        let mut views: Vec<&TextureView> = Vec::new();
        let mut samplers: Vec<&Sampler> = Vec::new();
        let mut slots = vec![
            0u32;
            d3d9_sm3::texture_slot_rows(
                overlays.materials[draw.material]
                    .film
                    .port
                    .module()
                    .sampler_count
            ) * 4
        ];
        let uploaded_views: Vec<Option<&TextureView>> = draw
            .bound
            .iter()
            .map(|(image, _)| {
                image.and_then(|image| {
                    uploaded
                        .material_images
                        .get(image.0 as usize)?
                        .as_ref()?
                        .as_ref()
                        .ok()
                        .map(|view| &view.view)
                })
            })
            .collect();
        for (ordinal, ((_, sampler), view)) in draw.bound.iter().zip(&uploaded_views).enumerate() {
            let bound = match (sampler, view) {
                (Some(sampler), Some(view)) => Bound::Material(view, *sampler),
                _ => Bound::Frame,
            };
            let (view, sampler) = match bound {
                Bound::Frame => (post.source, &frame_sampler_object),
                Bound::Material(view, sampler) => (
                    view,
                    gpu.samplers
                        .get(&sampler.packed_word())
                        .expect("created above"),
                ),
            };
            let texture = views.len() as u16;
            views.push(view);
            let sampler_index = samplers.len() as u16;
            samplers.push(sampler);
            if let Some(slot) = slots.get_mut(ordinal) {
                *slot = d3d9_sm3::texture_slot_word(texture, sampler_index);
            }
        }
        let mut bytes = draw.bytes.clone();
        bytes.extend_from_slice(bytemuck::cast_slice(&slots));
        queue.write_buffer(&material.constants_arena, 0, &bytes);
        queue.write_buffer(
            &material.vertex,
            0,
            &packed_fullscreen_vertices(size.x, size.y),
        );
        let textures = texture_table.views_bind_group(
            &device,
            &cache.get_bind_group_layout(&material.textures_layout),
            "overlay_images",
            &views,
            &samplers,
        );
        let destination_srgb;
        let output = if material.srgb_write {
            destination_srgb = post
                .destination_texture
                .create_view(&TextureViewDescriptor {
                    format: Some(TextureFormat::Rgba8UnormSrgb),
                    ..default()
                });
            &destination_srgb
        } else {
            post.destination
        };
        let attachments = [Some(RenderPassColorAttachment {
            view: output,
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Load,
                store: StoreOp::Store,
            },
            depth_slice: None,
        })];
        let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some(overlays.materials[draw.material].film.name),
            color_attachments: &attachments,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        render_pass.set_pipeline(cache.get_render_pipeline(draw.pipeline).expect("planned"));
        render_pass.set_bind_group(0, &material.constants, &[]);
        render_pass.set_bind_group(1, &textures, &[]);
        render_pass.set_vertex_buffer(0, *material.vertex.slice(..));
        render_pass.set_index_buffer(*material.index.slice(..), IndexFormat::Uint16);
        render_pass.set_viewport(0.0, 0.0, size.x as f32, size.y as f32, 0.0, 1.0);
        render_pass.draw_indexed(0..6, 0, 0..1);
        drop(render_pass);
        span.end(encoder);
    }
    report(format!(
        "screen overlays: READY [{}] size={size:?}",
        overlays
            .passes
            .iter()
            .map(|pass| pass.material.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    ));
}

pub(super) fn register(app: &mut App) {
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render_app
        .init_resource::<OverlayGpu>()
        .add_systems(Render, prepare_overlays.in_set(RenderSystems::Prepare))
        .add_systems(
            Core3d,
            draw_overlays
                .in_set(Core3dSystems::PostProcess)
                .after(tonemapping)
                .after(PostFxSet),
        );
}
