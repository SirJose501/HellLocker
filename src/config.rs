use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::context::{Faction, Mission, Subfaction};
use crate::preset::PresetScope;
use anyhow::{Context, Result, bail, ensure};
use tracing::info;

mod migration;
mod settings;
mod shortcuts;
pub use settings::SettingsDraft;
pub use shortcuts::ShortcutBindings;

pub(crate) const DEFAULT_CONFIG_TOML: &str = include_str!("../data/config.toml");
pub const AUTO_MONITOR: &str = "auto";

#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    pub presets: PresetsConfig,
    pub ui: UiSettings,
}

impl AppConfig {
    pub fn shortcut_bindings(&self) -> ShortcutBindings {
        ShortcutBindings {
            panel: self.ui.panel_shortcut.clone(),
            presets: self
                .presets
                .slots
                .iter()
                .map(|(name, slot)| (name.clone(), slot.shortcut.clone()))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PresetsConfig {
    pub slots: BTreeMap<String, PresetSlot>,
    pub apply_in_saved_order: bool,
    pub auto_ready_up: bool,
    pub save_fallback_when_taken: bool,
    /// HellLocker: wait up to three times longer for the game (see crate::timing).
    pub patient_mode: bool,
    pub scope: PresetScope,
    /// HellLocker: include the helmet, armor and cape when saving and applying equipment.
    pub helmet: bool,
    pub armor: bool,
    pub cape: bool,
    /// HellLocker: the preset list's folders, in order, each with its presets in order.
    pub folders: Vec<crate::folders::PresetFolder>,
    /// HellLocker: presets in no folder, in list order (others go last, by number).
    pub order: Vec<String>,
    /// HellLocker 1.9.4: the presets applied last, newest first.
    pub recent: Vec<String>,
    /// HellLocker 1.9.4: whether Favourites and Recently used are open in the list.
    pub favorites_open: bool,
    pub recent_open: bool,
}

impl PresetsConfig {
    pub fn attire_choice(&self) -> crate::item::AttireChoice {
        crate::item::AttireChoice {
            helmet: self.helmet,
            armor: self.armor,
            cape: self.cape,
        }
    }
}

impl Default for PresetsConfig {
    fn default() -> Self {
        Self {
            slots: (1..=6)
                .map(|n| {
                    (
                        format!("preset_{n}"),
                        PresetSlot {
                            label: String::new(),
                            shortcut: format!("Ctrl + Shift + F{}", n + 6),
                            ..Default::default()
                        },
                    )
                })
                .collect(),
            apply_in_saved_order: false,
            auto_ready_up: false,
            save_fallback_when_taken: false,
            patient_mode: false,
            scope: PresetScope::default(),
            helmet: true,
            armor: true,
            cape: true,
            folders: Vec::new(),
            order: Vec::new(),
            recent: Vec::new(),
            favorites_open: true,
            recent_open: true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PresetSlot {
    pub label: String,
    pub shortcut: String,
    /// HellLocker: the enemy faction this preset is for ("any" = general purpose).
    pub faction: Faction,
    /// HellLocker: the sub-faction ("any" = the whole faction).
    pub subfaction: Subfaction,
    /// HellLocker: the mission type this preset is for ("any" = general purpose).
    pub mission: Mission,
    /// HellLocker 1.9.4: starred; listed in Favourites at the top.
    pub favorite: bool,
    /// HellLocker 1.9.4: the player's notes for this preset.
    pub notes: String,
    /// HellLocker 1.9.5: the icons shown beside the preset in the list (see
    /// crate::preset_icons); "" leaves a slot at its default.
    pub icons: Vec<String>,
}

/// HellLocker 1.9.4: longest notes kept for one preset, in characters.
pub const NOTES_MAX: usize = 2000;
/// HellLocker 1.9.4: how many presets Recently used lists.
pub const RECENT_MAX: usize = 5;

impl PresetSlot {
    fn to_item(&self) -> toml_edit::Item {
        let mut table = toml_edit::Table::new();
        table.insert("label", toml_edit::value(self.label.as_str()));
        table.insert("shortcut", toml_edit::value(self.shortcut.as_str()));
        table.insert("faction", toml_edit::value(self.faction.key()));
        table.insert("subfaction", toml_edit::value(self.subfaction.key()));
        table.insert("mission", toml_edit::value(self.mission.key()));
        if self.favorite {
            table.insert("favorite", toml_edit::value(true));
        }
        if !self.notes.is_empty() {
            table.insert("notes", toml_edit::value(self.notes.as_str()));
        }
        if self.icons.iter().any(|icon| !icon.is_empty()) {
            let icons: toml_edit::Array = self.icons.iter().map(String::as_str).collect();
            table.insert("icons", toml_edit::value(icons));
        }
        toml_edit::Item::Table(table)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
pub enum UiLanguage {
    #[default]
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "en")]
    English,
    #[serde(rename = "zh-Hans")]
    SimplifiedChinese,
    #[serde(rename = "zh-Hant")]
    TraditionalChinese,
    #[serde(rename = "ja")]
    Japanese,
    #[serde(rename = "ko")]
    Korean,
    #[serde(rename = "de")]
    German,
    #[serde(rename = "fr")]
    French,
    #[serde(rename = "it")]
    Italian,
    #[serde(rename = "es-ES")]
    SpanishSpain,
    #[serde(rename = "es-419")]
    SpanishLatinAmerica,
    #[serde(rename = "pt-BR")]
    PortugueseBrazil,
    #[serde(rename = "pt-PT")]
    PortuguesePortugal,
    #[serde(rename = "pl")]
    Polish,
    #[serde(rename = "ru")]
    Russian,
}

impl UiLanguage {
    pub const ALL: [Self; 15] = [
        Self::Auto,
        Self::English,
        Self::SimplifiedChinese,
        Self::TraditionalChinese,
        Self::Japanese,
        Self::Korean,
        Self::German,
        Self::French,
        Self::Italian,
        Self::SpanishSpain,
        Self::SpanishLatinAmerica,
        Self::PortugueseBrazil,
        Self::PortuguesePortugal,
        Self::Polish,
        Self::Russian,
    ];

    pub fn locale(self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::English => Some("en"),
            Self::SimplifiedChinese => Some("zh-Hans"),
            Self::TraditionalChinese => Some("zh-Hant"),
            Self::Japanese => Some("ja"),
            Self::Korean => Some("ko"),
            Self::German => Some("de"),
            Self::French => Some("fr"),
            Self::Italian => Some("it"),
            Self::SpanishSpain => Some("es-ES"),
            Self::SpanishLatinAmerica => Some("es-419"),
            Self::PortugueseBrazil => Some("pt-BR"),
            Self::PortuguesePortugal => Some("pt-PT"),
            Self::Polish => Some("pl"),
            Self::Russian => Some("ru"),
        }
    }

    pub fn from_locale(locale: &str) -> Option<Self> {
        let locale = locale.replace('_', "-").to_ascii_lowercase();
        let mut parts = locale.split('-');
        match parts.next()? {
            "zh" => Some(
                if locale.contains("-hant")
                    || !locale.contains("-hans")
                        && parts.any(|part| matches!(part, "tw" | "hk" | "mo"))
                {
                    Self::TraditionalChinese
                } else {
                    Self::SimplifiedChinese
                },
            ),
            "es" => Some(
                if parts.any(|part| {
                    matches!(
                        part,
                        "419"
                            | "ar"
                            | "bo"
                            | "br"
                            | "bz"
                            | "cl"
                            | "co"
                            | "cr"
                            | "cu"
                            | "do"
                            | "ec"
                            | "gt"
                            | "hn"
                            | "mx"
                            | "ni"
                            | "pa"
                            | "pe"
                            | "pr"
                            | "py"
                            | "sv"
                            | "us"
                            | "uy"
                            | "ve"
                    )
                }) {
                    Self::SpanishLatinAmerica
                } else {
                    Self::SpanishSpain
                },
            ),
            "pt" => Some(if parts.any(|part| part == "br") {
                Self::PortugueseBrazil
            } else {
                Self::PortuguesePortugal
            }),
            language => Self::ALL
                .into_iter()
                .find(|candidate| candidate.locale() == Some(language)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiSettings {
    pub panel_shortcut: String,
    pub monitor: String,
    pub language: UiLanguage,
    /// HellLocker: the faction filter the panel opens with.
    pub view_faction: Faction,
    /// HellLocker: the sub-faction filter the panel opens with.
    pub view_subfaction: Subfaction,
    /// HellLocker: the mission filter the panel opens with.
    pub view_mission: Mission,
    /// HellLocker: keep the panel up when focus moves to the game.
    pub keep_open: bool,
    /// HellLocker: panel size, as a multiple of the Windows display scale (1.0 to 2.0).
    pub ui_scale: f64,
    /// HellLocker: how opaque the background art is, in percent (10 to 100).
    pub background_opacity: i64,
    /// HellLocker: where status pop-ups show, if at all.
    pub status_popup: StatusPopup,
    /// HellLocker: play a sound when a preset is saved.
    pub save_sound: bool,
    /// HellLocker: where the panel was last dragged to (desktop pixels), or empty for
    /// the top right of the game's display.
    pub panel_position: Vec<i32>,
    /// HellLocker 1.9.4: the right side folded away, showing only the preset list.
    pub compact: bool,
}

/// HellLocker: where the status pop-ups show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusPopup {
    #[default]
    TopRight,
    TopLeft,
    Off,
}

impl StatusPopup {
    pub const ALL: [Self; 3] = [Self::TopRight, Self::TopLeft, Self::Off];

    pub fn key(self) -> &'static str {
        match self {
            Self::TopRight => "top_right",
            Self::TopLeft => "top_left",
            Self::Off => "off",
        }
    }
}

impl UiSettings {
    pub const UI_SCALES: [f64; 5] = [1.0, 1.25, 1.5, 1.75, 2.0];

    /// The panel size, snapped to one the Settings dialog offers.
    pub fn ui_scale(&self) -> f64 {
        Self::UI_SCALES
            .into_iter()
            .min_by(|a, b| {
                (a - self.ui_scale)
                    .abs()
                    .total_cmp(&(b - self.ui_scale).abs())
            })
            .unwrap_or(1.0)
    }

    /// The background opacity, clamped to the range the Settings slider offers.
    pub fn background_opacity(&self) -> i64 {
        self.background_opacity.clamp(10, 100)
    }

    pub fn panel_position(&self) -> Option<(i32, i32)> {
        match self.panel_position[..] {
            [x, y] => Some((x, y)),
            _ => None,
        }
    }
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            panel_shortcut: "Ctrl + Alt + Enter".into(),
            monitor: AUTO_MONITOR.to_string(),
            language: UiLanguage::default(),
            view_faction: Faction::Any,
            view_subfaction: Subfaction::Any,
            view_mission: Mission::Any,
            keep_open: true,
            ui_scale: 1.5,
            background_opacity: 85,
            status_popup: StatusPopup::TopRight,
            save_sound: true,
            panel_position: Vec::new(),
            compact: false,
        }
    }
}

fn write_setting(
    document: &mut toml_edit::DocumentMut,
    section: &str,
    key: &str,
    value: impl Into<toml_edit::Value>,
) -> Result<()> {
    let defaults = DEFAULT_CONFIG_TOML.parse::<toml_edit::DocumentMut>()?;
    let default_key = defaults[section]
        .as_table_like()
        .and_then(|table| table.key(key))
        .with_context(|| format!("unknown setting: {section}.{key}"))?;
    let table = section_table(document, section)?;
    let mut value = value.into();
    if let Some(previous) = table.get(key).and_then(toml_edit::Item::as_value) {
        *value.decor_mut() = previous.decor().clone();
    }
    if let Some(setting) = table.get_mut(key) {
        *setting = toml_edit::Item::Value(value);
    } else {
        table
            .entry_format(default_key)
            .or_insert(toml_edit::Item::Value(value));
    }
    Ok(())
}

pub fn save_preset_label(config_path: &Path, preset: &str, label: &str) -> Result<()> {
    validate_preset_label(preset, label)?;
    edit_config(config_path, |document| {
        write_slot_field(document, preset, "label", label)
    })
}

/// HellLocker: which faction and mission type a preset is for.
pub fn save_preset_tags(
    config_path: &Path,
    preset: &str,
    faction: Faction,
    subfaction: Subfaction,
    mission: Mission,
) -> Result<()> {
    validate_preset_label(preset, "")?;
    edit_config(config_path, |document| {
        write_slot_field(document, preset, "faction", faction.key())?;
        write_slot_field(
            document,
            preset,
            "subfaction",
            subfaction.within(faction).key(),
        )?;
        write_slot_field(document, preset, "mission", mission.key())
    })
}

/// HellLocker: where the panel was dragged to, restored on the next start.
pub fn save_panel_position(config_path: &Path, position: Option<(i32, i32)>) -> Result<()> {
    let value: toml_edit::Array = position
        .map(|(x, y)| vec![i64::from(x), i64::from(y)])
        .unwrap_or_default()
        .into_iter()
        .collect();
    edit_config(config_path, |document| {
        write_setting(document, "ui", "panel_position", value)
    })
}

/// HellLocker: the panel's faction and mission filter, restored on the next start.
pub fn save_view(
    config_path: &Path,
    faction: Faction,
    subfaction: Subfaction,
    mission: Mission,
) -> Result<()> {
    edit_config(config_path, |document| {
        write_setting(document, "ui", "view_faction", faction.key())?;
        write_setting(
            document,
            "ui",
            "view_subfaction",
            subfaction.within(faction).key(),
        )?;
        write_setting(document, "ui", "view_mission", mission.key())
    })
}

/// HellLocker: the preset list's folders and order.
pub fn save_layout(config_path: &Path, layout: &crate::folders::Layout) -> Result<()> {
    let folders: toml_edit::Array = layout
        .folders
        .iter()
        .map(|folder| {
            let mut table = toml_edit::InlineTable::new();
            table.insert("id", folder.id.as_str().into());
            table.insert("name", folder.name.as_str().into());
            table.insert("open", folder.open.into());
            if !folder.icon.is_empty() {
                table.insert("icon", folder.icon.as_str().into());
            }
            table.insert(
                "presets",
                folder
                    .presets
                    .iter()
                    .map(String::as_str)
                    .collect::<toml_edit::Array>()
                    .into(),
            );
            let mut value = toml_edit::Value::InlineTable(table);
            // One folder per line keeps the file readable.
            value.decor_mut().set_prefix("\n    ");
            value
        })
        .collect();
    let mut folders = folders;
    if !folders.is_empty() {
        folders.set_trailing("\n");
    }
    let order: toml_edit::Array = layout.loose.iter().map(String::as_str).collect();
    edit_config(config_path, |document| {
        write_setting(document, "presets", "folders", folders)?;
        write_setting(document, "presets", "order", order)
    })
}

/// HellLocker: the Helmet, Armor and Cape boxes.
pub fn save_attire(config_path: &Path, attire: crate::item::AttireChoice) -> Result<()> {
    edit_config(config_path, |document| {
        write_setting(document, "presets", "helmet", attire.helmet)?;
        write_setting(document, "presets", "armor", attire.armor)?;
        write_setting(document, "presets", "cape", attire.cape)
    })
}

/// HellLocker 1.4.0 had one `attire` switch; it becomes the three boxes.
fn migrate_attire(document: &mut toml_edit::DocumentMut) -> bool {
    let Some(presets) = document
        .get_mut("presets")
        .and_then(|item| item.as_table_like_mut())
    else {
        return false;
    };
    let Some(old) = presets.remove("attire") else {
        return false;
    };
    let value = old.as_bool().unwrap_or(true);
    for key in ["helmet", "armor", "cape"] {
        if !presets.contains_key(key) {
            presets.insert(key, toml_edit::value(value));
        }
    }
    true
}

pub fn save_scope(config_path: &Path, scope: PresetScope) -> Result<()> {
    let value: toml_edit::InlineTable = [
        ("stratagems", scope.stratagems),
        ("equipment", scope.equipment),
    ]
    .into_iter()
    .collect();
    edit_config(config_path, |document| {
        write_setting(document, "presets", "scope", value)
    })
}

fn edit_config(
    config_path: &Path,
    edit: impl FnOnce(&mut toml_edit::DocumentMut) -> Result<()>,
) -> Result<()> {
    let text = fs::read_to_string(config_path)
        .with_context(|| format!("failed to read {}", config_path.display()))?;
    let mut document = text
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("failed to parse {}", config_path.display()))?;
    edit(&mut document)?;
    fs::write(config_path, document.to_string())
        .with_context(|| format!("failed to update {}", config_path.display()))
}

fn slots_table(document: &mut toml_edit::DocumentMut) -> Result<&mut dyn toml_edit::TableLike> {
    // Omitted slots mean the defaults; an explicit empty table means no presets.
    let presets = section_table(document, "presets")?;
    if !presets.contains_key("slots") {
        let defaults = DEFAULT_CONFIG_TOML.parse::<toml_edit::DocumentMut>()?;
        presets.insert("slots", defaults["presets"]["slots"].clone());
    }
    presets
        .get_mut("slots")
        .expect("slots inserted above")
        .as_table_like_mut()
        .context("presets.slots must be a table")
}

fn section_table<'a>(
    document: &'a mut toml_edit::DocumentMut,
    section: &str,
) -> Result<&'a mut dyn toml_edit::TableLike> {
    if document.get(section).is_none() {
        let defaults = DEFAULT_CONFIG_TOML.parse::<toml_edit::DocumentMut>()?;
        document[section] = defaults[section].clone();
    }
    // Child tables and their comments need an ordinary section, not an inline one.
    if let Some(inline) = document[section].as_inline_table() {
        document[section] = toml_edit::Item::Table(inline.clone().into_table());
    }
    document[section]
        .as_table_like_mut()
        .with_context(|| format!("{section} configuration must be a table"))
}

fn slot_table<'a>(
    document: &'a mut toml_edit::DocumentMut,
    preset: &str,
) -> Result<&'a mut dyn toml_edit::TableLike> {
    slots_table(document)?
        .entry(preset)
        .or_insert_with(|| PresetSlot::default().to_item())
        .as_table_like_mut()
        .with_context(|| format!("presets.slots.{preset} must be a table"))
}

fn write_slot_field(
    document: &mut toml_edit::DocumentMut,
    preset: &str,
    key: &str,
    value: impl Into<toml_edit::Value>,
) -> Result<()> {
    let table = slot_table(document, preset)?;
    let mut value = value.into();
    if let Some(previous) = table.get(key).and_then(toml_edit::Item::as_value) {
        *value.decor_mut() = previous.decor().clone();
    }
    table.insert(key, toml_edit::Item::Value(value));
    Ok(())
}

pub fn add_preset(
    config_path: &Path,
    preset: &str,
    faction: Faction,
    subfaction: Subfaction,
    mission: Mission,
) -> Result<()> {
    validate_preset_label(preset, "")?;
    edit_config(config_path, |document| {
        let slots = slots_table(document)?;
        ensure!(
            !slots.contains_key(preset),
            "preset already exists: {preset}"
        );
        let slot = PresetSlot {
            faction,
            subfaction: subfaction.within(faction),
            mission,
            ..Default::default()
        };
        slots.insert(preset, slot.to_item());
        Ok(())
    })
}

/// HellLocker 1.9.4: a copy of `from` under the new name `to`: same tags, notes and
/// label (with " (copy)"), no shortcut, not starred.
pub fn duplicate_preset(config_path: &Path, from: &str, to: &str, label: &str) -> Result<()> {
    validate_preset_label(to, label)?;
    edit_config(config_path, |document| {
        let source = slots_table(document)?
            .get(from)
            .and_then(toml_edit::Item::as_table_like)
            .map(|table| {
                let text = |key: &str| {
                    table
                        .get(key)
                        .and_then(toml_edit::Item::as_str)
                        .unwrap_or_default()
                        .to_owned()
                };
                (
                    text("faction"),
                    text("subfaction"),
                    text("mission"),
                    text("notes"),
                    table.get("icons").cloned(),
                )
            })
            .with_context(|| format!("preset not found: {from}"))?;
        let slots = slots_table(document)?;
        ensure!(!slots.contains_key(to), "preset already exists: {to}");
        let mut table = toml_edit::Table::new();
        table.insert("label", toml_edit::value(label));
        table.insert("shortcut", toml_edit::value(""));
        for (key, value) in [
            ("faction", &source.0),
            ("subfaction", &source.1),
            ("mission", &source.2),
        ] {
            table.insert(
                key,
                toml_edit::value(if value.is_empty() {
                    "any"
                } else {
                    value.as_str()
                }),
            );
        }
        if !source.3.is_empty() {
            table.insert("notes", toml_edit::value(source.3.as_str()));
        }
        if let Some(icons) = source.4 {
            table.insert("icons", icons);
        }
        slots.insert(to, toml_edit::Item::Table(table));
        Ok(())
    })
}

/// HellLocker 1.9.4: star or unstar a preset.
pub fn save_preset_favorite(config_path: &Path, preset: &str, favorite: bool) -> Result<()> {
    validate_preset_label(preset, "")?;
    edit_config(config_path, |document| {
        write_slot_field(document, preset, "favorite", favorite)
    })
}

/// HellLocker 1.9.4: a preset's notes (trimmed, at most NOTES_MAX characters).
pub fn save_preset_notes(config_path: &Path, preset: &str, notes: &str) -> Result<String> {
    validate_preset_label(preset, "")?;
    let notes: String = notes.trim().chars().take(NOTES_MAX).collect();
    edit_config(config_path, |document| {
        write_slot_field(document, preset, "notes", notes.as_str())
    })?;
    Ok(notes)
}

/// HellLocker 1.9.5: a preset's icons ("" = the default for that slot).
pub fn save_preset_icons(
    config_path: &Path,
    preset: &str,
    icons: &[String; crate::preset_icons::SLOTS],
) -> Result<()> {
    validate_preset_label(preset, "")?;
    let mut icons: Vec<&str> = icons.iter().map(String::as_str).collect();
    while icons.last().is_some_and(|icon| icon.is_empty()) {
        icons.pop();
    }
    edit_config(config_path, |document| {
        if icons.is_empty() {
            slot_table(document, preset)?.remove("icons");
            Ok(())
        } else {
            let value: toml_edit::Array = icons.iter().copied().collect();
            write_slot_field(document, preset, "icons", value)
        }
    })
}

/// HellLocker 1.9.4: the Recently used list, newest first.
pub fn save_recent(config_path: &Path, recent: &[String]) -> Result<()> {
    let value: toml_edit::Array = recent.iter().map(String::as_str).collect();
    edit_config(config_path, |document| {
        write_setting(document, "presets", "recent", value)
    })
}

/// HellLocker 1.9.4: whether Favourites (kind 1) or Recently used (kind 2) is open.
pub fn save_pinned_open(config_path: &Path, recent: bool, open: bool) -> Result<()> {
    let key = if recent {
        "recent_open"
    } else {
        "favorites_open"
    };
    edit_config(config_path, |document| {
        write_setting(document, "presets", key, open)
    })
}

/// HellLocker 1.9.4: the right side folded away.
pub fn save_compact(config_path: &Path, compact: bool) -> Result<()> {
    edit_config(config_path, |document| {
        write_setting(document, "ui", "compact", compact)
    })
}

/// HellLocker 1.9.4: `preset` was just applied: first in the list, at most RECENT_MAX.
pub fn push_recent(recent: &[String], preset: &str) -> Vec<String> {
    std::iter::once(preset.to_owned())
        .chain(recent.iter().filter(|name| *name != preset).cloned())
        .take(RECENT_MAX)
        .collect()
}

pub fn remove_preset(config_path: &Path, preset: &str) -> Result<()> {
    edit_config(config_path, |document| {
        slots_table(document)?.remove(preset);
        // Keep the empty parent explicit so deserialization cannot restore defaults.
        if let Some(table) = document["presets"]["slots"].as_table_mut()
            && table.is_empty()
        {
            table.set_implicit(false);
        }
        Ok(())
    })
}

pub fn load_app_config(config_path: &Path) -> Result<(AppConfig, bool)> {
    if !config_path.exists() {
        if let Some(parent) = config_path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create config directory {}", parent.display())
            })?;
        }
        fs::write(config_path, DEFAULT_CONFIG_TOML)
            .with_context(|| format!("failed to create {}", config_path.display()))?;
        info!("default configuration created");
    }
    let text = fs::read_to_string(config_path)
        .with_context(|| format!("failed to read {}", config_path.display()))?;
    let mut document = text.parse::<toml_edit::DocumentMut>()?;
    let migrated = migration::migrate(&mut document)?;
    // HellLocker: rewritten quietly; unlike the shortcut migration it needs no notice.
    if migrate_attire(&mut document) {
        fs::write(config_path, document.to_string())
            .context("failed to update the Helmet, Armor & Cape setting")?;
    }
    let config: AppConfig = toml::from_str(&document.to_string())
        .with_context(|| format!("failed to parse {}", config_path.display()))?;
    for (preset, slot) in &config.presets.slots {
        validate_preset_label(preset, &slot.label)?;
    }
    config.shortcut_bindings().validate()?;
    if migrated {
        fs::write(config_path, document.to_string()).context("failed to migrate configuration")?;
    }
    Ok((config, migrated))
}

fn validate_preset_label(preset: &str, label: &str) -> Result<()> {
    let valid_key = preset
        .strip_prefix("preset_")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|index| *index > 0)
        .is_some_and(|index| preset == format!("preset_{index}"));
    if !valid_key {
        bail!("preset ID must be preset_N with a positive integer N, got {preset}");
    }
    if label.chars().any(char::is_control) {
        bail!("presets.slots.{preset}.label must not contain control characters");
    }
    Ok(())
}

#[cfg(test)]
mod diverkit_tests {
    use super::*;

    fn temp_config(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "diverkit-config-test-{}-{name}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        fs::write(&path, DEFAULT_CONFIG_TOML).unwrap();
        path
    }

    #[test]
    fn folder_layout_survives_a_reload() {
        let path = temp_config("folders");
        let mut layout = crate::folders::Layout::new(
            Vec::new(),
            Vec::new(),
            ["preset_1", "preset_2", "preset_3"],
        );
        let id = layout.add_folder("Bot \"hunting\"");
        layout.move_preset("preset_3", &crate::folders::Drop::IntoFolder(id.clone()));
        layout.set_open(&id, false);
        assert!(layout.set_icon(&id, "boosters:dead-sprint"));
        assert!(!layout.set_icon(&id, "boosters:dead-sprint"));
        save_layout(&path, &layout).unwrap();
        let (config, _) = load_app_config(&path).unwrap();
        let reloaded = crate::folders::Layout::new(
            config.presets.folders,
            config.presets.order,
            ["preset_1", "preset_2", "preset_3"],
        );
        assert_eq!(reloaded, layout);
        assert_eq!(reloaded.folders[0].name, "Bot \"hunting\"");
        assert_eq!(reloaded.folders[0].icon, "boosters:dead-sprint");
        // Saving again rewrites in place rather than adding keys.
        save_layout(&path, &reloaded).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.matches("\nfolders = ").count(), 1);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn default_config_parses_with_any_tags() {
        let config: AppConfig = toml::from_str(DEFAULT_CONFIG_TOML).unwrap();
        assert_eq!(config.ui.view_faction, Faction::Any);
        assert_eq!(config.ui.view_mission, Mission::Any);
        assert_eq!(config.presets.slots.len(), 6);
        assert!(
            config
                .presets
                .slots
                .values()
                .all(|slot| { slot.faction == Faction::Any && slot.mission == Mission::Any })
        );
    }

    #[test]
    fn tags_and_view_survive_a_reload() {
        let path = temp_config("tags");
        add_preset(
            &path,
            "preset_7",
            Faction::Terminids,
            Subfaction::PredatorStrain,
            Mission::Blitz,
        )
        .unwrap();
        // A sub-faction of another faction is not kept.
        save_preset_tags(
            &path,
            "preset_2",
            Faction::Illuminate,
            Subfaction::JetBrigade,
            Mission::Defense,
        )
        .unwrap();
        save_view(
            &path,
            Faction::Automatons,
            Subfaction::CyborgLegion,
            Mission::Eradicate,
        )
        .unwrap();
        let (config, migrated) = load_app_config(&path).unwrap();
        assert!(!migrated);
        let slot = |name: &str| config.presets.slots[name].clone();
        assert_eq!(
            (slot("preset_7").faction, slot("preset_7").mission),
            (Faction::Terminids, Mission::Blitz)
        );
        assert_eq!(
            (slot("preset_2").faction, slot("preset_2").mission),
            (Faction::Illuminate, Mission::Defense)
        );
        // Untouched slots keep their label and shortcut, and default to "any".
        assert_eq!(slot("preset_1").shortcut, "Ctrl + Shift + F7");
        assert_eq!(slot("preset_1").faction, Faction::Any);
        assert_eq!(slot("preset_7").subfaction, Subfaction::PredatorStrain);
        assert_eq!(slot("preset_2").subfaction, Subfaction::Any);
        assert_eq!(slot("preset_1").subfaction, Subfaction::Any);
        assert_eq!(
            (config.ui.view_faction, config.ui.view_mission),
            (Faction::Automatons, Mission::Eradicate)
        );
        assert_eq!(config.ui.view_subfaction, Subfaction::CyborgLegion);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn icons_are_saved_copied_and_cleared() {
        let path = temp_config("icons");
        let icons = ["".to_string(), "stratagems:railgun".to_string()];
        save_preset_icons(&path, "preset_3", &icons).unwrap();
        duplicate_preset(&path, "preset_3", "preset_9", "Copy").unwrap();
        let (config, _) = load_app_config(&path).unwrap();
        assert_eq!(config.presets.slots["preset_3"].icons, icons);
        assert_eq!(config.presets.slots["preset_9"].icons, icons);
        save_preset_icons(&path, "preset_3", &Default::default()).unwrap();
        let (config, _) = load_app_config(&path).unwrap();
        assert!(config.presets.slots["preset_3"].icons.is_empty());
        assert!(!fs::read_to_string(&path).unwrap().contains("icons = []"));
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn slots_without_tags_still_load() {
        // A config written by HellLocker (no faction/mission keys) stays valid.
        let path = temp_config("legacy");
        fs::write(
            &path,
            "[presets.slots.preset_3]\nlabel = \"Old\"\nshortcut = \"\"\n",
        )
        .unwrap();
        let (config, _) = load_app_config(&path).unwrap();
        let slot = &config.presets.slots["preset_3"];
        assert_eq!(slot.label, "Old");
        assert_eq!((slot.faction, slot.mission), (Faction::Any, Mission::Any));
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn panel_settings_default_and_round_trip() {
        let config: AppConfig = toml::from_str(DEFAULT_CONFIG_TOML).unwrap();
        assert!(config.ui.keep_open);
        assert!(config.ui.save_sound);
        assert_eq!(config.ui.status_popup, StatusPopup::TopRight);
        assert!(!config.presets.patient_mode);
        assert_eq!(config.ui.ui_scale(), 1.5);
        assert_eq!(config.ui.panel_position(), None);
        assert_eq!(config.ui.background_opacity(), 85);
        let path = temp_config("panel");
        save_panel_position(&path, Some((-1920, 40))).unwrap();
        let draft = SettingsDraft {
            shortcuts: config.shortcut_bindings(),
            monitor: AUTO_MONITOR.into(),
            language: UiLanguage::Auto,
            apply_in_saved_order: false,
            auto_ready_up: false,
            save_fallback_when_taken: false,
            patient_mode: true,
            keep_open: false,
            ui_scale: 1.25,
            background_opacity: 40,
            status_popup: StatusPopup::TopLeft,
            save_sound: false,
            start_with_windows: false,
        };
        draft.save(&path).unwrap();
        let (config, _) = load_app_config(&path).unwrap();
        assert!(!config.ui.keep_open);
        assert_eq!(config.ui.ui_scale(), 1.25);
        assert_eq!(config.ui.background_opacity(), 40);
        assert_eq!(config.ui.status_popup, StatusPopup::TopLeft);
        assert!(!config.ui.save_sound);
        assert!(config.presets.patient_mode);
        // A second monitor left of the main one has negative coordinates.
        assert_eq!(config.ui.panel_position(), Some((-1920, 40)));
        save_panel_position(&path, None).unwrap();
        let (config, _) = load_app_config(&path).unwrap();
        assert_eq!(config.ui.panel_position(), None);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn odd_panel_sizes_snap_to_offered_ones() {
        let ui = |scale| UiSettings {
            ui_scale: scale,
            ..Default::default()
        };
        assert_eq!(ui(1.9).ui_scale(), 2.0);
        assert_eq!(ui(3.0).ui_scale(), 2.0);
        assert_eq!(ui(0.5).ui_scale(), 1.0);
        assert_eq!(ui(1.3).ui_scale(), 1.25);
    }

    #[test]
    fn the_single_attire_switch_becomes_three_boxes() {
        let path = temp_config("attire");
        let text = fs::read_to_string(&path).unwrap();
        let text = text.replace(
            "helmet = true\narmor = true\ncape = true\n",
            "attire = false\n",
        );
        assert!(text.contains("attire = false"));
        fs::write(&path, text).unwrap();
        let (config, _) = load_app_config(&path).unwrap();
        assert_eq!(
            config.presets.attire_choice(),
            crate::item::AttireChoice {
                helmet: false,
                armor: false,
                cape: false
            }
        );
        // The file was rewritten, and each box now saves on its own.
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("attire ="));
        save_attire(
            &path,
            crate::item::AttireChoice {
                helmet: true,
                armor: false,
                cape: true,
            },
        )
        .unwrap();
        let (config, _) = load_app_config(&path).unwrap();
        assert_eq!(
            (
                config.presets.helmet,
                config.presets.armor,
                config.presets.cape
            ),
            (true, false, true)
        );
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn unknown_tag_is_rejected() {
        let text = "[presets.slots.preset_1]\nfaction = \"cyborgs\"\n";
        assert!(toml::from_str::<AppConfig>(text).is_err());
    }
}
