// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Microphone and speaker for the video guest book (`docs/specs/GUEST_BOOK.md` § 6.2, § 7).
//!
//! The microphone is opened only on the guest-book screens and closed when the
//! [`MicStream`] is dropped: never any background listening. Everything is
//! converted to 48 kHz mono 16-bit, the format written to the AVI file.
//!
//! The audio callback runs on a real-time thread: it computes the block peak
//! (for the level meter) and, while recording, appends samples to a
//! preallocated buffer under a `try_lock` (never blocks, never allocates).
//!
//! `BOOTHSOFT_SYNTHETIC_MIC=1` replaces the device with a generated voice-like
//! signal (tests and screenshots on machines without a microphone).

use crate::avi::SAMPLE_RATE;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Two seconds of sound waiting for the writer thread at most.
const RING: usize = SAMPLE_RATE as usize * 2;
pub const DEFAULT_DEVICE: &str = "";

pub fn synthetic() -> bool {
    std::env::var_os("BOOTHSOFT_SYNTHETIC_MIC").is_some_and(|v| v == "1")
}

/// Names of the audio inputs, for the maintenance list. Slow on some systems:
/// call it from a worker thread.
pub fn list_inputs() -> Vec<String> {
    if synthetic() { return vec![crate::tr!(fr: "Micro simulé", en: "Simulated microphone").into()]; }
    let Ok(devices) = cpal::default_host().input_devices() else { return Vec::new() };
    devices.filter_map(|d| d.name().ok()).collect()
}

struct Shared {
    /// Peak since the last read, as `f32` bits (0.0 … 1.0, gain applied).
    peak: AtomicU32,
    recording: AtomicBool,
    ring: Mutex<VecDeque<i16>>,
    gain: AtomicU32,
    /// Milliseconds since `epoch` of the last callback (dead-device detection).
    alive: AtomicU64,
    failed: AtomicBool,
    epoch: Instant,
    stop: AtomicBool,
}

impl Shared {
    fn new(gain: u8) -> Arc<Self> {
        Arc::new(Self {
            peak: AtomicU32::new(0), recording: AtomicBool::new(false),
            ring: Mutex::new(VecDeque::with_capacity(RING)),
            gain: AtomicU32::new(f32::from(gain.clamp(1, 4)).to_bits()),
            alive: AtomicU64::new(0), failed: AtomicBool::new(false), epoch: Instant::now(),
            stop: AtomicBool::new(false),
        })
    }

    /// Mono 48 kHz samples in −1…1, from the callback or the synthetic thread.
    fn push(&self, block: &[f32]) {
        let gain = f32::from_bits(self.gain.load(Ordering::Relaxed));
        let mut peak = 0f32;
        let recording = self.recording.load(Ordering::Relaxed);
        let mut ring = if recording { self.ring.try_lock().ok() } else { None };
        for &x in block {
            let y = soft_clip(x * gain);
            peak = peak.max(y.abs());
            if let Some(ring) = ring.as_mut() {
                if ring.len() == RING { ring.pop_front(); }
                ring.push_back((y * 32_767.0) as i16);
            }
        }
        let _ = self.peak.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
            Some(f32::from_bits(old).max(peak).to_bits())
        });
        self.alive.store(self.epoch.elapsed().as_millis() as u64 + 1, Ordering::Relaxed);
    }
}

/// Gentle saturation above 0.8 instead of hard clipping.
fn soft_clip(x: f32) -> f32 {
    let a = x.abs();
    if a <= 0.8 { x } else { x.signum() * (0.8 + 0.2 * ((a - 0.8) / 0.2).tanh()) }
}

/// Mixes interleaved channels to mono and resamples to 48 kHz (linear).
struct Converter { channels: usize, step: f64, t: f64, prev: f32, out: Vec<f32> }

impl Converter {
    fn new(channels: u16, rate: u32) -> Self {
        Self { channels: usize::from(channels.max(1)), step: f64::from(rate) / f64::from(SAMPLE_RATE), t: 0.0, prev: 0.0,
            out: Vec::with_capacity(16_384) }
    }

    fn feed(&mut self, data: impl Iterator<Item = f32>, shared: &Shared) {
        self.out.clear();
        let mut sum = 0f32;
        let mut n = 0;
        for sample in data {
            sum += sample;
            n += 1;
            if n < self.channels { continue; }
            let x = sum / self.channels as f32;
            sum = 0.0;
            n = 0;
            while self.t <= 1.0 {
                self.out.push(self.prev + (x - self.prev) * self.t as f32);
                self.t += self.step;
            }
            self.t -= 1.0;
            self.prev = x;
        }
        shared.push(&self.out);
    }
}

/// Handle given to the recording thread: takes the samples recorded so far.
#[derive(Clone)]
pub struct MicTap(Arc<Shared>);

impl MicTap {
    pub fn drain_into(&self, out: &mut Vec<i16>) {
        if let Ok(mut ring) = self.0.ring.lock() { out.extend(ring.drain(..)); }
    }
}

pub struct MicStream {
    shared: Arc<Shared>,
    _stream: Option<cpal::Stream>,
    pub device: String,
}

impl MicStream {
    /// `device` is a name from [`list_inputs`], or "" for the system default.
    pub fn open(device: &str, gain: u8) -> Result<Self, String> {
        let shared = Shared::new(gain);
        if synthetic() {
            let feed = Arc::clone(&shared);
            std::thread::Builder::new().name("bs-synthetic-mic".into()).spawn(move || synthetic_voice(&feed))
                .map_err(|e| e.to_string())?;
            return Ok(Self { shared, _stream: None, device: crate::tr!(fr: "Micro simulé", en: "Simulated microphone").into() });
        }
        let host = cpal::default_host();
        let chosen = if device.is_empty() { None } else {
            host.input_devices().ok().and_then(|mut all| all.find(|d| d.name().is_ok_and(|n| n == device)))
        };
        let input = chosen.or_else(|| host.default_input_device()).ok_or(crate::tr!(fr: "Aucun micro trouvé", en: "No microphone found"))?;
        let name = input.name().unwrap_or_default();
        let config = input.default_input_config().map_err(|e| e.to_string())?;
        let mut converter = Converter::new(config.channels(), config.sample_rate().0);
        let feed = Arc::clone(&shared);
        let on_error = {
            let shared = Arc::clone(&shared);
            move |error: cpal::StreamError| { crate::log::info(format!("Micro: {error}")); shared.failed.store(true, Ordering::Relaxed); }
        };
        let stream_config: cpal::StreamConfig = config.config();
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => input.build_input_stream(&stream_config,
                move |data: &[f32], _: &_| converter.feed(data.iter().copied(), &feed), on_error, None),
            cpal::SampleFormat::I16 => input.build_input_stream(&stream_config,
                move |data: &[i16], _: &_| converter.feed(data.iter().map(|&s| f32::from(s) / 32_768.0), &feed), on_error, None),
            cpal::SampleFormat::U16 => input.build_input_stream(&stream_config,
                move |data: &[u16], _: &_| converter.feed(data.iter().map(|&s| (f32::from(s) - 32_768.0) / 32_768.0), &feed), on_error, None),
            cpal::SampleFormat::I32 => input.build_input_stream(&stream_config,
                move |data: &[i32], _: &_| converter.feed(data.iter().map(|&s| s as f32 / 2_147_483_648.0), &feed), on_error, None),
            other => return Err(format!("Format audio non pris en charge : {other}")),
        }.map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        crate::log::info(format!("Micro opened: {name} ({} Hz, {} ch)", config.sample_rate().0, config.channels()));
        Ok(Self { shared, _stream: Some(stream), device: name })
    }

    /// Peak level since the previous call (0.0 … 1.0).
    pub fn take_peak(&self) -> f32 { f32::from_bits(self.shared.peak.swap(0, Ordering::Relaxed)) }

    /// No sound block for a second, or a stream error: the device is gone.
    pub fn failed(&self) -> bool {
        if self.shared.failed.load(Ordering::Relaxed) { return true; }
        let alive = self.shared.alive.load(Ordering::Relaxed);
        let now = self.shared.epoch.elapsed().as_millis() as u64;
        if alive == 0 { now > 1_500 } else { now.saturating_sub(alive) > 1_000 }
    }

    pub fn set_gain(&self, gain: u8) { self.shared.gain.store(f32::from(gain.clamp(1, 4)).to_bits(), Ordering::Relaxed); }

    /// Start keeping samples (the buffer is emptied first).
    pub fn start_recording(&self) -> MicTap {
        if let Ok(mut ring) = self.shared.ring.lock() { ring.clear(); }
        self.shared.recording.store(true, Ordering::Relaxed);
        MicTap(Arc::clone(&self.shared))
    }

    pub fn stop_recording(&self) { self.shared.recording.store(false, Ordering::Relaxed); }
}

impl Drop for MicStream {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.shared.recording.store(false, Ordering::Relaxed);
    }
}

/// Syllable-like bursts of a 180 Hz voice with harmonics, 10 ms per block.
fn synthetic_voice(shared: &Shared) {
    let mut phase = 0f32;
    let mut n: u64 = 0;
    let mut block = vec![0f32; 480];
    let start = Instant::now();
    while !shared.stop.load(Ordering::Relaxed) {
        for x in &mut block {
            let t = n as f32 / SAMPLE_RATE as f32;
            let syllable = (t * 3.1 * std::f32::consts::TAU).sin().max(0.0) * (0.6 + 0.4 * (t * 0.7).sin());
            phase += 180.0 / SAMPLE_RATE as f32;
            let tone = (phase * std::f32::consts::TAU).sin() + 0.5 * (phase * 2.0 * std::f32::consts::TAU).sin()
                + 0.25 * (phase * 3.0 * std::f32::consts::TAU).sin();
            *x = 0.18 * syllable * tone;
            n += 1;
        }
        shared.push(&block);
        let due = start + Duration::from_micros(n * 1_000_000 / u64::from(SAMPLE_RATE));
        std::thread::sleep(due.saturating_duration_since(Instant::now()));
    }
}

/// Plays a message's sound track; its position is the playback clock.
pub struct Speaker {
    _stream: cpal::Stream,
    played: Arc<AtomicU64>,
}

impl Speaker {
    /// `None` when there is no output device (the video then follows the wall clock).
    pub fn play(pcm: Arc<Vec<i16>>, volume: u8) -> Option<Self> {
        if synthetic() { return None; }
        let device = cpal::default_host().default_output_device()?;
        let config = device.default_output_config().ok()?;
        let channels = usize::from(config.channels().max(1));
        let step = f64::from(SAMPLE_RATE) / f64::from(config.sample_rate().0);
        let played = Arc::new(AtomicU64::new(0));
        let position = Arc::clone(&played);
        let gain = f32::from(volume.min(100)) / 100.0;
        let mut t = 0f64;
        let mut next = move || -> f32 {
            let i = t as usize;
            let a = pcm.get(i).copied().unwrap_or(0);
            let b = pcm.get(i + 1).copied().unwrap_or(0);
            let f = (t - i as f64) as f32;
            t += step;
            position.store(t as u64, Ordering::Relaxed);
            (f32::from(a) + (f32::from(b) - f32::from(a)) * f) / 32_768.0 * gain
        };
        let stream_config: cpal::StreamConfig = config.config();
        let on_error = |error: cpal::StreamError| crate::log::info(format!("Speaker: {error}"));
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_output_stream(&stream_config, move |out: &mut [f32], _: &_| {
                for frame in out.chunks_mut(channels) { let x = next(); frame.fill(x); }
            }, on_error, None),
            cpal::SampleFormat::I16 => device.build_output_stream(&stream_config, move |out: &mut [i16], _: &_| {
                for frame in out.chunks_mut(channels) { let x = (next() * 32_767.0) as i16; frame.fill(x); }
            }, on_error, None),
            cpal::SampleFormat::U16 => device.build_output_stream(&stream_config, move |out: &mut [u16], _: &_| {
                for frame in out.chunks_mut(channels) { let x = (next() * 32_767.0 + 32_768.0) as u16; frame.fill(x); }
            }, on_error, None),
            _ => return None,
        }.ok()?;
        stream.play().ok()?;
        Some(Self { _stream: stream, played })
    }

    /// Seconds of sound already handed to the device.
    pub fn position(&self) -> f64 { self.played.load(Ordering::Relaxed) as f64 / f64::from(SAMPLE_RATE) }

    pub fn clock(&self) -> Arc<AtomicU64> { Arc::clone(&self.played) }
}

/// Meter value 0.0 … 1.0 for a linear peak: −60 dB → 0, 0 dB → 1.
pub fn meter(peak: f32) -> f32 {
    if peak <= 0.001 { return 0.0; }
    ((20.0 * peak.log10() + 60.0) / 60.0).clamp(0.0, 1.0)
}

pub fn db(peak: f32) -> f32 { if peak <= 1e-6 { -120.0 } else { 20.0 * peak.log10() } }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converter_mixes_and_resamples() {
        let shared = Shared::new(1);
        shared.recording.store(true, Ordering::Relaxed);
        let mut converter = Converter::new(2, 44_100);
        // 0.1 s of stereo 44.1 kHz at a constant 0.25 / 0.25.
        converter.feed(std::iter::repeat_n(0.25f32, 4_410 * 2), &shared);
        let mut out = Vec::new();
        MicTap(Arc::clone(&shared)).drain_into(&mut out);
        assert!((out.len() as i32 - 4_800).abs() <= 2, "{}", out.len());
        assert!(out[100..].iter().all(|&s| (s - 8_191).abs() < 3));
        assert!((f32::from_bits(shared.peak.load(Ordering::Relaxed)) - 0.25).abs() < 1e-3);
    }

    #[test]
    fn gain_saturates_softly() {
        assert_eq!(soft_clip(0.5), 0.5);
        assert!(soft_clip(4.0) <= 1.0 && soft_clip(4.0) > 0.95);
        assert!(soft_clip(-4.0) >= -1.0);
    }

    #[test]
    fn nothing_is_kept_unless_recording() {
        let shared = Shared::new(2);
        shared.push(&[0.1; 480]);
        let mut out = Vec::new();
        MicTap(Arc::clone(&shared)).drain_into(&mut out);
        assert!(out.is_empty());
        assert!(f32::from_bits(shared.peak.load(Ordering::Relaxed)) > 0.19);
    }

    #[test]
    fn meter_scale() {
        assert_eq!(meter(0.0), 0.0);
        assert!((meter(1.0) - 1.0).abs() < 1e-6);
        assert!((meter(0.001_000_1) - 0.0).abs() < 0.01);
        assert!((meter(0.031_6) - 0.5).abs() < 0.01);
    }
}
