//! Captured equipment names and their persistence, independent of automation.
use std::path::Path;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use super::{
    LOCAL_TEMPLATES_DIR, PresetFile, load_preset_file, load_template_image, save_template_image,
    validate_preset_name, write_preset_file,
};
use crate::{
    item::EquipmentKind,
    vision::{
        equipment::{EquipmentObserver, EquipmentText},
        text::TextSample,
    },
};

pub(crate) struct EquipmentItem {
    pub name: TextSample,
    pub category: Option<TextSample>,
    pub client_size: (u32, u32),
    /// HellLocker: the equipped item's tile from the list page, for previews only.
    pub picture: Option<image::RgbaImage>,
}

pub(crate) type EquipmentSet = [Option<EquipmentItem>; 6];

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct SavedEquipment {
    pub(super) client_size: (u32, u32),
    // Same order as the six entry cards: helmet, armor, cape, primary, secondary, throwable.
    pub(super) items: [SavedItem; 6],
}

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct SavedItem {
    pub(super) name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    category: Option<String>,
    /// A repaired category can be captured at a different resolution from the names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    category_client_size: Option<(u32, u32)>,
    /// HellLocker: the item's picture, for previews only (absent in older saves).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) picture: Option<String>,
}

/// HellLocker: whether this preset already has all six items saved, so a save may
/// leave some of them (the attire) as they are.
pub(crate) fn has_saved(path: &Path, name: &str) -> bool {
    path.exists() && load_preset_file(path).is_ok_and(|file| file.equipment.contains_key(name))
}

/// Saves the sampled items. An item left out (None) keeps what the preset already has,
/// which needs a full save of that preset first.
pub(crate) fn save(path: &Path, name: &str, items: &EquipmentSet) -> Result<()> {
    validate_preset_name(name)?;
    let sampled = items
        .iter()
        .flatten()
        .next()
        .context("no equipment items were sampled")?;
    let client_size = sampled.client_size;
    let mut file = if path.exists() {
        load_preset_file(path)?
    } else {
        PresetFile::default()
    };
    let previous = file.equipment.remove(name);
    if items.iter().any(Option::is_none) {
        let previous = previous.as_ref().context(
            "tick Helmet, Armor and Cape and save this preset once before leaving any of them out",
        )?;
        // Kept items' names are matched against the client size they were saved at.
        ensure!(
            previous.client_size == client_size,
            "the game resolution changed since this preset was saved; tick Helmet, Armor and Cape and save it again"
        );
    }
    let saved = std::array::from_fn(|index| {
        if items[index].is_none() {
            let kept = &previous.as_ref().expect("checked above").items[index];
            return SavedItem {
                name: kept.name.clone(),
                category: kept.category.clone(),
                category_client_size: kept.category_client_size,
                picture: kept.picture.clone(),
            };
        }
        SavedItem {
            name: format!(
                "{LOCAL_TEMPLATES_DIR}/{name}/equipment/{}-name.png",
                index + 1
            ),
            category: items[index].as_ref().unwrap().category.as_ref().map(|_| {
                format!(
                    "{LOCAL_TEMPLATES_DIR}/{name}/equipment/{}-category.png",
                    index + 1
                )
            }),
            category_client_size: None,
            picture: items[index].as_ref().unwrap().picture.as_ref().map(|_| {
                format!(
                    "{LOCAL_TEMPLATES_DIR}/{name}/equipment/{}-picture.png",
                    index + 1
                )
            }),
        }
    });
    for (item, target) in items.iter().zip(&saved) {
        let Some(item) = item else { continue };
        save_template_image(path, &target.name, &item.name.image)?;
        if let Some((category, relative)) = item.category.as_ref().zip(target.category.as_ref()) {
            save_template_image(path, relative, &category.image)?;
        }
        if let Some((picture, relative)) = item.picture.as_ref().zip(target.picture.as_ref()) {
            save_template_image(path, relative, picture)?;
        }
    }
    file.equipment.insert(
        name.to_owned(),
        SavedEquipment {
            client_size,
            items: saved,
        },
    );
    write_preset_file(path, &file)
}

/// Repair only the category hint of a successfully applied item; keep its name template.
pub(crate) fn update_category(
    path: &Path,
    name: &str,
    kind: EquipmentKind,
    category: &TextSample,
    client_size: (u32, u32),
) -> Result<()> {
    validate_preset_name(name)?;
    let mut file = load_preset_file(path)?;
    let saved = file
        .equipment
        .get_mut(name)
        .with_context(|| format!("equipment preset not found: {name}"))?;
    let category_client_size = (client_size != saved.client_size).then_some(client_size);
    let item = &mut saved.items[kind.index()];
    let relative = item.category.clone().unwrap_or_else(|| {
        format!(
            "{LOCAL_TEMPLATES_DIR}/{name}/equipment/{}-category.png",
            kind.index() + 1
        )
    });
    save_template_image(path, &relative, &category.image)?;
    if item.category.is_none() || item.category_client_size != category_client_size {
        item.category = Some(relative);
        item.category_client_size = category_client_size;
        write_preset_file(path, &file)?;
    }
    Ok(())
}

pub(crate) fn load(path: &Path, name: &str) -> Result<EquipmentSet> {
    let file = if path.exists() {
        load_preset_file(path)?
    } else {
        PresetFile::default()
    };
    let saved = file.equipment.get(name).with_context(|| {
        format!("equipment preset {name} has not been saved; use Save preset in the panel first")
    })?;
    let mut items = std::array::from_fn(|_| None);
    for (item, source) in items.iter_mut().zip(&saved.items) {
        *item = Some(EquipmentItem {
            name: load_text(path, &source.name, saved.client_size, EquipmentText::Name)?,
            category: source
                .category
                .as_deref()
                .map(|relative| {
                    load_text(
                        path,
                        relative,
                        source.category_client_size.unwrap_or(saved.client_size),
                        EquipmentText::Category,
                    )
                })
                .transpose()?,
            client_size: saved.client_size,
            picture: None,
        });
    }
    Ok(items)
}

pub(super) fn load_text(
    path: &Path,
    relative: &str,
    client_size: (u32, u32),
    field: EquipmentText,
) -> Result<TextSample> {
    let (observer, _) =
        EquipmentObserver::resolve(client_size.0, client_size.1, EquipmentKind::Helmet)?;
    let (rect, geometry) = observer.text_region(field);
    let image = load_template_image(path, relative)?;
    ensure!(
        image.dimensions() == (rect.w, rect.h),
        "equipment template {relative} does not match its saved capture geometry; save it again"
    );
    Ok(TextSample::extract(image, geometry))
}

#[cfg(test)]
mod hell_locker_tests {
    use super::*;

    #[test]
    fn items_saved_before_pictures_still_load() {
        let old = r#"{"name":"local_templates/preset_1/equipment/1-name.png"}"#;
        let item: SavedItem = serde_json::from_str(old).unwrap();
        assert!(item.picture.is_none());
        // And a picture-less item is written without the new key.
        assert!(!serde_json::to_string(&item).unwrap().contains("picture"));
    }

    fn item(client_size: (u32, u32), shade: u8) -> EquipmentItem {
        let (observer, _) =
            EquipmentObserver::resolve(client_size.0, client_size.1, EquipmentKind::Helmet)
                .unwrap();
        let (rect, geometry) = observer.text_region(EquipmentText::Name);
        let image =
            image::RgbaImage::from_pixel(rect.w, rect.h, image::Rgba([shade, shade, shade, 255]));
        EquipmentItem {
            name: TextSample::extract(image, geometry),
            category: None,
            client_size,
            picture: Some(image::RgbaImage::from_pixel(
                40,
                40,
                image::Rgba([shade, 0, 0, 255]),
            )),
        }
    }

    fn temp_presets(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("hl-equipment-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("presets.json")
    }

    #[test]
    fn saving_without_attire_keeps_the_saved_attire() {
        let path = temp_presets("keep");
        let size = (1920, 1080);
        let full: EquipmentSet = std::array::from_fn(|i| Some(item(size, 10 + i as u8)));
        save(&path, "preset_1", &full).unwrap();
        assert!(has_saved(&path, "preset_1"));
        let before = load_preset_file(&path).unwrap();
        let before = &before.equipment["preset_1"];
        // Weapons and grenade only (slots 3, 4, 5).
        let partial: EquipmentSet =
            std::array::from_fn(|i| (i >= 3).then(|| item(size, 200 + i as u8)));
        save(&path, "preset_1", &partial).unwrap();
        let after = load_preset_file(&path).unwrap();
        let after = &after.equipment["preset_1"];
        for i in 0..3 {
            assert_eq!(after.items[i].name, before.items[i].name);
            assert_eq!(after.items[i].picture, before.items[i].picture);
        }
        // All six still load for applying.
        assert!(load(&path, "preset_1").unwrap().iter().all(Option::is_some));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn saving_without_attire_needs_a_full_save_first() {
        let path = temp_presets("first");
        let partial: EquipmentSet =
            std::array::from_fn(|i| (i >= 3).then(|| item((1920, 1080), 9)));
        assert!(!has_saved(&path, "preset_2"));
        let error = save(&path, "preset_2", &partial).unwrap_err();
        assert!(format!("{error:#}").contains("Helmet, Armor and Cape"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn attire_selection() {
        use crate::item::AttireChoice;
        assert_eq!(EquipmentKind::selected(AttireChoice::ALL).len(), 6);
        let none = AttireChoice {
            helmet: false,
            armor: false,
            cape: false,
        };
        let kinds = EquipmentKind::selected(none);
        assert_eq!(kinds.len(), 3);
        assert!(kinds.iter().all(|kind| !kind.is_attire()));
        // Each box on its own: only armor left out.
        let kinds = EquipmentKind::selected(AttireChoice {
            armor: false,
            ..AttireChoice::ALL
        });
        assert_eq!(kinds.len(), 5);
        assert!(
            !kinds
                .iter()
                .any(|kind| matches!(kind, EquipmentKind::Armor))
        );
    }

    #[test]
    fn pictures_round_trip() {
        let new =
            r#"{"name":"n.png","picture":"local_templates/preset_1/equipment/1-picture.png"}"#;
        let item: SavedItem = serde_json::from_str(new).unwrap();
        assert_eq!(
            item.picture.as_deref(),
            Some("local_templates/preset_1/equipment/1-picture.png")
        );
    }
}
