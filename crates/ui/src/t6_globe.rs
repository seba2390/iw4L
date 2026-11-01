use bevy::{
    prelude::*, reflect::TypePath, render::render_resource::AsBindGroup, shader::ShaderRef,
};

#[derive(AsBindGroup, Asset, TypePath, Debug, Clone)]
pub(crate) struct GlobeMaterial {
    #[uniform(0)]
    pub(crate) color: Vec4,
    #[uniform(1)]
    pub(crate) rotation: Vec4,
    #[texture(2)]
    #[sampler(3)]
    pub(crate) map: Handle<Image>,
}

impl UiMaterial for GlobeMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://ui/t6_globe.wgsl".into()
    }
}

pub(crate) fn register(app: &mut App) {
    bevy::asset::embedded_asset!(app, "t6_globe.wgsl");
    app.add_plugins(UiMaterialPlugin::<GlobeMaterial>::default());
}
