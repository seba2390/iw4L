const LIST_LEN: usize = 48;
const ASSET_ENTRY_LEN: usize = 16;
const INLINE: i64 = -1;

/// The asset type of a compiled script module.
pub const SCRIPT_ASSET_TYPE: u32 = 54;

const SCRIPT_MAGIC: &[u8] = b"\x80GSC\r\n\0";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContentError {
    Truncated {
        what: &'static str,
        at: usize,
    },
    /// The asset list's strings or assets do not follow it in the stream.
    NotInline {
        what: &'static str,
    },
    BadString {
        at: usize,
    },
    /// Script assets found in the stream do not match the asset list's count.
    ScriptCount {
        listed: usize,
        found: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetList {
    /// Index 0 is the null string.
    pub strings: Vec<Option<String>>,
    pub asset_types: Vec<u32>,
    /// Where the first asset's data begins.
    pub data_at: usize,
}

pub fn asset_list(content: &[u8]) -> Result<AssetList, ContentError> {
    let string_count = u32_at(content, 0, "asset list")? as usize;
    let strings_ptr = i64_at(content, 8, "asset list")?;
    let asset_count = u32_at(content, 32, "asset list")? as usize;
    let assets_ptr = i64_at(content, 40, "asset list")?;
    let mut at = LIST_LEN;
    let mut strings = Vec::with_capacity(string_count);
    if string_count > 0 {
        if strings_ptr != INLINE {
            return Err(ContentError::NotInline { what: "strings" });
        }
        let pointers = at;
        at += string_count * 8;
        for index in 0..string_count {
            match i64_at(content, pointers + index * 8, "string pointer")? {
                0 => strings.push(None),
                INLINE => {
                    let tail = content.get(at..).ok_or(ContentError::BadString { at })?;
                    let end = tail
                        .iter()
                        .position(|&byte| byte == 0)
                        .ok_or(ContentError::BadString { at })?;
                    strings.push(Some(String::from_utf8_lossy(&tail[..end]).into_owned()));
                    at += end + 1;
                }
                _ => return Err(ContentError::BadString { at }),
            }
        }
    }
    let mut asset_types = Vec::with_capacity(asset_count);
    if asset_count > 0 {
        if assets_ptr != INLINE {
            return Err(ContentError::NotInline { what: "assets" });
        }
        for index in 0..asset_count {
            asset_types.push(u32_at(
                content,
                at + index * ASSET_ENTRY_LEN,
                "asset entry",
            )?);
        }
        at += asset_count * ASSET_ENTRY_LEN;
    }
    Ok(AssetList {
        strings,
        asset_types,
        data_at: at,
    })
}

/// A compiled script module and the name it is loaded by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScriptAsset<'a> {
    pub name: &'a str,
    pub bytes: &'a [u8],
}

/// The zone's compiled script modules. Each sits in the stream as its header
/// (inline name, length, inline buffer), its name, then its buffer; they are
/// found by that shape and counted against the asset list.
pub fn scripts<'a>(
    content: &'a [u8],
    list: &AssetList,
) -> Result<Vec<ScriptAsset<'a>>, ContentError> {
    let listed = list
        .asset_types
        .iter()
        .filter(|&&ty| ty == SCRIPT_ASSET_TYPE)
        .count();
    let mut found = Vec::with_capacity(listed);
    let mut from = list.data_at;
    while let Some(offset) = find(&content[from..], SCRIPT_MAGIC) {
        let buffer = from + offset;
        from = buffer + SCRIPT_MAGIC.len();
        if let Some(script) = script_at(content, buffer) {
            found.push(script);
            from = buffer + script.bytes.len();
        }
    }
    if found.len() != listed {
        return Err(ContentError::ScriptCount {
            listed,
            found: found.len(),
        });
    }
    Ok(found)
}

fn script_at(content: &[u8], buffer: usize) -> Option<ScriptAsset<'_>> {
    let name_end = buffer.checked_sub(1)?;
    if content[name_end] != 0 {
        return None;
    }
    let name_start = content[..name_end]
        .iter()
        .rposition(|&byte| byte == 0 || byte == 0xff)?
        + 1;
    let header = name_start.checked_sub(24)?;
    if i64_at(content, header, "").ok()? != INLINE
        || i64_at(content, header + 16, "").ok()? != INLINE
    {
        return None;
    }
    let len = u32_at(content, header + 8, "").ok()? as usize;
    let name = core::str::from_utf8(&content[name_start..name_end]).ok()?;
    if ![".gsc", ".csc", ".gsh"]
        .iter()
        .any(|extension| name.ends_with(extension))
    {
        return None;
    }
    Some(ScriptAsset {
        name,
        bytes: content.get(buffer..buffer + len)?,
    })
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn u32_at(bytes: &[u8], at: usize, what: &'static str) -> Result<u32, ContentError> {
    bytes
        .get(at..at + 4)
        .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
        .ok_or(ContentError::Truncated { what, at })
}

fn i64_at(bytes: &[u8], at: usize, what: &'static str) -> Result<i64, ContentError> {
    bytes
        .get(at..at + 8)
        .map(|word| i64::from_le_bytes(word.try_into().unwrap()))
        .ok_or(ContentError::Truncated { what, at })
}
