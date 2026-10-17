//! A game's own post chain: the post materials its rule names, taken from the
//! map's own game and admitted like the renderer's post materials, and the
//! code sources its passes execute against.

use bevy::prelude::*;
use game_api::post::{OverlayRules, PostChainRules};

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
    game_material_compile_locations(catalog, rules.materials, namespace)
}

/// The technique set and pass of each of a game's post materials.
pub(crate) fn game_material_compile_locations(
    catalog: &RuntimeMaterialCatalog,
    materials: &[&'static str],
    namespace: asset_core::AssetNamespace,
) -> Vec<(&'static str, CompileLocation)> {
    materials
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

/// The code texture of the frame as drawn so far (`feedback`), which a
/// game's overlays read.
pub const CODE_TEXTURE_FEEDBACK: u32 = 8;
/// The game time row.
const CODE_GAMETIME: u16 = super::command_context::CODE_GAMETIME;
/// The rows a game's overlays set.
pub const OVERLAY_ROWS: [u16; 4] = [
    render_material::CODE_T6_FLAME_DISTORTION,
    render_material::CODE_T6_OVERLAY_TEX_COORD,
    render_material::CODE_T6_FADE_EFFECT,
    render_material::CODE_T6_VIEWPORT_DIMENSIONS,
];

/// A game's overlays with each of their materials admitted, in the rule's
/// order, and each material's texture atlas (rows, columns).
#[derive(Clone, Debug)]
pub struct RuntimeOverlays {
    pub rules: OverlayRules,
    pub materials: Vec<RuntimePostFx>,
    pub atlases: Vec<[u8; 2]>,
    /// Each material's textures, by name hash.
    pub textures: Vec<Vec<(u32, render_material::RuntimeTextureBinding)>>,
}

/// The base every overlay draw executes against at `width` x `height`: the
/// chain's base, the game time and overlay rows zero, and the frame bound
/// as `feedback`.
pub fn overlay_sources(width: u32, height: u32) -> Option<RuntimeCodeSources> {
    let mut sources = post_chain_sources(width, height)?;
    for row in std::iter::once(CODE_GAMETIME).chain(OVERLAY_ROWS) {
        sources.set_constant_rows(row, &[[0; 4]]);
    }
    sources
        .set_texture(CODE_TEXTURE_FEEDBACK, POST_CHAIN_SAMPLER)
        .ok()?;
    Some(sources)
}

/// Admits every overlay material, or names the first that refused.
pub(crate) fn build_runtime_overlays(
    catalog: &RuntimeMaterialCatalog,
    prepared: &PreparedMaterialTable,
    programs: &RuntimeProgramRegistry,
    shaders: &[Handle<bevy::shader::Shader>],
    rules: &OverlayRules,
    namespace: asset_core::AssetNamespace,
) -> Result<RuntimeOverlays, (&'static str, PostFxAdmissionRefusal)> {
    let sources = overlay_sources(1, 1).ok_or((
        "",
        PostFxAdmissionRefusal::CodeSource(
            super::postfx_plan::PostFxSourceRefusal::InvalidDimensions {
                width: 1,
                height: 1,
            },
        ),
    ))?;
    let mut materials = Vec::with_capacity(rules.materials.len());
    let mut atlases = Vec::with_capacity(rules.materials.len());
    let mut textures = Vec::with_capacity(rules.materials.len());
    for &name in rules.materials {
        let material = postfx_material_in(catalog, name, namespace).map_err(|e| (name, e))?;
        atlases.push(material.texture_atlas.unwrap_or([0, 0]));
        textures.push(
            material
                .textures
                .iter()
                .filter_map(|(hash, binding)| Some((*hash, (*binding)?)))
                .collect(),
        );
        materials.push(
            admit_unlit_2d_material(
                catalog,
                prepared,
                programs,
                shaders,
                (material, name),
                false,
                &sources,
            )
            .map_err(|e| (name, e))?,
        );
    }
    Ok(RuntimeOverlays {
        rules: *rules,
        materials,
        atlases,
        textures,
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
    /// The local player's shellshock holds the naked vision at its own.
    shock_vision: bool,
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
    let last_stand =
        server_vision(meta.and_then(|meta| meta.view_effects.last_stand_vision.as_ref()));
    let PostVisionState {
        channels,
        server_naked,
        server_last_stand,
        client_serial,
        picked,
        shock_vision,
        ..
    } = &mut *state;
    let Some(channels) = channels.as_mut() else {
        return;
    };
    if let Some((name, duration_ms)) = naked.filter(|naked| server_naked.as_ref() != Some(naked)) {
        *server_naked = Some((name.clone(), duration_ms));
        channels.call(
            &game_api::post::VisionCall::ServerNaked { name, duration_ms },
            now_ms,
            &files,
        );
    }
    if let Some((name, duration_ms)) =
        last_stand.filter(|last_stand| server_last_stand.as_ref() != Some(last_stand))
    {
        *server_last_stand = Some((name.clone(), duration_ms));
        channels.call(
            &game_api::post::VisionCall::ServerLastStand { name, duration_ms },
            now_ms,
            &files,
        );
    }
    for (serial, call) in meta.map_or(&[][..], |meta| &meta.view_effects.client_visions) {
        if serial.wrapping_sub(*client_serial) as i32 > 0 {
            channels.call(call, now_ms, &files);
            *client_serial = *serial;
        }
    }
    if let Some(ps) = presented.player(local.0) {
        let shock = presented.shellshock(local.0);
        shellshock_vision(
            channels.as_mut(),
            shock_vision,
            shock.is_some(),
            shock
                .and_then(|shock| shock.vision.as_ref())
                .map(|vision| (vision.name.as_str(), vision.in_ms, vision.out_ms)),
            now_ms.wrapping_sub(ps.shellshock_time),
            ps.shellshock_time,
            ps.shellshock_duration,
            now_ms,
            &files,
        );
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

/// The shellshock's vision, as the game's client shock update sets it each
/// frame of a shock: blended in over the shock's in time while it lasts,
/// back to the server's naked vision over its out time once it is over, or
/// over 3 s when the shock names no vision.
#[allow(clippy::too_many_arguments)]
fn shellshock_vision(
    channels: &mut dyn game_api::post::VisionChannels,
    active: &mut bool,
    shocked: bool,
    vision: Option<(&str, i32, i32)>,
    elapsed: i32,
    start: i32,
    duration: i32,
    now_ms: i32,
    files: &dyn game_api::post::VisionFiles,
) {
    if !shocked || start == 0 || elapsed < 0 {
        return;
    }
    match vision {
        None if *active => {
            *active = false;
            channels.shellshock(None, SHOCK_VISION_RESTORE_MS, now_ms, files);
        }
        Some((name, in_ms, _)) if elapsed < duration => {
            if !*active {
                *active = true;
                channels.shellshock(Some(name), in_ms, now_ms, files);
            }
        }
        Some((_, _, out_ms)) if *active => {
            *active = false;
            channels.shellshock(None, out_ms, now_ms, files);
        }
        _ => {}
    }
}

/// How long a shock without a vision takes to restore the naked vision.
const SHOCK_VISION_RESTORE_MS: i32 = 3000;

/// The overlays this frame draws, as the game's rule planned them.
#[derive(Resource, Default)]
pub struct ScreenOverlayPasses {
    pub passes: Vec<game_api::post::OverlayPass>,
    /// The materials the last frame drew, for the log.
    logged: Vec<&'static str>,
}

/// Plans the frame's overlays from the presented client's screen.
pub(crate) fn plan_screen_overlays(
    generation: Option<Res<super::MaterialGeneration>>,
    clock: Res<net::FrameClock>,
    presented: Res<net::PresentedSnapshot>,
    local: Res<net::LocalPresentClient>,
    mut out: ResMut<ScreenOverlayPasses>,
) {
    let Some(Ok(overlays)) = generation.as_ref().and_then(|g| g.overlays.as_ref()) else {
        out.passes.clear();
        return;
    };
    let screen = presented
        .snapshot()
        .and_then(|snapshot| snapshot.meta.for_client(local.0))
        .map(|meta| meta.view_effects.client_screen.overlays)
        .unwrap_or_else(game_api::post::ScreenOverlays::default);
    out.passes = (overlays.rules.plan)(&game_api::post::OverlayFrame {
        now_ms: clock.time(),
        screen: &screen,
        atlases: &overlays.atlases,
    });
    let drawn: Vec<&'static str> = out.passes.iter().map(|pass| pass.material).collect();
    if drawn != out.logged {
        diag::info!(World, "screen overlays: {drawn:?} at {}", clock.time());
        out.logged = drawn;
    }
}

pub fn register(app: &mut App) {
    app.init_resource::<PostVisionState>()
        .init_resource::<ScreenOverlayPasses>()
        .add_systems(
            Update,
            (
                update_post_vision
                    .in_set(frame::InMatch)
                    .after(crate::prepare::scene::view_parms::stamp_prepared_scene_view)
                    .in_set(net::ClientSet::Present),
                plan_screen_overlays
                    .in_set(frame::InMatch)
                    .in_set(net::ClientSet::Present),
            ),
        );
}
