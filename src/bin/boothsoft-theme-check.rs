// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Validate every installed local theme without launching the GUI.
//! `cargo run --bin boothsoft-theme-check -- themes`

use boothsoft::{theme_config};
use std::{env, path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    let directory = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("themes"));
    match theme_config::discover(&directory) {
        Ok(catalog) => {
            for item in &catalog.installed {
                println!("PASS {} ({})", item.id, item.config.name);
            }
            if catalog.installed.is_empty() || catalog.rejected != 0 {
                eprintln!("Theme check failed: {} valid, {} rejected", catalog.installed.len(), catalog.rejected);
                ExitCode::FAILURE
            } else {
                println!("Validated {} local themes", catalog.installed.len());
                ExitCode::SUCCESS
            }
        }
        Err(error) => {
            eprintln!("Theme check failed: {error}");
            ExitCode::FAILURE
        }
    }
}
