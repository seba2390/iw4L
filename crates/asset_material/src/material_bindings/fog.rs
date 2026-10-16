use super::{BindingWriter, FrameStep, MaterialFogInputs};
use crate::t5_code_remap::{
    LEFTOVER_T5_CODE_BASE, T5_CODE_FOG, T5_CODE_FOG_COLOR, T5_CODE_FOG2, T5_CODE_SUN_FOG,
    T5_CODE_SUN_FOG_COLOR, T5_CODE_SUN_FOG_DIR,
};
use crate::t6_techset::CODE_T6_FOG;
#[derive(Clone, Copy, Debug)]
pub(super) enum FogPacking {
    T5,
    T6,
}
pub(super) fn prepare(steps: &mut Vec<FrameStep>, requested: &[u16]) {
    let t5 = [
        T5_CODE_FOG_COLOR,
        T5_CODE_SUN_FOG_COLOR,
        T5_CODE_SUN_FOG_DIR,
        T5_CODE_SUN_FOG,
        T5_CODE_FOG,
        T5_CODE_FOG2,
    ]
    .map(|i| LEFTOVER_T5_CODE_BASE + i);
    if super::demands_any(requested, &t5) {
        steps.push(FrameStep::Fog(FogPacking::T5));
    }
    if super::demands_any(requested, &CODE_T6_FOG) {
        steps.push(FrameStep::Fog(FogPacking::T6));
    }
}
const DEG_TO_RAD: f32 = f32::from_bits(0x3c8e_fa35);
const FLAT_SLOPE: f32 = f32::from_bits(0x4b18_9680);

pub(super) fn produce(
    sources: &mut BindingWriter<'_>,
    fog: &MaterialFogInputs,
    eye_z: f32,
    enabled: bool,
    packing: FogPacking,
) {
    let put = |sources: &mut BindingWriter<'_>, index, row: [f32; 4]| {
        if matches!(packing, FogPacking::T5) {
            sources.set_constant_rows(LEFTOVER_T5_CODE_BASE + index, &[row.map(f32::to_bits)]);
        }
    };
    let (height_density, base_height, color) = match fog.volumetric {
        Some(volume) => (
            if volume.halfway_height < 1.0 {
                0.0
            } else {
                1.0 / volume.halfway_height
            },
            volume.base_height,
            [
                fog.color_rgb[0],
                fog.color_rgb[1],
                fog.color_rgb[2],
                volume.color_scale,
            ],
        ),

        None => (
            0.0,
            0.0,
            super::fog_color_linear_and_gamma(fog.color_rgb, 1.0).0,
        ),
    };
    put(sources, T5_CODE_FOG_COLOR, color);
    let (sun_color, sun_dir, angles) = match fog.sun {
        Some(sun) => {
            let rgb = if fog.volumetric.is_some() {
                sun.color_rgb
            } else {
                let linear = super::fog_color_linear_and_gamma(sun.color_rgb, 1.0).0;
                [linear[0], linear[1], linear[2]]
            };
            (
                [rgb[0], rgb[1], rgb[2], fog.max_opacity],
                [
                    sun.sun_dir[0],
                    sun.sun_dir[1],
                    sun.sun_dir[2],
                    sun.begin_angle_deg,
                ],
                [sun.begin_angle_deg, sun.end_angle_deg],
            )
        }
        None => ([0.0, 0.0, 0.0, fog.max_opacity], [0.0; 4], [0.0; 2]),
    };
    let start = (f64::from(angles[0] * DEG_TO_RAD)).cos() as f32;
    let end = (f64::from(angles[1] * DEG_TO_RAD)).cos() as f32;
    let diff = start - end;
    let slope = if diff != 0.0 { 1.0 / diff } else { FLAT_SLOPE };
    put(sources, T5_CODE_SUN_FOG_COLOR, sun_color);
    put(sources, T5_CODE_SUN_FOG_DIR, sun_dir);
    put(sources, T5_CODE_SUN_FOG, [-end * slope, slope, 0.0, 0.0]);
    let density = if fog.halfway_dist > 0.0 {
        1.0 / fog.halfway_dist
    } else {
        0.0
    };
    let max_density = fog
        .volumetric
        .map_or(100.0 * density, |volume| volume.max_density);
    let (fog_row, fog2) = density_rows(
        density,
        max_density,
        height_density,
        base_height,
        fog.start_dist,
        eye_z,
        enabled,
    );
    put(sources, T5_CODE_FOG, fog_row);
    put(sources, T5_CODE_FOG2, fog2);
    if matches!(packing, FogPacking::T5) {
        return;
    }
    let scale = fog.volumetric.map_or(1.0, |volume| volume.color_scale);
    let hdr = |rgb: [f32; 4]| {
        [
            rgb[0] * scale,
            rgb[1] * scale,
            rgb[2] * scale,
            fog.max_opacity,
        ]
    };
    for (code, row) in CODE_T6_FOG.into_iter().zip([
        hdr(color),
        fog_row,
        fog2,
        sun_dir,
        hdr(sun_color),
        [-end * slope, slope, 0.0, 0.0],
    ]) {
        sources.set_constant_rows(code, &[row.map(f32::to_bits)]);
    }
}

fn density_rows(
    density: f32,
    max_density: f32,
    height: f32,
    base: f32,
    start: f32,
    eye_z: f32,
    enabled: bool,
) -> ([f32; 4], [f32; 4]) {
    if !enabled || density == 0.0 {
        return ([0.0; 4], [0.0; 4]);
    }
    let max_density = max_density.min(100.0 * density).max(density);
    let ln2 = (2.0_f64).ln() as f32;
    let log_ratio = (f64::from(density / max_density)).ln() as f32;
    let log_density = log_ratio - (height * ln2) * (eye_z - base);
    let integral = if log_density < 0.0 {
        (f64::from(log_density)).exp() as f32
    } else {
        log_density + 1.0
    };
    let scale = if height > 0.0 {
        (-max_density) * ln2
    } else {
        -max_density
    };
    (
        [log_density, scale, start * density, -(height * ln2)],
        [integral, 0.0, 0.0, 0.0],
    )
}
