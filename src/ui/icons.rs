//! HellLocker 1.9.5: preset icons in the panel — the pictures on the list's rows and
//! the picker on the right side.
use std::rc::Rc;

use slint::VecModel;

use super::{AppUi, IconChoice, PresetInfo, PresetRow, image};
use crate::preset_icons::SLOTS;

impl AppUi {
    /// The picture for an icon id, read once and then kept.
    pub(super) fn icon_picture(&self, id: &str) -> Option<slint::Image> {
        if id.is_empty() {
            return None;
        }
        self.icon_cache
            .borrow_mut()
            .entry(id.to_owned())
            .or_insert_with(|| self.icons.picture(id).map(|picture| image(&picture)))
            .clone()
    }

    /// Fill a list row's icon fields from the preset's chosen icons.
    pub(super) fn fill_icons(&self, row: &mut PresetRow, info: &PresetInfo) {
        let [a, b] = &info.icons;
        let picture_a = self.icon_picture(a);
        let picture_b = self.icon_picture(b);
        row.icon_a_id = a.as_str().into();
        row.has_icon_a = picture_a.is_some();
        row.icon_a = picture_a.unwrap_or_default();
        row.icon_b_id = b.as_str().into();
        row.has_icon_b = picture_b.is_some();
        row.icon_b = picture_b.unwrap_or_default();
    }

    /// Whether the list keeps room for a second icon on every row.
    pub(super) fn publish_icon_room(&self) {
        let any = self.rows.iter().any(|row| row.has_icon_b);
        self.window.set_any_second_icon(any);
    }

    /// A preset's icons were saved.
    pub fn set_icons(&mut self, preset: &str, icons: [String; SLOTS]) {
        let Some(info) = self.preset_info.get_mut(preset) else {
            return;
        };
        info.icons = icons;
        let info = info.clone();
        if let Some(index) = self.rows.iter().position(|row| row.name == preset) {
            let mut row = self.rows[index].clone();
            self.fill_icons(&mut row, &info);
            self.rows[index] = row;
        }
        self.publish(None);
    }

    /// A preset's stored icons.
    pub fn icons_of(&self, preset: &str) -> [String; SLOTS] {
        self.preset_info
            .get(preset)
            .map(|info| info.icons.clone())
            .unwrap_or_default()
    }

    /// The picker opened: look at the icons folder again, in case pictures were added
    /// (or changed: their kept pictures are dropped).
    pub fn refresh_icons(&mut self) {
        self.icons.rescan();
        self.icon_cache
            .borrow_mut()
            .retain(|id, _| !id.starts_with("custom:"));
        let present = self.icons.groups_present();
        if self.window.get_icon_group() as usize >= present.len()
            || !present[self.window.get_icon_group() as usize]
        {
            self.window.set_icon_group(0);
        }
        self.window
            .set_icon_groups(Rc::new(VecModel::from(present.to_vec())).into());
        // Rows showing the player's own pictures pick up a changed file.
        let infos: Vec<_> = self.preset_info.values().cloned().collect();
        for info in infos {
            if info.icons.iter().any(|id| id.starts_with("custom:"))
                && let Some(index) = self.rows.iter().position(|row| row.name == info.name)
            {
                let mut row = self.rows[index].clone();
                self.fill_icons(&mut row, &info);
                self.rows[index] = row;
            }
        }
        self.publish(None);
    }

    /// Show the icons matching the picker's search text and group.
    pub fn filter_icons(&self, search: &str, group: i32) {
        let choices: Vec<IconChoice> = self
            .icons
            .find(search, group)
            .into_iter()
            .filter_map(|entry| {
                Some(IconChoice {
                    id: entry.id.as_str().into(),
                    name: entry.name.as_str().into(),
                    group: entry.group as i32,
                    tag: entry.tag,
                    picture: self.icon_picture(&entry.id)?,
                })
            })
            .collect();
        self.window
            .set_icon_choices(Rc::new(VecModel::from(choices)).into());
    }

    /// The folder for the player's own icons.
    pub fn icons_dir(&self) -> std::path::PathBuf {
        self.icons.dir().to_path_buf()
    }
}
