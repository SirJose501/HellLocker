use std::sync::Arc;

use crate::item::EquipmentKind;

type AppEventHandler = dyn Fn(AppEvent) + Send + Sync;

#[derive(Clone, Default)]
pub struct AppEventSink(Option<Arc<AppEventHandler>>);

impl AppEventSink {
    pub fn new(handler: impl Fn(AppEvent) + Send + Sync + 'static) -> Self {
        Self(Some(Arc::new(handler)))
    }

    pub fn emit(&self, event: AppEvent) {
        if let Some(handler) = &self.0 {
            handler(event);
        }
    }
}

#[derive(Clone, Debug)]
pub enum PresetCompletion {
    Complete,
    Saved,
    EquipmentApplied,
    BoosterUnavailable,
    FallbackBoosterSaved,
    FallbackBoosterNotSaved,
    /// HellLocker: Scan equipment finished.
    Scanned {
        items: usize,
    },
}

#[derive(Clone, Debug)]
pub enum AppEvent {
    PresetStarted {
        preset: String,
    },
    HotkeyReleaseRequested {
        preset: String,
    },
    PresetCancelled {
        preset: String,
        reason: String,
    },
    EquipmentActionStarted {
        saving: bool,
    },
    StratagemsApplyStarted {
        stratagems: Vec<String>,
        booster: Option<String>,
        booster_confirmed: bool,
    },
    StratagemsProgress {
        remaining: Vec<String>,
    },
    BoosterProgress {
        item_id: String,
        confirmed: bool,
    },
    FallbackBoosterRequested {
        preset: String,
    },
    EquipmentProgress {
        kind: EquipmentKind,
        confirmed: bool,
    },
    EquipmentCaching {
        category: image::RgbaImage,
        repairing: bool,
    },
    /// HellLocker: Scan equipment is on this slot and has kept `items` pictures of it.
    ScanProgress {
        kind: crate::item::EquipmentKind,
        items: usize,
    },
    PresetDone {
        preset: String,
        completion: PresetCompletion,
    },
    PresetFailed {
        preset: String,
        error: String,
    },
}
