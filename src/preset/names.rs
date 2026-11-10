//! HellLocker 1.9.6: equipment names as text, for the panel's gear cards.
//!
//! A saved item's name is a picture of the game's text. Windows' own text recognition
//! (Windows.Media.Ocr, part of Windows 10 and 11) reads it once; the result is kept in
//! `library/names.json`, keyed by the picture, so each name is read only the first
//! time it is shown. When it can't be read (no recognizer for the language, an odd
//! picture), the card shows the picture of the name as before.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use image::RgbaImage;
use serde::{Deserialize, Serialize};

const FILE: &str = "names.json";
/// Raise when the reading changes, so old results are read again.
const VERSION: u32 = 1;
/// Names longer than this are not a name.
const MAX_LEN: usize = 48;

#[derive(Default, Serialize, Deserialize)]
struct Stored {
    version: u32,
    /// Picture key -> the name read ("" = could not be read).
    names: BTreeMap<String, String>,
}

pub struct NameReader {
    path: PathBuf,
    stored: Stored,
    changed: bool,
    #[cfg(target_os = "windows")]
    engine: Option<Option<windows::Media::Ocr::OcrEngine>>,
}

impl NameReader {
    /// The reader for presets kept at `presets` (its results live beside the library).
    pub fn new(presets: &Path) -> Self {
        let path = presets
            .parent()
            .unwrap_or(Path::new("."))
            .join(super::library::LIBRARY_DIR)
            .join(FILE);
        let stored = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Stored>(&bytes).ok())
            .filter(|stored| stored.version == VERSION)
            .unwrap_or(Stored {
                version: VERSION,
                names: BTreeMap::new(),
            });
        Self {
            path,
            stored,
            changed: false,
            #[cfg(target_os = "windows")]
            engine: None,
        }
    }

    /// The name in this picture (white text on transparent), or None.
    pub fn read(&mut self, picture: &RgbaImage) -> Option<String> {
        let key = key(picture);
        if let Some(name) = self.stored.names.get(&key) {
            return (!name.is_empty()).then(|| name.clone());
        }
        let name = self.recognize(picture).unwrap_or_default();
        self.stored.names.insert(key, name.clone());
        self.changed = true;
        (!name.is_empty()).then_some(name)
    }

    /// Keep what was read (best effort).
    pub fn save(&mut self) {
        if !self.changed {
            return;
        }
        self.changed = false;
        let result = (|| -> anyhow::Result<()> {
            if let Some(parent) = self.path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&self.path, serde_json::to_vec_pretty(&self.stored)?)?;
            Ok(())
        })();
        if let Err(error) = result {
            tracing::debug!(error = %format!("{error:#}"), "could not keep equipment names");
        }
    }

    #[cfg(target_os = "windows")]
    fn recognize(&mut self, picture: &RgbaImage) -> Option<String> {
        let engine = self
            .engine
            .get_or_insert_with(|| match windows_ocr::engine() {
                Ok(engine) => Some(engine),
                Err(error) => {
                    tracing::info!(%error, "equipment names shown as pictures: no Windows text recognition");
                    None
                }
            })
            .as_ref()?;
        match windows_ocr::read(engine, &prepare(picture)) {
            Ok(text) => clean(&text),
            Err(error) => {
                tracing::debug!(%error, "could not read an equipment name");
                None
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn recognize(&mut self, _picture: &RgbaImage) -> Option<String> {
        None
    }
}

/// A stable key for a name picture (FNV-1a over its size and pixels).
fn key(picture: &RgbaImage) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |byte: u8| {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    };
    for byte in picture
        .width()
        .to_le_bytes()
        .into_iter()
        .chain(picture.height().to_le_bytes())
    {
        feed(byte);
    }
    for byte in picture.as_raw() {
        feed(*byte);
    }
    format!("{hash:016x}")
}

/// Dark text on white, enlarged so the letters are big enough to read well.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn prepare(picture: &RgbaImage) -> RgbaImage {
    let margin = 8;
    let scale = (48 / picture.height().max(1)).clamp(1, 4);
    let resized = image::imageops::resize(
        picture,
        picture.width() * scale,
        picture.height() * scale,
        image::imageops::FilterType::CatmullRom,
    );
    let mut out = RgbaImage::from_pixel(
        resized.width() + 2 * margin,
        resized.height() + 2 * margin,
        image::Rgba([255, 255, 255, 255]),
    );
    for (x, y, pixel) in resized.enumerate_pixels() {
        let ink = 255 - pixel[3];
        out.put_pixel(x + margin, y + margin, image::Rgba([ink, ink, ink, 255]));
    }
    out
}

/// One line, tidy spacing; None when it doesn't look like a name.
fn clean(text: &str) -> Option<String> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let letters = text.chars().filter(|c| c.is_alphanumeric()).count();
    (letters >= 2 && text.chars().count() <= MAX_LEN).then_some(text)
}

#[cfg(target_os = "windows")]
mod windows_ocr {
    use image::RgbaImage;
    use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Security::Cryptography::CryptographicBuffer;
    use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};

    pub fn engine() -> windows::core::Result<OcrEngine> {
        // Already set up on this thread is fine.
        let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
        OcrEngine::TryCreateFromUserProfileLanguages()
    }

    pub fn read(engine: &OcrEngine, picture: &RgbaImage) -> windows::core::Result<String> {
        let max = OcrEngine::MaxImageDimension()?;
        if picture.width() > max || picture.height() > max {
            return Ok(String::new());
        }
        // RGBA -> BGRA.
        let mut bytes = picture.as_raw().clone();
        for pixel in bytes.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        let buffer = CryptographicBuffer::CreateFromByteArray(&bytes)?;
        let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
            &buffer,
            BitmapPixelFormat::Bgra8,
            picture.width() as i32,
            picture.height() as i32,
        )?;
        let result = engine.RecognizeAsync(&bitmap)?.join()?;
        Ok(result.Text()?.to_string_lossy())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_tidied_and_rejected_when_unlikely() {
        assert_eq!(
            clean("  AR-23   Liberator \n"),
            Some("AR-23 Liberator".into())
        );
        assert_eq!(clean("—"), None);
        assert_eq!(clean(&"x".repeat(60)), None);
    }

    #[test]
    fn results_are_kept_by_picture() {
        let dir = std::env::temp_dir().join(format!("hl-names-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let presets = dir.join("presets.json");
        let a = RgbaImage::from_pixel(20, 10, image::Rgba([255, 255, 255, 200]));
        let b = RgbaImage::from_pixel(20, 10, image::Rgba([255, 255, 255, 100]));
        assert_ne!(key(&a), key(&b));
        let mut reader = NameReader::new(&presets);
        reader.stored.names.insert(key(&a), "P-2 Peacemaker".into());
        reader.changed = true;
        reader.save();
        let mut again = NameReader::new(&presets);
        assert_eq!(again.read(&a).as_deref(), Some("P-2 Peacemaker"));
        // Prepared for reading: dark ink on white, at least 48 pixels of text height.
        let prepared = prepare(&a);
        assert!(prepared.height() >= 40);
        assert_eq!(prepared.get_pixel(0, 0)[0], 255);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
