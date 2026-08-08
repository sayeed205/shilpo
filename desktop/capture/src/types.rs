use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// What to do with a captured screenshot
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CaptureIntent {
    Clipboard,
    Annotation,
    Ocr,
    Menu,
}

/// Result/outcome of a screenshot capture operation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum CaptureOutcome {
    Accepted,
    Copied,
    Saved(PathBuf),
    TextCopied(String),
}

/// Rectangular area definition
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }
}

pub type Region = Rect;

/// Pixel formats supported by frame buffers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameFormat {
    Argb8888,
    Xrgb8888,
    Xbgr8888,
    Abgr8888,
}

/// Frame buffer storage (SHM memory or DMA-BUF zero-copy descriptor)
#[derive(Debug)]
pub enum FrameData {
    Shm(Vec<u8>),
    DmaBuf {
        fd: OwnedFd,
        stride: u32,
        modifier: u64,
    },
}

/// A single captured video frame
#[derive(Debug)]
pub struct Frame {
    pub data: FrameData,
    pub width: u32,
    pub height: u32,
    pub format: FrameFormat,
    pub timestamp: Instant,
}

/// Audio capture source selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum AudioSource {
    #[default]
    System,
    None,
}

/// Video recording capture target output source
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecordingSource {
    pub name: String,
    pub label: String,
}

impl RecordingSource {
    pub fn primary() -> Self {
        Self {
            name: "primary".to_string(),
            label: "Primary Output".to_string(),
        }
    }

    pub fn new(name: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            label: label.into(),
        }
    }
}

/// Request parameters for starting a recording
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecordingRequest {
    pub source: RecordingSource,
    pub audio: AudioSource,
}

/// Recording control commands sent to the controller/pipeline
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RecordingCommand {
    Start(RecordingRequest),
    Stop,
    Pause,
    Resume,
    Cancel,
    Status,
}

impl RecordingCommand {
    pub fn validate(&self, current_state: &RecordingState) -> Result<(), &'static str> {
        match self {
            RecordingCommand::Start(_) => {
                if matches!(
                    current_state,
                    RecordingState::Idle | RecordingState::Selecting
                ) {
                    Ok(())
                } else {
                    Err("recording session already active")
                }
            }
            RecordingCommand::Stop => {
                if matches!(
                    current_state,
                    RecordingState::Recording { .. } | RecordingState::Paused { .. }
                ) {
                    Ok(())
                } else {
                    Err("no active recording session to stop")
                }
            }
            RecordingCommand::Pause => {
                if matches!(current_state, RecordingState::Recording { .. }) {
                    Ok(())
                } else {
                    Err("recording is not active")
                }
            }
            RecordingCommand::Resume => {
                if matches!(current_state, RecordingState::Paused { .. }) {
                    Ok(())
                } else {
                    Err("recording is not paused")
                }
            }
            RecordingCommand::Cancel => Ok(()),
            RecordingCommand::Status => Ok(()),
        }
    }
}

/// State of active screen recording session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum RecordingState {
    #[default]
    Idle,
    Selecting,
    Recording {
        elapsed: Duration,
    },
    Paused {
        elapsed: Duration,
    },
    Finalizing,
}

impl RecordingState {
    pub fn is_stoppable(&self) -> bool {
        matches!(
            self,
            RecordingState::Recording { .. } | RecordingState::Paused { .. }
        )
    }

    pub fn is_recording(&self) -> bool {
        matches!(self, RecordingState::Recording { .. })
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self,
            RecordingState::Recording { .. }
                | RecordingState::Paused { .. }
                | RecordingState::Finalizing
        )
    }

    pub fn elapsed(&self) -> Duration {
        match self {
            RecordingState::Recording { elapsed } | RecordingState::Paused { elapsed } => *elapsed,
            _ => Duration::ZERO,
        }
    }
}

/// Events emitted by the recording pipeline
#[derive(Debug, Clone)]
pub enum RecordingEvent {
    StateChanged(RecordingState),
    ScreenshotCaptured { image: image::RgbaImage },
    Completed { path: PathBuf, duration: Duration },
    Error(String),
    FrameDropped,
}

/// Quality preset
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Quality {
    Low,
    #[default]
    Balanced,
    High,
    Lossless,
}

impl Quality {
    pub fn ffmpeg_params(&self) -> (u32, &'static str) {
        match self {
            Quality::Low => (28, "fast"),
            Quality::Balanced => (23, "medium"),
            Quality::High => (18, "slow"),
            Quality::Lossless => (0, "ultrafast"),
        }
    }
}

/// Recording stream configuration settings
#[derive(Debug, Clone)]
pub struct StreamConfig {
    pub framerate: u32,
    pub quality: Quality,
    pub output_dir: PathBuf,
}

impl Default for StreamConfig {
    fn default() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let output_dir = std::env::var_os("XDG_VIDEOS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("Videos"))
            .join("recordings");
        Self {
            framerate: 30,
            quality: Quality::Balanced,
            output_dir,
        }
    }
}
