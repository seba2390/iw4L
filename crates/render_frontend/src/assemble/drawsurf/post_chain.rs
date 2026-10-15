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
