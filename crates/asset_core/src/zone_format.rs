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
    /// The compiled script modules a decoded zone carries.
    pub scripts: fn(&[u8]) -> ZoneScripts,
    /// The zones, beside a map's own, whose script modules the map runs, in
    /// load order: a later zone's module replaces an earlier one's.
    pub script_zones: fn(&str) -> Vec<String>,
    /// The map entities a decoded zone carries (a map's own zone).
    pub entities: fn(&[u8]) -> ZoneEntities,
}

#[derive(Clone, Debug, Default)]
pub struct ZoneEntities {
    /// The entity text (`{ "classname" "worldspawn" … }`).
    pub text: Option<String>,
    pub report: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ZoneScripts {
    /// Module name (`scripts/…/x.gsc`) and its compiled bytes.
    pub modules: Vec<(String, Vec<u8>)>,
    pub report: Vec<String>,
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
