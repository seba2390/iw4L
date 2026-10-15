//! A game's own post chain: the post materials its rule names, taken from the
//! map's own game and admitted like the renderer's post materials, and the
//! code sources its passes execute against.

use bevy::prelude::*;
use game_api::post::PostChainRules;

use super::postfx_plan::{
    PostFxAdmissionRefusal, RuntimePostFx, admit_unlit_2d_material, film_sources,
    game_post_material_technique, postfx_material_in,
};
use super::{
    PreparedMaterialTable, RuntimeCodeSources, RuntimeMaterialCatalog, RuntimeProgramRegistry,
};

/// The resolved scene's code texture and the sampler state the renderer's own
/// post passes read it with.
const CODE_TEXTURE_RESOLVED_SCENE: u32 = super::postfx_plan::CODE_TEXTURE_RESOLVED_SCENE;
pub const POST_CHAIN_SAMPLER: u8 = super::postfx_plan::RESOLVED_SCENE_SAMPLER;

/// A game's chain with each of its materials admitted, in the rule's order.
#[derive(Clone, Debug)]
pub struct RuntimePostChain {
    pub rules: PostChainRules,
    pub materials: Vec<RuntimePostFx>,
}

/// The base every chain pass executes against at `width` x `height`: the
/// 2D projection and render target size, every post control and HDR row
/// zero, and every post texture and the resolved scene bound.
pub fn post_chain_sources(width: u32, height: u32) -> Option<RuntimeCodeSources> {
    let mut sources = film_sources(width, height, None).ok()?;
    for row in 0..render_material::CODE_POST_CONTROL_COUNT {
        sources.set_constant_rows(render_material::CODE_POST_CONTROL0 + row, &[[0; 4]]);
    }
    for row in 0..render_material::CODE_POST_HDR_CONTROL_COUNT {
        sources.set_constant_rows(render_material::CODE_POST_HDR_CONTROL0 + row, &[[0; 4]]);
    }
    for texture in 0..render_material::CODE_POST_TEXTURE_COUNT {
        sources
            .set_texture(
                render_material::CODE_POST_TEXTURE0 + texture,
                POST_CHAIN_SAMPLER,
            )
            .ok()?;
    }
    sources
        .set_texture(CODE_TEXTURE_RESOLVED_SCENE, POST_CHAIN_SAMPLER)
        .ok()?;
    Some(sources)
}

/// A chain material's technique set and pass, or why it is refused.
type CompileLocation = Result<(usize, usize), PostFxAdmissionRefusal>;

/// The technique set and pass of each chain material, to compile.
pub(crate) fn post_chain_compile_locations(
    catalog: &RuntimeMaterialCatalog,
    rules: &PostChainRules,
    namespace: asset_core::AssetNamespace,
) -> Vec<(&'static str, CompileLocation)> {
    rules
        .materials
        .iter()
        .map(|&name| {
            let location = postfx_material_in(catalog, name, namespace)
                .and_then(|material| game_post_material_technique(catalog, material))
                .map(|(_, set_i, _)| (set_i, 0));
            (name, location)
        })
        .collect()
}

/// Admits every material of the chain, or names the first that refused.
pub(crate) fn build_runtime_post_chain(
    catalog: &RuntimeMaterialCatalog,
    prepared: &PreparedMaterialTable,
    programs: &RuntimeProgramRegistry,
    shaders: &[Handle<bevy::shader::Shader>],
    rules: &PostChainRules,
    namespace: asset_core::AssetNamespace,
) -> Result<RuntimePostChain, (&'static str, PostFxAdmissionRefusal)> {
    let sources = post_chain_sources(1, 1).ok_or((
        "",
        PostFxAdmissionRefusal::CodeSource(
            super::postfx_plan::PostFxSourceRefusal::InvalidDimensions {
                width: 1,
                height: 1,
            },
        ),
    ))?;
    let materials = rules
        .materials
        .iter()
        .map(|&name| {
            let material = postfx_material_in(catalog, name, namespace).map_err(|e| (name, e))?;
            admit_unlit_2d_material(
                catalog,
                prepared,
                programs,
                shaders,
                (material, name),
                false,
                &sources,
            )
            .map_err(|e| (name, e))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RuntimePostChain {
        rules: *rules,
        materials,
    })
}

/// The local client's vision channels, as its game's post chain keeps them,
/// and the calls they have taken.
#[derive(Resource, Default)]
pub struct PostVisionState {
    channels: Option<Box<dyn game_api::post::VisionChannels>>,
    /// The vision files the channels read from; new files start them over.
    files: Option<std::sync::Arc<asset_world::T5VisionCatalog>>,
    server_naked: Option<(String, i32)>,
    server_last_stand: Option<(String, i32)>,
    client_serial: u32,
    picked: Option<(usize, String)>,
}

struct VisionFileTexts<'a>(&'a asset_world::T5VisionCatalog);

impl game_api::post::VisionFiles for VisionFileTexts<'_> {
    fn text(&self, path: &str) -> Option<&str> {
        self.0.get(path).map(|text| &**text)
    }
}

/// A server vision as its config string holds it: a new name or time is a
/// new call, the same one again is none.
fn server_vision(vision: Option<&sim::VisionChange>) -> Option<(String, i32)> {
    vision.map(|vision| (vision.name.clone(), vision.duration_ms))
}

/// Runs the game's vision channels for this frame: the calls the server and
/// the client scripts made since the last frame, then the vision the chain
/// draws with.
pub(crate) fn update_post_vision(
    rules: Option<Res<game_api::ModeRules>>,
    mut scene: ResMut<crate::prepare::scene::world::WorldScene>,
    clock: Res<net::FrameClock>,
    presented: Res<net::PresentedSnapshot>,
    local: Res<net::LocalPresentClient>,
    mut state: ResMut<PostVisionState>,
) {
    let Some(chain) = rules.and_then(|rules| rules.post_chain) else {
        return;
    };
    if scene.vision_files.is_empty() {
        *state = PostVisionState::default();
        scene.post_vision = None;
        return;
    }
    let files_arc = std::sync::Arc::clone(&scene.vision_files);
    if !state
        .files
        .as_ref()
        .is_some_and(|files| std::sync::Arc::ptr_eq(files, &files_arc))
    {
        let channels = (chain.vision_channels)();
        for gap in channels.gaps() {
            diag::info!(World, "game gap {}: {}", gap.id, gap.what);
        }
        *state = PostVisionState {
            channels: Some(channels),
            files: Some(std::sync::Arc::clone(&files_arc)),
            ..Default::default()
        };
    }
    let files = VisionFileTexts(&files_arc);
    let now_ms = clock.time();
    let snapshot = presented.snapshot();
    let meta = snapshot.and_then(|snapshot| snapshot.meta.for_client(local.0));
    let naked = server_vision(
        meta.and_then(|meta| meta.view_effects.naked_vision.as_ref())
            .or_else(|| snapshot.and_then(|s| s.meta.objectives.naked_vision.as_ref())),
    );
    let last_stand = server_vision(meta.and_then(|meta| meta.view_effects.last_stand_vision.as_ref()));
    let PostVisionState {
        channels,
        server_naked,
        server_last_stand,
        client_serial,
        picked,
        ..
    } = &mut *state;
    let Some(channels) = channels.as_mut() else {
        return;
    };
    if naked.is_some() && naked != *server_naked {
        let (name, duration_ms) = naked.clone().unwrap_or_default();
        channels.call(
            &game_api::post::VisionCall::ServerNaked { name, duration_ms },
            now_ms,
            &files,
        );
        *server_naked = naked;
    }
    if last_stand.is_some() && last_stand != *server_last_stand {
        let (name, duration_ms) = last_stand.clone().unwrap_or_default();
        channels.call(
            &game_api::post::VisionCall::ServerLastStand { name, duration_ms },
            now_ms,
            &files,
        );
        *server_last_stand = last_stand;
    }
    for (serial, call) in meta.map_or(&[][..], |meta| &meta.view_effects.client_visions) {
        if serial.wrapping_sub(*client_serial) as i32 > 0 {
            channels.call(call, now_ms, &files);
            *client_serial = *serial;
        }
    }
    let view = game_api::post::VisionView {
        now_ms,
        last_stand: presented
            .player(local.0)
            .is_some_and(|ps| ps.in_last_stand()),
    };
    let words = channels.frame(view, &files);
    if scene.post_vision.as_deref() != Some(words) {
        scene.post_vision = Some(std::sync::Arc::from(words));
    }
    for message in channels.take_messages() {
        diag::info!(World, "vision: {message}");
    }
    let (channel, name) = channels.picked();
    if picked
        .as_ref()
        .is_none_or(|(was, was_name)| *was != channel || was_name != name)
    {
        diag::info!(World, "vision channel: {channel} {name}");
        *picked = Some((channel, name.to_owned()));
    }
}

pub fn register(app: &mut App) {
    app.init_resource::<PostVisionState>().add_systems(
        Update,
        update_post_vision
            .in_set(frame::InMatch)
            .after(crate::prepare::scene::view_parms::stamp_prepared_scene_view)
            .in_set(net::ClientSet::Present),
    );
}
