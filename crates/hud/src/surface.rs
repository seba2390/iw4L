use bevy::prelude::*;
use hud_iw4::{AppliedRect, SAFE_AREA_ADJUSTED_DEFAULT, SAFE_AREA_DEFAULT, ScreenPlacement};

const SQUARE_PIXELS: f32 = 1.0;

#[derive(Resource, Clone, Copy, Debug)]
pub struct Hud2dSurface {
    width: f32,
    height: f32,
    place: ScreenPlacement,
}

impl Default for Hud2dSurface {
    fn default() -> Self {
        Self::new(0.0, 0.0)
    }
}

impl Hud2dSurface {
    pub fn new(width: f32, height: f32) -> Self {
        Self::with_safe_area(width, height, SAFE_AREA_DEFAULT)
    }

    pub fn with_safe_area(width: f32, height: f32, safe_area: f32) -> Self {
        let w = width.max(1.0);
        let h = height.max(1.0);
        let place = ScreenPlacement::setup(
            0.0,
            0.0,
            w,
            h,
            w,
            h,
            safe_area,
            safe_area,
            SAFE_AREA_ADJUSTED_DEFAULT,
            SAFE_AREA_ADJUSTED_DEFAULT,
            SQUARE_PIXELS,
        );
        Self {
            width,
            height,
            place,
        }
    }

    /// A 3D UI window `width` by `height` pixels: the virtual 640 by 480
    /// screen stretched over the whole window, with no safe area, as Black
    /// Ops places a window's menus.
    pub fn ui3d_window(width: f32, height: f32) -> Self {
        let size = [width.max(1.0), height.max(1.0)];
        let scale = [size[0] / 640.0, size[1] / 480.0];
        let place = ScreenPlacement {
            scale_virtual_to_real: scale,
            scale_virtual_to_full: scale,
            scale_real_to_virtual: [640.0 / size[0], 480.0 / size[1]],
            real_viewport_position: [0.0, 0.0],
            real_viewport_size: size,
            virtual_viewable_min: [0.0, 0.0],
            virtual_viewable_max: [640.0, 480.0],
            real_viewable_min: [0.0, 0.0],
            real_viewable_max: size,
            virtual_adjustable_min: [0.0, 0.0],
            virtual_adjustable_max: [640.0, 480.0],
            real_adjustable_min: [0.0, 0.0],
            real_adjustable_max: size,
            sub_screen_left: 0.0,
        };
        Self {
            width,
            height,
            place,
        }
    }

    pub fn is_ready(&self) -> bool {
        self.width > 1.0 && self.height > 1.0
    }

    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn height(&self) -> f32 {
        self.height
    }

    pub fn apply_rect(
        &self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        horz_align: i32,
        vert_align: i32,
    ) -> AppliedRect {
        self.place.apply_rect(x, y, w, h, horz_align, vert_align)
    }

    pub fn scale_virtual_to_real(&self) -> [f32; 2] {
        self.place.scale_virtual_to_real
    }

    pub fn display_pixel_aspect(&self) -> f32 {
        SQUARE_PIXELS
    }

    pub fn placement(&self) -> &ScreenPlacement {
        &self.place
    }
}

pub(crate) fn update_hud_surface(
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    hud_menus: Option<Res<asset_game::SessionHudMenus>>,
    mut surface: ResMut<Hud2dSurface>,
) {
    let safe_area = crate::chrome::ChromeAssets {
        catalog: hud_menus.as_ref().and_then(|m| m.0.as_deref()),
        localize: None,
    }
    .layout()
    .map_or(SAFE_AREA_DEFAULT, |layout| layout.safe_area());
    let next = match windows.single() {
        Ok(window) => Hud2dSurface::with_safe_area(window.width(), window.height(), safe_area),
        Err(_) => Hud2dSurface::with_safe_area(0.0, 0.0, safe_area),
    };
    *surface = next;
}
