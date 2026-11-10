//! HellLocker 1.9.4: Settings > Help > Check my setup. Looks at what most often stops
//! saving, applying and scanning, and says what to change, without touching the game.
use std::path::Path;

use crate::ui::SetupLevel;

/// One finding: how it went, a short title, and what it means or what to do.
pub struct Finding {
    pub level: SetupLevel,
    pub title: String,
    pub detail: String,
}

fn finding(level: SetupLevel, title: impl Into<String>, detail: impl Into<String>) -> Finding {
    Finding {
        level,
        title: title.into(),
        detail: detail.into(),
    }
}

/// Programs that draw on top of games, with the name players know them by.
const OVERLAYS: [(&str, &str); 7] = [
    ("rtss.exe", "RivaTuner / MSI Afterburner FPS counter"),
    ("gameoverlayui64.exe", "Steam overlay"),
    ("gameoverlayui.exe", "Steam overlay"),
    ("overwolf.exe", "Overwolf"),
    ("medal.exe", "Medal"),
    ("fpsmonitor.exe", "FPS Monitor"),
    ("gamebar.exe", "Xbox Game Bar"),
];

pub fn run(presets: &Path) -> Vec<Finding> {
    use SetupLevel::{Good, Info, Problem};
    let mut findings = Vec::new();

    // The game's window.
    // HellLocker 1.9.7: on Linux (Wine / Proton) say how it can see the game.
    #[cfg(target_os = "windows")]
    let wine = crate::platform::system::wine_version();
    #[cfg(not(target_os = "windows"))]
    let wine: Option<String> = None;
    if let Some(version) = &wine {
        findings.push(finding(
            Info,
            "Running on Linux (Wine / Proton) — experimental",
            format!(
                "Wine {version}. HellLocker must run inside the game's own Proton prefix to see it, and needs an X11 session (not Wayland). See \"Linux and Steam Deck\" in the README."
            ),
        ));
    }
    match crate::game_window::find_game_window() {
        Err(_) if wine.is_some() => findings.push(finding(
            Problem,
            "HellLocker can't see Helldivers 2",
            "If the game is running, HellLocker was started outside the game's Proton prefix. Close it and start it with: protontricks-launch --appid 553850 HellLocker.exe",
        )),
        Err(_) => findings.push(finding(
            Info,
            "Helldivers 2 is not running",
            "Start the game and check again to check its window and resolution.",
        )),
        Ok(game) => {
            #[cfg(target_os = "windows")]
            match crate::platform::system::game_window_layout(game.handle()) {
                Some(layout) => window_findings(&mut findings, &layout),
                None => findings.push(finding(
                    Info,
                    "Could not read the game's window",
                    "Try again with the game open.",
                )),
            }
            #[cfg(not(target_os = "windows"))]
            let _ = game;
            if let Err(error) = crate::permissions::ensure_input_access() {
                let text = format!("{error:#}");
                if text.contains("higher privileges") {
                    findings.push(finding(
                        Problem,
                        "The game runs as administrator",
                        "Windows will not let HellLocker click in it. Run the game normally, or start HellLocker as administrator too.",
                    ));
                }
            }
        }
    }

    // The game's own settings.
    match crate::game_settings::read_color_settings() {
        Ok(settings) if settings.hdr_enabled => findings.push(finding(
            Info,
            "HDR is on in the game",
            "HellLocker corrects for HDR. If saving or applying has trouble reading the screen, try it with HDR off.",
        )),
        Ok(_) => findings.push(finding(Good, "HDR is off", "")),
        Err(_) => {}
    }

    // Equipment pictures.
    let library = crate::preset::library::Library::load(presets);
    let state = crate::preset::library::ScanState::peek(presets);
    if state.is_some() {
        findings.push(finding(
            Info,
            "An equipment scan stopped partway",
            "Press Scan equipment to carry on from where it stopped.",
        ));
    } else if library.is_empty() {
        findings.push(finding(
            Problem,
            "Equipment has not been scanned yet",
            "On a loadout page, press Scan equipment once and leave the game alone for a few minutes. Saving is quicker and presets show their gear.",
        ));
    } else {
        findings.push(finding(
            Good,
            format!("Equipment scanned ({} pictures)", library.len()),
            "Scan again after game updates add new gear.",
        ));
    }

    // Programs that draw over the game.
    #[cfg(target_os = "windows")]
    {
        let running = crate::platform::system::running_programs();
        let mut found: Vec<&str> = OVERLAYS
            .iter()
            .filter(|(exe, _)| running.iter().any(|name| name.eq_ignore_ascii_case(exe)))
            .map(|(_, label)| *label)
            .collect();
        found.dedup();
        if found.is_empty() {
            findings.push(finding(Good, "No known overlays found", ""));
        } else {
            findings.push(finding(
                Info,
                format!("Overlays running: {}", found.join(", ")),
                "If one draws over the loadout screen (an FPS counter, say), turn it off or move it to a corner the loadout screen does not use.",
            ));
        }
        if let Some(lines) = crate::platform::system::wheel_scroll_lines()
            && lines != 3
        {
            findings.push(finding(
                Info,
                format!("Mouse wheel scrolls {lines} lines (Windows' default is 3)"),
                "If lists scroll too far while applying, set it back to 3 in Windows mouse settings.",
            ));
        }
    }

    if crate::timing::patient() {
        findings.push(finding(
            Info,
            "Patient mode is on",
            "Waits up to three times longer for the game. Turn it off in Gameplay if your PC keeps up.",
        ));
    }
    findings
}

#[cfg(target_os = "windows")]
fn window_findings(
    findings: &mut Vec<Finding>,
    layout: &crate::platform::system::GameWindowLayout,
) {
    use SetupLevel::{Good, Info, Problem};
    let (w, h) = layout.client;
    let (mw, mh) = layout.monitor;
    if layout.has_title_bar {
        findings.push(finding(
            Problem,
            "The game is in a window",
            "Set Display Mode to Borderless Window in the game's options.",
        ));
    } else if (w, h) == (mw, mh) {
        findings.push(finding(
            Good,
            "Borderless Window or Fullscreen",
            "Borderless Window works best. If Save or Apply cannot see the game, switch from Fullscreen to Borderless Window.",
        ));
    }
    if (w, h) == (mw, mh) {
        findings.push(finding(
            Good,
            format!("Game resolution {w} × {h}, same as the display"),
            "",
        ));
    } else {
        findings.push(finding(
            Problem,
            format!("Game resolution {w} × {h} on a {mw} × {mh} display"),
            "Set the game's resolution to your desktop resolution.",
        ));
    }
    if w < 1280 || h < 720 {
        findings.push(finding(
            Problem,
            "The game window is very small",
            "Below 1280 × 720 the text is too small to read reliably.",
        ));
    } else if w * 9 > h * 16 + h {
        findings.push(finding(
            Info,
            "Wide screen",
            "Ultrawide works; the loadout screen sits in the middle.",
        ));
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;
    use crate::platform::system::GameWindowLayout;

    fn check(client: (u32, u32), monitor: (u32, u32), has_title_bar: bool) -> Vec<Finding> {
        let mut findings = Vec::new();
        window_findings(
            &mut findings,
            &GameWindowLayout {
                client,
                monitor,
                has_title_bar,
            },
        );
        findings
    }

    fn problems(findings: &[Finding]) -> usize {
        findings
            .iter()
            .filter(|finding| finding.level == SetupLevel::Problem)
            .count()
    }

    #[test]
    fn window_checks() {
        // Borderless at the desktop resolution: nothing to fix.
        assert_eq!(problems(&check((2560, 1440), (2560, 1440), false)), 0);
        // A window with a title bar, smaller than the display: two things to fix.
        assert_eq!(problems(&check((1600, 900), (2560, 1440), true)), 2);
        // Tiny: also flagged.
        assert!(problems(&check((1024, 576), (1024, 576), false)) == 1);
        // Ultrawide is fine, with a note.
        let wide = check((3440, 1440), (3440, 1440), false);
        assert_eq!(problems(&wide), 0);
        assert!(wide.iter().any(|finding| finding.title == "Wide screen"));
    }
}
