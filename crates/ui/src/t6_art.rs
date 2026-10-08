use asset_core::{AssetKey, AssetKind, AssetNamespace};
use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

#[derive(Resource, Default)]
pub(crate) struct T6Art {
    generation: frame::WorldGeneration,
    images: HashMap<String, Handle<Image>>,
    missing: HashSet<String>,
}

impl T6Art {
    pub(crate) fn reset(&mut self, generation: frame::WorldGeneration) {
        if self.generation != generation {
            self.generation = generation;
            self.images.clear();
            self.missing.clear();
        }
    }

    pub(crate) fn image(&mut self, key: &str, images: &mut Assets<Image>) -> Option<Handle<Image>> {
        let asset = if key.contains(':') {
            Some(AssetKey::parse(key).ok()?)
        } else {
            None
        };
        if asset
            .as_ref()
            .is_some_and(|a| a.namespace != AssetNamespace::T6 || a.kind != AssetKind::Material)
        {
            return None;
        }
        let name = asset.as_ref().map_or(key, |a| a.name.as_str());
        let name = asset_core::AssetRef::bare_name(name).to_ascii_lowercase();
        if name.is_empty() || self.missing.contains(&name) {
            return None;
        }
        if let Some(image) = self.images.get(&name) {
            return Some(image.clone());
        }
        let Some((width, height, pixels)) =
            asset_material::zone_ui_image(AssetNamespace::T6, &name)
        else {
            self.missing.insert(name);
            return None;
        };
        let image = images.add(crate::classes::icons::rgba_ui_image(
            width,
            height,
            (*pixels).clone(),
        ));
        self.images.insert(name, image.clone());
        Some(image)
    }
}

pub(crate) fn localized<'a>(
    catalog: &'a asset_game::LocalizeCatalog,
    key: &str,
) -> Option<&'a str> {
    let key = key.trim_start_matches('@');
    if key.contains(':') {
        let asset = AssetKey::parse(key).ok()?;
        if asset.namespace != AssetNamespace::T6 || asset.kind != AssetKind::Localize {
            return None;
        }
        catalog.text_in(AssetNamespace::T6, &asset.name)
    } else {
        catalog.text_in(AssetNamespace::T6, key)
    }
}
