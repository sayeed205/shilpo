use anyhow::Result;
use shilpo_capture::{capture_frame, create_backend};
use shilpo_config::CaptureConfig;
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

pub struct ScreenCaptureService {
    polled: PolledService<ScreenCaptureInfo>,
    _capture_config: CaptureConfig,
}

impl ScreenCaptureService {
    pub fn new() -> Result<Self> {
        let capture_config = CaptureConfig::default();

        let available = create_backend()
            .map(|b| b.enumerate_sources().map(|s| !s.is_empty()).unwrap_or(false))
            .unwrap_or(false);

        let initial = ScreenCaptureInfo {
            is_recording: false,
            available,
        };

        let polled = PolledService::new(
            initial,
            Duration::from_secs(3),
            None,
            |current: &ScreenCaptureInfo| -> Result<ScreenCaptureInfo, std::convert::Infallible> {
                let available = create_backend()
                    .map(|b| b.enumerate_sources().map(|s| !s.is_empty()).unwrap_or(false))
                    .unwrap_or(false);
                let mut updated = current.clone();
                updated.available = available;
                Ok(updated)
            },
        );

        Ok(Self {
            polled,
            _capture_config: capture_config,
        })
    }

    pub fn new_offline() -> Self {
        let capture_config = CaptureConfig::default();
        Self {
            polled: PolledService::new_offline(ScreenCaptureInfo {
                is_recording: false,
                available: false,
            }),
            _capture_config: capture_config,
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<ScreenCaptureInfo> {
        self.polled.subscribe()
    }

    pub fn info(&self) -> ScreenCaptureInfo {
        self.polled.get()
    }

    pub fn take_screenshot(&self, _mode: ScreenshotMode, _output_path: Option<PathBuf>) -> bool {
        let info_state = self.polled.get();
        if !info_state.available {
            return false;
        }

        let _frame = capture_frame(None);
        true
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
