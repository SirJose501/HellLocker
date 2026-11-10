//! Passive action feedback, driven by the same events as the automation.
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::Result;
use slint::winit_030::WinitWindowAccessor;
use slint::{ComponentHandle, Model, VecModel};

use super::{ActionStatus, PresetInfo, StatusItem, StatusTone, place_top_corner};
use crate::app_events::{AppEvent, PresetCompletion};

pub(super) struct Status {
    window: ActionStatus,
    hide_at: Option<Instant>,
    presets: PathBuf,
    preset: String,
    /// HellLocker: which corner, or off.
    popup: crate::config::StatusPopup,
}

impl Status {
    pub fn new(presets: PathBuf) -> Result<Self> {
        Ok(Self {
            window: ActionStatus::new()?,
            hide_at: None,
            presets,
            preset: String::new(),
            popup: crate::config::StatusPopup::default(),
        })
    }

    pub fn set_popup(&mut self, popup: crate::config::StatusPopup) {
        self.popup = popup;
    }

    pub fn enabled(&self) -> bool {
        self.popup != crate::config::StatusPopup::Off
    }

    pub fn message(&self) -> String {
        self.window.get_message().into()
    }

    pub fn set_font_family(&self, family: &str) {
        self.window.set_ui_font_family(family.into());
    }

    pub fn update(&mut self, event: AppEvent, preset_info: &BTreeMap<String, PresetInfo>) {
        use StatusTone::{Error, Success, Warning, Working};
        if let AppEvent::PresetStarted { preset }
        | AppEvent::HotkeyReleaseRequested { preset }
        | AppEvent::PresetCancelled { preset, .. }
        | AppEvent::FallbackBoosterRequested { preset }
        | AppEvent::PresetDone { preset, .. }
        | AppEvent::PresetFailed { preset, .. } = &event
        {
            self.preset.clone_from(preset);
            let label = if preset == crate::preset::library::SCAN_PRESET {
                "Equipment scan"
            } else {
                preset_info
                    .get(preset)
                    .map(|info| info.label.as_str())
                    .unwrap_or("")
            };
            self.window.set_preset_label(label.into());
            self.window
                .set_preset_number(super::preset_number(preset).into());
        }
        if !matches!(
            &event,
            AppEvent::StratagemsProgress { .. }
                | AppEvent::BoosterProgress { .. }
                | AppEvent::EquipmentProgress { .. }
                | AppEvent::EquipmentCaching { .. }
                | AppEvent::ScanProgress { .. }
                | AppEvent::PresetDone { .. }
        ) {
            self.window.set_stratagems(Default::default());
            self.window.set_booster(StatusItem::default());
            self.window.set_picture(slint::Image::default());
        }
        let progress = self.progress(&event);
        let (message, tone, seconds) = match event {
            AppEvent::PresetStarted { .. } => ("Starting".into(), Working, None),
            AppEvent::HotkeyReleaseRequested { .. } => (
                "Release the shortcut keys and mouse button to continue".into(), Working, None),
            AppEvent::PresetCancelled { reason, .. } => (format!("Cancelled: {reason}"), Warning, Some(5)),
            AppEvent::EquipmentActionStarted { saving } => {
                (if saving { "Saving equipment" } else { "Applying equipment" }.into(), Working, None)
            }
            AppEvent::ScanProgress { kind, items } => (
                format!("Scanning {} · {items} pictures · leave the game alone", kind_label(kind)),
                Working,
                None,
            ),
            AppEvent::EquipmentCaching { category, repairing } => {
                self.window.set_picture(super::image(&category));
                (if repairing { "Updating equipment cache" } else { "Building equipment cache" }.into(),
                    Working, None)
            }
            AppEvent::StratagemsApplyStarted { stratagems, booster, booster_confirmed } => {
                let icons = stratagems.iter().map(|id| self.icon(id)).collect::<Vec<_>>();
                self.window.set_stratagems(Rc::new(VecModel::from(icons)).into());
                let mut booster = booster.map(|id| self.icon(&id)).unwrap_or_default();
                booster.confirmed = booster_confirmed;
                self.window.set_booster(booster);
                (format!("Applying Stratagems · 0/{}", stratagems.len()), Working, None)
            }
            AppEvent::StratagemsProgress { remaining } => {
                let icons = self.window.get_stratagems();
                for (index, mut icon) in icons.iter().enumerate() {
                    icon.confirmed = !remaining.iter().any(|id| id == icon.item_id.as_str());
                    icons.set_row_data(index, icon);
                }
                let completed = icons.iter().filter(|icon| icon.confirmed).count();
                (format!("Applying Stratagems · {completed}/{}", icons.row_count()), Working, None)
            }
            AppEvent::BoosterProgress { item_id, confirmed } => {
                let mut icon = self.window.get_booster();
                if icon.item_id.as_str() != item_id {
                    icon = self.icon(&item_id);
                }
                icon.confirmed = confirmed;
                self.window.set_booster(icon);
                ("Applying Booster".into(), Working, None)
            }
            AppEvent::EquipmentProgress { kind, confirmed } => {
                if !confirmed {
                    self.window.set_picture(match crate::preset::preview::equipment_name(
                        &self.presets, &self.preset, kind)
                    {
                        Ok(image) => super::image(&image),
                        Err(error) => {
                            tracing::warn!(preset = %self.preset, ?kind,
                                error = %format!("{error:#}"), "could not load status preview");
                            slint::Image::default()
                        }
                    });
                }
                (format!("Applying equipment · {}/{}", kind.index() + usize::from(confirmed),
                    crate::item::EquipmentKind::ALL.len()), Working, None)
            }
            AppEvent::FallbackBoosterRequested { .. } => (
                "Saved Booster is already in use. Select another to save as your fallback, or return to cancel.".into(),
                Warning, None),
            AppEvent::PresetDone { completion, .. } => match completion {
                PresetCompletion::Complete => ("Done".into(), Success, Some(2)),
                PresetCompletion::Saved => ("Saved".into(), Success, Some(2)),
                PresetCompletion::EquipmentApplied => ("Equipment applied".into(), Success, Some(2)),
                PresetCompletion::BoosterUnavailable => ("Booster already in use".into(), Warning, Some(5)),
                PresetCompletion::FallbackBoosterSaved => ("Fallback Booster saved".into(), Success, Some(2)),
                PresetCompletion::FallbackBoosterNotSaved => ("Fallback Booster not saved".into(), Warning, Some(5)),
                PresetCompletion::Scanned { items } => (format!("Scan done · {items} pictures saved"), Success, Some(5)),
            },
            AppEvent::PresetFailed { error, .. } => (
                format!("Failed: {}", error.lines().next().unwrap_or(&error).trim()), Error, Some(5)),
        };
        self.window.set_message(message.into());
        self.window.set_tone(tone);
        self.window.set_progress(progress);
        self.window.set_stop_key(crate::cancel::STOP_KEY.into());
        self.hide_at = seconds.map(|seconds| Instant::now() + Duration::from_secs(seconds));
    }

    /// HellLocker 1.9.4: how far along the current step is, for the bar (-1 = no bar).
    fn progress(&self, event: &AppEvent) -> f32 {
        let slots = crate::item::EquipmentKind::ALL.len() as f32;
        match event {
            AppEvent::PresetStarted { .. } | AppEvent::EquipmentActionStarted { .. } => 0.0,
            AppEvent::StratagemsApplyStarted { .. } => 0.0,
            AppEvent::StratagemsProgress { remaining } => {
                let total = self.window.get_stratagems().row_count().max(1) as f32;
                (total - remaining.len() as f32).max(0.0) / total
            }
            AppEvent::BoosterProgress { confirmed, .. } => {
                if *confirmed {
                    1.0
                } else {
                    0.9
                }
            }
            AppEvent::EquipmentProgress { kind, confirmed } => {
                (kind.index() as f32 + if *confirmed { 1.0 } else { 0.0 }) / slots
            }
            AppEvent::ScanProgress { kind, .. } => kind.index() as f32 / slots,
            AppEvent::EquipmentCaching { .. } => self.window.get_progress(),
            AppEvent::PresetDone { .. } => 1.0,
            _ => -1.0,
        }
    }

    /// Load only at the start or when switching to the fallback Booster.
    fn icon(&self, item_id: &str) -> StatusItem {
        let picture = match crate::preset::load_template_image(&self.presets, item_id) {
            Ok(image) => super::image(&image),
            Err(error) => {
                tracing::warn!(preset = %self.preset, item_id,
                    error = %format!("{error:#}"), "could not load status preview");
                slint::Image::default()
            }
        };
        StatusItem {
            item_id: item_id.into(),
            picture,
            confirmed: false,
        }
    }

    pub fn show(&self, monitor: &str, anchor: Option<(i32, i32)>) -> Result<()> {
        if !self.window.window().is_visible() {
            self.window.show()?;
            // Creation uses with_active(false). Set hit-testing before returning
            // to the event loop, so this surface can never intercept a click.
            self.window
                .window()
                .with_winit_window(|window| -> Result<()> {
                    window.set_cursor_hittest(false)?;
                    #[cfg(target_os = "windows")]
                    super::windows::make_passive(window)?;
                    Ok(())
                })
                .transpose()?;
            self.reposition(monitor, anchor)?;
        }
        Ok(())
    }

    pub fn reposition(&self, monitor: &str, anchor: Option<(i32, i32)>) -> Result<()> {
        if self.window.window().is_visible() {
            let left = self.popup == crate::config::StatusPopup::TopLeft;
            place_top_corner(self.window.window(), monitor, anchor, 16.0, None, left)?;
        }
        Ok(())
    }

    pub fn hide(&mut self) -> Result<()> {
        self.hide_at = None;
        self.window.hide()?;
        Ok(())
    }

    pub fn tick(&mut self) -> Result<()> {
        if self
            .hide_at
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.hide()?;
        }
        Ok(())
    }
}

/// HellLocker: a slot's name for scan progress.
fn kind_label(kind: crate::item::EquipmentKind) -> &'static str {
    use crate::item::EquipmentKind::*;
    match kind {
        Helmet => "helmets",
        Armor => "armor",
        Cape => "capes",
        Primary => "primaries",
        Secondary => "secondaries",
        Throwable => "throwables",
    }
}
