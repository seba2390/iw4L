use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::{AssetType, ZoneLoad, walk::decode_ptr};

const NAME: usize = 0;
const LEN: usize = 4;
const BUFFER: usize = 8;

/// A compiled script module (`ScriptParseTree`) as the zone stores it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScriptAsset<'z> {
    pub name: &'z str,
    pub bytes: &'z [u8],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptError {
    Header { index: usize },
    Name { index: usize },
    Buffer { name: String, len: u32 },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ZoneScripts<'z> {
    /// Modules whose bytes this zone carries, in asset-list order.
    pub modules: Vec<ScriptAsset<'z>>,
    /// Modules this zone names but another zone carries (`,name`, no bytes).
    pub references: Vec<&'z str>,
    pub errors: Vec<ScriptError>,
}

pub fn scripts(load: &ZoneLoad) -> ZoneScripts<'_> {
    let mut out = ZoneScripts::default();
    for (index, asset) in load
        .assets
        .iter()
        .enumerate()
        .filter(|(_, asset)| asset.ty == AssetType::Script)
    {
        match script(load, &asset.header, index) {
            Ok(Entry::Module(module)) => out.modules.push(module),
            Ok(Entry::Reference(name)) => out.references.push(name),
            Err(error) => out.errors.push(error),
        }
    }
    out
}

enum Entry<'z> {
    Module(ScriptAsset<'z>),
    Reference(&'z str),
}

fn script<'z>(load: &'z ZoneLoad, header: &[u8], index: usize) -> Result<Entry<'z>, ScriptError> {
    let word = |at: usize| {
        header
            .get(at..at + 4)
            .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
            .ok_or(ScriptError::Header { index })
    };
    let name = decode_ptr(word(NAME)?)
        .and_then(|at| load.blocks.cstr(at).ok())
        .and_then(|bytes| core::str::from_utf8(bytes).ok())
        .ok_or(ScriptError::Name { index })?;
    let len = word(LEN)?;
    if let Some(name) = name.strip_prefix(',')
        && len == 0
    {
        return Ok(Entry::Reference(name));
    }
    let bytes = decode_ptr(word(BUFFER)?)
        .and_then(|at| load.blocks.bytes(at, len as usize).ok())
        .ok_or_else(|| ScriptError::Buffer {
            name: name.to_string(),
            len,
        })?;
    Ok(Entry::Module(ScriptAsset { name, bytes }))
}
