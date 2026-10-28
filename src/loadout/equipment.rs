//! Equipment capture/application, independent of the UI and preset persistence.
mod cache;
mod evidence;
mod navigation;
mod search;
mod survey;

pub(crate) use crate::preset::equipment::{EquipmentItem, EquipmentSet};
pub(crate) use cache::EquipmentCache;
use evidence::EquipmentEvidence;

use crate::{
    automation::AutomationSession,
    game_window,
    input::Key,
    item::EquipmentKind,
    vision::equipment::{EquipmentObservation, EquipmentObserver},
    vision::text::TextSample,
};
use anyhow::{Context, Result, bail, ensure};
use search::{Next, Search};
use std::time::{Duration, Instant};

// Candidate/continuity tolerance is not enough to merge identities or skip a search.
const NAME_MATCH_THRESHOLD: f32 = 0.80;
// Cross-resolution rasterization can lower an identical name to about 0.93.
// Cache identity also requires separation from the runner-up.
const NAME_IDENTITY_THRESHOLD: f32 = 0.90;

#[derive(Clone, Copy, Debug)]
pub(crate) enum InputAction {
    Key(Key),
    ClickColumn(usize),
}

#[derive(Clone, Copy)]
pub(crate) struct Navigation {
    action: InputAction,
    retried: bool,
    /// Large opposite-direction focus jump after a settled vertical input.
    wrapped: bool,
}

impl InputAction {
    pub fn name(self) -> &'static str {
        match self {
            Self::Key(key) => key.name(),
            Self::ClickColumn(_) => "Click",
        }
    }

    pub fn column(self) -> Option<usize> {
        match self {
            Self::ClickColumn(col) => Some(col),
            Self::Key(_) => None,
        }
    }
}

// HellLocker: how long to wait for the game before giving up (nothing is clicked while
// waiting, and each wait ends as soon as the screen is confirmed). 1.5 s was too short on
// slower PCs, where opening an equipment list can take longer the first time.
const NAVIGATION_TIMEOUT: Duration = Duration::from_millis(4000);
const NAVIGATION_RETRY_AFTER: Duration = Duration::from_millis(300);
const EQUIP_TIMEOUT: Duration = Duration::from_millis(4000);
/// HellLocker: the longest any single wait may stretch to on a slow PC.
const MAX_WAIT: Duration = Duration::from_secs(30);
/// HellLocker: a screen check slower than this is logged once as a hint.
const SLOW_CHECK: Duration = Duration::from_millis(500);

/// HellLocker: `base`, plus room for three screen checks at the current speed.
fn stretched_wait(base: Duration, check_time: Duration) -> Duration {
    (base + check_time * 3).min(MAX_WAIT)
}
const INPUT_HOLD_MS: u64 = 45;
const INPUT_RELEASE_INTERVAL: Duration = Duration::from_millis(20);
const PAGE_STABLE_INTERVAL: Duration = Duration::from_millis(40);

pub(crate) struct Session<'a> {
    automation: AutomationSession<'a>,
    observer: EquipmentObserver,
    frame: crate::capture::CapturedFrame,
    page: EquipmentObservation,
    evidence: EquipmentEvidence<'a>,
    client_size: (u32, u32),
    last_release: Option<Instant>,
    /// HellLocker: how long one screen check (capture + read) takes on this PC, as the
    /// slowest recent one, easing back down as checks get faster.
    check_time: Duration,
}

pub(crate) enum ApplyProgress<'a> {
    Selecting(EquipmentKind),
    Caching {
        category: &'a TextSample,
        repairing: bool,
    },
    Selected {
        kind: EquipmentKind,
        corrected_category: Option<TextSample>,
    },
}

impl<'a> Session<'a> {
    pub fn new(
        mut automation: AutomationSession<'a>,
        observer: EquipmentObserver,
        client_size: (u32, u32),
    ) -> Result<Self> {
        let started = Instant::now();
        let frame = automation.capture()?;
        let page = observer.observe(&frame);
        let check_time = started.elapsed();
        let evidence = EquipmentEvidence::new(automation.evidence());

        Ok(Self {
            automation,
            observer,
            frame,
            page,
            evidence,
            client_size,
            last_release: None,
            check_time,
        })
    }

    /// HellLocker: a wait limit stretched by how slow screen checks are on this PC. On
    /// most PCs a check takes a few milliseconds and this is just `base`; where copying
    /// the game's picture takes seconds (a busy or switchable GPU), the waits grow with
    /// it instead of running out after one or two checks. Waiting never sends input.
    fn wait_limit(&self, base: Duration) -> Duration {
        stretched_wait(crate::timing::wait(base), self.check_time)
    }

    /// HellLocker: when an unanswered input may be repeated. A repeat is only right once
    /// the screen has been checked again after the input, so slow checks push it back.
    fn retry_after(&self) -> Duration {
        crate::timing::wait(NAVIGATION_RETRY_AFTER) + self.check_time * 2
    }

    fn note_check_time(&mut self, latest: Duration) {
        if latest >= SLOW_CHECK && self.check_time < SLOW_CHECK {
            tracing::warn!(
                check_s = latest.as_secs_f64(),
                "screen checks are slow on this PC; waits are stretched to match (capping the game's frame rate usually helps)"
            );
        }
        self.check_time = self.check_time.mul_f32(0.75).max(latest);
    }

    fn check_active(&self) -> Result<()> {
        ensure!(
            game_window::is_game_foreground(),
            "game lost foreground; equipment operation stopped"
        );
        Ok(())
    }

    fn refresh(&mut self) -> Result<()> {
        self.check_active()?;
        let check_started = Instant::now();
        let started = Instant::now();
        self.frame = self.automation.capture()?;
        let capture_ms = started.elapsed().as_secs_f64() * 1000.0;
        let started = Instant::now();
        self.page = self.observer.observe(&self.frame);
        let observe_ms = started.elapsed().as_secs_f64() * 1000.0;
        self.note_check_time(check_started.elapsed());
        self.evidence
            .observe(&self.frame, &self.page, capture_ms, observe_ms);
        Ok(())
    }

    pub fn at_entry(&self) -> bool {
        self.observer.entry(&self.frame).confirmed()
    }

    fn sample_current(
        &mut self,
        library: Option<(&std::path::Path, &crate::preset::library::Library)>,
    ) -> Result<EquipmentItem> {
        if !self.page.ready() {
            self.evidence
                .checkpoint(&self.frame, &self.page, "not-ready");
            bail!("equipment focus or name is not readable; operation stopped");
        }
        self.sync_category()?;
        self.evidence
            .checkpoint(&self.frame, &self.page, "remembered");
        // HellLocker: a scanned picture needs no waiting for the tile to load.
        let kind = self.observer.kind();
        let scanned =
            library.and_then(|(presets, library)| library.picture(presets, kind, &self.page.name));
        if scanned.is_some() {
            tracing::debug!(?kind, "equipment picture taken from the scan library");
        }
        Ok(EquipmentItem {
            name: self.page.name.clone(),
            category: Some(self.page.category.clone()),
            client_size: self.client_size,
            picture: scanned.or_else(|| self.settled_picture()),
        })
    }

    /// HellLocker: Scan equipment for one slot. Opens its list, walks every item, and
    /// keeps each item's name and loaded picture. Returns to the six-card page.
    pub fn scan_kind(
        &mut self,
        kind: EquipmentKind,
        cache: &mut EquipmentCache,
        mut found: impl FnMut(usize),
    ) -> Result<crate::preset::library::ScanBatch> {
        let span = tracing::info_span!("equipment_scan", ?kind);
        let _guard = span.enter();
        self.check_active()?;
        self.open(kind)
            .with_context(|| format!("failed to open {kind:?} for scanning"))?;
        let mut batch = crate::preset::library::ScanBatch::new(kind, self.client_size);
        let started = Instant::now();
        scan_walk(self, kind, cache, |session| {
            if session.page.ready() && batch.is_new(&session.page.name) {
                let name = session.page.name.clone();
                if let Some(picture) = session.settled_picture() {
                    batch.push(name, picture);
                    found(batch.len());
                }
            }
        })?;
        tracing::info!(
            items = batch.len(),
            elapsed_s = started.elapsed().as_secs_f64(),
            "equipment slot scanned"
        );
        self.return_to_entry()?;
        Ok(batch)
    }

    /// HellLocker: the tile once the game has finished loading its picture. While it
    /// loads, the tile shows a spinning ring, so the picture counts as loaded once three
    /// captures in a row (about a third of a second) are unchanged. Gives up after 3 s
    /// with the latest capture; the name and category were already read.
    fn settled_picture(&mut self) -> Option<image::RgbaImage> {
        const SETTLE: Duration = Duration::from_millis(110);
        const LIMIT: Duration = Duration::from_secs(3);
        let started = Instant::now();
        let mut previous = self.equipped_picture();
        let mut unchanged = 0;
        while started.elapsed() < LIMIT {
            std::thread::sleep(SETTLE);
            if self.refresh().is_err() {
                break;
            }
            let current = self.equipped_picture();
            match (&previous, &current) {
                (Some(a), Some(b)) if same_picture(a, b) => unchanged += 1,
                _ => unchanged = 0,
            }
            previous = current;
            if unchanged >= 2 {
                tracing::debug!(
                    wait_ms = started.elapsed().as_millis() as u64,
                    "equipment picture settled"
                );
                return previous;
            }
        }
        tracing::debug!("equipment picture did not settle; keeping the latest capture");
        previous
    }

    /// HellLocker: the focused item's tile (or the equipped one), cropped inside its
    /// frame. Only used for previews, so a miss just leaves the picture out.
    fn equipped_picture(&self) -> Option<image::RgbaImage> {
        let slots = &self.page.slots;
        // The saved name is the focused item's, so its tile comes first.
        let slot = self
            .page
            .focus
            .and_then(|index| slots.get(index))
            .or_else(|| slots.iter().find(|slot| slot.equipped))?;
        let [left, top, right, bottom] = slot.rect;
        let inset = (6.0 * self.page.scale).max(2.0);
        let (x0, y0) = ((left + inset).ceil(), (top + inset).ceil());
        let (x1, y1) = ((right - inset).floor(), (bottom - inset).floor());
        let (width, height) = self.frame.dimensions();
        if x0 < 0.0
            || y0 < 0.0
            || x1 > f64::from(width)
            || y1 > f64::from(height)
            || x1 - x0 < 8.0
            || y1 - y0 < 8.0
        {
            return None;
        }
        Some(
            image::imageops::crop_imm(
                &*self.frame,
                x0 as u32,
                y0 as u32,
                (x1 - x0) as u32,
                (y1 - y0) as u32,
            )
            .to_image(),
        )
    }

    /// The caller replaces its old set only after all six samples and returns succeed.
    /// HellLocker: samples only `kinds`; the others stay None (kept from the last save).
    pub fn sample_all(
        &mut self,
        kinds: &[EquipmentKind],
        library: Option<(&std::path::Path, &crate::preset::library::Library)>,
    ) -> Result<EquipmentSet> {
        let mut items = std::array::from_fn(|_| None);

        for &kind in kinds {
            let span = tracing::info_span!("equipment_item", ?kind);
            let _guard = span.enter();
            let item = (|| -> Result<EquipmentItem> {
                self.check_active()?;

                self.open(kind)?;
                let item = self.sample_current(library)?;

                self.return_to_entry()?;
                self.check_active()?;
                Ok(item)
            })()
            .with_context(|| {
                format!("failed to remember {kind:?}; previous equipment records retained")
            })?;
            items[kind.index()] = Some(item);
            tracing::debug!(completed = kind.index() + 1, "equipped item sampled");
        }
        self.check_active()?;

        Ok(items)
    }

    /// Check the complete set before opening or changing any equipment.
    /// HellLocker: applies only `kinds`, leaving the other slots as they are.
    pub fn apply_all(
        &mut self,
        items: &EquipmentSet,
        kinds: &[EquipmentKind],
        cache: &mut EquipmentCache,
        mut progress: impl FnMut(ApplyProgress<'_>),
    ) -> Result<()> {
        if let Some(kind) = kinds.iter().find(|&&kind| items[kind.index()].is_none()) {
            bail!("no saved item for {kind:?}; remember it before applying");
        }
        for &kind in kinds {
            let span = tracing::info_span!("equipment_item", ?kind);
            let _guard = span.enter();
            progress(ApplyProgress::Selecting(kind));

            self.open(kind)
                .with_context(|| format!("failed to open {kind:?} for application"))?;
            let corrected_category = self
                .apply_item(
                    items[kind.index()].as_ref().unwrap(),
                    cache.search(kind, self.page.scale),
                    &mut progress,
                )
                .with_context(|| format!("failed to apply {kind:?}"))?;
            progress(ApplyProgress::Selected {
                kind,
                corrected_category,
            });

            tracing::info!("equipment item applied");
        }

        tracing::debug!("equipment application completed");
        Ok(())
    }

    fn apply_item(
        &mut self,
        item: &EquipmentItem,
        cache: cache::SearchCache<'_>,
        progress: &mut impl FnMut(ApplyProgress<'_>),
    ) -> Result<Option<TextSample>> {
        ensure!(
            self.find_item(item, cache, progress)?,
            "saved equipment was not found in the list"
        );
        self.equip_and_return(item)
    }

    fn find_item(
        &mut self,
        target: &EquipmentItem,
        cache: cache::SearchCache<'_>,
        progress: &mut impl FnMut(ApplyProgress<'_>),
    ) -> Result<bool> {
        let kind = cache.kind;
        let mut search = Search::new(target, cache);
        self.evidence.start_search(target);
        let started = Instant::now();
        let mut navigation = None;
        let mut caching = None;
        let result = (|| loop {
            self.sync_category()?;
            let evaluate_started = Instant::now();
            let (visit, next) = match search.advance(&self.page, navigation.take()) {
                Ok(step) => step,
                Err(error) => {
                    self.evidence
                        .checkpoint(&self.frame, &self.page, "search-error");
                    return Err(error);
                }
            };
            self.evidence.search_visit(
                &self.frame,
                &visit,
                &self.page,
                evaluate_started.elapsed().as_secs_f64() * 1000.0,
            );
            // The survey may briefly cross into another category to find its boundary.
            // Keep its original title until cache work actually switches categories.
            let active = search.caching();
            if active != caching {
                progress(match active {
                    Some((_, repairing)) => ApplyProgress::Caching {
                        category: &self.page.category,
                        repairing,
                    },
                    None => ApplyProgress::Selecting(kind),
                });
                caching = active;
            }
            match next {
                Next::Input(action) => navigation = Some(self.navigate(action)?),
                Next::Found | Next::NotFound => {
                    let found = matches!(next, Next::Found);
                    self.evidence.checkpoint(
                        &self.frame,
                        &self.page,
                        if found { "found" } else { "not-found" },
                    );
                    return Ok(found);
                }
            }
        })();
        self.evidence
            .finish_search(&result, started.elapsed().as_secs_f64() * 1000.0);
        result
    }
}

/// HellLocker: what the scan walk needs from a list page: the current observation and
/// a way to press a key or click a column. The game session and the tests implement it.
trait ScanList {
    fn page(&self) -> &EquipmentObservation;
    fn press(&mut self, action: InputAction) -> Result<Navigation>;
    /// Before each observation (the session aligns hover with focus here).
    fn settle(&mut self) -> Result<()> {
        Ok(())
    }
}

impl ScanList for Session<'_> {
    fn page(&self) -> &EquipmentObservation {
        &self.page
    }

    fn press(&mut self, action: InputAction) -> Result<Navigation> {
        self.navigate(action)
    }

    fn settle(&mut self) -> Result<()> {
        self.sync_category()
    }
}

/// HellLocker: visits every item of one list, calling `visit` at each. A search for a
/// blank name (similar to nothing) takes the cold-scan route through all categories and
/// ends with NotFound. Because the search treats a category that wraps onto itself as
/// the whole list, the walk then presses C and starts over until a category heading
/// comes round again.
fn scan_walk<L: ScanList>(
    list: &mut L,
    kind: EquipmentKind,
    cache: &mut EquipmentCache,
    mut visit: impl FnMut(&mut L),
) -> Result<()> {
    const SAME_HEADING: f32 = 0.97;
    let blank = EquipmentItem {
        name: blank_name(&list.page().name),
        category: None,
        client_size: (0, 0),
        picture: None,
    };
    let mut headings: Vec<TextSample> = Vec::new();
    loop {
        let scale = list.page().scale;
        let mut search = Search::new(&blank, cache.search(kind, scale));
        let mut navigation = None;
        loop {
            list.settle()?;
            let heading = &list.page().category;
            if !headings
                .iter()
                .any(|seen| seen.cosine(heading) >= SAME_HEADING)
            {
                headings.push(heading.clone());
            }
            let (_, next) = search.advance(list.page(), navigation.take())?;
            visit(list);
            match next {
                Next::Input(action) => navigation = Some(list.press(action)?),
                Next::Found | Next::NotFound => break,
            }
        }
        drop(search);
        if !kind.has_categories() {
            return Ok(());
        }
        list.press(InputAction::Key(Key::C))?;
        list.settle()?;
        let heading = &list.page().category;
        if headings
            .iter()
            .any(|seen| seen.cosine(heading) >= SAME_HEADING)
        {
            return Ok(());
        }
        tracing::info!(
            ?kind,
            "scan continuing into a category the search did not cross into"
        );
    }
}

/// HellLocker: a name with no text, the size of a real one. It matches no item
/// (its similarity to every name is zero), so a search for it visits them all.
fn blank_name(like: &TextSample) -> TextSample {
    let (width, height) = like.image.dimensions();
    TextSample::extract(
        image::RgbaImage::from_pixel(width, height, image::Rgba([0, 0, 0, 255])),
        like.geometry,
    )
}

/// HellLocker: two captures of a tile are the same picture when almost no pixel moved.
/// A loading ring is small, so this counts changed pixels instead of averaging.
fn same_picture(a: &image::RgbaImage, b: &image::RgbaImage) -> bool {
    if a.dimensions() != b.dimensions() {
        return false;
    }
    let changed = a
        .pixels()
        .zip(b.pixels())
        .filter(|(p, q)| {
            p.0.iter()
                .zip(q.0.iter())
                .take(3)
                .any(|(x, y)| x.abs_diff(*y) > 40)
        })
        .count();
    // Fewer than 0.2 % of the pixels; allows for capture noise.
    changed * 500 < (a.width() * a.height()) as usize
}

#[cfg(test)]
mod hell_locker_tests {
    use super::same_picture;
    use image::{Rgba, RgbaImage};

    #[test]
    fn a_moving_ring_is_a_change_and_noise_is_not() {
        let base = RgbaImage::from_pixel(120, 120, Rgba([50, 60, 70, 255]));
        let mut noisy = base.clone();
        noisy.put_pixel(3, 3, Rgba([200, 60, 70, 255]));
        assert!(same_picture(&base, &noisy));
        let mut ring = base.clone();
        for x in 50..70 {
            for y in 50..54 {
                ring.put_pixel(x, y, Rgba([240, 220, 60, 255]));
            }
        }
        assert!(!same_picture(&base, &ring));
        assert!(!same_picture(&base, &RgbaImage::new(60, 60)));
    }
}

/// HellLocker: the scan's walk against a simulated equipment list. A search for a blank
/// name must visit every item of every category, then end with NotFound.
#[cfg(test)]
mod scan_walk_tests {
    use super::*;
    use crate::vision::equipment::{EquipmentSlot, EquipmentText};
    use image::{Rgba, RgbaImage};

    const SIZE: (u32, u32) = (1920, 1080);

    /// A text-like sample: a row of blocky white "glyphs" chosen by `seed`.
    fn text(field: EquipmentText, seed: u64) -> TextSample {
        let (observer, _) =
            EquipmentObserver::resolve(SIZE.0, SIZE.1, EquipmentKind::Primary).unwrap();
        let (rect, geometry) = observer.text_region(field);
        let mut state = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let mut next = || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 33) as u32
        };
        let glyph = (rect.h * 2 / 3).max(4);
        let mut image = RgbaImage::from_pixel(rect.w, rect.h, Rgba([0, 0, 0, 255]));
        let mut x = 2;
        while x + glyph < rect.w / 2 {
            let (w, h) = (2 + next() % (glyph - 2), 3 + next() % (glyph - 3));
            let top = (rect.h - h) / 2;
            for dx in 0..w {
                for dy in 0..h {
                    if next() % 5 != 0 {
                        image.put_pixel(x + dx, top + dy, Rgba([255, 255, 255, 255]));
                    }
                }
            }
            x += w + 2 + next() % 3;
        }
        TextSample::extract(image, geometry)
    }

    struct Game {
        names: Vec<Vec<TextSample>>,
        headings: Vec<TextSample>,
        columns: usize,
        circular: bool,
        category: usize,
        position: usize,
        current: EquipmentObservation,
        visits: usize,
    }

    impl Game {
        fn new(sizes: &[usize], columns: usize, start: (usize, usize)) -> Self {
            let mut seed = 1000;
            let names = sizes
                .iter()
                .map(|&n| {
                    (0..n)
                        .map(|_| {
                            seed += 1;
                            text(EquipmentText::Name, seed)
                        })
                        .collect()
                })
                .collect();
            let headings = (0..sizes.len())
                .map(|i| text(EquipmentText::Category, 50 + i as u64))
                .collect();
            let mut game = Self {
                names,
                headings,
                columns,
                circular: sizes.len() == 1,
                category: start.0,
                position: start.1,
                current: EquipmentObservation {
                    slots: Vec::new(),
                    focus: None,
                    hover: None,
                    name: text(EquipmentText::Name, 1),
                    category: text(EquipmentText::Category, 1),
                    scale: 1.0,
                },
                visits: 0,
            };
            game.current = game.observe();
            game
        }

        fn len(&self) -> usize {
            self.names[self.category].len()
        }

        fn observe(&self) -> EquipmentObservation {
            let row = self.position / self.columns;
            let width = (self.len() - row * self.columns).min(self.columns);
            let slots = (0..width)
                .map(|col| EquipmentSlot {
                    row: 0,
                    col,
                    rect: [0.0, 0.0, 10.0, 10.0],
                    frame_score: 1.0,
                    focus_response: if col == self.position % self.columns {
                        1.0
                    } else {
                        0.0
                    },
                    hover_response: 0.0,
                    equipped: false,
                })
                .collect();
            EquipmentObservation {
                slots,
                focus: Some(self.position % self.columns),
                hover: Some(self.position % self.columns),
                name: self.names[self.category][self.position].clone(),
                category: self.headings[self.category].clone(),
                scale: 1.0,
            }
        }

        fn last_row_start(&self) -> usize {
            (self.len() - 1) / self.columns * self.columns
        }

        /// What the game does with one input; true when focus wrapped within a list.
        fn input(&mut self, action: InputAction) -> bool {
            let col = self.position % self.columns;
            let categories = self.names.len();
            match action {
                InputAction::ClickColumn(c) => {
                    let start = self.position / self.columns * self.columns;
                    assert!(start + c < self.len(), "clicked an empty column");
                    self.position = start + c;
                }
                InputAction::Key(Key::C) => {
                    self.category = (self.category + 1) % categories;
                    self.position = 0;
                }
                InputAction::Key(Key::Z) => {
                    self.category = (self.category + categories - 1) % categories;
                    self.position = 0;
                }
                InputAction::Key(Key::Down) => {
                    if self.position / self.columns < (self.len() - 1) / self.columns {
                        self.position = (self.position + self.columns).min(self.len() - 1);
                    } else if self.circular {
                        self.position = col.min(self.len() - 1);
                        return true;
                    } else {
                        self.category = (self.category + 1) % categories;
                        self.position = col.min(self.len() - 1);
                    }
                }
                InputAction::Key(Key::Up) => {
                    if self.position >= self.columns {
                        self.position -= self.columns;
                    } else if self.circular {
                        self.position = (self.last_row_start() + col).min(self.len() - 1);
                        return true;
                    } else {
                        self.category = (self.category + categories - 1) % categories;
                        self.position = (self.last_row_start() + col).min(self.len() - 1);
                    }
                }
                InputAction::Key(other) => panic!("unexpected key {other:?}"),
            }
            false
        }
    }

    fn walk(kind: EquipmentKind, sizes: &[usize], start: (usize, usize)) -> (usize, usize) {
        walk_with(kind, sizes, start, sizes.len() == 1)
    }

    fn walk_with(
        kind: EquipmentKind,
        sizes: &[usize],
        start: (usize, usize),
        circular: bool,
    ) -> (usize, usize) {
        let mut game = Game::new(sizes, kind.list_columns(), start);
        game.circular = circular;
        let mut cache = EquipmentCache::default();
        let mut seen = std::collections::BTreeSet::new();
        scan_walk(&mut game, kind, &mut cache, |game| {
            seen.insert((game.category, game.position));
            game.visits += 1;
            assert!(game.visits < 2000, "the walk did not finish");
        })
        .unwrap();
        (seen.len(), sizes.iter().sum())
    }

    impl ScanList for Game {
        fn page(&self) -> &EquipmentObservation {
            &self.current
        }

        fn press(&mut self, action: InputAction) -> Result<Navigation> {
            let wrapped = self.input(action);
            self.current = self.observe();
            Ok(Navigation {
                action,
                retried: false,
                wrapped,
            })
        }
    }

    #[test]
    fn a_blank_search_visits_every_item_in_one_list() {
        let (seen, total) = walk(EquipmentKind::Helmet, &[11], (0, 4));
        assert_eq!(seen, total);
    }

    #[test]
    fn a_blank_search_covers_categories_that_wrap_on_their_own() {
        // If Down at the bottom of a category wraps to its own top, Z/C still crosses.
        let (seen, total) = walk_with(EquipmentKind::Throwable, &[6, 3, 8], (2, 5), true);
        assert_eq!(seen, total);
    }

    #[test]
    fn a_blank_search_visits_every_item_in_every_category() {
        let (seen, total) = walk(EquipmentKind::Primary, &[5, 4, 7], (1, 2));
        assert_eq!(seen, total);
    }
}

#[cfg(test)]
mod wait_tests {
    use super::*;

    #[test]
    fn waits_stretch_with_slow_screen_checks_but_stay_bounded() {
        let ms = Duration::from_millis;
        // A normal PC: checks of a few ms barely change the wait.
        assert_eq!(stretched_wait(NAVIGATION_TIMEOUT, ms(10)), ms(4030));
        // The reported PC: 3 s per check leaves room for three checks.
        assert_eq!(stretched_wait(NAVIGATION_TIMEOUT, ms(3000)), ms(13000));
        // Never longer than the cap.
        assert_eq!(stretched_wait(NAVIGATION_TIMEOUT, ms(20000)), MAX_WAIT);
    }
}
