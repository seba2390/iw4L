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

    pub fn supplements(self, path: &Path) -> Vec<String> {
        let mut zones = vec![self.patch().to_owned()];
        if self == Self::Zombies {
            let map = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if map.starts_with("zm_") {
                zones.push(format!("so_zclassic_{map}"));
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
        let suffix = match self {
            Self::Multiplayer => "_mp",
            Self::Zombies => "_zm",
        };
        let folders = crate::discover::ensure_artifacts_dir()
            .ok()
            .into_iter()
            .chain(crate::discover::user_data_dir())
            .map(|dir| dir.join("captured"));
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
}
