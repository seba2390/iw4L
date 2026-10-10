//! A compiled module is a header, then its tables laid end to end; each
//! table ends exactly where the next one (by offset) begins.

pub const MAGIC: &[u8; 7] = b"\x80GSC\r\n\0";

pub const VERSION: u8 = 0x06;

const HEADER_LEN: usize = 0x40;
const EXPORT_LEN: usize = 12;
const IMPORT_LEN: usize = 8;
const STRING_LEN: usize = 4;
const ANIMTREE_LEN: usize = 8;
const ANIMATION_LEN: usize = 8;
const FIXUP_LEN: usize = 8;
const PROFILE_LEN: usize = 8;

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
    /// A table does not end where the next table begins.
    Layout {
        table: Table,
        end: usize,
        next: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Table {
    Includes,
    AnimTrees,
    Code,
    Strings,
    Exports,
    Imports,
    Fixups,
    Profiles,
}

/// A function the module defines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Export {
    pub checksum: u32,
    /// Offset of the function's first opcode.
    pub code: u32,
    pub name: String,
    pub params: u8,
    pub flags: u8,
}

/// A function the module calls by name, with the code offsets of each call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    pub name: String,
    /// The callee's file (`maps/mp/zombies/_zm_utility`); empty for a builtin
    /// or a function of this module.
    pub namespace: String,
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
    pub fixups: usize,
    pub profiles: usize,
    pub flags: u8,
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
        let checksum = u32_at(bytes, 0x08)?;
        let includes_at = word(0x0C)?;
        let animtrees_at = word(0x10)?;
        let code_at = word(0x14)?;
        let strings_at = word(0x18)?;
        let exports_at = word(0x1C)?;
        let imports_at = word(0x20)?;
        let fixups_at = word(0x24)?;
        let profiles_at = word(0x28)?;
        let code_len = word(0x2C)?;
        let name_at = half_at(bytes, 0x30)?;
        let string_count = half_at(bytes, 0x32)?;
        let export_count = half_at(bytes, 0x34)?;
        let import_count = half_at(bytes, 0x36)?;
        let fixup_count = half_at(bytes, 0x38)?;
        let profile_count = half_at(bytes, 0x3A)?;
        let include_count = usize::from(bytes[0x3C]);
        let animtree_count = usize::from(bytes[0x3D]);
        let flags = bytes[0x3E];

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
                    name: c_string(bytes, half_at(bytes, at + 8)?, "export")?,
                    params: byte_at(bytes, at + 10)?,
                    flags: byte_at(bytes, at + 11)?,
                })
            })
            .collect::<Result<_, ModuleError>>()?;
        let mut imports = Vec::with_capacity(import_count);
        let mut at = imports_at;
        for _ in 0..import_count {
            let count = half_at(bytes, at + 4)?;
            imports.push(Import {
                name: c_string(bytes, half_at(bytes, at)?, "import")?,
                namespace: c_string(bytes, half_at(bytes, at + 2)?, "import namespace")?,
                params: byte_at(bytes, at + 6)?,
                flags: byte_at(bytes, at + 7)?,
                refs: u32_list(bytes, at + IMPORT_LEN, count)?,
            });
            at += IMPORT_LEN + 4 * count;
        }
        let imports_end = at;
        let mut strings = Vec::with_capacity(string_count);
        let mut at = strings_at;
        for _ in 0..string_count {
            let count = usize::from(byte_at(bytes, at + 2)?);
            strings.push(StringRef {
                text: c_string(bytes, half_at(bytes, at)?, "string")?,
                kind: byte_at(bytes, at + 3)?,
                refs: u32_list(bytes, at + STRING_LEN, count)?,
            });
            at += STRING_LEN + 4 * count;
        }
        let strings_end = at;
        // Each tree: its name, a count of tree references and of animation
        // references, the tree references' code offsets, then per animation
        // its name and its code offset.
        let mut animtrees = Vec::with_capacity(animtree_count);
        let mut at = animtrees_at;
        for _ in 0..animtree_count {
            let (tree_count, animation_count) = (half_at(bytes, at + 2)?, half_at(bytes, at + 4)?);
            let tree = c_string(bytes, half_at(bytes, at)?, "animtree")?;
            let tree_refs = u32_list(bytes, at + ANIMTREE_LEN, tree_count)?;
            at += ANIMTREE_LEN + 4 * tree_count;
            let animations = (0..animation_count)
                .map(|index| {
                    let entry = at + index * ANIMATION_LEN;
                    Ok((
                        c_string(bytes, word(entry)?, "animation")?,
                        u32_at(bytes, entry + 4)?,
                    ))
                })
                .collect::<Result<_, ModuleError>>()?;
            at += ANIMATION_LEN * animation_count;
            animtrees.push(AnimTreeRef {
                tree,
                tree_refs,
                animations,
            });
        }
        let animtrees_end = at;

        let tables = [
            (
                Table::Includes,
                includes_at,
                includes_at + 4 * include_count,
            ),
            (Table::AnimTrees, animtrees_at, animtrees_end),
            (Table::Code, code_at, code_at + code_len),
            (Table::Strings, strings_at, strings_end),
            (
                Table::Exports,
                exports_at,
                exports_at + EXPORT_LEN * export_count,
            ),
            (Table::Imports, imports_at, imports_end),
            (
                Table::Fixups,
                fixups_at,
                fixups_at + FIXUP_LEN * fixup_count,
            ),
            (
                Table::Profiles,
                profiles_at,
                profiles_at + PROFILE_LEN * profile_count,
            ),
        ];
        for &(table, start, end) in &tables {
            if end == start {
                continue;
            }
            let next = tables
                .iter()
                .map(|&(_, other, _)| other)
                .filter(|&other| other > start)
                .min()
                .unwrap_or(bytes.len())
                .min(bytes.len());
            if end != next {
                return Err(ModuleError::Layout { table, end, next });
            }
        }
        Ok(Self {
            checksum,
            name,
            includes,
            code: code_at..code_at + code_len,
            exports,
            imports,
            strings,
            animtrees,
            fixups: fixup_count,
            profiles: profile_count,
            flags,
        })
    }
}

fn byte_at(bytes: &[u8], at: usize) -> Result<u8, ModuleError> {
    bytes
        .get(at)
        .copied()
        .ok_or(ModuleError::OutOfBounds { what: "byte", at })
}

fn half_at(bytes: &[u8], at: usize) -> Result<usize, ModuleError> {
    bytes
        .get(at..at + 2)
        .map(|half| usize::from(u16::from_le_bytes([half[0], half[1]])))
        .ok_or(ModuleError::OutOfBounds { what: "half", at })
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
