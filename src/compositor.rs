// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Deterministic photo compositing. The displayed preview and exported JPEG share this pipeline.
use fast_image_resize::{images::{Image as ResizeImage, ImageRef}, FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use image::{ImageFormat, ImageReader, Limits, RgbImage, RgbaImage};
use std::{io, path::Path};

/// Center-crop to fill the canvas, matching the kiosk's cover-style live preview.
pub fn cover(photo: &RgbImage, width: u32, height: u32) -> RgbImage {
    if width == 0 || height == 0 || photo.width() == 0 || photo.height() == 0 {
        return RgbImage::new(width, height);
    }
    if photo.width() == width && photo.height() == height {
        return photo.clone();
    }
    // Crop in source coordinates before resizing. Resizing the entire input
    // first can create enormous temporary buffers for extreme aspect ratios
    // (e.g. a portrait input placed into a landscape print frame).
    let source_width = u64::from(photo.width());
    let source_height = u64::from(photo.height());
    let target_width = u64::from(width);
    let target_height = u64::from(height);
    let (crop_width, crop_height) = if source_width * target_height > source_height * target_width {
        ((source_height * target_width / target_height) as u32, photo.height())
    } else {
        (photo.width(), (source_width * target_height / target_width) as u32)
    };
    let crop_width = crop_width.clamp(1, photo.width());
    let crop_height = crop_height.clamp(1, photo.height());
    let x = (photo.width() - crop_width) / 2;
    let y = (photo.height() - crop_height) / 2;
    resize_rgb(photo, Some((x, y, crop_width, crop_height)), width, height, FilterType::Lanczos3)
}

/// SIMD u8 resampling (NEON / SSE4 / AVX2). Unlike `image::imageops::resize`,
/// no full-size f32 intermediate buffer is allocated: a 1280x960 -> 1800x1200
/// cover needs ~5 MB of scratch instead of ~20 MB, and runs several times faster.
fn resize_rgb(photo: &RgbImage, crop: Option<(u32, u32, u32, u32)>, width: u32, height: u32, filter: FilterType) -> RgbImage {
    let Ok(source) = ImageRef::new(photo.width(), photo.height(), photo.as_raw(), PixelType::U8x3) else {
        return RgbImage::new(width, height);
    };
    let mut target = ResizeImage::new(width, height, PixelType::U8x3);
    let mut options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(filter));
    if let Some((x, y, w, h)) = crop {
        options = options.crop(f64::from(x), f64::from(y), f64::from(w), f64::from(h));
    }
    if Resizer::new().resize(&source, &mut target, &options).is_err() {
        return RgbImage::new(width, height);
    }
    RgbImage::from_raw(width, height, target.into_vec()).unwrap_or_else(|| RgbImage::new(width, height))
}

/// Blur-like backdrop: box-average to 32 px wide, then smooth upscale to 256 px.
/// Two tiny resizes (~0.3 ms) instead of a real Gaussian blur on 2 Mpx.
pub fn smooth_backdrop(photo: &RgbImage) -> RgbImage {
    let (w, h) = photo.dimensions();
    if w == 0 || h == 0 { return RgbImage::new(1, 1); }
    let small_h = ((32.0 * f64::from(h) / f64::from(w)).round() as u32).max(1);
    let small = resize_rgb(photo, None, 32, small_h, FilterType::Box);
    let large_h = ((256.0 * f64::from(h) / f64::from(w)).round() as u32).max(1);
    resize_rgb(&small, None, 256, large_h, FilterType::CatmullRom)
}

/// Proportional downscale so the longest edge is at most `max_edge`.
/// (`imageops::thumbnail` takes exact dimensions and would distort the photo.)
pub fn fit_within(photo: &RgbImage, max_edge: u32) -> Option<RgbImage> {
    let (width, height) = photo.dimensions();
    let longest = width.max(height);
    if longest <= max_edge || width == 0 || height == 0 { return None; }
    let scale = f64::from(max_edge) / f64::from(longest);
    let (w, h) = (((f64::from(width) * scale).round() as u32).max(1), ((f64::from(height) * scale).round() as u32).max(1));
    Some(resize_rgb(photo, None, w, h, FilterType::CatmullRom))
}

/// Where the photo goes on the print, in frame pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window { pub x: u32, pub y: u32, pub width: u32, pub height: u32 }

impl Window {
    pub fn full(width: u32, height: u32) -> Self { Self { x: 0, y: 0, width, height } }
    /// Width / height of the photo once printed.
    pub fn ratio(&self) -> f32 { self.width as f32 / self.height.max(1) as f32 }
}

/// A print frame and its photo window, found once when the frame is loaded.
#[derive(Debug)]
pub struct PrintFrame {
    pub rgba: RgbaImage,
    pub window: Window,
}

impl PrintFrame {
    pub fn new(rgba: RgbaImage) -> Self {
        let window = detect_window(&rgba);
        Self { rgba, window }
    }
    pub fn portrait(&self) -> bool { self.rgba.height() > self.rgba.width() }
    /// The photo covers the whole print (frame with a full-bleed transparent area).
    pub fn full_bleed(&self) -> bool { self.window == Window::full(self.rgba.width(), self.rgba.height()) }
}

/// Photo window of a frame: the bounding box of the transparent area that
/// contains the centre of the print. Decorations may overlap the window's
/// edges (flowers, balloons): every transparent pixel connected to the centre
/// stays inside the box, so the photo always shows wherever the frame is
/// see-through. A frame whose see-through area reaches (almost) every edge is
/// full-bleed: the photo covers the whole print under it (former behaviour).
///
/// Example: a portrait frame (1200×1800) with its window at x 96, y 300,
/// 1008×1200 (the bundled Party portrait frame uses this layout).
pub fn detect_window(frame: &RgbaImage) -> Window {
    let (w, h) = frame.dimensions();
    let full = Window::full(w, h);
    if w < 3 || h < 3 { return full; }
    let clear = |x: u32, y: u32| frame.get_pixel(x, y)[3] < 128;
    // Start at the centre, or the closest see-through pixel of the central third.
    let (cx, cy) = (w / 2, h / 2);
    let start = if clear(cx, cy) { Some((cx, cy)) } else {
        let mut best: Option<(u64, (u32, u32))> = None;
        for y in (h / 3..h * 2 / 3).step_by(2) {
            for x in (w / 3..w * 2 / 3).step_by(2) {
                if clear(x, y) {
                    let d = u64::from(x.abs_diff(cx)).pow(2) + u64::from(y.abs_diff(cy)).pow(2);
                    if best.is_none_or(|(b, _)| d < b) { best = Some((d, (x, y))); }
                }
            }
        }
        best.map(|(_, p)| p)
    };
    let Some(start) = start else { return full };
    // Scanline flood fill; one byte per pixel of bookkeeping (≈ 2 MB for 1800×1200).
    let mut seen = vec![false; (w * h) as usize];
    let (mut x0, mut y0, mut x1, mut y1) = (start.0, start.1, start.0, start.1);
    let mut stack = vec![start];
    while let Some((x, y)) = stack.pop() {
        if seen[(y * w + x) as usize] { continue; }
        let mut left = x;
        while left > 0 && clear(left - 1, y) && !seen[(y * w + left - 1) as usize] { left -= 1; }
        let mut right = x;
        while right + 1 < w && clear(right + 1, y) && !seen[(y * w + right + 1) as usize] { right += 1; }
        for i in left..=right { seen[(y * w + i) as usize] = true; }
        x0 = x0.min(left); x1 = x1.max(right); y0 = y0.min(y); y1 = y1.max(y);
        for ny in [y.wrapping_sub(1), y + 1] {
            if ny >= h { continue; }
            let mut i = left;
            while i <= right {
                if clear(i, ny) && !seen[(ny * w + i) as usize] {
                    stack.push((i, ny));
                    // Skip the rest of this run: one seed per run is enough.
                    while i <= right && clear(i, ny) { i += 1; }
                } else { i += 1; }
            }
        }
    }
    let window = Window { x: x0, y: y0, width: x1 - x0 + 1, height: y1 - y0 + 1 };
    // Reaching (almost) every edge: full-bleed frame.
    if window.width * 100 >= w * 98 && window.height * 100 >= h * 98 { return full; }
    window
}

/// PNG frames must be transparent. The frame is sized to the *export* canvas, not the window.
pub fn compose(photo: &RgbImage, frame: Option<&PrintFrame>) -> RgbImage {
    compose_owned(photo.clone(), frame)
}

/// Take ownership of the captured RGB buffer: avoid a full-size clone when
/// the camera output already matches the configured print-frame dimensions.
/// The photo is cropped to the frame's window (cover) and placed there; the
/// rest of the print is white paper under the frame.
pub fn compose_owned(photo: RgbImage, frame: Option<&PrintFrame>) -> RgbImage {
    let Some(print) = frame else { return photo; };
    let frame = &print.rgba;
    let (width, height) = frame.dimensions();
    if width == 0 || height == 0 { return photo; }
    let window = print.window;
    let mut output = if print.full_bleed() {
        if photo.dimensions() == (width, height) { photo } else { cover(&photo, width, height) }
    } else {
        let placed = cover(&photo, window.width, window.height);
        drop(photo);
        let mut canvas = RgbImage::from_pixel(width, height, image::Rgb([255, 255, 255]));
        let row = window.width as usize * 3;
        for (y, src) in placed.as_raw().chunks_exact(row).enumerate() {
            let start = ((window.y as usize + y) * width as usize + window.x as usize) * 3;
            canvas.as_mut()[start..start + row].copy_from_slice(src);
        }
        canvas
    };
    for (dst, src) in output.as_mut().chunks_exact_mut(3)
        .zip(frame.as_raw().chunks_exact(4))
    {
        let alpha = u16::from(src[3]);
        if alpha == 0 { continue; }
        if alpha == 255 {
            dst.copy_from_slice(&src[..3]);
            continue;
        }
        let inverse = 255 - alpha;
        for channel in 0..3 {
            let value = u16::from(src[channel]) * alpha
                + u16::from(dst[channel]) * inverse;
            dst[channel] = ((value + 127) / 255) as u8;
        }
    }
    output
}

/// [`load_frame`] plus the photo window.
pub fn load_print_frame(path: &Path) -> io::Result<PrintFrame> {
    load_frame(path).map(PrintFrame::new)
}

/// A print frame known by its file, size and photo window, without its pixels:
/// the kiosk keeps this (a few bytes) instead of an 8.6 MB RGBA bitmap per
/// orientation, and the photo worker decodes the PNG for each print (≈ 30 ms).
#[derive(Debug, Clone)]
pub struct FrameRef {
    /// `None`: no frame, the photo covers the whole print.
    pub path: Option<std::path::PathBuf>,
    pub width: u32,
    pub height: u32,
    pub window: Window,
}

impl FrameRef {
    /// Decodes the frame once to check it and find its window, then drops the pixels.
    pub fn open(path: &Path) -> io::Result<Self> {
        let frame = load_print_frame(path)?;
        Ok(Self { path: Some(path.to_owned()), width: frame.rgba.width(), height: frame.rgba.height(), window: frame.window })
    }

    pub fn plain(width: u32, height: u32) -> Self { Self { path: None, width, height, window: Window::full(width, height) } }
    pub fn portrait(&self) -> bool { self.height > self.width }
    pub fn full_bleed(&self) -> bool { self.window == Window::full(self.width, self.height) }

    /// The print: photo placed in the window, frame on top.
    pub fn compose(&self, photo: RgbImage) -> io::Result<RgbImage> {
        let Some(path) = self.path.as_deref() else { return Ok(cover(&photo, self.width, self.height)); };
        let rgba = load_frame(path)?;
        if rgba.dimensions() != (self.width, self.height) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Frame changed on disk"));
        }
        Ok(compose_owned(photo, Some(&PrintFrame { rgba, window: self.window })))
    }
}

pub fn load_frame(path: &Path) -> io::Result<RgbaImage> {
    const FRAME_EDGE: u32 = 3200;
    const FRAME_FILE_BYTES: u64 = 16 * 1024 * 1024;
    if std::fs::metadata(path)?.len() > FRAME_FILE_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Frame PNG exceeds file size limit"));
    }
    let mut reader = ImageReader::open(path)?.with_guessed_format().map_err(io::Error::other)?;
    if reader.format() != Some(ImageFormat::Png) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Frame must be PNG"));
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(FRAME_EDGE);
    limits.max_image_height = Some(FRAME_EDGE);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    // Limit decoding itself, not just the dimensions after allocating a bitmap.
    let rgba = reader.decode().map_err(io::Error::other)?.into_rgba8();
    if rgba.width() < 600 || rgba.height() < 600 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Frame must be at least 600 px per side"));
    }
    if !rgba.pixels().any(|pixel| pixel[3] < 255) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Frame needs a transparent photo area"));
    }
    Ok(rgba)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, Rgba};
    #[test]
    fn fit_within_keeps_aspect_ratio() {
        let photo = RgbImage::new(1800, 1200);
        let small = fit_within(&photo, 1200).expect("downscaled");
        assert_eq!(small.dimensions(), (1200, 800));
        assert!(fit_within(&RgbImage::new(600, 400), 1200).is_none());
    }
    #[test]
    fn center_crop_preserves_center() {
        let mut photo = RgbImage::from_pixel(4, 2, Rgb([0,0,0]));
        for x in 1..3 { for y in 0..2 { photo.put_pixel(x,y,Rgb([255,0,0])); } }
        let cropped = cover(&photo, 2, 2);
        assert_eq!(cropped.dimensions(), (2,2));
        assert!(cropped.get_pixel(0,0)[0] > 200);
    }
    #[test]
    fn extreme_aspect_ratios_only_resize_the_source_crop() {
        // The old resize-then-crop path could allocate over 260 MiB
        // for this source despite a 600 x 600 output.
        let portrait = RgbImage::from_pixel(16, 4096, Rgb([17, 42, 91]));
        let landscape = cover(&portrait, 600, 600);
        assert_eq!(landscape.dimensions(), (600, 600));
        assert_eq!(landscape.get_pixel(300, 300).0, [17, 42, 91]);
        let wide = RgbImage::from_pixel(4096, 16, Rgb([61, 83, 101]));
        let square = cover(&wide, 600, 600);
        assert_eq!(square.get_pixel(300, 300).0, [61, 83, 101]);
    }

    #[test]
    fn frame_alpha_is_applied_to_export() {
        let photo = RgbImage::from_pixel(2,2,Rgb([10,20,30]));
        let mut frame = RgbaImage::from_pixel(2,2,Rgba([255,0,0,0]));
        frame.put_pixel(0,0,Rgba([255,0,0,255]));
        let composed = compose(&photo,Some(&PrintFrame::new(frame)));
        assert_eq!(composed.get_pixel(0,0).0,[255,0,0]);
        assert_eq!(composed.get_pixel(1,1).0,[10,20,30]);
    }
    #[test]
    fn half_alpha_blending_uses_no_rgba_canvas() {
        let photo = RgbImage::from_pixel(1, 1, Rgb([0, 100, 200]));
        let frame = RgbaImage::from_pixel(1, 1, Rgba([200, 100, 0, 128]));
        let result = compose(&photo, Some(&PrintFrame::new(frame)));
        let rgb = result.get_pixel(0, 0).0;
        assert!((rgb[0] as i16 - 100).abs() <= 1);
        assert_eq!(rgb[1], 100);
        assert!((rgb[2] as i16 - 100).abs() <= 1);
    }
    #[test]
    fn owned_composition_reuses_rgb_allocation_when_dimensions_match() {
        let original = RgbImage::from_pixel(64, 48, Rgb([12, 34, 56]));
        let ptr = original.as_ptr();
        let frame = RgbaImage::from_pixel(64, 48, Rgba([20, 50, 80, 0]));
        let result = compose_owned(original, Some(&PrintFrame::new(frame)));
        assert_eq!(result.as_ptr(), ptr);
        assert_eq!(result.get_pixel(0, 0).0, [12, 34, 56]);
    }
    /// Opaque border with a transparent window, like a typical portrait frame.
    fn windowed(w: u32, h: u32, window: Window, notches: bool) -> RgbaImage {
        let mut frame = RgbaImage::from_pixel(w, h, Rgba([40, 20, 60, 255]));
        for y in window.y..window.y + window.height {
            for x in window.x..window.x + window.width { frame.put_pixel(x, y, Rgba([0, 0, 0, 0])); }
        }
        if notches {
            // Decorations biting into the window's edges (a flower, a balloon).
            for y in window.y..window.y + 80 { for x in window.x..window.x + 120 { frame.put_pixel(x, y, Rgba([200, 50, 90, 255])); } }
            for y in window.y + window.height - 60..window.y + window.height { for x in window.x + 300..window.x + 500 { frame.put_pixel(x, y, Rgba([90, 200, 50, 255])); } }
        }
        frame
    }

    #[test]
    fn portrait_window_is_detected() {
        let expected = Window { x: 96, y: 300, width: 1008, height: 1200 };
        let frame = PrintFrame::new(windowed(1200, 1800, expected, true));
        assert_eq!(frame.window, expected);
        assert!(frame.portrait() && !frame.full_bleed());
        let landscape = Window { x: 108, y: 216, width: 1584, height: 864 };
        assert_eq!(detect_window(&windowed(1800, 1200, landscape, false)), landscape);
    }

    #[test]
    fn opaque_centre_still_finds_the_window() {
        let window = Window { x: 100, y: 100, width: 400, height: 400 };
        let mut frame = windowed(600, 600, window, false);
        // A small logo in the middle of the photo area.
        for y in 290..310 { for x in 290..310 { frame.put_pixel(x, y, Rgba([255, 255, 255, 255])); } }
        assert_eq!(detect_window(&frame), window);
    }

    #[test]
    fn photo_is_placed_in_the_window() {
        let window = Window { x: 96, y: 300, width: 1008, height: 1200 };
        let frame = PrintFrame::new(windowed(1200, 1800, window, false));
        let photo = RgbImage::from_pixel(6000 / 4, 4000 / 4, Rgb([10, 200, 30]));
        let print = compose_owned(photo, Some(&frame));
        assert_eq!(print.dimensions(), (1200, 1800));
        assert_eq!(print.get_pixel(600, 900).0, [10, 200, 30]);
        assert_eq!(print.get_pixel(97, 301).0, [10, 200, 30]);
        assert_eq!(print.get_pixel(50, 50).0, [40, 20, 60]);
    }

    #[test]
    fn full_bleed_frame_covers_the_print() {
        let mut frame = RgbaImage::from_pixel(600, 400, Rgba([0, 0, 0, 0]));
        for x in 0..600 { frame.put_pixel(x, 399, Rgba([255, 0, 0, 255])); }
        assert!(PrintFrame::new(frame).full_bleed());
    }

    #[test]
    fn no_frame_is_lossless() {
        let photo = RgbImage::from_pixel(2,2,Rgb([8,9,10]));
        assert_eq!(compose(&photo,None),photo);
    }
}
