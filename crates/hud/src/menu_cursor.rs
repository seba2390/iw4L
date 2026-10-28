//! The menus' cursor as the window system's own pointer.
//!
//! A cursor drawn into the frame trails the hand by every frame between the
//! input read and the scan-out: the render world runs a frame behind and the
//! swapchain queues more. The compositor moves a hardware cursor at the
//! display's rate whatever the game's frame rate is, so the game's cursor
//! material becomes the pointer's image at the size the menu would draw it,
//! its centre on the hotspot as the drawn quad's is. Without the material's
//! texels the quad is drawn as before.

use crate::images::HudImages;
use asset_core::AssetNamespace;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::{CursorIcon, CustomCursor, CustomCursorImage, PrimaryWindow, SystemCursorIcon};

/// The cursor the menus want this frame, in the pointer image's pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CursorSpec {
    pub namespace: AssetNamespace,
    pub material: String,
    pub size: [u32; 2],
}

#[derive(Resource, Default)]
pub(crate) struct MenuCursorRequest(pub Option<CursorSpec>);

/// The pointer image's size for a cursor the menus draw `w` by `h` window
/// units: macOS sizes a cursor image in points, the others in pixels.
pub(crate) fn cursor_image_size(w: f32, h: f32, scale_factor: f32) -> [u32; 2] {
    let k = if cfg!(target_os = "macos") {
        1.0
    } else {
        scale_factor
    };
    [w, h].map(|n| ((n * k).round() as u32).clamp(4, 256))
}

pub(crate) fn apply_menu_cursor(
    request: Res<MenuCursorRequest>,
    hud_images: Res<HudImages>,
    mut images: ResMut<Assets<Image>>,
    windows: Query<Entity, With<PrimaryWindow>>,
    mut commands: Commands,
    mut applied: Local<Option<(CursorSpec, Handle<Image>)>>,
    mut custom: Local<bool>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    match &request.0 {
        Some(spec) => {
            if applied.as_ref().is_none_or(|(old, _)| old != spec) {
                let Some((sw, sh, rgba)) = hud_images.rgba(spec.namespace, &spec.material) else {
                    return;
                };
                let [w, h] = spec.size;
                let pixels = resample(*sw, *sh, rgba, w, h);
                let image = Image::new(
                    Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                    TextureDimension::D2,
                    pixels,
                    TextureFormat::Rgba8UnormSrgb,
                    RenderAssetUsages::MAIN_WORLD,
                );
                diag::info!(
                    Ui,
                    "menu cursor: {} {w}x{h} as the window's pointer",
                    spec.material
                );
                if let Some((_, old)) = applied.take() {
                    images.remove(&old);
                }
                *applied = Some((spec.clone(), images.add(image)));
                *custom = false;
            }
            if !*custom {
                let (_, handle) = applied.as_ref().expect("set above");
                let [w, h] = spec.size;
                commands
                    .entity(window)
                    .insert(CursorIcon::Custom(CustomCursor::Image(CustomCursorImage {
                        handle: handle.clone(),
                        hotspot: ((w / 2).min(w - 1) as u16, (h / 2).min(h - 1) as u16),
                        ..default()
                    })));
                *custom = true;
            }
        }
        None if *custom => {
            commands
                .entity(window)
                .insert(CursorIcon::System(SystemCursorIcon::Default));
            *custom = false;
        }
        None => {}
    }
}

/// Bilinear resample of straight-alpha RGBA, filtered premultiplied so a
/// transparent texel's colour does not bleed into the edge.
fn resample(sw: u32, sh: u32, src: &[u8], dw: u32, dh: u32) -> Vec<u8> {
    let (sw, sh) = (sw.max(1) as usize, sh.max(1) as usize);
    let texel = |x: usize, y: usize| -> [f32; 4] {
        let i = (y.min(sh - 1) * sw + x.min(sw - 1)) * 4;
        let p = src.get(i..i + 4).unwrap_or(&[0; 4]);
        let a = f32::from(p[3]) / 255.0;
        [
            f32::from(p[0]) * a,
            f32::from(p[1]) * a,
            f32::from(p[2]) * a,
            a,
        ]
    };
    let mut out = Vec::with_capacity(dw as usize * dh as usize * 4);
    for y in 0..dh {
        let fy = ((y as f32 + 0.5) * sh as f32 / dh as f32 - 0.5).max(0.0);
        let (y0, ty) = (fy as usize, fy.fract());
        for x in 0..dw {
            let fx = ((x as f32 + 0.5) * sw as f32 / dw as f32 - 0.5).max(0.0);
            let (x0, tx) = (fx as usize, fx.fract());
            let [a, b, c, d] = [
                texel(x0, y0),
                texel(x0 + 1, y0),
                texel(x0, y0 + 1),
                texel(x0 + 1, y0 + 1),
            ];
            let mut p = [0.0f32; 4];
            for k in 0..4 {
                let top = a[k] + (b[k] - a[k]) * tx;
                let bottom = c[k] + (d[k] - c[k]) * tx;
                p[k] = top + (bottom - top) * ty;
            }
            let alpha = p[3];
            let un = |v: f32| {
                if alpha > 0.0 {
                    (v / alpha).round().clamp(0.0, 255.0) as u8
                } else {
                    0
                }
            };
            out.extend_from_slice(&[un(p[0]), un(p[1]), un(p[2]), (alpha * 255.0).round() as u8]);
        }
    }
    out
}
