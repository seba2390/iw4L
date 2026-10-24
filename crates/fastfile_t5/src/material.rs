//! Materials, technique sets, shaders and images read out of the zone into
//! plain data.

use alloc::vec::Vec;

use crate::size::{
    GFX_STATE_BITS, TECHNIQUE_OCCUPANCY_WORDS, TECHNIQUE_SLOT_COUNT, occupancy_any, occupancy_test,
};
use crate::zone::{
    GfxImageGeometry, MaterialGeometry, Ptr, ShaderGeometry, TechniqueSetGeometry, ZoneStream,
};

/// Which of a technique set's slots hold a technique, which of those had
/// their body read, and the pass counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TechniqueOccupancy {
    pub slots: [u64; TECHNIQUE_OCCUPANCY_WORDS],
    pub scanned: [u64; TECHNIQUE_OCCUPANCY_WORDS],
    pub technique0_flags: u8,
    pub max_pass_count: u16,
    pub pass_count_by_slot: [u8; TECHNIQUE_SLOT_COUNT],
}

impl Default for TechniqueOccupancy {
    fn default() -> Self {
        Self {
            slots: [0; TECHNIQUE_OCCUPANCY_WORDS],
            scanned: [0; TECHNIQUE_OCCUPANCY_WORDS],
            technique0_flags: 0,
            max_pass_count: 0,
            pass_count_by_slot: [0; TECHNIQUE_SLOT_COUNT],
        }
    }
}

impl TechniqueOccupancy {
    pub fn slot_occupied(self, slot: usize) -> bool {
        occupancy_test(&self.slots, slot)
    }

    /// Two reads of the same technique set as one: every slot either holds,
    /// and where this one has no flags or pass count, the other's.
    pub fn union(mut self, other: TechniqueOccupancy) -> TechniqueOccupancy {
        for i in 0..TECHNIQUE_OCCUPANCY_WORDS {
            self.slots[i] |= other.slots[i];
            self.scanned[i] |= other.scanned[i];
        }
        self.max_pass_count = self.max_pass_count.max(other.max_pass_count);
        if self.technique0_flags == 0 {
            self.technique0_flags = other.technique0_flags;
        }
        for i in 0..TECHNIQUE_SLOT_COUNT {
            if self.pass_count_by_slot[i] == 0 {
                self.pass_count_by_slot[i] = other.pass_count_by_slot[i];
            }
        }
        self
    }
}

impl TechniqueSetGeometry {
    /// `None` when no slot holds a technique.
    pub fn occupancy(&self) -> Option<TechniqueOccupancy> {
        occupancy_any(&self.technique_slots).then_some(TechniqueOccupancy {
            slots: self.technique_slots,
            scanned: self.technique_slots_scanned,
            technique0_flags: self.technique0_flags,
            max_pass_count: self.max_pass_count,
            pass_count_by_slot: self.pass_count_by_slot,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MaterialTextureRead {
    pub name_hash: u32,
    pub name_start: u8,
    pub name_end: u8,
    pub sampler_state: u8,
    pub semantic: u8,
    /// Where the texture's image pointer sits.
    pub image_slot: Ptr,
}

#[derive(Clone, Copy, Debug)]
pub struct MaterialConstantRead {
    pub name_hash: u32,
    pub name: [u8; 12],
    pub literal: [f32; 4],
}

impl MaterialGeometry {
    pub fn read_textures(&self, s: &ZoneStream<'_>) -> Option<Vec<MaterialTextureRead>> {
        let mut textures = Vec::with_capacity(self.texture_count);
        if let Some(table) = self.textures {
            for i in 0..self.texture_count {
                let texture = table.at(i * 16);
                let semantic = s.u8_at(texture, 7).ok()?;
                textures.push(MaterialTextureRead {
                    name_hash: s.u32_at(texture, 0).ok()?,
                    name_start: s.u8_at(texture, 4).ok()?,
                    name_end: s.u8_at(texture, 5).ok()?,
                    sampler_state: s.u8_at(texture, 6).ok()?,
                    semantic,
                    image_slot: texture.at(12),
                });
            }
        }
        Some(textures)
    }

    pub fn read_constants(&self, s: &ZoneStream<'_>) -> Option<Vec<MaterialConstantRead>> {
        let mut constants = Vec::with_capacity(self.constant_count);
        if let Some(table) = self.constants {
            for i in 0..self.constant_count {
                let constant = table.at(i * 32);
                let mut name = [0; 12];
                for (offset, byte) in name.iter_mut().enumerate() {
                    *byte = s.u8_at(constant, 4 + offset).ok()?;
                }
                constants.push(MaterialConstantRead {
                    name_hash: s.u32_at(constant, 0).ok()?,
                    name,
                    literal: [
                        s.f32_at(constant, 16).ok()?,
                        s.f32_at(constant, 20).ok()?,
                        s.f32_at(constant, 24).ok()?,
                        s.f32_at(constant, 28).ok()?,
                    ],
                });
            }
        }
        Some(constants)
    }

    /// The state-bit rows, each two words; the table ends at the first row
    /// that does not read.
    pub fn read_state_bits(&self, s: &ZoneStream<'_>) -> Vec<[u32; 2]> {
        let word = |offset| s.u32_at(self.state_bits?, offset).ok();
        let mut table = Vec::with_capacity(self.state_bits_count);
        for index in 0..self.state_bits_count {
            let offset = index * GFX_STATE_BITS;
            let (Some(low), Some(high)) = (word(offset), word(offset + 4)) else {
                break;
            };
            table.push([low, high]);
        }
        table
    }
}

impl ShaderGeometry {
    /// The program's bytes; empty without a program.
    pub fn read_program(&self, s: &ZoneStream<'_>) -> Option<Vec<u8>> {
        let mut bytes = Vec::with_capacity(self.program_words * 4);
        if let Some(program) = self.program {
            for i in 0..self.program_words {
                bytes.extend_from_slice(&s.u32_at(program, i * 4).ok()?.to_le_bytes());
            }
        }
        Some(bytes)
    }
}

impl GfxImageGeometry {
    /// The image's pixels as the zone carries them; empty when it carries
    /// none.
    pub fn read_payload(&self, s: &ZoneStream<'_>) -> Option<Vec<u8>> {
        if self.source_len == 0 {
            return Some(Vec::new());
        }
        Some(
            s.source_slice(self.source_offset, self.source_len)
                .ok()?
                .to_vec(),
        )
    }
}
