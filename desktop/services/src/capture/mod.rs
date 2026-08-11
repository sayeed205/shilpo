pub mod backend;
pub mod types;

pub use backend::create_backend;
pub use types::{CaptureIntent, CaptureOutcome, Frame, FrameFormat, Rect, Region};

/// Capture a single frame from the specified output or primary output
pub fn capture_frame(output: Option<&str>) -> anyhow::Result<Frame> {
    let mut backend = create_backend()?;
    backend.capture_frame(output)
}

/// Crop a region from an RgbaImage
pub fn crop_image(img: &image::RgbaImage, region: Region) -> image::RgbaImage {
    let x = region.x.max(0) as u32;
    let y = region.y.max(0) as u32;
    if x >= img.width() || y >= img.height() || region.is_empty() {
        return image::RgbaImage::new(0, 0);
    }
    let width = region.width.min(img.width() - x);
    let height = region.height.min(img.height() - y);
    image::imageops::crop_imm(img, x, y, width, height).to_image()
}

/// Copy image to clipboard
pub fn copy_image_to_clipboard(_img: &image::RgbaImage) -> anyhow::Result<()> {
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|error| anyhow::anyhow!("initializing clipboard: {error}"))?;
    let image = arboard::ImageData {
        width: _img.width() as usize,
        height: _img.height() as usize,
        bytes: std::borrow::Cow::Owned(_img.as_raw().clone()),
    };
    clipboard
        .set_image(image)
        .map_err(|error| anyhow::anyhow!("copying screenshot to clipboard: {error}"))?;
    Ok(())
}

/// Convert a captured frame into an owned RGBA image.
pub fn frame_to_rgba(frame: &Frame) -> anyhow::Result<image::RgbaImage> {
    let bytes = &frame.data;
    let expected = frame.width as usize * frame.height as usize * 4;
    if bytes.len() < expected {
        anyhow::bail!(
            "capture buffer is truncated: {} < {expected} bytes",
            bytes.len()
        );
    }
    let mut rgba = vec![0; expected];
    for (src, dst) in bytes[..expected]
        .chunks_exact(4)
        .zip(rgba.chunks_exact_mut(4))
    {
        match frame.format {
            FrameFormat::Argb8888 | FrameFormat::Xrgb8888 => {
                dst.copy_from_slice(&[
                    src[2],
                    src[1],
                    src[0],
                    if matches!(frame.format, FrameFormat::Argb8888) {
                        src[3]
                    } else {
                        255
                    },
                ]);
            }
            FrameFormat::Abgr8888 | FrameFormat::Xbgr8888 => {
                dst.copy_from_slice(&[
                    src[0],
                    src[1],
                    src[2],
                    if matches!(frame.format, FrameFormat::Abgr8888) {
                        src[3]
                    } else {
                        255
                    },
                ]);
            }
        }
    }
    image::RgbaImage::from_raw(frame.width, frame.height, rgba)
        .ok_or_else(|| anyhow::anyhow!("invalid capture dimensions"))
}
