//! An animation's parts read out of the zone into plain data: its name,
//! its tracks' names, its notifies, its packed frame data and its root
//! motion.

use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::content::ScriptStrings;
use crate::zone::{Ptr, XAnimPartsGeometry, ZonePtr, ZoneStream};

/// An animation's parts as the zone stores them, read out.
#[derive(Clone, Debug, Default)]
pub struct XAnimRead {
    pub name: String,
    pub numframes: u16,
    pub flags: u8,
    pub bone_count: [u8; 10],
    pub framerate: f32,
    /// The tracks' bone names.
    pub names: Vec<String>,
    /// Each notify's name and time.
    pub notifies: Vec<(String, f32)>,
    pub data_byte: Vec<u8>,
    pub data_short: Vec<u16>,
    pub data_int: Vec<u32>,
    pub random_data_byte: Vec<u8>,
    pub random_data_short: Vec<u16>,
    pub random_data_int: Vec<u32>,
    pub indices: Vec<u16>,
    pub delta_trans: Option<XAnimDeltaTrans>,
    pub delta_quat: Option<XAnimDeltaQuat>,
}

/// Root motion's translation: its key count less one, its packing, its
/// range and its frame indices and packed keys.
#[derive(Clone, Debug, Default)]
pub struct XAnimDeltaTrans {
    pub size: u16,
    pub small: bool,
    pub mins: [f32; 3],
    pub step: [f32; 3],
    pub indices: Vec<u16>,
    pub packed: Vec<u8>,
}

/// Root motion's yaw: its frame indices and keys.
#[derive(Clone, Debug, Default)]
pub struct XAnimDeltaQuat {
    pub indices: Vec<u16>,
    pub frames: Vec<[i16; 4]>,
}

impl XAnimPartsGeometry {
    /// The parts read out; `None` without a name, or with tracks but no
    /// names for them.
    pub fn read(&self, s: &ZoneStream<'_>, strings: &ScriptStrings) -> Option<XAnimRead> {
        let name = s.cstr(self.name?).ok()?;
        if name.is_empty() {
            return None;
        }
        let track_count = self.bone_count[9] as usize;
        let mut names = Vec::with_capacity(track_count);
        if let Some(arr) = self.names {
            for i in 0..track_count {
                let sid = s.u16_at(arr, i * 2).unwrap_or(0);
                names.push(strings.get(s, sid).unwrap_or("").to_owned());
            }
        } else if track_count > 0 {
            return None;
        }

        let mut notifies = Vec::with_capacity(self.notify_count);
        if let Some(arr) = self.notify {
            for i in 0..self.notify_count {
                let b = arr.at(i * crate::size::XANIM_NOTIFY_INFO);
                let sid = s.u16_at(b, 0).unwrap_or(0);
                let time = s.f32_at(b, 4).unwrap_or(0.0);
                notifies.push((strings.get(s, sid).unwrap_or("").to_owned(), time));
            }
        }

        let indices = if self.indices_are_bytes {
            copy_u8(s, self.indices, self.index_count)
                .into_iter()
                .map(u16::from)
                .collect()
        } else {
            copy_u16(s, self.indices, self.index_count)
        };
        let (delta_trans, delta_quat) = copy_delta(s, self.delta, self.numframes);
        Some(XAnimRead {
            name: name.to_owned(),
            numframes: self.numframes,
            flags: self.flags,
            bone_count: self.bone_count,
            framerate: self.framerate,
            names,
            notifies,
            data_byte: copy_u8(s, self.data_byte, self.data_byte_count),
            data_short: copy_u16(s, self.data_short, self.data_short_count),
            data_int: copy_u32(s, self.data_int, self.data_int_count),
            random_data_byte: copy_u8(s, self.random_data_byte, self.random_data_byte_count),
            random_data_short: copy_u16(s, self.random_data_short, self.random_data_short_count),
            random_data_int: copy_u32(s, self.random_data_int, self.random_data_int_count),
            indices,
            delta_trans,
            delta_quat,
        })
    }
}

fn copy_delta(
    s: &ZoneStream<'_>,
    delta: Option<Ptr>,
    numframes: u16,
) -> (Option<XAnimDeltaTrans>, Option<XAnimDeltaQuat>) {
    let Some(delta) = delta else {
        return (None, None);
    };
    let body = |slot: usize| match s.ptr_at(delta, slot) {
        Ok(ZonePtr::Offset(p)) => Some(s.resolve_alias(p)),
        _ => None,
    };
    let indices = |p: Ptr, n: usize| -> Vec<u16> {
        if numframes < 256 {
            copy_u8(s, Some(p), n).into_iter().map(u16::from).collect()
        } else {
            copy_u16(s, Some(p), n)
        }
    };
    let frames_at = |p: Ptr, off: usize| match s.ptr_at(p, off) {
        Ok(ZonePtr::Offset(q)) => Some(s.resolve_alias(q)),
        _ => None,
    };
    let vec3 = |p: Ptr, off: usize| -> Option<[f32; 3]> {
        Some([
            s.f32_at(p, off).ok()?,
            s.f32_at(p, off + 4).ok()?,
            s.f32_at(p, off + 8).ok()?,
        ])
    };

    let trans = body(0).and_then(|p| {
        let size = s.u16_at(p, 0).ok()?;
        let small = s.u8_at(p, 2).ok()? != 0;
        if size == 0 {
            return Some(XAnimDeltaTrans {
                size,
                small,
                mins: vec3(p, 4)?,
                ..XAnimDeltaTrans::default()
            });
        }
        let n = usize::from(size) + 1;
        let packed = copy_u8(s, frames_at(p, 28), if small { 3 * n } else { 6 * n });
        Some(XAnimDeltaTrans {
            size,
            small,
            mins: vec3(p, 4)?,
            step: vec3(p, 16)?,
            indices: indices(p.at(32), n),
            packed,
        })
    });

    let quat = body(4).and_then(|p| {
        let size = s.u16_at(p, 0).ok()?;
        let pair = |q: Ptr, i: usize| -> Option<[i16; 4]> {
            Some([0, 0, s.i16_at(q, i * 4).ok()?, s.i16_at(q, i * 4 + 2).ok()?])
        };
        if size == 0 {
            return Some(XAnimDeltaQuat {
                indices: Vec::new(),
                frames: vec![pair(p.at(4), 0)?],
            });
        }
        let n = usize::from(size) + 1;
        let frames = frames_at(p, 4)?;
        Some(XAnimDeltaQuat {
            indices: indices(p.at(8), n),
            frames: (0..n).map(|i| pair(frames, i)).collect::<Option<_>>()?,
        })
    });
    (trans, quat)
}

fn copy_u8(s: &ZoneStream<'_>, ptr: Option<Ptr>, count: usize) -> Vec<u8> {
    let Some(p) = ptr else {
        return Vec::new();
    };
    s.slice_at(p, 0, count)
        .map(|b| b.to_vec())
        .unwrap_or_else(|_| Vec::new())
}

fn copy_u16(s: &ZoneStream<'_>, ptr: Option<Ptr>, count: usize) -> Vec<u16> {
    let Some(p) = ptr else {
        return Vec::new();
    };
    let Ok(bytes) = s.slice_at(p, 0, count * 2) else {
        return Vec::new();
    };
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect()
}

fn copy_u32(s: &ZoneStream<'_>, ptr: Option<Ptr>, count: usize) -> Vec<u32> {
    let Some(p) = ptr else {
        return Vec::new();
    };
    let Ok(bytes) = s.slice_at(p, 0, count * 4) else {
        return Vec::new();
    };
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}
