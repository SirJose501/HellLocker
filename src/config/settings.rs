//! Editable settings are saved together, without rewriting preset names or scope.
use std::path::Path;

use anyhow::Result;

use super::{ShortcutBindings, StatusPopup, UiLanguage};

pub struct SettingsDraft {
    pub shortcuts: ShortcutBindings,
    pub monitor: String,
    pub language: UiLanguage,
    pub apply_in_saved_order: bool,
    pub auto_ready_up: bool,
    pub save_fallback_when_taken: bool,
    /// HellLocker: wait up to three times longer for the game.
    pub patient_mode: bool,
    /// HellLocker
    pub keep_open: bool,
    /// HellLocker; applied at the next start.
    pub ui_scale: f64,
    /// HellLocker: background art opacity in percent (10 to 100).
    pub background_opacity: i64,
    /// HellLocker: where status pop-ups show.
    pub status_popup: StatusPopup,
    /// HellLocker: a sound when a preset is saved.
    pub save_sound: bool,
    /// HellLocker: start at sign-in. Kept in Windows (the Run key), not the config.
    pub start_with_windows: bool,
}

impl SettingsDraft {
    pub fn save(&self, path: &Path) -> Result<()> {
        super::edit_config(path, |document| {
            super::write_setting(
                document,
                "ui",
                "panel_shortcut",
                self.shortcuts.panel.as_str(),
            )?;
            super::write_setting(document, "ui", "monitor", self.monitor.as_str())?;
            super::write_setting(
                document,
                "ui",
                "language",
                self.language.locale().unwrap_or("auto"),
            )?;
            super::write_setting(document, "ui", "keep_open", self.keep_open)?;
            super::write_setting(document, "ui", "ui_scale", self.ui_scale)?;
            super::write_setting(
                document,
                "ui",
                "background_opacity",
                self.background_opacity.clamp(10, 100),
            )?;
            super::write_setting(document, "ui", "status_popup", self.status_popup.key())?;
            super::write_setting(document, "ui", "save_sound", self.save_sound)?;
            for (key, value) in [
                ("apply_in_saved_order", self.apply_in_saved_order),
                ("auto_ready_up", self.auto_ready_up),
                ("save_fallback_when_taken", self.save_fallback_when_taken),
                ("patient_mode", self.patient_mode),
            ] {
                super::write_setting(document, "presets", key, value)?;
            }
            for (name, binding) in &self.shortcuts.presets {
                super::write_slot_field(document, name, "shortcut", binding)?;
            }
            Ok(())
        })
    }
}
