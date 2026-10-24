//! A screen blur as the backends that share one plan it: a Gaussian blur of
//! the full frame into a quarter-size image. A blur radius (in 480-line
//! units) becomes a vertical radius in the frame's pixels and a horizontal
//! one scaled by the screen's pixel aspect; a first pass filters the frame
//! down into the quarter-size image in two dimensions, spending the smaller
//! radius (at most the widest a two-tap pass covers) at the frame's size,
//! and the passes that follow ping-pong between two quarter-size images, one
//! dimension at a time, until the radius left is too small to matter; the
//! last pass writes the blurred image.

use crate::post::{BlurImage, BlurPass};

/// The smallest radius a pass is planned for.
const RADIUS_MIN: f32 = 0.329_505_12;
/// The widest radius a two-dimensional (two-tap) pass covers.
const RADIUS_MAX_2D: f32 = 1.389_560_5;
/// The widest radius an eight-tap pass covers, and its square's share.
const RADIUS_MAX_1D: f32 = 6.497_750_3;
const RADIUS_MAX_1D_SQUARED: f32 = 42.220_76;

/// The filter material for a pass with `taps` taps (1 to 8).
const FILTERS: [&str; 8] = [
    "filter_symmetric_1",
    "filter_symmetric_2",
    "filter_symmetric_3",
    "filter_symmetric_4",
    "filter_symmetric_5",
    "filter_symmetric_6",
    "filter_symmetric_7",
    "filter_symmetric_8",
];

/// What the backend adds before rounding a value to the nearest integer
/// (ties to even): 2^-30.
pub const NUDGE: f64 = 9.313_225_746_154_785e-10;

/// Every material a blur draws with.
pub const MATERIALS: [&str; 8] = FILTERS;

/// The display aspect the auto aspect ratio picks for a screen: 16:10 when
/// its height is ten sixteenths of its width (rounded), 16:9 below, 4:3
/// above.
fn display_aspect([width, height]: [u32; 2]) -> f32 {
    let sixteenths = height as f32 * 16.0 / width as f32;
    let rounded = (f64::from(sixteenths) + NUDGE).round_ties_even() as i32;
    if rounded == 10 {
        1.6
    } else if rounded < 10 {
        1.777_777_8
    } else {
        1.333_333_4
    }
}

/// The blur passes for `radius` over a frame of `full` size, into images of
/// `quarter` size.
pub fn plan(radius: f32, full: [u32; 2], quarter: [u32; 2]) -> Vec<BlurPass> {
    // Both in the x87 unit's precision, as the backend works them out.
    let pixel_aspect =
        (f64::from(full[1]) * f64::from(display_aspect(full)) / f64::from(full[0])) as f32;
    let vertical = (f64::from(full[1]) * f64::from(radius) * f64::from(0.002_083_333_4f32)) as f32;
    let horizontal = pixel_aspect * vertical;
    let mut passes = Vec::new();
    let (mut x, mut y) = (horizontal, vertical);
    if full != quarter {
        let mut down = if 0.0 <= y - x { x } else { y };
        if RADIUS_MAX_2D < down {
            down = RADIUS_MAX_2D;
        }
        x = ((x * x - down * down).abs().sqrt() * quarter[0] as f32) / full[0] as f32;
        y = ((y * y - down * down).abs().sqrt() * quarter[1] as f32) / full[1] as f32;
        passes.push(two_dimensional(down, full, quarter));
    }
    while x >= RADIUS_MIN || y >= RADIUS_MIN {
        if RADIUS_MIN > (x - y).abs() {
            let both = (x + y) * 0.5;
            if RADIUS_MAX_2D >= both {
                passes.push(two_dimensional(both, quarter, quarter));
                break;
            }
        }
        let (axis, pass_radius) = if x > y {
            if RADIUS_MAX_1D > x {
                let r = x;
                x = 0.0;
                (0, r)
            } else {
                x = (x * x - RADIUS_MAX_1D_SQUARED).sqrt();
                (0, RADIUS_MAX_1D)
            }
        } else if RADIUS_MAX_1D >= y {
            let r = y;
            y = 0.0;
            (1, r)
        } else {
            y = (y * y - RADIUS_MAX_1D_SQUARED).sqrt();
            (1, RADIUS_MAX_1D)
        };
        passes.push(one_dimensional(pass_radius, quarter, axis));
        if passes.len() >= 32 {
            break;
        }
    }
    let last = passes.len().saturating_sub(1);
    for (at, pass) in passes.iter_mut().enumerate() {
        pass.source = if at == 0 {
            BlurImage::Frame
        } else {
            BlurImage::Ping(((at - 1) & 1) as u8)
        };
        pass.target = if at == last {
            BlurImage::Blurred
        } else {
            BlurImage::Ping((at & 1) as u8)
        };
    }
    passes
}

/// A pass filtering one dimension of an image into another of the same size.
fn one_dimensional(radius: f32, size: [u32; 2], axis: usize) -> BlurPass {
    let (taps, offsets, weights) = points(radius, size[axis], size[axis], 8);
    let mut rows = [[0.0; 4]; 8];
    for (row, (offset, weight)) in rows.iter_mut().zip(offsets.iter().zip(weights)) {
        row[axis] = *offset;
        row[3] = weight;
    }
    BlurPass {
        material: FILTERS[taps.clamp(1, 8) - 1],
        source: BlurImage::Frame,
        target: BlurImage::Blurred,
        size,
        taps: rows,
    }
}

/// A pass filtering both dimensions of an image (two taps each way) into one
/// of `to` size.
fn two_dimensional(radius: f32, from: [u32; 2], to: [u32; 2]) -> BlurPass {
    let (_, x, wx) = points(radius, from[0], to[0], 2);
    let (_, y, wy) = points(radius, from[1], to[1], 2);
    let mut rows = [[0.0; 4]; 8];
    for j in 0..2 {
        for i in 0..2 {
            let weight = wy[j] * wx[i];
            rows[j * 4 + i * 2] = [-x[i], y[j], 0.0, weight];
            rows[j * 4 + i * 2 + 1] = [x[i], y[j], 0.0, weight];
        }
    }
    BlurPass {
        material: FILTERS[7],
        source: BlurImage::Frame,
        target: BlurImage::Blurred,
        size: to,
        taps: rows,
    }
}

/// A Gaussian's tap points for `radius` over an image `from` pixels wide
/// written `to` pixels wide: each tap merges two neighbouring texels (half a
/// texel off when the size ratio rounds even, as a 4:1 one does), at most
/// `limit` taps, the
/// offsets in the source's texture coordinates and the weights halved
/// around the centre; taps past the first whose weight is under 0.01 are
/// dropped.
fn points(radius: f32, from: u32, to: u32, limit: usize) -> (usize, [f32; 8], [f32; 8]) {
    let from = from as f32;
    let ratio = from / to as f32;
    let rounded = (f64::from(ratio) + NUDGE).round_ties_even() as i32;
    let shift = if rounded & 1 != 0 { 0.0 } else { 0.5 };
    let exponent = -0.5 / (radius * radius);
    let mut offsets = [0.0; 8];
    let mut weights = [0.0; 8];
    let mut total = 0.0;
    for i in 0..limit {
        let a = (2 * i) as f32 + shift;
        let b = (2 * i + 1) as f32 + shift;
        let mut wa = (a * exponent * a).exp();
        let wb = (b * exponent * b).exp();
        if i == 0 && shift == 0.0 {
            wa *= 0.5;
        }
        let weight = wb + wa;
        weights[i] = weight;
        offsets[i] = if weight == 0.0 {
            ((b + a) * 0.5) / from
        } else {
            (wa * a + wb * b) / (weight * from)
        };
        total += weight;
    }
    if 0.001 >= total {
        weights[0] = 0.5;
        return (1, offsets, weights);
    }
    let scale = 0.5 / total;
    let mut taps = limit;
    for i in (0..limit).rev() {
        weights[i] *= scale;
        if 0.01 > weights[i] {
            taps = i + 1;
        }
    }
    (taps, offsets, weights)
}
