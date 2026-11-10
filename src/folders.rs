//! HellLocker: preset folders and the order of the preset list.
//!
//! The layout is a list of folders, each holding preset IDs in order, followed by the
//! presets that are in no folder. It is kept in `config.toml` as `presets.folders` and
//! `presets.order` and only arranges the list: presets themselves live in
//! `presets.slots` as before, and a preset missing from the layout is listed last.

/// One folder, as stored in `presets.folders`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PresetFolder {
    pub id: String,
    pub name: String,
    pub open: bool,
    pub presets: Vec<String>,
    /// HellLocker 1.9.6: the folder's logo (a crate::preset_icons id; "" = the folder
    /// symbol).
    pub icon: String,
}

impl Default for PresetFolder {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            open: true,
            presets: Vec::new(),
            icon: String::new(),
        }
    }
}

/// Where a dragged preset lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Drop {
    /// At the end of a folder.
    IntoFolder(String),
    /// Just before or after another preset, in that preset's folder (or none).
    Before(String),
    After(String),
    /// At the end of the list, outside every folder.
    End,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Layout {
    pub folders: Vec<PresetFolder>,
    /// Presets in no folder, in list order.
    pub loose: Vec<String>,
}

pub const MAX_FOLDER_NAME: usize = 40;

impl Layout {
    /// The stored layout, made consistent with the presets that exist: unknown or
    /// repeated presets are dropped, and presets it does not mention are added to the
    /// end, by preset number.
    pub fn new<'a>(
        folders: Vec<PresetFolder>,
        order: Vec<String>,
        presets: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        let mut all: Vec<&str> = presets.into_iter().collect();
        all.sort_by_key(|name| (preset_order(name), name.to_string()));
        let exists = |name: &str| all.contains(&name);
        let mut seen: Vec<String> = Vec::new();
        let take = |name: String, seen: &mut Vec<String>| {
            (exists(&name) && !seen.contains(&name)).then(|| {
                seen.push(name.clone());
                name
            })
        };
        let mut used_ids: Vec<String> = Vec::new();
        let mut layout = Self::default();
        for mut folder in folders {
            folder.presets = folder
                .presets
                .into_iter()
                .filter_map(|name| take(name, &mut seen))
                .collect();
            folder.name = clean_name(&folder.name);
            folder.icon = crate::preset_icons::clean_id(&folder.icon);
            if folder.id.is_empty() || used_ids.contains(&folder.id) {
                folder.id = next_id(&used_ids);
            }
            used_ids.push(folder.id.clone());
            layout.folders.push(folder);
        }
        layout.loose = order
            .into_iter()
            .filter_map(|name| take(name, &mut seen))
            .collect();
        for name in all {
            if !seen.iter().any(|seen| seen == name) {
                layout.loose.push(name.to_string());
            }
        }
        layout
    }

    pub fn folder(&self, id: &str) -> Option<&PresetFolder> {
        self.folders.iter().find(|folder| folder.id == id)
    }

    /// The folder a preset is in, if any.
    pub fn folder_of(&self, preset: &str) -> Option<&str> {
        self.folders
            .iter()
            .find(|folder| folder.presets.iter().any(|name| name == preset))
            .map(|folder| folder.id.as_str())
    }

    /// A new, open folder at the end of the folders; returns its ID.
    pub fn add_folder(&mut self, name: &str) -> String {
        let ids: Vec<String> = self.folders.iter().map(|f| f.id.clone()).collect();
        let id = next_id(&ids);
        let name = match clean_name(name) {
            name if name.is_empty() => self.unused_name(),
            name => name,
        };
        self.folders.push(PresetFolder {
            id: id.clone(),
            name,
            ..Default::default()
        });
        id
    }

    fn unused_name(&self) -> String {
        (1..)
            .map(|n| {
                if n == 1 {
                    "New folder".to_string()
                } else {
                    format!("New folder {n}")
                }
            })
            .find(|name| self.folders.iter().all(|folder| &folder.name != name))
            .expect("an unused name exists")
    }

    /// False when there is no such folder. An empty name keeps the old one.
    pub fn rename_folder(&mut self, id: &str, name: &str) -> bool {
        let name = clean_name(name);
        match self.folders.iter_mut().find(|folder| folder.id == id) {
            Some(folder) => {
                if !name.is_empty() {
                    folder.name = name;
                }
                true
            }
            None => false,
        }
    }

    /// Removes a folder; its presets are kept, at the top of the presets in no folder.
    pub fn delete_folder(&mut self, id: &str) -> bool {
        let Some(index) = self.folders.iter().position(|folder| folder.id == id) else {
            return false;
        };
        let folder = self.folders.remove(index);
        self.loose.splice(0..0, folder.presets);
        true
    }

    /// HellLocker 1.9.6: a folder's logo ("" = the folder symbol).
    pub fn set_icon(&mut self, id: &str, icon: &str) -> bool {
        let icon = crate::preset_icons::clean_id(icon);
        match self.folders.iter_mut().find(|folder| folder.id == id) {
            Some(folder) if folder.icon != icon => {
                folder.icon = icon;
                true
            }
            _ => false,
        }
    }

    pub fn set_open(&mut self, id: &str, open: bool) -> bool {
        match self.folders.iter_mut().find(|folder| folder.id == id) {
            Some(folder) => {
                folder.open = open;
                true
            }
            None => false,
        }
    }

    /// Moves a preset; false (and nothing changes) when the move has no valid target.
    pub fn move_preset(&mut self, preset: &str, drop: &Drop) -> bool {
        let target_ok = match drop {
            Drop::IntoFolder(id) => self.folder(id).is_some(),
            Drop::Before(other) | Drop::After(other) => other != preset && self.contains(other),
            Drop::End => true,
        };
        if !target_ok || !self.contains(preset) {
            return false;
        }
        self.remove_preset(preset);
        let name = preset.to_string();
        match drop {
            Drop::IntoFolder(id) => {
                let folder = self
                    .folders
                    .iter_mut()
                    .find(|folder| &folder.id == id)
                    .expect("checked above");
                folder.presets.push(name);
                folder.open = true;
            }
            Drop::Before(other) | Drop::After(other) => {
                let after = matches!(drop, Drop::After(_));
                let list = self.list_holding_mut(other).expect("checked above");
                let at = list.iter().position(|n| n == other).expect("in this list");
                list.insert(if after { at + 1 } else { at }, name);
            }
            Drop::End => self.loose.push(name),
        }
        true
    }

    /// A new preset goes at the end of the list, outside every folder.
    pub fn add_preset(&mut self, preset: &str) {
        if !self.contains(preset) {
            self.loose.push(preset.to_string());
        }
    }

    pub fn remove_preset(&mut self, preset: &str) {
        for folder in &mut self.folders {
            folder.presets.retain(|name| name != preset);
        }
        self.loose.retain(|name| name != preset);
    }

    fn contains(&self, preset: &str) -> bool {
        self.folder_of(preset).is_some() || self.loose.iter().any(|name| name == preset)
    }

    fn list_holding_mut(&mut self, preset: &str) -> Option<&mut Vec<String>> {
        if self.loose.iter().any(|name| name == preset) {
            return Some(&mut self.loose);
        }
        self.folders
            .iter_mut()
            .find(|folder| folder.presets.iter().any(|name| name == preset))
            .map(|folder| &mut folder.presets)
    }
}

/// `preset_7` sorts as 7; other names after every numbered preset.
fn preset_order(name: &str) -> u32 {
    name.strip_prefix("preset_")
        .and_then(|n| n.parse().ok())
        .unwrap_or(u32::MAX)
}

fn next_id(used: &[String]) -> String {
    (1..)
        .map(|n| format!("folder_{n}"))
        .find(|id| !used.contains(id))
        .expect("an unused id exists")
}

/// One line, trimmed, at most `MAX_FOLDER_NAME` characters.
pub fn clean_name(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_FOLDER_NAME)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[String]) -> Vec<&str> {
        list.iter().map(String::as_str).collect()
    }

    fn sample() -> Layout {
        Layout::new(
            vec![PresetFolder {
                id: "folder_1".into(),
                name: "Bugs".into(),
                open: false,
                presets: vec!["preset_2".into(), "preset_1".into()],
                ..Default::default()
            }],
            vec!["preset_4".into()],
            ["preset_1", "preset_2", "preset_3", "preset_4"],
        )
    }

    #[test]
    fn stored_layout_is_made_consistent_with_the_presets() {
        let layout = Layout::new(
            vec![
                PresetFolder {
                    id: "folder_1".into(),
                    name: "  Bugs \n team ".into(),
                    presets: vec!["preset_2".into(), "gone".into(), "preset_2".into()],
                    ..Default::default()
                },
                // A repeated ID gets a fresh one; a preset can only be in one place.
                PresetFolder {
                    id: "folder_1".into(),
                    name: "Bots".into(),
                    presets: vec!["preset_2".into(), "preset_3".into()],
                    ..Default::default()
                },
            ],
            vec!["preset_1".into(), "preset_3".into()],
            ["preset_1", "preset_2", "preset_3", "preset_10", "preset_4"],
        );
        assert_eq!(layout.folders[0].name, "Bugs team");
        assert_eq!(names(&layout.folders[0].presets), ["preset_2"]);
        assert_eq!(layout.folders[1].id, "folder_2");
        assert_eq!(names(&layout.folders[1].presets), ["preset_3"]);
        // Unmentioned presets follow, by number (4 before 10).
        assert_eq!(names(&layout.loose), ["preset_1", "preset_4", "preset_10"]);
    }

    #[test]
    fn dragging_into_folders_and_between_presets() {
        let mut layout = sample();
        // Onto a closed folder: appended, and the folder opens.
        assert!(layout.move_preset("preset_4", &Drop::IntoFolder("folder_1".into())));
        assert_eq!(
            names(&layout.folders[0].presets),
            ["preset_2", "preset_1", "preset_4"]
        );
        assert!(layout.folders[0].open);
        // Before a loose preset: leaves the folder.
        assert!(layout.move_preset("preset_1", &Drop::Before("preset_3".into())));
        assert_eq!(names(&layout.loose), ["preset_1", "preset_3"]);
        assert_eq!(layout.folder_of("preset_1"), None);
        // After a preset in a folder: joins that folder at that spot.
        assert!(layout.move_preset("preset_3", &Drop::After("preset_2".into())));
        assert_eq!(
            names(&layout.folders[0].presets),
            ["preset_2", "preset_3", "preset_4"]
        );
        // To the end, outside every folder.
        assert!(layout.move_preset("preset_2", &Drop::End));
        assert_eq!(names(&layout.loose), ["preset_1", "preset_2"]);
        // Nothing changes for invalid targets.
        let before = layout.clone();
        assert!(!layout.move_preset("preset_2", &Drop::Before("preset_2".into())));
        assert!(!layout.move_preset("preset_2", &Drop::IntoFolder("nope".into())));
        assert!(!layout.move_preset("missing", &Drop::End));
        assert_eq!(layout, before);
    }

    #[test]
    fn folders_are_added_renamed_and_deleted_without_losing_presets() {
        let mut layout = sample();
        let id = layout.add_folder("");
        assert_eq!(id, "folder_2");
        assert_eq!(layout.folder(&id).unwrap().name, "New folder");
        let second = layout.add_folder(" ");
        assert_eq!(layout.folder(&second).unwrap().name, "New folder 2");
        assert!(layout.rename_folder(&id, "  Bot   hunting "));
        assert_eq!(layout.folder(&id).unwrap().name, "Bot hunting");
        // An empty name keeps the old one; long names are cut.
        assert!(layout.rename_folder(&id, "   "));
        assert_eq!(layout.folder(&id).unwrap().name, "Bot hunting");
        layout.rename_folder(&id, &"x".repeat(80));
        assert_eq!(layout.folder(&id).unwrap().name.len(), MAX_FOLDER_NAME);
        // Deleting a folder keeps its presets, at the top of the loose ones.
        assert!(layout.delete_folder("folder_1"));
        assert_eq!(
            names(&layout.loose),
            ["preset_2", "preset_1", "preset_4", "preset_3"]
        );
        assert!(!layout.delete_folder("folder_1"));
        // New presets go last; removed ones disappear everywhere.
        layout.add_preset("preset_9");
        layout.remove_preset("preset_1");
        assert_eq!(
            names(&layout.loose),
            ["preset_2", "preset_4", "preset_3", "preset_9"]
        );
    }
}
