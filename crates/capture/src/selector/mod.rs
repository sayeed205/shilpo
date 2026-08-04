pub mod editor;
pub mod ocr;

pub use editor::*;
pub use ocr::*;

use crate::backend::{CaptureBackend, capture_for_selector, create_production_backend};
use crate::types::{CaptureIntent, CaptureOutcome, CaptureResult, Region};
use shilpo_config::CaptureConfig;
use std::sync::Arc;

/// Copy an RGBA frame to the compositor clipboard.
pub fn copy_image_to_clipboard(image: &image::RgbaImage) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|error| error.to_string())?;
    let data = arboard::ImageData {
        width: image.width() as usize,
        height: image.height() as usize,
        bytes: std::borrow::Cow::Borrowed(image.as_raw()),
    };
    clipboard.set_image(data).map_err(|error| error.to_string())
}

/// Crop a logical selection to a frame, clamping all coordinates safely.
pub fn crop_image(image: &image::RgbaImage, region: Region) -> image::RgbaImage {
    let x = region.x.max(0.0).floor() as u32;
    let y = region.y.max(0.0).floor() as u32;
    let right = (region.x + region.width).max(0.0).ceil() as u32;
    let bottom = (region.y + region.height).max(0.0).ceil() as u32;
    let x = x.min(image.width());
    let y = y.min(image.height());
    let right = right.min(image.width()).max(x);
    let bottom = bottom.min(image.height()).max(y);
    image::imageops::crop_imm(image, x, y, right - x, bottom - y).to_image()
}

/// Open the selector overlay and execute the capture flow according to `intent`.
pub async fn open_selector(
    intent: CaptureIntent,
    config: &CaptureConfig,
    backend_override: Option<Arc<dyn CaptureBackend>>,
) -> CaptureOutcome {
    let backend: Arc<dyn CaptureBackend> = match backend_override {
        Some(backend) => backend,
        None => match create_production_backend() {
            Some(backend) => Arc::from(backend),
            None => return CaptureOutcome::Unavailable,
        },
    };

    let outputs = backend.outputs();
    let (captured, output_name, output_size) = if let Some(target_output) = outputs.first() {
        let frame = match backend.capture_output(&target_output.name, config.show_pointer) {
            Ok(frame) => frame,
            Err(err) => return CaptureOutcome::CaptureFailed(err.to_string()),
        };
        (
            frame,
            target_output.name.clone(),
            target_output.logical_size,
        )
    } else {
        let frame = match capture_for_selector(None).await {
            Ok(frame) => frame,
            Err(_err) => return CaptureOutcome::Unavailable,
        };
        let size = (frame.image.width(), frame.image.height());
        (frame, "portal-screen".to_string(), size)
    };

    // Calculate crop region (fullscreen default or center selection)
    let selected_region = Region {
        x: 0.0,
        y: 0.0,
        width: output_size.0 as f64,
        height: output_size.1 as f64,
    };

    match intent {
        CaptureIntent::Clipboard => {
            // Immediate clipboard copy without saving
            if let Err(err) = copy_image_to_clipboard(&captured.image) {
                return CaptureOutcome::ClipboardFailed(err.to_string());
            }

            CaptureOutcome::Success(CaptureResult {
                saved_path: None,
                to_clipboard: true,
                ocr_text: None,
                recording: None,
            })
        }
        CaptureIntent::Annotation => {
            let _ = (captured, output_name, selected_region);
            CaptureOutcome::EditorUnavailable(
                "the GPUI annotation surface is not available in this session".into(),
            )
        }
        CaptureIntent::Ocr => OcrEngine::recognize(&captured.image),
        CaptureIntent::Menu => CaptureOutcome::Cancelled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn crop_clamps_selection_to_frame_bounds() {
        let image = RgbaImage::from_pixel(10, 8, Rgba([1, 2, 3, 255]));
        let cropped = crop_image(
            &image,
            Region {
                x: -4.0,
                y: 2.0,
                width: 30.0,
                height: 20.0,
            },
        );
        assert_eq!(cropped.dimensions(), (10, 6));
    }

    #[test]
    fn empty_selection_produces_empty_image() {
        let image = RgbaImage::new(10, 8);
        let cropped = crop_image(&image, Region::default());
        assert_eq!(cropped.dimensions(), (0, 0));
    }
}
