// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! On-demand discovery. No device polling on the guest screen or render thread.
use crate::{camera, printer};
use eframe::egui;
use std::{sync::mpsc::{self, Receiver}, thread};

pub struct DeviceInventory {
    pub cameras: Vec<camera::CameraDevice>,
    pub printers: Vec<String>,
    /// The OS default queue, used when maintenance leaves the printer on "default".
    pub default_printer: Option<String>,
    pub camera_error: Option<String>,
    pub printer_error: Option<String>,
}

pub fn scan(ctx: egui::Context) -> Receiver<DeviceInventory> {
    let (tx, rx) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let camera_result = camera::list_cameras();
        let printer_result = printer::list_printers();
        let inventory = DeviceInventory {
            cameras: camera_result.as_ref().map_or_else(|_| Vec::new(), Clone::clone),
            printers: printer_result.as_ref().map_or_else(|_| Vec::new(), Clone::clone),
            default_printer: printer::default_printer(),
            camera_error: camera_result.err(),
            printer_error: printer_result.err().map(|error| error.to_string()),
        };
        if tx.send(inventory).is_ok() { ctx.request_repaint(); }
    });
    rx
}
