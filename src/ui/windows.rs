//! Windows-specific window, display and font choices stay at the UI platform boundary.
use anyhow::Result;
use slint::winit_030::winit::{
    platform::windows::WindowAttributesExtWindows,
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::{Window, WindowAttributes},
};
use windows::Win32::{
    Foundation::{HWND, POINT},
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint},
    System::Threading::{AttachThreadInput, GetCurrentThreadId},
    UI::Input::KeyboardAndMouse::SetFocus,
    UI::WindowsAndMessaging::{
        BringWindowToTop, GWL_EXSTYLE, GetForegroundWindow, GetWindowLongPtrW,
        GetWindowThreadProcessId, IsIconic, SW_RESTORE, SetForegroundWindow, SetWindowLongPtrW,
        ShowWindow, WS_EX_NOACTIVATE,
    },
};

pub(super) fn window_attributes(attributes: WindowAttributes) -> WindowAttributes {
    attributes.with_skip_taskbar(true)
}

pub(super) fn font_family(locale: &str) -> &'static str {
    match legible_font(locale) {
        // HellLocker: the bundled panel font (Latin only; CJK keeps its system font).
        "Segoe UI" => "Liberation Sans",
        cjk => cjk,
    }
}

/// HellLocker: a plain system font for the Settings dialog and language names.
pub(super) fn legible_font(locale: &str) -> &'static str {
    match locale {
        "zh-Hans" => "Microsoft YaHei UI",
        "zh-Hant" => "Microsoft JhengHei UI",
        "ja" => "Yu Gothic UI",
        "ko" => "Malgun Gothic",
        _ => "Segoe UI",
    }
}

pub(super) fn make_passive(window: &Window) -> Result<()> {
    let RawWindowHandle::Win32(handle) = window.window_handle()?.as_raw() else {
        unreachable!("Windows UI has a Win32 handle");
    };
    let hwnd = HWND(handle.hwnd.get() as *mut _);
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | WS_EX_NOACTIVATE.0 as isize);
    }
    Ok(())
}

/// HellLocker: brings the panel to the front and gives it the keyboard, without
/// pressing any keys. (winit's `focus_window` taps a fake Alt key to get around
/// Windows' focus rules, which other programs see: Discord, for one, took it as a
/// mute or push-to-talk keybind.) The panel is shown in answer to the user's own
/// shortcut or tray click, so Windows normally allows this; if it does not, the
/// panel briefly shares the current foreground window's input to hand focus over.
pub(super) fn bring_to_front(window: &Window) -> Result<()> {
    let RawWindowHandle::Win32(handle) = window.window_handle()?.as_raw() else {
        unreachable!("Windows UI has a Win32 handle");
    };
    let hwnd = HWND(handle.hwnd.get() as *mut _);
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        if GetForegroundWindow() == hwnd || SetForegroundWindow(hwnd).as_bool() {
            return Ok(());
        }
        let foreground = GetForegroundWindow();
        let foreground_thread = GetWindowThreadProcessId(foreground, None);
        let own_thread = GetCurrentThreadId();
        if foreground_thread != 0 && foreground_thread != own_thread {
            let attached = AttachThreadInput(own_thread, foreground_thread, true).as_bool();
            let _ = BringWindowToTop(hwnd);
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(Some(hwnd));
            if attached {
                let _ = AttachThreadInput(own_thread, foreground_thread, false);
            }
        }
        if GetForegroundWindow() != hwnd {
            tracing::debug!("Windows kept another window in front of the panel");
        }
    }
    Ok(())
}

pub(super) fn monitor_bounds(id: &str) -> Result<(i32, i32, i32, i32)> {
    let bounds = crate::platform::monitors::resolve_bounds(Some(id), None)?;
    Ok((
        bounds.left,
        bounds.top,
        bounds.right - bounds.left,
        bounds.bottom - bounds.top,
    ))
}

pub(super) fn monitor_choices() -> Result<Vec<super::MonitorChoice>> {
    Ok(crate::platform::monitors::available()?
        .into_iter()
        .map(|monitor| super::MonitorChoice {
            id: monitor.id.into(),
            role: super::MonitorRole::Available,
            number: monitor.source.trim_start_matches(r"\\.\DISPLAY").into(),
            name: monitor.name.into(),
            primary: monitor.primary,
        })
        .collect())
}

pub(super) fn work_area(bounds: (i32, i32, i32, i32)) -> Result<(i32, i32, i32, i32)> {
    let point = POINT {
        x: bounds.0 + bounds.2 / 2,
        y: bounds.1 + bounds.3 / 2,
    };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe { GetMonitorInfoW(MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST), &mut info) }
        .ok()?;
    let rect = info.rcWork;
    Ok((
        rect.left,
        rect.top,
        rect.right - rect.left,
        rect.bottom - rect.top,
    ))
}
