use bevy::{
    prelude::*,
    window::{MonitorSelection, PresentMode, PrimaryWindow},
};
use std::collections::BTreeMap;

#[derive(Resource, Clone, Debug, Default)]
pub struct BindingView {
    pub chords: BTreeMap<u32, String>,
    pub listening: Option<u32>,
    pub revision: u64,
}

impl BindingView {
    pub fn chord(&self, command_id: u32) -> &str {
        self.chords
            .get(&command_id)
            .map(String::as_str)
            .unwrap_or("UNBOUND")
    }
}

#[derive(Resource)]
pub struct PresentModeOverride(pub PresentMode);

pub(crate) fn apply_window_settings(
    settings: Res<frame::GameSettings>,
    present_override: Option<Res<PresentModeOverride>>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut applied: Local<Option<(frame::DisplayResolution, bool, PresentMode, u32)>>,
) {
    let present_mode = present_override.map_or_else(
        || {
            if settings.vsync {
                PresentMode::AutoVsync
            } else {
                PresentMode::AutoNoVsync
            }
        },
        |mode| mode.0,
    );
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    // The desktop's scale is known only once the window exists; the size is
    // applied again when it arrives or changes (a move to another display).
    let display = (
        settings.resolution,
        settings.fullscreen,
        present_mode,
        window.resolution.scale_factor().to_bits(),
    );
    if applied.as_ref() == Some(&display) {
        return;
    }
    // The display setting names the window's size in the desktop's own units,
    // so a 1280x720 window on a Retina display covers 1280x720 points rather
    // than a quarter of that; the drawn image follows the display's pixels.
    window.resolution.set(
        settings.resolution.width as f32,
        settings.resolution.height as f32,
    );
    window.mode = if settings.fullscreen {
        bevy::window::WindowMode::BorderlessFullscreen(MonitorSelection::Current)
    } else {
        bevy::window::WindowMode::Windowed
    };
    window.present_mode = present_mode;
    *applied = Some(display);
}
