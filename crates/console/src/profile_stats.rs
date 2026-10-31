use std::{fs, path::PathBuf};

use bevy::prelude::*;
use sim::ProfileStats;

/// Where the local profile's stats buffers live (a game that keeps them:
/// Black Ops multiplayer), and the match's last rewrite saved.
#[derive(Resource, Default)]
pub(crate) struct ProfileStatsPersistence {
    path: Option<PathBuf>,
    saved: Option<Vec<u8>>,
}

pub(crate) fn load(
    identity: Res<ui::LaunchIdentity>,
    role: Res<frame::RuntimeRole>,
    mut stats: ResMut<ProfileStats>,
    mut persistence: ResMut<ProfileStatsPersistence>,
) {
    if *role != frame::RuntimeRole::Listen {
        return;
    }
    let path = std::env::var_os("IW4L_PROFILE_STATS_PATH")
        .map(PathBuf::from)
        .or_else(|| {
            crate::user_settings::settings_path(&identity.artifacts)
                .map(|path| path.with_file_name("profile_stats.dat"))
        });
    let Some(path) = path else {
        warn!("no HOME or XDG_CONFIG_HOME; profile stats are session-only");
        return;
    };
    match fs::read(&path) {
        Ok(bytes) => stats.bytes = Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => warn!("could not read {}: {error}", path.display()),
    }
    persistence.saved = stats.bytes.clone();
    persistence.path = Some(path);
}

/// Saves the stats a match rebuilt for the profile.
pub(crate) fn save(
    role: Res<frame::RuntimeRole>,
    authority: Option<Res<net::AuthorityWorld>>,
    mut stats: ResMut<ProfileStats>,
    mut persistence: ResMut<ProfileStatsPersistence>,
) {
    if *role != frame::RuntimeRole::Listen {
        return;
    }
    let Some(authority) = authority else {
        return;
    };
    let current = authority.0.profile_stats();
    if current.bytes.is_none() || current.bytes == persistence.saved {
        return;
    }
    stats.bytes = current.bytes.clone();
    let (Some(path), Some(bytes)) = (persistence.path.as_ref(), stats.bytes.as_ref()) else {
        return;
    };
    let result = write_stats(path, bytes);
    match result {
        Ok(()) => persistence.saved = stats.bytes.clone(),
        Err(error) => warn!("could not save {}: {error}", path.display()),
    }
}

fn write_stats(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("dat.tmp");
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, path)
}
