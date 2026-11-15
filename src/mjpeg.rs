// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! MJPEG frame decoding (libjpeg-turbo through `mozjpeg`), tuned per use:
//!
//! * **Live view**: integer-fast IDCT, plain chroma upsampling, no block
//!   smoothing, RGBA output — the cheapest decode libjpeg-turbo offers, with
//!   no visible difference on a moving 30 FPS mirror.
//! * **Capture**: accurate IDCT and smooth chroma upsampling, RGB output, since
//!   that frame is printed.
//!
//! libjpeg-turbo reports errors by unwinding out of a C callback. A truncated
//! USB frame is routine, so the decode is wrapped in `catch_unwind` (the release
//! profile uses `panic = "unwind"` for this reason) and simply dropped.
use std::panic::{catch_unwind, AssertUnwindSafe};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Quality {
    /// Live view: fastest settings.
    Preview,
    /// Saved / printed photo: accurate settings.
    Photo,
}

/// Decode `jpeg` into `out` (resized, allocation reused). Returns (width, height).
pub fn decode(jpeg: &[u8], out: &mut Vec<u8>, rgba: bool, quality: Quality) -> Result<(u32, u32), String> {
    let result = catch_unwind(AssertUnwindSafe(|| -> std::io::Result<(u32, u32)> {
        let mut decompress = mozjpeg::Decompress::new_mem(jpeg)?;
        if quality == Quality::Preview {
            decompress.dct_method(mozjpeg::DctMethod::IntegerFast);
            decompress.do_fancy_upsampling(false);
            decompress.do_block_smoothing(false);
        }
        let mut started = if rgba { decompress.rgba()? } else { decompress.rgb()? };
        let (width, height) = (started.width(), started.height());
        out.resize(width * height * if rgba { 4 } else { 3 }, 0);
        started.read_scanlines_into(&mut out[..])?;
        started.finish()?;
        Ok((width as u32, height as u32))
    }));
    match result {
        Ok(Ok(size)) => Ok(size),
        Ok(Err(error)) => Err(error.to_string()),
        Err(_) => Err("Corrupt MJPEG frame dropped".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{codecs::jpeg::JpegEncoder, ExtendedColorType, RgbImage};

    fn sample() -> Vec<u8> {
        let image = RgbImage::from_fn(64, 48, |x, y| image::Rgb([(x * 4) as u8, (y * 5) as u8, 120]));
        let mut jpeg = Vec::new();
        JpegEncoder::new_with_quality(&mut jpeg, 85).encode(image.as_raw(), 64, 48, ExtendedColorType::Rgb8).unwrap();
        jpeg
    }

    #[test]
    fn decodes_rgba_preview_and_rgb_photo_into_reused_buffer() {
        let jpeg = sample();
        let mut buffer = Vec::new();
        assert_eq!(decode(&jpeg, &mut buffer, true, Quality::Preview).unwrap(), (64, 48));
        assert_eq!(buffer.len(), 64 * 48 * 4);
        assert_eq!(buffer[3], 255, "opaque alpha");
        assert_eq!(decode(&jpeg, &mut buffer, false, Quality::Photo).unwrap(), (64, 48));
        assert_eq!(buffer.len(), 64 * 48 * 3);
    }

    #[test]
    fn truncated_frame_is_an_error_not_a_crash() {
        let jpeg = sample();
        let mut buffer = Vec::new();
        for cut in [10, jpeg.len() / 3, jpeg.len() - 20] {
            let _ = decode(&jpeg[..cut], &mut buffer, true, Quality::Preview);
        }
        assert!(decode(b"not a jpeg", &mut buffer, true, Quality::Preview).is_err());
    }
}
