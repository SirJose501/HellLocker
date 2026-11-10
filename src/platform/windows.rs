pub mod monitors;
pub mod system;

use anyhow::{Context, Result};
use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Diagnostics::Debug::MessageBeep;
use windows::Win32::System::Threading::{CreateMutexExW, SYNCHRONIZATION_SYNCHRONIZE};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForSystem, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND, MESSAGEBOX_STYLE, MessageBoxW,
};
use windows::core::{PCWSTR, w};

use crate::app_paths::AppPaths;

/// HellLocker: the Windows display scale (1.0 at 100 %, 1.5 at 150 % ...).
pub fn system_scale() -> f64 {
    // Valid once initialize() made the process DPI aware; 96 DPI is 100 %.
    let dpi = unsafe { GetDpiForSystem() };
    if dpi == 0 { 1.0 } else { f64::from(dpi) / 96.0 }
}

pub struct SingleInstance(HANDLE);

impl SingleInstance {
    pub fn try_acquire() -> Result<Option<Self>> {
        // Scope to this login session, regardless of executable location.
        // Request only synchronization access, not full access to an elevated instance.
        let handle = unsafe {
            CreateMutexExW(
                None,
                w!("Local\\HellLocker"),
                0,
                SYNCHRONIZATION_SYNCHRONIZE.0,
            )
        }
        .context("failed to check whether HellLocker is already running")?;
        let already_running = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        let instance = Self(handle);
        if already_running {
            Ok(None)
        } else {
            Ok(Some(instance))
        }
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        // Holding the handle keeps the marker alive; no mutex ownership is needed.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

pub fn initialize() {
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
}

/// HellLocker: whether the stop key (Backspace) is held down right now.
pub fn stop_key_down() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_BACK};
    // The high bit is set while the key is down.
    let state = unsafe { GetAsyncKeyState(i32::from(VK_BACK.0)) };
    state < 0
}

pub fn notify_preset_saved() {
    unsafe {
        MessageBeep(MB_ICONINFORMATION).ok();
    }
}

pub fn show_error(title: &str, message: &str) {
    show_message(title, message, MB_ICONERROR);
}

pub fn show_notice(title: &str, message: &str) {
    show_message(title, message, MB_ICONINFORMATION);
}

fn show_message(title: &str, message: &str, icon: MESSAGEBOX_STYLE) {
    let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
    let message: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let _ = MessageBoxW(
            None,
            PCWSTR(message.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | icon | MB_SETFOREGROUND,
        );
    }
}

pub fn app_paths() -> Result<AppPaths> {
    let executable = std::env::current_exe().context("failed to locate the executable")?;
    let directory = executable.parent().with_context(|| {
        format!(
            "executable has no parent directory: {}",
            executable.display()
        )
    })?;
    // HellLocker: presets and settings live in %APPDATA%\HellLocker, so replacing the
    // app folder with an update never loses them.
    let appdata = std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_absolute());
    let data = crate::app_paths::data_dir(directory, appdata.as_deref());
    // Before logging starts, so a problem here is reported in the error box.
    if crate::app_paths::migrate_legacy_data(directory, &data)? {
        show_notice(
            "HellLocker - Presets moved",
            &format!(
                "Your presets and settings were moved to:\r\n{}\r\n\r\nThey now stay there when you update HellLocker, so you can replace the whole app folder. The old copy is in \"{}\" next to HellLocker.exe and can be deleted.",
                data.display(),
                crate::app_paths::MOVED_DATA_DIR
            ),
        );
    }
    std::fs::create_dir_all(&data)
        .with_context(|| format!("failed to create {}", data.display()))?;
    Ok(AppPaths::in_dir(&data))
}
