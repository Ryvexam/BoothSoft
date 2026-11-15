// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
mod kiosk;

use boothsoft::{storage::Storage, theme, theme_config};
use eframe::egui;
use kiosk::Kiosk;

/// Image buffers (camera frames, 1800x1200 composites, JPEG scratch) are
/// multi-megabyte. glibc's *dynamic* mmap threshold grows after the first free
/// and then keeps those buffers in the heap forever (~60 MB retained RSS after
/// one photo). Pinning the threshold makes every large buffer an mmap that is
/// released immediately. macOS and Windows allocators already do this.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn tune_allocator() {
    // SAFETY: mallopt is called once, before any thread is spawned.
    unsafe {
        libc::mallopt(libc::M_MMAP_THRESHOLD, 512 * 1024);
        libc::mallopt(libc::M_TRIM_THRESHOLD, 1024 * 1024);
        libc::mallopt(libc::M_ARENA_MAX, 2);
    }
}
#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn tune_allocator() {}

fn main() -> eframe::Result {
    tune_allocator();
    // Without a GPU, Mesa's llvmpipe renders on the CPU. Its default worker pool
    // (one thread per core) spends more time synchronising than drawing for a
    // kiosk scene: measured 173 % -> 103 % CPU for the same 30 FPS live view with
    // one thread. GPU drivers ignore this variable. An explicit value is kept.
    if std::env::var_os("LP_NUM_THREADS").is_none() {
        // SAFETY (edition 2021: safe fn): set before any thread or GL context exists.
        std::env::set_var("LP_NUM_THREADS", "1");
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut cli_theme: Option<String> = None;
    let mut demo = false;
    let mut windowed = false;
    let mut safe_mode = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--theme" if index + 1 < args.len() => {
                index += 1;
                cli_theme = Some(args[index].clone());
            }
            "--demo" => demo = true,
            "--windowed" => windowed = true,
            // Added by boothsoft-guard after repeated crashes.
            "--safe-mode" => safe_mode = true,
            "--help" | "-h" => {
                println!("BoothSoft {}\nUsage: boothsoft [--theme NAME] [--demo] [--windowed]", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            _ => {
                eprintln!("Invalid argument. Usage: boothsoft [--theme NAME] [--demo] [--windowed]");
                std::process::exit(2);
            }
        }
        index += 1;
    }
    let themes_root = theme_config::installed_themes_root().unwrap_or_else(|error| {
        eprintln!("Cannot find themes: {error}");
        std::process::exit(2);
    });
    let mut storage = Storage::open().unwrap_or_else(|error| {
        eprintln!("Cannot initialize local storage: {error}");
        std::process::exit(2);
    });
    boothsoft::log::init(&storage.root);
    boothsoft::log::install_panic_hook();
    boothsoft::log::info(format!("BoothSoft {} starting ({}, {})", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH));

    if safe_mode {
        // Fewest moving parts: bundled theme, CPU live view (no direct OpenGL path).
        boothsoft::log::info("Safe mode: bundled theme, CPU live view");
        cli_theme = Some("default".into());
        std::env::set_var("BOOTHSOFT_LIVE_CPU", "1");
    }
    let lang = storage.settings.language.resolve();
    boothsoft::i18n::set(lang);
    boothsoft::log::info(format!("Language: {} (setting {:?}, system {:?})", lang.code(), storage.settings.language,
        boothsoft::i18n::system().map(boothsoft::i18n::Lang::code)));
    if demo { storage.settings.demo_mode = true; }
    if windowed { storage.settings.fullscreen = false; }
    // Explicit command-line themes are strict. A removed saved theme falls back
    // to the bundled default, with a diagnostic visible in administration.
    let requested_id = cli_theme.as_deref().unwrap_or(&storage.settings.theme_id);
    let (selected_theme, startup_warning) = match theme_config::load_any(&themes_root, requested_id) {
        Ok(selected) => (selected, None),
        Err(error) if cli_theme.is_none() => {
            let fallback = theme_config::load_any(&themes_root, "default").unwrap_or_else(|fallback_error| {
                eprintln!("Saved theme '{requested_id}' failed: {error}; default failed: {fallback_error}");
                std::process::exit(2);
            });
            (fallback, Some(format!("Saved theme '{requested_id}' unavailable: {error}. Using default.")))
        }
        Err(error) => {
            eprintln!("Cannot load theme '{requested_id}': {error}");
            std::process::exit(2);
        }
    };
    boothsoft::log::info(format!("Theme: {}{}", selected_theme.id,
        startup_warning.as_deref().map(|w| format!(" ({w})")).unwrap_or_default()));
    theme::set_active(selected_theme);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("BoothSoft — Photo kiosk")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 650.0])
            .with_fullscreen(storage.settings.fullscreen),
        ..Default::default()
    };
    eframe::run_native("boothsoft", options, Box::new(move |cc| {
        Ok(Box::new(Kiosk::new(cc, storage, themes_root, startup_warning)))
    }))
}

