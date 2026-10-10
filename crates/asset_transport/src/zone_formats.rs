use std::sync::RwLock;

use asset_core::ZoneFormat;

static FORMATS: RwLock<Vec<ZoneFormat>> = RwLock::new(Vec::new());

/// The zone formats of games whose zones the session's game registry knows
/// and this crate does not open itself.
pub fn register_zone_formats(formats: Vec<ZoneFormat>) {
    *FORMATS
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = formats;
}

pub(crate) fn registered() -> Vec<ZoneFormat> {
    FORMATS
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}
