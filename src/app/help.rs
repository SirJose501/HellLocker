//! HellLocker: the Help tab of Settings: report a bug, open the data folder, copy
//! system info, and back up or restore presets.
use std::fmt::Write as _;
use std::path::Path;

use tracing::{info, warn};

use super::AppController;
use crate::ui::HelpAction;

impl AppController {
    pub(super) fn help(&mut self, action: HelpAction) {
        info!(?action, "help action");
        let data = self
            .paths
            .config
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let result = match action {
            HelpAction::ReportBug => open_url(crate::support::BUG_REPORTS)
                .map(|()| "Opened the Bugs tab in your browser.".to_string()),
            HelpAction::OpenDataFolder => {
                open_folder(&data).map(|()| "Opened the data folder.".to_string())
            }
            HelpAction::CopySystemInfo => {
                let text = self.system_info();
                copy_text(self.ui.native_handle(), &text)
                    .map(|()| "System info copied. Paste it into your bug report.".to_string())
            }
            HelpAction::Backup => self.backup(&data),
            HelpAction::Restore => self.restore(&data),
        };
        match result {
            Ok(message) if message.is_empty() => {}
            Ok(message) => self.ui.settings_notice(&message),
            Err(error) => {
                warn!(?action, error = %format!("{error:#}"), "help action failed");
                self.ui.settings_problem(&format!("{error:#}"));
            }
        }
    }

    fn backup(&self, data: &Path) -> anyhow::Result<String> {
        let Some(target) = pick_save_zip(
            self.ui.native_handle(),
            "Save a HellLocker backup",
            &crate::backup::file_name(),
        ) else {
            return Ok(String::new());
        };
        let count = crate::backup::create(data, &target)?;
        let name = target
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(format!(
            "Backup saved: {name} ({count} {}).",
            if count == 1 { "file" } else { "files" }
        ))
    }

    fn restore(&mut self, data: &Path) -> anyhow::Result<String> {
        let owner = self.ui.native_handle();
        let Some(backup) = pick_open_zip(owner, "Restore a HellLocker backup") else {
            return Ok(String::new());
        };
        // Checked before asking, so a wrong file is reported straight away.
        crate::backup::stage(&backup, data)?;
        if !confirm(
            owner,
            "HellLocker - Restore backup",
            "Replace your presets and settings with this backup?\r\n\r\nWhat you have now is kept in backups\\before-restore.zip in the data folder. HellLocker restarts to finish.",
        ) {
            let _ = std::fs::remove_dir_all(data.join("restore.pending"));
            return Ok("Restore cancelled. Nothing changed.".into());
        }
        crate::backup::request_restart();
        self.ui.close_settings();
        self.request_exit()?;
        Ok(String::new())
    }

    /// What helps with a bug report, without names or file paths.
    fn system_info(&self) -> String {
        let mut text = String::new();
        let portable = std::env::current_exe()
            .ok()
            .and_then(|exe| {
                exe.parent()
                    .map(|dir| dir.join(crate::app_paths::PORTABLE_MARKER).exists())
            })
            .unwrap_or(false);
        let _ = writeln!(
            text,
            "HellLocker {} ({})",
            env!("CARGO_PKG_VERSION"),
            if portable {
                "portable"
            } else {
                "data in AppData"
            }
        );
        let _ = writeln!(text, "Windows: {}", windows_version());
        let scale = system_scale();
        let _ = writeln!(
            text,
            "Display scale: {:.0}% · Panel size: {:.0}% · Panel display: {}",
            scale * 100.0,
            self.ui_scale * 100.0,
            self.ui.monitor()
        );
        for (index, monitor) in displays().into_iter().enumerate() {
            let _ = writeln!(text, "Display {}: {monitor}", index + 1);
        }
        match crate::game_settings::read_color_settings() {
            Ok(game) => {
                let _ = writeln!(
                    text,
                    "Game: HDR {} · brightness {} · UI brightness {}",
                    if game.hdr_enabled { "on" } else { "off" },
                    game.screen_brightness,
                    game.ui_brightness
                );
            }
            Err(_) => {
                let _ = writeln!(text, "Game: settings file not found");
            }
        }
        let _ = writeln!(
            text,
            "Game running: {}",
            if crate::game_window::find_game_window().is_ok() {
                "yes"
            } else {
                "no"
            }
        );
        if let Some(lines) = wheel_scroll_lines() {
            let _ = writeln!(text, "Mouse wheel: {lines} lines");
        }
        let (presets, folders) = self.ui.preset_counts();
        let _ = writeln!(text, "Presets: {presets} in {folders} folders");
        let _ = writeln!(
            text,
            "Settings: patient mode {} · keep open {} · saved order {} · automatic readying {} · booster fallbacking {}",
            on_off(crate::timing::patient()),
            on_off(self.keep_open),
            on_off(self.settings.apply_in_saved_order),
            on_off(self.settings.auto_ready_up),
            on_off(self.settings.save_fallback_when_taken),
        );
        text
    }
}

fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

#[cfg(target_os = "windows")]
use crate::platform::{
    system::{
        confirm, copy_text, open_folder, open_url, pick_open_zip, pick_save_zip,
        wheel_scroll_lines, windows_version,
    },
    system_scale,
};

#[cfg(target_os = "windows")]
fn displays() -> Vec<String> {
    crate::platform::monitors::available()
        .map(|monitors| {
            monitors
                .into_iter()
                .map(|monitor| {
                    let r = monitor.bounds;
                    format!(
                        "{} x {}{}",
                        r.right - r.left,
                        r.bottom - r.top,
                        if monitor.primary { " (primary)" } else { "" }
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(not(target_os = "windows"))]
mod fallback {
    use std::path::{Path, PathBuf};

    use anyhow::{Result, bail};

    pub fn confirm(_: Option<isize>, _: &str, _: &str) -> bool {
        false
    }
    pub fn copy_text(_: Option<isize>, _: &str) -> Result<()> {
        bail!("only available on Windows")
    }
    pub fn open_folder(_: &Path) -> Result<()> {
        bail!("only available on Windows")
    }
    pub fn open_url(_: &str) -> Result<()> {
        bail!("only available on Windows")
    }
    pub fn pick_open_zip(_: Option<isize>, _: &str) -> Option<PathBuf> {
        None
    }
    pub fn pick_save_zip(_: Option<isize>, _: &str, _: &str) -> Option<PathBuf> {
        None
    }
    pub fn wheel_scroll_lines() -> Option<u32> {
        None
    }
    pub fn windows_version() -> String {
        String::new()
    }
    pub fn system_scale() -> f64 {
        1.0
    }
    pub fn displays() -> Vec<String> {
        Vec::new()
    }
}
#[cfg(not(target_os = "windows"))]
use fallback::*;
