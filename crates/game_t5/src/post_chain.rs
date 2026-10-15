//! Black Ops' post chain as its renderer draws it after every 3D frame: the
//! bloom mips, remap, blur and streaks, the smoothed bloom applied through
//! the S-curve into the frame, then the film pass. Each pass sets its post
//! controls from the vision the frame draws with before it draws, in the
//! order and float operations of Black Ops' own routines.
//!
//! The renderer copies the vision's bloom block into its view (`r_bloomTweaks`
//! off), with the blur radius moved in front of the expansion source; the
//! offsets below are the vision's own.

use crate::vision::VisionWords;
use game_api::post::{
    PostChainRules, PostConstant, PostFormat, PostFrame, PostImage, PostOutput, PostPass,
    PostSampler, PostTarget,
};
use game_api::unknown;

/// `$bloom` mip 1, mip 2, mip 3, the blur ping and pong, the streak target.
const MIP1: u8 = 0;
const MIP2: u8 = 1;
const MIP3: u8 = 2;
const PING: u8 = 3;
const PONG: u8 = 4;
const STREAK: u8 = 5;

const TARGETS: [PostTarget; 6] = [
    PostTarget {
        shift: 1,
        format: PostFormat::Rgba16Unorm,
    },
    PostTarget {
        shift: 2,
        format: PostFormat::Rgba16Unorm,
    },
    PostTarget {
        shift: 3,
        format: PostFormat::Rgba16Unorm,
    },
    PostTarget {
        shift: 3,
        format: PostFormat::Rgba16Unorm,
    },
    PostTarget {
        shift: 3,
        format: PostFormat::Rgba16Unorm,
    },
    PostTarget {
        shift: 3,
        format: PostFormat::Rgba8Unorm,
    },
];

const DOWNSAMPLE: &str = "bloom_downsample_ldr";
const RESAMPLE: &str = "resample_final";
const CONVOLUTION: &str = "bloom_downsample_convolution";
const REMAP: &str = "bloom_remap";
const BLUR_X: &str = "bloom_blur_x";
const BLUR_Y: &str = "bloom_blur_y";
const REMAP_STREAK: &str = "bloom_remap_streak";
const STREAK_X: &str = "bloom_streak_x";
const STREAK_Y: &str = "bloom_streak_y";
const SMOOTH: &str = "bloom_smooth_a";
const APPLY: &str = "bloom_apply_ldr";
const FILM: &str = "postfx_color";

pub(crate) const POST_CHAIN: PostChainRules = PostChainRules {
    targets: &TARGETS,
    materials: &[
        DOWNSAMPLE,
        RESAMPLE,
        CONVOLUTION,
        REMAP,
        BLUR_X,
        BLUR_Y,
        REMAP_STREAK,
        STREAK_X,
        STREAK_Y,
        SMOOTH,
        APPLY,
        FILM,
    ],
    plan,
    sampler_state: unknown!(
        "t5.render.post_sampler_state",
        "the sampler states Black Ops' post chain reads its code textures with: the chain sets none, and the default its command buffer starts from was not found; the passes read linear, clamped (the one state Black Ops itself sets on the first post texture)",
        "the code-image sampler-state defaults in Black Ops' command buffer setup"
    ),
    vision_channels: crate::vision_channels::new_channels,
};

const R_BLOOM_BLUR_RADIUS: usize = 0xec;
const R_BLOOM_TINT_WEIGHTS: usize = 0xf0;
const R_BLOOM_COLOR_SCALE: usize = 0x100;
const R_BLOOM_TINT_SCALE: usize = 0x110;
const R_BLOOM_CURVE_BREAKPOINT: usize = 0x120;
const R_BLOOM_CURVE_LO_GAMMA: usize = 0x140;
const R_BLOOM_CURVE_HI_GAMMA: usize = 0x190;

/// A streak's fields: levels 0 and 1, inner and outer tint, tint control,
/// tint.
struct Streak {
    levels0: usize,
    levels1: usize,
    inner_tint: usize,
    outer_tint: usize,
    tint_control: usize,
    tint: usize,
}

const STREAK_FIELDS_X: Streak = Streak {
    levels0: 0x1f8,
    levels1: 0x208,
    inner_tint: 0x218,
    outer_tint: 0x224,
    tint_control: 0x230,
    tint: 0x240,
};
const STREAK_FIELDS_Y: Streak = Streak {
    levels0: 0x24c,
    levels1: 0x25c,
    inner_tint: 0x26c,
    outer_tint: 0x278,
    tint_control: 0x284,
    tint: 0x294,
};

fn controls(first: u8, rows: &[[f32; 4]]) -> Vec<(PostConstant, [f32; 4])> {
    rows.iter()
        .enumerate()
        .map(|(i, row)| (PostConstant::Control(first + i as u8), *row))
        .collect()
}

fn pass(
    material: &'static str,
    output: PostOutput,
    samplers: Vec<(PostSampler, PostImage)>,
    constants: Vec<(PostConstant, [f32; 4])>,
) -> PostPass {
    PostPass {
        material,
        output,
        resolve_frame: false,
        samplers,
        constants,
    }
}

fn texture(n: u8, image: PostImage) -> (PostSampler, PostImage) {
    (PostSampler::Texture(n), image)
}

/// The downsample offsets: two texels of a `size` image either way.
fn texel_offsets(size: [u32; 2]) -> [f32; 4] {
    let x = 1.0 / size[0] as f32;
    let y = 1.0 / size[1] as f32;
    [x * -2.0, y * -2.0, x * 2.0, y * 2.0]
}

fn plan(frame: &PostFrame<'_>) -> Vec<PostPass> {
    let vision = VisionWords(frame.vision);
    let size = |target: u8| TARGETS[usize::from(target)].size(frame.display);
    let mut passes = vec![
        pass(
            DOWNSAMPLE,
            PostOutput::Target(MIP1),
            vec![texture(0, PostImage::Scene)],
            controls(0, &[texel_offsets(frame.display)]),
        ),
        pass(
            RESAMPLE,
            PostOutput::Target(MIP2),
            vec![texture(0, PostImage::Target(MIP1))],
            Vec::new(),
        ),
        pass(
            CONVOLUTION,
            PostOutput::Target(MIP3),
            vec![texture(0, PostImage::Target(MIP2))],
            controls(0, &[texel_offsets(size(MIP2))]),
        ),
        pass(
            REMAP,
            PostOutput::Target(PING),
            vec![
                texture(0, PostImage::Target(MIP3)),
                texture(1, PostImage::Target(MIP3)),
            ],
            remap_controls(&vision),
        ),
    ];
    let radius = vision.float(R_BLOOM_BLUR_RADIUS);
    let mip3 = size(MIP3);
    passes.push(pass(
        BLUR_X,
        PostOutput::Target(PONG),
        vec![texture(0, PostImage::Target(PING))],
        controls(0, &blur_controls(1.0 / mip3[0] as f32, radius)),
    ));
    passes.push(pass(
        BLUR_Y,
        PostOutput::Target(PING),
        vec![texture(0, PostImage::Target(PONG))],
        controls(0, &blur_controls(1.0 / mip3[1] as f32, radius)),
    ));
    let (mut src, mut dst) = (PING, PONG);
    let kernel = streak_kernel();
    let streak = size(STREAK);
    for (fields, material, texel) in [
        (&STREAK_FIELDS_X, STREAK_X, 1.0 / streak[0] as f32),
        (&STREAK_FIELDS_Y, STREAK_Y, 1.0 / streak[1] as f32),
    ] {
        let tint = vision.vec3(fields.tint);
        if (tint[2] + tint[1]) + tint[0] == 0.0 {
            continue;
        }
        passes.push(pass(
            REMAP_STREAK,
            PostOutput::Target(STREAK),
            vec![texture(0, PostImage::Target(MIP3))],
            controls(0, &remap_streak_controls(&vision, fields)),
        ));
        let mut constants = controls(0, &streak_controls(&vision, fields, &kernel, texel));
        constants.push((PostConstant::Hdr(0), vision.vec4(fields.tint_control)));
        constants.push((
            PostConstant::Hdr(1),
            [tint[0] * 0.125, tint[1] * 0.125, tint[2] * 0.125, 1.0],
        ));
        passes.push(pass(
            material,
            PostOutput::Target(dst),
            vec![
                texture(0, PostImage::Target(STREAK)),
                texture(1, PostImage::Target(src)),
            ],
            constants,
        ));
        std::mem::swap(&mut src, &mut dst);
    }
    let x = 1.0 / mip3[0] as f32;
    let y = 1.0 / mip3[1] as f32;
    passes.push(pass(
        SMOOTH,
        PostOutput::Target(dst),
        vec![texture(0, PostImage::Target(src))],
        controls(
            0,
            &[
                [x * 1.5, y * 0.5, x * 0.5, y * -1.5],
                [x * -1.5, y * -0.5, x * -0.5, y * 1.5],
            ],
        ),
    ));
    let film = vision.film();
    passes.push(pass(
        APPLY,
        PostOutput::Frame,
        vec![
            texture(1, PostImage::Target(dst)),
            texture(0, PostImage::Scene),
            texture(2, PostImage::Scene),
        ],
        controls(0, &film.s_curve_constants()),
    ));
    passes.push(PostPass {
        material: FILM,
        output: PostOutput::Frame,
        resolve_frame: true,
        samplers: vec![(PostSampler::ResolvedScene, PostImage::Scene)],
        constants: controls(0, &film.constants()),
    });
    passes
}

const BREAKPOINT_SCALE: f32 = 4.0;
const CURVE_MAX: f32 = 32.0;
const CURVE_MIN: f32 = f32::from_bits(0x3800_0000);
const INV_CURVE_MAX: f32 = 0.031_25;
const GAMMA_DEFAULT: f32 = f32::from_bits(0x3dcc_cccd);

/// The remap pass's controls 0..8 and E, F. It also writes control 9 (the
/// expansion weights) and C (the expansion source), which only
/// `bloom_expansion` reads; Black Ops never draws that material.
fn remap_controls(vision: &VisionWords<'_>) -> Vec<(PostConstant, [f32; 4])> {
    let color_scale = vision.vec4(R_BLOOM_COLOR_SCALE);
    let tint_scale = vision.vec4(R_BLOOM_TINT_SCALE);
    let breakpoint = vision.vec4(R_BLOOM_CURVE_BREAKPOINT);
    let mut lo = color_scale[3] * BREAKPOINT_SCALE;
    let mut hi = tint_scale[3] * BREAKPOINT_SCALE;
    if lo > CURVE_MAX {
        lo = CURVE_MAX;
    }
    if hi > CURVE_MAX {
        hi = CURVE_MAX;
    }
    if lo >= hi {
        hi = lo + CURVE_MIN;
    }
    let mut points = breakpoint.map(|b| b * BREAKPOINT_SCALE);
    for p in &mut points {
        if CURVE_MIN > *p {
            *p = CURVE_MIN;
        }
    }
    for p in &mut points {
        if *p > CURVE_MAX {
            *p = CURVE_MAX;
        }
    }
    let range = 1.0 / ((hi - lo) * INV_CURVE_MAX);
    let below = points.map(|p| p * INV_CURVE_MAX);
    let above = points.map(|p| (CURVE_MAX - p) * INV_CURVE_MAX);
    let inv_below = below.map(|b| 1.0 / b);
    let inv_above = above.map(|a| 1.0 / a);
    let gamma = |offset: usize| {
        let gamma = vision.vec4(offset);
        if gamma[0] == 0.0 {
            [GAMMA_DEFAULT; 4]
        } else {
            gamma
        }
    };
    let lo_gamma = gamma(R_BLOOM_CURVE_LO_GAMMA);
    let hi_gamma = gamma(R_BLOOM_CURVE_HI_GAMMA);
    let mut out = controls(
        0,
        &[
            vision.vec4(R_BLOOM_TINT_WEIGHTS),
            [range, -((lo * INV_CURVE_MAX) * range), 0.25, 0.5],
            inv_below,
            inv_above,
            std::array::from_fn(|i| -(inv_above[i] * below[i])),
            lo_gamma.map(|g| 1.0 / g),
            hi_gamma.map(|g| 1.0 / g),
            below,
            above,
        ],
    );
    out.push((
        PostConstant::Control(0xe),
        [color_scale[0], color_scale[1], color_scale[2], 0.0],
    ));
    out.push((
        PostConstant::Control(0xf),
        [tint_scale[0], tint_scale[1], tint_scale[2], INV_CURVE_MAX],
    ));
    out
}

/// The blur pass's controls 0..3: twelve gaussian taps of `radius` (sigma
/// clamped to 1..3) paired into six bilinear taps, `texel` apart.
fn blur_controls(texel: f32, radius: f32) -> [[f32; 4]; 4] {
    let capped = if radius - 3.0 >= 0.0 { 3.0 } else { radius };
    let sigma = if 1.0 - radius >= 0.0 { 1.0 } else { capped };
    let k = -0.5 / (sigma * sigma);
    let mut weights = [0.0f32; 12];
    let mut x = 0.5f32;
    let mut sum = 0.0f32;
    for weight in &mut weights {
        *weight = f64::from(x * x * k).exp() as f32;
        sum += *weight * 2.0;
        x += 1.0;
    }
    let scale = 1.0 / sum;
    for weight in &mut weights {
        *weight *= scale;
    }
    let mut sums = [0.0f32; 6];
    let mut offsets = [0.0f32; 6];
    for pair in 0..6 {
        let (near, far) = (weights[pair * 2], weights[pair * 2 + 1]);
        sums[pair] = far + near;
        offsets[pair] = if sums[pair] == 0.0 {
            0.0
        } else if pair == 0 {
            far / sums[pair]
        } else {
            far / sums[pair] + (pair * 2) as f32
        };
    }
    let offsets = offsets.map(|o| o * texel);
    [
        [offsets[0], offsets[1], offsets[2], 0.0],
        [offsets[3], offsets[4], offsets[5], 0.0],
        [sums[0], sums[1], sums[2], sums[3]],
        [sums[4], sums[5], 0.0, 0.0],
    ]
}

/// The streak taps: 32 weights falling off by 0.9 a texel, paired into 16
/// bilinear taps (their weight and offset).
fn streak_kernel() -> [(f32, f32); 16] {
    let falloff = f64::from(0.9f32);
    let weights: [f32; 32] = std::array::from_fn(|i| falloff.powf(i as f64) as f32);
    std::array::from_fn(|pair| {
        let (near, far) = (weights[pair * 2], weights[pair * 2 + 1]);
        let sum = near + far;
        // x87 at double precision: the quotient and the sum round to double,
        // the offset to float.
        let offset = (f64::from(far) / f64::from(sum) + (pair * 2) as f64) as f32;
        (sum, offset)
    })
}

const STREAK_RANGE_MIN: f32 = f32::from_bits(0x3a00_0000);

/// The streak remap pass's controls 0 and 1.
fn remap_streak_controls(vision: &VisionWords<'_>, fields: &Streak) -> [[f32; 4]; 2] {
    let levels0 = vision.vec4(fields.levels0);
    let levels1 = vision.vec4(fields.levels1);
    let lo = levels1[0];
    let mut hi = levels1[1];
    if lo >= hi {
        hi = lo + STREAK_RANGE_MIN;
    }
    let range = 1.0 / (hi - lo);
    [
        [
            levels0[0] * range,
            levels0[1] * range,
            levels0[2] * range,
            1.0 / levels0[3],
        ],
        [-(range * lo), levels1[3] - levels1[2], levels1[2], 0.0],
    ]
}

/// A streak pass's controls 0..F: each tap's weight times a tint stepping
/// from the inner to the outer tint, and its offset in `texel`s.
fn streak_controls(
    vision: &VisionWords<'_>,
    fields: &Streak,
    kernel: &[(f32, f32); 16],
    texel: f32,
) -> [[f32; 4]; 16] {
    let inner = vision.vec3(fields.inner_tint);
    let outer = vision.vec3(fields.outer_tint);
    let step: [f32; 3] = std::array::from_fn(|c| (outer[c] - inner[c]) * 0.0625);
    let mut tint = inner;
    std::array::from_fn(|tap| {
        if tap > 0 {
            tint = std::array::from_fn(|c| step[c] + tint[c]);
        }
        let (weight, offset) = kernel[tap];
        [
            weight * tint[0],
            weight * tint[1],
            weight * tint[2],
            offset * texel,
        ]
    })
}
