// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Day-by-day log on disk (`BoothSoft/logs/YYYY-MM-DD.log`), for diagnosing an
//! evening after the fact: nobody watches the booth while it runs.
//!
//! Writing happens on a dedicated thread; callers only push a line into a
//! bounded channel (never blocks — a line is dropped if 256 are already waiting).
//! Lines are in English: they are diagnostics, not guest-facing text.
use chrono::Local;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{mpsc::{self, SyncSender}, OnceLock},
    thread,
};

/// Number of daily files kept.
pub const KEEP_DAYS: usize = 14;

static SINK: OnceLock<SyncSender<String>> = OnceLock::new();
static DIR: OnceLock<PathBuf> = OnceLock::new();

/// Start the writer. Later calls are ignored. Without `init`, `info` does nothing.
pub fn init(data_root: &Path) {
    let dir = data_root.join("logs");
    if fs::create_dir_all(&dir).is_err() { return; }
    prune(&dir, KEEP_DAYS);
    let (tx, rx) = mpsc::sync_channel::<String>(256);
    if SINK.set(tx).is_err() { return; }
    let _ = DIR.set(dir.clone());
    let spawned = thread::Builder::new().name("bs-log".into()).spawn(move || {
        while let Ok(line) = rx.recv() {
            let file = dir.join(format!("{}.log", Local::now().format("%Y-%m-%d")));
            if let Ok(mut out) = OpenOptions::new().create(true).append(true).open(file) {
                let _ = out.write_all(line.as_bytes());
                // Drain what queued meanwhile into the same open file.
                while let Ok(more) = rx.try_recv() { let _ = out.write_all(more.as_bytes()); }
            }
        }
    });
    if spawned.is_err() { eprintln!("BoothSoft: log thread unavailable"); }
}

/// Append one line. Cheap and non-blocking; safe from any thread.
pub fn info(message: impl AsRef<str>) {
    let Some(sink) = SINK.get() else { return; };
    let clean: String = message.as_ref().chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let _ = sink.try_send(format!("{} {}\n", Local::now().format("%Y-%m-%dT%H:%M:%S%.3f"), clean));
}

/// Folder holding the log files, once `init` has run.
pub fn directory() -> Option<&'static Path> { DIR.get().map(PathBuf::as_path) }

/// Record panics too (the default hook still prints them).
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        let location = panic.location().map(|l| format!(" at {}:{}", l.file(), l.line())).unwrap_or_default();
        let message = panic.payload().downcast_ref::<&str>().map(|s| (*s).to_owned())
            .or_else(|| panic.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown".into());
        info(format!("PANIC{location}: {message}"));
        previous(panic);
    }));
}

/// Keep the `keep` most recent `YYYY-MM-DD.log` files.
fn prune(dir: &Path, keep: usize) {
    let Ok(entries) = fs::read_dir(dir) else { return; };
    let mut logs: Vec<PathBuf> = entries.flatten().map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "log")
            && p.file_stem().and_then(|s| s.to_str()).is_some_and(|s| s.len() == 10 && s.as_bytes()[4] == b'-'))
        .collect();
    logs.sort();
    let excess = logs.len().saturating_sub(keep);
    for old in logs.into_iter().take(excess) { let _ = fs::remove_file(old); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_keeps_the_most_recent_days() {
        let dir = std::env::temp_dir().join(format!("boothsoft-log-test-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&dir).unwrap();
        for day in 1..=20 { fs::write(dir.join(format!("2026-11-{day:02}.log")), b"x").unwrap(); }
        fs::write(dir.join("notes.txt"), b"keep me").unwrap();
        prune(&dir, 14);
        let mut left: Vec<_> = fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().into_string().unwrap()).collect();
        left.sort();
        assert_eq!(left.len(), 15);
        assert_eq!(left[0], "2026-11-07.log");
        assert!(left.contains(&"notes.txt".to_owned()));
        fs::remove_dir_all(dir).unwrap();
    }
}
