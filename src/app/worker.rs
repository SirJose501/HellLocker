use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use anyhow::{Context, Result};
use tracing::{debug, error, info_span};

use super::failure::save_failure_report;
use crate::app_events::{AppEvent, AppEventSink};
use crate::app_paths::AppPaths;
use crate::capture::CaptureSessionManager;
use crate::preset_action::{
    PresetActionContext, PresetActionOptions, PresetActionOutcome, PresetCommand, execute_preset,
};
use crate::vision::RecognizerRuntime;
use crate::window::{WindowIdentity, WindowTarget};

pub(super) enum Work {
    Prepare(WindowIdentity),
    Discard,
    LoadPreviews,
    Run {
        preset: String,
        command: PresetCommand,
        settings: PresetActionOptions,
        target: WindowIdentity,
    },
    Shutdown,
}

pub(super) enum WorkerEvent {
    Progress(AppEvent),
    Finished {
        saved: bool,
        /// HellLocker 1.9.4: a preset was applied (for Recently used).
        applied: bool,
    },
    PreviewsLoaded(
        std::result::Result<
            std::collections::BTreeMap<String, crate::preset::preview::PresetPreview>,
            String,
        >,
    ),
}

pub(super) struct ActionWorker {
    commands: Sender<Work>,
    pub events: Receiver<WorkerEvent>,
    thread: Option<JoinHandle<()>>,
}

impl ActionWorker {
    pub fn start(paths: AppPaths) -> Result<Self> {
        let (commands, receiver) = mpsc::channel();
        let (event_tx, events) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("action".into())
            .spawn(move || {
                let runtime = match RecognizerRuntime::load() {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
                };
                if ready_tx.send(Ok(())).is_err() { return; }
                let progress = event_tx.clone();
                let sink = AppEventSink::new(move |event| {
                    let _ = progress.send(WorkerEvent::Progress(event));
                });
                let mut capture_session = CaptureSessionManager::new();
                let cache_path = paths.presets.with_file_name("equipment-cache.json");
                let mut equipment_cache = crate::loadout::equipment::EquipmentCache::load(&cache_path);
                while let Ok(mut command) = receiver.recv() {
                    // Only the latest pending preparation hint matters. A Run or
                    // Shutdown command is a boundary and is never coalesced away.
                    while matches!(command, Work::Prepare(_) | Work::Discard) {
                        match receiver.try_recv() {
                            Ok(next) => command = next,
                            Err(_) => break,
                        }
                    }
                    match command {
                        Work::Prepare(target) => prepare_capture(&mut capture_session, target),
                        Work::Discard => capture_session.cancel_prepare(),
                        Work::LoadPreviews => {
                            let previews = crate::preset::preview::load(&paths.presets)
                                .map_err(|error| format!("{error:#}"));
                            if event_tx.send(WorkerEvent::PreviewsLoaded(previews)).is_err() { break; }
                        }
                        Work::Shutdown => break,
                        Work::Run { preset, command, settings, target } => {
                            let span = info_span!("preset_action", %preset, ?command);
                            let _guard = span.enter();
                            let evidence = crate::evidence::OperationEvidence::new();
                            let mut context = PresetActionContext {
                                presets: &paths.presets,
                                options: settings,
                                events: &sink,
                                equipment_cache: &mut equipment_cache,
                                evidence: &evidence,
                            };
                            let start = Instant::now();
                            crate::cancel::reset();
                            let outcome = execute_preset(&runtime, &mut context, &preset, command, &mut capture_session, target);
                            // HellLocker: a stop the user asked for is not a failure.
                            // Clicking HellLocker's own panel (its Stop button, say) takes
                            // focus from the game; that is the user stopping it too.
                            let stopped = outcome.as_ref().err().is_some_and(|error| {
                                crate::cancel::is_cancel(error)
                                    || (crate::cancel::lost_focus(error) && own_window_in_front())
                            });
                            if stopped {
                                tracing::info!(elapsed_s = start.elapsed().as_secs_f64(), "preset action stopped by the user");
                                sink.emit(AppEvent::PresetCancelled { preset: preset.clone(), reason: "you stopped it".into() });
                            } else if let Err(error) = &outcome {
                                let message = format!("{error:#}");
                                error!(elapsed_s = start.elapsed().as_secs_f64(), error = %message, "preset action failed");
                                sink.emit(AppEvent::PresetFailed { preset: preset.clone(), error: message });
                            }
                            capture_session.finish_action();
                            crate::cancel::reset();
                            if let Err(error) = &outcome
                                && !stopped
                            {
                                save_failure_report(&paths.last_failure, &preset, command, error, Some(&evidence));
                            }
                            equipment_cache.save_if_changed(&cache_path);
                            let saved = matches!(&outcome, Ok(PresetActionOutcome::Saved));
                            let applied = matches!(&outcome, Ok(PresetActionOutcome::Applied));
                            // Completion follows all progress on the same FIFO channel.
                            if event_tx.send(WorkerEvent::Finished { saved, applied }).is_err() { break; }
                        }
                    }
                }
            })
            .context("failed to start action worker")?;
        let worker = Self {
            commands,
            events,
            thread: Some(thread),
        };
        ready_rx
            .recv()
            .context("action worker stopped during initialization")??;
        Ok(worker)
    }

    pub fn send(&self, command: Work) -> Result<()> {
        self.commands.send(command).context("action worker stopped")
    }

    pub fn shutdown(&mut self) {
        let _ = self.commands.send(Work::Shutdown);
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::error!("action worker panicked");
        }
    }
}

impl Drop for ActionWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn prepare_capture(capture_session: &mut CaptureSessionManager, identity: WindowIdentity) {
    // Opening the panel can change foreground before the worker consumes this hint.
    let result =
        WindowTarget::from_identity(identity).and_then(|target| capture_session.prepare(&target));

    if let Err(error) = result {
        debug!(
            error = %format!("{error:#}"),
            "capture preparation skipped"
        );
    }
}

/// HellLocker: whether one of HellLocker's own windows is in front.
#[cfg(target_os = "windows")]
fn own_window_in_front() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    let mut process = 0u32;
    unsafe { GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut process)) };
    process == std::process::id()
}

#[cfg(not(target_os = "windows"))]
fn own_window_in_front() -> bool {
    false
}
