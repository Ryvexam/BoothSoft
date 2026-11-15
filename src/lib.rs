// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! BoothSoft's reusable native photobooth engine.
//!
//! The interactive kiosk, diagnostics, and microbenchmarks link the same modules.
//! No module is copy-included to silence dead-code warnings.
/// Attribution required by the licence (CPAL-1.0, Exhibit B in `LICENSE`): shown
/// for a few seconds at every launch, on the sleep screen and in maintenance.
/// Modified versions must keep an equally prominent display.
pub const ATTRIBUTION: &str = "BoothSoft, créé par Dôm'Booth — dombooth.com";

pub mod audio;
pub mod autostart;
pub mod avi;
pub mod camera;
pub mod compositor;
pub mod devices;
pub mod disk;
pub mod durable_file;
pub mod effects;
pub mod guestbook;
pub mod i18n;
pub mod image_io;
pub mod log;
pub mod watchdog;
pub mod mjpeg;
pub mod model;
pub mod photo_worker;
pub mod print_jobs;
pub mod print_quota;
pub mod printer;
pub mod storage;
pub mod theme;
pub mod theme_config;
pub mod theme_import;
