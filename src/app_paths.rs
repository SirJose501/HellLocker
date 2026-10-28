use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

#[derive(Clone)]
pub struct AppPaths {
    pub config: PathBuf,
    pub presets: PathBuf,
    pub log: PathBuf,
    pub last_failure: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> Result<Self> {
        crate::platform::app_paths()
    }

    /// Every file HellLocker keeps lives in `data`.
    pub fn in_dir(data: &Path) -> Self {
        Self {
            config: data.join("config.toml"),
            presets: data.join("presets.json"),
            log: data.join("app.log"),
            last_failure: data.join("last_failure.zip"),
        }
    }
}

/// HellLocker: the name of the folder older builds kept next to the exe; after its
/// contents are copied to the user's data folder it is renamed to `MOVED_DATA_DIR`.
pub const LEGACY_DATA_DIR: &str = "data";
pub const MOVED_DATA_DIR: &str = "data (moved to AppData)";
/// HellLocker: a file of this name next to the exe keeps everything in `data` there
/// instead (a portable install, e.g. on a USB stick).
pub const PORTABLE_MARKER: &str = "portable.txt";

/// HellLocker: where the data lives for an exe in `exe_dir`, given the user's
/// roaming application data folder (`%APPDATA%`). Without one, or for a portable
/// install, it stays next to the exe as before.
pub fn data_dir(exe_dir: &Path, appdata: Option<&Path>) -> PathBuf {
    match appdata {
        Some(appdata) if !exe_dir.join(PORTABLE_MARKER).exists() => appdata.join("HellLocker"),
        _ => exe_dir.join(LEGACY_DATA_DIR),
    }
}

/// HellLocker: copy the presets and settings of an older build (kept in `data` next
/// to the exe) into the new data folder, once, when the new folder has no settings
/// yet. The old folder is then renamed so it is clear it is no longer used. Returns
/// whether anything was moved.
pub fn migrate_legacy_data(exe_dir: &Path, data: &Path) -> Result<bool> {
    let legacy = exe_dir.join(LEGACY_DATA_DIR);
    if legacy == data || !legacy.join("config.toml").is_file() || data.join("config.toml").exists()
    {
        return Ok(false);
    }
    copy_dir(&legacy, data)
        .with_context(|| format!("failed to copy {} to {}", legacy.display(), data.display()))?;
    // Not fatal: the copy is complete, and the new folder now wins on every start.
    let _ = fs::rename(&legacy, exe_dir.join(MOVED_DATA_DIR));
    Ok(true)
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hl-paths-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn data_goes_to_appdata_unless_portable() {
        let root = temp("dir");
        let exe = root.join("HellLocker");
        let appdata = root.join("AppData");
        fs::create_dir_all(&exe).unwrap();
        assert_eq!(data_dir(&exe, Some(&appdata)), appdata.join("HellLocker"));
        assert_eq!(data_dir(&exe, None), exe.join("data"));
        fs::write(exe.join(PORTABLE_MARKER), "").unwrap();
        assert_eq!(data_dir(&exe, Some(&appdata)), exe.join("data"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn old_data_is_copied_once_and_the_old_folder_renamed() {
        let root = temp("migrate");
        let exe = root.join("HellLocker");
        let data = root.join("AppData").join("HellLocker");
        let picture = Path::new("local_templates").join("library").join("1");
        fs::create_dir_all(exe.join("data").join(&picture)).unwrap();
        fs::write(exe.join("data").join("config.toml"), "old").unwrap();
        fs::write(exe.join("data").join(&picture).join("1-name.png"), "png").unwrap();
        assert!(migrate_legacy_data(&exe, &data).unwrap());
        assert_eq!(fs::read_to_string(data.join("config.toml")).unwrap(), "old");
        assert!(data.join(&picture).join("1-name.png").is_file());
        assert!(!exe.join("data").exists());
        assert!(exe.join(MOVED_DATA_DIR).join("config.toml").is_file());
        // Settings already in the new folder are never overwritten.
        fs::create_dir_all(exe.join("data")).unwrap();
        fs::write(exe.join("data").join("config.toml"), "older").unwrap();
        assert!(!migrate_legacy_data(&exe, &data).unwrap());
        assert_eq!(fs::read_to_string(data.join("config.toml")).unwrap(), "old");
        let _ = fs::remove_dir_all(&root);
    }
}
