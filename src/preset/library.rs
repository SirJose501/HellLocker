//! HellLocker: the equipment picture library, filled by Scan equipment.
//!
//! Each entry is an item's name (as read on its list page) and the picture of its tile,
//! saved once the game had finished loading it. Saving a preset takes an item's picture
//! from here instead of waiting for the tile to load, and presets saved without
//! pictures show them from here too. The library only affects pictures; saving and
//! applying still read names from the screen.
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use image::RgbaImage;
use serde::{Deserialize, Serialize};

use super::{LOCAL_TEMPLATES_DIR, load_template_image, resolve_template_path, save_template_image};
use crate::{
    item::EquipmentKind,
    vision::{equipment::EquipmentText, text::TextSample},
};

/// The pseudo preset name a scan runs under (status, logs and failure reports).
pub(crate) const SCAN_PRESET: &str = "equipment_scan";

const LIBRARY_DIR: &str = "library";
const LIBRARY_VERSION: u32 = 1;
/// Same identity rules as the equipment cache: a clear best match, well ahead of the rest.
const MATCH_THRESHOLD: f32 = 0.90;
const MATCH_MARGIN: f32 = 0.04;
/// Two reads of one item's name during a scan.
const DUPLICATE_THRESHOLD: f32 = 0.97;

#[derive(Default, Serialize, Deserialize)]
struct LibraryFile {
    version: u32,
    /// One list per equipment slot, in `EquipmentKind::ALL` order.
    kinds: Vec<KindFile>,
}

#[derive(Default, Serialize, Deserialize)]
struct KindFile {
    /// The client size the names were read at; names are compared across sizes.
    client_size: (u32, u32),
    items: Vec<ItemFile>,
}

#[derive(Serialize, Deserialize)]
struct ItemFile {
    name: String,
    picture: String,
}

/// The scanned items, loaded for one save or one preview refresh.
pub(crate) struct Library {
    kinds: Vec<Vec<(TextSample, String)>>,
}

impl Library {
    /// An empty library when none was scanned yet or it cannot be read.
    pub fn load(presets: &Path) -> Self {
        let mut kinds = vec![Vec::new(); EquipmentKind::ALL.len()];
        match read_file(presets) {
            Ok(Some(file)) => {
                for (index, kind) in file.kinds.iter().enumerate().take(kinds.len()) {
                    for item in &kind.items {
                        match super::equipment::load_text(
                            presets,
                            &item.name,
                            kind.client_size,
                            EquipmentText::Name,
                        ) {
                            Ok(name) => kinds[index].push((name, item.picture.clone())),
                            Err(error) => tracing::debug!(
                                name = %item.name, error = %format!("{error:#}"),
                                "skipping unreadable library entry"
                            ),
                        }
                    }
                }
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(error = %format!("{error:#}"), "equipment library could not be read")
            }
        }
        Self { kinds }
    }

    pub fn is_empty(&self) -> bool {
        self.kinds.iter().all(Vec::is_empty)
    }

    /// The picture saved for the item with this name, when one entry clearly matches.
    pub fn picture(
        &self,
        presets: &Path,
        kind: EquipmentKind,
        name: &TextSample,
    ) -> Option<RgbaImage> {
        let mut scores = self.kinds[kind.index()]
            .iter()
            .map(|(entry, picture)| (entry.cosine(name), picture))
            .collect::<Vec<_>>();
        scores.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
        let (best, picture) = scores.first()?;
        let runner_up = scores.get(1).map_or(0.0, |entry| entry.0);
        if *best < MATCH_THRESHOLD || best - runner_up < MATCH_MARGIN {
            return None;
        }
        load_template_image(presets, picture).ok()
    }
}

fn library_path(presets: &Path) -> Result<std::path::PathBuf> {
    resolve_template_path(
        presets,
        &format!("{LOCAL_TEMPLATES_DIR}/{LIBRARY_DIR}/library.json"),
    )
}

fn read_file(presets: &Path) -> Result<Option<LibraryFile>> {
    let path = library_path(presets)?;
    if !path.exists() {
        return Ok(None);
    }
    let file: LibraryFile = crate::assets::parse_json_file(&path)?;
    Ok((file.version == LIBRARY_VERSION).then_some(file))
}

/// Collects one slot's items during a scan, skipping repeat visits to the same item.
pub(crate) struct ScanBatch {
    pub kind: EquipmentKind,
    pub client_size: (u32, u32),
    items: Vec<(TextSample, RgbaImage)>,
}

impl ScanBatch {
    pub fn new(kind: EquipmentKind, client_size: (u32, u32)) -> Self {
        Self {
            kind,
            client_size,
            items: Vec::new(),
        }
    }

    /// False when this name was already scanned in this batch.
    pub fn is_new(&self, name: &TextSample) -> bool {
        !self
            .items
            .iter()
            .any(|(seen, _)| seen.cosine(name) >= DUPLICATE_THRESHOLD)
    }

    pub fn push(&mut self, name: TextSample, picture: RgbaImage) {
        self.items.push((name, picture));
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }
}

/// Replaces one slot's part of the library with a finished scan of it.
pub(crate) fn save_batch(presets: &Path, batch: &ScanBatch) -> Result<()> {
    let mut file = read_file(presets).ok().flatten().unwrap_or_default();
    file.version = LIBRARY_VERSION;
    file.kinds
        .resize_with(EquipmentKind::ALL.len(), KindFile::default);
    let slot = batch.kind.index();
    // Older files of this slot are replaced by the new numbering.
    let dir = resolve_template_path(
        presets,
        &format!("{LOCAL_TEMPLATES_DIR}/{LIBRARY_DIR}/{}", slot + 1),
    )?;
    if dir.exists() {
        fs::remove_dir_all(&dir).with_context(|| format!("failed to clear {}", dir.display()))?;
    }
    let mut items = Vec::with_capacity(batch.items.len());
    for (index, (name, picture)) in batch.items.iter().enumerate() {
        let base = format!(
            "{LOCAL_TEMPLATES_DIR}/{LIBRARY_DIR}/{}/{}",
            slot + 1,
            index + 1
        );
        let item = ItemFile {
            name: format!("{base}-name.png"),
            picture: format!("{base}-picture.png"),
        };
        save_template_image(presets, &item.name, &name.image)?;
        save_template_image(presets, &item.picture, picture)?;
        items.push(item);
    }
    file.kinds[slot] = KindFile {
        client_size: batch.client_size,
        items,
    };
    let path = library_path(presets)?;
    fs::write(&path, serde_json::to_string_pretty(&file)?)
        .with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vision::equipment::EquipmentObserver;

    fn name(size: (u32, u32), seed: u32) -> TextSample {
        let (observer, _) =
            EquipmentObserver::resolve(size.0, size.1, EquipmentKind::Helmet).unwrap();
        let (rect, geometry) = observer.text_region(EquipmentText::Name);
        // Distinct white "glyph" bars on black, so each seed reads as a different name.
        let image = RgbaImage::from_fn(rect.w, rect.h, |x, y| {
            let on =
                y > rect.h / 4 && y < rect.h * 3 / 4 && (x / 7 + seed).is_multiple_of(seed % 5 + 2);
            let v = if on { 255 } else { 0 };
            image::Rgba([v, v, v, 255])
        });
        TextSample::extract(image, geometry)
    }

    #[test]
    fn scanned_pictures_are_found_by_name() {
        let dir = std::env::temp_dir().join(format!("hl-library-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let presets = dir.join("presets.json");
        let size = (1920, 1080);
        assert!(Library::load(&presets).is_empty());
        let mut batch = ScanBatch::new(EquipmentKind::Primary, size);
        for seed in 0..3 {
            let sample = name(size, seed);
            if batch.is_new(&sample) {
                batch.push(
                    sample,
                    RgbaImage::from_pixel(30, 20, image::Rgba([seed as u8 * 80, 0, 0, 255])),
                );
            }
        }
        // A repeat visit to an item is not added twice.
        assert!(!batch.is_new(&name(size, 1)));
        save_batch(&presets, &batch).unwrap();
        let library = Library::load(&presets);
        assert!(!library.is_empty());
        let found = library
            .picture(&presets, EquipmentKind::Primary, &name(size, 2))
            .unwrap();
        assert_eq!(found.get_pixel(0, 0).0[0], 160);
        // Pictures are per slot: the same name finds nothing among secondaries.
        assert!(
            library
                .picture(&presets, EquipmentKind::Secondary, &name(size, 2))
                .is_none()
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
