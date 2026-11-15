// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Link between BoothSoft and its guard (`boothsoft-guard`, see `src/bin/boothsoft-guard.rs`).
//!
//! When started by the guard (`BOOTHSOFT_GUARDED=1`), the application prints a
//! heartbeat line on stdout every few seconds — but only while its **UI thread**
//! is still drawing. A crashed process stops the beats by exiting; a frozen UI
//! stops them because the UI no longer stamps `UiPulse`. The guard then
//! restarts the application.
//!
//! Cost: one extra repaint every `BEAT` (5 s) while guarded, nothing otherwise.
use std::{
    io::Write,
    sync::{atomic::{AtomicU64, Ordering}, Arc},
    thread,
    time::{Duration, Instant},
};

/// Environment variable set by the guard on the child process.
pub const GUARDED_ENV: &str = "BOOTHSOFT_GUARDED";
/// Line printed on stdout for each heartbeat.
pub const BEAT_LINE: &str = "BOOTHSOFT-HEARTBEAT";
/// Interval between heartbeats.
pub const BEAT: Duration = Duration::from_secs(5);
/// The guard restarts the application after this long without a heartbeat.
pub const STALL: Duration = Duration::from_secs(30);
/// Exit code meaning "restart me now" (scheduled restart), not a crash.
pub const EXIT_RESTART: i32 = 75;
/// Command-line flag the guard adds after repeated crashes.
pub const SAFE_MODE_FLAG: &str = "--safe-mode";

pub fn guarded() -> bool {
    std::env::var_os(GUARDED_ENV).is_some_and(|v| v == "1")
}

/// Last time the UI thread drew a frame, as milliseconds since `start`.
#[derive(Clone)]
pub struct UiPulse { start: Instant, last: Arc<AtomicU64> }

impl UiPulse {
    /// Call once per drawn frame (one atomic store).
    pub fn stamp(&self) {
        self.last.store(self.start.elapsed().as_millis() as u64, Ordering::Relaxed);
    }
    fn age(&self) -> Duration {
        let now = self.start.elapsed().as_millis() as u64;
        Duration::from_millis(now.saturating_sub(self.last.load(Ordering::Relaxed)))
    }
}

/// Start the heartbeat thread if guarded. `wake` must make the UI draw one frame
/// (an `egui::Context::request_repaint`); it is called every `BEAT`.
pub fn start(wake: impl Fn() + Send + 'static) -> Option<UiPulse> {
    if !guarded() { return None; }
    let pulse = UiPulse { start: Instant::now(), last: Arc::new(AtomicU64::new(0)) };
    let watched = pulse.clone();
    let spawned = thread::Builder::new().name("bs-heartbeat".into()).spawn(move || loop {
        wake();
        thread::sleep(BEAT);
        // The UI had a whole period to answer the wake-up: beat only if it did.
        if watched.age() < BEAT * 2 {
            let mut out = std::io::stdout().lock();
            if writeln!(out, "{BEAT_LINE}").and_then(|()| out.flush()).is_err() { return; }
        }
    });
    spawned.ok().map(|_| pulse)
}

/// Parse `--safe-mode`-style flags shared by the guard and the app.
pub fn is_safe_mode(args: &[String]) -> bool { args.iter().any(|a| a == SAFE_MODE_FLAG) }

/// Crash history kept by the guard: decides restart delay and safe mode.
#[derive(Default)]
pub struct CrashPolicy { crashes: Vec<Instant> }

impl CrashPolicy {
    /// Record a crash (or stall) at `now`; returns true if the next start must be in safe mode
    /// (3 crashes within 5 minutes).
    pub fn record(&mut self, now: Instant) -> bool {
        self.crashes.retain(|t| now.duration_since(*t) < Duration::from_secs(300));
        self.crashes.push(now);
        self.crashes.len() >= 3
    }
    /// A run that lasted 10 minutes clears the history.
    pub fn healthy_run(&mut self, lasted: Duration) {
        if lasted >= Duration::from_secs(600) { self.crashes.clear(); }
    }
    /// Pause before restarting: 1 s, then 3 s, then 10 s for repeated failures.
    pub fn delay(&self) -> Duration {
        match self.crashes.len() { 0 | 1 => Duration::from_secs(1), 2 => Duration::from_secs(3), _ => Duration::from_secs(10) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_crashes_in_five_minutes_mean_safe_mode() {
        let mut policy = CrashPolicy::default();
        let t0 = Instant::now();
        assert!(!policy.record(t0));
        assert!(!policy.record(t0 + Duration::from_secs(60)));
        assert!(policy.record(t0 + Duration::from_secs(120)));
        assert_eq!(policy.delay(), Duration::from_secs(10));
    }

    #[test]
    fn old_crashes_are_forgotten() {
        let mut policy = CrashPolicy::default();
        let t0 = Instant::now();
        policy.record(t0);
        policy.record(t0 + Duration::from_secs(10));
        assert!(!policy.record(t0 + Duration::from_secs(400)));
        policy.healthy_run(Duration::from_secs(700));
        assert_eq!(policy.delay(), Duration::from_secs(1));
    }
}
