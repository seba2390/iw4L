mod envelope;

pub use envelope::{
    BLOCK_SIZES, BLOCKS_OFFSET, Block, BlockError, Blocks, FileHeader, FileHeaderError, MAGIC,
    ZONE_NAME_LEN, ZONE_VERSION_PC, parse_file_header,
};

pub const ZONE_FORMAT: asset_core::ZoneFormat = asset_core::ZoneFormat {
    game: asset_core::FamilyId::T7,
    magic: MAGIC,
    version: ZONE_VERSION_PC,
    decode: content,
};

/// The content stream of a zone file: its blocks, inflated and joined.
pub fn content(file: &[u8]) -> Result<Vec<u8>, String> {
    let header = parse_file_header(file).map_err(|error| format!("{error:?}"))?;
    let expected = usize::try_from(header.content_size)
        .map_err(|_| format!("content size {} does not fit", header.content_size))?;
    let mut out = Vec::with_capacity(expected);
    for block in Blocks::new(file) {
        let block = block.map_err(|error| format!("{error:?}"))?;
        let inflated =
            miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(block.zlib, block.inflated_len)
                .map_err(|error| format!("block at {:#x}: {error:?}", block.at))?;
        if inflated.len() != block.inflated_len {
            return Err(format!(
                "block at {:#x} inflated to {} bytes, its header says {}",
                block.at,
                inflated.len(),
                block.inflated_len
            ));
        }
        out.extend_from_slice(&inflated);
    }
    if out.len() != expected {
        return Err(format!(
            "blocks inflated to {} bytes, the header says {expected}",
            out.len()
        ));
    }
    Ok(out)
}
