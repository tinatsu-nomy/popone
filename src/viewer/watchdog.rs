//! Watchdog for monitoring main-thread responsiveness.
//!
//! The main thread calls [`Heartbeat::tick`] every frame, and the watchdog thread
//! checks the heartbeat periodically. If it is not updated for a given period,
//! the main thread is considered "unresponsive" and a warning is logged.
//!
//! When `update()` is not guaranteed to run (e.g. while minimized), call
//! [`Heartbeat::pause`] to suspend monitoring and avoid false positives.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Sentinel value indicating the paused state (impossible as an epoch-millis value).
const PAUSED: u64 = u64::MAX;

/// Main-thread heartbeat (epoch milliseconds shared via AtomicU64).
#[derive(Clone)]
pub struct Heartbeat(Arc<AtomicU64>);

impl Heartbeat {
    fn new() -> Self {
        Self(Arc::new(AtomicU64::new(epoch_millis())))
    }

    /// Call every frame to record the current time (enables monitoring).
    pub fn tick(&self) {
        self.0.store(epoch_millis(), Ordering::Relaxed);
    }

    /// Suspend monitoring (use when `update()` is not guaranteed, e.g. while minimized).
    pub fn pause(&self) {
        self.0.store(PAUSED, Ordering::Relaxed);
    }

    fn load(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

fn epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Start the watchdog thread and return its [`Heartbeat`].
///
/// - `threshold`: warn if the heartbeat has not been updated within this duration
/// - `interval`: check interval
pub fn start(threshold: Duration, interval: Duration) -> Heartbeat {
    let heartbeat = Heartbeat::new();
    let hb = heartbeat.clone();
    let threshold_ms = threshold.as_millis() as u64;

    std::thread::Builder::new()
        .name("watchdog".into())
        .spawn(move || {
            let mut was_unresponsive = false;
            let mut freeze_start: u64 = 0;

            loop {
                std::thread::sleep(interval);
                let last = hb.load();

                // Skip monitoring while paused (window minimized / inactive, etc.).
                if last == PAUSED {
                    if was_unresponsive {
                        log::info!("[watchdog] Monitoring paused (window minimized/inactive)");
                        was_unresponsive = false;
                    }
                    continue;
                }

                let now = epoch_millis();
                let elapsed = now.saturating_sub(last);

                if elapsed > threshold_ms {
                    if !was_unresponsive {
                        freeze_start = last;
                        log::warn!(
                            "[watchdog] Main thread unresponsive (no heartbeat for {elapsed}ms)"
                        );
                        was_unresponsive = true;
                    } else {
                        let total = now.saturating_sub(freeze_start);
                        log::warn!("[watchdog] Main thread still unresponsive (total {total}ms)");
                    }
                } else if was_unresponsive {
                    let total = now.saturating_sub(freeze_start);
                    log::info!("[watchdog] Main thread recovered after {total}ms freeze");
                    was_unresponsive = false;
                }
            }
        })
        .expect("watchdog thread spawn failed");

    heartbeat
}
