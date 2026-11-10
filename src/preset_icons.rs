//! HellLocker 1.9.5: the pictures a preset can show beside its name in the list.
//!
//! A preset has two icon slots. An empty first slot shows the preset's faction (or
//! sub-faction) emblem, as before; an empty second slot shows nothing. A chosen icon is
//! stored in config.toml by id: "<group>:<name>", for example "stratagems:railgun" or
//! "custom:my picture.png".
//!
//! Built-in icons come from assets/ (see build.rs). The player's own pictures are PNG
//! files in the data folder's `icons` folder.
use std::path::{Path, PathBuf};

use image::RgbaImage;

mod bundled {
    include!(concat!(env!("OUT_DIR"), "/bundled_icons.rs"));
}

/// How many icons a preset can have.
pub const SLOTS: usize = 2;
/// The folder in the data folder for the player's own icons.
pub const CUSTOM_DIR: &str = "icons";
const CUSTOM_PREFIX: &str = "custom";
/// Pictures are shrunk to this many pixels on their longest side.
const SIZE: u32 = 96;
/// The most of the player's own pictures that are listed, and the largest file read.
const CUSTOM_MAX: usize = 500;
const CUSTOM_MAX_BYTES: u64 = 8 * 1024 * 1024;
/// The longest icon id kept in config.toml.
const ID_MAX: usize = 200;

/// The picker's groups, in the order its tabs show them. The numbers are what the
/// panel uses (0 is "All").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Factions = 1,
    Subfactions = 2,
    Missions = 3,
    Stratagems = 4,
    Boosters = 5,
    Conditions = 6,
    Other = 7,
    Custom = 8,
}

impl Group {
    pub const COUNT: usize = 9;

    fn from_folder(folder: &str) -> Self {
        match folder {
            "factions" => Self::Factions,
            "subfactions" => Self::Subfactions,
            "missions" => Self::Missions,
            "stratagems" => Self::Stratagems,
            "boosters" => Self::Boosters,
            "conditions" => Self::Conditions,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone)]
enum Source {
    Bundled(&'static [u8]),
    File(PathBuf),
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub group: Group,
    /// For factions, sub-factions and missions: the tag's index, so the panel can show
    /// its translated name. 0 otherwise.
    pub tag: i32,
    source: Source,
}

pub struct Catalog {
    entries: Vec<Entry>,
    dir: PathBuf,
}

impl Catalog {
    /// The built-in icons plus the player's own from `<data>/icons`.
    pub fn new(data: &Path) -> Self {
        let mut catalog = Self {
            entries: builtin(),
            dir: data.join(CUSTOM_DIR),
        };
        catalog.rescan();
        catalog
    }

    /// The folder for the player's own icons.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Look at the player's icon folder again (pictures may have been added).
    pub fn rescan(&mut self) {
        self.entries.retain(|entry| entry.group != Group::Custom);
        self.entries.extend(custom(&self.dir));
    }

    /// Icons in `group` (0 = all) whose name contains every word of `search`.
    pub fn find(&self, search: &str, group: i32) -> Vec<&Entry> {
        let words: Vec<String> = search.split_whitespace().map(str::to_lowercase).collect();
        self.entries
            .iter()
            .filter(|entry| group == 0 || entry.group as i32 == group)
            .filter(|entry| {
                let name = entry.name.to_lowercase();
                words.iter().all(|word| name.contains(word.as_str()))
            })
            .collect()
    }

    /// Which groups have any icons, indexed like `Group` (0 = All, always true).
    pub fn groups_present(&self) -> [bool; Group::COUNT] {
        let mut present = [false; Group::COUNT];
        present[0] = true;
        for entry in &self.entries {
            present[entry.group as usize] = true;
        }
        present
    }

    /// The picture for `id`, shrunk to the icon size. None for an unknown id or a
    /// picture that can't be read (a custom file that was removed, say).
    pub fn picture(&self, id: &str) -> Option<RgbaImage> {
        let entry = self.entries.iter().find(|entry| entry.id == id);
        let bytes = match entry.map(|entry| &entry.source) {
            Some(Source::Bundled(bytes)) => bytes.to_vec(),
            Some(Source::File(path)) => read_custom(path)?,
            // A custom picture added since the last look at the folder.
            None => read_custom(&self.dir.join(custom_file(id)?))?,
        };
        let picture = match image::load_from_memory(&bytes) {
            Ok(picture) => picture.into_rgba8(),
            Err(error) => {
                tracing::debug!(%id, %error, "could not read an icon");
                return None;
            }
        };
        Some(shrink(picture))
    }
}

/// An id as stored: trimmed, and empty unless it looks like "<group>:<name>".
pub fn clean_id(id: &str) -> String {
    let id = id.trim();
    let valid = id.len() <= ID_MAX
        && id
            .split_once(':')
            .is_some_and(|(group, name)| !group.is_empty() && !name.trim().is_empty())
        && !id.contains(['/', '\\'])
        && !id.chars().any(char::is_control);
    if valid { id.to_owned() } else { String::new() }
}

/// A preset's stored icons, always `SLOTS` long.
pub fn slots(stored: &[String]) -> [String; SLOTS] {
    let mut slots: [String; SLOTS] = Default::default();
    for (slot, id) in slots.iter_mut().zip(stored) {
        *slot = clean_id(id);
    }
    slots
}

fn custom_file(id: &str) -> Option<&str> {
    let name = id.strip_prefix(CUSTOM_PREFIX)?.strip_prefix(':')?;
    let plain = !name.is_empty() && !name.contains(['/', '\\', ':']) && name != "." && name != "..";
    plain.then_some(name)
}

fn read_custom(path: &Path) -> Option<Vec<u8>> {
    let size = std::fs::metadata(path).ok()?.len();
    if size > CUSTOM_MAX_BYTES {
        tracing::debug!(path = %path.display(), size, "icon picture too large");
        return None;
    }
    std::fs::read(path).ok()
}

fn shrink(picture: RgbaImage) -> RgbaImage {
    if picture.width() <= SIZE && picture.height() <= SIZE {
        return picture;
    }
    let scale = SIZE as f32 / picture.width().max(picture.height()) as f32;
    let width = ((picture.width() as f32 * scale).round() as u32).max(1);
    let height = ((picture.height() as f32 * scale).round() as u32).max(1);
    image::imageops::resize(
        &picture,
        width,
        height,
        image::imageops::FilterType::Triangle,
    )
}

fn builtin() -> Vec<Entry> {
    use crate::context::{Faction, Mission, Subfaction};
    let mut entries = Vec::new();
    let factions: [(Faction, &'static [u8]); 3] = [
        (
            Faction::Terminids,
            include_bytes!("../assets/faction-terminids.png"),
        ),
        (
            Faction::Automatons,
            include_bytes!("../assets/faction-automatons.png"),
        ),
        (
            Faction::Illuminate,
            include_bytes!("../assets/faction-illuminate.png"),
        ),
    ];
    for (faction, bytes) in factions {
        entries.push(Entry {
            id: format!("factions:{}", faction.key()),
            name: title(faction.key()),
            group: Group::Factions,
            tag: faction.index(),
            source: Source::Bundled(bytes),
        });
    }
    let subfactions: [(Subfaction, &'static [u8]); 9] = [
        (
            Subfaction::PredatorStrain,
            include_bytes!("../assets/subfaction-predator_strain.png"),
        ),
        (
            Subfaction::SporeBurstStrain,
            include_bytes!("../assets/subfaction-spore_burst_strain.png"),
        ),
        (
            Subfaction::RuptureStrain,
            include_bytes!("../assets/subfaction-rupture_strain.png"),
        ),
        (
            Subfaction::JetBrigade,
            include_bytes!("../assets/subfaction-jet_brigade.png"),
        ),
        (
            Subfaction::IncinerationCorps,
            include_bytes!("../assets/subfaction-incineration_corps.png"),
        ),
        (
            Subfaction::CyborgLegion,
            include_bytes!("../assets/subfaction-cyborg_legion.png"),
        ),
        (
            Subfaction::MindlessMasses,
            include_bytes!("../assets/subfaction-mindless_masses.png"),
        ),
        (
            Subfaction::Appropriators,
            include_bytes!("../assets/subfaction-appropriators.png"),
        ),
        (
            Subfaction::VoteSnatchers,
            include_bytes!("../assets/subfaction-vote_snatchers.png"),
        ),
    ];
    for (subfaction, bytes) in subfactions {
        entries.push(Entry {
            id: format!("subfactions:{}", subfaction.key()),
            name: title(subfaction.key()),
            group: Group::Subfactions,
            tag: subfaction.index(),
            source: Source::Bundled(bytes),
        });
    }
    for &(folder, stem, name, bytes) in bundled::BUNDLED {
        let group = Group::from_folder(folder);
        let tag = match group {
            Group::Missions => Mission::ALL
                .iter()
                .find(|mission| mission.key() == stem)
                .map_or(0, |mission| mission.index()),
            _ => 0,
        };
        entries.push(Entry {
            id: format!("{folder}:{stem}"),
            name: name.to_owned(),
            group,
            tag,
            source: Source::Bundled(bytes),
        });
    }
    entries.sort_by_key(|entry| entry.group);
    entries
}

fn custom(dir: &Path) -> Vec<Entry> {
    let Ok(listing) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<(String, PathBuf)> = listing
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            let path = entry.path();
            let png = path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("png"));
            let name = entry.file_name().into_string().ok()?;
            (png && custom_file(&format!("{CUSTOM_PREFIX}:{name}")).is_some())
                .then_some((name, path))
        })
        .collect();
    files.sort_by_key(|(name, _)| name.to_lowercase());
    files.truncate(CUSTOM_MAX);
    files
        .into_iter()
        .map(|(file, path)| Entry {
            id: format!("{CUSTOM_PREFIX}:{file}"),
            name: Path::new(&file)
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or(file.clone()),
            group: Group::Custom,
            tag: 0,
            source: Source::File(path),
        })
        .collect()
}

/// "spore_burst_strain" -> "Spore Burst Strain".
fn title(key: &str) -> String {
    key.split('_')
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hl-icons-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn builtin_icons_load_and_search() {
        let catalog = Catalog::new(&temp("builtin"));
        let present = catalog.groups_present();
        for group in [
            Group::Factions,
            Group::Subfactions,
            Group::Missions,
            Group::Stratagems,
            Group::Boosters,
            Group::Conditions,
        ] {
            assert!(present[group as usize], "{group:?} has icons");
        }
        assert!(catalog.find("", 0).len() > 100);
        let found = catalog.find("eagle 500", 0);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].group, Group::Stratagems);
        let picture = catalog.picture(&found[0].id).expect("bundled icon decodes");
        assert!(picture.width() <= SIZE && picture.height() <= SIZE);
        assert!(catalog.picture("factions:terminids").is_some());
        assert!(catalog.picture("stratagems:no-such-thing").is_none());
        // The four mission types come first, with their tags for translated names.
        let missions = catalog.find("", Group::Missions as i32);
        assert!(missions.len() > 40);
        assert_eq!(
            missions[..4]
                .iter()
                .map(|entry| entry.tag)
                .collect::<Vec<_>>(),
            [1, 2, 3, 4]
        );
        assert!(missions[4..].iter().all(|entry| entry.tag == 0));
        assert_eq!(catalog.find("dead sprint", Group::Boosters as i32).len(), 1);
        assert_eq!(catalog.find("ion storms", 0).len(), 1);
    }

    #[test]
    fn own_pictures_are_found_and_shrunk() {
        let data = temp("custom");
        let dir = data.join(CUSTOM_DIR);
        std::fs::create_dir_all(&dir).unwrap();
        RgbaImage::from_pixel(300, 150, image::Rgba([255, 0, 0, 255]))
            .save(dir.join("Big Red.png"))
            .unwrap();
        std::fs::write(dir.join("notes.txt"), "not a picture").unwrap();
        std::fs::write(dir.join("broken.png"), "not a picture").unwrap();
        let mut catalog = Catalog::new(&data);
        let own = catalog.find("", Group::Custom as i32);
        assert_eq!(own.len(), 2);
        let picture = catalog.picture("custom:Big Red.png").unwrap();
        assert_eq!((picture.width(), picture.height()), (SIZE, SIZE / 2));
        assert!(catalog.picture("custom:broken.png").is_none());
        assert!(catalog.picture("custom:../config.toml").is_none());
        std::fs::remove_file(dir.join("broken.png")).unwrap();
        catalog.rescan();
        assert_eq!(catalog.find("", Group::Custom as i32).len(), 1);
    }

    #[test]
    fn stored_ids_are_cleaned() {
        assert_eq!(clean_id(" stratagems:railgun "), "stratagems:railgun");
        assert_eq!(clean_id("railgun"), "");
        assert_eq!(clean_id("custom:../x.png"), "");
        assert_eq!(
            slots(&["a:b".into(), "c:d".into(), "e:f".into()]),
            ["a:b".to_string(), "c:d".to_string()]
        );
        assert_eq!(slots(&[]), [String::new(), String::new()]);
    }
}
