use crate::FamilyId;

/// How a game's zone file is recognised and turned into its content stream,
/// for a game whose zones `asset_transport` does not open itself.
#[derive(Clone, Copy)]
pub struct ZoneFormat {
    pub game: FamilyId,
    /// The bytes the file starts with.
    pub magic: &'static [u8],
    /// The little-endian word at offset 8.
    pub version: u32,
    pub decode: fn(&[u8]) -> Result<Vec<u8>, String>,
}

impl ZoneFormat {
    pub fn recognises(&self, file: &[u8]) -> bool {
        file.starts_with(self.magic) && file.get(8..12) == Some(&self.version.to_le_bytes()[..])
    }
}

impl core::fmt::Debug for ZoneFormat {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ZoneFormat")
            .field("game", &self.game)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}
