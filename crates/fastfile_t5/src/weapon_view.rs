//! A Black Ops weapon as the zone stream holds it while it loads: its
//! geometry, and the definition's and the variant's fields read by offset
//! (`crate::size`), with pointers followed to the names they hold.

use alloc::vec::Vec;

use crate::content::ScriptStrings;
use crate::size as sz;
use crate::zone::{Ptr, WeaponGeometry, ZonePtr, ZoneStream};

#[derive(Clone, Copy)]
pub struct WeaponView<'s, 'z> {
    stream: &'s ZoneStream<'z>,
    strings: &'s ScriptStrings,
    geometry: WeaponGeometry,
}

impl<'z> ZoneStream<'z> {
    /// The weapon the stream loaded last, read with the zone's script strings.
    pub fn weapon_view<'s>(&'s self, strings: &'s ScriptStrings) -> Option<WeaponView<'s, 'z>> {
        Some(WeaponView {
            stream: self,
            strings,
            geometry: self.weapon()?,
        })
    }
}

impl<'s> WeaponView<'s, '_> {
    pub fn geometry(&self) -> &WeaponGeometry {
        &self.geometry
    }

    /// The string a geometry name pointer points at.
    pub fn cstr(&self, ptr: Option<Ptr>) -> Option<&'s str> {
        self.stream.cstr(ptr?).ok()
    }

    pub fn def_i32(&self, off: usize) -> i32 {
        self.geometry
            .weap_def
            .map_or(0, |body| self.stream.i32_at(body, off).unwrap_or(0))
    }

    pub fn def_u8(&self, off: usize) -> u8 {
        self.geometry
            .weap_def
            .map_or(0, |body| self.stream.u8_at(body, off).unwrap_or(0))
    }

    pub fn def_f32(&self, off: usize) -> f32 {
        self.geometry
            .weap_def
            .map_or(0.0, |body| self.stream.f32_at(body, off).unwrap_or(0.0))
    }

    pub fn variant_i32(&self, off: usize) -> i32 {
        self.geometry
            .variant
            .map_or(0, |variant| self.stream.i32_at(variant, off).unwrap_or(0))
    }

    pub fn variant_u8(&self, off: usize) -> u8 {
        self.geometry
            .variant
            .map_or(0, |variant| self.stream.u8_at(variant, off).unwrap_or(0))
    }

    pub fn variant_f32(&self, off: usize) -> f32 {
        self.geometry.variant.map_or(0.0, |variant| {
            self.stream.f32_at(variant, off).unwrap_or(0.0)
        })
    }

    fn follow(&self, body: Ptr, off: usize) -> Option<Ptr> {
        match self.stream.ptr_at(body, off).ok()? {
            ZonePtr::Offset(q) => Some(self.stream.resolve_alias(q)),
            _ => None,
        }
    }

    fn str_at(&self, body: Ptr, off: usize) -> Option<&'s str> {
        self.stream
            .cstr(self.follow(body, off)?)
            .ok()
            .filter(|s| !s.is_empty())
    }

    /// The non-empty string the definition's field at `off` points at.
    pub fn def_str(&self, off: usize) -> Option<&'s str> {
        self.str_at(self.geometry.weap_def?, off)
    }

    /// The name of the asset (material, effect) the definition's field at
    /// `off` points at.
    pub fn def_asset_name(&self, off: usize) -> Option<&'s str> {
        let header = self.follow(self.geometry.weap_def?, off)?;
        self.str_at(header, 0)
    }

    pub fn variant_asset_name(&self, off: usize) -> Option<&'s str> {
        let header = self.follow(self.geometry.variant?, off)?;
        self.str_at(header, 0)
    }

    /// The definition's asset field at `off`, when it is not null.
    pub fn def_slot(&self, off: usize) -> Option<Ptr> {
        let body = self.geometry.weap_def?;
        match self.stream.ptr_at(body, off).ok()? {
            ZonePtr::Null => None,
            _ => Some(body.at(off)),
        }
    }

    pub fn variant_slot(&self, off: usize) -> Option<Ptr> {
        let variant = self.geometry.variant?;
        match self.stream.ptr_at(variant, off).ok()? {
            ZonePtr::Null => None,
            _ => Some(variant.at(off)),
        }
    }

    /// The `N` floats the definition's pointer field at `off` points at.
    pub fn def_f32_array<const N: usize>(&self, off: usize) -> Option<[f32; N]> {
        let arr = self.follow(self.geometry.weap_def?, off)?;
        let mut values = [0.0; N];
        for (i, value) in values.iter_mut().enumerate() {
            *value = self.stream.f32_at(arr, i * 4).ok()?;
        }
        Some(values)
    }

    /// The `N` string pointers the definition's field at `off` points at.
    pub fn def_str_array<const N: usize>(&self, off: usize) -> Option<[Option<&'s str>; N]> {
        self.stream
            .string_array_at::<N>(self.geometry.weap_def?, off)
    }

    /// The notetrack map whose script-string keys and values the
    /// definition's two fields point at, up to the first empty key.
    pub fn def_note_map(&self, keys_off: usize, values_off: usize) -> Vec<(&'s str, &'s str)> {
        let Some(body) = self.geometry.weap_def else {
            return Vec::new();
        };
        let (Some(keys), Some(values)) =
            (self.follow(body, keys_off), self.follow(body, values_off))
        else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for i in 0..sz::WEAPON_NOTETRACK_COUNT {
            let Ok(key_id) = self.stream.u16_at(keys, i * 2) else {
                break;
            };
            if key_id == 0 {
                break;
            }
            let Ok(val_id) = self.stream.u16_at(values, i * 2) else {
                break;
            };
            let Some(key) = self.script_string(key_id) else {
                continue;
            };
            let Some(val) = self.script_string(val_id) else {
                continue;
            };
            out.push((key, val));
        }
        out
    }

    fn script_string(&self, id: u16) -> Option<&'s str> {
        self.strings.get(self.stream, id).filter(|s| !s.is_empty())
    }

    /// The variant's animation names, by the weapon's own slot.
    pub fn xanim_names(&self) -> [Option<&'s str>; sz::WEAPON_XANIM_COUNT] {
        let Some(arr) = self.geometry.sz_xanims else {
            return [None; sz::WEAPON_XANIM_COUNT];
        };
        core::array::from_fn(|i| self.str_at(arr, i * 4))
    }

    /// The variant's hide tags, in order, skipping empty slots.
    pub fn hide_tags(&self) -> Vec<&'s str> {
        let Some(arr) = self.geometry.hide_tags else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for i in 0..sz::WEAPON_HIDE_TAG_COUNT {
            let Ok(id) = self.stream.u16_at(arr, i * 2) else {
                break;
            };
            if id == 0 {
                continue;
            }
            if let Some(name) = self.script_string(id) {
                out.push(name);
            }
        }
        out
    }

    /// The definition's and the variant's bytes, when both are whole.
    pub fn def_bytes(&self) -> Option<(&'s [u8], &'s [u8])> {
        let def = self
            .stream
            .slice_at(self.geometry.weap_def?, 0, sz::WEAPON_DEF)
            .ok()?;
        let variant = self
            .stream
            .slice_at(self.geometry.variant?, 0, sz::WEAPON_VARIANT_DEF)
            .ok()?;
        Some((def, variant))
    }
}
