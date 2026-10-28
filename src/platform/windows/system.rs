//! HellLocker: small Windows services for the Settings window: starting with Windows,
//! opening links and folders, the clipboard, file pickers and system details.
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, HANDLE, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, REG_SZ, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, GetSaveFileNameW, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT,
    OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, PCWSTR, PWSTR, w};

const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const RUN_VALUE: PCWSTR = w!("HellLocker");
/// Windows' Unicode text clipboard format.
const CF_UNICODETEXT: u32 = 13;

/// The command Windows runs at sign-in when "Start with Windows" is on.
fn run_command() -> Result<String> {
    let exe = std::env::current_exe().context("failed to locate the executable")?;
    Ok(format!("\"{}\"", exe.display()))
}

fn read_string(root: HKEY, key: PCWSTR, value: PCWSTR) -> Result<Option<String>> {
    let mut buffer = vec![0u16; 1024];
    let mut size = (buffer.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            root,
            key,
            value,
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    status.ok().context("failed to read the registry")?;
    let len = (size as usize / 2).saturating_sub(1).min(buffer.len());
    Ok(Some(String::from_utf16_lossy(&buffer[..len])))
}

fn read_dword(root: HKEY, key: PCWSTR, value: PCWSTR) -> Option<u32> {
    let mut data = 0u32;
    let mut size = 4u32;
    let status = unsafe {
        RegGetValueW(
            root,
            key,
            value,
            RRF_RT_REG_DWORD,
            None,
            Some(std::ptr::from_mut(&mut data).cast()),
            Some(&mut size),
        )
    };
    status.is_ok().then_some(data)
}

/// Whether HellLocker starts when the user signs in to Windows (this exe, from here).
pub fn starts_with_windows() -> Result<bool> {
    Ok(read_string(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE)?
        .is_some_and(|command| command.eq_ignore_ascii_case(&run_command().unwrap_or_default())))
}

/// Turn starting with Windows on or off, for the current user only.
pub fn set_start_with_windows(on: bool) -> Result<()> {
    if on {
        let command: Vec<u16> = run_command()?.encode_utf16().chain(Some(0)).collect();
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                RUN_VALUE,
                REG_SZ.0,
                Some(command.as_ptr().cast()),
                (command.len() * 2) as u32,
            )
        }
        .ok()
        .context("failed to turn on Start with Windows")?;
    } else {
        let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE) };
        if status != ERROR_FILE_NOT_FOUND {
            status
                .ok()
                .context("failed to turn off Start with Windows")?;
        }
    }
    tracing::info!(start_with_windows = on, "start with Windows changed");
    Ok(())
}

fn shell_open(target: &HSTRING) -> Result<()> {
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            target,
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecute reports success with a value above 32.
    if result.0 as usize <= 32 {
        bail!("Windows could not open {target}");
    }
    Ok(())
}

/// Open a web page in the default browser.
pub fn open_url(url: &str) -> Result<()> {
    shell_open(&HSTRING::from(url))
}

/// Open a folder in File Explorer.
pub fn open_folder(path: &Path) -> Result<()> {
    shell_open(&HSTRING::from(path.as_os_str()))
}

/// Put text on the clipboard.
pub fn copy_text(owner: Option<isize>, text: &str) -> Result<()> {
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    unsafe {
        OpenClipboard(owner.map(|hwnd| HWND(hwnd as *mut _))).context("the clipboard is busy")?;
        let result = (|| -> Result<()> {
            EmptyClipboard()?;
            let memory = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2)?;
            let target = GlobalLock(memory);
            if target.is_null() {
                bail!("could not write to the clipboard");
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), target.cast::<u16>(), wide.len());
            let _ = GlobalUnlock(memory);
            // The clipboard owns the memory once this succeeds.
            SetClipboardData(CF_UNICODETEXT, Some(HANDLE(memory.0)))?;
            Ok(())
        })();
        let _ = CloseClipboard();
        result
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// Ask where to save a zip file; None if the user cancels.
pub fn pick_save_zip(owner: Option<isize>, title: &str, file_name: &str) -> Option<PathBuf> {
    pick_zip(owner, title, file_name, true)
}

/// Ask for a zip file to open; None if the user cancels.
pub fn pick_open_zip(owner: Option<isize>, title: &str) -> Option<PathBuf> {
    pick_zip(owner, title, "", false)
}

fn pick_zip(owner: Option<isize>, title: &str, file_name: &str, save: bool) -> Option<PathBuf> {
    let mut file = vec![0u16; 1024];
    for (slot, unit) in file.iter_mut().zip(file_name.encode_utf16().take(1000)) {
        *slot = unit;
    }
    let filter: Vec<u16> = "HellLocker backup (*.zip)\0*.zip\0"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let title = wide(title);
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner.map(|hwnd| HWND(hwnd as *mut _)).unwrap_or_default(),
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(file.as_mut_ptr()),
        nMaxFile: file.len() as u32,
        lpstrTitle: PCWSTR(title.as_ptr()),
        lpstrDefExt: w!("zip"),
        Flags: OFN_NOCHANGEDIR
            | OFN_PATHMUSTEXIST
            | if save {
                OFN_OVERWRITEPROMPT
            } else {
                OFN_FILEMUSTEXIST
            },
        ..Default::default()
    };
    let chosen = unsafe {
        if save {
            GetSaveFileNameW(&mut dialog)
        } else {
            GetOpenFileNameW(&mut dialog)
        }
    };
    if !chosen.as_bool() {
        return None;
    }
    let len = file
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(file.len());
    Some(PathBuf::from(String::from_utf16_lossy(&file[..len])))
}

/// The Windows edition and version, e.g. "Windows 11 Pro 24H2 (build 26100)".
pub fn windows_version() -> String {
    let key = w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
    let read = |value| read_string(HKEY_LOCAL_MACHINE, key, value).ok().flatten();
    let build = read(w!("CurrentBuild")).unwrap_or_default();
    // The product name still says "Windows 10" on Windows 11; the build tells them apart.
    let mut product = read(w!("ProductName")).unwrap_or_else(|| "Windows".into());
    if build.parse::<u32>().is_ok_and(|build| build >= 22000) {
        product = product.replacen("Windows 10", "Windows 11", 1);
    }
    let release = read(w!("DisplayVersion")).unwrap_or_default();
    let revision = read_dword(HKEY_LOCAL_MACHINE, key, w!("UBR"))
        .map(|ubr| format!(".{ubr}"))
        .unwrap_or_default();
    format!("{product} {release} (build {build}{revision})")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Lines the mouse wheel scrolls per notch (Windows' default is 3).
pub fn wheel_scroll_lines() -> Option<u32> {
    use windows::Win32::UI::WindowsAndMessaging::{
        SPI_GETWHEELSCROLLLINES, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    };
    let mut lines = 0u32;
    unsafe {
        SystemParametersInfoW(
            SPI_GETWHEELSCROLLLINES,
            0,
            Some(std::ptr::from_mut(&mut lines).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .ok()
    .map(|()| lines)
}

/// Ask OK / Cancel over the panel; true for OK.
pub fn confirm(owner: Option<isize>, title: &str, message: &str) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        IDOK, MB_DEFBUTTON2, MB_ICONWARNING, MB_OKCANCEL, MB_SETFOREGROUND, MB_TOPMOST, MessageBoxW,
    };
    let title = wide(title);
    let message = wide(message);
    let answer = unsafe {
        MessageBoxW(
            owner.map(|hwnd| HWND(hwnd as *mut _)),
            PCWSTR(message.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OKCANCEL | MB_ICONWARNING | MB_DEFBUTTON2 | MB_SETFOREGROUND | MB_TOPMOST,
        )
    };
    answer == IDOK
}
