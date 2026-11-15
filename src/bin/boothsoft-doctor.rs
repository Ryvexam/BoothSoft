// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Terminal-only device diagnostics. This binary has a console on Windows,
//! unlike the fullscreen kiosk executable, and never opens a GPU window.
//! It enumerates existing OS devices without requiring a server or API token.

use boothsoft::{camera, printer};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg != "--json") {
        eprintln!("Usage: boothsoft-doctor [--json]");
        return ExitCode::from(2);
    }
    let cameras = camera::list_cameras();
    let printers = printer::list_printers();
    let failed = cameras.is_err() || printers.is_err();

    if args.contains(&"--json".to_owned()) {
        let result = serde_json::json!({
            "camera_error": cameras.as_ref().err(),
            "printer_error": printers.as_ref().err().map(ToString::to_string),
            "cameras": cameras.as_ref().map(|found| found.iter().map(|item| {
                serde_json::json!({"index":item.index,"name":item.name})
            }).collect::<Vec<_>>()).unwrap_or_default(),
            "printers": printers.as_ref().ok(),
        });
        println!("{}", serde_json::to_string_pretty(&result).expect("serializable diagnostic"));
    } else {
        match &cameras {
            Ok(devices) => {
                println!("Cameras ({}):", devices.len());
                for device in devices { println!("  #{} {}", device.index, device.name); }
            }
            Err(error) => eprintln!("Camera discovery failed: {error}"),
        }
        match &printers {
            Ok(devices) => {
                println!("Printers ({}):", devices.len());
                for device in devices { println!("  {device}"); }
            }
            Err(error) => eprintln!("Printer discovery failed: {error}"),
        }
    }
    if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}
