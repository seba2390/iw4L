use super::script::tables::Tables;
use super::script::{Environment, FaultKind, Value};
use super::{Axis, State};
use std::sync::Arc;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnimationState {
    pub shader_vectors: [Option<f32>; 6],
    pub material: Option<u32>,
    pub font: Option<u32>,
    pub alignment: Option<u8>,
    pub ui_3d_window: Option<i32>,
    pub horizontal: Option<Axis>,
    pub vertical: Option<Axis>,
    pub zoom: Option<f32>,
    pub rotation: [Option<f32>; 3],
    pub scale_delta: Option<f32>,
    pub color: [Option<f32>; 3],
    pub opacity: Option<f32>,
    pub spacing: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnimationStep {
    Interpolate(f32),
    Complete { lateness: f32, interrupted: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimationTimer {
    duration: i32,
    remaining: i32,
    options: [bool; 2],
}

impl AnimationTimer {
    pub fn new(duration: i32, options: [bool; 2]) -> Self {
        Self {
            duration,
            remaining: duration,
            options,
        }
    }

    pub fn remaining(&self) -> i32 {
        self.remaining
    }

    pub fn advance_clocks(
        &mut self,
        game_elapsed: i32,
        real_elapsed: i32,
        use_game_time: bool,
    ) -> AnimationStep {
        self.advance(if use_game_time {
            game_elapsed
        } else {
            real_elapsed
        })
    }

    pub fn advance(&mut self, elapsed: i32) -> AnimationStep {
        if self.remaining > 0 {
            self.remaining = self.remaining.wrapping_sub(elapsed);
            let fraction = (1.0 - self.remaining as f32 / self.duration as f32).min(1.0);
            let fraction = if (self.options[0] && fraction < 0.5)
                || (self.options[1] && fraction > 0.5)
            {
                ((fraction * std::f32::consts::PI + std::f32::consts::PI * 1.5).sin() + 1.0) * 0.5
            } else {
                fraction
            };
            if fraction < 1.0 {
                return AnimationStep::Interpolate(fraction);
            }
        }
        self.complete(false)
    }

    pub fn interrupt(&mut self) -> AnimationStep {
        self.complete(true)
    }

    fn complete(&mut self, interrupted: bool) -> AnimationStep {
        let lateness = self.remaining.wrapping_neg() as f32;
        self.remaining = 0;
        AnimationStep::Complete {
            lateness,
            interrupted,
        }
    }
}

impl State {
    pub fn interpolate(&mut self, source: &Self, target: &Self, fraction: f32) {
        let inverse = 1.0 - fraction;
        let blend = |start: f32, end: f32| start * inverse + end * fraction;
        for ((value, start), end) in self
            .shader_vectors
            .iter_mut()
            .zip(source.shader_vectors)
            .zip(target.shader_vectors)
        {
            if let (Some(start), Some(end)) = (start, end) {
                *value = Some(std::array::from_fn(|i| blend(start[i], end[i])));
            }
        }
        for (indices, enabled) in [
            (
                [0, 2],
                source.changes.horizontal && target.changes.horizontal,
            ),
            ([1, 3], source.changes.vertical && target.changes.vertical),
        ] {
            for index in indices {
                self.resolved_rectangle[index] = if enabled {
                    blend(
                        source.resolved_rectangle[index],
                        target.resolved_rectangle[index],
                    )
                } else {
                    source.resolved_rectangle[index]
                };
            }
        }
        for ((destination, (start, end)), enabled) in self
            .rotation
            .iter_mut()
            .zip(source.rotation.into_iter().zip(target.rotation))
            .zip(
                source
                    .changes
                    .rotation
                    .into_iter()
                    .zip(target.changes.rotation)
                    .map(|(start, end)| start && end),
            )
        {
            if enabled {
                *destination = blend(start, end);
            }
        }
        for ((destination, (start, end)), enabled) in self
            .color
            .iter_mut()
            .zip(source.color.into_iter().zip(target.color))
            .zip(
                source
                    .changes
                    .color
                    .into_iter()
                    .zip(target.changes.color)
                    .map(|(start, end)| start && end),
            )
        {
            if enabled {
                *destination = blend(start, end);
            }
        }
        for (destination, start, end, enabled) in [
            (
                &mut self.zoom,
                source.zoom,
                target.zoom,
                source.changes.zoom && target.changes.zoom,
            ),
            (
                &mut self.scale_delta,
                source.scale_delta,
                target.scale_delta,
                source.changes.scale_delta && target.changes.scale_delta,
            ),
            (
                &mut self.opacity,
                source.opacity,
                target.opacity,
                source.changes.opacity && target.changes.opacity,
            ),
            (
                &mut self.spacing,
                source.spacing,
                target.spacing,
                source.changes.spacing && target.changes.spacing,
            ),
        ] {
            if enabled {
                *destination = blend(start, end);
            }
        }
    }
}

fn field(tables: &mut Tables, table: &Value, name: &[u8]) -> Result<Value, FaultKind> {
    tables.field(table, &Value::Bytes(Arc::from(name)))
}

fn number(value: &Value) -> Result<f32, FaultKind> {
    match value {
        Value::Number(number) => Ok(*number),
        Value::Bytes(bytes) => Ok(core::str::from_utf8(bytes)
            .ok()
            .and_then(|text| text.trim().parse::<f32>().ok())
            .unwrap_or(0.0)),
        Value::Nil | Value::Boolean(_) | Value::Reference { .. } => Ok(0.0),
        Value::Hash(_) => Err(FaultKind::BindingRequired),
    }
}

fn optional_number(
    tables: &mut Tables,
    table: &Value,
    name: &[u8],
) -> Result<Option<f32>, FaultKind> {
    let value = field(tables, table, name)?;
    if value == Value::Nil {
        Ok(None)
    } else {
        number(&value).map(Some)
    }
}

fn axis(
    tables: &mut Tables,
    table: &Value,
    start: &[u8],
    end: &[u8],
    anchor_start: &[u8],
    anchor_end: &[u8],
) -> Result<Option<Axis>, FaultKind> {
    let Some(start) = optional_number(tables, table, start)? else {
        return Ok(None);
    };
    let anchor_start = field(tables, table, anchor_start)?.truthy();
    let end = number(&field(tables, table, end)?)?;
    let anchor_end = field(tables, table, anchor_end)?.truthy();
    Ok(Some(Axis {
        start,
        end,
        anchor_start,
        anchor_end,
    }))
}

impl AnimationState {
    pub fn apply(&self, destination: &mut State) {
        for (index, value) in self.shader_vectors.iter().enumerate() {
            if let Some(value) = value {
                destination.shader_vectors[index] = Some([*value, 0.0, 0.0, 0.0]);
            }
        }
        if let Some(material) = self.material {
            destination.material = Some(material);
        }
        if let Some(font) = self.font {
            destination.font = Some(font);
        }
        if let Some(alignment) = self.alignment {
            destination.alignment = Some(alignment);
        }
        if let Some(ui_3d_window) = self.ui_3d_window {
            destination.ui_3d_window = Some(ui_3d_window);
        }
        if let Some(axis) = self.horizontal {
            destination.horizontal = axis;
            destination.changes.horizontal = true;
        }
        if let Some(axis) = self.vertical {
            destination.vertical = axis;
            destination.changes.vertical = true;
        }
        fn scalar(value: Option<f32>, destination: &mut f32, changed: &mut bool) {
            if let Some(value) = value {
                *destination = value;
                *changed = true;
            }
        }
        scalar(
            self.zoom,
            &mut destination.zoom,
            &mut destination.changes.zoom,
        );
        for ((value, field), changed) in self
            .rotation
            .iter()
            .zip(&mut destination.rotation)
            .zip(&mut destination.changes.rotation)
        {
            scalar(*value, field, changed);
        }
        scalar(
            self.scale_delta,
            &mut destination.scale_delta,
            &mut destination.changes.scale_delta,
        );
        for ((value, field), changed) in self
            .color
            .iter()
            .zip(&mut destination.color)
            .zip(&mut destination.changes.color)
        {
            scalar(*value, field, changed);
        }
        scalar(
            self.opacity,
            &mut destination.opacity,
            &mut destination.changes.opacity,
        );
        scalar(
            self.spacing,
            &mut destination.spacing,
            &mut destination.changes.spacing,
        );
    }

    pub fn parse(tables: &mut Tables, table: &Value) -> Result<Self, FaultKind> {
        if !matches!(table, Value::Reference { kind: 5, .. }) {
            return Err(FaultKind::Argument(3));
        }
        let horizontal = axis(
            tables,
            table,
            b"left",
            b"right",
            b"leftAnchor",
            b"rightAnchor",
        )?;
        let vertical = axis(
            tables,
            table,
            b"top",
            b"bottom",
            b"topAnchor",
            b"bottomAnchor",
        )?;
        let zoom = optional_number(tables, table, b"zoom")?;
        let rotation = [
            optional_number(tables, table, b"xRot")?,
            optional_number(tables, table, b"yRot")?,
            optional_number(tables, table, b"zRot")?,
        ];
        let scale_delta = optional_number(tables, table, b"scale")?.map(|scale| scale - 1.0);
        let color = [
            optional_number(tables, table, b"red")?,
            optional_number(tables, table, b"green")?,
            optional_number(tables, table, b"blue")?,
        ];
        let mut opacity = optional_number(tables, table, b"alpha")?;
        if let Some(multiplier) = optional_number(tables, table, b"alphaMultiplier")? {
            opacity = Some(multiplier);
        }
        let spacing = optional_number(tables, table, b"spacing")?;
        let mut shader_vectors = [None; 6];
        for (index, value) in shader_vectors.iter_mut().enumerate() {
            *value = optional_number(tables, table, format!("shaderVector{index}").as_bytes())?;
        }
        let ui_3d_window = optional_number(tables, table, b"ui3DWindow")?.map(|value| value as i32);
        Ok(Self {
            shader_vectors,
            material: match field(tables, table, b"material")? {
                Value::Nil => None,
                Value::Reference { kind: 2, identity } => Some(identity),
                _ => return Err(FaultKind::BindingRequired),
            },
            font: match field(tables, table, b"font")? {
                Value::Nil => None,
                Value::Reference { kind: 2, identity } => Some(identity),
                _ => return Err(FaultKind::BindingRequired),
            },
            alignment: optional_number(tables, table, b"alignment")?.map(|value| value as u8),
            ui_3d_window,
            horizontal,
            vertical,
            zoom,
            rotation,
            scale_delta,
            color,
            opacity,
            spacing,
        })
    }
}
