//! Black Ops 2's revive overlay rows: the colour matrix of a vision's edge
//! colour temperature and saturation, each colour's column scaled by its
//! edge scale and contrast, with the contrast's pivot and the edge offset,
//! then the edge mask adjustment and amount.

use game_api::post::ReviveFx;

type Mat4 = [f32; 16];

const fn bits(rows: [[u32; 4]; 4]) -> Mat4 {
    let mut out = [0.0; 16];
    let mut r = 0;
    while r < 4 {
        let mut c = 0;
        while c < 4 {
            out[r * 4 + c] = f32::from_bits(rows[r][c]);
            c += 1;
        }
        r += 1;
    }
    out
}

const ONE: u32 = 0x3f80_0000;
const LINEAR_SRGB_TO_XYZ: Mat4 = bits([
    [0x3ed3_2618, 0x3e59_b3d0, 0x3c9e_1b09, 0],
    [0x3eb7_1759, 0x3f37_1759, 0x3df4_1f21, 0],
    [0x3e38_d4fe, 0x3d93_dd98, 0x3f73_53f8, 0],
    [0, 0, 0, ONE],
]);
const BRADFORD: Mat4 = bits([
    [0x3f65_2546, 0xbf40_0d1b, 0x3d1f_559b, 0],
    [0x3e88_6595, 0x3fdb_53f8, 0xbd8c_49ba, 0],
    [0xbe25_460b, 0x3d16_52bd, 0x3f83_c9ef, 0],
    [0, 0, 0, ONE],
]);
const BRADFORD_INVERSE: Mat4 = bits([
    [0x3f7c_ab93, 0x3edd_5715, 0xbc0b_bd38, 0],
    [0xbe16_9553, 0x3f04_b33e, 0x3d24_0421, 0],
    [0x3e23_cd57, 0x3d49_e55c, 0x3f77_eec4, 0],
    [0, 0, 0, ONE],
]);
const XYZ_TO_LINEAR_SRGB: Mat4 = bits([
    [0x404f_67cb, 0xbf78_214b, 0x3d63_e1bc, 0],
    [0xbfc4_c4f0, 0x3ff0_2071, 0xbe50_e454, 0],
    [0xbeff_44b3, 0x3d2a_364f, 0x3f87_4e12, 0],
    [0, 0, 0, ONE],
]);

/// `a × b` (row vectors), each output row summed as the backend sums it:
/// (w·b3 + y·b1) + (z·b2 + x·b0).
fn mat_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [0.0; 16];
    for r in 0..4 {
        let row = &a[r * 4..r * 4 + 4];
        for c in 0..4 {
            out[r * 4 + c] =
                (row[3] * b[12 + c] + row[1] * b[4 + c]) + (row[2] * b[8 + c] + row[0] * b[c]);
        }
    }
    out
}

/// The CIE XYZ (Y = 1) of a black body at `kelvin`, held to 1667 K to
/// 25000 K, worked in doubles.
fn chromaticity(kelvin: f32) -> [f32; 4] {
    let t = f64::from(kelvin).clamp(1667.0, 25000.0);
    let t2 = t * t;
    let t3 = t2 * t;
    let x = if 4000.0 > t {
        (877.6956 / t - (266_123_900.000_000_03 / t3 + 234_358.0 / t2)) + 0.17991
    } else {
        ((2_107_037.9 / t2 - 3_025_846_900.0 / t3) + 222.634_699_999_999_98 / t) + 0.24039
    };
    let x2 = x * x;
    let x3 = x2 * x;
    let y = if 2222.0 > t {
        (x * 2.185_558_32 - (x3 * 1.106_381_4 + x2 * 1.348_110_2)) - 0.202_196_83
    } else if 4000.0 > t {
        (x * 2.091_370_15 - (x2 * 1.374_185_93 + x3 * 0.954_947_6)) - 0.167_488_67
    } else {
        (x * 3.751_129_97 + (x3 * 3.081_758 - x2 * 5.873_386_7)) - 0.370_014_83
    };
    let inverse = 1.0 / y;
    [
        (inverse * x) as f32,
        1.0,
        (((1.0 - x) - y) * inverse) as f32,
        1.0,
    ]
}

/// One Bradford cone response of a colour: (y·b1 + x·b0) + (w·b3 + z·b2).
fn bradford(xyz: [f32; 4], column: usize) -> f32 {
    (xyz[1] * BRADFORD[4 + column] + xyz[0] * BRADFORD[column])
        + (xyz[3] * BRADFORD[12 + column] + xyz[2] * BRADFORD[8 + column])
}

/// The colour matrix that moves 6500 K white to `kelvin`, snapped to
/// 1/1024 steps.
fn color_temp_matrix(kelvin: f32) -> Mat4 {
    let reference = chromaticity(6500.0);
    let target = chromaticity(kelvin);
    let mut scale = [0.0; 16];
    for i in 0..3 {
        scale[i * 5] = bradford(target, i) / bradford(reference, i);
    }
    scale[15] = 1.0;
    let adapted = mat_mul(&mat_mul(&BRADFORD, &scale), &BRADFORD_INVERSE);
    let mut out = mat_mul(&mat_mul(&LINEAR_SRGB_TO_XYZ, &adapted), &XYZ_TO_LINEAR_SRGB);
    for i in [0, 1, 2, 4, 5, 6, 8, 9, 10] {
        out[i] = ((out[i] + 0.000_488_281_25) * 1024.0).floor() * 0.000_976_562_5;
    }
    out
}

/// The colour matrix that scales saturation by `amount` around a
/// (0.25, 0.5, 0.25) grey.
fn saturation_matrix(amount: f32) -> Mat4 {
    let quarter = (1.0 - amount) * 0.25;
    let half = (1.0 - amount) * 0.5;
    [
        quarter + amount,
        quarter,
        quarter,
        0.0,
        half,
        half + amount,
        half,
        0.0,
        quarter,
        quarter,
        quarter + amount,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ]
}

/// `postFxControl0` to `postFxControl3` for the revive pass.
pub(crate) fn rows(fx: &ReviveFx) -> [[f32; 4]; 4] {
    let m = mat_mul(
        &color_temp_matrix(fx.edge_color_temp),
        &saturation_matrix(fx.edge_saturation),
    );
    let mut out = [[0.0; 4]; 4];
    for (c, row) in out.iter_mut().take(3).enumerate() {
        let gain = fx.edge_contrast[c] * fx.edge_scale[c];
        *row = [
            gain * m[c],
            gain * m[4 + c],
            gain * m[8 + c],
            (1.0 - fx.edge_contrast[c]) * 0.5 + fx.edge_offset[c],
        ];
    }
    out[3] = [fx.edge_mask_adjust, fx.edge_amount, 0.0, 0.0];
    out
}
