//! Commit settings only after shortcut registration and the single file write succeed.
use anyhow::{Result, ensure};
use tracing::warn;

use super::{ActionState, AppController, panel_hotkey, preset_hotkeys};
use crate::{config::SettingsDraft, input::Hotkeys, preset_action::PresetActionOptions};

impl AppController {
    pub(super) fn open_settings(&self) {
        if self.exiting || !matches!(self.state, ActionState::Idle) || self.ui.is_editing() {
            return;
        }
        self.ui.open_settings(SettingsDraft {
            shortcuts: self.shortcut_bindings.clone(),
            monitor: self.ui.monitor().to_owned(),
            language: self.ui.language(),
            apply_in_saved_order: self.settings.apply_in_saved_order,
            auto_ready_up: self.settings.auto_ready_up,
            save_fallback_when_taken: self.settings.save_fallback_when_taken,
            patient_mode: crate::timing::patient(),
            keep_open: self.keep_open,
            ui_scale: self.ui_scale,
            background_opacity: self.background_opacity,
            status_popup: self.status_popup,
            save_sound: self.save_sound,
            start_with_windows: start_with_windows(),
        });
    }

    pub(super) fn save_settings(&mut self, draft: SettingsDraft) -> Result<()> {
        draft.shortcuts.validate()?;
        self.hotkeys.set_enabled(false)?;
        self.panel_hotkey.set_enabled(false)?;

        let bindings = preset_hotkeys(&draft.shortcuts)?;
        let specs: Vec<_> = bindings.iter().map(|binding| binding.hotkey).collect();
        let mut hotkeys = Hotkeys::new(&specs)?;
        let mut panel = panel_hotkey(&draft.shortcuts)?;
        // Validate the entire candidate set before replacing the saved bindings.
        panel.set_enabled(true)?;
        hotkeys.set_enabled(true)?;
        let unavailable = panel
            .unavailable()
            .chain(hotkeys.unavailable())
            .collect::<Vec<_>>()
            .join("\n");
        ensure!(
            unavailable.is_empty(),
            "Shortcuts unavailable:\n{unavailable}"
        );
        panel.set_enabled(false)?;
        hotkeys.set_enabled(false)?;
        draft.save(&self.paths.config)?;
        // HellLocker: kept by Windows; a failure here leaves the other settings saved.
        let startup_result = if draft.start_with_windows != start_with_windows() {
            set_start_with_windows(draft.start_with_windows)
        } else {
            Ok(())
        };

        self.bindings = bindings;
        self.hotkeys = hotkeys;
        self.panel_hotkey = panel;
        self.shortcut_bindings = draft.shortcuts;
        self.keep_open = draft.keep_open;
        let resize = (draft.ui_scale - self.ui_scale).abs() > f64::EPSILON;
        self.ui_scale = draft.ui_scale;
        self.background_opacity = draft.background_opacity;
        self.ui.set_background_opacity(draft.background_opacity);
        self.status_popup = draft.status_popup;
        self.save_sound = draft.save_sound;
        crate::timing::set_patient(draft.patient_mode);
        if let Err(error) = self.ui.set_status_popup(draft.status_popup) {
            warn!(error = %format!("{error:#}"), "could not move the status pop-up");
        }
        self.settings = PresetActionOptions {
            attire: self.settings.attire,
            apply_in_saved_order: draft.apply_in_saved_order,
            auto_ready_up: draft.auto_ready_up,
            save_fallback_when_taken: draft.save_fallback_when_taken,
        };
        let language_result = self.ui.set_language(draft.language);
        self.ui.settings_saved(&self.shortcut_bindings);
        if let Err(error) = language_result {
            warn!(error = %format!("{error:#}"), "settings saved but language change failed");
            self.ui.set_status(&format!(
                "Settings saved, but language change failed: {error:#}"
            ));
        }
        if self.ui.monitor() != draft.monitor
            && let Err(error) = self.ui.set_monitor(draft.monitor)
        {
            warn!(error = %format!("{error:#}"), "settings saved but window placement failed");
            self.ui.set_status(&format!(
                "Settings saved, but window placement failed: {error:#}"
            ));
        }
        if let Err(error) = startup_result {
            warn!(error = %format!("{error:#}"), "settings saved but start with Windows failed");
            self.ui.set_status(&format!(
                "Settings saved, but Start with Windows could not be changed: {error:#}"
            ));
        }
        if resize {
            self.ui
                .set_status("Panel size saved. Restart HellLocker (tray icon > Exit) to apply it.");
        }
        tracing::info!("settings saved");
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn start_with_windows() -> bool {
    crate::platform::system::starts_with_windows().unwrap_or_else(|error| {
        warn!(error = %format!("{error:#}"), "could not read start with Windows");
        false
    })
}

#[cfg(not(target_os = "windows"))]
fn start_with_windows() -> bool {
    false
}

#[cfg(target_os = "windows")]
fn set_start_with_windows(on: bool) -> Result<()> {
    crate::platform::system::set_start_with_windows(on)
}

#[cfg(not(target_os = "windows"))]
fn set_start_with_windows(_on: bool) -> Result<()> {
    anyhow::bail!("only available on Windows")
}
