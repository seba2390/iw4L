//! Black Ops 2's screen placement: how its 640x480 menu coordinates land on
//! the screen (`ScrPlace_SetupViewport`, `ScrPlace_ApplyRect`), and which of
//! its UI fonts a menu item's font and scale draw with (`UI_GetFontHandle`).
//!
//! The 640x480 space is a 4:3 box centred in the viewport; an alignment
//! picks what an x or y counts from: the box (0), the safe area's edges or
//! centre (1, 3, 7), the viewport (2, 4, 5, 6) or the adjustable area (8, 9,
//! 10), which outside splitscreen is the safe area.

/// A local client's placement, as the engine keeps it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// 640x480 units to real pixels, in the 4:3 box.
    pub scale_virtual_to_real: [f32; 2],
    /// 640x480 units to real pixels, across the whole viewport.
    pub scale_virtual_to_full: [f32; 2],
    /// Real pixels to 640x480 units.
    pub scale_real_to_virtual: [f32; 2],
    pub virtual_viewable_min: [f32; 2],
    pub virtual_viewable_max: [f32; 2],
    pub real_viewport_base: [f32; 2],
    pub real_viewport_size: [f32; 2],
    pub real_viewable_min: [f32; 2],
    pub real_viewable_max: [f32; 2],
    pub real_adjustable_min: [f32; 2],
    pub real_adjustable_max: [f32; 2],
    /// Where the 4:3 box starts in the viewport.
    pub sub_screen_left: f32,
    /// The UI scale every rect is first multiplied by (1 outside
    /// splitscreen; the engine's global UI scale is 1 on PC).
    pub ui_scale: f32,
}

/// C's float-to-int round of `x + 0.5` toward negative infinity.
fn round_half_down(x: f32) -> f32 {
    (x + 0.5).floor()
}

impl Placement {
    /// The placement of a `width` x `height` viewport at the origin with
    /// square pixels, for safe areas `safe_horizontal` and `safe_vertical`
    /// (`safeArea_horizontal`, `safeArea_vertical`), outside splitscreen.
    pub fn setup(width: f32, height: f32, safe_horizontal: f32, safe_vertical: f32) -> Self {
        let boxed = (height * (4.0 / 3.0)).min(width);
        let scale_x = (1.0 / boxed) * width;
        // The safe area's real edges, rounded to whole pixels.
        let left = round_half_down((1.0 - safe_horizontal) * 0.5 * width);
        let top = round_half_down((1.0 - safe_vertical) * 0.5 * height);
        let min = [left.max(0.0), top.max(0.0)];
        let max = [(width - left).min(width), (height - top).min(height)];
        let virtual_min = [
            min[0] * scale_x * (640.0 / width),
            min[1] * (480.0 / height),
        ];
        let virtual_max = [
            max[0] * scale_x * (640.0 / width),
            max[1] * (480.0 / height),
        ];
        Self {
            scale_virtual_to_real: [boxed * 0.001_562_5, height * 0.002_083_333_4],
            scale_virtual_to_full: [width * 0.001_562_5, height * 0.002_083_333_4],
            scale_real_to_virtual: [(1.0 / boxed) * 640.0, (1.0 / height) * 480.0],
            virtual_viewable_min: virtual_min,
            virtual_viewable_max: virtual_max,
            real_viewport_base: [0.0, 0.0],
            real_viewport_size: [width, height],
            real_viewable_min: min,
            real_viewable_max: max,
            real_adjustable_min: min,
            real_adjustable_max: max,
            sub_screen_left: (width - boxed) * 0.5,
            ui_scale: 1.0,
        }
    }

    /// A 640x480 rect's real pixels, by its horizontal and vertical
    /// alignments.
    pub fn apply_rect(&self, rect: [f32; 4], horz_align: i32, vert_align: i32) -> [f32; 4] {
        let [mut x, mut y, mut w, mut h] = rect.map(|v| v * self.ui_scale);
        let [sx, sy] = self.scale_virtual_to_real;
        match horz_align {
            1 => x = sx * x + self.real_viewable_min[0],
            2 => x = sx * x + self.real_viewport_size[0] * 0.5 + self.real_viewport_base[0],
            3 => x = sx * x + self.real_viewable_max[0],
            // As the engine writes it: the base and the scale summed, times x.
            4 => {
                x *= self.real_viewport_base[0] + self.scale_virtual_to_full[0];
                w *= self.scale_virtual_to_full[0];
            }
            5 => {}
            6 => {
                x *= self.scale_real_to_virtual[0];
                w *= self.scale_real_to_virtual[0];
            }
            7 => {
                x = (self.real_viewable_max[0] + self.real_viewable_min[0]) * 0.5 + sx * x;
            }
            8 => x = sx * x + self.real_adjustable_min[0],
            9 => {
                x = (self.real_adjustable_max[0] + self.real_adjustable_min[0]) * 0.5 + sx * x;
            }
            10 => x = sx * x + self.real_adjustable_max[0],
            _ => x = sx * x + self.sub_screen_left,
        }
        if !matches!(horz_align, 4..=6) {
            w *= sx;
        }
        match vert_align {
            1 => y = sy * y + self.real_viewable_min[1],
            2 => y = self.real_viewport_size[1] * 0.5 + sy * y + self.real_viewport_base[1],
            3 => y = sy * y + self.real_viewable_max[1],
            4 => {
                y *= self.real_viewport_base[1] + self.scale_virtual_to_full[1];
                h *= self.scale_virtual_to_full[1];
            }
            5 => {}
            6 => {
                y *= self.scale_real_to_virtual[1];
                h *= self.scale_real_to_virtual[1];
            }
            7 => {
                y = (self.real_viewable_max[1] + self.real_viewable_min[1]) * 0.5 + sy * y;
            }
            8 => y = sy * y + self.real_adjustable_min[1],
            9 => {
                y = (self.real_adjustable_max[1] + self.real_adjustable_min[1]) * 0.5 + sy * y;
            }
            10 => y = sy * y + self.real_adjustable_max[1],
            _ => y *= sy,
        }
        if !matches!(vert_align, 4..=6) {
            h *= sy;
        }
        [x, y, w, h]
    }
}

/// The UI font a menu item's font enum draws with at `scale`
/// (`UI_GetFontHandle`): 1 normal, 2 big, 3 small, 5 console, 6 extra big,
/// 7 extra small, 8 the distance font; 4 is bold when the real scale reaches
/// `ui_bigFont`, else as 0, which picks small at or below `ui_smallFont`, big
/// at or above `ui_bigFont`, else normal, by the scale in real pixels.
pub fn menu_font(
    font: i32,
    scale: f32,
    placement: &Placement,
    ui_small_font: f32,
    ui_big_font: f32,
) -> &'static str {
    match font {
        1 => return NORMAL,
        2 => return BIG,
        3 => return SMALL,
        5 => return CONSOLE,
        6 => return EXTRA_BIG,
        7 => return EXTRA_SMALL,
        8 => return DIST,
        _ => {}
    }
    let real = placement.ui_scale * scale * placement.scale_virtual_to_real[1];
    if font == 4 {
        if ui_small_font >= real {
            return SMALL;
        }
        return if real >= ui_big_font { BOLD } else { NORMAL };
    }
    if ui_small_font >= real {
        SMALL
    } else if real >= ui_big_font {
        BIG
    } else {
        NORMAL
    }
}

const NORMAL: &str = "fonts/720/normalFont";
const BIG: &str = "fonts/720/bigFont";
const SMALL: &str = "fonts/720/smallFont";
const BOLD: &str = "fonts/720/boldFont";
const CONSOLE: &str = "fonts/720/consoleFont";
const EXTRA_BIG: &str = "fonts/720/extraBigFont";
const EXTRA_SMALL: &str = "fonts/720/extraSmallFont";
const DIST: &str = "fonts/distfont";
