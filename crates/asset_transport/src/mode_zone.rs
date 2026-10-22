//! The start a zombies load runs — the gametype and start location the host
//! picked — and the zone that gametype adds to the map.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};

/// A zombies gametype and, for maps with several, its start location
/// (`ui_gametype`, `ui_zm_mapstartlocation`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ZombiesStart {
    pub gametype: String,
    pub location: Option<String>,
}

static SELECTED: RwLock<Option<ZombiesStart>> = RwLock::new(None);

/// The host's pick, for the loads that follow.
pub fn select_zombies_start(start: Option<ZombiesStart>) {
    *SELECTED
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = start;
}

/// The host's pick, else the launch's: `IW4L_GAMETYPE` and
/// `IW4L_ZM_LOCATION`.
pub fn zombies_start() -> Option<ZombiesStart> {
    if let Some(start) = SELECTED
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
    {
        return Some(start);
    }
    let gametype = std::env::var("IW4L_GAMETYPE")
        .ok()
        .map(|gametype| gametype.trim().to_ascii_lowercase())
        .filter(|gametype| !gametype.is_empty())?;
    let location = std::env::var("IW4L_ZM_LOCATION")
        .ok()
        .map(|location| location.trim().to_ascii_lowercase())
        .filter(|location| !location.is_empty());
    Some(ZombiesStart { gametype, location })
}

type ModeZoneKey = (PathBuf, String, String);

static RESOLVED: Mutex<Option<(ModeZoneKey, Option<String>)>> = Mutex::new(None);

/// The zone the selected gametype adds to the map at `path`, as the map's
/// game's tables name it; `None` when the game adds none or nothing is
/// selected.
pub fn mode_zone(path: &Path) -> Option<String> {
    let start = zombies_start()?;
    let map = path.file_stem()?.to_str()?.to_ascii_lowercase();
    let format = crate::registered_script_zone_format(crate::zone_game_for_path(path)?)?;
    let key = (
        path.with_file_name(format!("{}.ff", format.mode_tables_zone)),
        map,
        start.gametype,
    );
    let mut resolved = RESOLVED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((at, zone)) = resolved.as_ref()
        && *at == key
    {
        return zone.clone();
    }
    let zone = crate::zone::open_zone_shared(&key.0)
        .ok()
        .and_then(|tables| {
            let scripts = (format.scripts)(&tables.bytes);
            (format.mode_zone)(&scripts.tables, &key.1, &key.2)
        });
    diag::info!(
        Zone,
        "mode zone: {} {} -> {}",
        key.1,
        key.2,
        zone.as_deref().unwrap_or("none")
    );
    *resolved = Some((key, zone.clone()));
    zone
}
