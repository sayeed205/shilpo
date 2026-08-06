use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenshotMode {
    Fullscreen,
    Region,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordMode {
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
    recording_process: Arc<Mutex<Option<std::process::Child>>>,
}

impl ScreenCaptureService {
    pub fn new() -> Result<Self> {
        let available = Self::detect_backend();
        let initial = ScreenCaptureInfo {
            is_recording: false,
            available,
        };
        let polled = PolledService::new(
            initial,
            Duration::from_secs(3),
            None,
            |current: &ScreenCaptureInfo| -> Result<ScreenCaptureInfo, std::convert::Infallible> {
                let available = Self::detect_backend();
                let mut updated = current.clone();
                updated.available = available;
                Ok(updated)
            },
        );
        Ok(Self {
            polled,
            recording_process: Arc::new(Mutex::new(None)),
        })
    }

    pub fn new_offline() -> Self {
        Self {
            polled: PolledService::new_offline(ScreenCaptureInfo {
                is_recording: false,
                available: false,
            }),
            recording_process: Arc::new(Mutex::new(None)),
        }
    }

    fn detect_backend() -> bool {
        Command::new("grim").arg("-h").output().is_ok()
            || Command::new("wf-recorder").arg("-h").output().is_ok()
    }

    pub fn subscribe(&self) -> watch::Receiver<ScreenCaptureInfo> {
        self.polled.subscribe()
    }

    pub fn info(&self) -> ScreenCaptureInfo {
        self.polled.get()
    }

    pub fn take_screenshot(&self, mode: ScreenshotMode, output_path: Option<PathBuf>) -> bool {
        let info_state = self.polled.get();
        if !info_state.available {
            return false;
        }

        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let default_path = std::env::var("HOME")
            .map(|h| PathBuf::from(h).join(format!("Pictures/screenshot_{}.png", timestamp)))
            .unwrap_or_else(|_| PathBuf::from("screenshot.png"));

        let save_path = output_path.unwrap_or(default_path);

        match mode {
            ScreenshotMode::Fullscreen => {
                let _ = Command::new("grim").arg(&save_path).spawn();
            }
            ScreenshotMode::Region => {
                let _ = Command::new("sh")
                    .args([
                        "-c",
                        &format!("grim -g \"$(slurp)\" {}", save_path.display()),
                    ])
                    .spawn();
            }
        }
        true
    }

    pub fn toggle_recording(&self, audio: bool, mode: RecordMode) -> bool {
        let mut info_state = self.polled.get();
        let mut proc_lock = self.recording_process.lock().unwrap();

        if !info_state.available {
            return false;
        }

        if info_state.is_recording {
            if let Some(mut child) = proc_lock.take() {
                let _ = child.kill();
            }
            info_state.is_recording = false;
        } else {
            let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
            let default_path = std::env::var("HOME")
                .map(|h| PathBuf::from(h).join(format!("Videos/recording_{}.mp4", timestamp)))
                .unwrap_or_else(|_| PathBuf::from("recording.mp4"));

            let mut cmd = String::from("wf-recorder");
            if audio {
                cmd.push_str(" --audio");
            }

            match mode {
                RecordMode::Fullscreen => {
                    cmd.push_str(&format!(" -f {}", default_path.display()));
                }
                RecordMode::Region => {
                    cmd.push_str(&format!(" -g \"$(slurp)\" -f {}", default_path.display()));
                }
            }

            if let Ok(child) = Command::new("sh").args(["-c", &cmd]).spawn() {
                *proc_lock = Some(child);
                info_state.is_recording = true;
            }
        }
        let is_rec = info_state.is_recording;
        self.polled.send_replace(info_state);
        is_rec
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
        assert!(!info.is_recording);
        assert!(!service.take_screenshot(ScreenshotMode::Fullscreen, None));
    }
}
