mod decode;
mod names;
mod opcodes;
mod program;
mod translate;

pub use decode::{DecodeError, Instruction, Operand, decode_function};
pub use opcodes::{KNOWN_VALUES, Layout, Opcode, opcode};
pub use program::{Built, Source, build, name_of};
pub use translate::{Linker, Site, Translator};

pub const MAGIC: &[u8; 7] = b"\x80GSC\r\n\0";

pub const VERSION: u8 = 0x1c;

const HEADER_LEN: usize = 0x48;
const EXPORT_LEN: usize = 20;
const IMPORT_LEN: usize = 12;
const STRING_LEN: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleError {
    TooShort {
        len: usize,
    },
    BadMagic,
    BadVersion {
        got: u8,
    },
    /// A table or string runs past the end of the module.
    OutOfBounds {
        what: &'static str,
        at: usize,
    },
    /// The code segment does not end where the export table begins.
    CodeSegment {
        end: usize,
        exports: usize,
    },
}

/// A function the module defines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Export {
    pub checksum: u32,
    /// Offset of the function's first opcode.
    pub code: u32,
    pub name: u32,
    pub namespace: u32,
    pub params: u8,
    pub flags: u8,
}

/// A function the module calls by name, with the code offsets of each call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    pub name: u32,
    pub namespace: u32,
    pub params: u8,
    pub flags: u8,
    pub refs: Vec<u32>,
}

/// A string the module's code references, with the code offsets that use it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringRef {
    pub text: String,
    pub kind: u8,
    pub refs: Vec<u32>,
}

/// An animation tree the module's code names, with the code offsets that use
/// it: as a tree (`#animtree`) and as each named animation (`%name`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnimTreeRef {
    pub tree: String,
    pub tree_refs: Vec<u32>,
    pub animations: Vec<(String, u32)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub checksum: u32,
    pub name: String,
    pub includes: Vec<String>,
    /// Byte range of the code segment.
    pub code: core::ops::Range<usize>,
    pub exports: Vec<Export>,
    pub imports: Vec<Import>,
    pub strings: Vec<StringRef>,
    pub animtrees: Vec<AnimTreeRef>,
}

impl Module {
    pub fn parse(bytes: &[u8]) -> Result<Self, ModuleError> {
        if bytes.len() < HEADER_LEN {
            return Err(ModuleError::TooShort { len: bytes.len() });
        }
        if &bytes[..MAGIC.len()] != MAGIC {
            return Err(ModuleError::BadMagic);
        }
        if bytes[7] != VERSION {
            return Err(ModuleError::BadVersion { got: bytes[7] });
        }
        let word = |at: usize| u32_at(bytes, at).map(|value| value as usize);
        let half = |at: usize| usize::from(u16::from_le_bytes([bytes[at], bytes[at + 1]]));
        let checksum = u32_at(bytes, 0x08)?;
        let includes_at = word(0x0C)?;
        let code_at = word(0x14)?;
        let strings_at = word(0x18)?;
        let exports_at = word(0x20)?;
        let imports_at = word(0x24)?;
        let code_len = word(0x30)?;
        let name_at = word(0x34)?;
        let string_count = half(0x38);
        let export_count = half(0x3A);
        let import_count = half(0x3C);
        let include_count = usize::from(bytes[0x44]);
        let animtrees_at = word(0x10)?;
        let animtree_count = usize::from(bytes[0x45]);

        if code_at + code_len != exports_at {
            return Err(ModuleError::CodeSegment {
                end: code_at + code_len,
                exports: exports_at,
            });
        }
        let name = c_string(bytes, name_at, "name")?;
        let includes = (0..include_count)
            .map(|index| c_string(bytes, word(includes_at + index * 4)?, "include"))
            .collect::<Result<_, _>>()?;
        let exports = (0..export_count)
            .map(|index| {
                let at = exports_at + index * EXPORT_LEN;
                Ok(Export {
                    checksum: u32_at(bytes, at)?,
                    code: u32_at(bytes, at + 4)?,
                    name: u32_at(bytes, at + 8)?,
                    namespace: u32_at(bytes, at + 12)?,
                    params: byte_at(bytes, at + 16)?,
                    flags: byte_at(bytes, at + 17)?,
                })
            })
            .collect::<Result<_, _>>()?;
        let mut imports = Vec::with_capacity(import_count);
        let mut at = imports_at;
        for _ in 0..import_count {
            let count = usize::from(u16::from_le_bytes([
                byte_at(bytes, at + 8)?,
                byte_at(bytes, at + 9)?,
            ]));
            imports.push(Import {
                name: u32_at(bytes, at)?,
                namespace: u32_at(bytes, at + 4)?,
                params: byte_at(bytes, at + 10)?,
                flags: byte_at(bytes, at + 11)?,
                refs: u32_list(bytes, at + IMPORT_LEN, count)?,
            });
            at += IMPORT_LEN + 4 * count;
        }
        let mut strings = Vec::with_capacity(string_count);
        let mut at = strings_at;
        for _ in 0..string_count {
            let count = usize::from(byte_at(bytes, at + 4)?);
            strings.push(StringRef {
                text: c_string(bytes, word(at)?, "string")?,
                kind: byte_at(bytes, at + 5)?,
                refs: u32_list(bytes, at + STRING_LEN, count)?,
            });
            at += STRING_LEN + 4 * count;
        }
        // Each tree: its name, a count of tree references and of animation
        // references, the tree references' code offsets, then per animation
        // its name and its code offset (both 64-bit).
        let mut animtrees = Vec::with_capacity(animtree_count);
        let mut at = animtrees_at;
        for _ in 0..animtree_count {
            let half = |at: usize| -> Result<usize, ModuleError> {
                Ok(usize::from(u16::from_le_bytes([
                    byte_at(bytes, at)?,
                    byte_at(bytes, at + 1)?,
                ])))
            };
            let (tree_count, animation_count) = (half(at + 4)?, half(at + 6)?);
            let tree = c_string(bytes, word(at)?, "animtree")?;
            let tree_refs = u32_list(bytes, at + 8, tree_count)?;
            at += 8 + 4 * tree_count;
            let animations = (0..animation_count)
                .map(|index| {
                    let entry = at + index * 16;
                    Ok((
                        c_string(bytes, word(entry)?, "animation")?,
                        word(entry + 8)? as u32,
                    ))
                })
                .collect::<Result<_, ModuleError>>()?;
            at += 16 * animation_count;
            animtrees.push(AnimTreeRef {
                tree,
                tree_refs,
                animations,
            });
        }
        Ok(Self {
            checksum,
            name,
            includes,
            code: code_at..exports_at,
            exports,
            imports,
            strings,
            animtrees,
        })
    }
}

impl Module {
    /// Each export with its decoded code, in code order. A function's code
    /// runs to the next function or the end of the code segment.
    pub fn functions(&self, bytes: &[u8]) -> Vec<(Export, Result<Vec<Instruction>, DecodeError>)> {
        let mut exports = self.exports.clone();
        exports.sort_by_key(|export| export.code);
        let ends: Vec<usize> = exports
            .iter()
            .skip(1)
            .map(|export| export.code as usize)
            .chain(core::iter::once(self.code.end))
            .collect();
        exports
            .into_iter()
            .zip(ends)
            .map(|(export, end)| (export, decode_function(bytes, export.code as usize, end)))
            .collect()
    }
}

fn byte_at(bytes: &[u8], at: usize) -> Result<u8, ModuleError> {
    bytes
        .get(at)
        .copied()
        .ok_or(ModuleError::OutOfBounds { what: "byte", at })
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32, ModuleError> {
    bytes
        .get(at..at + 4)
        .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
        .ok_or(ModuleError::OutOfBounds { what: "word", at })
}

fn u32_list(bytes: &[u8], at: usize, count: usize) -> Result<Vec<u32>, ModuleError> {
    (0..count)
        .map(|index| u32_at(bytes, at + index * 4))
        .collect()
}

fn c_string(bytes: &[u8], at: usize, what: &'static str) -> Result<String, ModuleError> {
    let tail = bytes
        .get(at..)
        .ok_or(ModuleError::OutOfBounds { what, at })?;
    let end = tail
        .iter()
        .position(|&byte| byte == 0)
        .ok_or(ModuleError::OutOfBounds { what, at })?;
    Ok(String::from_utf8_lossy(&tail[..end]).into_owned())
}
