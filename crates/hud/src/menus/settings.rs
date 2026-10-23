//! A game's own dvars behind its menus' settings (`GameMenus::menu_dvars`):
//! its menus show them at their registered defaults until set, and those
//! standing for a runtime setting follow it both ways.

use std::collections::HashMap;

use asset_game::MenuCatalog;
use bevy::prelude::*;
use game_api::MenuSetting;

fn setting_value(settings: &frame::GameSettings, setting: MenuSetting) -> f32 {
    match setting {
        MenuSetting::MasterVolume => settings.master_volume,
        MenuSetting::MouseSensitivity => settings.sensitivity,
        MenuSetting::FieldOfView => settings.fov,
    }
}

fn set_setting(settings: &mut frame::GameSettings, setting: MenuSetting, value: f32) {
    match setting {
        MenuSetting::MasterVolume => settings.master_volume = value,
        MenuSetting::MouseSensitivity => settings.sensitivity = value,
        MenuSetting::FieldOfView => settings.fov = value,
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
    mut shown: Local<HashMap<&'static str, String>>,
) {
    // A match's own menus first, else the front end's.
    let namespace = session
        .as_ref()
        .and_then(|session| session.0.as_ref())
        .and_then(|menus| menus.namespace)
        .or_else(|| catalog.and_then(|catalog| catalog.namespace));
    let Some(menus) = namespace.and_then(crate::chrome::game_menus) else {
        return;
    };
    let mut moved = false;
    for dvar in menus.menu_dvars() {
        let Some(setting) = dvar.setting else {
            if dvars.get(dvar.name).is_none() {
                dvars.set(dvar.name, dvar.default);
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
        let value = setting_value(&settings, setting).to_string();
        if dvars.get(dvar.name) != Some(value.as_str()) {
            dvars.set(dvar.name, value.as_str());
        }
        shown.insert(dvar.name, value);
    }
    if moved {
        settings.sanitize();
        settings.touch();
    }
}
