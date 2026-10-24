//! The memory a zone loads into: one buffer per XFile block, and the insert
//! map of the virtual block's pointer slots.

use alloc::vec;
use alloc::vec::Vec;

use crate::zone::{MAX_XFILE_COUNT, Result, XFILE_BLOCK_RUNTIME, ZoneHeader, ZoneStream};

pub struct ZoneMemory {
    blocks: [Vec<u8>; MAX_XFILE_COUNT],
    insert_map: Vec<u8>,
}

impl ZoneMemory {
    pub fn for_header(header: &ZoneHeader) -> ZoneMemory {
        let blocks = core::array::from_fn(|i| vec![0u8; block_capacity(header, i)]);
        let insert_map = vec![0u8; ZoneStream::insert_map_len(header)];
        ZoneMemory { blocks, insert_map }
    }

    pub fn total_bytes(&self) -> usize {
        self.blocks.iter().map(|b| b.len()).sum()
    }

    pub fn stream<'a>(&'a mut self, image: &'a [u8]) -> Result<ZoneStream<'a>> {
        let [b0, b1, b2, b3, b4, b5, b6] = &mut self.blocks;
        let blocks: [&mut [u8]; MAX_XFILE_COUNT] = [
            b0.as_mut_slice(),
            b1.as_mut_slice(),
            b2.as_mut_slice(),
            b3.as_mut_slice(),
            b4.as_mut_slice(),
            b5.as_mut_slice(),
            b6.as_mut_slice(),
        ];
        ZoneStream::new(image, blocks, &mut self.insert_map)
    }
}

fn block_capacity(header: &ZoneHeader, block: usize) -> usize {
    let logical = header.block_size[block] as usize;
    if logical == 0 || block != XFILE_BLOCK_RUNTIME {
        return logical;
    }

    logical.saturating_add(0x0f).next_multiple_of(0x1000)
}
