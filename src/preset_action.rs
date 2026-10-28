//! Coordinate preset actions; each section owns its save/apply implementation.
mod combined;
mod equipment;
mod stratagems;

use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result, bail, ensure};
use tracing::{debug, info};

use crate::app_events::{AppEvent, AppEventSink};
use crate::capture::CaptureSessionManager;
use crate::game_settings::read_color_settings;
use crate::game_window::wait_for_foreground_game_window;
use crate::loadout::pages::{Page, show_page};
use crate::permissions;
use crate::preset::PresetScope;
use crate::vision::RecognizerRuntime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetActionOutcome {
    Saved,
    Applied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetCommand {
    ApplySaved(PresetScope),
    SaveStratagems,
    SaveEquipment,
    ApplyStratagems,
    ApplyEquipment,
    SaveFull,
    ApplyFull,
    /// HellLocker: scan every equipment list into the picture library.
    ScanEquipment,
}

#[derive(Clone, Copy)]
pub struct PresetActionOptions {
    /// HellLocker: which of helmet, armor and cape to save and apply (weapons and
    /// grenade always).
    pub attire: crate::item::AttireChoice,
    pub apply_in_saved_order: bool,
    pub auto_ready_up: bool,
    pub save_fallback_when_taken: bool,
}

pub(crate) struct PresetActionContext<'a> {
    pub presets: &'a Path,
    pub options: PresetActionOptions,
    pub events: &'a AppEventSink,
    pub equipment_cache: &'a mut crate::loadout::equipment::EquipmentCache,
    pub evidence: &'a crate::evidence::OperationEvidence,
}

pub fn execute_preset(
    runtime: &RecognizerRuntime,
    context: &mut PresetActionContext<'_>,
    preset_name: &str,
    command: PresetCommand,
    capture_session: &mut CaptureSessionManager,
    expected_window: crate::window::WindowIdentity,
) -> Result<PresetActionOutcome> {
    let action_start = Instant::now();
    info!("preset action started");
    context.events.emit(AppEvent::PresetStarted {
        preset: preset_name.to_string(),
    });

    let command = if let PresetCommand::ApplySaved(scope) = command {
        let (stratagems, equipment) = crate::preset::saved_sections(context.presets, preset_name)?;
        match (scope.stratagems && stratagems, scope.equipment && equipment) {
            (true, true) => PresetCommand::ApplyFull,
            (true, false) => PresetCommand::ApplyStratagems,
            (false, true) => PresetCommand::ApplyEquipment,
            (false, false) => {
                bail!("preset {preset_name} has no saved content in the selected sections")
            }
        }
    } else {
        command
    };
    debug!(resolved_command = ?command, "preset action scope resolved");

    let game_window =
        wait_for_foreground_game_window().context("failed to locate Helldivers window")?;
    ensure!(
        expected_window == game_window.identity(),
        "the game window changed before the action started; try again"
    );
    permissions::ensure_input_access()?;
    let (client_w, client_h) = game_window.client_size();
    debug!(client_w, client_h, "game window ready");
    // HellLocker: list scrolling depends on how far one wheel notch moves.
    #[cfg(target_os = "windows")]
    log_wheel_settings();

    let capture_start = Instant::now();
    let capture = capture_session
        .acquire(&game_window)
        .context("failed to get capture session")?;
    debug!(
        elapsed_s = capture_start.elapsed().as_secs_f64(),
        "capture session ready"
    );
    let game_color_settings =
        read_color_settings().context("failed to read Helldivers color settings")?;
    context.evidence.record(
        "action-context",
        None,
        serde_json::json!({
            "client_size": [client_w, client_h], "game_colors": game_color_settings,
            "resolved_command": format!("{command:?}"),
        }),
    );
    let (outcome, completion) = match command {
        PresetCommand::SaveFull | PresetCommand::ApplyFull => combined::execute(
            runtime,
            context,
            preset_name,
            command,
            capture,
            game_window,
            game_color_settings,
        )?,
        PresetCommand::SaveStratagems | PresetCommand::ApplyStratagems => {
            show_page(
                capture,
                runtime,
                &game_window,
                game_color_settings,
                Page::Stratagems,
                context.evidence,
            )?;
            stratagems::execute(
                runtime,
                context,
                preset_name,
                command,
                capture,
                game_window,
                game_color_settings,
            )?
        }
        PresetCommand::ScanEquipment => {
            show_page(
                capture,
                runtime,
                &game_window,
                game_color_settings,
                Page::Equipment,
                context.evidence,
            )?;
            equipment::scan(capture, game_window, game_color_settings, context)?
        }
        PresetCommand::SaveEquipment | PresetCommand::ApplyEquipment => {
            show_page(
                capture,
                runtime,
                &game_window,
                game_color_settings,
                Page::Equipment,
                context.evidence,
            )?;
            equipment::execute(
                capture,
                game_window,
                game_color_settings,
                context,
                preset_name,
                command,
            )?
        }
        PresetCommand::ApplySaved(_) => unreachable!("saved sections are resolved before dispatch"),
    };
    info!(
        elapsed_s = action_start.elapsed().as_secs_f64(),
        ?completion,
        "preset action completed"
    );
    context.events.emit(AppEvent::PresetDone {
        preset: preset_name.to_string(),
        completion,
    });
    Ok(outcome)
}

/// HellLocker: logs Windows' "lines to scroll at a time" setting (3 by default) at INFO,
/// so bug reports show it: a different value can make the game scroll lists further
/// per wheel notch than the list reading expects.
#[cfg(target_os = "windows")]
fn log_wheel_settings() {
    use windows::Win32::UI::WindowsAndMessaging::{
        SPI_GETWHEELSCROLLLINES, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    };
    let mut lines: u32 = 0;
    let read = unsafe {
        SystemParametersInfoW(
            SPI_GETWHEELSCROLLLINES,
            0,
            Some(std::ptr::from_mut(&mut lines).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    match read {
        // u32::MAX is "one screen at a time".
        Ok(()) if lines == u32::MAX => tracing::info!(
            wheel_scroll_lines = "one screen",
            "Windows mouse wheel setting (default 3 lines)"
        ),
        Ok(()) if lines != 3 => tracing::info!(
            wheel_scroll_lines = lines,
            "Windows mouse wheel setting differs from the default 3 lines"
        ),
        Ok(()) => tracing::debug!(wheel_scroll_lines = lines, "Windows mouse wheel setting"),
        Err(error) => tracing::debug!(%error, "could not read the mouse wheel setting"),
    }
}
