pub(crate) mod equipment;
pub(crate) mod library;
pub(crate) mod names;
pub(crate) mod preview;
pub(crate) mod stratagems;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use image::RgbaImage;
use serde::{Deserialize, Serialize};

use crate::assets::{decode_rgba8, parse_json_file};
use stratagems::StratagemPreset;

const PRESET_SCHEMA_VERSION: u32 = 1;
const LOCAL_TEMPLATES_DIR: &str = "local_templates";

/// Sections to save or apply. Unchecked sections are left unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PresetScope {
    pub stratagems: bool,
    pub equipment: bool,
}

impl Default for PresetScope {
    fn default() -> Self {
        Self {
            stratagems: true,
            equipment: true,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct PresetFile {
    #[serde(default)]
    schema_version: u32,
    presets: BTreeMap<String, StratagemPreset>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    equipment: BTreeMap<String, equipment::SavedEquipment>,
}

impl Default for PresetFile {
    fn default() -> Self {
        Self {
            schema_version: PRESET_SCHEMA_VERSION,
            presets: BTreeMap::new(),
            equipment: BTreeMap::new(),
        }
    }
}

pub fn saved_sections(path: &Path, name: &str) -> Result<(bool, bool)> {
    let file = load_preset_file(path)?;
    Ok((
        file.presets.contains_key(name),
        file.equipment.contains_key(name),
    ))
}

/// Remove both saved sections and only this preset's own template directory.
pub fn remove(path: &Path, name: &str) -> Result<()> {
    validate_preset_name(name)?;
    let directory = resolve_template_path(path, &format!("{LOCAL_TEMPLATES_DIR}/{name}"))?;
    if directory.exists() {
        let root = directory
            .parent()
            .expect("template directory has a parent")
            .canonicalize()?;
        ensure!(
            directory.canonicalize()?.parent() == Some(root.as_path()),
            "preset template directory resolves outside its template root"
        );
    }
    if path.exists() {
        let mut file = load_preset_file(path)?;
        file.presets.remove(name);
        file.equipment.remove(name);
        write_preset_file(path, &file)?;
    }
    match fs::remove_dir_all(&directory) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error)
            .with_context(|| format!("failed to remove preset templates {}", directory.display())),
    }
}

/// HellLocker 1.9.4: copy `from`'s saved Stratagems and equipment, with their pictures,
/// to the new preset `to`. Nothing to copy (an unsaved preset) is fine.
pub fn duplicate(path: &Path, from: &str, to: &str) -> Result<()> {
    validate_preset_name(from)?;
    validate_preset_name(to)?;
    if !path.exists() {
        return Ok(());
    }
    let mut file = load_preset_file(path)?;
    ensure!(
        !file.presets.contains_key(to) && !file.equipment.contains_key(to),
        "preset {to} already has saved content"
    );
    // Saved entries point at their pictures by path; point the copy at its own.
    let old_prefix = format!("{LOCAL_TEMPLATES_DIR}/{from}/");
    let new_prefix = format!("{LOCAL_TEMPLATES_DIR}/{to}/");
    let repoint = |value: serde_json::Value| repoint_paths(value, &old_prefix, &new_prefix);
    if let Some(stratagems) = file.presets.get(from) {
        let copy = serde_json::from_value(repoint(serde_json::to_value(stratagems)?))?;
        file.presets.insert(to.to_owned(), copy);
    }
    if let Some(equipment) = file.equipment.get(from) {
        let copy = serde_json::from_value(repoint(serde_json::to_value(equipment)?))?;
        file.equipment.insert(to.to_owned(), copy);
    }
    let source = resolve_template_path(path, &format!("{LOCAL_TEMPLATES_DIR}/{from}"))?;
    let target = resolve_template_path(path, &format!("{LOCAL_TEMPLATES_DIR}/{to}"))?;
    if source.is_dir() {
        if target.exists() {
            fs::remove_dir_all(&target)
                .with_context(|| format!("failed to clear {}", target.display()))?;
        }
        copy_dir(&source, &target)
            .with_context(|| format!("failed to copy the pictures of {from}"))?;
    }
    write_preset_file(path, &file)
}

fn repoint_paths(value: serde_json::Value, old: &str, new: &str) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::String(text) => Value::String(match text.strip_prefix(old) {
            Some(rest) => format!("{new}{rest}"),
            None => text,
        }),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| repoint_paths(item, old, new))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, item)| (key, repoint_paths(item, old, new)))
                .collect(),
        ),
        other => other,
    }
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

pub(crate) fn archive_legacy_preset_file(path: &Path) -> Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }

    let document: serde_json::Value = parse_json_file(path)?;
    let schema_version = document
        .get("schema_version")
        .and_then(serde_json::Value::as_u64);

    match schema_version {
        Some(version) if version == u64::from(PRESET_SCHEMA_VERSION) => return Ok(None),
        Some(version) if version > u64::from(PRESET_SCHEMA_VERSION) => {
            bail!(
                "preset data uses schema version {version}, but this version only supports schema version {PRESET_SCHEMA_VERSION}"
            );
        }
        _ => {}
    }

    let backup = path.with_file_name("presets.backup.json");
    if backup.exists() {
        fs::remove_file(&backup)
            .with_context(|| format!("failed to replace {}", backup.display()))?;
    }

    fs::rename(path, &backup).with_context(|| {
        format!(
            "failed to archive legacy preset data from {} to {}",
            path.display(),
            backup.display()
        )
    })?;
    Ok(Some(backup))
}

pub fn resolve_template_path(presets_path: &Path, relative: &str) -> Result<PathBuf> {
    let relative_path = Path::new(relative);
    ensure!(
        !relative_path.is_absolute(),
        "local template path must be relative"
    );
    ensure!(
        relative_path
            .components()
            .all(|component| matches!(component, Component::Normal(_))),
        "local template path contains an invalid component"
    );
    ensure!(
        relative_path.starts_with(LOCAL_TEMPLATES_DIR),
        "local template path must be inside {LOCAL_TEMPLATES_DIR}"
    );
    let base = presets_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    Ok(base.join(relative_path))
}

pub fn load_template_image(presets_path: &Path, relative: &str) -> Result<RgbaImage> {
    let path = resolve_template_path(presets_path, relative)?;
    let bytes = std::fs::read(&path)
        .with_context(|| format!("failed to read local template {}", path.display()))?;
    decode_rgba8(&bytes)
        .with_context(|| format!("failed to decode local template {}", path.display()))
}

fn load_preset_file(path: &Path) -> Result<PresetFile> {
    let presets: PresetFile = parse_json_file(path)?;
    ensure!(
        presets.schema_version == PRESET_SCHEMA_VERSION,
        "unsupported preset schema version {}; expected {}. Delete the old preset data before continuing",
        presets.schema_version,
        PRESET_SCHEMA_VERSION
    );
    Ok(presets)
}

fn write_preset_file(path: &Path, presets: &PresetFile) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(presets)?;
    fs::write(path, json).with_context(|| format!("failed to write {}", path.display()))
}

fn validate_preset_name(name: &str) -> Result<()> {
    ensure!(!name.is_empty(), "preset name must not be empty");
    ensure!(
        name.bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')),
        "preset name contains characters that cannot be used in a template directory: {name:?}"
    );
    Ok(())
}

fn save_template_image(presets_path: &Path, relative: &str, image: &RgbaImage) -> Result<()> {
    ensure!(
        image.width() > 0 && image.height() > 0,
        "cannot save an empty local template"
    );
    let destination = resolve_template_path(presets_path, relative)?;
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    image
        .save(&destination)
        .with_context(|| format!("failed to save local template {}", destination.display()))
}

#[cfg(test)]
mod duplicate_tests {
    use super::*;

    #[test]
    fn picture_paths_follow_the_copy() {
        let value = serde_json::json!({
            "name": "local_templates/preset_3/equipment/1-name.png",
            "other": "local_templates/preset_30/x.png",
            "list": ["local_templates/preset_3/a.png", 4],
        });
        let copied = repoint_paths(
            value,
            "local_templates/preset_3/",
            "local_templates/preset_9/",
        );
        assert_eq!(
            copied["name"],
            "local_templates/preset_9/equipment/1-name.png"
        );
        // Another preset whose name starts the same is left alone.
        assert_eq!(copied["other"], "local_templates/preset_30/x.png");
        assert_eq!(copied["list"][0], "local_templates/preset_9/a.png");
        assert_eq!(copied["list"][1], 4);
    }
}
