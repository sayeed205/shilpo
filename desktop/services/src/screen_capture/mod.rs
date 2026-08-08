use anyhow::Result;
use shilpo_capture::{capture_frame, copy_image_to_clipboard, create_backend, frame_to_rgba};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::watch;

use crate::polled::PolledService;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenshotMode {
    Fullscreen,
    Region,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScreenCaptureInfo {
    pub is_recording: bool,
    pub available: bool,
}

fn query_availability() -> bool {
    create_backend()
        .map(|b| {
            b.enumerate_sources()
                .map(|s| !s.is_empty())
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

pub struct ScreenCaptureService {
    polled: PolledService<ScreenCaptureInfo>,
}

impl ScreenCaptureService {
    pub fn new() -> Result<Self> {
        let initial = ScreenCaptureInfo {
            is_recording: false,
            available: query_availability(),
        };

        let polled = PolledService::new(
            initial,
            Duration::from_secs(3),
            None,
            |current: &ScreenCaptureInfo| -> Result<ScreenCaptureInfo, std::convert::Infallible> {
                let mut updated = current.clone();
                updated.available = query_availability();
                Ok(updated)
            },
        );

        Ok(Self { polled })
    }

    pub fn new_offline() -> Self {
        Self {
            polled: PolledService::new_offline(ScreenCaptureInfo {
                is_recording: false,
                available: false,
            }),
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<ScreenCaptureInfo> {
        self.polled.subscribe()
    }

    pub fn info(&self) -> ScreenCaptureInfo {
        self.polled.get()
    }

    pub fn take_screenshot(&self, _mode: ScreenshotMode, output_path: Option<PathBuf>) -> bool {
        let info_state = self.polled.get();
        if !info_state.available {
            return false;
        }

        let Ok(frame) = capture_frame(None) else {
            return false;
        };
        let Ok(image) = frame_to_rgba(&frame) else {
            return false;
        };
        if let Some(path) = output_path {
            image.save(path).is_ok()
        } else {
            copy_image_to_clipboard(&image).is_ok()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screen_capture_offline() {
        let service = ScreenCaptureService::new_offline();
        let info = service.info();
        assert!(!info.available);
        assert!(!service.take_screenshot(ScreenshotMode::Fullscreen, None));
    }
}
