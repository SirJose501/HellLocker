//! HellLocker: stopping a running save, apply or scan on purpose.
//!
//! While an action runs, the app watches the stop key (Backspace, which HellLocker
//! never presses itself) and the panel's Stop button. Either sets a flag that every
//! capture and every input checks, so the action ends within a frame, without
//! clicking anything further, and is reported as stopped rather than failed.
use std::sync::atomic::{AtomicBool, Ordering};

static REQUESTED: AtomicBool = AtomicBool::new(false);

/// The key that stops a running action, as shown to the user.
pub const STOP_KEY: &str = "Backspace";

#[derive(Debug)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "stopped by you")
    }
}

impl std::error::Error for Cancelled {}

/// A new action starts: forget an earlier stop.
pub fn reset() {
    REQUESTED.store(false, Ordering::Relaxed);
}

pub fn request() {
    if !REQUESTED.swap(true, Ordering::Relaxed) {
        tracing::info!("stop requested");
    }
}

/// Fails once a stop was requested.
pub fn check() -> anyhow::Result<()> {
    if REQUESTED.load(Ordering::Relaxed) {
        return Err(Cancelled.into());
    }
    Ok(())
}

/// Whether an action's error is a requested stop.
pub fn is_cancel(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| cause.downcast_ref::<Cancelled>().is_some())
}

/// Whether an action ended because the game lost focus.
pub fn lost_focus(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        let text = cause.to_string();
        text.contains("lost foreground")
            || text.contains("lost focus")
            || text.contains("another window became foreground")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;

    #[test]
    fn a_stop_is_recognised_through_context() {
        reset();
        assert!(check().is_ok());
        request();
        let error = check().context("while applying").unwrap_err();
        assert!(is_cancel(&error));
        assert!(!is_cancel(&anyhow::anyhow!("other")));
        assert!(lost_focus(
            &anyhow::anyhow!("game lost foreground; equipment operation stopped").context("failed")
        ));
        reset();
        assert!(check().is_ok());
    }
}
