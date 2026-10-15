//! Black Ops vision files, read the way its renderer reads them, and the
//! film and S-curve shader constants it derives from one.

/// How a vision field's value is read and lerped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum T5VisionFieldKind {
    /// `%i`, stored as a byte.
    Bool,
    Float,
    Vec3,
    Vec4,
    /// `%f`, but snapped instead of lerped.
    Lut,
    /// `%i`, stored as an integer.
    Int,
}

use T5VisionFieldKind::{Bool, Float, Int, Lut, Vec3, Vec4};

/// Bytes in one vision.
pub const T5_VISION_SIZE: usize = 0x2a0;

/// Black Ops' vision field table, in its order: name, byte offset, kind.
/// A field may be named once per file; a later line naming it again is an
/// unknown field.
pub const T5_VISION_FIELDS: [(&str, usize, T5VisionFieldKind); 68] = [
    ("r_filmEnable", 0x0, Bool),
    ("r_filmContrast", 0x5c, Vec3),
    ("r_filmColorTemp", 0x2c, Vec3),
    ("r_filmHue", 0x14, Vec3),
    ("r_filmSaturation", 0x20, Vec3),
    ("r_filmDarkTint", 0x50, Vec3),
    ("r_filmMidTint", 0x44, Vec3),
    ("r_filmLightTint", 0x38, Vec3),
    ("r_filmMidStart", 0x4, Float),
    ("r_filmMidEnd", 0x8, Float),
    ("r_filmDarkFeather", 0xc, Float),
    ("r_filmLightFeather", 0x10, Float),
    ("r_filmBleach", 0x68, Vec3),
    ("r_primaryLightUseTweaks", 0xb0, Bool),
    ("r_primaryLightTweakDiffuseStrength", 0xb4, Float),
    ("r_primaryLightTweakSpecularStrength", 0xb8, Float),
    ("r_filmLut", 0x74, Lut),
    ("r_reviveFX_enable", 0x78, Bool),
    ("r_reviveFX_edgeColorTemp", 0x7c, Float),
    ("r_reviveFX_edgeSaturation", 0x80, Float),
    ("r_reviveFX_edgeScale", 0x84, Vec3),
    ("r_reviveFX_edgeContrast", 0x90, Vec3),
    ("r_reviveFX_edgeOffset", 0x9c, Vec3),
    ("r_reviveFX_edgeMaskAdjust", 0xa8, Float),
    ("r_reviveFX_edgeAmount", 0xac, Float),
    ("snd_masterRingmod", 0xbc, Float),
    ("snd_reverbRingmod", 0xc0, Float),
    ("snd_hiFilter", 0xc4, Float),
    ("snd_lowFilter", 0xc8, Float),
    ("r_sCurveEnable", 0xcc, Int),
    ("r_sCurveShoulderStrength", 0xd0, Float),
    ("r_sCurveLinearStrength", 0xd4, Float),
    ("r_sCurveLinearAngle", 0xd8, Float),
    ("r_sCurveToeStrength", 0xdc, Float),
    ("r_sCurveToeNumerator", 0xe0, Float),
    ("r_sCurveToeDenominator", 0xe4, Float),
    ("r_postEmissiveBrightening", 0xe8, Float),
    ("r_bloomBlurRadius", 0xec, Float),
    ("r_bloomTintWeights", 0xf0, Vec4),
    ("r_bloomColorScale", 0x100, Vec4),
    ("r_bloomTintScale", 0x110, Vec4),
    ("r_bloomCurveBreakpoint", 0x120, Vec4),
    ("r_bloomCurveLoBlack", 0x130, Vec4),
    ("r_bloomCurveLoGamma", 0x140, Vec4),
    ("r_bloomCurveLoWhite", 0x150, Vec4),
    ("r_bloomCurveLoRemapBlack", 0x160, Vec4),
    ("r_bloomCurveLoRemapWhite", 0x170, Vec4),
    ("r_bloomCurveHiBlack", 0x180, Vec4),
    ("r_bloomCurveHiGamma", 0x190, Vec4),
    ("r_bloomCurveHiWhite", 0x1a0, Vec4),
    ("r_bloomCurveHiRemapBlack", 0x1b0, Vec4),
    ("r_bloomCurveHiRemapWhite", 0x1c0, Vec4),
    ("r_bloomExpansionControl", 0x1d0, Vec4),
    ("r_bloomExpansionWeights", 0x1e0, Vec4),
    ("r_bloomExpansionSource", 0x1f0, Int),
    ("r_bloomPersistence", 0x1f4, Float),
    ("r_bloomStreakXLevels0", 0x1f8, Vec4),
    ("r_bloomStreakXLevels1", 0x208, Vec4),
    ("r_bloomStreakXInnerTint", 0x218, Vec3),
    ("r_bloomStreakXOuterTint", 0x224, Vec3),
    ("r_bloomStreakXTintControl", 0x230, Vec4),
    ("r_bloomStreakXTint", 0x240, Vec3),
    ("r_bloomStreakYLevels0", 0x24c, Vec4),
    ("r_bloomStreakYLevels1", 0x25c, Vec4),
    ("r_bloomStreakYInnerTint", 0x26c, Vec3),
    ("r_bloomStreakYOuterTint", 0x278, Vec3),
    ("r_bloomStreakYTintControl", 0x284, Vec4),
    ("r_bloomStreakYTint", 0x294, Vec3),
];

/// One vision as Black Ops holds it: the bytes its field table addresses.
/// A field a file leaves out keeps what the slot held before; a vision read
/// into a fresh slot holds zero there.
#[derive(Clone, Copy, PartialEq)]
pub struct T5Vision {
    words: [u32; T5_VISION_SIZE / 4],
}

impl Default for T5Vision {
    fn default() -> Self {
        Self {
            words: [0; T5_VISION_SIZE / 4],
        }
    }
}

impl std::fmt::Debug for T5Vision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("T5Vision")
            .field("film_enable", &self.byte(0))
            .finish_non_exhaustive()
    }
}

pub type T5VisionCatalog = std::collections::BTreeMap<String, T5Vision>;

impl T5Vision {
    fn byte(&self, offset: usize) -> bool {
        self.words[offset / 4] & 0xff != 0
    }

    fn float(&self, offset: usize) -> f32 {
        f32::from_bits(self.words[offset / 4])
    }

    fn int(&self, offset: usize) -> i32 {
        self.words[offset / 4] as i32
    }

    fn vec3(&self, offset: usize) -> [f32; 3] {
        std::array::from_fn(|i| self.float(offset + i * 4))
    }

    /// The film block Black Ops copies out of the vision for the renderer.
    #[must_use]
    pub fn film(&self) -> T5Film {
        let mut curve = [0.0; 6];
        for (i, value) in curve.iter_mut().enumerate() {
            *value = self.float(0xd0 + i * 4);
        }
        T5Film {
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

/// Reads a `vision/*.vision` raw file into `catalog` under its lower-case
/// name, as a fresh vision slot holds it; other raw files are left alone.
pub fn capture_t5_vision(
    catalog: &mut T5VisionCatalog,
    warnings: &mut Vec<String>,
    name: &str,
    data: &[u8],
    zlib_compressed: bool,
) {
    let key = name.replace('\\', "/").to_ascii_lowercase();
    if !key.starts_with("vision/") || !key.ends_with(".vision") {
        return;
    }
    let Some(text) = crate::decode_rawfile_text(data, zlib_compressed) else {
        warnings.push(format!("vision {key}: not text"));
        return;
    };
    let mut vision = T5Vision::default();
    warnings.extend(read_t5_vision(&text, &key, &mut vision));
    catalog.insert(key, vision);
}

/// Reads a vision file into `vision` as Black Ops does: each line names a
/// field and gives its value; a value that does not read leaves the field as
/// it was. Returns the warnings Black Ops prints.
pub fn read_t5_vision(text: &str, file: &str, vision: &mut T5Vision) -> Vec<String> {
    let mut warnings = Vec::new();
    let mut seen = [false; T5_VISION_FIELDS.len()];
    let mut tokens = Tokens::new(text);
    while let Some(name) = tokens.next_token() {
        let field = T5_VISION_FIELDS
            .iter()
            .enumerate()
            .find(|(i, (known, ..))| !seen[*i] && name.eq_ignore_ascii_case(known));
        let Some((index, &(_, offset, kind))) = field else {
            warnings.push(format!("WARNING: unknown dvar '{name}' in file '{file}'"));
            tokens.skip_rest_of_line();
            continue;
        };
        let value = tokens.rest_of_line();
        if !vision.set(offset, kind, &value) {
            warnings.push(format!("WARNING: malformed dvar '{name}' in file '{file}'"));
            continue;
        }
        seen[index] = true;
    }
    warnings
}

impl T5Vision {
    fn set(&mut self, offset: usize, kind: T5VisionFieldKind, value: &str) -> bool {
        let at = offset / 4;
        match kind {
            Bool | Int => {
                let Some(value) = scan_ints(value) else {
                    return false;
                };
                self.words[at] = if kind == Bool {
                    (self.words[at] & !0xff) | u32::from(value != 0)
                } else {
                    value as u32
                };
            }
            Float | Lut => {
                let Some([value]) = scan_floats::<1>(value) else {
                    return false;
                };
                self.words[at] = value.to_bits();
            }
            Vec3 => {
                let Some(values) = scan_floats::<3>(value) else {
                    return false;
                };
                for (i, value) in values.into_iter().enumerate() {
                    self.words[at + i] = value.to_bits();
                }
            }
            Vec4 => {
                let Some(values) = scan_floats::<4>(value) else {
                    return false;
                };
                for (i, value) in values.into_iter().enumerate() {
                    self.words[at + i] = value.to_bits();
                }
            }
        }
        true
    }
}

/// `%i`: the leading integer.
fn scan_ints(text: &str) -> Option<i32> {
    let text = text.trim_start();
    let end = text
        .char_indices()
        .find(|&(i, c)| !(c.is_ascii_digit() || (i == 0 && (c == '-' || c == '+'))))
        .map_or(text.len(), |(i, _)| i);
    text[..end].parse().ok()
}

/// `%f %f …`: the leading floats, whitespace between them.
fn scan_floats<const N: usize>(text: &str) -> Option<[f32; N]> {
    let mut out = [0.0; N];
    let mut words = text.split_whitespace();
    for value in &mut out {
        let word = words.next()?;
        let end = word
            .char_indices()
            .find(|&(i, c)| {
                !(c.is_ascii_digit()
                    || c == '.'
                    || ((c == 'e' || c == 'E') && i > 0)
                    || ((c == '-' || c == '+') && (i == 0 || word[..i].ends_with(['e', 'E']))))
            })
            .map_or(word.len(), |(i, _)| i);
        *value = word[..end].parse().ok()?;
        if end != word.len() {
            // `%f` stops at the first character that does not read; the
            // next conversion then fails on it.
            return (N == 1).then_some(out);
        }
    }
    Some(out)
}

/// Black Ops' script tokenizer as the vision reader uses it: tokens split by
/// whitespace, `"…"` quoted, `//` and `/* */` comments skipped.
struct Tokens<'a> {
    text: &'a str,
    at: usize,
}

impl<'a> Tokens<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, at: 0 }
    }

    fn rest(&self) -> &'a str {
        &self.text[self.at..]
    }

    fn skip_space_and_comments(&mut self, cross_lines: bool) -> bool {
        loop {
            let rest = self.rest();
            let Some(c) = rest.chars().next() else {
                return false;
            };
            if c == '\n' && !cross_lines {
                return false;
            }
            if c.is_whitespace() {
                self.at += c.len_utf8();
            } else if rest.starts_with("//") {
                let end = rest.find('\n').unwrap_or(rest.len());
                self.at += end;
            } else if let Some(comment) = rest.strip_prefix("/*") {
                let end = comment.find("*/").map_or(rest.len(), |end| end + 4);
                self.at += end;
            } else {
                return true;
            }
        }
    }

    fn token(&mut self) -> String {
        let rest = self.rest();
        if let Some(quoted) = rest.strip_prefix('"') {
            let end = quoted.find('"').unwrap_or(quoted.len());
            self.at += 1 + end + usize::from(end < quoted.len());
            return quoted[..end].to_owned();
        }
        let end = rest
            .find(|c: char| c.is_whitespace() || c == '"')
            .unwrap_or(rest.len());
        self.at += end;
        rest[..end].to_owned()
    }

    fn next_token(&mut self) -> Option<String> {
        self.skip_space_and_comments(true).then(|| self.token())
    }

    /// The tokens left on this line, joined by single spaces.
    fn rest_of_line(&mut self) -> String {
        let mut parts = Vec::new();
        while self.skip_space_and_comments(false) {
            parts.push(self.token());
        }
        parts.join(" ")
    }

    fn skip_rest_of_line(&mut self) {
        while self.skip_space_and_comments(false) {
            self.token();
        }
    }
}

/// The film block Black Ops' renderer reads: three of each zone value are
/// (dark, mid, light).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct T5Film {
    pub mid_start: f32,
    pub mid_end: f32,
    pub dark_feather: f32,
    pub light_feather: f32,
    pub bleach: [f32; 3],
    pub color_temp: [f32; 3],
    pub hue: [f32; 3],
    pub saturation: [f32; 3],
    pub dark_tint: [f32; 3],
    pub mid_tint: [f32; 3],
    pub light_tint: [f32; 3],
    pub contrast: [f32; 3],
    pub enable: bool,
    pub lut: f32,
    pub s_curve_enable: f32,
    pub shoulder_strength: f32,
    pub linear_strength: f32,
    pub linear_angle: f32,
    pub toe_strength: f32,
    pub toe_numerator: f32,
    pub toe_denominator: f32,
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

impl T5Film {
    /// `postFxControl0`..`postFxControlA` for the film pass: three rows per
    /// zone (dark, mid, light), the bleach, and the zone ranges.
    #[must_use]
    pub fn constants(&self) -> [[f32; 4]; 11] {
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
    pub fn s_curve_constants(&self) -> [[f32; 4]; 3] {
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
