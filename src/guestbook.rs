// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Video guest book: recording thread (`bs-record`), playback thread
//! (`bs-playback`) and the message files. See `docs/specs/GUEST_BOOK.md`.
//!
//! Files of one message, in `sessions/<event folder>/livre-d-or/`:
//! `message-YYYYMMDD-HHMMSS.avi`, `…-apercu.jpg` (first frame), `….json`.

use crate::audio::{MicTap, Speaker};
use crate::avi::{self, AviFile, AviWriter};
use crate::camera::{RecordFrame, WakeUi};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub const FOLDER: &str = "livre-d-or";
/// Shorter messages are thrown away (a tap by mistake).
pub const MIN_SECONDS: f64 = 2.0;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MessageInfo {
    pub recorded_at: String,
    pub seconds: f64,
    pub frames: u32,
    #[serde(default)]
    pub skipped_frames: u32,
    #[serde(default)]
    pub microphone: String,
    #[serde(default)]
    pub has_audio: bool,
}

/// A message on disk.
#[derive(Debug, Clone)]
pub struct Message {
    pub video: PathBuf,
    pub info: MessageInfo,
}

impl Message {
    pub fn thumbnail(&self) -> PathBuf { sibling(&self.video, "-apercu.jpg") }
    pub fn metadata(&self) -> PathBuf { sibling(&self.video, ".json") }
    pub fn stem(&self) -> String { self.video.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default() }

    /// Removes the video, its thumbnail and its metadata.
    pub fn delete(&self) -> std::io::Result<()> {
        fs::remove_file(&self.video)?;
        let _ = fs::remove_file(self.thumbnail());
        let _ = fs::remove_file(self.metadata());
        Ok(())
    }
}

fn sibling(video: &Path, suffix: &str) -> PathBuf {
    let stem = video.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    video.with_file_name(format!("{stem}{suffix}"))
}

pub fn folder(event_dir: &Path) -> PathBuf { event_dir.join(FOLDER) }

/// Messages of one event folder, oldest first.
pub fn list(event_dir: &Path) -> Vec<Message> {
    let Ok(entries) = fs::read_dir(folder(event_dir)) else { return Vec::new() };
    let mut messages: Vec<Message> = entries.filter_map(Result::ok).map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "avi"))
        .map(|video| {
            let info = fs::read(sibling(&video, ".json")).ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .or_else(|| AviFile::open(&video).ok().map(|a| MessageInfo {
                    seconds: a.duration(), frames: a.frames.len() as u32, has_audio: a.has_audio, ..Default::default() }))
                .unwrap_or_default();
            Message { video, info }
        }).collect();
    messages.sort_by(|a, b| a.video.cmp(&b.video));
    messages
}

/// Every event folder that holds messages (for the maintenance page).
pub fn event_folders(sessions_root: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(sessions_root) else { return Vec::new() };
    let mut names: Vec<String> = entries.filter_map(Result::ok)
        .filter(|e| e.path().join(FOLDER).is_dir())
        .filter_map(|e| e.file_name().to_str().map(str::to_owned)).collect();
    names.sort();
    names
}

/// Startup: turn `.part` files left by a crash into playable messages.
pub fn repair_all(sessions_root: &Path) {
    for event in event_folders(sessions_root) {
        let dir = folder(&sessions_root.join(&event));
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for part in entries.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "part")) {
            match avi::repair(&part) {
                Ok(video) => {
                    crate::log::info(format!("Guest book: repaired {}", video.display()));
                    if let Ok(avi) = AviFile::open(&video) {
                        let info = MessageInfo { recorded_at: String::new(), seconds: avi.duration(), frames: avi.frames.len() as u32,
                            skipped_frames: 0, microphone: String::new(), has_audio: avi.has_audio };
                        let _ = fs::write(sibling(&video, ".json"), serde_json::to_vec_pretty(&info).unwrap_or_default());
                    }
                }
                Err(error) => {
                    crate::log::info(format!("Guest book: unusable {} ({error}), deleted", part.display()));
                    let _ = fs::remove_file(&part);
                }
            }
        }
    }
}

pub enum RecordResult {
    Saved(Message),
    /// Too short: nothing kept.
    TooShort,
    Failed(String),
}

/// Recording in progress. Dropping the camera's sender (or calling
/// [`Recording::stop`]) ends it; the result arrives on [`Recording::done`].
pub struct Recording {
    pub started: Instant,
    pub done: Receiver<RecordResult>,
    stop: Arc<AtomicBool>,
}

impl Recording {
    /// Returns the recording and the sender to give to the camera
    /// (`CameraCommand::Record(Some(sender))`).
    pub fn start(dir: &Path, mic: Option<(MicTap, String)>, wake: WakeUi) -> std::io::Result<(Self, SyncSender<RecordFrame>)> {
        fs::create_dir_all(dir)?;
        let now = chrono::Local::now();
        let mut video = dir.join(format!("message-{}.avi", now.format("%Y%m%d-%H%M%S")));
        let mut n = 2;
        while video.exists() { video = dir.join(format!("message-{}-{n}.avi", now.format("%Y%m%d-%H%M%S"))); n += 1; }
        // One second of frames at most waits for the disk.
        let (frames_tx, frames) = mpsc::sync_channel::<RecordFrame>(30);
        let (done_tx, done) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let recorded_at = now.to_rfc3339();
        thread::Builder::new().name("bs-record".into()).spawn(move || {
            let result = write_message(&video, &frames, mic, &stopping, recorded_at);
            let _ = done_tx.send(result);
            wake();
        })?;
        Ok((Self { started: Instant::now(), done, stop }, frames_tx))
    }

    pub fn stop(&self) { self.stop.store(true, Ordering::Relaxed); }
}

fn write_message(video: &Path, frames: &Receiver<RecordFrame>, mic: Option<(MicTap, String)>, stop: &AtomicBool, recorded_at: String) -> RecordResult {
    let mut writer: Option<AviWriter> = None;
    let mut audio = Vec::with_capacity(48_000);
    let (mut first, mut last) = (None::<Instant>, None::<Instant>);
    let microphone = mic.as_ref().map(|m| m.1.clone()).unwrap_or_default();
    let fail = |writer: Option<AviWriter>, error: std::io::Error| {
        if let Some(writer) = writer { writer.abandon(); }
        crate::log::info(format!("Guest book: write failed: {error}"));
        RecordResult::Failed(error.to_string())
    };
    loop {
        let frame = match frames.recv_timeout(Duration::from_millis(100)) {
            Ok(frame) => Some(frame),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        if let Some(frame) = frame {
            if writer.is_none() {
                match AviWriter::create(video, frame.width, frame.height, mic.is_some()) {
                    Ok(created) => writer = Some(created),
                    Err(error) => return fail(None, error),
                }
                if let Err(error) = fs::write(sibling(video, "-apercu.jpg"), &frame.jpeg) { return fail(writer, error); }
            }
            first.get_or_insert(frame.at);
            last = Some(frame.at);
            if let Err(error) = writer.as_mut().expect("created above").write_video(&frame.jpeg) { return fail(writer, error); }
        }
        if let (Some(w), Some(tap)) = (writer.as_mut(), mic.as_ref()) {
            audio.clear();
            tap.0.drain_into(&mut audio);
            if let Err(error) = w.write_audio(&audio) { return fail(writer, error); }
        }
        if stop.load(Ordering::Relaxed) { break; }
    }
    let Some(mut writer) = writer else { return RecordResult::Failed("No frame received from the camera".into()) };
    if let Some(tap) = mic.as_ref() {
        audio.clear();
        tap.0.drain_into(&mut audio);
        let _ = writer.write_audio(&audio);
    }
    let frames_written = writer.frames();
    let span = match (first, last) { (Some(a), Some(b)) => b.duration_since(a).as_secs_f64(), _ => 0.0 };
    let fps = if frames_written > 1 && span > 0.0 { f64::from(frames_written - 1) / span } else { 30.0 };
    let seconds = f64::from(frames_written) / fps;
    if seconds < MIN_SECONDS {
        writer.abandon();
        let _ = fs::remove_file(sibling(video, "-apercu.jpg"));
        return RecordResult::TooShort;
    }
    let has_audio = writer.has_audio();
    match writer.finish(fps) {
        Ok(path) => {
            let info = MessageInfo { recorded_at, seconds, frames: frames_written, skipped_frames: 0, microphone, has_audio };
            let _ = fs::write(sibling(&path, ".json"), serde_json::to_vec_pretty(&info).unwrap_or_default());
            crate::log::info(format!("Guest book: saved {} ({seconds:.1} s, {fps:.1} fps, sound: {has_audio})", path.display()));
            RecordResult::Saved(Message { video: path, info })
        }
        Err(error) => fail(None, error),
    }
}

/// Plays a message: frames decoded on `bs-playback`, paced by the sound.
pub struct Player {
    slot: Arc<Mutex<Option<RgbaImage>>>,
    stop: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
    _speaker: Option<Speaker>,
    pub duration: f64,
    started: Instant,
}

impl Player {
    pub fn start(path: &Path, volume: u8, wake: WakeUi) -> std::io::Result<Self> {
        let mut avi = AviFile::open(path)?;
        let duration = avi.duration();
        let pcm = Arc::new(avi.read_audio()?);
        let speaker = if pcm.is_empty() { None } else { Speaker::play(pcm, volume) };
        let clock = speaker.as_ref().map(Speaker::clock);
        let slot = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));
        let (out, halt, end) = (Arc::clone(&slot), Arc::clone(&stop), Arc::clone(&finished));
        thread::Builder::new().name("bs-playback".into()).spawn(move || {
            play(&mut avi, clock, &out, &halt, &wake);
            end.store(true, Ordering::Relaxed);
            wake();
        })?;
        Ok(Self { slot, stop, finished, _speaker: speaker, duration, started: Instant::now() })
    }

    pub fn take_frame(&self) -> Option<RgbaImage> { self.slot.lock().ok()?.take() }
    pub fn finished(&self) -> bool { self.finished.load(Ordering::Relaxed) }
    pub fn elapsed(&self) -> f64 { self.started.elapsed().as_secs_f64().min(self.duration) }
}

impl Drop for Player {
    fn drop(&mut self) { self.stop.store(true, Ordering::Relaxed); }
}

fn play(avi: &mut AviFile, clock: Option<Arc<AtomicU64>>, out: &Mutex<Option<RgbaImage>>, stop: &AtomicBool, wake: &WakeUi) {
    let start = Instant::now();
    let mut jpeg = Vec::new();
    let mut spare: Vec<u8> = Vec::new();
    let fps = avi.fps.max(1.0);
    let total = avi.frames.len();
    let now = |clock: &Option<Arc<AtomicU64>>| -> f64 {
        let wall = start.elapsed().as_secs_f64();
        // The sound leads; if the device stalls, the wall clock takes over.
        clock.as_ref().map_or(wall, |c| (c.load(Ordering::Relaxed) as f64 / f64::from(avi::SAMPLE_RATE)).max(wall - 0.25))
    };
    let mut n = 0;
    while n < total && !stop.load(Ordering::Relaxed) {
        let due = n as f64 / fps;
        let t = now(&clock);
        if t < due {
            thread::sleep(Duration::from_secs_f64((due - t).min(0.010)));
            continue;
        }
        // Late by more than a frame: skip ahead instead of decoding every frame.
        let target = ((t * fps) as usize).min(total - 1).max(n);
        if avi.read_frame(target, &mut jpeg).is_ok() {
            if let Ok((w, h)) = crate::mjpeg::decode(&jpeg, &mut spare, true, crate::mjpeg::Quality::Preview) {
                if let Some(frame) = RgbaImage::from_raw(w, h, std::mem::take(&mut spare)) {
                    if let Ok(mut slot) = out.lock() {
                        if let Some(old) = slot.replace(frame) { spare = old.into_raw(); }
                    }
                    wake();
                }
            }
        }
        n = target + 1;
    }
    // Let the last words finish before showing "Revoir".
    while !stop.load(Ordering::Relaxed) && now(&clock) < total as f64 / fps {
        thread::sleep(Duration::from_millis(20));
    }
}

/// Copies an event's messages to `target/<event>-livre-d-or/` with a READ-ME.
/// Returns the number of messages copied.
pub fn export(event_dir: &Path, target: &Path, progress: &dyn Fn(usize, usize)) -> std::io::Result<usize> {
    let messages = list(event_dir);
    let name = event_dir.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "evenement".into());
    let out = target.join(crate::tr!(fr: format!("{name}-livre-d-or"), en: format!("{name}-guest-book")));
    fs::create_dir_all(&out)?;
    let (file, text) = crate::tr!(
        fr: ("LISEZ-MOI.txt", "Messages vidéo du livre d'or.\r\n\r\nOuvrez les fichiers .avi avec VLC (gratuit : https://www.videolan.org/) \
            ou le lecteur vidéo de votre ordinateur.\r\nChaque fichier est un message, nommé avec la date et l'heure de l'enregistrement.\r\n"),
        en: ("README.txt", "Guest book video messages.\r\n\r\nOpen the .avi files with VLC (free: https://www.videolan.org/) \
            or your computer's video player.\r\nEach file is one message, named after the date and time it was recorded.\r\n"));
    fs::write(out.join(file), text)?;
    for (i, message) in messages.iter().enumerate() {
        progress(i, messages.len());
        let file = message.video.file_name().unwrap_or_default();
        fs::copy(&message.video, out.join(file))?;
    }
    progress(messages.len(), messages.len());
    Ok(messages.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jpeg() -> Vec<u8> {
        let image = image::RgbImage::from_pixel(32, 24, image::Rgb([200, 120, 40]));
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80)
            .encode(image.as_raw(), 32, 24, image::ExtendedColorType::Rgb8).unwrap();
        out
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("boothsoft-guestbook-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn feed(frames: SyncSender<RecordFrame>, count: u32) {
        let start = Instant::now();
        for i in 0..count {
            frames.send(RecordFrame { jpeg: jpeg(), width: 32, height: 24, at: start + Duration::from_millis(u64::from(i) * 40) }).unwrap();
        }
    }

    #[test]
    fn records_lists_exports_and_deletes() {
        let root = temp("flow");
        let event = root.join("sessions").join("mariage");
        let (recording, frames) = Recording::start(&folder(&event), None, Arc::new(|| {})).unwrap();
        feed(frames, 75); // 3 s at 25 fps; dropping the sender ends the recording
        let RecordResult::Saved(message) = recording.done.recv().unwrap() else { panic!("not saved") };
        assert!((message.info.seconds - 3.0).abs() < 0.05, "{}", message.info.seconds);
        assert!(message.thumbnail().exists() && message.metadata().exists());
        let listed = list(&event);
        assert_eq!(listed.len(), 1);
        assert_eq!(event_folders(&root.join("sessions")), vec!["mariage".to_owned()]);
        let usb = root.join("usb");
        assert_eq!(export(&event, &usb, &|_, _| {}).unwrap(), 1);
        assert!(usb.join("mariage-livre-d-or").join("LISEZ-MOI.txt").exists() || usb.join("mariage-guest-book").join("README.txt").exists());
        listed[0].delete().unwrap();
        assert!(list(&event).is_empty() && !message.thumbnail().exists());
    }

    #[test]
    fn too_short_message_is_dropped() {
        let root = temp("short");
        let (recording, frames) = Recording::start(&root, None, Arc::new(|| {})).unwrap();
        feed(frames, 20); // 0.8 s
        assert!(matches!(recording.done.recv().unwrap(), RecordResult::TooShort));
        assert!(fs::read_dir(&root).unwrap().next().is_none());
    }

    #[test]
    fn playback_reaches_the_end() {
        let root = temp("play");
        let (recording, frames) = Recording::start(&root, None, Arc::new(|| {})).unwrap();
        feed(frames, 60);
        let RecordResult::Saved(message) = recording.done.recv().unwrap() else { panic!() };
        let player = Player::start(&message.video, 80, Arc::new(|| {})).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut shown = 0;
        while !player.finished() && Instant::now() < deadline {
            if player.take_frame().is_some() { shown += 1; }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(player.finished());
        assert!(shown > 10, "{shown}");
    }
}
