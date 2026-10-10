mod content;
mod envelope;

pub use content::{AssetList, ContentError, SCRIPT_ASSET_TYPE, ScriptAsset, asset_list, scripts};

pub use envelope::{
    BLOCK_SIZES, BLOCKS_OFFSET, Block, BlockError, Blocks, FileHeader, FileHeaderError, MAGIC,
    ZONE_NAME_LEN, ZONE_VERSION_PC, parse_file_header,
};

pub const ZONE_FORMAT: asset_core::ZoneFormat = asset_core::ZoneFormat {
    game: asset_core::FamilyId::T7,
    magic: MAGIC,
    version: ZONE_VERSION_PC,
    decode: content,
    scripts: zone_scripts,
    script_zones,
};

fn script_zones(map: &str) -> Vec<String> {
    let mut zones = vec!["core_patch".to_owned()];
    if map.starts_with("zm_") {
        zones.extend(["zm_levelcommon".to_owned(), "zm_patch".to_owned()]);
    }
    zones.extend([map.to_owned(), format!("{map}_patch")]);
    zones
}

fn zone_scripts(content: &[u8]) -> asset_core::ZoneScripts {
    let mut out = asset_core::ZoneScripts::default();
    let list = match asset_list(content) {
        Ok(list) => list,
        Err(error) => {
            out.report.push(format!("t7 asset list: {error:?}"));
            return out;
        }
    };
    let found = match scripts(content, &list) {
        Ok(found) => found,
        Err(error) => {
            out.report.push(format!("t7 scripts: {error:?}"));
            return out;
        }
    };
    let (mut exports, mut imports, mut refused) = (0, 0, Vec::new());
    for script in &found {
        match gsc_t7::Module::parse(script.bytes) {
            Ok(module) if module.name == script.name => {
                exports += module.exports.len();
                imports += module.imports.len();
            }
            Ok(module) => refused.push(format!("{}: names itself {}", script.name, module.name)),
            Err(error) => refused.push(format!("{}: {error:?}", script.name)),
        }
        out.modules
            .push((script.name.to_owned(), script.bytes.to_vec()));
    }
    out.report.push(format!(
        "t7 scripts: {} assets, {} modules, {} refused, {exports} exports, {imports} imports",
        list.asset_types.len(),
        found.len(),
        refused.len()
    ));
    out.report.extend(refused.into_iter().take(8));
    out
}

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
