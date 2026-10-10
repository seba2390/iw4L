pub const MAGIC: &[u8; 8] = b"TAff0000";

pub const ZONE_VERSION_PC: u32 = 0x251;

pub const ZONE_NAME_LEN: usize = 0x40;

pub const BLOCK_SIZES: usize = 12;

pub const BLOCKS_OFFSET: usize = 0x248;

const CONTENT_SIZE_AT: usize = 0x90;
const BLOCK_SIZES_AT: usize = 0x98;
const NAME_AT: usize = 0xF8;
const BLOCK_HEADER_LEN: usize = 16;

/// Blocks never cross a multiple of this offset; a block header that would is
/// written with an inflated length of 0, and the next block starts at the
/// boundary.
const SECTION_LEN: usize = 0x80_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileHeader {
    pub version: u32,
    /// Size of the content stream the blocks inflate to.
    pub content_size: u64,
    pub block_sizes: [u64; BLOCK_SIZES],
    name: [u8; ZONE_NAME_LEN],
    name_len: usize,
}

impl FileHeader {
    pub fn name(&self) -> &[u8] {
        &self.name[..self.name_len]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileHeaderError {
    TooShort { len: usize },
    BadMagic { got: [u8; 8] },
    BadVersion { got: u32 },
    EmptyName,
}

pub fn parse_file_header(bytes: &[u8]) -> Result<FileHeader, FileHeaderError> {
    if bytes.len() < BLOCKS_OFFSET {
        return Err(FileHeaderError::TooShort { len: bytes.len() });
    }
    let magic: [u8; 8] = bytes[0..8].try_into().unwrap();
    if &magic != MAGIC {
        return Err(FileHeaderError::BadMagic { got: magic });
    }
    let version = u32_at(bytes, 8);
    if version != ZONE_VERSION_PC {
        return Err(FileHeaderError::BadVersion { got: version });
    }
    let mut block_sizes = [0; BLOCK_SIZES];
    for (index, size) in block_sizes.iter_mut().enumerate() {
        *size = u64_at(bytes, BLOCK_SIZES_AT + index * 8);
    }
    let name: [u8; ZONE_NAME_LEN] = bytes[NAME_AT..NAME_AT + ZONE_NAME_LEN].try_into().unwrap();
    let name_len = name.iter().position(|&b| b == 0).unwrap_or(ZONE_NAME_LEN);
    if name_len == 0 {
        return Err(FileHeaderError::EmptyName);
    }
    Ok(FileHeader {
        version,
        content_size: u64_at(bytes, CONTENT_SIZE_AT),
        block_sizes,
        name,
        name_len,
    })
}

/// One compressed block: a zlib stream that inflates to `inflated_len` bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Block<'a> {
    pub at: usize,
    pub inflated_len: usize,
    pub zlib: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockError {
    /// The block header at this offset does not fit the file.
    Truncated { at: usize },
    /// The block header names a different offset than the one it sits at.
    Misplaced { at: usize, names: usize },
}

/// The blocks after the header, up to the all-zero block header that ends them.
pub struct Blocks<'a> {
    file: &'a [u8],
    at: usize,
    done: bool,
}

impl<'a> Blocks<'a> {
    pub fn new(file: &'a [u8]) -> Self {
        Self {
            file,
            at: BLOCKS_OFFSET,
            done: false,
        }
    }
}

impl<'a> Iterator for Blocks<'a> {
    type Item = Result<Block<'a>, BlockError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let at = self.at;
        if at + BLOCK_HEADER_LEN > self.file.len() {
            self.done = true;
            return Some(Err(BlockError::Truncated { at }));
        }
        let compressed = u32_at(self.file, at) as usize;
        let inflated_len = u32_at(self.file, at + 4) as usize;
        let stored = u32_at(self.file, at + 8) as usize;
        let names = u32_at(self.file, at + 12) as usize;
        if compressed == 0 && inflated_len == 0 {
            self.done = true;
            return None;
        }
        if names != at {
            self.done = true;
            return Some(Err(BlockError::Misplaced { at, names }));
        }
        if inflated_len == 0 {
            self.at = (at / SECTION_LEN + 1) * SECTION_LEN;
            return self.next();
        }
        let start = at + BLOCK_HEADER_LEN;
        let Some(zlib) = self.file.get(start..start + compressed) else {
            self.done = true;
            return Some(Err(BlockError::Truncated { at }));
        };
        self.at = start + stored.max(compressed);
        Some(Ok(Block {
            at,
            inflated_len,
            zlib,
        }))
    }
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}
