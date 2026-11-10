//! Read-only presentation data. No window, UI framework or current game state required.
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result};
use image::{Rgba, RgbaImage};

use super::{PresetFile, load_preset_file, load_template_image, stratagems};
use crate::{
    item::EquipmentKind,
    vision::{equipment::EquipmentText, text::TextSample},
};

#[derive(Clone, Copy, Debug)]
pub enum PreviewRole {
    Stratagem(usize),
    Booster,
    FallbackBooster,
    Equipment(EquipmentKind),
}

pub struct PreviewImage {
    pub role: PreviewRole,
    pub image: RgbaImage,
    /// HellLocker: an equipment item's picture, when it was saved with one.
    pub picture: Option<RgbaImage>,
    /// HellLocker 1.9.6: an equipment item's name as text, when it could be read.
    pub name: Option<String>,
}

pub enum PreviewContent {
    Missing,
    Ready(Vec<PreviewImage>),
    Invalid(String),
}

pub struct PresetPreview {
    pub stratagems: PreviewContent,
    pub equipment: PreviewContent,
}

pub fn load(path: &Path) -> Result<BTreeMap<String, PresetPreview>> {
    let file = if path.exists() {
        load_preset_file(path)?
    } else {
        PresetFile::default()
    };
    let names: BTreeSet<_> = file.presets.keys().chain(file.equipment.keys()).collect();
    // HellLocker: pictures for items saved without one come from Scan equipment.
    let library = super::library::Library::load(path);
    let mut reader = super::names::NameReader::new(path);
    let previews = names
        .into_iter()
        .map(|name| {
            let stratagems = content(file.presets.get(name).map(|preset| {
                stratagems::validate(name, preset)?;
                preset
                    .stratagems
                    .iter()
                    .enumerate()
                    .map(|(index, item)| (PreviewRole::Stratagem(index + 1), item))
                    .chain(
                        preset
                            .booster
                            .iter()
                            .map(|item| (PreviewRole::Booster, item)),
                    )
                    .chain(
                        preset
                            .fallback_booster
                            .iter()
                            .map(|item| (PreviewRole::FallbackBooster, item)),
                    )
                    .map(|(role, item)| {
                        Ok(PreviewImage {
                            role,
                            image: load_template_image(path, &item.path)?,
                            picture: None,
                            name: None,
                        })
                    })
                    .collect()
            }));
            let equipment = content(file.equipment.get(name).map(|equipment| {
                EquipmentKind::ALL
                    .iter()
                    .zip(&equipment.items)
                    .map(|(kind, item)| {
                        let name = super::equipment::load_text(
                            path,
                            &item.name,
                            equipment.client_size,
                            EquipmentText::Name,
                        )?;
                        // A missing or unreadable picture only leaves the preview without it.
                        let picture = item
                            .picture
                            .as_deref()
                            .and_then(|relative| load_template_image(path, relative).ok())
                            .or_else(|| library.picture(path, *kind, &name))
                            .map(trim_picture);
                        let image = name_preview(&name);
                        Ok(PreviewImage {
                            role: PreviewRole::Equipment(*kind),
                            name: reader.read(&image),
                            image,
                            picture,
                        })
                    })
                    .collect()
            }));
            (
                name.clone(),
                PresetPreview {
                    stratagems,
                    equipment,
                },
            )
        })
        .collect();
    reader.save();
    Ok(previews)
}

/// HellLocker 1.9.6: an item picture without its plain margins (transparent, or the
/// tile's flat background), so it fills the gear card. A picture that would lose too
/// much (no clear item in it) is kept as it is.
pub(crate) fn trim_picture(picture: RgbaImage) -> RgbaImage {
    let (width, height) = picture.dimensions();
    if width < 8 || height < 8 {
        return picture;
    }
    let see_through = picture.pixels().any(|pixel| pixel[3] < 250);
    let corners = [
        *picture.get_pixel(0, 0),
        *picture.get_pixel(width - 1, 0),
        *picture.get_pixel(0, height - 1),
        *picture.get_pixel(width - 1, height - 1),
    ];
    let background: [i32; 3] = std::array::from_fn(|channel| {
        corners
            .iter()
            .map(|pixel| i32::from(pixel[channel]))
            .sum::<i32>()
            / 4
    });
    let is_item = |pixel: &image::Rgba<u8>| {
        if see_through {
            pixel[3] > 24
        } else {
            (0..3).any(|channel| (i32::from(pixel[channel]) - background[channel]).abs() > 40)
        }
    };
    let (mut left, mut top, mut right, mut bottom) = (width, height, 0, 0);
    for (x, y, pixel) in picture.enumerate_pixels() {
        if is_item(pixel) {
            left = left.min(x);
            top = top.min(y);
            right = right.max(x);
            bottom = bottom.max(y);
        }
    }
    if right < left || bottom < top {
        return picture;
    }
    let (w, h) = (right - left + 1, bottom - top + 1);
    if w * h * 10 < width * height {
        return picture;
    }
    let pad = (w.max(h) / 24).max(1);
    let left = left.saturating_sub(pad);
    let top = top.saturating_sub(pad);
    let w = (w + 2 * pad).min(width - left);
    let h = (h + 2 * pad).min(height - top);
    image::imageops::crop_imm(&picture, left, top, w, h).to_image()
}

pub fn equipment_name(path: &Path, preset: &str, kind: EquipmentKind) -> Result<RgbaImage> {
    let file = load_preset_file(path)?;
    let equipment = file
        .equipment
        .get(preset)
        .context("equipment preset not saved")?;
    load_name(
        path,
        &equipment.items[kind.index()].name,
        equipment.client_size,
    )
}

fn load_name(path: &Path, name: &str, client_size: (u32, u32)) -> Result<RgbaImage> {
    Ok(name_preview(&super::equipment::load_text(
        path,
        name,
        client_size,
        EquipmentText::Name,
    )?))
}

fn content(result: Option<Result<Vec<PreviewImage>>>) -> PreviewContent {
    match result {
        None => PreviewContent::Missing,
        Some(Ok(images)) => PreviewContent::Ready(images),
        Some(Err(error)) => PreviewContent::Invalid(format!("{error:#}")),
    }
}

fn name_preview(sample: &TextSample) -> RgbaImage {
    let foreground = sample.foreground();
    let (width, height) = foreground.dimensions();
    // Keep the captured extent; presentation scales by height and clips overflow.
    // Use response as alpha only, so strokes are not dimmed twice.
    RgbaImage::from_fn(width, height, |x, y| {
        Rgba([255, 255, 255, foreground.get_pixel(x, y)[0]])
    })
}

/// Display-only crop of a centered heading. Identity keeps the complete text region.
pub(crate) fn equipment_category(sample: &TextSample) -> RgbaImage {
    let image = name_preview(sample);
    let (width, height) = image.dimensions();
    let mut projection = vec![0u32; width as usize];
    for (x, _, pixel) in image.enumerate_pixels() {
        projection[x as usize] += pixel[3] as u32;
    }
    let threshold = projection.iter().copied().max().unwrap_or(0) as f32 * 0.03;
    let columns = projection
        .iter()
        .enumerate()
        .filter_map(|(x, &value)| (value as f32 > threshold).then_some(x))
        .collect::<Vec<_>>();
    let gap = (height as f32 * 0.5).round_ties_even() as usize;
    let center = (width / 2) as usize;
    let group = columns
        .chunk_by(|a, b| b - a - 1 <= gap)
        .min_by_key(|group| {
            let distance = group[0]
                .saturating_sub(center)
                .max(center.saturating_sub(group[group.len() - 1]));
            (
                distance,
                Reverse(group.iter().map(|&x| projection[x]).sum::<u32>()),
            )
        });
    let Some(group) = group else {
        return RgbaImage::new(0, 0);
    };
    let padding = (height as f32 * 0.1).round_ties_even().max(2.0) as u32;
    let left = (group[0] as u32).saturating_sub(padding);
    let right = (group[group.len() - 1] as u32 + 1 + padding).min(width);
    image::imageops::crop_imm(&image, left, 0, right - left, height).to_image()
}

#[cfg(test)]
mod trim_tests {
    use super::trim_picture;
    use image::{Rgba, RgbaImage};

    #[test]
    fn plain_margins_are_trimmed() {
        // A grey tile with a bright item in the middle.
        let mut tile = RgbaImage::from_pixel(100, 60, Rgba([40, 40, 40, 255]));
        for x in 30..70 {
            for y in 20..40 {
                tile.put_pixel(x, y, Rgba([220, 200, 120, 255]));
            }
        }
        let trimmed = trim_picture(tile);
        assert!(trimmed.width() < 50 && trimmed.width() >= 40);
        assert!(trimmed.height() < 30 && trimmed.height() >= 20);
        // Transparent margins too.
        let mut clear = RgbaImage::new(80, 80);
        for x in 10..70 {
            for y in 30..50 {
                clear.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
        assert!(trim_picture(clear).height() < 30);
        // A plain picture stays as it is.
        let plain = RgbaImage::from_pixel(50, 50, Rgba([10, 10, 10, 255]));
        assert_eq!(trim_picture(plain).dimensions(), (50, 50));
    }
}
