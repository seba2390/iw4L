//! A map's entity string and its static models' placements read out of the
//! zone.

use alloc::vec::Vec;

use crate::zone::{GfxWorldGeometry, Ptr, ZonePtr, ZoneStream};

const DRAW_INST: usize = 76;
const CULL_DIST_OFF: usize = 0x00;
const PLACEMENT_OFF: usize = 0x04;
const ORIGIN_OFF: usize = PLACEMENT_OFF;
const AXIS_OFF: usize = PLACEMENT_OFF + 12;
const SCALE_OFF: usize = PLACEMENT_OFF + 0x30;
const MODEL_OFF: usize = 0x38;
const REFLECTION_PROBE_OFF: usize = 0x4a;
const PRIMARY_LIGHT_OFF: usize = 0x4b;

/// A static model's placement as the world stores it.
#[derive(Clone, Copy, Debug)]
pub struct StaticModelDrawRead {
    /// The model's slot: where it was defined, or this field when the model
    /// was first defined inline here.
    pub model_slot: Ptr,
    pub origin: [f32; 3],
    pub axis: [[f32; 3]; 3],
    pub scale: f32,
    pub cull_dist: f32,
    pub reflection_probe_index: u8,
    pub primary_light_index: u8,
}

impl ZoneStream<'_> {
    /// The map entities' text, when the zone carries a map's entities.
    pub fn map_entity_string(&self) -> Option<&str> {
        let map_ents = self.map_ents()?;
        let ptr = map_ents.entity_string?;
        let bytes = self.slice_at(ptr, 0, map_ents.entity_chars).ok()?;
        core::str::from_utf8(bytes).ok()
    }
}

impl GfxWorldGeometry {
    /// Every static model's placement; `None` without the instance tables
    /// or when one does not read.
    pub fn read_static_model_draws(&self, s: &ZoneStream<'_>) -> Option<Vec<StaticModelDrawRead>> {
        let (Some(draw_insts), Some(_insts)) = (self.smodel_draw_insts, self.smodel_insts) else {
            return None;
        };
        let mut out = Vec::with_capacity(self.smodel_count);
        for index in 0..self.smodel_count {
            let inst = draw_insts.at(index * DRAW_INST);
            let mut axis = [[0.0; 3]; 3];
            for (row, values) in axis.iter_mut().enumerate() {
                for (column, value) in values.iter_mut().enumerate() {
                    *value = s.f32_at(inst, AXIS_OFF + (row * 3 + column) * 4).ok()?;
                }
            }
            let model_slot = match s.ptr_at(inst, MODEL_OFF).ok()? {
                ZonePtr::Offset(p) => p,
                _ => inst.at(MODEL_OFF),
            };
            out.push(StaticModelDrawRead {
                model_slot,
                origin: [
                    s.f32_at(inst, ORIGIN_OFF).ok()?,
                    s.f32_at(inst, ORIGIN_OFF + 4).ok()?,
                    s.f32_at(inst, ORIGIN_OFF + 8).ok()?,
                ],
                axis,
                scale: s.f32_at(inst, SCALE_OFF).ok()?,
                cull_dist: s.f32_at(inst, CULL_DIST_OFF).ok()?,
                reflection_probe_index: s.u8_at(inst, REFLECTION_PROBE_OFF).ok()?,
                primary_light_index: s.u8_at(inst, PRIMARY_LIGHT_OFF).ok()?,
            });
        }
        Some(out)
    }
}
