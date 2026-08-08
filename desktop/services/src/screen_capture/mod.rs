use crate::error::ServiceError;
use shilpo_capture::{capture_frame, copy_image_to_clipboard, create_backend, frame_to_rgba};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenshotMode {
    Fullscreen,
    Region,
}

/// Operation-driven non-probing Screen Capture service.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScreenCaptureService;

impl ScreenCaptureService {
    pub fn new() -> Result<Self, ServiceError> {
        Ok(Self)
    }

    pub fn new_offline() -> Self {
        Self
    }

    /// Takes a screenshot of the specified mode and optional output path.
    /// Backend availability is discovered dynamically on operation request.
    pub fn take_screenshot(
        &self,
        _mode: ScreenshotMode,
        output_path: Option<PathBuf>,
    ) -> Result<(), ServiceError> {
        let backend = create_backend().map_err(|err| ServiceError::ScreenCapture {
            message: format!("Failed to create capture backend: {err}"),
        })?;

        let sources = backend
            .enumerate_sources()
            .map_err(|err| ServiceError::ScreenCapture {
                message: format!("Failed to enumerate capture sources: {err}"),
            })?;

        if sources.is_empty() {
            return Err(ServiceError::ScreenCapture {
                message: "No capture sources available".to_string(),
            });
        }

        let frame = capture_frame(None).map_err(|err| ServiceError::ScreenCapture {
            message: format!("Failed to capture frame: {err}"),
        })?;

        let image = frame_to_rgba(&frame).map_err(|err| ServiceError::ScreenCapture {
            message: format!("Failed to convert frame to RGBA: {err}"),
        })?;

        if let Some(path) = output_path {
            image.save(path).map_err(|err| ServiceError::ScreenCapture {
                message: format!("Failed to save screenshot image: {err}"),
            })?;
        } else {
            copy_image_to_clipboard(&image).map_err(|err| ServiceError::ScreenCapture {
                message: format!("Failed to copy screenshot to clipboard: {err}"),
            })?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screen_capture_offline() {
        let service = ScreenCaptureService::new_offline();
        let res = service.take_screenshot(ScreenshotMode::Fullscreen, None);
        assert!(res.is_err());
    }
}

