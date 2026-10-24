//! An effect read out of the zone into plain data: its header, and each
//! element's record, velocity and visibility samples, visuals and child
//! effects.

use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;

use crate::size::{
    FX_EFFECT_DEF, FX_EFFECT_DEF_EMISSION_OFF, FX_EFFECT_DEF_LOOPING_OFF,
    FX_EFFECT_DEF_ONESHOT_OFF, FX_ELEM_DECAL, FX_ELEM_DEF, FX_ELEM_EFFECT_EMITTED_OFF,
    FX_ELEM_EFFECT_ON_DEATH_OFF, FX_ELEM_EFFECT_ON_IMPACT_OFF, FX_ELEM_MARK_VISUALS, FX_ELEM_MODEL,
    FX_ELEM_RUNNER, FX_ELEM_SOUND, FX_ELEM_TRAIL_DEF_OFF, FX_ELEM_TYPE_OFF,
    FX_ELEM_VEL_SAMPLES_OFF, FX_ELEM_VEL_STATE_SAMPLE, FX_ELEM_VIS_SAMPLES_OFF,
    FX_ELEM_VIS_STATE_SAMPLE, FX_ELEM_VISUALS, FX_ELEM_VISUALS_OFF,
};
use crate::zone::{FxEffectDefGeometry, Ptr, ZonePtr, ZoneStream};

/// The highest stored element type that draws sprites.
const LAST_SPRITE_TYPE: u8 = 6;

#[derive(Clone, Debug)]
pub struct FxEffectRead {
    pub name: String,
    /// The effect's record, `FX_EFFECT_DEF` bytes.
    pub header: Vec<u8>,
    pub elems: Vec<FxElemRead>,
}

#[derive(Clone, Debug)]
pub struct FxElemRead {
    /// The element's record, `FX_ELEM_DEF` bytes.
    pub raw: Vec<u8>,
    pub vel_samples: Vec<u8>,
    pub vis_samples: Vec<u8>,
    pub visuals: FxVisualsRead,
    pub effect_on_impact: String,
    pub effect_on_death: String,
    pub effect_emitted: String,
    /// The element points at a trail.
    pub has_extended: bool,
}

/// An element's visuals, by what its type draws. With more than one visual
/// they sit in an array the visual field points at; `None` (or `Err`) when
/// that field does not hold an offset.
#[derive(Clone, Debug)]
pub enum FxVisualsRead {
    /// A light, an element without visuals, or a type that draws none.
    None,
    /// Sprites' materials; the error carries the field and what it held.
    Materials(Result<Vec<FxMaterialSlotRead>, (Ptr, &'static str)>),
    /// Decals: a pair of materials per mark.
    Marks(Option<Vec<[FxMaterialSlotRead; 2]>>),
    Models(Option<Vec<String>>),
    Runners(Option<Vec<String>>),
    Sounds(Option<Vec<String>>),
}

/// Where a material handle sits, and the material it points at when it
/// holds an offset.
#[derive(Clone, Copy, Debug)]
pub struct FxMaterialSlotRead {
    pub slot: Ptr,
    pub alias: Option<Ptr>,
}

impl FxEffectDefGeometry {
    /// The effect read out; `None` without a name, when its record does not
    /// agree with the counts the loader saw, or when an element does not
    /// read.
    pub fn read(&self, s: &ZoneStream<'_>) -> Option<FxEffectRead> {
        let name = s.cstr(self.name?).ok()?;
        if name.is_empty() {
            return None;
        }
        let header = s.slice_at(self.header, 0, FX_EFFECT_DEF).ok()?;
        if header.len() < FX_EFFECT_DEF {
            return None;
        }
        let count = |off: usize| {
            i32::from_le_bytes([
                header[off],
                header[off + 1],
                header[off + 2],
                header[off + 3],
            ])
        };
        if count(FX_EFFECT_DEF_LOOPING_OFF) != self.looping_count
            || count(FX_EFFECT_DEF_ONESHOT_OFF) != self.one_shot_count
            || count(FX_EFFECT_DEF_EMISSION_OFF) != self.emission_count
        {
            return None;
        }
        let mut elems = Vec::with_capacity(self.elem_def_count);
        if let Some(arr) = self.elem_defs {
            for i in 0..self.elem_def_count {
                elems.push(read_elem(s, arr.at(i * FX_ELEM_DEF))?);
            }
        } else if self.elem_def_count > 0 {
            return None;
        }
        Some(FxEffectRead {
            name: name.to_owned(),
            header: header.to_vec(),
            elems,
        })
    }
}

fn read_elem(s: &ZoneStream<'_>, p: Ptr) -> Option<FxElemRead> {
    let raw = s.slice_at(p, 0, FX_ELEM_DEF).ok()?.to_vec();
    if raw.len() < FX_ELEM_DEF {
        return None;
    }
    let elem_type = raw[FX_ELEM_TYPE_OFF];
    let visual_count = raw[FX_ELEM_TYPE_OFF + 1] as usize;
    let vel_count = raw[FX_ELEM_TYPE_OFF + 2] as usize + 1;
    let vis_count = raw[FX_ELEM_TYPE_OFF + 3] as usize + 1;
    let vel_samples = copy_ptr_array(
        s,
        p,
        FX_ELEM_VEL_SAMPLES_OFF,
        vel_count,
        FX_ELEM_VEL_STATE_SAMPLE,
    );
    let vis_samples = copy_ptr_array(
        s,
        p,
        FX_ELEM_VIS_SAMPLES_OFF,
        vis_count,
        FX_ELEM_VIS_STATE_SAMPLE,
    );
    let visuals = read_visuals(s, p.at(FX_ELEM_VISUALS_OFF), elem_type, visual_count);
    let effect_on_impact = read_name(s, p, FX_ELEM_EFFECT_ON_IMPACT_OFF);
    let effect_on_death = read_name(s, p, FX_ELEM_EFFECT_ON_DEATH_OFF);
    let effect_emitted = read_name(s, p, FX_ELEM_EFFECT_EMITTED_OFF);
    let has_extended = !matches!(s.ptr_at(p, FX_ELEM_TRAIL_DEF_OFF).ok()?, ZonePtr::Null);
    Some(FxElemRead {
        raw,
        vel_samples,
        vis_samples,
        visuals,
        effect_on_impact,
        effect_on_death,
        effect_emitted,
        has_extended,
    })
}

fn read_visuals(s: &ZoneStream<'_>, vis: Ptr, elem_type: u8, count: usize) -> FxVisualsRead {
    if count == 0 {
        return FxVisualsRead::None;
    }
    let array = || match s.ptr_at(vis, 0) {
        Ok(ZonePtr::Offset(arr)) => Ok(s.resolve_alias(arr)),
        Ok(ZonePtr::Following) => Err("following"),
        Ok(ZonePtr::Insert) => Err("insert"),
        Ok(ZonePtr::Null) => Err("null"),
        Err(_) => Err("read_err"),
    };
    let names = |at: fn(Ptr, usize) -> Ptr| -> Option<Vec<String>> {
        if count == 1 {
            return Some(alloc::vec![read_name(s, vis, 0)]);
        }
        let arr = array().ok()?;
        Some((0..count).map(|i| read_name(s, at(arr, i), 0)).collect())
    };
    let visual = |arr: Ptr, i: usize| arr.at(i * FX_ELEM_VISUALS);
    match elem_type {
        0..=LAST_SPRITE_TYPE => {
            if count == 1 {
                return FxVisualsRead::Materials(Ok(alloc::vec![material_slot(s, vis)]));
            }
            FxVisualsRead::Materials(
                array()
                    .map(|arr| {
                        (0..count)
                            .map(|i| material_slot(s, visual(arr, i)))
                            .collect()
                    })
                    .map_err(|kind| (vis, kind)),
            )
        }
        FX_ELEM_DECAL => FxVisualsRead::Marks(array().ok().map(|arr| {
            (0..count.max(1))
                .map(|i| {
                    let m = arr.at(i * FX_ELEM_MARK_VISUALS);
                    [material_slot(s, m.at(0)), material_slot(s, m.at(4))]
                })
                .collect()
        })),
        FX_ELEM_MODEL => FxVisualsRead::Models(names(visual)),
        FX_ELEM_RUNNER => FxVisualsRead::Runners(names(visual)),
        FX_ELEM_SOUND => FxVisualsRead::Sounds(names(visual)),
        _ => FxVisualsRead::None,
    }
}

fn material_slot(s: &ZoneStream<'_>, slot: Ptr) -> FxMaterialSlotRead {
    let alias = match s.ptr_at(slot, 0) {
        Ok(ZonePtr::Offset(target)) => Some(s.resolve_alias(target)),
        _ => None,
    };
    FxMaterialSlotRead { slot, alias }
}

fn copy_ptr_array(
    s: &ZoneStream<'_>,
    parent: Ptr,
    field: usize,
    count: usize,
    stride: usize,
) -> Vec<u8> {
    if count == 0 {
        return Vec::new();
    }
    let Ok(ZonePtr::Offset(arr)) = s.ptr_at(parent, field) else {
        return Vec::new();
    };
    let arr = s.resolve_alias(arr);
    let nbytes = count.saturating_mul(stride);
    s.slice_at(arr, 0, nbytes)
        .map(|b| b.to_vec())
        .unwrap_or_else(|_| Vec::new())
}

fn read_name(s: &ZoneStream<'_>, p: Ptr, field: usize) -> String {
    match s.ptr_at(p, field) {
        Ok(ZonePtr::Offset(name)) => s.cstr(s.resolve_alias(name)).ok().unwrap_or("").to_owned(),
        _ => String::new(),
    }
}
