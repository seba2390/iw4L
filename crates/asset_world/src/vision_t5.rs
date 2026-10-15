//! Black Ops vision files: their raw text by path. The game reads one only
//! when a vision channel loads it (`game_t5`).

use std::sync::Arc;

/// Vision files by lower-case path (`vision/<name>.vision`).
pub type T5VisionCatalog = std::collections::BTreeMap<String, Arc<str>>;

/// Keeps a `vision/*.vision` raw file's text in `catalog` under its
/// lower-case path; other raw files are left alone.
pub fn capture_t5_vision(
    catalog: &mut T5VisionCatalog,
    warnings: &mut Vec<String>,
    name: &str,
    data: &[u8],
    zlib_compressed: bool,
) {
    let key = name.replace('\\', "/").to_ascii_lowercase();
    if !key.starts_with("vision/") || !key.ends_with(".vision") {
        return;
    }
    let Some(text) = crate::decode_rawfile_text(data, zlib_compressed) else {
        warnings.push(format!("vision {key}: not text"));
        return;
    };
    catalog.insert(key, Arc::from(text.as_str()));
}
