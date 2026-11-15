// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Standalone, hardware-independent image pipeline microbenchmark.
//! Run with `cargo run --release --bin boothsoft-bench -- 50`.
//! This reports wall time only; it does not claim real-world camera/GPU/CPU usage.

use boothsoft::{effects, compositor};
use image::{Rgb, RgbImage, Rgba, RgbaImage};
use std::{hint::black_box, time::Instant};

fn run<F>(iterations: u32, mut operation: F) -> (f64, f64)
where
    F: FnMut(),
{
    operation(); // Warm-up; exclude initial allocation and lazy initialization.
    let start = Instant::now();
    for _ in 0..iterations {
        operation();
    }
    let milliseconds = start.elapsed().as_secs_f64() * 1_000.0;
    (milliseconds, milliseconds / f64::from(iterations))
}

fn main() {
    let iterations = std::env::args()
        .nth(1)
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|count| (1..=500).contains(count))
        .unwrap_or(30);
    let preview = RgbImage::from_fn(854, 480, |x, y| {
        Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
    });
    let original = RgbImage::from_fn(1920, 1080, |x, y| {
        Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
    });
    let mut frame = RgbaImage::from_pixel(1600, 1200, Rgba([0, 0, 0, 0]));
    for x in 0..frame.width() {
        for y in [0, 1, 1198, 1199] {
            frame.put_pixel(x, y, Rgba([60, 85, 70, 255]));
        }
    }
    let (filter_total, filter_average) = run(iterations, || {
        let image = effects::apply_owned(preview.clone(), effects::PhotoFilter::Sepia);
        black_box(image);
    });
    let frame = compositor::PrintFrame::new(frame);
    let (compose_total, compose_average) = run(iterations, || {
        let image = compositor::compose(&original, Some(&frame));
        black_box(image);
    });
    println!("BoothSoft image pipeline microbenchmark — {iterations} iterations");
    println!("Preview filter 854x480: total {filter_total:.1} ms, average {filter_average:.2} ms");
    println!("Compose 1920x1080 -> 1600x1200: total {compose_total:.1} ms, average {compose_average:.2} ms");
    // The extreme aspect ratio used to force a giant resize-then-crop
    // intermediate. Keep the input small so this can run in CI safely.
    let narrow = RgbImage::from_pixel(320, 4096, Rgb([15, 45, 90]));
    let (crop_total, crop_average) = run(iterations, || {
        let output = compositor::cover(&narrow, 1600, 1200);
        black_box(output);
    });
    println!("Extreme crop 320x4096 -> 1600x1200: total {crop_total:.1} ms, average {crop_average:.2} ms");
    println!("Not a camera, GPU, RAM or production latency benchmark; see docs/PERFORMANCE.md.");
}
