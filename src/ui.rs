//! Interactive presentation and its event loop. No capture, injected input or game-window APIs.
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use anyhow::Result;
use slint::winit_030::{WinitWindowAccessor, winit::event::WindowEvent};
use slint::{ComponentHandle, Model, Rgba8Pixel, SharedPixelBuffer, VecModel};

use crate::app_events::AppEvent;
use crate::config::{SettingsDraft, ShortcutBindings, UiLanguage};
use crate::context::{Faction, Mission, Subfaction};
use crate::folders::{Drop, Layout};
use crate::item::EquipmentKind;
use crate::preset::PresetScope;
use crate::preset::preview::{PresetPreview, PreviewContent, PreviewRole as PreviewImageRole};

mod icons;
mod settings;
mod status;
#[cfg(target_os = "windows")]
mod windows;

slint::include_modules!();

#[derive(Clone, Default)]
pub struct PresetInfo {
    pub name: String,
    pub label: String,
    pub shortcut: String,
    pub faction: Faction,
    pub subfaction: Subfaction,
    pub mission: Mission,
    /// HellLocker 1.9.4
    pub favorite: bool,
    pub notes: String,
    /// HellLocker 1.9.5: the icon ids beside the name ("" = the slot's default).
    pub icons: [String; crate::preset_icons::SLOTS],
}

/// HellLocker: the Help tab's buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpAction {
    ReportBug,
    OpenDataFolder,
    CopySystemInfo,
    Backup,
    Restore,
    CheckSetup,
}

pub enum UiRequest {
    OpenPanel,
    Exit,
    Action {
        preset: String,
        saving: bool,
        scope: Option<PresetScope>,
    },
    SetScope(PresetScope),
    Rename {
        preset: String,
        label: String,
    },
    OpenSettings,
    SaveSettings(SettingsDraft),
    AddPreset,
    DeletePreset(String),
    HidePanel,
    /// HellLocker: the panel's faction / mission filter changed.
    SetView {
        faction: Faction,
        subfaction: Subfaction,
        mission: Mission,
    },
    /// HellLocker: Scan equipment was pressed.
    Scan,
    /// HellLocker: the preset search text changed.
    Search(String),
    /// HellLocker: the Helmet, Armor & Cape switch changed.
    SetAttire(crate::item::AttireChoice),
    /// HellLocker: a change to the preset list's folders or order.
    Folder(FolderRequest),
    /// HellLocker: a button on Settings > Help.
    Help(HelpAction),
    /// HellLocker 1.9.4: star or unstar a preset.
    SetFavorite {
        preset: String,
        favorite: bool,
    },
    /// HellLocker 1.9.4: a preset's notes were edited.
    SetNotes {
        preset: String,
        notes: String,
    },
    /// HellLocker 1.9.4: make a copy of a preset.
    Duplicate(String),
    /// HellLocker 1.9.4: open or close Favourites (or, with `recent`, Recently used).
    OpenPinned {
        recent: bool,
        open: bool,
    },
    /// HellLocker 1.9.4: fold the right side away, or bring it back.
    SetCompact(bool),
    /// HellLocker 1.9.4: the panel's Stop button.
    Stop,
    /// HellLocker 1.9.5: a preset's icon in `slot` was picked ("" = back to default).
    SetIcon {
        preset: String,
        slot: usize,
        id: String,
    },
    /// HellLocker 1.9.5: the icon picker opened, or its search / group changed.
    IconPicker,
    IconFilter {
        search: String,
        group: i32,
    },
    /// HellLocker 1.9.5: open the folder for the player's own icons.
    OpenIconsFolder,
    /// HellLocker: new faction / mission tags for one preset.
    SetTags {
        preset: String,
        faction: Faction,
        subfaction: Subfaction,
        mission: Mission,
    },
}

/// HellLocker: what the folder controls in the panel asked for.
pub enum FolderRequest {
    New,
    Rename {
        id: String,
        name: String,
    },
    Delete(String),
    Open {
        id: String,
        open: bool,
    },
    /// HellLocker 1.9.6: a folder's logo was picked ("" = back to the folder symbol).
    SetIcon {
        id: String,
        icon: String,
    },
    /// A preset dropped at `y` (logical pixels from the top of the list); `column` is
    /// how far across the list, in columns (1.5 = the middle of the second column).
    Move {
        preset: String,
        column: f32,
        y: f32,
    },
}

// HellLocker: the preset list's row geometry, matching panel.slint.
const FOLDER_HEIGHT: f32 = 34.0;
const ROW_HEIGHT: f32 = 34.0;
const ROW_GAP: f32 = 3.0;
/// HellLocker 1.9.6: presets sit in two columns, filled left to right.
pub(crate) const COLUMNS: usize = 2;

/// HellLocker: one shown row of the preset list, for finding where a drop lands.
#[derive(Debug, Clone, PartialEq)]
enum ListEntry {
    Folder {
        id: String,
        top: f32,
    },
    Preset {
        name: String,
        top: f32,
        column: usize,
    },
    /// HellLocker 1.9.4: Favourites / Recently used, which nothing can be dropped into.
    Pinned {
        top: f32,
        height: f32,
    },
}

pub struct AppUi {
    window: PresetPanel,
    _tray: AppTray,
    requests: Receiver<UiRequest>,
    preset_info: BTreeMap<String, PresetInfo>,
    /// Every preset row; the panel only gets the ones the current filter shows.
    rows: Vec<PresetRow>,
    /// HellLocker: the folders and order of the preset list.
    layout: Layout,
    /// HellLocker: the rows last shown, top to bottom.
    entries: std::cell::RefCell<Vec<ListEntry>>,
    view: (Faction, Subfaction, Mission),
    /// HellLocker: lower-case search text; empty shows every preset the filter allows.
    search: String,
    status: status::Status,
    monitor: String,
    language: UiLanguage,
    anchor: Option<(i32, i32)>,
    placement_dirty: Rc<Cell<bool>>,
    /// HellLocker: where the user dragged the panel (outer position, desktop pixels).
    position: Rc<Cell<Option<(i32, i32)>>>,
    /// Where HellLocker itself last put the panel, so its own moves aren't mistaken for drags.
    placed_at: Rc<Cell<Option<(i32, i32)>>>,
    /// When the last drag happened and it has not been saved yet.
    moved_at: Rc<Cell<Option<Instant>>>,
    /// HellLocker 1.9.4: the presets applied last (newest first), and whether the
    /// Favourites and Recently used sections are open.
    recent: Vec<String>,
    favorites_open: bool,
    recent_open: bool,
    /// HellLocker 1.9.5: the icons a preset can show, and the pictures read so far.
    icons: crate::preset_icons::Catalog,
    icon_cache: RefCell<std::collections::HashMap<String, Option<slint::Image>>>,
}

impl AppUi {
    pub fn new(
        preset_info: Vec<PresetInfo>,
        monitor: String,
        language: UiLanguage,
        presets: PathBuf,
    ) -> Result<Self> {
        // Both surfaces are created without activation. Only the interactive
        // panel explicitly requests focus; status updates never do.
        slint::BackendSelector::new()
            .backend_name("winit".into())
            .with_winit_window_attributes_hook(|attributes| {
                let attributes = attributes.with_active(false);
                #[cfg(target_os = "windows")]
                let attributes = windows::window_attributes(attributes);
                attributes
            })
            .select()?;
        let window = PresetPanel::new()?;
        window.set_version(env!("CARGO_PKG_VERSION").into());
        #[cfg(target_os = "windows")]
        window.set_language_fonts(
            UiLanguage::ALL
                .map(|language| {
                    language
                        .locale()
                        .map(windows::legible_font)
                        .unwrap_or("")
                        .into()
                })
                .into(),
        );
        let (send, requests) = mpsc::channel();
        let tray = AppTray::new()?;
        let icon = image::load_from_memory(include_bytes!("../assets/app-icon.ico"))?.into_rgba8();
        tray.set_tray_icon(image(&icon));
        tray.set_version(env!("CARGO_PKG_VERSION").into());
        let open_send = send.clone();
        tray.on_open_panel(move || {
            let _ = open_send.send(UiRequest::OpenPanel);
        });
        let exit_send = send.clone();
        tray.on_exit(move || {
            let _ = exit_send.send(UiRequest::Exit);
        });
        let placement_dirty = Rc::new(Cell::new(false));
        let dirty = placement_dirty.clone();
        let position = Rc::new(Cell::new(None));
        let placed_at = Rc::new(Cell::new(None));
        let moved_at = Rc::new(Cell::new(None));
        let (dragged, placed, moved) = (position.clone(), placed_at.clone(), moved_at.clone());
        let mut handle_keys = settings::connect(&window, send.clone());
        window.window().on_winit_window_event(move |_, event| {
            match event {
                WindowEvent::ScaleFactorChanged { .. } => dirty.set(true),
                WindowEvent::Moved(at) if placed.get() != Some((at.x, at.y)) => {
                    // Only the title bar moves the panel, so this is the user's drag.
                    dragged.set(Some((at.x, at.y)));
                    moved.set(Some(Instant::now()));
                }
                _ => {}
            }
            handle_keys(event)
        });
        let add_send = send.clone();
        window.on_add_preset(move || {
            let _ = add_send.send(UiRequest::AddPreset);
        });
        let delete_send = send.clone();
        window.on_request_delete(move |preset| {
            let _ = delete_send.send(UiRequest::DeletePreset(preset.to_string()));
        });
        let weak = window.as_weak();
        let action_send = send.clone();
        window.on_request_action(move |saving, scope| {
            if let Some(window) = weak.upgrade()
                && window.invoke_action_available(saving, scope)
                && let Some(entry) = window
                    .get_presets()
                    .row_data(window.get_current_index() as usize)
            {
                let _ = action_send.send(UiRequest::Action {
                    preset: entry.name.to_string(),
                    saving,
                    scope: match scope {
                        ActionScope::Selected => None,
                        ActionScope::Stratagems => Some(PresetScope {
                            stratagems: true,
                            equipment: false,
                        }),
                        ActionScope::Equipment => Some(PresetScope {
                            stratagems: false,
                            equipment: true,
                        }),
                    },
                });
            }
        });
        let scope_send = send.clone();
        window.on_request_scope(move |stratagems, equipment| {
            let _ = scope_send.send(UiRequest::SetScope(PresetScope {
                stratagems,
                equipment,
            }));
        });
        let rename_send = send.clone();
        let weak = window.as_weak();
        window.on_request_rename(move |preset, label| {
            let label = label.trim();
            if let Some(window) = weak.upgrade()
                && window
                    .get_presets()
                    .iter()
                    .any(|row| row.name == preset && row.label != label)
            {
                let _ = rename_send.send(UiRequest::Rename {
                    preset: preset.to_string(),
                    label: label.to_owned(),
                });
            }
        });
        let view_send = send.clone();
        window.on_request_view(move |faction, subfaction, mission| {
            let faction = Faction::from_index(faction);
            let _ = view_send.send(UiRequest::SetView {
                faction,
                subfaction: Subfaction::from_index(subfaction).within(faction),
                mission: Mission::from_index(mission),
            });
        });
        let tags_send = send.clone();
        window.on_request_tags(move |preset, faction, subfaction, mission| {
            let faction = Faction::from_index(faction);
            let _ = tags_send.send(UiRequest::SetTags {
                preset: preset.to_string(),
                faction,
                subfaction: Subfaction::from_index(subfaction).within(faction),
                mission: Mission::from_index(mission),
            });
        });
        let folder_send = send.clone();
        window.on_request_new_folder(move || {
            let _ = folder_send.send(UiRequest::Folder(FolderRequest::New));
        });
        let folder_send = send.clone();
        window.on_request_rename_folder(move |id, name| {
            let _ = folder_send.send(UiRequest::Folder(FolderRequest::Rename {
                id: id.to_string(),
                name: name.to_string(),
            }));
        });
        let folder_send = send.clone();
        window.on_request_delete_folder(move |id| {
            let _ = folder_send.send(UiRequest::Folder(FolderRequest::Delete(id.to_string())));
        });
        let folder_send = send.clone();
        window.on_request_folder_icon(move |id, icon| {
            let _ = folder_send.send(UiRequest::Folder(FolderRequest::SetIcon {
                id: id.to_string(),
                icon: icon.to_string(),
            }));
        });
        let folder_send = send.clone();
        window.on_request_open_folder(move |id, open| {
            let _ = folder_send.send(UiRequest::Folder(FolderRequest::Open {
                id: id.to_string(),
                open,
            }));
        });
        let folder_send = send.clone();
        window.on_request_move(move |preset, column, y| {
            let _ = folder_send.send(UiRequest::Folder(FolderRequest::Move {
                preset: preset.to_string(),
                column,
                y,
            }));
        });
        // HellLocker 1.9.6: the row the arrow keys move to in the two-column list.
        let weak = window.as_weak();
        window.on_neighbour(move |index, dx, dy| {
            weak.upgrade().map_or(index, |window| {
                let rows: Vec<(f32, i32)> = window
                    .get_presets()
                    .iter()
                    .map(|row| (row.top, row.column))
                    .collect();
                neighbour(&rows, index, dx, dy)
            })
        });
        let list_send = send.clone();
        window.on_request_favorite(move |preset, favorite| {
            let _ = list_send.send(UiRequest::SetFavorite {
                preset: preset.to_string(),
                favorite,
            });
        });
        let list_send = send.clone();
        window.on_request_notes(move |preset, notes| {
            let _ = list_send.send(UiRequest::SetNotes {
                preset: preset.to_string(),
                notes: notes.to_string(),
            });
        });
        let list_send = send.clone();
        window.on_request_duplicate(move |preset| {
            let _ = list_send.send(UiRequest::Duplicate(preset.to_string()));
        });
        let list_send = send.clone();
        window.on_request_open_pinned(move |kind, open| {
            let _ = list_send.send(UiRequest::OpenPinned {
                recent: kind == 2,
                open,
            });
        });
        let list_send = send.clone();
        window.on_request_compact(move |compact| {
            let _ = list_send.send(UiRequest::SetCompact(compact));
        });
        let list_send = send.clone();
        window.on_request_icon(move |preset, slot, id| {
            let _ = list_send.send(UiRequest::SetIcon {
                preset: preset.to_string(),
                slot: slot.clamp(0, crate::preset_icons::SLOTS as i32 - 1) as usize,
                id: id.to_string(),
            });
        });
        let list_send = send.clone();
        window.on_request_icon_picker(move || {
            let _ = list_send.send(UiRequest::IconPicker);
        });
        let list_send = send.clone();
        window.on_request_icon_filter(move |search, group| {
            let _ = list_send.send(UiRequest::IconFilter {
                search: search.to_string(),
                group,
            });
        });
        let list_send = send.clone();
        window.on_request_icons_folder(move || {
            let _ = list_send.send(UiRequest::OpenIconsFolder);
        });
        let list_send = send.clone();
        window.on_request_stop(move || {
            let _ = list_send.send(UiRequest::Stop);
        });
        let search_send = send.clone();
        window.on_request_search(move |text| {
            let _ = search_send.send(UiRequest::Search(text.to_string()));
        });
        let scan_send = send.clone();
        window.on_request_scan(move || {
            let _ = scan_send.send(UiRequest::Scan);
        });
        let attire_send = send.clone();
        window.on_request_attire(move |helmet, armor, cape| {
            let _ = attire_send.send(UiRequest::SetAttire(crate::item::AttireChoice {
                helmet,
                armor,
                cape,
            }));
        });
        let close_send = send.clone();
        window.on_dismiss(move || {
            let _ = close_send.send(UiRequest::HidePanel);
        });
        window.window().on_close_requested(move || {
            let _ = send.send(UiRequest::HidePanel);
            slint::CloseRequestResponse::KeepWindowShown
        });
        let data_dir = presets
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf();
        let mut ui = Self {
            status: status::Status::new(presets)?,
            monitor,
            language,
            anchor: None,
            placement_dirty,
            position,
            placed_at,
            moved_at,
            window,
            _tray: tray,
            requests,
            rows: Vec::new(),
            layout: Layout::default(),
            entries: Default::default(),
            view: (Faction::Any, Subfaction::Any, Mission::Any),
            search: String::new(),
            recent: Vec::new(),
            favorites_open: true,
            recent_open: true,
            icons: crate::preset_icons::Catalog::new(&data_dir),
            icon_cache: Default::default(),
            preset_info: preset_info
                .into_iter()
                .map(|info| (info.name.clone(), info))
                .collect(),
        };
        ui.set_language(language)?;
        ui.set_previews(BTreeMap::new());
        Ok(ui)
    }

    pub fn show_panel(&mut self, anchor: Option<(i32, i32)>) -> Result<()> {
        self.anchor = anchor;
        self.status.hide()?;
        self.window.invoke_focus_list();
        self.window.set_pending(false);
        self.window.set_loading(true);
        self.window.set_status("".into());
        self.window.show()?;
        self.reposition()?;
        self.window.window().with_winit_window(|window| {
            window.set_minimized(false);
            // HellLocker: not winit's focus_window, which taps a fake Alt key that
            // other programs (Discord keybinds) react to.
            #[cfg(target_os = "windows")]
            if let Err(error) = windows::bring_to_front(window) {
                tracing::debug!(error = %format!("{error:#}"), "could not bring the panel to the front");
            }
            #[cfg(not(target_os = "windows"))]
            window.focus_window();
        });
        self.window.invoke_focus_list();
        Ok(())
    }

    /// HellLocker: show the panel again after an action without taking focus from the game.
    pub fn reveal_panel(&mut self) -> Result<()> {
        if self.is_panel_visible() {
            return Ok(());
        }
        self.status.hide()?;
        self.window.set_status("".into());
        self.window.show()?;
        self.reposition()
    }

    /// HellLocker: the panel's outer rectangle in desktop pixels, while it is shown.
    pub fn panel_rect(&self) -> Option<(i32, i32, i32, i32)> {
        if !self.is_panel_visible() {
            return None;
        }
        self.window.window().with_winit_window(|window| {
            let at = window.outer_position().ok()?;
            let size = window.outer_size();
            Some((at.x, at.y, size.width as i32, size.height as i32))
        })?
    }

    /// HellLocker: the position saved last time (None = top right of the game's display).
    pub fn set_saved_position(&self, position: Option<(i32, i32)>) {
        self.position.set(position);
    }

    /// HellLocker: a drag that has settled for a second and should be saved.
    pub fn take_moved_position(&self) -> Option<Option<(i32, i32)>> {
        let settled = self
            .moved_at
            .get()
            .is_some_and(|at| at.elapsed() >= Duration::from_secs(1));
        settled.then(|| {
            self.moved_at.set(None);
            self.position.get()
        })
    }

    pub fn hide_panel(&self) -> Result<()> {
        self.window.invoke_finish_rename(true);
        self.window.invoke_close_dialogs();
        self.window.hide()?;
        Ok(())
    }

    pub fn is_panel_visible(&self) -> bool {
        self.window.window().is_visible()
    }

    pub fn set_anchor(&mut self, anchor: Option<(i32, i32)>) {
        self.anchor = anchor;
    }

    pub fn set_monitor(&mut self, monitor: String) -> Result<()> {
        self.monitor = monitor;
        if self.is_panel_visible() {
            self.reposition()?;
        }
        self.status.reposition(&self.monitor, self.anchor)
    }

    fn reposition(&self) -> Result<()> {
        let size = slint::LogicalSize::new(
            self.window.get_ideal_width(),
            self.window.get_ideal_height(),
        );
        let placed = match self.position.get() {
            // HellLocker: back where the user dragged it, if that display is still there.
            Some(at) => match place_at(self.window.window(), at, size)? {
                Some(placed) => Some(placed),
                None => {
                    self.position.set(None);
                    self.moved_at.set(Some(Instant::now()));
                    place_top_right(
                        self.window.window(),
                        &self.monitor,
                        self.anchor,
                        24.0,
                        Some(size),
                    )?
                    .map(|size| (size, self.outer_position()))
                }
            },
            None => place_top_right(
                self.window.window(),
                &self.monitor,
                self.anchor,
                24.0,
                Some(size),
            )?
            .map(|size| (size, self.outer_position())),
        };
        if let Some((size, at)) = placed {
            self.placed_at.set(at);
            self.window.set_available_width(size.width);
            self.window.set_available_height(size.height);
        }
        Ok(())
    }

    fn outer_position(&self) -> Option<(i32, i32)> {
        self.window
            .window()
            .with_winit_window(|window| window.outer_position().ok().map(|at| (at.x, at.y)))?
    }

    pub fn monitor(&self) -> &str {
        &self.monitor
    }

    pub fn language(&self) -> UiLanguage {
        self.language
    }

    pub fn set_language(&mut self, language: UiLanguage) -> Result<()> {
        let locale = language.locale().unwrap_or_else(|| {
            sys_locale::get_locales()
                .find_map(|locale| UiLanguage::from_locale(&locale).and_then(UiLanguage::locale))
                .unwrap_or("en")
        });
        slint::select_bundled_translation(locale)?;
        #[cfg(target_os = "windows")]
        let (font_family, settings_font) =
            (windows::font_family(locale), windows::legible_font(locale));
        #[cfg(not(target_os = "windows"))]
        let (font_family, settings_font) = ("", "");
        self.window.set_ui_font_family(font_family.into());
        self.window
            .global::<Fonts>()
            .set_settings(settings_font.into());
        self.status.set_font_family(font_family);
        self.language = language;
        Ok(())
    }

    pub fn open_settings(&self, draft: SettingsDraft) {
        self.window.invoke_finish_rename(true);
        // Every preset, not just the filtered list, so hidden shortcuts are kept.
        settings::open(&self.window, draft, &self.rows);
    }

    /// HellLocker: whether helmet, armor and cape are saved and applied.
    pub fn set_attire(&self, attire: crate::item::AttireChoice) {
        self.window.set_scope_helmet(attire.helmet);
        self.window.set_scope_armor(attire.armor);
        self.window.set_scope_cape(attire.cape);
    }

    /// HellLocker: how opaque the background art is (percent, 10 to 100).
    pub fn set_background_opacity(&self, percent: i64) {
        self.window
            .set_background_opacity(percent.clamp(10, 100) as f32 / 100.0);
    }

    /// HellLocker: show only presets whose name contains `text` (any case).
    pub fn set_search(&mut self, text: &str) {
        let text = text.trim().to_lowercase();
        if text != self.search {
            self.search = text;
            self.publish(None);
        }
    }

    /// HellLocker: the filter the panel shows now.
    pub fn view(&self) -> (Faction, Subfaction, Mission) {
        self.view
    }

    /// HellLocker: show only presets for this faction and mission (plus general ones).
    pub fn set_view(&mut self, faction: Faction, subfaction: Subfaction, mission: Mission) {
        let subfaction = subfaction.within(faction);
        self.view = (faction, subfaction, mission);
        self.window.set_view_faction(faction.index());
        self.window.set_view_subfaction(subfaction.index());
        self.window.set_view_mission(mission.index());
        self.publish(None);
    }

    /// HellLocker: retag one preset; it leaves the list if the filter no longer shows it.
    pub fn set_tags(
        &mut self,
        preset: &str,
        faction: Faction,
        subfaction: Subfaction,
        mission: Mission,
    ) {
        let subfaction = subfaction.within(faction);
        if let Some(info) = self.preset_info.get_mut(preset) {
            info.faction = faction;
            info.subfaction = subfaction;
            info.mission = mission;
        }
        if let Some(row) = self.rows.iter_mut().find(|row| row.name == preset) {
            row.faction = faction.index();
            row.subfaction = subfaction.index();
            row.mission = mission.index();
        }
        self.publish(Some(preset));
        self.set_status("");
    }

    /// Rebuild the panel list from what is stored (after a failed edit).
    pub fn refresh(&self) {
        self.publish(None);
    }

    fn selected_name(&self) -> Option<String> {
        self.window
            .get_presets()
            .row_data(self.window.get_current_index() as usize)
            .map(|row| row.name.to_string())
    }

    /// Push the rows the filter shows to the panel, keeping `select` (or the current
    /// selection) selected when it is still listed.
    fn publish(&self, select: Option<&str>) {
        self.publish_icon_room();
        let keep = select.map(str::to_owned).or_else(|| self.selected_name());
        let (faction, subfaction, mission) = self.view;
        let shows = |row: &PresetRow| {
            Faction::from_index(row.faction).shows_in(faction)
                && Subfaction::from_index(row.subfaction).shows_in(subfaction)
                && Mission::from_index(row.mission).shows_in(mission)
                && matches_search(&self.search, &row.label, &row.number)
        };
        // HellLocker: while filtering or searching, a folder only shows when it holds a
        // matching preset. HellLocker 1.9.6: only a search opens closed folders (and
        // the pinned sections); with a faction or mission filter they open and close
        // as usual.
        let filtering = (faction, subfaction, mission)
            != (Faction::Any, Subfaction::Any, Mission::Any)
            || !self.search.is_empty();
        let searching = !self.search.is_empty();
        let renaming = self.window.get_renaming_folder();
        let by_name: BTreeMap<&str, &PresetRow> = self
            .rows
            .iter()
            .map(|row| (row.name.as_str(), row))
            .collect();
        let mut visible: Vec<PresetRow> = Vec::new();
        let mut folders = Vec::new();
        let mut entries = Vec::new();
        let mut top = 0.0_f32;
        // HellLocker 1.9.4: Favourites and Recently used at the top. Their rows are
        // copies (the presets stay in their folders too), and nothing drops into them.
        let favorites: Vec<&str> = self
            .layout
            .folders
            .iter()
            .flat_map(|folder| folder.presets.iter())
            .chain(self.layout.loose.iter())
            .map(String::as_str)
            .filter(|name| by_name.get(name).is_some_and(|row| row.favorite))
            .collect();
        let recent: Vec<&str> = self.recent.iter().map(String::as_str).collect();
        for (kind, names, open) in [
            (1, favorites, self.favorites_open),
            (2, recent, self.recent_open),
        ] {
            let matching: Vec<&PresetRow> = names
                .iter()
                .filter_map(|name| by_name.get(name).copied())
                .filter(|row| shows(row))
                .collect();
            if matching.is_empty() {
                continue;
            }
            let open = open || searching;
            folders.push(FolderRow {
                id: format!("pinned-{kind}").into(),
                // The panel shows the section's (translated) name.
                name: "".into(),
                count: matching.len().to_string().into(),
                open,
                top,
                kind,
                ..Default::default()
            });
            entries.push(ListEntry::Pinned {
                top,
                height: FOLDER_HEIGHT + ROW_GAP,
            });
            top += FOLDER_HEIGHT + ROW_GAP;
            if open {
                for (index, row) in matching.iter().enumerate() {
                    let mut row = (*row).clone();
                    row.pinned = true;
                    row.column = (index % COLUMNS) as i32;
                    row.top = top + (index / COLUMNS) as f32 * (ROW_HEIGHT + ROW_GAP);
                    visible.push(row);
                }
                let height = bands(matching.len()) as f32 * (ROW_HEIGHT + ROW_GAP);
                entries.push(ListEntry::Pinned { top, height });
                top += height;
            }
        }
        for folder in &self.layout.folders {
            let matching: Vec<&PresetRow> = folder
                .presets
                .iter()
                .filter_map(|name| by_name.get(name.as_str()).copied())
                .filter(|row| shows(row))
                .collect();
            // The folder being named (a new one is empty) always shows.
            if filtering && matching.is_empty() && renaming != folder.id.as_str() {
                continue;
            }
            let open = folder.open || searching;
            let total = folder.presets.len();
            let count = if filtering && matching.len() < total {
                format!("{} / {total}", matching.len())
            } else {
                total.to_string()
            };
            let picture = self.icon_picture(&folder.icon);
            folders.push(FolderRow {
                id: folder.id.as_str().into(),
                name: folder.name.as_str().into(),
                count: count.into(),
                open,
                top,
                kind: 0,
                icon_id: folder.icon.as_str().into(),
                has_icon: picture.is_some(),
                icon: picture.unwrap_or_default(),
            });
            entries.push(ListEntry::Folder {
                id: folder.id.clone(),
                top,
            });
            top += FOLDER_HEIGHT + ROW_GAP;
            if open {
                place_rows(&mut visible, &mut entries, &matching, true, &mut top);
            }
        }
        let loose: Vec<&PresetRow> = self
            .layout
            .loose
            .iter()
            .filter_map(|name| by_name.get(name.as_str()).copied())
            .filter(|row| shows(row))
            .collect();
        place_rows(&mut visible, &mut entries, &loose, false, &mut top);
        // The row already selected stays selected when it still holds the preset (it
        // may be listed twice: in Favourites and in its folder).
        let current = self.window.get_current_index();
        let index = keep
            .and_then(|name| {
                usize::try_from(current)
                    .ok()
                    .filter(|&index| {
                        visible
                            .get(index)
                            .is_some_and(|row| row.name == name.as_str())
                    })
                    .or_else(|| visible.iter().position(|row| row.name == name.as_str()))
            })
            .map_or_else(
                || {
                    self.window
                        .get_current_index()
                        .min(visible.len().saturating_sub(1) as i32)
                        .max(0)
                },
                |index| index as i32,
            );
        *self.entries.borrow_mut() = entries;
        self.window.set_list_height(top);
        self.window
            .set_folders(Rc::new(VecModel::from(folders)).into());
        self.window
            .set_presets(Rc::new(VecModel::from(visible)).into());
        self.window.set_current_index(index);
    }

    /// HellLocker: the stored folders and order of the preset list.
    /// HellLocker 1.9.4: Recently used, and whether it and Favourites are open.
    pub fn set_pinned(&mut self, recent: Vec<String>, favorites_open: bool, recent_open: bool) {
        self.recent = recent;
        self.favorites_open = favorites_open;
        self.recent_open = recent_open;
        self.publish(None);
    }

    pub fn set_recent(&mut self, recent: Vec<String>) {
        self.recent = recent;
        self.publish(None);
    }

    pub fn recent(&self) -> &[String] {
        &self.recent
    }

    pub fn set_pinned_open(&mut self, recent: bool, open: bool) {
        if recent {
            self.recent_open = open;
        } else {
            self.favorites_open = open;
        }
        self.publish(None);
    }

    pub fn set_favorite(&mut self, preset: &str, favorite: bool) {
        if let Some(info) = self.preset_info.get_mut(preset) {
            info.favorite = favorite;
        }
        if let Some(row) = self.rows.iter_mut().find(|row| row.name == preset) {
            row.favorite = favorite;
        }
        self.publish(None);
    }

    pub fn set_notes(&mut self, preset: &str, notes: &str) {
        if let Some(info) = self.preset_info.get_mut(preset) {
            info.notes = notes.to_owned();
        }
        if let Some(row) = self.rows.iter_mut().find(|row| row.name == preset) {
            row.notes = notes.into();
        }
        self.publish(None);
    }

    /// HellLocker 1.9.4: the shown title of a preset (its name, or "Preset N").
    pub fn preset_title(&self, preset: &str) -> String {
        match self.preset_info.get(preset) {
            Some(info) if !info.label.is_empty() => info.label.clone(),
            _ => format!("Preset {}", preset_number(preset)),
        }
    }

    /// HellLocker 1.9.4: a copy of `from` was made as `to`; list it just below `from`,
    /// select it and start renaming it. Returns the layout to save.
    pub fn add_copy(&mut self, from: &str, to: String, label: String) -> Layout {
        let Some(source) = self.preset_info.get(from).cloned() else {
            return self.layout.clone();
        };
        let info = PresetInfo {
            name: to.clone(),
            label: label.clone(),
            shortcut: String::new(),
            favorite: false,
            ..source
        };
        let mut row = self
            .rows
            .iter()
            .find(|row| row.name == from)
            .cloned()
            .unwrap_or_default();
        row.name = to.as_str().into();
        row.number = preset_number(&to).into();
        row.label = label.into();
        row.shortcut = "".into();
        row.favorite = false;
        row.pinned = false;
        self.rows.push(row);
        self.preset_info.insert(to.clone(), info);
        self.layout.add_preset(&to);
        self.layout.move_preset(&to, &Drop::After(from.to_owned()));
        self.publish(Some(&to));
        self.window.invoke_focus_list();
        // After the panel has handled the new selection, which ends any rename.
        let weak = self.window.as_weak();
        slint::Timer::single_shot(Duration::from_millis(80), move || {
            if let Some(window) = weak.upgrade() {
                window.invoke_begin_rename();
            }
        });
        self.layout.clone()
    }

    /// HellLocker 1.9.4: fold the right side away (or back), keeping the panel's left
    /// edge where it is, so the list stays put and the details slide in from the right.
    pub fn set_compact(&mut self, compact: bool) -> Result<()> {
        if self.window.get_compact() == compact {
            return Ok(());
        }
        let at = self.outer_position();
        self.window.set_compact(compact);
        if self.is_panel_visible()
            && let Some(at) = at
        {
            let size = slint::LogicalSize::new(
                self.window.get_ideal_width(),
                self.window.get_ideal_height(),
            );
            if let Some((size, placed)) = place_at(self.window.window(), at, size)? {
                self.placed_at.set(placed);
                self.window.set_available_width(size.width);
                self.window.set_available_height(size.height);
            }
        }
        Ok(())
    }

    pub fn set_layout(&mut self, layout: Layout) {
        self.layout = layout;
        self.publish(None);
    }

    /// HellLocker: carries out a folder request; returns the new layout to save when
    /// something changed.
    pub fn folder_request(&mut self, request: FolderRequest) -> Option<Layout> {
        let mut select = None;
        let changed = match request {
            FolderRequest::New => {
                let id = self.layout.add_folder("");
                // Start renaming it straight away.
                self.window.set_renaming_folder(id.as_str().into());
                true
            }
            FolderRequest::Rename { id, name } => {
                self.window.set_renaming_folder("".into());
                let before = self.layout.folder(&id).map(|f| f.name.clone());
                self.layout.rename_folder(&id, &name)
                    && before != self.layout.folder(&id).map(|f| f.name.clone())
            }
            FolderRequest::Delete(id) => self.layout.delete_folder(&id),
            FolderRequest::SetIcon { id, icon } => self.layout.set_icon(&id, &icon),
            FolderRequest::Open { id, open } => {
                // Closing the folder that holds the selection selects the next row shown.
                if !open
                    && let Some(current) = self.selected_name()
                    && self.layout.folder_of(&current) == Some(id.as_str())
                {
                    select = self
                        .entries
                        .borrow()
                        .iter()
                        .skip_while(|entry| {
                            !matches!(entry, ListEntry::Folder { id: shown, .. } if *shown == id)
                        })
                        .filter_map(|entry| match entry {
                            ListEntry::Preset { name, .. } => Some(name.clone()),
                            ListEntry::Folder { .. } | ListEntry::Pinned { .. } => None,
                        })
                        .find(|name| self.layout.folder_of(name) != Some(id.as_str()));
                }
                self.layout.set_open(&id, open)
            }
            FolderRequest::Move { preset, column, y } => {
                select = Some(preset.clone());
                // Nothing lands in Favourites or Recently used.
                match drop_at(&self.entries.borrow(), column, y) {
                    Some(drop) => self.layout.move_preset(&preset, &drop),
                    None => false,
                }
            }
        };
        self.publish(select.as_deref());
        changed.then(|| self.layout.clone())
    }

    pub fn event(&mut self, event: AppEvent) -> Result<()> {
        let failed = matches!(
            &event,
            AppEvent::PresetFailed { .. } | AppEvent::PresetCancelled { .. }
        );
        match &event {
            AppEvent::PresetFailed { preset, error }
            | AppEvent::PresetCancelled {
                preset,
                reason: error,
            } => {
                let label = if preset == crate::preset::library::SCAN_PRESET {
                    "Equipment scan"
                } else {
                    self.preset_info
                        .get(preset)
                        .map_or("", |info| info.label.as_str())
                };
                self.window.set_failure_label(label.into());
                self.window.set_failure_number(preset_number(preset).into());
                self.window.set_failure_details(error.as_str().into());
            }
            AppEvent::PresetDone { .. } => {
                self.window.set_failure_label("".into());
                self.window.set_failure_number("".into());
                self.window.set_failure_details("".into());
            }
            _ => {}
        }
        self.status.update(event, &self.preset_info);
        if self.is_panel_visible() {
            if failed {
                self.set_status("");
            } else {
                self.set_status(&self.status.message());
            }
        } else if self.status.enabled() {
            self.status.show(&self.monitor, self.anchor)?;
        }
        Ok(())
    }

    /// HellLocker: where the status pop-ups show, if at all.
    pub fn set_status_popup(&mut self, popup: crate::config::StatusPopup) -> Result<()> {
        self.status.set_popup(popup);
        if popup == crate::config::StatusPopup::Off {
            self.status.hide()?;
        } else {
            self.status.reposition(&self.monitor, self.anchor)?;
        }
        Ok(())
    }

    pub fn tick(&mut self) -> Result<()> {
        // Wait until Slint and the OS have finished processing the DPI event.
        if self.placement_dirty.replace(false) {
            if self.is_panel_visible() {
                self.reposition()?;
            }
            self.status.reposition(&self.monitor, self.anchor)?;
        }
        self.status.tick()
    }

    pub fn is_panel_active(&self) -> bool {
        self.window
            .window()
            .with_winit_window(|window| window.has_focus())
            .unwrap_or(false)
    }
    pub fn is_editing(&self) -> bool {
        self.window.get_editing_name()
            || !self.window.get_renaming_folder().is_empty()
            || self.window.get_settings_open()
            || self.window.get_delete_open()
    }

    pub fn shortcuts_blocked(&self) -> bool {
        self.window.get_editing_name()
            || !self.window.get_renaming_folder().is_empty()
            || self.window.get_searching()
            || self.window.get_recording_shortcut() >= 0
    }

    /// HellLocker: close the Settings window without saving.
    pub fn close_settings(&self) {
        self.window.invoke_close_dialogs();
    }

    pub fn settings_saved(&mut self, config: &ShortcutBindings) {
        self.set_panel_shortcut(&config.panel);
        for row in &mut self.rows {
            let shortcut = config
                .presets
                .get(row.name.as_str())
                .cloned()
                .unwrap_or_default();
            self.preset_info
                .entry(row.name.to_string())
                .or_insert_with(|| PresetInfo {
                    name: row.name.to_string(),
                    label: row.label.to_string(),
                    ..Default::default()
                })
                .shortcut
                .clone_from(&shortcut);
            row.shortcut = shortcut.into();
        }
        self.publish(None);
        self.window.set_settings_saving(false);
        self.window.invoke_close_dialogs();
    }

    pub fn settings_failed(&self, error: &str) {
        self.window.set_settings_saving(false);
        self.window.set_settings_error(error.into());
    }

    pub fn set_shortcut_warning(&self, warning: &str) {
        if self.window.get_shortcut_warning() != warning {
            self.window.set_shortcut_warning(warning.into());
        }
    }

    pub fn next_preset_name(&self) -> String {
        // All presets, including the ones the filter hides.
        let next = self
            .preset_info
            .keys()
            .filter_map(|name| {
                name.strip_prefix("preset_")
                    .and_then(|n| n.parse::<usize>().ok())
            })
            .max()
            .unwrap_or(0)
            + 1;
        format!("preset_{next}")
    }

    pub fn add_preset(
        &mut self,
        name: String,
        faction: Faction,
        subfaction: Subfaction,
        mission: Mission,
    ) {
        let subfaction = subfaction.within(faction);
        self.rows.push(PresetRow {
            number: preset_number(&name).into(),
            name: name.as_str().into(),
            faction: faction.index(),
            subfaction: subfaction.index(),
            mission: mission.index(),
            stratagems: section(PreviewContent::Missing),
            equipment: section(PreviewContent::Missing),
            ..Default::default()
        });
        self.preset_info.insert(
            name.clone(),
            PresetInfo {
                name: name.clone(),
                faction,
                subfaction,
                mission,
                ..Default::default()
            },
        );
        self.layout.add_preset(&name);
        self.publish(Some(&name));
        self.window.invoke_focus_list();
    }

    pub fn set_label(&mut self, preset: &str, label: String) {
        self.preset_info
            .entry(preset.to_owned())
            .or_insert_with(|| PresetInfo {
                name: preset.to_owned(),
                ..Default::default()
            })
            .label
            .clone_from(&label);
        if let Some(row) = self.rows.iter_mut().find(|row| row.name == preset) {
            row.label = label.into();
        }
        self.publish(None);
        self.set_status("");
    }

    /// Returns the list layout without the preset, to save.
    pub fn remove_preset(&mut self, preset: &str) -> Layout {
        self.preset_info.remove(preset);
        self.rows.retain(|row| row.name != preset);
        self.layout.remove_preset(preset);
        // The row below the deleted one takes its place, as before.
        self.publish(None);
        self.window.invoke_focus_list();
        self.set_status("");
        self.layout.clone()
    }
    pub fn set_scope(&self, scope: PresetScope) {
        self.window.set_scope_stratagems(scope.stratagems);
        self.window.set_scope_equipment(scope.equipment);
        self.set_status("");
    }
    pub fn set_panel_shortcut(&self, shortcut: &str) {
        let keys = shortcut
            .split('+')
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(slint::SharedString::from)
            .collect::<Vec<_>>();
        self.window
            .set_panel_shortcut_keys(Rc::new(VecModel::from(keys)).into());
    }
    pub fn take_request(&self) -> Option<UiRequest> {
        self.requests.try_recv().ok()
    }
    pub fn set_status(&self, message: &str) {
        self.window.set_status(message.into());
    }

    /// HellLocker: a result line in the Settings window (Help tab buttons).
    pub fn settings_notice(&self, message: &str) {
        self.window.set_settings_notice(message.into());
        self.window.set_settings_error("".into());
    }

    /// HellLocker 1.9.4: what Check my setup found.
    pub fn set_setup_lines(&self, lines: Vec<SetupLine>) {
        self.window
            .set_setup_lines(Rc::new(VecModel::from(lines)).into());
    }

    /// HellLocker: a problem shown in the Settings window.
    pub fn settings_problem(&self, message: &str) {
        self.window.set_settings_notice("".into());
        self.window.set_settings_error(message.into());
    }

    /// HellLocker: how many presets and folders there are (for system info).
    pub fn preset_counts(&self) -> (usize, usize) {
        (self.rows.len(), self.layout.folders.len())
    }

    /// HellLocker: the panel's native window handle, to own dialogs over it.
    pub fn native_handle(&self) -> Option<isize> {
        use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        self.window
            .window()
            .with_winit_window(|window| match window.window_handle().ok()?.as_raw() {
                RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
                _ => None,
            })
            .flatten()
    }
    pub fn preview_failed(&self, message: &str) {
        self.window.set_loading(false);
        self.set_status(message);
    }
    pub fn set_pending(&self, pending: bool) {
        if pending {
            self.window.invoke_focus_list();
        }
        self.window.set_pending(pending);
    }

    pub fn set_previews(&mut self, mut previews: BTreeMap<String, PresetPreview>) {
        self.window.set_loading(false);
        let mut rows: Vec<_> = self
            .preset_info
            .iter()
            .map(|(name, info)| {
                let preview = previews.remove(name).unwrap_or(PresetPreview {
                    stratagems: PreviewContent::Missing,
                    equipment: PreviewContent::Missing,
                });
                let mut row = PresetRow {
                    number: preset_number(name).into(),
                    label: info.label.as_str().into(),
                    name: name.as_str().into(),
                    shortcut: info.shortcut.as_str().into(),
                    faction: info.faction.index(),
                    subfaction: info.subfaction.within(info.faction).index(),
                    mission: info.mission.index(),
                    stratagems: section(preview.stratagems),
                    equipment: section(preview.equipment),
                    favorite: info.favorite,
                    notes: info.notes.as_str().into(),
                    ..Default::default()
                };
                self.fill_icons(&mut row, info);
                row
            })
            .collect();
        rows.sort_by(|a, b| {
            let order = |name: &str| {
                name.strip_prefix("preset_")
                    .and_then(|n| n.parse::<u32>().ok())
                    .unwrap_or(u32::MAX)
            };
            order(&a.name)
                .cmp(&order(&b.name))
                .then_with(|| a.name.as_str().cmp(b.name.as_str()))
        });
        self.rows = rows;
        self.publish(None);
        if !self.window.get_pending() {
            self.set_status("");
        }
    }
}

/// HellLocker 1.9.6: how many lines `count` presets take in the two-column list.
fn bands(count: usize) -> usize {
    count.div_ceil(COLUMNS)
}

/// HellLocker: adds a section's preset rows to the shown list from `top`, two to a
/// line, and moves `top` below them.
fn place_rows(
    visible: &mut Vec<PresetRow>,
    entries: &mut Vec<ListEntry>,
    rows: &[&PresetRow],
    in_folder: bool,
    top: &mut f32,
) {
    for (index, row) in rows.iter().enumerate() {
        let mut row = (*row).clone();
        let column = index % COLUMNS;
        row.top = *top + (index / COLUMNS) as f32 * (ROW_HEIGHT + ROW_GAP);
        row.column = column as i32;
        row.in_folder = in_folder;
        entries.push(ListEntry::Preset {
            name: row.name.to_string(),
            top: row.top,
            column,
        });
        visible.push(row);
    }
    *top += bands(rows.len()) as f32 * (ROW_HEIGHT + ROW_GAP);
}

/// HellLocker: where a preset dropped at `y` lands: onto a folder header goes into the
/// folder; onto a preset's left or right half goes before or after it; beside the last
/// preset of a line goes after it; below the last row goes to the end, outside every
/// folder. `column` is how far across the list the drop is, in columns.
fn drop_at(entries: &[ListEntry], column: f32, y: f32) -> Option<Drop> {
    let y = y.max(0.0);
    let column = column.clamp(0.0, COLUMNS as f32 - 0.001);
    let target = column.floor() as usize;
    let mut line: Option<(String, usize)> = None;
    for entry in entries {
        match entry {
            ListEntry::Pinned { top, height } if y < top + height => return None,
            ListEntry::Folder { id, top } if y < top + FOLDER_HEIGHT + ROW_GAP => {
                return Some(Drop::IntoFolder(id.clone()));
            }
            ListEntry::Preset {
                name,
                top,
                column: at,
            } if y >= *top && y < top + ROW_HEIGHT + ROW_GAP => {
                if *at == target {
                    return Some(if column.fract() < 0.5 {
                        Drop::Before(name.clone())
                    } else {
                        Drop::After(name.clone())
                    });
                }
                line = Some((name.clone(), *at));
            }
            ListEntry::Preset { top, .. } if y < *top => break,
            _ => {}
        }
    }
    // Past the end of a short line: after its last preset.
    if let Some((name, _)) = line {
        return Some(Drop::After(name));
    }
    Some(Drop::End)
}

/// HellLocker 1.9.6: the row the arrow keys move to from `index` in the two-column
/// list (rows given as (top, column)): `dx` steps along the line, `dy` to the next or
/// previous line, staying in the same column when that line has one.
fn neighbour(rows: &[(f32, i32)], index: i32, dx: i32, dy: i32) -> i32 {
    let Some(&(top, column)) = usize::try_from(index).ok().and_then(|i| rows.get(i)) else {
        return if rows.is_empty() {
            0
        } else {
            index.clamp(0, rows.len() as i32 - 1)
        };
    };
    if dx != 0 {
        let next = index + dx.signum();
        let same_line = usize::try_from(next)
            .ok()
            .and_then(|i| rows.get(i))
            .is_some_and(|&(other, _)| other == top);
        return if same_line { next } else { index };
    }
    if dy == 0 {
        return index;
    }
    // The nearest line above or below.
    let line = rows
        .iter()
        .map(|&(other, _)| other)
        .filter(|&other| if dy > 0 { other > top } else { other < top })
        .fold(None, |best: Option<f32>, other| match best {
            Some(best) if (dy > 0 && best <= other) || (dy < 0 && best >= other) => Some(best),
            _ => Some(other),
        });
    let Some(line) = line else {
        return index;
    };
    let in_line = |wanted: i32| {
        rows.iter()
            .position(|&(other, at)| other == line && at == wanted)
    };
    in_line(column)
        .or_else(|| (0..column).rev().find_map(in_line))
        .map_or(index, |found| found as i32)
}

/// HellLocker: whether a preset matches the (lower-case) search: its name, or for an
/// unnamed preset its default "Preset N" title.
fn matches_search(search: &str, label: &str, number: &str) -> bool {
    if search.is_empty() {
        return true;
    }
    let title = if label.is_empty() {
        format!("preset {number}")
    } else {
        label.to_lowercase()
    };
    search.split_whitespace().all(|word| title.contains(word))
}

pub(super) fn preset_number(name: &str) -> &str {
    name.strip_prefix("preset_").unwrap_or_default()
}

/// Keep panel navigation available without forbidding these bindings in the game.
pub fn uses_shortcut(shortcut: &crate::input::HotkeySpec) -> bool {
    use crate::input::{HotkeyModifier, Key};
    let modifiers = shortcut.modifiers;
    (modifiers.iter().next().is_none()
        && matches!(
            shortcut.key,
            Key::Up
                | Key::Down
                | Key::W
                | Key::S
                | Key::Left
                | Key::Right
                | Key::Enter
                | Key::Space
                | Key::Tab
                | Key::Home
                | Key::End
                | Key::PageUp
                | Key::PageDown
                | Key::A
                | Key::D
                | Key::Q
                | Key::E
        ))
        || (shortcut.key == Key::Tab && modifiers.iter().eq([HotkeyModifier::Shift]))
        || (shortcut.key == Key::S && modifiers.iter().eq([HotkeyModifier::Ctrl]))
}

fn section(content: PreviewContent) -> PreviewSection {
    match content {
        PreviewContent::Missing => PreviewSection::default(),
        PreviewContent::Invalid(error) => PreviewSection {
            saved: true,
            error: error.into(),
            ..Default::default()
        },
        PreviewContent::Ready(images) => PreviewSection {
            ready: true,
            saved: true,
            error: Default::default(),
            items: Rc::new(VecModel::from(
                images
                    .into_iter()
                    .map(|item| {
                        let (role, number) = match item.role {
                            PreviewImageRole::Stratagem(number) => {
                                (PreviewRole::Stratagem, number as i32)
                            }
                            PreviewImageRole::Booster => (PreviewRole::Booster, 0),
                            PreviewImageRole::FallbackBooster => (PreviewRole::FallbackBooster, 0),
                            PreviewImageRole::Equipment(kind) => (
                                match kind {
                                    EquipmentKind::Helmet => PreviewRole::Helmet,
                                    EquipmentKind::Armor => PreviewRole::Armor,
                                    EquipmentKind::Cape => PreviewRole::Cape,
                                    EquipmentKind::Primary => PreviewRole::Primary,
                                    EquipmentKind::Secondary => PreviewRole::Secondary,
                                    EquipmentKind::Throwable => PreviewRole::Throwable,
                                },
                                0,
                            ),
                        };
                        PreviewRow {
                            role,
                            number,
                            picture: image(&item.image),
                            has_icon: item.picture.is_some(),
                            icon: item.picture.as_ref().map(image).unwrap_or_default(),
                            name: item.name.as_deref().unwrap_or_default().into(),
                        }
                    })
                    .collect::<Vec<_>>(),
            ))
            .into(),
        },
    }
}

fn image(image: &image::RgbaImage) -> slint::Image {
    slint::Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        image.as_raw(),
        image.width(),
        image.height(),
    ))
}

/// The UI owns the main event loop; the application supplies one non-blocking tick.
pub fn run(mut tick: impl FnMut() -> Result<bool> + 'static) -> Result<()> {
    let error = Rc::new(RefCell::new(None));
    let tick_error = error.clone();
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(10),
        move || {
            let done = match tick() {
                Ok(done) => done,
                Err(error) => {
                    *tick_error.borrow_mut() = Some(error);
                    true
                }
            };
            if done {
                let _ = slint::quit_event_loop();
            }
        },
    );
    slint::run_event_loop_until_quit()?;
    error.borrow_mut().take().map_or(Ok(()), Err)
}

/// The size the panel got, and where it was put (outer position).
type Placement = (slint::LogicalSize, Option<(i32, i32)>);

/// HellLocker: put the panel at a saved outer position, sized to fit that display's work
/// area. None when no display contains the position any more.
fn place_at(
    window: &slint::Window,
    at: (i32, i32),
    preferred: slint::LogicalSize,
) -> Result<Option<Placement>> {
    use slint::winit_030::winit::dpi::{PhysicalPosition, PhysicalSize};
    window
        .with_winit_window(|native| -> Result<Option<Placement>> {
            let Some(screen) = native.available_monitors().find(|screen| {
                let p = screen.position();
                let s = screen.size();
                at.0 >= p.x
                    && at.1 >= p.y
                    && at.0 < p.x + s.width as i32
                    && at.1 < p.y + s.height as i32
            }) else {
                return Ok(None);
            };
            let p = screen.position();
            let s = screen.size();
            let bounds = (p.x, p.y, s.width as i32, s.height as i32);
            #[cfg(target_os = "windows")]
            let bounds = windows::work_area(bounds)?;
            let scale = f64::from(window.scale_factor());
            let fitted = slint::LogicalSize::new(
                preferred.width.min((bounds.2 as f64 / scale) as f32),
                preferred.height.min((bounds.3 as f64 / scale) as f32),
            );
            let physical = PhysicalSize::new(
                (f64::from(fitted.width) * scale).round() as u32,
                (f64::from(fitted.height) * scale).round() as u32,
            );
            native.set_min_inner_size(Some(physical));
            native.set_max_inner_size(Some(physical));
            window.set_size(slint::PhysicalSize::new(physical.width, physical.height));
            // Keep the whole panel on that display.
            let x = at.0.clamp(
                bounds.0,
                (bounds.0 + bounds.2 - physical.width as i32).max(bounds.0),
            );
            let y = at.1.clamp(
                bounds.1,
                (bounds.1 + bounds.3 - physical.height as i32).max(bounds.1),
            );
            native.set_outer_position(PhysicalPosition::new(x, y));
            Ok(Some((fitted, Some((x, y)))))
        })
        .transpose()
        .map(Option::flatten)
}

/// Placement uses desktop coordinates only inside the presentation layer.
fn place_top_right(
    window: &slint::Window,
    monitor: &str,
    anchor: Option<(i32, i32)>,
    margin: f64,
    preferred_size: Option<slint::LogicalSize>,
) -> Result<Option<slint::LogicalSize>> {
    place_top_corner(window, monitor, anchor, margin, preferred_size, false)
}

/// HellLocker: the top right corner of the display, or the top left.
fn place_top_corner(
    window: &slint::Window,
    monitor: &str,
    anchor: Option<(i32, i32)>,
    margin: f64,
    preferred_size: Option<slint::LogicalSize>,
    from_left: bool,
) -> Result<Option<slint::LogicalSize>> {
    use slint::winit_030::winit::dpi::PhysicalPosition;
    window
        .with_winit_window(|native| -> Result<Option<slint::LogicalSize>> {
            let screens: Vec<_> = native.available_monitors().collect();
            let screen = anchor
                .and_then(|(x, y)| {
                    screens.iter().find(|screen| {
                        let p = screen.position();
                        let s = screen.size();
                        x >= p.x
                            && y >= p.y
                            && x < p.x + s.width as i32
                            && y < p.y + s.height as i32
                    })
                })
                .cloned()
                .or_else(|| native.current_monitor());
            let Some(screen) = screen else {
                return Ok(None);
            };
            let p = screen.position();
            let s = screen.size();
            let mut bounds = (p.x, p.y, s.width as i32, s.height as i32);
            #[cfg(target_os = "windows")]
            if monitor != "auto" {
                bounds = windows::monitor_bounds(monitor)?;
            }
            // HellLocker: Slint's own scale, which includes the panel size setting.
            let scale = f64::from(window.scale_factor());
            #[cfg(target_os = "windows")]
            let bounds = windows::work_area(bounds)?;
            // Fit the layout in logical pixels; Slint owns DPI scaling and rendering.
            let margin = (margin * scale).round() as i32;
            let fitted = preferred_size.map(|size| {
                slint::LogicalSize::new(
                    size.width
                        .min(((bounds.2 - 2 * margin) as f64 / scale) as f32),
                    size.height
                        .min(((bounds.3 - 2 * margin) as f64 / scale) as f32),
                )
            });
            let size = fitted.map_or_else(
                || native.outer_size().to_logical::<f64>(native.scale_factor()),
                |size| {
                    slint::winit_030::winit::dpi::LogicalSize::new(
                        size.width as f64,
                        size.height as f64,
                    )
                },
            );
            let physical = slint::PhysicalSize::new(
                (size.width * scale).round() as u32,
                (size.height * scale).round() as u32,
            );
            if fitted.is_some() {
                // Slint 1.18 caches physical min/max sizes when logical constraints
                // stay unchanged. Refresh them as well as the actual window size.
                let bounds = slint::winit_030::winit::dpi::PhysicalSize::new(
                    physical.width,
                    physical.height,
                );
                native.set_min_inner_size(Some(bounds));
                native.set_max_inner_size(Some(bounds));
                window.set_size(physical);
            }
            let w = physical.width as i32;
            let x = if from_left {
                bounds.0 + margin
            } else {
                bounds.0 + bounds.2 - w - margin
            };
            let y = bounds.1 + margin;
            native.set_outer_position(PhysicalPosition::new(x, y));
            Ok(fitted)
        })
        .transpose()
        .map(Option::flatten)
}

#[cfg(test)]
mod search_tests {
    use super::matches_search;

    #[test]
    fn search_matches_names_and_default_titles() {
        assert!(matches_search("", "Bug Blitz", "1"));
        assert!(matches_search("bug", "Bug Blitz Fire", "1"));
        assert!(matches_search("fire bug", "Bug Blitz Fire", "1"));
        assert!(!matches_search("bot", "Bug Blitz Fire", "1"));
        // An unnamed preset is found by its "Preset N" title.
        assert!(matches_search("preset 3", "", "3"));
        assert!(!matches_search("preset 3", "", "4"));
    }
}

#[cfg(test)]
mod drop_tests {
    use super::{Drop, FOLDER_HEIGHT, ListEntry, ROW_GAP, ROW_HEIGHT, drop_at, neighbour};

    #[test]
    fn drops_land_on_folders_rows_and_the_end() {
        // A folder header, then a line of two presets in it, then one in no folder.
        let folder = FOLDER_HEIGHT + ROW_GAP;
        let row = ROW_HEIGHT + ROW_GAP;
        let preset = |name: &str, top: f32, column: usize| ListEntry::Preset {
            name: name.into(),
            top,
            column,
        };
        let entries = vec![
            ListEntry::Folder {
                id: "folder_1".into(),
                top: 0.0,
            },
            preset("preset_1", folder, 0),
            preset("preset_2", folder, 1),
            preset("preset_3", folder + row, 0),
        ];
        let into = Some(Drop::IntoFolder("folder_1".into()));
        assert_eq!(drop_at(&entries, 0.5, -20.0), into);
        assert_eq!(drop_at(&entries, 1.5, 10.0), into);
        let at = folder + 5.0;
        assert_eq!(
            drop_at(&entries, 0.2, at),
            Some(Drop::Before("preset_1".into()))
        );
        assert_eq!(
            drop_at(&entries, 0.8, at),
            Some(Drop::After("preset_1".into()))
        );
        assert_eq!(
            drop_at(&entries, 1.2, at),
            Some(Drop::Before("preset_2".into()))
        );
        assert_eq!(
            drop_at(&entries, 9.0, at),
            Some(Drop::After("preset_2".into()))
        );
        // Beside the last preset, on a line it has to itself.
        assert_eq!(
            drop_at(&entries, 1.5, folder + row + 10.0),
            Some(Drop::After("preset_3".into()))
        );
        assert_eq!(
            drop_at(&entries, 0.5, folder + 2.0 * row + 10.0),
            Some(Drop::End)
        );
    }

    #[test]
    fn nothing_drops_into_favourites_or_recently_used() {
        let header = FOLDER_HEIGHT + ROW_GAP;
        let row = ROW_HEIGHT + ROW_GAP;
        let entries = vec![
            ListEntry::Pinned {
                top: 0.0,
                height: header,
            },
            ListEntry::Pinned {
                top: header,
                height: row,
            },
            ListEntry::Preset {
                name: "preset_1".into(),
                top: header + row,
                column: 0,
            },
        ];
        assert_eq!(drop_at(&entries, 0.5, 5.0), None);
        assert_eq!(drop_at(&entries, 1.5, header + 10.0), None);
        assert_eq!(
            drop_at(&entries, 0.2, header + row + 5.0),
            Some(Drop::Before("preset_1".into()))
        );
    }

    #[test]
    fn arrow_keys_move_through_the_columns() {
        // Two full lines, then a line with one preset.
        let rows = [(0.0, 0), (0.0, 1), (40.0, 0), (40.0, 1), (80.0, 0)];
        assert_eq!(neighbour(&rows, 1, 0, 1), 3);
        assert_eq!(neighbour(&rows, 3, 0, 1), 4);
        assert_eq!(neighbour(&rows, 4, 0, -1), 2);
        assert_eq!(neighbour(&rows, 0, 0, -1), 0);
        assert_eq!(neighbour(&rows, 0, 1, 0), 1);
        assert_eq!(neighbour(&rows, 1, 1, 0), 1);
        assert_eq!(neighbour(&rows, 2, -1, 0), 2);
        assert_eq!(neighbour(&[], 0, 0, 1), 0);
    }
}
