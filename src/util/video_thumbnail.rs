//! Video thumbnails, kept apart from `video_decoder` so this has no UI
//! framework dependency.

use std::{path::Path, sync::OnceLock};

use ffmpeg_next as ffmpeg;
use image::RgbaImage;

const MAX_VIDEO_WIDTH: u32 = 480;

pub(super) static FFMPEG_INIT_OK: OnceLock<bool> = OnceLock::new();

pub(super) fn target_video_size(width: u32, height: u32) -> (u32, u32) {
    if width <= MAX_VIDEO_WIDTH {
        return (width.max(1), height.max(1));
    }
    let ratio = f64::from(MAX_VIDEO_WIDTH) / f64::from(width);
    let scaled_height = (f64::from(height) * ratio).round() as u32;
    (MAX_VIDEO_WIDTH, scaled_height.max(1))
}

/// Extracts the first video frame as a PNG thumbnail.
/// Returns `Some((width, height))` on success.
pub fn extract_video_thumbnail_to_file(
    video_path: &Path,
    output_png_path: &Path,
) -> Option<(u32, u32)> {
    if !*FFMPEG_INIT_OK.get_or_init(|| ffmpeg::init().is_ok()) {
        return None;
    }

    let mut input_ctx = ffmpeg::format::input(video_path).ok()?;
    let input_stream = input_ctx.streams().best(ffmpeg::media::Type::Video)?;
    let stream_index = input_stream.index();

    let context_decoder =
        ffmpeg::codec::context::Context::from_parameters(input_stream.parameters()).ok()?;
    let mut decoder = context_decoder.decoder().video().ok()?;
    let source_width = decoder.width();
    let source_height = decoder.height();
    if source_width == 0 || source_height == 0 {
        return None;
    }

    let (target_width, target_height) = target_video_size(source_width, source_height);
    let mut scaler = ffmpeg::software::scaling::Context::get(
        decoder.format(),
        source_width,
        source_height,
        ffmpeg::format::Pixel::RGBA,
        target_width,
        target_height,
        ffmpeg::software::scaling::Flags::BILINEAR,
    )
    .ok()?;

    let mut decoded_frame = ffmpeg::util::frame::Video::empty();
    let mut converted = ffmpeg::util::frame::Video::empty();

    for (stream, packet) in input_ctx.packets() {
        if stream.index() != stream_index {
            continue;
        }
        if decoder.send_packet(&packet).is_err() {
            continue;
        }
        if decoder.receive_frame(&mut decoded_frame).is_ok()
            && scaler.run(&decoded_frame, &mut converted).is_ok()
        {
            return write_frame_as_png(&converted, target_width, target_height, output_png_path);
        }
    }

    let _ = decoder.send_eof();
    if decoder.receive_frame(&mut decoded_frame).is_ok()
        && scaler.run(&decoded_frame, &mut converted).is_ok()
    {
        return write_frame_as_png(&converted, target_width, target_height, output_png_path);
    }

    None
}

fn write_frame_as_png(
    frame: &ffmpeg::util::frame::Video,
    width: u32,
    height: u32,
    output_path: &Path,
) -> Option<(u32, u32)> {
    let w = usize::try_from(width).ok()?;
    let h = usize::try_from(height).ok()?;
    let stride = frame.stride(0);
    let row_bytes = w.checked_mul(4)?;
    if stride < row_bytes {
        return None;
    }
    let src_plane = frame.data(0);
    if src_plane.len() < stride.checked_mul(h)? {
        return None;
    }

    let mut packed = vec![0u8; row_bytes.checked_mul(h)?];
    for row in 0..h {
        let src_start = row.checked_mul(stride)?;
        let src_end = src_start.checked_add(row_bytes)?;
        let dst_start = row.checked_mul(row_bytes)?;
        let dst_end = dst_start.checked_add(row_bytes)?;
        packed[dst_start..dst_end].copy_from_slice(&src_plane[src_start..src_end]);
    }

    let img = RgbaImage::from_vec(width, height, packed)?;
    img.save(output_path).ok()?;
    Some((width, height))
}
