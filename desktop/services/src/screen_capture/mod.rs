use anyhow::Result;
use shilpo_capture::{CaptureIntent, backend::create_production_backend, open_selector};
use shilpo_config::CaptureConfig;
use std::path::PathBuf;

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

use crate::polled::PolledService;
use std::time::Duration;
use tokio::sync::watch;

pub struct ScreenCaptureService {
    polled: PolledService<ScreenCaptureInfo>,
    capture_config: CaptureConfig,
}

impl ScreenCaptureService {
    pub fn new() -> Result<Self> {
        let capture_config = CaptureConfig::default();

        let available = create_production_backend()
            .map(|backend| !backend.outputs().is_empty())
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
                let available = create_production_backend()
                    .map(|backend| !backend.outputs().is_empty())
                    .unwrap_or(false);
                let mut updated = current.clone();
                updated.available = available;
                Ok(updated)
            },
        );

        Ok(Self {
            polled,
            capture_config,
        })
    }

    pub fn new_offline() -> Self {
        let capture_config = CaptureConfig::default();
        Self {
            polled: PolledService::new_offline(ScreenCaptureInfo {
                is_recording: false,
                available: false,
            }),
            capture_config,
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<ScreenCaptureInfo> {
        self.polled.subscribe()
    }

    pub fn info(&self) -> ScreenCaptureInfo {
        self.polled.get()
    }

    pub fn take_screenshot(&self, mode: ScreenshotMode, _output_path: Option<PathBuf>) -> bool {
        let info_state = self.polled.get();
        if !info_state.available {
            return false;
        }

        let intent = match mode {
            ScreenshotMode::Fullscreen | ScreenshotMode::Region => CaptureIntent::Clipboard,
        };

        let config = self.capture_config.clone();
        tokio::spawn(async move {
            let _ = open_selector(intent, &config, None).await;
        });

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
