use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use asset_core::AssetNamespace;
use bevy::prelude::Resource;

use crate::material_images::{ZoneUiImage, ZoneUiRgba};

static NEXT_PUBLICATION: AtomicU64 = AtomicU64::new(1);
type UiKey = (AssetNamespace, String);

#[derive(Clone, Default)]
pub struct UiImageBuild {
    modules: HashMap<UiKey, Arc<[u8]>>,
    tables: HashMap<UiKey, asset_core::ZoneTable>,
    layouts: HashMap<AssetNamespace, Arc<[asset_core::BitLayout]>>,
    localized: HashMap<AssetNamespace, Arc<[(String, String)]>>,
    installed_maps: HashMap<AssetNamespace, Arc<[String]>>,
    images: HashMap<UiKey, String>,
    fallbacks: HashMap<UiKey, String>,
    zone: HashMap<UiKey, ZoneUiImage>,
    archives: HashMap<AssetNamespace, Arc<asset_transport::IwdIndex>>,
    archive_paths: HashMap<std::path::PathBuf, Arc<asset_transport::IwdIndex>>,
}

struct UiImageData {
    id: u64,
    build: UiImageBuild,
}

#[derive(Resource, Clone)]
pub struct UiImagePublication(Arc<UiImageData>);

impl Default for UiImagePublication {
    fn default() -> Self {
        UiImageBuild::default().publish()
    }
}

impl std::fmt::Debug for UiImagePublication {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("UiImagePublication")
            .field(&self.id())
            .finish()
    }
}

fn key(namespace: AssetNamespace, material: &str) -> UiKey {
    (
        namespace,
        asset_core::AssetRef::bare_name(material).to_ascii_lowercase(),
    )
}

impl UiImageBuild {
    pub fn script_tables(
        &mut self,
        namespace: AssetNamespace,
        tables: impl IntoIterator<Item = asset_core::ZoneTable>,
    ) {
        for table in tables {
            self.tables.insert(key(namespace, &table.name), table);
        }
    }

    pub fn script_modules(
        &mut self,
        namespace: AssetNamespace,
        modules: impl IntoIterator<Item = (String, Arc<[u8]>)>,
    ) {
        self.modules.extend(
            modules
                .into_iter()
                .map(|(name, bytes)| ((namespace, name.to_ascii_lowercase()), bytes)),
        );
    }

    pub fn script_layouts(
        &mut self,
        namespace: AssetNamespace,
        layouts: Vec<asset_core::BitLayout>,
    ) {
        self.layouts.insert(namespace, layouts.into());
    }

    pub fn script_localized(
        &mut self,
        namespace: AssetNamespace,
        localized: Vec<(String, String)>,
    ) {
        self.localized.insert(namespace, localized.into());
    }

    pub fn script_installed_maps(&mut self, namespace: AssetNamespace, maps: Vec<String>) {
        self.installed_maps.insert(namespace, maps.into());
    }
    pub fn retain_materials(&mut self, materials: &crate::MaterialCatalog) {
        for material in &materials.materials {
            if let Some(image) = materials.hud_image_name(material) {
                self.images.insert(
                    key(material.namespace, material.name.as_str()),
                    image.to_owned(),
                );
            }
        }
    }

    pub fn preview_fallback(&mut self, namespace: AssetNamespace, material: &str, image: &str) {
        self.fallbacks
            .insert(key(namespace, material), image.to_owned());
    }

    pub fn zone_images(
        &mut self,
        namespace: AssetNamespace,
        images: impl IntoIterator<Item = (String, ZoneUiImage)>,
    ) {
        self.zone.extend(
            images
                .into_iter()
                .map(|(name, image)| (key(namespace, &name), image)),
        );
    }

    pub fn retain_archives(&mut self, games: &asset_transport::GamesRoot) -> Result<(), String> {
        let trees = asset_transport::NamespaceTrees::discover(games);
        let mut first_error = None;
        for namespace in [
            AssetNamespace::Iw4,
            AssetNamespace::Iw5,
            AssetNamespace::T5,
            AssetNamespace::T6,
        ] {
            if let Some(main) = trees.main_for(namespace) {
                match asset_transport::IwdIndex::open(main) {
                    Ok(index) => self.archive_source(namespace, main.to_path_buf(), index),
                    Err(error) if first_error.is_none() => first_error = Some(error),
                    Err(_) => {}
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    pub fn archive_source(
        &mut self,
        namespace: AssetNamespace,
        main: std::path::PathBuf,
        index: Arc<asset_transport::IwdIndex>,
    ) {
        self.archive_paths.insert(main, index.clone());
        self.archives.insert(namespace, index);
    }

    pub fn publish(self) -> UiImagePublication {
        UiImagePublication(Arc::new(UiImageData {
            id: NEXT_PUBLICATION.fetch_add(1, Ordering::Relaxed),
            build: self,
        }))
    }
}

impl UiImagePublication {
    pub fn script_tables(&self, namespace: AssetNamespace) -> Vec<asset_core::ZoneTable> {
        self.0
            .build
            .tables
            .iter()
            .filter(|((family, _), _)| *family == namespace)
            .map(|(_, table)| table.clone())
            .collect()
    }

    pub fn script_modules(&self, namespace: AssetNamespace) -> Vec<(String, Arc<[u8]>)> {
        let mut modules: Vec<_> = self
            .0
            .build
            .modules
            .iter()
            .filter(|((family, _), _)| *family == namespace)
            .map(|((_, name), bytes)| (name.clone(), bytes.clone()))
            .collect();
        modules.sort_by(|a, b| a.0.cmp(&b.0));
        modules
    }

    pub fn script_layouts(&self, namespace: AssetNamespace) -> Arc<[asset_core::BitLayout]> {
        self.0
            .build
            .layouts
            .get(&namespace)
            .cloned()
            .unwrap_or_else(|| Arc::from(&[][..]))
    }

    pub fn script_localized(&self, namespace: AssetNamespace) -> Arc<[(String, String)]> {
        self.0
            .build
            .localized
            .get(&namespace)
            .cloned()
            .unwrap_or_else(|| Arc::from(&[][..]))
    }

    pub fn script_installed_maps(&self, namespace: AssetNamespace) -> Arc<[String]> {
        self.0
            .build
            .installed_maps
            .get(&namespace)
            .cloned()
            .unwrap_or_else(|| Arc::from(&[][..]))
    }

    pub fn with_assets(
        &self,
        namespace: AssetNamespace,
        images: impl IntoIterator<Item = (String, ZoneUiImage)>,
        modules: impl IntoIterator<Item = (String, Arc<[u8]>)>,
    ) -> Self {
        let mut build = self.0.build.clone();
        build.zone_images(namespace, images);
        build.script_modules(namespace, modules);
        build.publish()
    }
    pub fn id(&self) -> u64 {
        self.0.id
    }

    /// This publication plus one match's own zone images, as a new publication.
    pub fn with_zone_images(
        &self,
        namespace: AssetNamespace,
        images: impl IntoIterator<Item = (String, ZoneUiImage)>,
    ) -> Self {
        let mut build = self.0.build.clone();
        build.zone_images(namespace, images);
        build.publish()
    }

    pub(crate) fn archive(
        &self,
        namespace: AssetNamespace,
    ) -> Option<&Arc<asset_transport::IwdIndex>> {
        self.0.build.archives.get(&namespace)
    }
    pub(crate) fn archive_at(
        &self,
        main: &std::path::Path,
    ) -> Option<&Arc<asset_transport::IwdIndex>> {
        self.0.build.archive_paths.get(main)
    }

    pub fn material_image(&self, namespace: AssetNamespace, material: &str) -> Option<&str> {
        self.0
            .build
            .images
            .get(&key(namespace, material))
            .map(String::as_str)
    }

    pub fn preview_fallback(&self, namespace: AssetNamespace, material: &str) -> Option<&str> {
        self.0
            .build
            .fallbacks
            .get(&key(namespace, material))
            .map(String::as_str)
    }

    pub fn has_zone_image(&self, namespace: AssetNamespace, material: &str) -> bool {
        self.0.build.zone.contains_key(&key(namespace, material))
    }

    pub fn zone_material_state(
        &self,
        namespace: AssetNamespace,
        material: &str,
    ) -> Option<render_material::CompiledPassState> {
        self.0
            .build
            .zone
            .get(&key(namespace, material))
            .and_then(|image| image.state)
    }

    pub fn zone_image(&self, namespace: AssetNamespace, material: &str) -> Option<ZoneUiRgba> {
        let image = self.0.build.zone.get(&key(namespace, material))?;
        if image.rgba.is_some() {
            return image.rgba.clone();
        }
        match crate::decode_iwi_rgba(&image.iwi) {
            Ok((width, height, rgba)) => Some((width, height, Arc::new(rgba))),
            Err(error) => {
                diag::warn!(Zone, "zone UI image {material}: {error}");
                None
            }
        }
    }
}
