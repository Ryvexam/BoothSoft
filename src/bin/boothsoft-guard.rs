// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
//! boothsoft-guard: keeps BoothSoft running, unattended.
//!
//! * starts `boothsoft` (same folder) with the same arguments;
//! * restarts it if it exits with an error, crashes, or stops sending heartbeats
//!   for 30 s (frozen UI) — see `boothsoft::watchdog`;
//! * after 3 failures within 5 minutes, restarts it in safe mode
//!   (bundled theme, CPU live view) until it has run 10 minutes;
//! * exits when BoothSoft exits normally (« Fermer BoothSoft »).
//!
//! Costs nothing while the application is healthy: it sleeps on a channel.
use boothsoft::{log, model::data_root, watchdog};
use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

enum Event { Beat, Closed }

fn app_path() -> PathBuf {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--app") {
        if let Some(path) = args.get(i + 1) { return PathBuf::from(path); }
    }
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("boothsoft-guard"));
    exe.with_file_name(format!("boothsoft{}", std::env::consts::EXE_SUFFIX))
}

/// Arguments for the application: everything except the guard's own `--app PATH`.
fn app_args() -> Vec<String> {
    let mut out = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--app" { args.next(); } else if arg != watchdog::SAFE_MODE_FLAG { out.push(arg); }
    }
    out
}

fn main() {
    log::init(&data_root());
    let app = app_path();
    let base_args = app_args();
    let mut policy = watchdog::CrashPolicy::default();
    let mut safe_mode = false;
    log::info(format!("Guard {} watching {}", env!("CARGO_PKG_VERSION"), app.display()));
    loop {
        let mut args = base_args.clone();
        if safe_mode { args.push(watchdog::SAFE_MODE_FLAG.into()); }
        let started = Instant::now();
        let mut child = match Command::new(&app).args(&args).env(watchdog::GUARDED_ENV, "1")
            .stdout(Stdio::piped()).spawn() {
            Ok(child) => child,
            Err(error) => {
                log::info(format!("Guard: cannot start {}: {error}", app.display()));
                thread::sleep(Duration::from_secs(10));
                continue;
            }
        };
        log::info(format!("Guard: started pid {}{}", child.id(), if safe_mode { " in safe mode" } else { "" }));

        // Forward the child's stdout; heartbeats become events, other lines pass through.
        let (tx, rx) = mpsc::channel();
        if let Some(stdout) = child.stdout.take() {
            thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break; };
                    if line.trim() == watchdog::BEAT_LINE {
                        if tx.send(Event::Beat).is_err() { return; }
                    } else {
                        println!("{line}");
                    }
                }
                let _ = tx.send(Event::Closed);
            });
        }

        // Wait for beats; the first beat gets extra time (start-up, shader compilation).
        let mut stalled = false;
        let mut first = true;
        loop {
            let limit = if first { watchdog::STALL * 2 } else { watchdog::STALL };
            match rx.recv_timeout(limit) {
                Ok(Event::Beat) => first = false,
                Ok(Event::Closed) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    stalled = true;
                    log::info(format!("Guard: no heartbeat for {} s, killing pid {}", limit.as_secs(), child.id()));
                    let _ = child.kill();
                    break;
                }
            }
        }
        let status = child.wait();
        let lasted = started.elapsed();
        policy.healthy_run(lasted);
        let code = status.as_ref().ok().and_then(std::process::ExitStatus::code);
        match code {
            Some(0) if !stalled => {
                log::info("Guard: BoothSoft closed normally, guard stops");
                return;
            }
            Some(watchdog::EXIT_RESTART) if !stalled => {
                log::info("Guard: scheduled restart");
                safe_mode = false;
                continue;
            }
            _ => {
                log::info(format!("Guard: BoothSoft stopped after {} s ({}), restarting",
                    lasted.as_secs(), if stalled { "frozen".to_owned() } else { format!("{status:?}") }));
                safe_mode = policy.record(Instant::now()) || (safe_mode && lasted < Duration::from_secs(600));
                thread::sleep(policy.delay());
            }
        }
    }
}
