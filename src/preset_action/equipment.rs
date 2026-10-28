//! Connect equipment automation to persistent presets and application feedback.

use anyhow::{Context, Result, ensure};

use super::{PresetActionContext, PresetActionOutcome, PresetCommand};
use crate::{
    app_events::{AppEvent, PresetCompletion},
    automation::AutomationSession,
    capture::CaptureSource,
    game_settings::GameColorSettings,
    item::EquipmentKind,
    loadout::equipment::{ApplyProgress, EquipmentSet, Session},
    preset::equipment as storage,
    vision::equipment::EquipmentObserver,
    window::WindowTarget,
};

pub(super) fn execute(
    capture: &mut CaptureSource,
    target: WindowTarget,
    colors: GameColorSettings,
    context: &mut PresetActionContext<'_>,
    preset: &str,
    command: PresetCommand,
) -> Result<(PresetActionOutcome, PresetCompletion)> {
    if command == PresetCommand::SaveEquipment {
        let items = sample(capture, target, colors, context, preset)?;
        storage::save(context.presets, preset, &items)
            .context("failed to save equipment preset")?;
        Ok((PresetActionOutcome::Saved, PresetCompletion::Saved))
    } else {
        let items =
            storage::load(context.presets, preset).context("failed to load equipment preset")?;
        apply(capture, target, colors, context, preset, &items)?;
        Ok((
            PresetActionOutcome::Applied,
            PresetCompletion::EquipmentApplied,
        ))
    }
}

pub(super) fn sample(
    capture: &mut CaptureSource,
    target: WindowTarget,
    colors: GameColorSettings,
    context: &mut PresetActionContext<'_>,
    preset: &str,
) -> Result<EquipmentSet> {
    context
        .events
        .emit(AppEvent::EquipmentActionStarted { saving: true });
    // HellLocker: without attire, keep the preset's saved attire; a preset that has
    // none yet is saved in full.
    let attire = if storage::has_saved(context.presets, preset) {
        context.options.attire
    } else {
        crate::item::AttireChoice::ALL
    };
    let kinds = EquipmentKind::selected(attire);
    let library = crate::preset::library::Library::load(context.presets);
    let presets = context.presets;
    with_session(capture, target, colors, context.evidence, |session| {
        session.sample_all(&kinds, (!library.is_empty()).then_some((presets, &library)))
    })
}

/// HellLocker: Scan equipment. Walks all six lists, saving each slot's part of the
/// library as soon as that slot is done, so an interrupted scan keeps its progress.
pub(super) fn scan(
    capture: &mut CaptureSource,
    target: WindowTarget,
    colors: GameColorSettings,
    context: &mut PresetActionContext<'_>,
) -> Result<(PresetActionOutcome, PresetCompletion)> {
    context
        .events
        .emit(AppEvent::EquipmentActionStarted { saving: true });
    let presets = context.presets;
    let events = context.events;
    let cache = &mut *context.equipment_cache;
    let mut total = 0;
    with_session(capture, target, colors, context.evidence, |session| {
        for kind in EquipmentKind::ALL {
            events.emit(AppEvent::ScanProgress { kind, items: 0 });
            let batch = session.scan_kind(kind, cache, |items| {
                events.emit(AppEvent::ScanProgress { kind, items });
            })?;
            crate::preset::library::save_batch(presets, &batch)
                .with_context(|| format!("failed to save the scanned {kind:?} pictures"))?;
            total += batch.len();
        }
        Ok(())
    })?;
    tracing::info!(items = total, "equipment scan saved");
    Ok((
        PresetActionOutcome::Saved,
        PresetCompletion::Scanned { items: total },
    ))
}

pub(super) fn apply(
    capture: &mut CaptureSource,
    target: WindowTarget,
    colors: GameColorSettings,
    context: &mut PresetActionContext<'_>,
    preset: &str,
    items: &EquipmentSet,
) -> Result<()> {
    context
        .events
        .emit(AppEvent::EquipmentActionStarted { saving: false });
    let client_size = capture.output_size();
    let kinds = EquipmentKind::selected(context.options.attire);
    with_session(capture, target, colors, context.evidence, |session| {
        session.apply_all(items, &kinds, context.equipment_cache, |progress| {
            let event = match progress {
                ApplyProgress::Selecting(kind) => AppEvent::EquipmentProgress {
                    kind,
                    confirmed: false,
                },
                ApplyProgress::Caching {
                    category,
                    repairing,
                } => AppEvent::EquipmentCaching {
                    category: crate::preset::preview::equipment_category(category),
                    repairing,
                },
                ApplyProgress::Selected {
                    kind,
                    corrected_category,
                } => {
                    if let Some(category) = corrected_category {
                        match storage::update_category(
                            context.presets,
                            preset,
                            kind,
                            &category,
                            client_size,
                        ) {
                            Ok(()) => {
                                tracing::info!("saved corrected equipment category")
                            }
                            Err(error) => tracing::warn!(path = %context.presets.display(), error = %format!("{error:#}"),
                                "equipment applied, but its corrected category could not be saved"),
                        }
                    }
                    AppEvent::EquipmentProgress {
                        kind,
                        confirmed: true,
                    }
                }
            };
            context.events.emit(event);
        })
    })
}

fn with_session<T>(
    capture: &mut CaptureSource,
    target: WindowTarget,
    colors: GameColorSettings,
    evidence: &crate::evidence::OperationEvidence,
    operation: impl FnOnce(&mut Session<'_>) -> Result<T>,
) -> Result<T> {
    let (width, height) = capture.output_size();
    let (observer, resolved) = EquipmentObserver::resolve(width, height, EquipmentKind::Helmet)?;
    let region = capture.region(resolved.rect, colors)?;
    let automation = AutomationSession::new(region, target, evidence)?;
    let mut session = Session::new(automation, observer, (width, height))?;
    ensure!(
        session.at_entry(),
        "return to the six-card equipment entry before saving or applying equipment"
    );
    operation(&mut session)
}
