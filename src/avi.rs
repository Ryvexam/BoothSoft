// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Minimal AVI (RIFF) writer and reader for guest-book messages.
//!
//! Video is MJPEG: the JPEG frames the camera already produces are written
//! as they are, so recording costs a byte copy and a disk write, no encoding.
//! Sound is PCM 16-bit mono 48 kHz. Messages last 30 s at most (< 1 GB), so
//! the classic AVI 1.0 layout with an `idx1` index is enough (no OpenDML).
//!
//! The file is written to `<name>.part` and renamed by [`AviWriter::finish`].
//! A `.part` left by a crash is turned into a playable file by [`repair`].

use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub const SAMPLE_RATE: u32 = 48_000;
const VIDEO_ID: &[u8; 4] = b"00dc";
const AUDIO_ID: &[u8; 4] = b"01wb";
const AVIF_HASINDEX: u32 = 0x10;
const AVIF_ISINTERLEAVED: u32 = 0x100;
const AVIIF_KEYFRAME: u32 = 0x10;

// Byte offsets of the header fields patched by `finish`, fixed by `write_headers`.
const AVIH_USEC: u64 = 32;
const AVIH_FRAMES: u64 = 48;
const VIDEO_STRH_SCALE: u64 = 128;
const VIDEO_STRH_LENGTH: u64 = 140;

/// `.part` path used while a file is being written.
pub fn part_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".part");
    PathBuf::from(name)
}

struct Entry { id: [u8; 4], offset: u32, size: u32 }

pub struct AviWriter {
    out: BufWriter<File>,
    path: PathBuf,
    part: PathBuf,
    has_audio: bool,
    /// Offset of the `movi` fourcc (index offsets are relative to it).
    movi: u64,
    pos: u64,
    index: Vec<Entry>,
    frames: u32,
    samples: u64,
    largest: u32,
}

impl AviWriter {
    /// Start a file; it appears at `path` only once [`finish`](Self::finish) succeeds.
    pub fn create(path: &Path, width: u32, height: u32, has_audio: bool) -> io::Result<Self> {
        let part = part_path(path);
        let mut out = BufWriter::with_capacity(1 << 20, File::create(&part)?);
        let movi = write_headers(&mut out, width, height, has_audio, 30.0, 0, 0)?;
        Ok(Self { out, path: path.to_owned(), part, has_audio, movi, pos: movi + 4, index: Vec::with_capacity(2048), frames: 0, samples: 0, largest: 0 })
    }

    pub fn has_audio(&self) -> bool { self.has_audio }
    pub fn frames(&self) -> u32 { self.frames }

    fn chunk(&mut self, id: &[u8; 4], data: &[u8]) -> io::Result<()> {
        let size = u32::try_from(data.len()).map_err(|_| io::Error::other("chunk too large"))?;
        self.index.push(Entry { id: *id, offset: (self.pos - self.movi) as u32, size });
        self.out.write_all(id)?;
        self.out.write_all(&size.to_le_bytes())?;
        self.out.write_all(data)?;
        if size % 2 == 1 { self.out.write_all(&[0])?; }
        self.pos += 8 + u64::from(size) + u64::from(size % 2);
        self.largest = self.largest.max(size);
        Ok(())
    }

    /// One complete JPEG image.
    pub fn write_video(&mut self, jpeg: &[u8]) -> io::Result<()> {
        self.chunk(VIDEO_ID, jpeg)?;
        self.frames += 1;
        Ok(())
    }

    /// Mono 48 kHz samples. Ignored when the file was created without sound.
    pub fn write_audio(&mut self, pcm: &[i16]) -> io::Result<()> {
        if !self.has_audio || pcm.is_empty() { return Ok(()); }
        let mut bytes = Vec::with_capacity(pcm.len() * 2);
        for sample in pcm { bytes.extend_from_slice(&sample.to_le_bytes()); }
        self.chunk(AUDIO_ID, &bytes)?;
        self.samples += pcm.len() as u64;
        Ok(())
    }

    /// Write the index, fix the headers with the measured frame rate, rename.
    pub fn finish(mut self, fps: f64) -> io::Result<PathBuf> {
        self.out.write_all(b"idx1")?;
        self.out.write_all(&((self.index.len() * 16) as u32).to_le_bytes())?;
        for entry in &self.index {
            self.out.write_all(&entry.id)?;
            self.out.write_all(&AVIIF_KEYFRAME.to_le_bytes())?;
            self.out.write_all(&entry.offset.to_le_bytes())?;
            self.out.write_all(&entry.size.to_le_bytes())?;
        }
        let end = self.pos + 8 + self.index.len() as u64 * 16;
        let mut file = self.out.into_inner().map_err(|e| e.into_error())?;
        patch_headers(&mut file, end, self.movi, self.pos, fps, self.frames, self.samples, self.largest, self.has_audio)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&self.part, &self.path)?;
        Ok(self.path)
    }

    /// Stop and delete the partial file (message abandoned).
    pub fn abandon(self) {
        let part = self.part.clone();
        drop(self.out);
        let _ = fs::remove_file(part);
    }
}

fn u32le(out: &mut impl Write, value: u32) -> io::Result<()> { out.write_all(&value.to_le_bytes()) }
fn u16le(out: &mut impl Write, value: u16) -> io::Result<()> { out.write_all(&value.to_le_bytes()) }

/// Writes everything up to and including `LIST….movi`; returns the offset of `movi`.
fn write_headers(out: &mut impl Write, width: u32, height: u32, has_audio: bool, fps: f64, frames: u32, samples: u64) -> io::Result<u64> {
    let (scale, rate) = rate_of(fps);
    let strl_video = 4 + (8 + 56) + (8 + 40);
    let strl_audio = 4 + (8 + 56) + (8 + 18);
    let hdrl = 4 + (8 + 56) + (8 + strl_video) + if has_audio { 8 + strl_audio } else { 0 };
    out.write_all(b"RIFF")?; u32le(out, 0)?; out.write_all(b"AVI ")?;
    out.write_all(b"LIST")?; u32le(out, hdrl)?; out.write_all(b"hdrl")?;
    // avih (offset 32 = data start)
    out.write_all(b"avih")?; u32le(out, 56)?;
    u32le(out, (1_000_000.0 / fps).round() as u32)?;
    u32le(out, 0)?; u32le(out, 0)?;
    u32le(out, AVIF_HASINDEX | AVIF_ISINTERLEAVED)?;
    u32le(out, frames)?; u32le(out, 0)?;
    u32le(out, if has_audio { 2 } else { 1 })?;
    u32le(out, 1 << 20)?;
    u32le(out, width)?; u32le(out, height)?;
    for _ in 0..4 { u32le(out, 0)?; }
    // Video stream (strh data starts at 108)
    out.write_all(b"LIST")?; u32le(out, strl_video)?; out.write_all(b"strl")?;
    out.write_all(b"strh")?; u32le(out, 56)?;
    out.write_all(b"vids")?; out.write_all(b"MJPG")?;
    u32le(out, 0)?; u16le(out, 0)?; u16le(out, 0)?; u32le(out, 0)?;
    u32le(out, scale)?; u32le(out, rate)?; u32le(out, 0)?; u32le(out, frames)?;
    u32le(out, 1 << 20)?; u32le(out, u32::MAX)?; u32le(out, 0)?;
    u16le(out, 0)?; u16le(out, 0)?; u16le(out, width as u16)?; u16le(out, height as u16)?;
    out.write_all(b"strf")?; u32le(out, 40)?;
    u32le(out, 40)?; u32le(out, width)?; u32le(out, height)?; u16le(out, 1)?; u16le(out, 24)?;
    out.write_all(b"MJPG")?; u32le(out, width * height * 3)?;
    for _ in 0..4 { u32le(out, 0)?; }
    if has_audio {
        out.write_all(b"LIST")?; u32le(out, strl_audio)?; out.write_all(b"strl")?;
        out.write_all(b"strh")?; u32le(out, 56)?;
        out.write_all(b"auds")?; u32le(out, 0)?;
        u32le(out, 0)?; u16le(out, 0)?; u16le(out, 0)?; u32le(out, 0)?;
        u32le(out, 1)?; u32le(out, SAMPLE_RATE)?; u32le(out, 0)?; u32le(out, samples as u32)?;
        u32le(out, SAMPLE_RATE)?; u32le(out, u32::MAX)?; u32le(out, 2)?;
        for _ in 0..4 { u16le(out, 0)?; }
        out.write_all(b"strf")?; u32le(out, 18)?;
        u16le(out, 1)?; u16le(out, 1)?; u32le(out, SAMPLE_RATE)?; u32le(out, SAMPLE_RATE * 2)?;
        u16le(out, 2)?; u16le(out, 16)?; u16le(out, 0)?;
    }
    out.write_all(b"LIST")?; u32le(out, 0)?;
    let movi = 12 + 8 + u64::from(hdrl) + 8;
    out.write_all(b"movi")?;
    Ok(movi)
}

fn rate_of(fps: f64) -> (u32, u32) {
    let fps = if fps.is_finite() && fps > 0.5 { fps.min(120.0) } else { 30.0 };
    (1000, (fps * 1000.0).round() as u32)
}

#[allow(clippy::too_many_arguments)]
fn patch_headers(file: &mut File, end: u64, movi: u64, movi_end: u64, fps: f64, frames: u32, samples: u64, largest: u32, has_audio: bool) -> io::Result<()> {
    let at = |file: &mut File, offset: u64, value: u32| -> io::Result<()> {
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(&value.to_le_bytes())
    };
    let (scale, rate) = rate_of(fps);
    at(file, 4, (end - 8) as u32)?;
    at(file, AVIH_USEC, (1_000_000.0 * f64::from(scale) / f64::from(rate)).round() as u32)?;
    at(file, AVIH_FRAMES, frames)?;
    at(file, AVIH_USEC + 28, largest + 8)?;
    at(file, VIDEO_STRH_SCALE, scale)?;
    at(file, VIDEO_STRH_SCALE + 4, rate)?;
    at(file, VIDEO_STRH_LENGTH, frames)?;
    at(file, VIDEO_STRH_LENGTH + 4, largest)?;
    if has_audio {
        // Audio strh data: after the video strl (12 + 56 + 8 + 40 + 8) → 108 + 112 + 12 + 8.
        let audio_strh = 108 + 56 + 48 + 12 + 8;
        at(file, audio_strh + 32, samples as u32)?;
    }
    at(file, movi - 4, (movi_end - movi) as u32)?;
    file.seek(SeekFrom::End(0))?;
    Ok(())
}

/// A finished message, opened for playback.
pub struct AviFile {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    /// (offset, size) of every JPEG frame in the file.
    pub frames: Vec<(u64, u32)>,
    audio: Vec<(u64, u32)>,
    pub has_audio: bool,
    file: File,
}

impl AviFile {
    pub fn open(path: &Path) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let len = file.metadata()?.len();
        let mut head = [0u8; 12];
        file.read_exact(&mut head)?;
        if &head[0..4] != b"RIFF" || &head[8..12] != b"AVI " { return Err(io::Error::other("not an AVI file")); }
        let (mut width, mut height, mut usec, mut has_audio) = (0, 0, 33_333u32, false);
        let mut movi = None;
        let mut idx1 = None;
        let mut pos = 12u64;
        while pos + 8 <= len {
            file.seek(SeekFrom::Start(pos))?;
            let mut h = [0u8; 12];
            file.read_exact(&mut h[..8])?;
            let size = u64::from(u32::from_le_bytes(h[4..8].try_into().unwrap()));
            match &h[0..4] {
                b"LIST" => {
                    file.read_exact(&mut h[8..12])?;
                    match &h[8..12] {
                        b"hdrl" => {
                            let mut data = vec![0u8; size.saturating_sub(4).min(64 * 1024) as usize];
                            file.read_exact(&mut data)?;
                            if let Some(i) = find(&data, b"avih") {
                                let d = &data[i + 8..];
                                usec = rd(d, 0);
                                width = rd(d, 32);
                                height = rd(d, 36);
                            }
                            has_audio = find(&data, b"auds").is_some();
                        }
                        b"movi" => movi = Some((pos + 8, size)),
                        _ => {}
                    }
                }
                b"idx1" => idx1 = Some((pos + 8, size)),
                _ => {}
            }
            pos += 8 + size + size % 2;
        }
        let (movi, movi_size) = movi.ok_or_else(|| io::Error::other("no movi list"))?;
        let mut entries = Vec::new();
        if let Some((start, size)) = idx1 {
            file.seek(SeekFrom::Start(start))?;
            let mut data = vec![0u8; size as usize];
            file.read_exact(&mut data)?;
            for e in data.chunks_exact(16) {
                entries.push(([e[0], e[1], e[2], e[3]], movi + u64::from(rd(e, 8)) + 8, rd(e, 12)));
            }
        } else {
            entries = scan_movi(&mut file, movi, movi_size)?;
        }
        let frames = entries.iter().filter(|e| &e.0 == VIDEO_ID).map(|e| (e.1, e.2)).collect();
        let fps = if usec > 0 { 1_000_000.0 / f64::from(usec) } else { 30.0 };
        let audio = entries.iter().filter(|e| &e.0 == AUDIO_ID).map(|e| (e.1, e.2)).collect();
        Ok(Self { width, height, fps, frames, audio, has_audio, file })
    }

    pub fn duration(&self) -> f64 { self.frames.len() as f64 / self.fps }

    /// JPEG bytes of frame `n`, into a reused buffer.
    pub fn read_frame(&mut self, n: usize, buffer: &mut Vec<u8>) -> io::Result<()> {
        let (offset, size) = *self.frames.get(n).ok_or_else(|| io::Error::other("no such frame"))?;
        buffer.resize(size as usize, 0);
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(buffer)
    }

    /// The whole sound track (30 s ≈ 2.9 MB).
    pub fn read_audio(&mut self) -> io::Result<Vec<i16>> {
        let mut pcm = Vec::new();
        let mut bytes = Vec::new();
        for (offset, size) in self.audio.clone() {
            bytes.resize(size as usize, 0);
            self.file.seek(SeekFrom::Start(offset))?;
            self.file.read_exact(&mut bytes)?;
            pcm.extend(bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])));
        }
        Ok(pcm)
    }
}

fn rd(data: &[u8], at: usize) -> u32 {
    data.get(at..at + 4).map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()))
}

fn find(data: &[u8], needle: &[u8; 4]) -> Option<usize> {
    data.windows(4).position(|w| w == needle)
}

/// Chunks of a `movi` list without an index (crash before `finish`).
fn scan_movi(file: &mut File, movi: u64, size: u64) -> io::Result<Vec<([u8; 4], u64, u32)>> {
    let len = file.metadata()?.len();
    let end = if size > 4 { (movi + size).min(len) } else { len };
    let mut entries = Vec::new();
    let mut pos = movi + 4;
    while pos + 8 <= end {
        file.seek(SeekFrom::Start(pos))?;
        let mut h = [0u8; 8];
        file.read_exact(&mut h)?;
        let id = [h[0], h[1], h[2], h[3]];
        let chunk = u32::from_le_bytes(h[4..8].try_into().unwrap());
        if (&id != VIDEO_ID && &id != AUDIO_ID) || pos + 8 + u64::from(chunk) > end { break; }
        entries.push((id, pos + 8, chunk));
        pos += 8 + u64::from(chunk) + u64::from(chunk % 2);
    }
    Ok(entries)
}

/// Turn a `.part` left by a crash into a playable `.avi` (index rebuilt,
/// headers fixed). Returns the final path, or an error when nothing usable
/// was written (the caller deletes the `.part`).
pub fn repair(part: &Path) -> io::Result<PathBuf> {
    let name = part.to_string_lossy();
    let path = PathBuf::from(name.strip_suffix(".part").ok_or_else(|| io::Error::other("not a .part file"))?);
    let mut file = fs::OpenOptions::new().read(true).write(true).open(part)?;
    let mut data = vec![0u8; 4096.min(file.metadata()?.len() as usize)];
    file.read_exact(&mut data)?;
    let movi = find(&data, b"movi").ok_or_else(|| io::Error::other("no movi list"))? as u64;
    let has_audio = find(&data, b"auds").is_some();
    let entries = scan_movi(&mut file, movi, 0)?;
    let frames = entries.iter().filter(|e| &e.0 == VIDEO_ID).count() as u32;
    if frames == 0 { return Err(io::Error::other("no frame recorded")); }
    let samples: u64 = entries.iter().filter(|e| &e.0 == AUDIO_ID).map(|e| u64::from(e.2 / 2)).sum();
    let largest = entries.iter().map(|e| e.2).max().unwrap_or(0);
    let last = entries.last().map_or(movi + 4, |e| e.1 + u64::from(e.2) + u64::from(e.2 % 2));
    file.set_len(last)?;
    file.seek(SeekFrom::Start(last))?;
    let mut out = BufWriter::new(&mut file);
    out.write_all(b"idx1")?;
    out.write_all(&((entries.len() * 16) as u32).to_le_bytes())?;
    for (id, offset, size) in &entries {
        out.write_all(id)?;
        out.write_all(&AVIIF_KEYFRAME.to_le_bytes())?;
        out.write_all(&((offset - 8 - movi) as u32).to_le_bytes())?;
        out.write_all(&size.to_le_bytes())?;
    }
    out.flush()?;
    drop(out);
    // Frame rate: from the sound when there is some, else the 30 fps default.
    let fps = if samples > 0 { f64::from(frames) * f64::from(SAMPLE_RATE) / samples as f64 } else { 30.0 };
    let end = last + 8 + entries.len() as u64 * 16;
    patch_headers(&mut file, end, movi, last, fps, frames, samples, largest, has_audio)?;
    file.sync_all()?;
    drop(file);
    fs::rename(part, &path)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jpeg(shade: u8) -> Vec<u8> {
        let image = image::RgbImage::from_pixel(64, 48, image::Rgb([shade, 100, 200]));
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80)
            .encode(image.as_raw(), 64, 48, image::ExtendedColorType::Rgb8).unwrap();
        out
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("boothsoft-avi-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.join("message.avi")
    }

    fn record(path: &Path, frames: u32, audio: bool) -> AviWriter {
        let mut writer = AviWriter::create(path, 64, 48, audio).unwrap();
        for i in 0..frames {
            writer.write_video(&jpeg((i * 8) as u8)).unwrap();
            writer.write_audio(&vec![(i as i16) * 100; 1600]).unwrap();
        }
        writer
    }

    #[test]
    fn written_file_reads_back() {
        let path = temp("roundtrip");
        let written = record(&path, 60, true).finish(30.0).unwrap();
        assert_eq!(written, path);
        assert!(!part_path(&path).exists());
        let mut avi = AviFile::open(&path).unwrap();
        assert_eq!((avi.width, avi.height, avi.frames.len()), (64, 48, 60));
        assert!(avi.has_audio && (avi.fps - 30.0).abs() < 0.01 && (avi.duration() - 2.0).abs() < 0.01);
        let mut buffer = Vec::new();
        avi.read_frame(59, &mut buffer).unwrap();
        assert_eq!(buffer, jpeg((59 * 8) as u8));
        let pcm = avi.read_audio().unwrap();
        assert_eq!(pcm.len(), 60 * 1600);
        assert_eq!(pcm[1600 * 3], 300);
    }

    #[test]
    fn ffprobe_sees_both_streams() {
        let Ok(probe) = std::process::Command::new("ffprobe").arg("-version").output() else { return };
        if !probe.status.success() { return; }
        let path = temp("ffprobe");
        record(&path, 45, true).finish(15.0).unwrap();
        let out = std::process::Command::new("ffprobe")
            .args(["-v", "error", "-show_entries", "stream=codec_name,r_frame_rate,sample_rate,channels", "-show_entries", "format=duration", "-of", "csv=p=0"])
            .arg(&path).output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        assert!(text.contains("mjpeg") && text.contains("15/1"), "{text}");
        assert!(text.contains("pcm_s16le") && text.contains("48000"), "{text}");
        let duration: f64 = text.lines().last().unwrap().trim().parse().unwrap();
        assert!((duration - 3.0).abs() < 0.1, "{text}");
    }

    #[test]
    fn video_only_file() {
        let path = temp("silent");
        record(&path, 10, false).finish(30.0).unwrap();
        let mut avi = AviFile::open(&path).unwrap();
        assert!(!avi.has_audio);
        assert_eq!(avi.frames.len(), 10);
        assert!(avi.read_audio().unwrap().is_empty());
    }

    #[test]
    fn crashed_recording_is_repaired() {
        let path = temp("repair");
        let mut writer = record(&path, 40, true);
        // Simulated crash: buffers flushed, no index, headers never fixed.
        writer.out.flush().unwrap();
        std::mem::forget(writer);
        let part = part_path(&path);
        // A torn last chunk, as after a power cut.
        let mut file = fs::OpenOptions::new().append(true).open(&part).unwrap();
        file.write_all(b"00dc\xff\x00\x00\x00abc").unwrap();
        drop(file);
        let repaired = repair(&part).unwrap();
        assert_eq!(repaired, path);
        let mut avi = AviFile::open(&path).unwrap();
        assert_eq!(avi.frames.len(), 40);
        assert!((avi.fps - 30.0).abs() < 0.1);
        let mut buffer = Vec::new();
        avi.read_frame(39, &mut buffer).unwrap();
        assert_eq!(buffer, jpeg((39 * 8) as u8));
    }

    #[test]
    fn empty_part_is_rejected() {
        let path = temp("empty");
        let writer = AviWriter::create(&path, 64, 48, true).unwrap();
        let part = part_path(&path);
        drop(writer);
        assert!(repair(&part).is_err());
    }
}
