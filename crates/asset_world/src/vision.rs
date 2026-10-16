#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FilmVision {
    pub enable: bool,
    pub contrast: f32,
    pub brightness: f32,
    pub desaturation: f32,
    pub desaturation_dark: f32,
    pub invert: bool,
    pub light_tint: [f32; 3],
    pub medium_tint: [f32; 3],
    pub dark_tint: [f32; 3],
    pub glow_enable: bool,
    pub glow_radius: f32,
    pub glow_bloom_cutoff: f32,
    pub glow_bloom_desaturation: f32,
    pub glow_bloom_intensity: f32,
}

impl Default for FilmVision {
    fn default() -> Self {
        Self {
            enable: false,
            contrast: 1.0,
            brightness: 0.0,
            desaturation: 0.0,
            desaturation_dark: 0.0,
            invert: false,
            light_tint: [1.0; 3],
            medium_tint: [1.0; 3],
            dark_tint: [1.0; 3],
            glow_enable: false,
            glow_radius: 0.0,
            glow_bloom_cutoff: 0.0,
            glow_bloom_desaturation: 0.0,
            glow_bloom_intensity: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilmVisionParseError {
    Decode,
    InvalidScalar(&'static str),
    InvalidVector(&'static str),
    MissingField(&'static str),
}

fn is_vision_source(name: &str) -> bool {
    let name = name.replace('\\', "/").to_ascii_lowercase();
    name.starts_with("vision/") && name.ends_with(".vision")
}

fn scalar(value: &str, key: &'static str) -> Result<f32, FilmVisionParseError> {
    value
        .trim()
        .trim_matches('"')
        .parse()
        .map_err(|_| FilmVisionParseError::InvalidScalar(key))
}

fn vector(value: &str, key: &'static str) -> Result<[f32; 3], FilmVisionParseError> {
    let value = value.trim().trim_matches('"');
    let mut values = value.split_whitespace();
    let x = values
        .next()
        .ok_or(FilmVisionParseError::InvalidVector(key))?
        .parse()
        .map_err(|_| FilmVisionParseError::InvalidVector(key))?;
    let y = values
        .next()
        .ok_or(FilmVisionParseError::InvalidVector(key))?
        .parse()
        .map_err(|_| FilmVisionParseError::InvalidVector(key))?;
    let z = values
        .next()
        .ok_or(FilmVisionParseError::InvalidVector(key))?
        .parse()
        .map_err(|_| FilmVisionParseError::InvalidVector(key))?;
    if values.next().is_some() {
        return Err(FilmVisionParseError::InvalidVector(key));
    }
    Ok([x, y, z])
}

pub fn parse_film_vision_rawfile(
    name: &str,
    data: &[u8],
    zlib_compressed: bool,
) -> Result<Option<FilmVision>, FilmVisionParseError> {
    if !is_vision_source(name) {
        return Ok(None);
    }
    let source =
        super::decode_rawfile_text(data, zlib_compressed).ok_or(FilmVisionParseError::Decode)?;
    let mut enable = None;
    let mut contrast = None;
    let mut brightness = None;
    let mut desaturation = None;
    let mut desaturation_dark = None;
    let mut invert = None;
    let mut light_tint = None;
    let mut medium_tint = None;
    let mut dark_tint = None;
    let mut glow_enable = None;
    let mut glow_radius = None;
    let mut glow_bloom_cutoff = None;
    let mut glow_bloom_desaturation = None;
    let mut glow_bloom_intensity = None;
    for line in source.lines().map(str::trim) {
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let Some(split) = line.find(char::is_whitespace) else {
            continue;
        };
        let value = line[split..].trim();
        match line[..split].to_ascii_lowercase().as_str() {
            "r_filmenable" => enable = Some(scalar(value, "r_filmEnable")? != 0.0),
            "r_filmcontrast" => contrast = Some(scalar(value, "r_filmContrast")?),
            "r_filmbrightness" => brightness = Some(scalar(value, "r_filmBrightness")?),
            "r_filmdesaturation" => desaturation = Some(scalar(value, "r_filmDesaturation")?),
            "r_filmdesaturationdark" => {
                desaturation_dark = Some(scalar(value, "r_filmDesaturationDark")?)
            }
            "r_filminvert" => invert = Some(scalar(value, "r_filmInvert")? != 0.0),
            "r_filmlighttint" => light_tint = Some(vector(value, "r_filmLightTint")?),
            "r_filmmediumtint" => medium_tint = Some(vector(value, "r_filmMediumTint")?),
            "r_filmdarktint" => dark_tint = Some(vector(value, "r_filmDarkTint")?),
            "r_glow" => glow_enable = Some(scalar(value, "r_glow")? != 0.0),
            "r_glowradius0" => glow_radius = Some(scalar(value, "r_glowRadius0")?),
            "r_glowbloomcutoff" => glow_bloom_cutoff = Some(scalar(value, "r_glowBloomCutoff")?),
            "r_glowbloomdesaturation" => {
                glow_bloom_desaturation = Some(scalar(value, "r_glowBloomDesaturation")?)
            }
            "r_glowbloomintensity0" => {
                glow_bloom_intensity = Some(scalar(value, "r_glowBloomIntensity0")?)
            }
            _ => {}
        }
    }
    let missing = |key| FilmVisionParseError::MissingField(key);

    let glow = match (
        glow_enable,
        glow_radius,
        glow_bloom_cutoff,
        glow_bloom_desaturation,
        glow_bloom_intensity,
    ) {
        (Some(enable), Some(radius), Some(cutoff), Some(desaturation), Some(intensity)) => {
            (enable, radius, cutoff, desaturation, intensity)
        }
        _ => (false, 0.0, 0.0, 0.0, 0.0),
    };
    let desaturation = desaturation.ok_or_else(|| missing("r_filmDesaturation"))?;
    let light_tint = light_tint.ok_or_else(|| missing("r_filmLightTint"))?;
    let dark_tint = dark_tint.ok_or_else(|| missing("r_filmDarkTint"))?;
    Ok(Some(FilmVision {
        enable: enable.ok_or_else(|| missing("r_filmEnable"))?,
        contrast: contrast.ok_or_else(|| missing("r_filmContrast"))?,
        brightness: brightness.ok_or_else(|| missing("r_filmBrightness"))?,
        desaturation,
        desaturation_dark: desaturation_dark.unwrap_or(desaturation),
        invert: invert.ok_or_else(|| missing("r_filmInvert"))?,
        light_tint,
        medium_tint: medium_tint
            .unwrap_or_else(|| std::array::from_fn(|i| (light_tint[i] + dark_tint[i]) * 0.5)),
        dark_tint,
        glow_enable: glow.0,
        glow_radius: glow.1,
        glow_bloom_cutoff: glow.2,
        glow_bloom_desaturation: glow.3,
        glow_bloom_intensity: glow.4,
    }))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct T6FilmGrade {
    pub controls: [[f32; 4]; 14],
    pub strength: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum T6FilmGradeParseError {
    InvalidText,
    InvalidSpan,
    InvalidEnable,
    InvalidVector(String),
    MissingField(&'static str),
    DegenerateFilterWeights,
    NonFiniteControls,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct T6Bloom {
    pub controls: [[f32; 4]; 10],
    pub strength: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct T6Vision {
    pub film: Option<T6FilmGrade>,
    pub bloom: Option<T6Bloom>,
    pub tone_strength: f32,
    /// The fields it was derived from, for a vision read from a file.
    pub fields: Option<T6VisionFields>,
}

/// Black Ops 2's vision vector fields (`vc_*`), in its vision field table
/// order.
pub const T6_VISION_VECTORS: [&str; 23] = [
    "lib", "lig", "liw", "lob", "low", "rgbh", "rgbl", "yh", "yl", "rs", "re", "smr", "smg", "smb",
    "mmr", "mmg", "mmb", "hmr", "hmg", "hmb", "fgm", "fsm", "fbm",
];

/// A `.vision` file's color fields as Black Ops 2's vision loader keeps
/// them: `r_filmEnable` and the `vc_*` vectors, `None` where the file sets
/// none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct T6VisionFields {
    pub film_enable: Option<bool>,
    pub vectors: [Option<[f32; 4]>; 23],
}

impl T6VisionFields {
    fn vector(&self, name: &'static str) -> Option<[f32; 4]> {
        let index = T6_VISION_VECTORS.iter().position(|field| *field == name)?;
        self.vectors[index]
    }

    /// `visionsetnakedlerp`'s blend: each vector from `to` (fraction 0)
    /// toward `from` (fraction 1), `r_filmEnable` from `from`. A vector
    /// either side lacks is lacked.
    #[must_use]
    pub fn lerp(to: &Self, from: &Self, fraction: f32) -> Self {
        Self {
            film_enable: from.film_enable,
            vectors: std::array::from_fn(|i| {
                let (a, b) = (to.vectors[i]?, from.vectors[i]?);
                Some(std::array::from_fn(|c| (b[c] - a[c]) * fraction + a[c]))
            }),
        }
    }

    /// The film grade and bloom Black Ops 2 derives from these fields.
    pub fn vision(&self) -> Result<T6Vision, T6FilmGradeParseError> {
        Ok(T6Vision {
            film: self.film_grade()?,
            bloom: self.bloom()?,
            tone_strength: 1.0,
            fields: Some(*self),
        })
    }

    fn film_grade(&self) -> Result<Option<T6FilmGrade>, T6FilmGradeParseError> {
        use T6FilmGradeParseError as Error;
        if !self
            .film_enable
            .ok_or(Error::MissingField("r_filmEnable"))?
        {
            return Ok(None);
        }
        let field = |name: &'static str| self.vector(name).ok_or(Error::MissingField(name));
        let (mut rs, mut re) = (field("rs")?, field("re")?);
        const EPSILON: f32 = 1.0 / 4096.0;
        if re[0] <= rs[0] {
            re[0] = rs[0] + EPSILON;
        }
        if re[1] <= rs[1] {
            rs[1] = re[1] - EPSILON;
        }
        if re[2] <= rs[2] {
            rs[2] = re[2] - EPSILON;
        }
        if rs[3] <= re[2] {
            rs[3] = re[2] + EPSILON;
        }
        if re[3] <= rs[3] {
            re[3] = rs[3] + EPSILON;
        }
        let scale = [
            1.0 / (rs[0] - re[0]),
            1.0 / (re[1] - rs[1]),
            1.0 / (re[2] - rs[2]),
            1.0 / (rs[3] - re[3]),
        ];
        let bias = [
            -scale[0] * re[0],
            -scale[1] * rs[1],
            -scale[2] * rs[2],
            -scale[3] * re[3],
        ];
        let fsm = field("fsm")?;
        let sum = fsm[0] + fsm[1] + fsm[2];
        if !sum.is_finite() || sum <= 0.0 || fsm[..3].iter().any(|v| *v < 0.0) {
            return Err(Error::DegenerateFilterWeights);
        }
        let weight = 1.0 / sum;
        let grade = T6FilmGrade {
            controls: [
                scale,
                bias,
                field("smr")?,
                field("mmr")?,
                field("hmr")?,
                field("smg")?,
                field("mmg")?,
                field("hmg")?,
                field("smb")?,
                field("mmb")?,
                field("hmb")?,
                field("fgm")?,
                [fsm[0] * weight, fsm[1] * weight, fsm[2] * weight, fsm[3]],
                field("fbm")?,
            ],
            strength: 1.0,
        };
        if !grade.controls.iter().flatten().all(|v| v.is_finite()) {
            return Err(Error::NonFiniteControls);
        }
        Ok(Some(grade))
    }

    fn bloom(&self) -> Result<Option<T6Bloom>, T6FilmGradeParseError> {
        use T6FilmGradeParseError as Error;
        let names = T6_VISION_VECTORS[..9].iter();
        if !names.clone().any(|key| self.vector(key).is_some()) {
            return Ok(None);
        }
        let get = |key: &'static str| self.vector(key).ok_or(Error::MissingField(key));
        let black = get("lib")?;
        let gamma = get("lig")?;
        let white = get("liw")?;
        let out_black = get("lob")?;
        let out_white = get("low")?;
        let high_rgb = get("rgbh")?;
        let low_rgb = get("rgbl")?;
        let high_luma = get("yh")?;
        let low_luma = get("yl")?;
        let scale = std::array::from_fn(|i| 1.0 / (white[i] - black[i]).max(1.0 / 65536.0));
        let bias = std::array::from_fn(|i| -black[i] * scale[i]);
        let output_scale =
            std::array::from_fn(|i| (out_white[i] - out_black[i]).max(1.0 / 65536.0));
        let controls = [
            [
                high_rgb[3] * (1.0 - low_rgb[3]),
                high_luma[3] * (1.0 - low_luma[3]),
                high_rgb[3],
                high_luma[3],
            ],
            scale,
            bias,
            gamma,
            output_scale,
            out_black,
            high_rgb,
            low_rgb,
            high_luma,
            low_luma,
        ];
        if gamma.iter().any(|g| *g <= 0.0) || !controls.iter().flatten().all(|v| v.is_finite()) {
            return Err(Error::NonFiniteControls);
        }
        Ok(Some(T6Bloom {
            controls,
            strength: 1.0,
        }))
    }
}

pub type T6VisionCatalog =
    std::collections::BTreeMap<String, Result<T6Vision, T6FilmGradeParseError>>;

fn vector4(value: &str) -> Option<[f32; 4]> {
    let values: Vec<f32> = value
        .trim()
        .trim_matches('"')
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    values.try_into().ok()
}

/// A `.vision` file's `r_filmEnable` and `vc_*` fields; a later line sets a
/// field again.
pub fn parse_t6_vision_fields(source: &str) -> Result<T6VisionFields, T6FilmGradeParseError> {
    use T6FilmGradeParseError as Error;
    let mut fields = T6VisionFields {
        film_enable: None,
        vectors: [None; 23],
    };
    for line in source.lines().map(str::trim) {
        let Some(split) = line.find(char::is_whitespace) else {
            continue;
        };
        let key = line[..split].to_ascii_lowercase();
        let value = line[split..].trim();
        if key == "r_filmenable" {
            let value = value
                .trim_matches('"')
                .parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or(Error::InvalidEnable)?;
            fields.film_enable = Some(value != 0.0);
        } else if let Some(name) = key.strip_prefix("vc_")
            && let Some(index) = T6_VISION_VECTORS.iter().position(|field| *field == name)
        {
            fields.vectors[index] = Some(
                vector4(value)
                    .filter(|values| values.iter().all(|v| v.is_finite()))
                    .ok_or_else(|| Error::InvalidVector(name.to_owned()))?,
            );
        }
    }
    Ok(fields)
}

pub fn parse_t6_film_grade(source: &str) -> Result<Option<T6FilmGrade>, T6FilmGradeParseError> {
    parse_t6_vision_fields(source)?.film_grade()
}

pub fn parse_t6_vision(source: &str) -> Result<T6Vision, T6FilmGradeParseError> {
    parse_t6_vision_fields(source)?.vision()
}
