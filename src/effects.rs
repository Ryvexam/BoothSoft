// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Allocation-free, deterministic RGB filters for each owned image buffer.
use image::RgbImage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhotoFilter {
    Original,
    /// Punchy colours and a touch of contrast ("Éclat").
    Vivid,
    /// Low contrast, lifted blacks, warm skin tones ("Doux").
    Soft,
    BlackWhite,
    Sepia,
    Vintage,
}

impl PhotoFilter {
    /// Display order of the live filter strip.
    pub const ALL: [PhotoFilter; 6] = [Self::Original, Self::Vivid, Self::Soft, Self::BlackWhite, Self::Sepia, Self::Vintage];

    pub fn next(self) -> Self {
        let index = Self::ALL.iter().position(|f| *f == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    /// Short guest-facing name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Original => "Naturel",
            Self::Vivid => "Éclat",
            Self::Soft => "Doux",
            Self::BlackWhite => "N&B",
            Self::Sepia => "Sépia",
            Self::Vintage => "Vintage",
        }
    }
}

/// Every filter is an affine colour transform: `out = M · rgb + offset`
/// (rows of M, channels in 0..1). This single definition drives both the GPU
/// live view (shader uniform) and the saved photo (fixed-point CPU loop), so the
/// guest sees exactly what will be printed.
pub fn color_matrix(filter: PhotoFilter) -> ([[f32; 3]; 3], [f32; 3]) {
    // Integer luma weights (77, 150, 29) / 256, as used historically by BoothSoft.
    const L: [f32; 3] = [77.0 / 256.0, 150.0 / 256.0, 29.0 / 256.0];
    let saturation = |s: f32| {
        let mut m = [[0.0; 3]; 3];
        for (row, values) in m.iter_mut().enumerate() {
            for (column, value) in values.iter_mut().enumerate() {
                *value = (1.0 - s) * L[column] + if row == column { s } else { 0.0 };
            }
        }
        m
    };
    let scale = |m: [[f32; 3]; 3], gains: [f32; 3]| {
        let mut out = m;
        for (row, values) in out.iter_mut().enumerate() { for value in values.iter_mut() { *value *= gains[row]; } }
        out
    };
    match filter {
        PhotoFilter::Original => ([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], [0.0; 3]),
        PhotoFilter::Vivid => {
            let contrast = 1.10;
            (scale(saturation(1.38), [contrast; 3]), [0.5 * (1.0 - contrast); 3])
        }
        PhotoFilter::Soft => {
            let contrast = 0.86;
            let lift = 0.5 * (1.0 - contrast) + 0.03;
            (scale(saturation(0.82), [contrast * 1.04, contrast * 1.0, contrast * 0.93]), [lift + 0.01, lift, lift - 0.01])
        }
        PhotoFilter::BlackWhite => ([L, L, L], [0.0; 3]),
        PhotoFilter::Sepia => ([[0.393, 0.769, 0.189], [0.349, 0.686, 0.168], [0.272, 0.534, 0.131]], [0.0; 3]),
        PhotoFilter::Vintage => ([
            [0.67 * L[0] + 0.33, 0.67 * L[1], 0.67 * L[2]],
            [0.40 * L[0], 0.40 * L[1] + 0.60, 0.40 * L[2]],
            [0.35 * L[0], 0.35 * L[1], 0.35 * L[2] + 0.65],
        ], [15.0 / 255.0, 6.0 / 255.0, 0.0]),
    }
}

/// Apply a filter without creating another RGB image or allocating per pixel.
/// The float matrix is converted once to 12-bit fixed point for the hot loop.
pub fn apply_in_place(image: &mut RgbImage, filter: PhotoFilter) {
    if filter == PhotoFilter::Original { return; }
    let (matrix, offset) = color_matrix(filter);
    let fixed = |v: f32| (v * 4096.0).round() as i32;
    let m: [[i32; 3]; 3] = matrix.map(|row| row.map(fixed));
    // Offsets are in 0..1 channel units: scale by 255 and add rounding.
    let o: [i32; 3] = offset.map(|v| fixed(v * 255.0) + 2048);
    for pixel in image.pixels_mut() {
        let [r, g, b] = pixel.0.map(i32::from);
        pixel.0 = [0, 1, 2].map(|c| ((m[c][0] * r + m[c][1] * g + m[c][2] * b + o[c]) >> 12).clamp(0, 255) as u8);
    }
}

/// The owned path avoids an extra full-frame RGB copy on camera capture.
pub fn apply_owned(mut image: RgbImage, filter: PhotoFilter) -> RgbImage {
    apply_in_place(&mut image, filter);
    image
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    #[test]
    fn filter_cycles() {
        assert_eq!(PhotoFilter::Vintage.next(), PhotoFilter::Original);
        let mut seen = vec![PhotoFilter::Original];
        let mut f = PhotoFilter::Original.next();
        while f != PhotoFilter::Original { seen.push(f); f = f.next(); }
        assert_eq!(seen, PhotoFilter::ALL.to_vec());
    }

    #[test]
    fn grayscale_is_neutral() {
        let image = RgbImage::from_pixel(1, 1, Rgb([255, 0, 0]));
        let pixel = apply_owned(image, PhotoFilter::BlackWhite).get_pixel(0, 0).0;
        assert_eq!(pixel[0], pixel[1]);
        assert_eq!(pixel[1], pixel[2]);
    }

    #[test]
    fn original_does_not_change_pixels() {
        let original = RgbImage::from_pixel(4, 4, Rgb([12, 42, 87]));
        assert_eq!(apply_owned(original.clone(), PhotoFilter::Original), original);
    }

    #[test]
    fn all_filters_stay_in_channel_range() {
        for filter in PhotoFilter::ALL {
            for rgb in [[255, 255, 255], [0, 0, 0], [255, 0, 0], [0, 255, 40]] {
                let result = apply_owned(RgbImage::from_pixel(1, 1, Rgb(rgb)), filter);
                assert_eq!(result.dimensions(), (1, 1));
            }
        }
    }

    #[test]
    fn historical_filters_keep_their_look() {
        // Same values as the pre-matrix integer implementation (±1 rounding).
        let sepia = apply_owned(RgbImage::from_pixel(1, 1, Rgb([100, 150, 200])), PhotoFilter::Sepia).get_pixel(0, 0).0;
        let expected = [(393 * 100 + 769 * 150 + 189 * 200) / 1000, (349 * 100 + 686 * 150 + 168 * 200) / 1000, (272 * 100 + 534 * 150 + 131 * 200) / 1000];
        for c in 0..3 { assert!((i32::from(sepia[c]) - expected[c].min(255)).abs() <= 1, "{sepia:?} vs {expected:?}"); }
    }

    #[test]
    fn vivid_saturates_and_soft_desaturates() {
        let spread = |p: [u8; 3]| i32::from(*p.iter().max().unwrap()) - i32::from(*p.iter().min().unwrap());
        let base = [180, 120, 90];
        let vivid = apply_owned(RgbImage::from_pixel(1, 1, Rgb(base)), PhotoFilter::Vivid).get_pixel(0, 0).0;
        let soft = apply_owned(RgbImage::from_pixel(1, 1, Rgb(base)), PhotoFilter::Soft).get_pixel(0, 0).0;
        assert!(spread(vivid) > spread(base) && spread(soft) < spread(base));
    }
}
