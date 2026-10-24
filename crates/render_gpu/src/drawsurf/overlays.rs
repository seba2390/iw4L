//! A game's screen overlays, drawn over the frame after the post passes and
//! before the HUD: each a fullscreen draw of one of the game's post
//! materials with the rows its rule set, reading the frame as drawn so far
//! (the `feedback` code texture) and the material's own textures.

use super::gpu_resources::{DecodedSampler, RuntimeUploadedImageRegistry};
use super::postfx::{
    ExtractedFilm, ExtractedPostFx, PostFxGpuRefusal, PostFxSet,
    packed_fullscreen_vertices_colored, postfx_layouts, queue_postfx_pipeline_into,
    require_postfx_pipeline, stretch_pic_buffers,
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
use game_api::post::{BlurImage, BlurRules, OverlayConstant, OverlayFeedback, OverlayPass};
use render_material::{
    MaterialGenerationId, RuntimeCodeSources, RuntimeImageId, SamplerSource, rebind_stable_material,
};
use std::collections::HashMap;

/// The code texture of the frame as drawn so far (`feedback`).
const CODE_TEXTURE_FEEDBACK: u32 = 8;
/// The resolved scene's code texture: the overlay pass resolves the frame
/// as drawn so far into it before its overlays draw.
const CODE_TEXTURE_RESOLVED_SCENE: u32 = 10;
/// The game's first post effect texture: the image its screen blur leaves.
const CODE_TEXTURE_POST_EFFECT_0: u32 = 11;
/// The first filter tap row (eight in all).
const CODE_FILTER_TAP0: u16 = 10;
/// The game time row.
const CODE_GAMETIME: u16 = 0x07;
/// The render target size row.
const CODE_RENDER_TARGET_SIZE: u16 = 0x16;
/// The view-projection, world and world-view-projection rows.
const CODE_TRANSPOSE_VIEW_PROJECTION: u16 = 0x56;
const CODE_TRANSPOSE_WORLD0: u16 = 0x62;
const CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0: u16 = 0x6a;
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
    /// How the game blurs the frame for an overlay that asks for it.
    pub blur: Option<BlurRules>,
    pub passes: Vec<OverlayPass>,
}

struct OverlayMaterial {
    constants_layout: BindGroupLayoutDescriptor,
    textures_layout: BindGroupLayoutDescriptor,
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

fn create_material(material: &ExtractedOverlayMaterial) -> OverlayMaterial {
    let film = &material.film;
    let (constants_layout, textures_layout) = postfx_layouts(film);
    let state = super::state::GfxPassState::from_prepared(film.shell.passes()[0].state());
    OverlayMaterial {
        constants_layout,
        textures_layout,
        srgb_write: state.srgb_write_enable(),
        blends: state
            .apply_change_state_0_host(AlphaMode::Opaque, false)
            .blend
            .blend_state()
            .is_some(),
    }
}

fn prepare_overlays(extracted: Res<ExtractedPostFx>, mut gpu: ResMut<OverlayGpu>) {
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
        materials: overlays.materials.iter().map(create_material).collect(),
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
        OverlayConstant::PostFxControl(at) => {
            let at = u16::from(at);
            if at >= render_material::CODE_POST_CONTROL_COUNT {
                return None;
            }
            render_material::CODE_POST_CONTROL0 + at
        }
    })
}

/// What a draw's sampler reads.
#[derive(Clone, Copy, Debug)]
enum Source {
    /// The frame as drawn so far.
    Frame,
    /// One of the blur's images.
    Blur(BlurImage),
    /// A material texture, with its sampler.
    Material(RuntimeImageId, DecodedSampler),
}

/// Where a draw writes: over the frame, or into one of the blur's images.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Output {
    Frame,
    Blur(BlurImage),
}

struct PlannedDraw {
    material: usize,
    pipeline: CachedRenderPipelineId,
    bytes: Vec<u8>,
    /// What each sampler slot reads, by slot.
    bound: Vec<Source>,
    output: Output,
    size: UVec2,
    /// The draw's vertex colour, packed as the 2D vertices carry it.
    color: u32,
}

/// The blur's images: two working images and the blurred one.
struct BlurTextures {
    size: UVec2,
    format: TextureFormat,
    images: [Texture; 3],
    views: [TextureView; 3],
}

fn blur_index(image: BlurImage) -> usize {
    match image {
        BlurImage::Ping(at) => usize::from(at & 1),
        BlurImage::Blurred | BlurImage::Frame => 2,
    }
}

/// One planned draw, or why the frame draws none.
struct DrawRequest<'a> {
    material: &'a str,
    constants: &'a [(OverlayConstant, [f32; 4])],
    taps: Option<&'a [[f32; 4]; 8]>,
    feedback: Source,
    post_effect: Option<Source>,
    output: Output,
    size: UVec2,
    color: [u8; 4],
}

fn plan_draw(
    request: DrawRequest<'_>,
    overlays: &ExtractedOverlays,
    gpu: &mut OverlayGpu,
    cache: &PipelineCache,
) -> Result<PlannedDraw, String> {
    let name = request.material;
    let Some(at) = overlays.materials.iter().position(|m| m.film.name == name) else {
        return Err(format!("screen overlays: RED material={name} unknown"));
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
        .or_insert_with(|| queue_postfx_pipeline_into(film, cache, Some(format)))
        .clone()
        .map_err(|cause| format!("screen overlays: RED material={name} pipeline {cause:?}"))?;
    require_postfx_pipeline(cache.get_render_pipeline_state(pipeline), film.name)
        .map_err(|cause| format!("screen overlays: waiting material={name} {cause:?}"))?;
    let size = request.size;
    let mut sources: RuntimeCodeSources = film_sources(size.x, size.y, None)
        .map_err(|_| "screen overlays: RED film sources".to_owned())?;
    // A 2D view leaves the view identity: its view-projection is the
    // same 2D projection, and it draws its quads with an identity world.
    if let Some(projection) = sources.constant_arc(CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0) {
        sources.set_constant_rows(CODE_TRANSPOSE_VIEW_PROJECTION, &projection);
    }
    sources.set_constant_rows(
        CODE_TRANSPOSE_WORLD0,
        &[
            [1.0f32.to_bits(), 0, 0, 0],
            [0, 1.0f32.to_bits(), 0, 0],
            [0, 0, 1.0f32.to_bits(), 0],
            [0, 0, 0, 1.0f32.to_bits()],
        ],
    );
    for row in std::iter::once(CODE_GAMETIME)
        .chain([
            render_material::CODE_T6_FLAME_DISTORTION,
            render_material::CODE_T6_OVERLAY_TEX_COORD,
            render_material::CODE_T6_FADE_EFFECT,
            render_material::CODE_T6_VIEWPORT_DIMENSIONS,
        ])
        .chain(render_material::CODE_T6_SCRIPT_VECTOR)
        .chain(
            (0..render_material::CODE_POST_CONTROL_COUNT)
                .map(|at| render_material::CODE_POST_CONTROL0 + at),
        )
    {
        sources.set_constant_rows(row, &[[0; 4]]);
    }
    for &(constant, mut row) in request.constants {
        let Some(code) = code_row(constant) else {
            return Err(format!(
                "screen overlays: RED material={name} constant {constant:?}"
            ));
        };
        if constant == OverlayConstant::RenderTargetSize {
            row[0] = size.x as f32;
            row[1] = size.y as f32;
        }
        sources.set_constant_rows(code, &[row.map(f32::to_bits)]);
    }
    if let Some(taps) = request.taps {
        for (i, row) in taps.iter().enumerate() {
            sources.set_constant_rows(CODE_FILTER_TAP0 + i as u16, &[row.map(f32::to_bits)]);
        }
    }
    for index in [
        CODE_TEXTURE_FEEDBACK,
        CODE_TEXTURE_RESOLVED_SCENE,
        CODE_TEXTURE_POST_EFFECT_0,
    ] {
        sources
            .set_texture(index, POST_SAMPLER)
            .map_err(|_| "screen overlays: RED code texture".to_owned())?;
    }
    let execution = rebind_stable_material(&film.shell, &film.catalog, &sources)
        .map_err(|cause| format!("screen overlays: RED material={name} execute {cause:?}"))?;
    let Some(executed) = execution.pass(0).filter(|_| execution.pass_count() == 1) else {
        return Err(format!("screen overlays: RED material={name} pass count"));
    };
    let constants = film
        .port
        .pack_hit(executed)
        .map_err(|cause| format!("screen overlays: RED material={name} constants {cause:?}"))?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(bytemuck::cast_slice(&constants.vertex));
    bytes.extend_from_slice(bytemuck::cast_slice(&constants.pixel));
    let mut bound = Vec::with_capacity(film.port.abi().samplers.len());
    for binding in &film.port.abi().samplers {
        bound.push(match binding.source {
            SamplerSource::CodeTexture {
                index: CODE_TEXTURE_FEEDBACK | CODE_TEXTURE_RESOLVED_SCENE,
            } => request.feedback,
            SamplerSource::CodeTexture {
                index: CODE_TEXTURE_POST_EFFECT_0,
            } => request.post_effect.ok_or_else(|| {
                format!("screen overlays: RED material={name} reads the blur without one")
            })?,
            SamplerSource::MaterialTexture { name_hash } => {
                let Some(&(_, image, sampler)) = material
                    .textures
                    .iter()
                    .find(|(hash, ..)| *hash == name_hash)
                else {
                    return Err(format!(
                        "screen overlays: RED material={name} texture {name_hash:#x} missing"
                    ));
                };
                Source::Material(image, sampler)
            }
            other => {
                return Err(format!(
                    "screen overlays: RED material={name} reads {other:?}"
                ));
            }
        });
    }
    let [r, g, b, a] = request.color;
    Ok(PlannedDraw {
        material: at,
        pipeline,
        bytes,
        bound,
        output: request.output,
        size,
        color: u32::from_le_bytes([b, g, r, a]),
    })
}

/// Every draw the frame's overlays take, in order: each overlay's blur
/// passes, then the overlay.
fn plan_draws(
    overlays: &ExtractedOverlays,
    gpu: &mut OverlayGpu,
    cache: &PipelineCache,
    size: UVec2,
) -> Result<Vec<PlannedDraw>, String> {
    let mut planned = Vec::new();
    for pass in &overlays.passes {
        let blurred = if let Some(radius) = pass.blur {
            let Some(rules) = overlays.blur else {
                return Err(format!(
                    "screen overlays: RED material={} blurs without blur rules",
                    pass.material
                ));
            };
            let quarter = UVec2::new(
                (size.x >> rules.shift).max(1),
                (size.y >> rules.shift).max(1),
            );
            for step in (rules.plan)(radius, size.to_array(), quarter.to_array()) {
                planned.push(plan_draw(
                    DrawRequest {
                        material: step.material,
                        constants: &[],
                        taps: Some(&step.taps),
                        feedback: match step.source {
                            BlurImage::Frame => Source::Frame,
                            image => Source::Blur(image),
                        },
                        post_effect: None,
                        output: Output::Blur(step.target),
                        size: UVec2::from_array(step.size),
                        color: [255; 4],
                    },
                    overlays,
                    gpu,
                    cache,
                )?);
            }
            Some(Source::Blur(BlurImage::Blurred))
        } else {
            None
        };
        planned.push(plan_draw(
            DrawRequest {
                material: &pass.material,
                constants: &pass.constants,
                taps: None,
                feedback: match pass.feedback {
                    OverlayFeedback::Frame => Source::Frame,
                    OverlayFeedback::Blurred => blurred.ok_or_else(|| {
                        format!(
                            "screen overlays: RED material={} reads the blur without one",
                            pass.material
                        )
                    })?,
                },
                post_effect: blurred,
                output: Output::Frame,
                size,
                color: pass.color,
            },
            overlays,
            gpu,
            cache,
        )?);
    }
    Ok(planned)
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
    mut frame_copy: Local<Option<Texture>>,
    mut blur_textures: Local<Option<BlurTextures>>,
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
    let planned = match plan_draws(overlays, &mut gpu, &cache, size) {
        Ok(planned) => planned,
        Err(line) => {
            report(line);
            return;
        }
    };
    // Every material texture must be uploaded before anything draws.
    for draw in &planned {
        for source in &draw.bound {
            let Source::Material(image, _) = source else {
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
        for source in &draw.bound {
            if let Source::Material(_, sampler) = source {
                gpu.samplers
                    .entry(sampler.packed_word())
                    .or_insert_with(|| device.create_sampler(&sampler.descriptor()));
            }
        }
    }
    let main_format = target.main_texture().format();
    if let Some(blur_size) = planned.iter().find_map(|draw| match draw.output {
        Output::Blur(_) => Some(draw.size),
        Output::Frame => None,
    }) && blur_textures
        .as_ref()
        .is_none_or(|textures| textures.size != blur_size || textures.format != main_format)
    {
        let images = std::array::from_fn(|_| {
            device.create_texture(&TextureDescriptor {
                label: Some("overlay_blur_image"),
                size: Extent3d {
                    width: blur_size.x,
                    height: blur_size.y,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: main_format,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                view_formats: &[TextureFormat::Rgba8UnormSrgb],
            })
        });
        let views = std::array::from_fn(|i: usize| {
            images[i].create_view(&TextureViewDescriptor::default())
        });
        *blur_textures = Some(BlurTextures {
            size: blur_size,
            format: main_format,
            images,
            views,
        });
    }
    let diagnostics = context.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let extent = Extent3d {
        width: size.x,
        height: size.y,
        depth_or_array_layers: 1,
    };
    for draw in &planned {
        let material = &gpu.materials[draw.material];
        let film = &overlays.materials[draw.material].film;
        let encoder = context.command_encoder();
        let span = diagnostics.time_span(encoder, GPU_SPAN_POSTFX);
        let reads_frame = draw
            .bound
            .iter()
            .any(|source| matches!(source, Source::Frame));
        // A blur pass writes one of the blur's images from the frame or
        // another of them; an opaque overlay redraws the frame from the frame
        // as drawn so far; a blended one draws over the frame itself, reading
        // a copy of it when it reads the frame at all.
        let current_frame;
        let copied_frame;
        let (frame, output_texture, output): (&TextureView, &Texture, &TextureView) =
            match draw.output {
                Output::Blur(image) => {
                    let Some(textures) = blur_textures.as_ref() else {
                        span.end(encoder);
                        continue;
                    };
                    let at = blur_index(image);
                    current_frame = target
                        .main_texture()
                        .create_view(&TextureViewDescriptor::default());
                    (&current_frame, &textures.images[at], &textures.views[at])
                }
                Output::Frame if material.blends => {
                    let main = target.main_texture();
                    if reads_frame {
                        if frame_copy.as_ref().is_none_or(|copy| {
                            copy.size() != extent || copy.format() != main.format()
                        }) {
                            *frame_copy = Some(device.create_texture(&TextureDescriptor {
                                label: Some("overlay_frame_copy"),
                                size: extent,
                                mip_level_count: 1,
                                sample_count: 1,
                                dimension: TextureDimension::D2,
                                format: main.format(),
                                usage: TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING,
                                view_formats: &[],
                            }));
                        }
                        let copy = frame_copy.as_ref().expect("created above");
                        encoder.copy_texture_to_texture(
                            main.as_image_copy(),
                            copy.as_image_copy(),
                            extent,
                        );
                        copied_frame = copy.create_view(&TextureViewDescriptor::default());
                    } else {
                        copied_frame = main.create_view(&TextureViewDescriptor::default());
                    }
                    (&copied_frame, main, target.main_texture_view())
                }
                Output::Frame => {
                    let post = target.post_process_write();
                    (post.source, post.destination_texture, post.destination)
                }
            };
        let mut views: Vec<&TextureView> = Vec::new();
        let mut samplers: Vec<&Sampler> = Vec::new();
        let mut slots =
            vec![0u32; d3d9_sm3::texture_slot_rows(film.port.module().sampler_count) * 4];
        for (ordinal, source) in draw.bound.iter().enumerate() {
            let (view, sampler) = match *source {
                Source::Frame => (frame, &frame_sampler_object),
                Source::Blur(image) => {
                    let Some(textures) = blur_textures.as_ref() else {
                        continue;
                    };
                    (&textures.views[blur_index(image)], &frame_sampler_object)
                }
                Source::Material(image, sampler) => {
                    let Some(view) = uploaded
                        .material_images
                        .get(image.0 as usize)
                        .and_then(|view| view.as_ref()?.as_ref().ok())
                        .map(|view| &view.view)
                    else {
                        continue;
                    };
                    (
                        view,
                        gpu.samplers
                            .get(&sampler.packed_word())
                            .expect("created above"),
                    )
                }
            };
            let texture = views.len() as u16;
            views.push(view);
            let sampler_index = samplers.len() as u16;
            samplers.push(sampler);
            if let Some(slot) = slots.get_mut(ordinal) {
                *slot = d3d9_sm3::texture_slot_word(texture, sampler_index);
            }
        }
        // Each draw keeps its own constants and vertices: one material may
        // draw more than once in a frame (a blur's filter passes).
        let mut bytes = draw.bytes.clone();
        bytes.extend_from_slice(bytemuck::cast_slice(&slots));
        let constants_arena = device.create_buffer(&BufferDescriptor {
            label: Some("overlay_draw_constants"),
            size: (bytes.len().max(16) as u64).next_multiple_of(16),
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&constants_arena, 0, &bytes);
        let constants = device.create_bind_group(
            "overlay_constants",
            &cache.get_bind_group_layout(&material.constants_layout),
            &[BindGroupEntry {
                binding: 0,
                resource: BindingResource::Buffer(BufferBinding {
                    buffer: &constants_arena,
                    offset: 0,
                    size: None,
                }),
            }],
        );
        let (vertex, index) = stretch_pic_buffers(&device, "overlay_draw");
        queue.write_buffer(
            &vertex,
            0,
            &packed_fullscreen_vertices_colored(draw.size.x, draw.size.y, draw.color),
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
            destination_srgb = output_texture.create_view(&TextureViewDescriptor {
                format: Some(TextureFormat::Rgba8UnormSrgb),
                ..default()
            });
            &destination_srgb
        } else {
            output
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
            label: Some(film.name),
            color_attachments: &attachments,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        render_pass.set_pipeline(cache.get_render_pipeline(draw.pipeline).expect("planned"));
        render_pass.set_bind_group(0, &constants, &[]);
        render_pass.set_bind_group(1, &textures, &[]);
        render_pass.set_vertex_buffer(0, *vertex.slice(..));
        render_pass.set_index_buffer(*index.slice(..), IndexFormat::Uint16);
        render_pass.set_viewport(0.0, 0.0, draw.size.x as f32, draw.size.y as f32, 0.0, 1.0);
        render_pass.draw_indexed(0..6, 0, 0..1);
        drop(render_pass);
        span.end(encoder);
    }
    report(format!(
        "screen overlays: READY [{}] size={size:?} draws={}",
        overlays
            .passes
            .iter()
            .map(|pass| pass.material.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        planned.len()
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
                .after(PostFxSet)
                .before(super::iw_tess::HudTessSet),
        );
}
