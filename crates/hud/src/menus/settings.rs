//! A game's own dvars behind its menus' settings (`GameMenus::menu_dvars`):
//! its menus show them at their registered defaults until set, and those
//! standing for a runtime setting follow it both ways.

use std::collections::HashMap;

use asset_game::MenuCatalog;
use bevy::prelude::*;
use game_api::MenuSetting;

/// The runtime setting's value, for a setting the runtime keeps; the
/// others live in the menus' dvars.
fn setting_value(settings: &frame::GameSettings, setting: MenuSetting) -> Option<f32> {
    match setting {
        MenuSetting::MasterVolume | MenuSetting::SquaredMasterVolume => {
            Some(settings.master_volume)
        }
        MenuSetting::MouseSensitivity => Some(settings.sensitivity),
        MenuSetting::FieldOfView => Some(settings.fov),
        MenuSetting::DisplayGamma | MenuSetting::CategoryVolume(_) => None,
    }
}

fn set_setting(settings: &mut frame::GameSettings, setting: MenuSetting, value: f32) {
    match setting {
        MenuSetting::MasterVolume | MenuSetting::SquaredMasterVolume => {
            settings.master_volume = value
        }
        MenuSetting::MouseSensitivity => settings.sensitivity = value,
        MenuSetting::FieldOfView => settings.fov = value,
        MenuSetting::DisplayGamma | MenuSetting::CategoryVolume(_) => {}
    }
}

/// A value a menu set since the last frame moves its setting; the setting's
/// value is then what the menus show.
pub(crate) fn sync_game_menu_dvars(
    (session, catalog): (
        Option<Res<asset_game::SessionHudMenus>>,
        Option<Res<MenuCatalog>>,
    ),
    mut dvars: ResMut<frame::UiMenuDvars>,
    mut settings: ResMut<frame::GameSettings>,
    (mut gamma, mut volumes, mut master_preset, mut master_gain): (
        ResMut<frame::DisplayGamma>,
        ResMut<frame::SoundCategoryVolumes>,
        ResMut<frame::MasterPreset>,
        ResMut<frame::MasterVolumeGain>,
    ),
    mut shown: Local<HashMap<&'static str, String>>,
) {
    // A match's own menus first, else the front end's.
    let namespace = session
        .as_ref()
        .and_then(|session| session.0.as_ref())
        .and_then(|menus| menus.namespace)
        .or_else(|| catalog.and_then(|catalog| catalog.namespace));
    let Some(menus) = namespace.and_then(crate::chrome::game_menus) else {
        gamma.set_if_neq(frame::DisplayGamma::default());
        volumes.set_if_neq(frame::SoundCategoryVolumes::default());
        master_preset.set_if_neq(frame::MasterPreset::default());
        master_gain.set_if_neq(frame::MasterVolumeGain::default());
        return;
    };
    let mut moved = false;
    let mut display_gamma = None;
    let mut category_volumes = Vec::new();
    for dvar in menus.menu_dvars() {
        let Some(setting) = dvar
            .setting
            .filter(|setting| setting_value(&settings, *setting).is_some())
        else {
            if dvars.get(dvar.name).is_none() {
                dvars.set(dvar.name, dvar.default);
            }
            let value = dvars
                .get(dvar.name)
                .and_then(|value| value.parse::<f32>().ok())
                .filter(|value| value.is_finite());
            match dvar.setting {
                Some(MenuSetting::DisplayGamma) => display_gamma = value,
                Some(MenuSetting::CategoryVolume(categories)) => category_volumes.extend(
                    categories
                        .iter()
                        .map(|category| (*category, value.unwrap_or(1.0))),
                ),
                _ => {}
            }
            continue;
        };
        let set_by_menu = dvars
            .get(dvar.name)
            .filter(|value| shown.get(dvar.name).is_some_and(|shown| shown != value))
            .and_then(|value| value.parse::<f32>().ok())
            .filter(|value| value.is_finite());
        if let Some(value) = set_by_menu {
            set_setting(&mut settings, setting, value);
            moved = true;
        }
        let Some(value) = setting_value(&settings, setting) else {
            continue;
        };
        let value = value.to_string();
        if dvars.get(dvar.name) != Some(value.as_str()) {
            dvars.set(dvar.name, value.as_str());
        }
        shown.insert(dvar.name, value);
    }
    gamma.set_if_neq(frame::DisplayGamma(display_gamma));
    volumes.set_if_neq(frame::SoundCategoryVolumes(category_volumes));
    let squared = menus
        .menu_dvars()
        .iter()
        .any(|dvar| dvar.setting == Some(MenuSetting::SquaredMasterVolume));
    master_gain.set_if_neq(frame::MasterVolumeGain(
        squared.then(|| settings.master_volume * settings.master_volume),
    ));
    master_preset.set_if_neq(frame::MasterPreset(
        menus.master_preset(&|name| dvars.get(name).map(str::to_owned)),
    ));
    if moved {
        settings.sanitize();
        settings.touch();
    }
}
