use anyhow::Result;
use shilpo_capture::{CaptureIntent, backend::create_production_backend, open_selector};
use shilpo_config::CaptureConfig;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenshotMode {
    Fullscreen,
    Region,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenCaptureInfo {
    pub available: bool,
}

pub struct ScreenCaptureService {
    info: Arc<Mutex<ScreenCaptureInfo>>,
    capture_config: CaptureConfig,
}

impl ScreenCaptureService {
    pub fn new() -> Result<Self> {
        let capture_config = CaptureConfig::default();

        let available = create_production_backend()
            .map(|backend| !backend.outputs().is_empty())
            .unwrap_or(false);

        Ok(Self {
            info: Arc::new(Mutex::new(ScreenCaptureInfo { available })),
            capture_config,
        })
    }

    pub fn new_offline() -> Self {
        let capture_config = CaptureConfig::default();

        Self {
            info: Arc::new(Mutex::new(ScreenCaptureInfo { available: false })),
            capture_config,
        }
    }

    pub fn info(&self) -> ScreenCaptureInfo {
        self.info.lock().unwrap().clone()
    }

    pub fn take_screenshot(&self, mode: ScreenshotMode, _output_path: Option<PathBuf>) -> bool {
        let info_lock = self.info.lock().unwrap();
        if !info_lock.available {
            return false;
        }
        drop(info_lock);

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
