// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Resource-bounded decoding of files used by the print pipeline.
//! JPEG and PNG are deliberately the only accepted media formats.
use image::{ImageFormat, ImageReader, Limits, RgbImage};
use std::{io, path::Path};

const MAX_PRINT_EDGE: u32 = 8_192;
const MAX_PRINT_PIXELS: u64 = 24_000_000;
const MAX_DECODE_ALLOC: u64 = 192 * 1024 * 1024;

pub fn read_print_jpeg(path: &Path) -> io::Result<RgbImage> {
    let header = ImageReader::open(path)?.with_guessed_format().map_err(io::Error::other)?;
    if header.format() != Some(ImageFormat::Jpeg) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Print input must be JPEG"));
    }
    let (width, height) = header.into_dimensions().map_err(io::Error::other)?;
    if width == 0 || height == 0 || width > MAX_PRINT_EDGE || height > MAX_PRINT_EDGE
        || u64::from(width) * u64::from(height) > MAX_PRINT_PIXELS {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Print JPEG exceeds image limits"));
    }
    let mut reader = ImageReader::open(path)?.with_guessed_format().map_err(io::Error::other)?;
    if reader.format() != Some(ImageFormat::Jpeg) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "JPEG changed while decoding"));
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_PRINT_EDGE);
    limits.max_image_height = Some(MAX_PRINT_EDGE);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    reader.limits(limits);
    let decoded = reader.decode().map_err(io::Error::other)?;
    let (decoded_width, decoded_height) = (decoded.width(), decoded.height());
    if u64::from(decoded_width) * u64::from(decoded_height) > MAX_PRINT_PIXELS {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Decoded image exceeds pixel budget"));
    }
    Ok(decoded.into_rgb8())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::{SystemTime, UNIX_EPOCH}};
    #[test]
    fn rejects_non_jpeg_and_preserves_normal_jpeg() {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("boothsoft-imageio-{}-{stamp}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let photo = image::RgbImage::from_pixel(40, 32, image::Rgb([10, 20, 30]));
        let jpg = root.join("photo.jpg");
        let png = root.join("fake.jpg");
        photo.save_with_format(&jpg, ImageFormat::Jpeg).unwrap();
        photo.save_with_format(&png, ImageFormat::Png).unwrap();
        assert_eq!(read_print_jpeg(&jpg).unwrap().dimensions(), (40, 32));
        assert_eq!(read_print_jpeg(&png).unwrap_err().kind(), io::ErrorKind::InvalidData);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pixel_budget_rejects_oversized_dimensions_without_large_allocations() {
        assert!(u64::from(MAX_PRINT_EDGE) * u64::from(MAX_PRINT_EDGE) > MAX_PRINT_PIXELS);
    }
}
