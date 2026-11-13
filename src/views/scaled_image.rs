//! Local images pre-scaled to the device pixels they cover.
//!
//! GPUI samples images bilinearly without mipmaps, so a 192px avatar drawn at
//! 64 device pixels skips most source pixels and aliases, and a screenshot
//! shrunk to a thumbnail turns its text grainy. Resampling once on the CPU
//! with Lanczos, to a nearby size bucket, keeps edges and detail smooth the
//! way a browser's downscale does.
use gpui_kit::{App, ImageAssetLoader, ImageCacheError, ImageSource, Resource, Window};
use image::{
    DynamicImage, ImageDecoder, ImageFormat, ImageReader, RgbaImage, imageops::FilterType,
};
use std::path::PathBuf;

mod state;

/// How the image fills its box, matching the `ObjectFit` it is drawn with.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Fit {
    /// Fills the box, center-cropping the overflow.
    Cover,
    /// Fits inside the box; never enlarged past the source.
    Contain,
}

/// A local image file and the box, in device pixels, to scale it to.
#[derive(Clone, PartialEq, Eq)]
struct ScaledSource {
    path: PathBuf,
    width_px: u32,
    height_px: u32,
    fit: Fit,
}

// A geometric ladder limits the number of resizes. Quantize both box edges;
// scale_file preserves the source aspect ratio, leaving cropping to ObjectFit.
fn bucket(pixels: f32) -> u32 {
    let pixels = pixels.max(1.);
    2_f32.powf((pixels.log2() * 2.).ceil() / 2.).ceil() as u32
}

/// Decodes the file and resamples it for its box, as BGRA for GPUI.
fn scale_file(source: &ScaledSource) -> Result<RgbaImage, ImageCacheError> {
    // Keybase caches attachments as `*.attachment`, so the format comes from
    // the contents, not the extension.
    let reader = ImageReader::open(&source.path)?.with_guessed_format()?;
    if matches!(reader.format(), Some(ImageFormat::Gif | ImageFormat::WebP)) {
        // Possibly animated: leave it to the plain loader, which keeps every frame.
        return Err(ImageCacheError::Asset(
            "animated formats are not pre-scaled".into(),
        ));
    }
    let mut decoder = reader.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut decoded = DynamicImage::from_decoder(decoder)?;
    // Originals can retain camera EXIF rotation even when Keybase's preview
    // pixels are already upright. Orient before measuring or resampling.
    decoded.apply_orientation(orientation);
    let x = source.width_px as f64 / decoded.width() as f64;
    let y = source.height_px as f64 / decoded.height() as f64;
    let scale = match source.fit {
        Fit::Cover => x.max(y),
        Fit::Contain => x.min(y),
    }
    .min(1.);
    let scaled = if scale < 1. {
        decoded.resize_exact(
            (decoded.width() as f64 * scale).round().max(1.) as u32,
            (decoded.height() as f64 * scale).round().max(1.) as u32,
            FilterType::Lanczos3,
        )
    } else {
        decoded
    };
    let mut scaled = scaled.into_rgba8();
    // GPUI expects BGRA.
    for pixel in scaled.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Ok(scaled)
}

/// An image source for the file at `path` drawn in a `width` by `height`
/// point box, or `None` for a `.gif`, whose frames the plain path loader keeps
/// animating. During resizing, the previous completed image stays visible. For a file
/// this loader leaves alone (a GIF or WebP without the extension, or one it
/// cannot decode), the plain loader handles it after scaling returns an error.
pub fn scaled_image_source(
    path: PathBuf,
    width: f32,
    height: f32,
    fit: Fit,
) -> Option<ImageSource> {
    let is_gif = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gif"));
    if is_gif {
        return None;
    }
    let plain = Resource::Path(path.clone().into());
    Some(ImageSource::from(
        move |window: &mut Window, cx: &mut App| {
            let scale = window.scale_factor();
            let source = ScaledSource {
                path: path.clone(),
                width_px: bucket(width * scale),
                height_px: bucket(height * scale),
                fit,
            };
            match state::image_for(source, window, cx) {
                Some(Ok(image)) => Some(Ok(image)),
                Some(Err(_)) => window.use_asset::<ImageAssetLoader>(&plain, cx),
                None => None,
            }
        },
    ))
}

/// `source` pre-scaled for a `width` by `height` point box when it is a local
/// file; other sources are returned unchanged.
pub fn scaled_for_box(source: ImageSource, width: f32, height: f32, fit: Fit) -> ImageSource {
    match &source {
        ImageSource::Resource(Resource::Path(path)) => {
            scaled_image_source(path.to_path_buf(), width, height, fit).unwrap_or(source)
        }
        _ => source,
    }
}

#[cfg(test)]
mod tests {
    use super::{Fit, ScaledSource, bucket, scale_file};
    use image::{ImageFormat, RgbaImage};
    use std::path::{Path, PathBuf};

    /// A `width` by `height` image saved as `format` under a Keybase-style
    /// `.attachment` name, so the extension says nothing about the format.
    fn attachment(name: &str, width: u32, height: u32, format: ImageFormat) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "zbase-scaled-{name}-{}.attachment",
            std::process::id()
        ));
        RgbaImage::new(width, height)
            .save_with_format(&path, format)
            .unwrap();
        path
    }

    fn scaled(path: &Path, width_px: u32, height_px: u32, fit: Fit) -> Option<(u32, u32)> {
        let source = ScaledSource {
            path: path.to_path_buf(),
            width_px,
            height_px,
            fit,
        };
        scale_file(&source).ok().map(|image| image.dimensions())
    }

    #[test]
    fn camera_orientation_is_applied_to_full_size_and_resampled_attachments() {
        use image::{Rgb, RgbImage};
        // Four distinct corners make rotations and mirrored orientations visible.
        let colors = [[240, 20, 20], [20, 240, 20], [20, 20, 240], [240, 240, 20]];
        let pixels = RgbImage::from_fn(80, 40, |x, y| {
            Rgb(colors[usize::from(x >= 40) + 2 * usize::from(y >= 20)])
        });
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 100)
            .encode_image(&pixels)
            .unwrap();
        // EXIF IFD0 orientation in a little-endian TIFF inside JPEG APP1.
        for orientation in 1u8..=8 {
            let mut exif = b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0".to_vec();
            exif.extend_from_slice(&[orientation, 0, 0, 0, 0, 0, 0, 0]);
            let mut bytes = jpeg[..2].to_vec();
            bytes.extend_from_slice(&[0xff, 0xe1]);
            bytes.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
            bytes.extend_from_slice(&exif);
            bytes.extend_from_slice(&jpeg[2..]);
            let path = std::env::temp_dir().join(format!(
                "zbase-exif-{}-{orientation}.attachment",
                std::process::id()
            ));
            std::fs::write(&path, bytes).unwrap();
            for size in [160, 20] {
                let image = scale_file(&ScaledSource {
                    path: path.clone(),
                    width_px: size,
                    height_px: size,
                    fit: Fit::Contain,
                })
                .unwrap();
                let long = size.min(80);
                assert_eq!(
                    image.dimensions(),
                    if orientation >= 5 {
                        (long / 2, long)
                    } else {
                        (long, long / 2)
                    }
                );
                let expected = colors[[0, 1, 3, 2, 0, 2, 3, 1][orientation as usize - 1]];
                let actual = image.get_pixel(1, 1).0;
                for (actual, expected) in
                    [actual[2], actual[1], actual[0]].into_iter().zip(expected)
                {
                    assert!(
                        actual.abs_diff(expected) < 25,
                        "orientation {orientation}, size {size}: wrong corner"
                    );
                }
            }
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn contain_shrinks_to_the_box_keeping_the_aspect_ratio() {
        let path = attachment("contain", 640, 180, ImageFormat::Png);
        assert_eq!(scaled(&path, 560, 480, Fit::Contain), Some((560, 158)));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn contain_never_enlarges() {
        let path = attachment("small", 410, 87, ImageFormat::Png);
        assert_eq!(scaled(&path, 560, 480, Fit::Contain), Some((410, 87)));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn cover_preserves_aspect_ratio_for_gpu_cropping() {
        let path = attachment("cover", 640, 480, ImageFormat::Png);
        assert_eq!(scaled(&path, 200, 200, Fit::Cover), Some((267, 200)));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn resize_ladder_bounds_variants_and_final_downscale() {
        let sizes: std::collections::HashSet<_> = (72..=280).map(|n| bucket(n as f32)).collect();
        assert!(sizes.len() <= 5);
        for n in 1..=4096 {
            let rounded = bucket(n as f32);
            assert!(rounded >= n);
            assert!((rounded as f32) <= n as f32 * std::f32::consts::SQRT_2 + 1.);
        }
    }

    #[test]
    fn cover_never_enlarges_small_sources() {
        let path = attachment("small-cover", 40, 20, ImageFormat::Png);
        assert_eq!(scaled(&path, 200, 200, Fit::Cover), Some((40, 20)));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn gifs_are_left_to_the_animating_loader() {
        let path = attachment("gif", 64, 64, ImageFormat::Gif);
        assert_eq!(scaled(&path, 32, 32, Fit::Contain), None);
        let _ = std::fs::remove_file(path);
    }
}
