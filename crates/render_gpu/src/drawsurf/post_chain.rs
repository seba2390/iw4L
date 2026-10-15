//! A game's own post chain, drawn after the 3D frame in place of the
//! renderer's post passes: the passes its rule plans for the frame, each a
//! fullscreen draw of one of the game's post materials into one of the
//! chain's render targets or the frame, with the post controls the chain has
//! set so far.

use super::postfx::{
    ExtractedFilm, ExtractedPostFx, PostFxGpuRefusal, PostFxSet, packed_fullscreen_vertices,
    postfx_layouts, queue_postfx_pipeline_into, require_postfx_pipeline,
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
use game_api::post::{PostConstant, PostFormat, PostFrame, PostImage, PostOutput, PostSampler};
use render_material::{MaterialGenerationId, RuntimeCodeSources, rebind_stable_material};
use std::collections::HashMap;
use std::sync::Arc;

/// The resolved scene's code texture and the sampler state the chain's code
/// textures are read with while the game's rule leaves them unknown
/// (`PostChainRules::sampler_state`).
const CODE_TEXTURE_RESOLVED_SCENE: u32 = 10;
const POST_SAMPLER: u8 = 0x62;

#[derive(Clone, Debug)]
pub struct ExtractedPostChain {
    pub rules: game_api::post::PostChainRules,
    /// The chain's materials, in the rule's order.
    pub materials: Vec<ExtractedFilm>,
    /// The vision the frame draws with.
    pub vision: Arc<[u32]>,
}

struct ChainTarget {
    _texture: Texture,
    view: TextureView,
    srgb_view: Option<TextureView>,
    size: UVec2,
    format: TextureFormat,
}

impl ChainTarget {
    fn new(device: &RenderDevice, size: UVec2, format: TextureFormat) -> Self {
        let srgb = (format == TextureFormat::Rgba8Unorm).then_some(TextureFormat::Rgba8UnormSrgb);
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("post_chain_target"),
            size: Extent3d {
                width: size.x,
                height: size.y,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: TextureUsages::RENDER_ATTACHMENT
                | TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_DST,
            view_formats: srgb.as_slice(),
        });
        let view = texture.create_view(&TextureViewDescriptor::default());
        let srgb_view = srgb.map(|format| {
            texture.create_view(&TextureViewDescriptor {
                format: Some(format),
                ..default()
            })
        });
        Self {
            _texture: texture,
            view,
            srgb_view,
            size,
            format,
        }
    }
}

struct ChainMaterial {
    film: ExtractedFilm,
    srgb_write: bool,
    /// The material's pass blends into its target instead of replacing it.
    blends: bool,
    textures_layout: BindGroupLayoutDescriptor,
    constants: BindGroup,
    constants_arena: Buffer,
    vertex: Buffer,
    index: Buffer,
}

#[derive(Resource, Default)]
struct PostChainGpu {
    owner: Option<(MaterialGenerationId, UVec2)>,
    targets: Vec<ChainTarget>,
    /// The frame as drawn by a pass before the chain's last frame pass.
    frame: Option<ChainTarget>,
    materials: Vec<ChainMaterial>,
    pipelines:
        HashMap<(&'static str, TextureFormat), Result<CachedRenderPipelineId, PostFxGpuRefusal>>,
    sampler: Option<Sampler>,
}

fn texture_format(format: PostFormat) -> TextureFormat {
    match format {
        PostFormat::Rgba16Unorm => TextureFormat::Rgba16Unorm,
        PostFormat::Rgba8Unorm => TextureFormat::Rgba8Unorm,
    }
}

fn create_material(
    film: ExtractedFilm,
    device: &RenderDevice,
    cache: &PipelineCache,
) -> ChainMaterial {
    let module = film.port.module();
    let total_size = (module.vertex_constant_len + module.pixel_constant_len) * 16
        + d3d9_sm3::texture_slot_rows(module.sampler_count) * 16;
    let (constants_layout, textures_layout) = postfx_layouts(&film);
    let constants_arena = device.create_buffer(&BufferDescriptor {
        label: Some("post_chain_constant_arena"),
        size: total_size.max(16) as u64,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let constants = device.create_bind_group(
        "post_chain_constants",
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
    let (vertex, index) = super::postfx::stretch_pic_buffers(device, "post_chain");
    let state = super::state::GfxPassState::from_prepared(film.shell.passes()[0].state());
    let srgb_write = state.srgb_write_enable();
    let blends = state
        .apply_change_state_0_host(AlphaMode::Opaque, false)
        .blend
        .blend_state()
        .is_some();
    ChainMaterial {
        film,
        srgb_write,
        blends,
        textures_layout,
        constants,
        constants_arena,
        vertex,
        index,
    }
}

fn prepare_post_chain(
    extracted: Res<ExtractedPostFx>,
    mut gpu: ResMut<PostChainGpu>,
    device: Res<RenderDevice>,
    cache: Res<PipelineCache>,
    views: Query<&ExtractedView>,
) {
    let (Some(chain), Some(sampler)) = (extracted.post_chain.as_ref(), extracted.sampler) else {
        *gpu = PostChainGpu::default();
        return;
    };
    let Some(view) = views.iter().find(|v| v.viewport.z > 0 && v.viewport.w > 0) else {
        return;
    };
    let size = UVec2::new(view.viewport.z, view.viewport.w);
    let Some(generation) = chain.materials.first().map(|film| film.generation) else {
        return;
    };
    if gpu.owner == Some((generation, size)) {
        return;
    }
    if gpu.owner.map(|(owner, _)| owner) != Some(generation) {
        gpu.pipelines.clear();
        gpu.materials = chain
            .materials
            .iter()
            .map(|film| create_material(film.clone(), &device, &cache))
            .collect();
        gpu.sampler = Some(device.create_sampler(&sampler.descriptor()));
    }
    gpu.targets = chain
        .rules
        .targets
        .iter()
        .map(|target| {
            let [w, h] = target.size([size.x, size.y]);
            ChainTarget::new(&device, UVec2::new(w, h), texture_format(target.format))
        })
        .collect();
    gpu.frame = Some(ChainTarget::new(&device, size, TextureFormat::Rgba8Unorm));
    gpu.owner = Some((generation, size));
    diag::info!(
        World,
        "post chain GPU: targets={:?} frame={size:?}",
        gpu.targets
            .iter()
            .map(|t| (t.size, t.format))
            .collect::<Vec<_>>()
    );
}

/// Where a pass draws, and its size.
struct Output<'a> {
    view: &'a TextureView,
    size: UVec2,
}

struct PlannedDraw {
    material: usize,
    pipeline: CachedRenderPipelineId,
    bytes: Vec<u8>,
    size: UVec2,
    images: Vec<PostImage>,
    output: PostOutput,
    to_destination: bool,
    srgb: bool,
    resolve_frame: bool,
}

fn code_constant(constant: PostConstant) -> u16 {
    match constant {
        PostConstant::Control(row) => render_material::CODE_POST_CONTROL0 + u16::from(row),
        PostConstant::Hdr(row) => render_material::CODE_POST_HDR_CONTROL0 + u16::from(row),
    }
}

fn code_texture(sampler: PostSampler) -> u32 {
    match sampler {
        PostSampler::Texture(n) => render_material::CODE_POST_TEXTURE0 + u32::from(n),
        PostSampler::ResolvedScene => CODE_TEXTURE_RESOLVED_SCENE,
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_post_chain(
    view: ViewQuery<(&ViewTarget, &ExtractedView)>,
    mut gpu: ResMut<PostChainGpu>,
    extracted: Res<ExtractedPostFx>,
    cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    mut context: RenderContext,
    mut texture_table: ResMut<super::texture_table::ExactTextureTable>,
    mut reported: Local<Option<String>>,
) {
    let Some(chain) = extracted.post_chain.as_ref() else {
        return;
    };
    let (target, _) = view.into_inner();
    let size = UVec2::new(
        target.main_texture().width(),
        target.main_texture().height(),
    );
    if gpu.owner.map(|(_, owner)| owner) != Some(size)
        || gpu.materials.len() != chain.materials.len()
        || target.main_texture_format() != TextureFormat::Rgba8Unorm
    {
        return;
    }
    let passes = (chain.rules.plan)(&PostFrame {
        display: [size.x, size.y],
        vision: &chain.vision,
    });
    let last_frame = passes
        .iter()
        .rposition(|pass| pass.output == PostOutput::Frame);
    let mut report = |line: String| {
        if reported.as_ref() != Some(&line) {
            diag::info!(World, "{line}");
            *reported = Some(line);
        }
    };
    // Plan every draw before encoding any: a pipeline still compiling or a
    // pass that does not execute leaves the frame as drawn.
    let mut state: Vec<(u16, [u32; 4])> = Vec::new();
    let mut planned = Vec::with_capacity(passes.len());
    for (i, pass) in passes.iter().enumerate() {
        let Some(material) = chain
            .rules
            .materials
            .iter()
            .position(|&m| m == pass.material)
        else {
            report(format!(
                "post chain: RED material={} not in the rule",
                pass.material
            ));
            return;
        };
        let to_destination = Some(i) == last_frame;
        let (out_size, format) = match pass.output {
            PostOutput::Target(t) => {
                let Some(target) = gpu.targets.get(usize::from(t)) else {
                    report(format!("post chain: RED target {t} missing"));
                    return;
                };
                (target.size, target.format)
            }
            PostOutput::Frame => (size, TextureFormat::Rgba8Unorm),
        };
        let srgb = gpu.materials[material].srgb_write && format == TextureFormat::Rgba8Unorm;
        let pipeline_format = if srgb {
            TextureFormat::Rgba8UnormSrgb
        } else {
            format
        };
        let pipeline = {
            let PostChainGpu {
                pipelines,
                materials,
                ..
            } = &mut *gpu;
            let film = &materials[material].film;
            pipelines
                .entry((film.name, pipeline_format))
                .or_insert_with(|| queue_postfx_pipeline_into(film, &cache, Some(pipeline_format)))
                .clone()
        };
        let pipeline = match pipeline {
            Ok(pipeline) => pipeline,
            Err(cause) => {
                report(format!(
                    "post chain: RED material={} pipeline {cause:?}",
                    pass.material
                ));
                return;
            }
        };
        let film = &gpu.materials[material].film;
        if let Err(cause) =
            require_postfx_pipeline(cache.get_render_pipeline_state(pipeline), film.name)
        {
            report(format!(
                "post chain: waiting material={} {cause:?}",
                pass.material
            ));
            return;
        }
        for &(constant, row) in &pass.constants {
            let index = code_constant(constant);
            let row = row.map(f32::to_bits);
            match state.iter_mut().find(|(at, _)| *at == index) {
                Some(slot) => slot.1 = row,
                None => state.push((index, row)),
            }
        }
        let Ok(mut sources): Result<RuntimeCodeSources, _> =
            film_sources(out_size.x, out_size.y, None)
        else {
            report("post chain: RED film sources".into());
            return;
        };
        for row in 0..render_material::CODE_POST_CONTROL_COUNT {
            sources.set_constant_rows(render_material::CODE_POST_CONTROL0 + row, &[[0; 4]]);
        }
        for row in 0..render_material::CODE_POST_HDR_CONTROL_COUNT {
            sources.set_constant_rows(render_material::CODE_POST_HDR_CONTROL0 + row, &[[0; 4]]);
        }
        for (index, row) in &state {
            sources.set_constant_rows(*index, &[*row]);
        }
        for (sampler, _) in &pass.samplers {
            if sources
                .set_texture(code_texture(*sampler), POST_SAMPLER)
                .is_err()
            {
                report("post chain: RED code texture".into());
                return;
            }
        }
        let execution = match rebind_stable_material(&film.shell, &film.catalog, &sources) {
            Ok(execution) => execution,
            Err(cause) => {
                report(format!(
                    "post chain: RED material={} execute {cause:?}",
                    pass.material
                ));
                return;
            }
        };
        let Some(executed) = execution.pass(0).filter(|_| execution.pass_count() == 1) else {
            report(format!(
                "post chain: RED material={} pass count",
                pass.material
            ));
            return;
        };
        let constants = match film.port.pack_hit(executed) {
            Ok(constants) => constants,
            Err(cause) => {
                report(format!(
                    "post chain: RED material={} constants {cause:?}",
                    pass.material
                ));
                return;
            }
        };
        let mut bytes = Vec::new();
        bytes.extend_from_slice(bytemuck::cast_slice(&constants.vertex));
        bytes.extend_from_slice(bytemuck::cast_slice(&constants.pixel));
        let mut slots =
            vec![0u32; d3d9_sm3::texture_slot_rows(film.port.module().sampler_count) * 4];
        for lane in executed.code_samplers {
            let Some(image) = pass
                .samplers
                .iter()
                .position(|(sampler, _)| code_texture(*sampler) == lane.index)
            else {
                report(format!(
                    "post chain: RED material={} reads code texture {} the pass does not bind",
                    pass.material, lane.index
                ));
                return;
            };
            let Some(ordinal) = film
                .port
                .abi()
                .samplers
                .iter()
                .position(|binding| binding.register == lane.register)
            else {
                report(format!(
                    "post chain: RED material={} sampler register",
                    pass.material
                ));
                return;
            };
            slots[ordinal] = d3d9_sm3::texture_slot_word(image as u16, 0);
        }
        bytes.extend_from_slice(bytemuck::cast_slice(&slots));
        planned.push(PlannedDraw {
            material,
            pipeline,
            bytes,
            size: out_size,
            images: pass.samplers.iter().map(|(_, image)| *image).collect(),
            output: pass.output,
            to_destination,
            srgb,
            resolve_frame: pass.resolve_frame,
        });
    }
    let Some(sampler) = gpu.sampler.as_ref() else {
        return;
    };
    let post = target.post_process_write();
    let destination_srgb = post
        .destination_texture
        .create_view(&TextureViewDescriptor {
            format: Some(TextureFormat::Rgba8UnormSrgb),
            ..default()
        });
    let diagnostics = context.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let encoder = context.command_encoder();
    let span = diagnostics.time_span(encoder, GPU_SPAN_POSTFX);
    let frame = gpu.frame.as_ref().expect("prepared with the targets");
    // The scene the passes read: the frame as drawn, then the frame each
    // resolve copies into it.
    let mut scene = post.source;
    let mut frame_written = false;
    for draw in &planned {
        if draw.resolve_frame && frame_written {
            scene = &frame.view;
        }
        let material = &gpu.materials[draw.material];
        queue.write_buffer(&material.constants_arena, 0, &draw.bytes);
        queue.write_buffer(
            &material.vertex,
            0,
            &packed_fullscreen_vertices(draw.size.x, draw.size.y),
        );
        let views: Vec<&TextureView> = draw
            .images
            .iter()
            .map(|image| match image {
                PostImage::Scene => scene,
                PostImage::Target(t) => &gpu.targets[usize::from(*t)].view,
            })
            .collect();
        let textures = texture_table.views_bind_group(
            &device,
            &cache.get_bind_group_layout(&material.textures_layout),
            "post_chain_images",
            &views,
            &[sampler, sampler],
        );
        let output = match draw.output {
            PostOutput::Target(t) => {
                let target = &gpu.targets[usize::from(t)];
                Output {
                    view: if draw.srgb {
                        target.srgb_view.as_ref().unwrap_or(&target.view)
                    } else {
                        &target.view
                    },
                    size: target.size,
                }
            }
            PostOutput::Frame if draw.to_destination => Output {
                view: if draw.srgb {
                    &destination_srgb
                } else {
                    post.destination
                },
                size,
            },
            PostOutput::Frame => {
                frame_written = true;
                Output {
                    view: if draw.srgb {
                        frame.srgb_view.as_ref().unwrap_or(&frame.view)
                    } else {
                        &frame.view
                    },
                    size,
                }
            }
        };
        let attachments = [Some(RenderPassColorAttachment {
            view: output.view,
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Clear(LinearRgba::BLACK.into()),
                store: StoreOp::Store,
            },
            depth_slice: None,
        })];
        let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some(material.film.name),
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
        render_pass.set_viewport(
            0.0,
            0.0,
            output.size.x as f32,
            output.size.y as f32,
            0.0,
            1.0,
        );
        render_pass.draw_indexed(0..6, 0, 0..1);
    }
    span.end(encoder);
    report(format!(
        "post chain: READY passes={} size={size:?} [{}] blend=[{}] srgb_write=[{}]",
        planned.len(),
        passes
            .iter()
            .map(|pass| pass.material)
            .collect::<Vec<_>>()
            .join(" "),
        gpu.materials
            .iter()
            .filter(|material| material.blends)
            .map(|material| material.film.name)
            .collect::<Vec<_>>()
            .join(" "),
        gpu.materials
            .iter()
            .filter(|material| material.srgb_write)
            .map(|material| material.film.name)
            .collect::<Vec<_>>()
            .join(" ")
    ));
}

pub(super) fn register(app: &mut App) {
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render_app
        .init_resource::<PostChainGpu>()
        .add_systems(Render, prepare_post_chain.in_set(RenderSystems::Prepare))
        .add_systems(
            Core3d,
            draw_post_chain
                .in_set(PostFxSet)
                .in_set(Core3dSystems::PostProcess)
                .after(tonemapping),
        );
}
