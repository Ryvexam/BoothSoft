// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Single-flight image encoding worker.
//!
//! Full-resolution JPEG encoding and frame compositing never run on egui's UI
//! thread. The bounded queue prevents a burst of captures from exhausting RAM.
use crate::{compositor, effects::{self, PhotoFilter}};
use image::{codecs::jpeg::JpegEncoder, ExtendedColorType, RgbImage};
use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    sync::{mpsc::{self, Receiver, SyncSender, TrySendError}, Arc},
    thread,
};

/// Longest edge of the review preview: the review screen shows the print at
/// ~80 % of the screen height, sharp up to 1080p.
pub const PREVIEW_EDGE: u32 = 1600;

pub struct PhotoCommand {
    pub session_id: String,
    pub image: RgbImage,
    pub filter: PhotoFilter,
    pub frame: Option<compositor::FrameRef>,
    pub original: PathBuf,
    pub framed: PathBuf,
}

pub struct PhotoResult {
    pub session_id: String,
    pub original: PathBuf,
    pub framed: PathBuf,
    pub preview: RgbImage,
    /// Tiny, smoothly upscaled copy used as the review screen's blurred backdrop.
    pub backdrop: RgbImage,
}

pub enum PhotoEvent {
    Saved(PhotoResult),
    Failed(String),
}

pub struct PhotoWorker {
    jobs: SyncSender<PhotoCommand>,
    pub results: Receiver<PhotoEvent>,
}

impl PhotoWorker {
    pub fn start(wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        // At most one full-resolution capture can await processing.
        let (jobs, rx) = mpsc::sync_channel::<PhotoCommand>(1);
        let (tx, results) = mpsc::channel::<PhotoEvent>();
        thread::Builder::new().name("bs-photo".into()).spawn(move || {
            while let Ok(job) = rx.recv() {
                let result = process(job).map(PhotoEvent::Saved)
                    .unwrap_or_else(|error| PhotoEvent::Failed(error.to_string()));
                if tx.send(result).is_err() { break; }
                wake();
            }
        }).expect("spawn worker thread");
        Self { jobs, results }
    }

    pub fn submit(&self, job: PhotoCommand) -> Result<(), &'static str> {
        self.jobs.try_send(job).map_err(|error| match error {
            TrySendError::Full(_) => "Photo processor is busy",
            TrySendError::Disconnected(_) => "Photo processor is unavailable",
        })
    }
}

fn process(mut job: PhotoCommand) -> io::Result<PhotoResult> {
    effects::apply_in_place(&mut job.image, job.filter);
    save_jpeg(&job.original, &job.image, 92)?;
    // No frame: reuse the original encoded JPEG, saving a second expensive
    // JPEG pass and its temporary RGB allocation. Real camera frames move into
    // the compositor, which only reallocates when dimensions must change.
    let has_frame = job.frame.is_some();
    let composed = match job.frame.as_ref() {
        Some(frame) => frame.compose(job.image).inspect_err(|_| { let _ = fs::remove_file(&job.original); })?,
        None => job.image,
    };
    let framed_write = if has_frame {
        save_jpeg(&job.framed, &composed, 94)
    } else {
        if let Some(parent) = job.framed.parent() { fs::create_dir_all(parent)?; }
        fs::copy(&job.original, &job.framed).map(|_| ())
    };
    if let Err(error) = framed_write {
        let _ = fs::remove_file(&job.original);
        return Err(error);
    }
    // The on-screen preview never needs print resolution. Downscale here, on the
    // worker, so the UI thread uploads a small texture and holds no 6 MB bitmap.
    // A "blurred" backdrop for free: average down to 32 px, then upscale to 256 px
    // with a smooth filter; the GPU stretches it to the screen.
    let backdrop = compositor::smooth_backdrop(&composed);
    let preview = compositor::fit_within(&composed, PREVIEW_EDGE).unwrap_or(composed);
    Ok(PhotoResult {
        session_id: job.session_id,
        original: job.original,
        framed: job.framed,
        preview,
        backdrop,
    })
}

fn save_jpeg(path: &Path, image: &RgbImage, quality: u8) -> io::Result<()> {
    if let Some(parent) = path.parent() { fs::create_dir_all(parent)?; }
    let tmp = path.with_extension("jpg.tmp");
    // The encoder owns the file while writing. It must be dropped before
    // replacing the destination: Windows cannot reliably rename an open file.
    let encode_result = (|| {
        let file = File::create(&tmp)?;
        let mut encoder = JpegEncoder::new_with_quality(file, quality);
        encoder.encode(
            image.as_raw(), image.width(), image.height(), ExtendedColorType::Rgb8,
        ).map_err(io::Error::other)
    })();
    if let Err(error) = encode_result {
        let _ = fs::remove_file(&tmp);
        return Err(error);
    }
    if let Err(error) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_jpeg_replace_cleans_up_temporary_file() {
        let root = std::env::temp_dir().join(format!("boothsoft-jpeg-{}-{}",
            std::process::id(), std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&root).unwrap();
        let output = root.join("photo.jpg");
        fs::create_dir(&output).unwrap();
        let image = RgbImage::from_pixel(8, 8, image::Rgb([80, 90, 100]));
        assert!(save_jpeg(&output, &image, 90).is_err());
        assert!(!output.with_extension("jpg.tmp").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_capture_produces_original_framed_and_preview_files() {
        let root = std::env::temp_dir().join(format!("boothsoft-photo-worker-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let original = root.join("original.jpg");
        let framed = root.join("framed.jpg");
        let result = process(PhotoCommand {
            session_id: "test".into(),
            image: RgbImage::from_pixel(64, 48, image::Rgb([100, 120, 130])),
            filter: PhotoFilter::Original,
            frame: None,
            original: original.clone(),
            framed: framed.clone(),
        }).unwrap();
        assert_eq!(result.preview.dimensions(), (64, 48));
        assert!(original.is_file());
        assert!(framed.is_file());
        assert!(!original.with_extension("jpg.tmp").exists());
        assert!(!framed.with_extension("jpg.tmp").exists());
        assert_eq!(fs::read(&original).unwrap(), fs::read(&framed).unwrap());
        fs::remove_dir_all(root).unwrap();
    }
}
