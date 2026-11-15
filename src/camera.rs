// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Native camera worker with a single-slot, latest-frame preview buffer.
//!
//! Camera I/O and decoding never run on the UI thread. A preview that the UI has
//! not consumed is overwritten, rather than queued. Captured full-resolution
//! frames use a separate reliable channel and are never dropped intentionally.
use image::{RgbImage, RgbaImage};
use std::{
    sync::{
        mpsc::{self, Receiver, Sender, SyncSender},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

/// 30 FPS target for a fluid mirror; drivers may deliver less in low light.
pub const PREVIEW_INTERVAL: Duration = Duration::from_millis(33);
/// Longest edge of the preview when it is drawn through egui textures (CPU fallback).
pub const PREVIEW_WIDTH: u32 = 854;

#[derive(Debug, Clone)]
pub struct CameraDevice {
    pub index: u32,
    pub name: String,
}

/// Enumerates OS webcam inputs. DSLR devices without webcam/UVC mode require
/// a separate tethering backend and are intentionally not reported as working.
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
pub fn list_cameras() -> Result<Vec<CameraDevice>, String> {
    use nokhwa::{query, utils::ApiBackend};
    #[cfg(target_os = "macos")]
    nokhwa::nokhwa_initialize(|_| {});
    let mut devices = Vec::new();
    for info in query(ApiBackend::Auto).map_err(|error| error.to_string())? {
        if let Ok(index) = info.index().as_index() {
            if devices.len() < 32 {
                devices.push(CameraDevice {
                    index,
                    name: info.human_name().chars().take(100).collect(),
                });
            }
        }
    }
    devices.sort_by_key(|device| device.index);
    Ok(devices)
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub fn list_cameras() -> Result<Vec<CameraDevice>, String> {
    Ok(Vec::new())
}

pub type WakeUi = Arc<dyn Fn() + Send + Sync>;
/// Preview frames are RGBA: GPU drivers (and llvmpipe) take 32-bit texels as a
/// straight copy, whereas 24-bit RGB is repacked on the CPU at every upload.
type PreviewSlot = Arc<Mutex<Option<RgbaImage>>>;

pub enum CameraCommand {
    Start { index: u32 },
    Capture,
    /// Stop decoding/uploading preview frames while keeping the camera open.
    SetPreviewEnabled(bool),
    /// Live-view frames per second (clamped to 10..=30).
    SetFrameRate(u8),
    /// Sleep (attract screen): decode 4 frames/s, display nothing, and report
    /// `CameraEvent::Motion` when someone walks up to the booth.
    SetSleep(bool),
    /// Guest book: copy every live frame's JPEG bytes to this channel (no
    /// re-encoding for MJPEG sources); `None` stops. See `guestbook.rs`.
    Record(Option<SyncSender<RecordFrame>>),
    Stop,
    Shutdown,
}

/// One live frame for the guest-book recorder: the camera's own JPEG bytes.
pub struct RecordFrame {
    pub jpeg: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub at: Instant,
}

pub enum CameraEvent {
    Captured(RgbImage),
    /// Someone is in front of the camera: wakes the booth from sleep, and keeps
    /// it awake while guests move around without touching the screen.
    Motion,
    Status(String),
    Error(String),
}

pub struct CameraWorker {
    tx: Sender<CameraCommand>,
    pub rx: Receiver<CameraEvent>,
    latest: PreviewSlot,
    recycle: SyncSender<RgbaImage>,
}

impl CameraWorker {
    /// `wake_ui` may be called from the camera thread; it must be thread-safe.
    pub fn new(wake_ui: WakeUi) -> Self {
        let (tx, commands) = mpsc::channel();
        let (events, rx) = mpsc::sync_channel(4);
        let latest = Arc::new(Mutex::new(None));
        let preview = Arc::clone(&latest);
        // Bounded: at most a few spare buffers wait; extra ones are simply freed.
        let (recycle, recycled) = mpsc::sync_channel(3);
        thread::Builder::new().name("bs-camera".into())
            .spawn(move || camera_loop(commands, events, preview, recycled, wake_ui))
            .expect("spawn camera thread");
        Self { tx, rx, latest, recycle }
    }

    /// Hand a displayed preview buffer back to the camera thread. Steady-state
    /// preview then decodes into the same two or three buffers forever:
    /// no per-frame allocation, no allocator growth.
    pub fn recycler(&self) -> SyncSender<RgbaImage> {
        self.recycle.clone()
    }

    pub fn send(&self, command: CameraCommand) {
        let _ = self.tx.send(command);
    }

    /// Take ownership of the newest frame; the producer never blocks on the UI.
    pub fn take_preview(&self) -> Option<RgbaImage> {
        self.latest.lock().ok()?.take()
    }
}

impl Drop for CameraWorker {
    fn drop(&mut self) {
        self.send(CameraCommand::Shutdown);
    }
}

/// Returns the frame it replaced (never displayed) so its buffer can be reused.
fn publish_preview(slot: &PreviewSlot, frame: RgbaImage, wake: &WakeUi) -> Option<RgbaImage> {
    let mut latest = slot.lock().ok()?;
    let replaced = latest.replace(frame);
    drop(latest);
    wake();
    replaced
}

fn publish_status(events: &SyncSender<CameraEvent>, event: CameraEvent, wake: &WakeUi) {
    if events.try_send(event).is_ok() {
        wake();
    }
}

/// Where RGB frames come from: a real device, or a synthetic MJPEG stream used to
/// benchmark the preview pipeline without hardware.
trait FrameSource {
    /// Decode the next frame into `buffer` (RGB, or RGBA when `rgba`), reusing its allocation.
    fn read(&mut self, buffer: &mut Vec<u8>, rgba: bool) -> Result<(u32, u32), String>;
    /// Keep a copy of the compressed bytes of each frame read (guest-book recording).
    fn keep_jpeg(&mut self, _keep: bool) {}
    /// JPEG bytes of the last frame read, when the source delivers JPEG and
    /// [`keep_jpeg`](Self::keep_jpeg) is on.
    fn last_jpeg(&self) -> Option<&[u8]> { None }
}

/// JPEG for the recorder: the source's own bytes, or the decoded RGBA frame
/// re-encoded (quality 80) when the camera does not stream MJPEG.
fn record_jpeg(source: &dyn FrameSource, frame: &RgbaImage) -> Option<Vec<u8>> {
    if let Some(jpeg) = source.last_jpeg() { return Some(jpeg.to_vec()); }
    let result = std::panic::catch_unwind(|| -> std::io::Result<Vec<u8>> {
        let mut compress = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_EXT_RGBA);
        compress.set_size(frame.width() as usize, frame.height() as usize);
        compress.set_quality(80.0);
        compress.set_fastest_defaults();
        let mut started = compress.start_compress(Vec::with_capacity(200_000))?;
        started.write_scanlines(frame.as_raw())?;
        started.finish()
    });
    result.ok().and_then(Result::ok)
}

/// Full-resolution capture for the photo pipeline (RGB, fresh buffer).
fn read_capture(source: &mut dyn FrameSource) -> Result<RgbImage, String> {
    let mut buffer = Vec::new();
    let (width, height) = source.read(&mut buffer, false)?;
    RgbImage::from_raw(width, height, buffer).ok_or_else(|| "Frame buffer size mismatch".to_owned())
}

/// Live-view frame decoded straight to RGBA into a recycled buffer.
fn read_preview(source: &mut dyn FrameSource, mut buffer: Vec<u8>) -> Result<RgbaImage, String> {
    let (width, height) = source.read(&mut buffer, true)?;
    RgbaImage::from_raw(width, height, buffer).ok_or_else(|| "Frame buffer size mismatch".to_owned())
}

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
struct DeviceSource(nokhwa::Camera, Option<Vec<u8>>);

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
impl FrameSource for DeviceSource {
    fn read(&mut self, buffer: &mut Vec<u8>, rgba: bool) -> Result<(u32, u32), String> {
        let frame = self.0.frame().map_err(|error| error.to_string())?;
        let resolution = frame.resolution();
        let (width, height) = (resolution.width_x, resolution.height_y);
        if frame.source_frame_format() == nokhwa::utils::FrameFormat::MJPEG {
            if let Some(kept) = self.1.as_mut() { kept.clear(); kept.extend_from_slice(frame.buffer()); }
            // Our own libjpeg-turbo path: fast settings for the live view, and a
            // corrupt frame is an error instead of an unwinding crash.
            let quality = if rgba { crate::mjpeg::Quality::Preview } else { crate::mjpeg::Quality::Photo };
            return crate::mjpeg::decode(frame.buffer(), buffer, rgba, quality);
        }
        buffer.resize(width as usize * height as usize * if rgba { 4 } else { 3 }, 0);
        if rgba {
            frame.decode_image_to_buffer::<nokhwa::pixel_format::RgbAFormat>(buffer)
        } else {
            frame.decode_image_to_buffer::<nokhwa::pixel_format::RgbFormat>(buffer)
        }.map_err(|error| error.to_string())?;
        Ok((width, height))
    }

    fn keep_jpeg(&mut self, keep: bool) { self.1 = keep.then(Vec::new); }

    fn last_jpeg(&self) -> Option<&[u8]> { self.1.as_deref().filter(|b| !b.is_empty()) }
}

/// Pre-encoded 1280x720 MJPEG loop (moving shapes), decoded per frame exactly
/// like a webcam stream. Enabled with `BOOTHSOFT_SYNTHETIC_CAMERA=1`.
/// Skips redundant redraws when nothing moves in front of the camera.
///
/// Rendering, not decoding, dominates live-view cost (especially without a
/// GPU). Each decoded frame is summarised as the mean luma of a 16x9 grid
/// (~3.6 k sampled pixels, a few microseconds). It is displayed only if a cell
/// changed noticeably since the last *displayed* frame, or as a keep-alive.
/// A frozen pose (countdown) or an empty booth then costs a third of the
/// rendering while looking identical; any movement is shown at full rate.
pub struct MotionGate {
    last: Option<[u16; MotionGate::CELLS]>,
    last_shown: Instant,
}

impl MotionGate {
    const COLUMNS: usize = 16;
    const ROWS: usize = 9;
    const CELLS: usize = Self::COLUMNS * Self::ROWS;
    /// Mean-luma change of one cell (0..255 scale) that counts as motion.
    /// Sensor noise averaged over a cell stays well below this.
    const THRESHOLD: u16 = 3;
    /// Static scenes are still refreshed at 10 FPS (exposure drift, safety).
    pub const KEEP_ALIVE: Duration = Duration::from_millis(100);

    pub fn new() -> Self { Self { last: None, last_shown: Instant::now() } }

    pub fn reset(&mut self) { self.last = None; }

    fn signature(frame: &RgbaImage) -> [u16; Self::CELLS] {
        let (width, height) = (frame.width() as usize, frame.height() as usize);
        let raw = frame.as_raw();
        let mut sums = [0u32; Self::CELLS];
        let mut counts = [0u32; Self::CELLS];
        // Every 8th pixel on every 8th row is plenty for a 144-cell average.
        for y in (0..height).step_by(8) {
            let row = y * Self::ROWS / height.max(1);
            for x in (0..width).step_by(8) {
                let i = (y * width + x) * 4;
                let luma = (77 * u32::from(raw[i]) + 150 * u32::from(raw[i + 1]) + 29 * u32::from(raw[i + 2])) >> 8;
                let cell = row * Self::COLUMNS + x * Self::COLUMNS / width.max(1);
                sums[cell] += luma;
                counts[cell] += 1;
            }
        }
        std::array::from_fn(|i| (sums[i] / counts[i].max(1)) as u16)
    }

    /// Decide whether `frame` must be displayed now.
    pub fn should_show(&mut self, frame: &RgbaImage, now: Instant) -> bool {
        let signature = Self::signature(frame);
        let moved = self.last.is_none_or(|last| last.iter().zip(&signature).any(|(a, b)| a.abs_diff(*b) > Self::THRESHOLD));
        if moved || now.duration_since(self.last_shown) >= Self::KEEP_ALIVE {
            self.last = Some(signature);
            self.last_shown = now;
            true
        } else {
            false
        }
    }
}

impl Default for MotionGate {
    fn default() -> Self { Self::new() }
}

/// Presence detector used while the booth sleeps. Unlike `MotionGate` (which
/// catches any visible change), it needs a *large* change — several grid cells,
/// on two consecutive samples — so sensor noise, flicker or a far-away guest
/// passing by does not wake the booth. The reference adapts to slow lighting changes.
pub struct PresenceDetector {
    reference: Option<[u16; MotionGate::CELLS]>,
    reference_at: Instant,
    hits: u8,
}

impl PresenceDetector {
    /// Mean-luma change of a cell that counts (0..255 scale).
    const CELL_DELTA: u16 = 12;
    /// Cells (out of 144) that must change: roughly a person-sized area.
    const MIN_CELLS: usize = 6;
    const REFRESH: Duration = Duration::from_secs(20);

    pub fn new() -> Self { Self { reference: None, reference_at: Instant::now(), hits: 0 } }

    pub fn reset(&mut self) { self.reference = None; self.hits = 0; }

    /// Returns true once when presence is confirmed.
    pub fn observe(&mut self, frame: &RgbaImage, now: Instant) -> bool {
        let signature = MotionGate::signature(frame);
        let Some(reference) = self.reference else {
            self.reference = Some(signature);
            self.reference_at = now;
            return false;
        };
        let changed = reference.iter().zip(&signature).filter(|(a, b)| a.abs_diff(**b) > Self::CELL_DELTA).count();
        if changed >= Self::MIN_CELLS {
            self.hits += 1;
        } else {
            self.hits = 0;
            if now.duration_since(self.reference_at) >= Self::REFRESH {
                self.reference = Some(signature);
                self.reference_at = now;
            }
        }
        if self.hits >= 2 {
            self.reset();
            return true;
        }
        false
    }
}

impl Default for PresenceDetector {
    fn default() -> Self { Self::new() }
}

/// One pixel of the demo scene: a warm party room and two guests swaying.
fn guests_pixel(x: f32, y: f32, t: f32) -> [u8; 3] {
    let people = [
        (455.0 + 14.0 * t.sin(), 330.0 + 10.0 * (2.0 * t).cos(), [244, 200, 168], [92, 58, 44], [240, 71, 125], [61, 214, 196]),
        (835.0 + 12.0 * (t + 1.3).sin(), 350.0 + 12.0 * (2.0 * t + 0.8).cos(), [226, 172, 134], [40, 36, 48], [255, 200, 87], [240, 71, 125]),
    ];
    for (cx, cy, skin, hair, shirt, hat) in people {
        let (dx, dy) = (x - cx, y - cy);
        let d = (dx * dx + dy * dy).sqrt();
        // Party hat with stripes and a pompom.
        let (apex, base) = (cy - 205.0, cy - 88.0);
        if ((x - cx).powi(2) + (y - apex).powi(2)).sqrt() < 14.0 { return [255, 250, 240]; }
        if y > apex && y < base && dx.abs() < (y - apex) / (base - apex) * 62.0 {
            return if ((y - apex) / 16.0) as i32 % 2 == 0 { hat } else { hat.map(|c: u8| c.saturating_add(45)) };
        }
        if d < 104.0 {
            // Eyes, smile, cheeks, hair, then skin.
            if ((dx.abs() - 36.0).powi(2) + (dy + 16.0).powi(2)).sqrt() < 11.0 { return [38, 30, 34]; }
            let smile = (dx * dx + (dy - 8.0).powi(2)).sqrt();
            if dy > 30.0 && smile > 50.0 && smile < 59.0 { return [150, 50, 60]; }
            if ((dx.abs() - 60.0).powi(2) + (dy - 30.0).powi(2)).sqrt() < 17.0 { return [240, 150, 140]; }
            if d > 99.0 { return hair; }
            if dy < -52.0 - 18.0 * (dx / 30.0).cos() { return hair; }
            return skin;
        }
        // Shoulders.
        let body = (dx / 200.0).powi(2) + ((y - (cy + 300.0)) / 210.0).powi(2);
        if body < 1.0 && y > cy + 60.0 { return shirt; }
    }
    // Room: plum to warm gradient with a few soft lights.
    let mut rgb = [58.0 + 60.0 * y / 720.0, 38.0 + 34.0 * y / 720.0, 88.0 - 10.0 * y / 720.0];
    for (lx, ly, r, c) in [(150.0, 120.0, 70.0, [255.0, 200.0, 87.0]), (1090.0, 90.0, 90.0, [240.0, 71.0, 125.0]),
                           (640.0, 60.0, 60.0, [61.0, 214.0, 196.0]), (1180.0, 420.0, 55.0, [255.0, 138.0, 76.0]), (90.0, 470.0, 50.0, [177.0, 140.0, 255.0])] {
        let glow = (1.0 - ((x - lx).powi(2) + (y - ly).powi(2)).sqrt() / r).clamp(0.0, 1.0);
        for k in 0..3 { rgb[k] += (c[k] - rgb[k]) * glow * glow * 0.75; }
    }
    rgb.map(|c| c.clamp(0.0, 255.0) as u8)
}

pub struct SyntheticSource {
    frames: Vec<Vec<u8>>,
    next: usize,
    last: usize,
}

impl SyntheticSource {
    pub fn new() -> Self {
        use image::codecs::jpeg::JpegEncoder;
        let (width, height) = (1280u32, 720u32);
        // `BOOTHSOFT_SYNTHETIC_CAMERA=static`: a still scene with sensor-like noise.
        let still = std::env::var_os("BOOTHSOFT_SYNTHETIC_CAMERA").is_some_and(|v| v == "static");
        // `BOOTHSOFT_SYNTHETIC_CAMERA=guests`: two cartoon guests, for screenshots and demos.
        let guests = std::env::var_os("BOOTHSOFT_SYNTHETIC_CAMERA").is_some_and(|v| v == "guests");
        let frames = (0..24).map(|i| {
            let t = if still { 0.0 } else { i as f32 / 24.0 * std::f32::consts::TAU };
            if guests {
                let image = RgbImage::from_fn(width, height, |x, y| image::Rgb(guests_pixel(x as f32, y as f32, t)));
                let mut jpeg = Vec::new();
                JpegEncoder::new_with_quality(&mut jpeg, 88)
                    .encode(image.as_raw(), width, height, image::ExtendedColorType::Rgb8).expect("encode synthetic frame");
                return jpeg;
            }
            let (cx, cy) = (640.0 + 260.0 * t.cos(), 360.0 + 120.0 * (2.0 * t).sin());
            let image = RgbImage::from_fn(width, height, |x, y| {
                let (fx, fy) = (x as f32, y as f32);
                let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
                let noise = if still { ((x.wrapping_mul(7919) ^ y.wrapping_mul(104_729) ^ (i * 31)) % 5) as u8 } else { 0 };
                let base = if d < 150.0 { [236, 190, 160] }
                    else if d < 158.0 { [90, 60, 50] }
                    else { [(120.0 + 80.0 * fy / 720.0) as u8, (150.0 + 40.0 * fx / 1280.0) as u8, 170] };
                image::Rgb(base.map(|c: u8| c.saturating_add(noise)))
            });
            let mut jpeg = Vec::new();
            JpegEncoder::new_with_quality(&mut jpeg, 80)
                .encode(image.as_raw(), width, height, image::ExtendedColorType::Rgb8).expect("encode synthetic frame");
            jpeg
        }).collect();
        Self { frames, next: 0, last: 0 }
    }

    pub fn enabled() -> bool {
        std::env::var_os("BOOTHSOFT_SYNTHETIC_CAMERA").is_some_and(|value| value == "1" || value == "static" || value == "guests")
    }
}

impl Default for SyntheticSource {
    fn default() -> Self { Self::new() }
}

impl FrameSource for SyntheticSource {
    fn read(&mut self, buffer: &mut Vec<u8>, rgba: bool) -> Result<(u32, u32), String> {
        let jpeg = &self.frames[self.next];
        self.last = self.next;
        self.next = (self.next + 1) % self.frames.len();
        // Same decoder and settings as a real MJPEG webcam.
        let quality = if rgba { crate::mjpeg::Quality::Preview } else { crate::mjpeg::Quality::Photo };
        crate::mjpeg::decode(jpeg, buffer, rgba, quality)
    }

    fn last_jpeg(&self) -> Option<&[u8]> { Some(&self.frames[self.last]) }
}

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
fn camera_loop(commands: Receiver<CameraCommand>, events: SyncSender<CameraEvent>, preview: PreviewSlot,
               recycled: Receiver<RgbaImage>, wake: WakeUi) {
    use nokhwa::{
        pixel_format::RgbFormat,
        utils::{CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType},
        Camera,
    };

    #[cfg(target_os = "macos")]
    nokhwa::nokhwa_initialize(|_| {});

    let mut camera: Option<Box<dyn FrameSource>> = None;
    let mut spare: Option<RgbaImage> = None;
    let mut interval = PREVIEW_INTERVAL;
    let mut gate = MotionGate::new();
    let mut sleeping = false;
    let mut presence = PresenceDetector::new();
    const SLEEP_INTERVAL: Duration = Duration::from_millis(250);
    // While live, presence is reported at most every 2 s (it only feeds the idle timer).
    let mut last_presence = Instant::now() - Duration::from_secs(10);
    let mut preview_enabled = true;
    let mut next_preview = Instant::now();
    let mut recorder: Option<SyncSender<RecordFrame>> = None;
    let mut record_dropped = 0u32;
    loop {
        // Block when stopped/paused. When previewing, sleep until the next
        // frame is due instead of periodically polling the application state.
        let command = if camera.is_some() && (preview_enabled || sleeping) {
            commands.recv_timeout(next_preview.saturating_duration_since(Instant::now()))
        } else {
            commands.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected)
        };
        match command {
            Ok(CameraCommand::Start { index }) => {
                camera = None;
                if let Ok(mut slot) = preview.lock() { *slot = None; }
                // Prefer a native 720p/30 FPS MJPEG stream: smooth live view, cheap to decode.
                // Fall back when the camera driver does not expose MJPEG.
                let make_camera = |fourcc| Camera::new(
                    CameraIndex::Index(index),
                    RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(
                        CameraFormat::new_from(1280, 720, fourcc, 30),
                    )),
                );
                if SyntheticSource::enabled() {
                    camera = Some(Box::new(SyntheticSource::new()));
                    preview_enabled = true;
                    next_preview = Instant::now();
                    publish_status(&events, CameraEvent::Status("Synthetic camera connected".into()), &wake);
                    continue;
                }
                let opened = make_camera(FrameFormat::MJPEG)
                    .or_else(|_| make_camera(FrameFormat::YUYV))
                    .or_else(|_| Camera::new(CameraIndex::Index(index),
                        RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestResolution)));
                match opened {
                    Ok(mut opened) => match opened.open_stream() {
                        Ok(()) => {
                            camera = Some(Box::new(DeviceSource(opened, None)));
                            preview_enabled = true;
                            next_preview = Instant::now();
                            publish_status(&events, CameraEvent::Status(format!("Camera {index} connected")), &wake);
                        }
                        Err(error) => publish_status(&events, CameraEvent::Error(format!("Camera stream: {error}")), &wake),
                    },
                    Err(error) => publish_status(&events, CameraEvent::Error(format!("Camera {index}: {error}")), &wake),
                }
            }
            Ok(CameraCommand::Capture) => {
                if let Some(device) = camera.as_mut() {
                    // Captures get a fresh buffer: they leave for the photo worker.
                    match read_capture(device.as_mut()) {
                        Ok(image) => {
                            // Do not decode a second frame in the same iteration.
                            next_preview = Instant::now() + interval;
                            // Capture is never discarded due to preview congestion.
                            if events.send(CameraEvent::Captured(image)).is_err() { break; }
                            wake();
                        }
                        Err(error) => publish_status(&events, CameraEvent::Error(error), &wake),
                    }
                } else {
                    publish_status(&events, CameraEvent::Error("No camera connected".into()), &wake);
                }
            }
            Ok(CameraCommand::SetSleep(enabled)) => {
                sleeping = enabled;
                presence.reset();
                next_preview = Instant::now();
            }
            Ok(CameraCommand::SetFrameRate(fps)) => {
                interval = Duration::from_millis(1_000 / u64::from(fps.clamp(10, 30)));
            }
            Ok(CameraCommand::SetPreviewEnabled(enabled)) => {
                preview_enabled = enabled;
                gate.reset();
                if enabled { next_preview = Instant::now(); }
                else if let Ok(mut slot) = preview.lock() { *slot = None; }
            }
            Ok(CameraCommand::Record(sink)) => {
                if recorder.is_some() && sink.is_none() && record_dropped > 0 {
                    crate::log::info(format!("Guest book: {record_dropped} frame(s) skipped (recorder busy)"));
                }
                record_dropped = 0;
                recorder = sink;
                if let Some(device) = camera.as_mut() { device.keep_jpeg(recorder.is_some()); }
            }
            Ok(CameraCommand::Stop) => {
                camera = None;
                recorder = None;
                if let Ok(mut slot) = preview.lock() { *slot = None; }
                publish_status(&events, CameraEvent::Status("Camera stopped".into()), &wake);
            }
            Ok(CameraCommand::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if let Some(device) = camera.as_mut().filter(|_| sleeping && !preview_enabled && Instant::now() >= next_preview) {
            // Asleep: 4 decodes per second, nothing displayed, buffer reused.
            let buffer = spare.take().map(RgbaImage::into_raw).unwrap_or_default();
            if let Ok(frame) = read_preview(device.as_mut(), buffer) {
                if presence.observe(&frame, Instant::now()) {
                    publish_status(&events, CameraEvent::Motion, &wake);
                }
                spare = Some(frame);
            }
            next_preview = (next_preview + SLEEP_INTERVAL).max(Instant::now());
        }
        if let Some(device) = camera.as_mut().filter(|_| preview_enabled && Instant::now() >= next_preview) {
            // Reuse a buffer the UI has finished displaying, if one came back.
            let buffer = spare.take().or_else(|| recycled.try_iter().last())
                .map(RgbaImage::into_raw).unwrap_or_default();
            if let Ok(frame) = read_preview(device.as_mut(), buffer) {
                let now = Instant::now();
                if let Some(sink) = recorder.as_ref() {
                    // Every frame is recorded, moving or not (no motion gate).
                    if let Some(jpeg) = record_jpeg(device.as_ref(), &frame) {
                        let item = RecordFrame { jpeg, width: frame.width(), height: frame.height(), at: now };
                        match sink.try_send(item) {
                            Ok(()) => {}
                            Err(mpsc::TrySendError::Full(_)) => record_dropped += 1,
                            Err(mpsc::TrySendError::Disconnected(_)) => recorder = None,
                        }
                    }
                }
                if presence.observe(&frame, now) && now.duration_since(last_presence) >= Duration::from_secs(2) {
                    last_presence = now;
                    publish_status(&events, CameraEvent::Motion, &wake);
                }
                if gate.should_show(&frame, now) {
                    // Full sensor resolution goes to the GPU, which scales for free.
                    spare = publish_preview(&preview, frame, &wake);
                } else {
                    // Nothing moved: no redraw; the buffer is reused for the next decode.
                    spare = Some(frame);
                }
            }
            // Schedule from the previous deadline, not from "now": decode time
            // must not stretch the period (30 fps stays 30 fps, not 27).
            next_preview = (next_preview + interval).max(Instant::now());
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn camera_loop(commands: Receiver<CameraCommand>, events: SyncSender<CameraEvent>, _preview: PreviewSlot,
               _recycled: Receiver<RgbaImage>, wake: WakeUi) {
    while let Ok(command) = commands.recv() {
        match command {
            CameraCommand::Start { .. } | CameraCommand::Capture => publish_status(
                &events, CameraEvent::Error("No native camera backend on this platform".into()), &wake,
            ),
            CameraCommand::Stop | CameraCommand::SetPreviewEnabled(_) | CameraCommand::SetFrameRate(_) | CameraCommand::SetSleep(_) | CameraCommand::Record(_) => {}
            CameraCommand::Shutdown => break,
        }
    }
}

pub fn demo_frame() -> RgbImage {
    let (width, height) = (1280, 960);
    RgbImage::from_fn(width, height, |x, y| {
        let wave = (x as f32 / 120.0).sin() * 22.0;
        if y < 650 { image::Rgb([232, 238, 221]) }
        else if (y as f32) < 740.0 + wave { image::Rgb([157, 184, 145]) }
        else { image::Rgb([54, 91, 66]) }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_preview_replaces_older_frame() {
        let slot = Arc::new(Mutex::new(None));
        let wake: WakeUi = Arc::new(|| {});
        assert!(publish_preview(&slot, RgbaImage::from_pixel(1, 1, image::Rgba([1, 0, 0, 255])), &wake).is_none());
        let replaced = publish_preview(&slot, RgbaImage::from_pixel(1, 1, image::Rgba([2, 0, 0, 255])), &wake);
        assert_eq!(replaced.expect("older frame is handed back for reuse").get_pixel(0, 0)[0], 1);
        let frame = slot.lock().unwrap().take().unwrap();
        assert_eq!(frame.get_pixel(0, 0)[0], 2);
        assert!(slot.lock().unwrap().is_none());
    }

    #[test]
    fn preview_budget_is_bounded() {
        assert!(PREVIEW_INTERVAL >= Duration::from_millis(33));
        const { assert!(PREVIEW_WIDTH <= 1280) };
    }

    #[test]
    fn motion_gate_skips_still_frames_and_shows_motion_at_once() {
        let mut gate = MotionGate::new();
        let t0 = Instant::now();
        let still = RgbaImage::from_fn(320, 180, |x, y| image::Rgba([(x % 200) as u8, (y % 150) as u8, 90, 255]));
        assert!(gate.should_show(&still, t0), "first frame is always shown");
        // Sensor-like noise of +-2 levels is not motion.
        let noisy = RgbaImage::from_fn(320, 180, |x, y| {
            let p = still.get_pixel(x, y).0;
            image::Rgba([p[0].saturating_add(((x + y) % 3) as u8), p[1], p[2], 255])
        });
        assert!(!gate.should_show(&noisy, t0 + Duration::from_millis(33)));
        // A hand entering one corner (one cell) is motion.
        let mut moved = still.clone();
        for y in 0..20 { for x in 0..20 { moved.put_pixel(x, y, image::Rgba([255, 255, 255, 255])); } }
        assert!(gate.should_show(&moved, t0 + Duration::from_millis(66)));
        // Keep-alive refresh even when still.
        assert!(!gate.should_show(&moved, t0 + Duration::from_millis(99)));
        assert!(gate.should_show(&moved, t0 + Duration::from_millis(66) + MotionGate::KEEP_ALIVE));
    }

    #[test]
    fn presence_needs_a_large_change_on_two_samples() {
        let mut detector = PresenceDetector::new();
        let t0 = Instant::now();
        let empty = RgbaImage::from_pixel(320, 180, image::Rgba([90, 90, 90, 255]));
        assert!(!detector.observe(&empty, t0));
        // Small change (one cell): nobody there.
        let mut small = empty.clone();
        for y in 0..20 { for x in 0..20 { small.put_pixel(x, y, image::Rgba([255, 255, 255, 255])); } }
        assert!(!detector.observe(&small, t0 + Duration::from_millis(250)));
        assert!(!detector.observe(&small, t0 + Duration::from_millis(500)));
        // A person-sized change must persist for two samples.
        let mut person = empty.clone();
        for y in 40..180 { for x in 100..200 { person.put_pixel(x, y, image::Rgba([200, 160, 140, 255])); } }
        assert!(!detector.observe(&person, t0 + Duration::from_millis(750)));
        assert!(detector.observe(&person, t0 + Duration::from_millis(1000)));
    }

    #[test]
    fn synthetic_source_decodes_into_reused_buffer() {
        let mut source = SyntheticSource::new();
        let mut buffer = Vec::new();
        let (w, h) = source.read(&mut buffer, true).expect("first frame");
        assert_eq!((w, h), (1280, 720));
        assert_eq!(buffer.len(), 1280 * 720 * 4);
        let capacity = buffer.capacity();
        let pointer = buffer.as_ptr();
        source.read(&mut buffer, true).expect("second frame");
        assert_eq!((buffer.capacity(), buffer.as_ptr()), (capacity, pointer), "no reallocation in steady state");
    }
}
