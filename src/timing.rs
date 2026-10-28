//! HellLocker: Patient mode, for slower or hitching PCs.
//!
//! Every wait for the game (a list to open, a page to settle, a click to show) ends as
//! soon as the screen confirms it, so a longer limit costs nothing when the game is
//! quick. Patient mode makes those limits three times as long, for PCs where the game
//! takes longer to respond than HellLocker normally allows. Nothing is clicked while
//! waiting.
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static PATIENT: AtomicBool = AtomicBool::new(false);

/// How much longer waits are in Patient mode.
pub const PATIENT_FACTOR: u32 = 3;

pub fn set_patient(on: bool) {
    PATIENT.store(on, Ordering::Relaxed);
    tracing::info!(patient_mode = on, "game wait limits set");
}

pub fn patient() -> bool {
    PATIENT.load(Ordering::Relaxed)
}

/// A wait limit for the game, stretched in Patient mode.
pub fn wait(base: Duration) -> Duration {
    if patient() {
        base * PATIENT_FACTOR
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patient_mode_stretches_waits() {
        let base = Duration::from_millis(400);
        set_patient(false);
        assert_eq!(wait(base), base);
        set_patient(true);
        assert_eq!(wait(base), Duration::from_millis(1200));
        set_patient(false);
    }
}
