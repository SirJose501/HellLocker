//! HellLocker: back up and restore presets and settings (Settings > Help).
//!
//! A backup is a zip of `config.toml`, `presets.json`, the `local_templates`
//! pictures and the player's own preset `icons`. Restoring checks the zip, unpacks it
//! next to the live data, and swaps it in the next time HellLocker starts (it restarts
//! itself right away), so the running app can never write over the restored files.
//! The data being replaced is saved as `backups/before-restore.zip` first.
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result, ensure};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

const CONFIG: &str = "config.toml";
const PRESETS: &str = "presets.json";
const TEMPLATES: &str = "local_templates";
/// HellLocker 1.9.5: the player's own preset icons.
const ICONS: &str = crate::preset_icons::CUSTOM_DIR;
/// Where a checked restore waits for the next start.
const PENDING: &str = "restore.pending";
/// The copy of the data a restore replaced.
pub const BACKUPS: &str = "backups";
const BEFORE_RESTORE: &str = "before-restore.zip";
/// More than any real set of presets; stops a broken or hostile zip filling the disk.
const MAX_UNPACKED: u64 = 1024 * 1024 * 1024;

static RESTART: AtomicBool = AtomicBool::new(false);

/// Ask for HellLocker to start again once it has closed (after a restore).
pub fn request_restart() {
    RESTART.store(true, Ordering::Relaxed);
}

pub fn restart_requested() -> bool {
    RESTART.load(Ordering::Relaxed)
}

/// A file name for a new backup, e.g. `HellLocker-backup-2026-10-27.zip`.
pub fn file_name() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() / 86_400)
        .unwrap_or_default() as i64;
    let (year, month, day) = civil_date(days);
    format!("HellLocker-backup-{year:04}-{month:02}-{day:02}.zip")
}

/// Days since 1970-01-01 to a calendar date (Howard Hinnant's algorithm).
fn civil_date(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// Write a backup of the data folder to `target`. Returns how many files it holds.
pub fn create(data: &Path, target: &Path) -> Result<usize> {
    ensure!(
        data.join(CONFIG).is_file(),
        "there are no settings to back up yet"
    );
    let pending = target.with_extension("zip.part");
    let result = (|| -> Result<usize> {
        let mut archive = ZipWriter::new(fs::File::create(&pending)?);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(6));
        let mut count = 0;
        for name in [CONFIG, PRESETS] {
            let path = data.join(name);
            if path.is_file() {
                archive.start_file(name, options)?;
                archive.write_all(&fs::read(&path)?)?;
                count += 1;
            }
        }
        let mut files = Vec::new();
        collect_files(&data.join(TEMPLATES), &mut files)?;
        collect_files(&data.join(ICONS), &mut files)?;
        files.sort();
        for path in files {
            let relative = path.strip_prefix(data)?;
            let name = relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            archive.start_file(name, options)?;
            archive.write_all(&fs::read(&path)?)?;
            count += 1;
        }
        archive.finish()?.sync_all()?;
        Ok(count)
    })();
    match result {
        Ok(count) => {
            fs::rename(&pending, target)
                .with_context(|| format!("failed to save {}", target.display()))?;
            tracing::info!(files = count, "backup saved");
            Ok(count)
        }
        Err(error) => {
            let _ = fs::remove_file(&pending);
            Err(error.context("failed to write the backup"))
        }
    }
}

fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            collect_files(&entry.path(), files)?;
        } else if kind.is_file() {
            files.push(entry.path());
        }
    }
    Ok(())
}

/// Where a zip entry belongs in the data folder, or None if a backup never holds it.
fn entry_path(name: &str) -> Option<PathBuf> {
    let path = Path::new(name);
    if !path
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
    {
        return None;
    }
    let mut parts = path.components();
    let first = parts.next()?.as_os_str().to_str()?;
    match first {
        CONFIG | PRESETS if parts.next().is_none() => Some(path.to_path_buf()),
        TEMPLATES | ICONS if parts.next().is_some() => Some(path.to_path_buf()),
        _ => None,
    }
}

/// Check a backup and unpack it to wait for the next start. Nothing live changes.
pub fn stage(backup: &Path, data: &Path) -> Result<()> {
    let staging = data.join(PENDING);
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    let result = (|| -> Result<()> {
        let file = fs::File::open(backup)
            .with_context(|| format!("failed to open {}", backup.display()))?;
        let mut archive = ZipArchive::new(file).context("this is not a HellLocker backup zip")?;
        let mut unpacked = 0u64;
        let mut has_config = false;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index)?;
            if entry.is_dir() {
                continue;
            }
            let Some(relative) = entry
                .enclosed_name()
                .as_deref()
                .and_then(|name| entry_path(&name.to_string_lossy().replace('\\', "/")))
            else {
                tracing::debug!(name = ?entry.name(), "skipped a file a backup does not hold");
                continue;
            };
            has_config |= relative == Path::new(CONFIG);
            let target = staging.join(&relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out = fs::File::create(&target)?;
            let budget = MAX_UNPACKED.saturating_sub(unpacked);
            let copied = std::io::copy(&mut (&mut entry).take(budget + 1), &mut out)?;
            unpacked += copied;
            ensure!(unpacked <= MAX_UNPACKED, "the backup is too large");
        }
        ensure!(
            has_config,
            "this zip has no HellLocker settings (config.toml) in it"
        );
        // The same check HellLocker makes at start, so a bad file is caught now.
        crate::config::load_app_config(&staging.join(CONFIG))
            .context("the backup's settings could not be read")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

/// At start: swap in a restore that is waiting. Returns whether one was applied.
pub fn apply_pending(data: &Path) -> Result<bool> {
    let staging = data.join(PENDING);
    if !staging.join(CONFIG).is_file() {
        if staging.exists() {
            fs::remove_dir_all(&staging)?;
        }
        return Ok(false);
    }
    // Keep what is being replaced, in case the wrong backup was picked.
    if data.join(CONFIG).is_file() {
        let backups = data.join(BACKUPS);
        fs::create_dir_all(&backups)?;
        create(data, &backups.join(BEFORE_RESTORE))
            .context("failed to save the current presets before restoring")?;
    }
    for name in [CONFIG, PRESETS] {
        let path = data.join(name);
        if path.exists() {
            fs::remove_file(&path)?;
        }
    }
    let templates = data.join(TEMPLATES);
    if templates.exists() {
        fs::remove_dir_all(&templates)?;
    }
    // Icons are only replaced by a backup that has some (older backups have none).
    let icons = data.join(ICONS);
    if staging.join(ICONS).exists() && icons.exists() {
        fs::remove_dir_all(&icons)?;
    }
    for name in [CONFIG, PRESETS, TEMPLATES, ICONS] {
        let from = staging.join(name);
        if from.exists() {
            fs::rename(&from, data.join(name))
                .with_context(|| format!("failed to restore {name}"))?;
        }
    }
    fs::remove_dir_all(&staging)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_all(path: &Path) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        fs::File::open(path)?.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hl-backup-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn dates() {
        assert_eq!(civil_date(0), (1970, 1, 1));
        assert_eq!(civil_date(20_752), (2026, 10, 26));
        assert_eq!(civil_date(11_016), (2000, 2, 29));
    }

    #[test]
    fn only_backup_files_are_restored() {
        assert!(entry_path("config.toml").is_some());
        assert!(entry_path("local_templates/preset_1/a.png").is_some());
        assert!(entry_path("local_templates").is_none());
        assert!(entry_path("icons/mine.png").is_some());
        assert!(entry_path("../config.toml").is_none());
        assert!(entry_path("app.log").is_none());
        assert!(entry_path("config.toml/x").is_none());
    }

    #[test]
    fn backup_and_restore_round_trip() {
        let data = temp("data");
        fs::write(data.join(CONFIG), crate::config::DEFAULT_CONFIG_TOML).unwrap();
        fs::write(data.join(PRESETS), "{\"a\":1}").unwrap();
        fs::create_dir_all(data.join("local_templates/preset_1")).unwrap();
        fs::write(data.join("local_templates/preset_1/pic.png"), [1, 2, 3]).unwrap();
        fs::write(data.join("app.log"), "log").unwrap();
        fs::create_dir_all(data.join(ICONS)).unwrap();
        fs::write(data.join(ICONS).join("mine.png"), [4]).unwrap();
        let zip = data.join("backup.zip");
        assert_eq!(create(&data, &zip).unwrap(), 4);

        // Change everything, then restore.
        fs::write(data.join(PRESETS), "{\"b\":2}").unwrap();
        fs::write(data.join("local_templates/preset_1/new.png"), [9]).unwrap();
        stage(&zip, &data).unwrap();
        // Nothing live changes until the next start.
        assert_eq!(fs::read_to_string(data.join(PRESETS)).unwrap(), "{\"b\":2}");
        assert!(apply_pending(&data).unwrap());
        assert_eq!(fs::read_to_string(data.join(PRESETS)).unwrap(), "{\"a\":1}");
        assert_eq!(
            read_all(&data.join("local_templates/preset_1/pic.png")).unwrap(),
            [1, 2, 3]
        );
        assert!(!data.join("local_templates/preset_1/new.png").exists());
        assert_eq!(read_all(&data.join(ICONS).join("mine.png")).unwrap(), [4]);
        assert!(data.join(BACKUPS).join(BEFORE_RESTORE).is_file());
        assert_eq!(fs::read_to_string(data.join("app.log")).unwrap(), "log");
        assert!(!apply_pending(&data).unwrap());

        // A zip that is not a backup is refused and leaves nothing behind.
        let other = data.join("other.zip");
        let mut archive = ZipWriter::new(fs::File::create(&other).unwrap());
        archive
            .start_file("readme.txt", SimpleFileOptions::default())
            .unwrap();
        archive.write_all(b"hi").unwrap();
        archive.finish().unwrap();
        assert!(stage(&other, &data).is_err());
        assert!(!data.join(PENDING).exists());
        let _ = fs::remove_dir_all(&data);
    }
}
