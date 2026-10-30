use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum T6ContentMode {
    Multiplayer,
    Zombies,
}

impl T6ContentMode {
    pub fn for_path(path: &Path) -> Self {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let stem = stem.to_ascii_lowercase();
        if stem.starts_with("zm_") || stem.ends_with("_zm") || stem.contains("_zm_") {
            Self::Zombies
        } else {
            Self::Multiplayer
        }
    }

    pub fn common(self) -> &'static str {
        match self {
            Self::Multiplayer => "common_mp",
            Self::Zombies => "common_zm",
        }
    }

    pub fn patch(self) -> &'static str {
        match self {
            Self::Multiplayer => "patch_mp",
            Self::Zombies => "patch_zm",
        }
    }

    pub fn startup(self) -> [&'static str; 3] {
        match self {
            Self::Multiplayer => ["code_post_gfx_mp", "localized_code_post_gfx_mp", "patch_mp"],
            Self::Zombies => ["code_post_gfx_zm", "localized_code_post_gfx_zm", "patch_zm"],
        }
    }

    pub fn localized(self) -> [&'static str; 3] {
        match self {
            Self::Multiplayer => ["patch_mp", "ui_mp", "code_post_gfx_mp"],
            Self::Zombies => ["patch_zm", "ui_zm", "code_post_gfx_zm"],
        }
    }

    pub fn icons(self) -> &'static [&'static str] {
        match self {
            Self::Multiplayer => &[
                "patch_ui_mp.ff",
                "patch_mp.ff",
                "ui_mp.ff",
                "common_mp.ff",
                "code_post_gfx_mp.ff",
                "code_pre_gfx_mp.ff",
            ],
            Self::Zombies => &[
                "patch_ui_zm.ff",
                "patch_zm.ff",
                "ui_zm.ff",
                "common_zm.ff",
                "code_post_gfx_zm.ff",
                "code_pre_gfx_zm.ff",
                "patch_ui_mp.ff",
                "patch_mp.ff",
            ],
        }
    }

    /// The installed `*_load_zm` zones beside `common`: Black Ops 2 keeps
    /// each zombies map pack's loading screens there (`dlczm0_load_zm`
    /// carries Nuketown's, `dlc3_load_zm` Buried's).
    pub fn load_zones(self, common: &Path) -> Vec<std::path::PathBuf> {
        if self != Self::Zombies {
            return Vec::new();
        }
        let Some(Ok(entries)) = common.parent().map(std::fs::read_dir) else {
            return Vec::new();
        };
        let mut zones: Vec<_> = entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.to_ascii_lowercase().ends_with("_load_zm.ff"))
            })
            .collect();
        zones.sort();
        zones
    }

    /// Mounted DLC package names (`dlc0`, `dlczm0`, `dlc1`, `dlc2`, `dlc3`,
    /// `dlc4`, `dlc5`, `dlc0dd`, `seasonpass`, `textures`) whose load zone
    /// (`<pack>_load_zm.ff` or `<pack>_load_mp.ff`) exists in the zone folder.
    pub fn dlc_packages(zone_file: &Path) -> Vec<String> {
        let Some(Ok(entries)) = zone_file.parent().map(std::fs::read_dir) else {
            return Vec::new();
        };
        let mut packs: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().to_str().map(str::to_ascii_lowercase))
            .filter_map(|name| {
                name.strip_suffix("_load_zm.ff")
                    .or_else(|| name.strip_suffix("_load_mp.ff"))
                    .map(str::to_owned)
            })
            .collect();
        packs.sort();
        packs.dedup();
        packs
    }

    pub fn supplements(self, path: &Path) -> Vec<String> {
        let mut zones = vec![self.patch().to_owned()];
        if self == Self::Zombies {
            let map = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if map.starts_with("zm_") {
                zones.extend(crate::mode_zone(path));
                zones.push(format!("{map}_patch"));
            }
        }
        zones
    }

    /// Patch zones the game downloads at launch and keeps only in memory, as
    /// the owner captured them into `<artifacts>/captured/` or into the
    /// user's IW4L folder (`~/.config/iw4l/captured/`), which every checkout
    /// and launch shares.
    pub fn captured_patches(self) -> Vec<std::path::PathBuf> {
        captured_patch_files(self.capture_suffix())
    }

    /// Compiled modules captured one by one, which replace the zones' and
    /// the captured patches' (`captured_script_modules`).
    pub fn captured_modules(self) -> Vec<(String, Vec<u8>)> {
        captured_script_modules(self.capture_suffix())
    }

    fn capture_suffix(self) -> &'static str {
        match self {
            Self::Multiplayer => "_mp",
            Self::Zombies => "_zm",
        }
    }
}

/// Compiled script modules the owner dumped one by one from the game's memory
/// into `<capture folder>/scripts<suffix>/` (`maps/mp/gametypes/_globallogic.gsc`
/// under it), by module path; the first folder's copy of a module wins.
pub fn captured_script_modules(suffix: &str) -> Vec<(String, Vec<u8>)> {
    fn walk(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<(String, Vec<u8>)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for path in entries.filter_map(|entry| entry.ok().map(|entry| entry.path())) {
            if path.is_dir() {
                walk(root, &path, out);
                continue;
            }
            let script = path
                .extension()
                .is_some_and(|ext| ext == "gsc" || ext == "csc");
            if let (true, Ok(name), Ok(bytes)) =
                (script, path.strip_prefix(root), std::fs::read(&path))
            {
                let name = name.to_string_lossy().replace('\\', "/");
                if !out.iter().any(|(seen, _)| *seen == name) {
                    out.push((name, bytes));
                }
            }
        }
    }
    let mut modules = Vec::new();
    for folder in capture_folders() {
        let root = folder.join(format!("scripts{suffix}"));
        walk(&root, &root, &mut modules);
    }
    modules
}

/// The folders captured patch zones are read from: `<artifacts>/captured/`,
/// then the user's IW4L folder's (`~/.config/iw4l/captured/`).
pub fn capture_folders() -> Vec<std::path::PathBuf> {
    crate::discover::ensure_artifacts_dir()
        .ok()
        .into_iter()
        .chain(crate::discover::user_data_dir())
        .map(|dir| dir.join("captured"))
        .collect()
}

/// The captured downloaded patch zones (`ffotd_*<suffix>*.ff`) in the
/// capture folders, by file name.
pub fn captured_patch_files(suffix: &str) -> Vec<std::path::PathBuf> {
    let folders = capture_folders();
    let mut zones: Vec<std::path::PathBuf> = Vec::new();
    for folder in folders {
        let Ok(entries) = std::fs::read_dir(folder) else {
            continue;
        };
        for path in entries.filter_map(|entry| entry.ok().map(|entry| entry.path())) {
            let captured = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| stem.starts_with("ffotd_") && stem.contains(suffix))
                && path.extension().is_some_and(|ext| ext == "ff");
            if captured
                && !zones
                    .iter()
                    .any(|seen| seen.file_name() == path.file_name())
            {
                zones.push(path);
            }
        }
    }
    zones.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    zones
}
