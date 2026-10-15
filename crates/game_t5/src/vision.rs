//! Black Ops' vision constants: the film block its renderer copies out of a
//! vision and the shader constants it derives from it.

/// The film block Black Ops' renderer reads: three of each zone value are
/// (dark, mid, light).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Film {
    pub(crate) mid_start: f32,
    pub(crate) mid_end: f32,
    pub(crate) dark_feather: f32,
    pub(crate) light_feather: f32,
    pub(crate) bleach: [f32; 3],
    pub(crate) color_temp: [f32; 3],
    pub(crate) hue: [f32; 3],
    pub(crate) saturation: [f32; 3],
    pub(crate) dark_tint: [f32; 3],
    pub(crate) mid_tint: [f32; 3],
    pub(crate) light_tint: [f32; 3],
    pub(crate) contrast: [f32; 3],
    pub(crate) enable: bool,
    pub(crate) lut: f32,
    pub(crate) s_curve_enable: f32,
    pub(crate) shoulder_strength: f32,
    pub(crate) linear_strength: f32,
    pub(crate) linear_angle: f32,
    pub(crate) toe_strength: f32,
    pub(crate) toe_numerator: f32,
    pub(crate) toe_denominator: f32,
}

type Mat4 = [f32; 16];

const IDENTITY_ROW3: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// The order Black Ops sums each element's four products in: entry `i` of
/// row-major `a × b` adds `a[row][k] * b[k][col]` over these `k`.
const MUL_ORDER: [[usize; 4]; 16] = [
    [0, 2, 1, 3],
    [2, 0, 1, 3],
    [0, 2, 1, 3],
    [1, 3, 0, 2],
    [1, 2, 3, 0],
    [0, 1, 2, 3],
    [1, 2, 3, 0],
    [0, 3, 2, 1],
    [1, 2, 3, 0],
    [0, 1, 3, 2],
    [1, 3, 2, 0],
    [0, 3, 2, 1],
    [1, 2, 3, 0],
    [0, 1, 3, 2],
    [1, 3, 2, 0],
    [0, 3, 2, 1],
];

/// Row-major `a × b`, summed in Black Ops' order.
fn mat_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [0.0; 16];
    for (i, order) in MUL_ORDER.iter().enumerate() {
        let (row, col) = (i / 4, i % 4);
        let term = |k: usize| a[row * 4 + k] * b[k * 4 + col];
        out[i] = ((term(order[0]) + term(order[1])) + term(order[2])) + term(order[3]);
    }
    out
}

fn rows(rows: [[f32; 4]; 3]) -> Mat4 {
    let mut out = [0.0; 16];
    for (r, row) in rows.iter().chain([&IDENTITY_ROW3]).enumerate() {
        out[r * 4..r * 4 + 4].copy_from_slice(row);
    }
    out
}

/// Snaps the 3×3 part to 1/1024 steps.
fn snap(m: &mut Mat4) {
    for i in [0, 1, 2, 4, 5, 6, 8, 9, 10] {
        m[i] = (m[i] * 1024.0 + 0.5).floor() * 0.000_976_562_5;
    }
}

const fn bits(rows: [[u32; 3]; 3]) -> [[f32; 4]; 3] {
    let mut out = [[0.0; 4]; 3];
    let mut r = 0;
    while r < 3 {
        let mut c = 0;
        while c < 3 {
            out[r][c] = f32::from_bits(rows[r][c]);
            c += 1;
        }
        r += 1;
    }
    out
}

const LINEAR_SRGB_TO_XYZ: [[f32; 4]; 3] = bits([
    [0x3ed3_2618, 0x3e59_b3d0, 0x3c9e_1b09],
    [0x3eb7_1759, 0x3f37_1759, 0x3df4_1f21],
    [0x3e38_d4fe, 0x3d93_dd98, 0x3f73_53f8],
]);
const BRADFORD: [[f32; 4]; 3] = bits([
    [0x3f65_2546, 0xbf40_0d1b, 0x3d1f_559b],
    [0x3e88_6595, 0x3fdb_53f8, 0xbd8c_49ba],
    [0xbe25_460b, 0x3d16_52bd, 0x3f83_c9ef],
]);
const BRADFORD_INVERSE: [[f32; 4]; 3] = bits([
    [0x3f7c_ab93, 0x3edd_5715, 0xbc0b_bd38],
    [0xbe16_9553, 0x3f04_b33e, 0x3d24_0421],
    [0x3e23_cd57, 0x3d49_e55c, 0x3f77_eec4],
]);
const XYZ_TO_LINEAR_SRGB: [[f32; 4]; 3] = bits([
    [0x404f_67cb, 0xbf78_214b, 0x3d63_e1bc],
    [0xbfc4_c4f0, 0x3ff0_2071, 0xbe50_e454],
    [0xbeff_44b3, 0x3d2a_364f, 0x3f87_4e12],
]);
/// Into and out of the frame where hue turns around the grey axis.
const HUE_TO_AXIS: [[f32; 4]; 3] = bits([
    [0x3f51_05eb, 0, 0x3f08_d5b4],
    [0xbed1_05eb, 0x3f35_04f1, 0x3f87_1b01],
    [0xbed1_05eb, 0xbf35_04f3, 0x3e11_7517],
]);
const HUE_FROM_AXIS: [[f32; 4]; 3] = bits([
    [0x3f58_c722, 0xbec1_837a, 0xbec1_837b],
    [0xbebe_f06d, 0x3eab_1978, 0xbf8a_3e94],
    [0x3f13_cd39, 0x3f13_cd39, 0x3f13_cd39],
]);
const DEGREES_TO_RADIANS: f32 = f32::from_bits(0x3c8e_fa03);

/// The colour of a black body (below 4000 K) or of daylight at `kelvin`, as
/// CIE xyz with w = 1.
fn chromaticity(kelvin: f32) -> [f32; 4] {
    let t = if 1650.0 > kelvin {
        1650.0
    } else if f64::from(kelvin) > 25000.0 {
        25000.0
    } else {
        f64::from(kelvin)
    };
    let t2 = t * t;
    let t3 = t2 * t;
    let x = if t < 4000.0 {
        (877.6956 / t - (266_123_900.000_000_03 / t3 + 234_358.0 / t2)) + 0.17991
    } else if t < 7000.0 {
        ((99.11 / t - 4_607_000_000.0 / t3) + 2_967_800.0 / t2) + 0.244_063
    } else {
        ((247.480_000_000_000_02 / t - 2_006_400_000.000_000_2 / t3) + 1_901_800.0 / t2) + 0.23704
    };
    let x2 = x * x;
    let x3 = x2 * x;
    let y = if t >= 4000.0 {
        (x * 2.87 - x2 * 3.0) - 0.275
    } else if x <= 0.50338 && t > 2222.0 {
        (x * 2.091_370_15 - (x2 * 1.374_185_93 + x3 * 0.954_947_6)) - 0.167_488_67
    } else {
        (x * 2.185_558_32 - (x3 * 1.106_381_4 + x2 * 1.348_110_2)) - 0.202_196_83
    };
    let (x, y) = (x as f32, y as f32);
    [x, y, (1.0 - x) - y, 1.0]
}

fn bradford_column(xyz: [f32; 4], column: usize) -> f32 {
    ((xyz[3] * IDENTITY_ROW3[column] + xyz[2] * BRADFORD[2][column]) + xyz[1] * BRADFORD[1][column])
        + xyz[0] * BRADFORD[0][column]
}

/// The colour matrix that moves the 6500 K white to `kelvin`.
fn color_temp_matrix(kelvin: f32) -> Mat4 {
    let reference = chromaticity(6500.0);
    let target = chromaticity(kelvin);
    let mut scale = [0.0; 16];
    for i in 0..3 {
        scale[i * 5] = bradford_column(target, i) / bradford_column(reference, i);
    }
    scale[15] = 1.0;
    let adapted = mat_mul(&mat_mul(&rows(BRADFORD), &scale), &rows(BRADFORD_INVERSE));
    let mut out = mat_mul(
        &mat_mul(&rows(LINEAR_SRGB_TO_XYZ), &adapted),
        &rows(XYZ_TO_LINEAR_SRGB),
    );
    snap(&mut out);
    out
}

/// The colour matrix that turns hue by `degrees` around the grey axis.
fn hue_matrix(degrees: f32) -> Mat4 {
    let radians = degrees * DEGREES_TO_RADIANS;
    let sin = f64::from(radians).sin() as f32;
    let cos = f64::from(radians).cos() as f32;
    let turn = rows([
        [cos, sin, 0.0, 0.0],
        [-sin, cos, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
    ]);
    let mut out = mat_mul(&mat_mul(&rows(HUE_TO_AXIS), &turn), &rows(HUE_FROM_AXIS));
    snap(&mut out);
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

impl Film {
    /// `postFxControl0`..`postFxControlA` for the film pass: three rows per
    /// zone (dark, mid, light), the bleach, and the zone ranges.
    #[must_use]
    pub(crate) fn constants(&self) -> [[f32; 4]; 11] {
        let mut out = [[0.0; 4]; 11];
        let tints = [self.dark_tint, self.mid_tint, self.light_tint];
        for zone in 0..3 {
            let color = mat_mul(
                &mat_mul(
                    &color_temp_matrix(self.color_temp[zone]),
                    &hue_matrix(self.hue[zone]),
                ),
                &saturation_matrix(self.saturation[zone]),
            );
            let contrast = self.contrast[zone];
            for channel in 0..3 {
                let scale = tints[zone][channel] * contrast;
                out[zone * 3 + channel] = [
                    scale * color[channel],
                    scale * color[4 + channel],
                    scale * color[8 + channel],
                    (1.0 - contrast) * 0.5,
                ];
            }
        }
        out[9] = [self.bleach[0], self.bleach[1], self.bleach[2], 1.0];
        let start = self.mid_start;
        let end = if self.mid_end < start {
            start
        } else {
            self.mid_end
        };
        let mut dark = start - self.dark_feather * start;
        let mut light = (1.0 - end) * self.light_feather + end;
        if dark == start {
            dark -= 6.103_515_6e-5;
        }
        if end == light {
            light += 6.103_515_6e-5;
        }
        let dark_scale = 1.0 / (start - dark);
        let light_scale = 1.0 / (light - end);
        out[10] = [
            dark_scale,
            -(dark_scale * dark),
            light_scale,
            -(light_scale * end),
        ];
        out
    }

    /// `postFxControl0`..`postFxControl2` for the S-curve the bloom apply
    /// pass draws with.
    #[must_use]
    pub(crate) fn s_curve_constants(&self) -> [[f32; 4]; 3] {
        let shoulder = self.shoulder_strength;
        let linear = self.linear_strength;
        let toe_den = self.toe_denominator * self.toe_strength;
        let angle = self.linear_angle * linear;
        let toe_num = self.toe_numerator * self.toe_strength;
        let toe_ratio = self.toe_numerator / self.toe_denominator;
        let white = 1.0
            / (((shoulder * 8.0 + angle) * 8.0 + toe_num)
                / ((shoulder * 8.0 + linear) * 8.0 + toe_den)
                - toe_ratio);
        [
            [shoulder, linear, toe_den, 0.0],
            [shoulder, angle, toe_num, 0.0],
            [-toe_ratio, white, 0.0, 0.0],
        ]
    }
}

/// A vision as Black Ops' vision reader leaves it, by its byte offsets.
pub(crate) struct VisionWords<'a>(pub &'a [u32]);

impl VisionWords<'_> {
    fn word(&self, offset: usize) -> u32 {
        self.0.get(offset / 4).copied().unwrap_or(0)
    }

    pub(crate) fn byte(&self, offset: usize) -> bool {
        self.word(offset) & 0xff != 0
    }

    pub(crate) fn float(&self, offset: usize) -> f32 {
        f32::from_bits(self.word(offset))
    }

    pub(crate) fn int(&self, offset: usize) -> i32 {
        self.word(offset) as i32
    }

    pub(crate) fn vec3(&self, offset: usize) -> [f32; 3] {
        std::array::from_fn(|i| self.float(offset + i * 4))
    }

    pub(crate) fn vec4(&self, offset: usize) -> [f32; 4] {
        std::array::from_fn(|i| self.float(offset + i * 4))
    }

    /// The film block Black Ops copies out of the vision for the renderer.
    #[must_use]
    pub(crate) fn film(&self) -> Film {
        let mut curve = [0.0; 6];
        for (i, value) in curve.iter_mut().enumerate() {
            *value = self.float(0xd0 + i * 4);
        }
        Film {
            mid_start: self.float(0x4),
            mid_end: self.float(0x8),
            dark_feather: self.float(0xc),
            light_feather: self.float(0x10),
            bleach: self.vec3(0x68),
            color_temp: self.vec3(0x2c),
            hue: self.vec3(0x14),
            saturation: self.vec3(0x20),
            dark_tint: self.vec3(0x50),
            mid_tint: self.vec3(0x44),
            light_tint: self.vec3(0x38),
            contrast: self.vec3(0x5c),
            enable: self.byte(0x0),
            lut: self.float(0x74),
            s_curve_enable: self.int(0xcc) as f32,
            shoulder_strength: curve[0],
            linear_strength: curve[1],
            linear_angle: curve[2],
            toe_strength: curve[3],
            toe_numerator: curve[4],
            toe_denominator: curve[5],
        }
    }
}
