//! Black Ops vision files, read the way its renderer reads them: each line
//! names a field of its vision table and gives its value.

/// How a vision field's value is read and lerped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FieldKind {
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

use FieldKind::{Bool, Float, Int, Lut, Vec3, Vec4};

/// Bytes in one vision.
pub(crate) const VISION_SIZE: usize = 0x2a0;

/// Black Ops' vision field table, in its order: name, byte offset, kind.
/// A field may be named once per file; a later line naming it again is an
/// unknown field.
pub(crate) const VISION_FIELDS: [(&str, usize, FieldKind); 68] = [
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

/// One vision slot as Black Ops holds it: the bytes its field table
/// addresses. Reading a file into a slot sets only the fields the file names;
/// the rest keep what the slot held before (zero in a fresh slot).
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct VisionSlot {
    pub(crate) words: [u32; VISION_SIZE / 4],
}

impl Default for VisionSlot {
    fn default() -> Self {
        Self {
            words: [0; VISION_SIZE / 4],
        }
    }
}

impl std::fmt::Debug for VisionSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VisionSlot")
            .field("film_enable", &(self.words[0] & 0xff != 0))
            .finish_non_exhaustive()
    }
}

/// Reads a vision file into `vision` as Black Ops does: each line names a
/// field and gives its value; a value that does not read leaves the field as
/// it was. Returns the warnings Black Ops prints.
pub(crate) fn read_vision(text: &str, file: &str, vision: &mut VisionSlot) -> Vec<String> {
    let mut warnings = Vec::new();
    let mut seen = [false; VISION_FIELDS.len()];
    let mut tokens = Tokens::new(text);
    while let Some(name) = tokens.next_token() {
        let field = VISION_FIELDS
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

impl VisionSlot {
    fn set(&mut self, offset: usize, kind: FieldKind, value: &str) -> bool {
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
