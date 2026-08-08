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
    Microphone,
    Both,
    None,
}

/// Video recording capture target source
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RecordingSource {
    Output(String),
    Window(u64),
    Region(Rect),
}

impl RecordingSource {
    pub fn primary() -> Self {
        Self::Output("primary".to_string())
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
                if matches!(current_state, RecordingState::Idle | RecordingState::Selecting) {
                    Ok(())
                } else {
                    Err("recording session already active")
                }
            }
            RecordingCommand::Stop => {
                if matches!(current_state, RecordingState::Recording { .. } | RecordingState::Paused { .. }) {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RecordingState {
    Idle,
    Selecting,
    Recording { elapsed: Duration },
    Paused { elapsed: Duration },
    Finalizing,
}

impl Default for RecordingState {
    fn default() -> Self {
        Self::Idle
    }
}

impl RecordingState {
    pub fn is_stoppable(&self) -> bool {
        matches!(self, RecordingState::Recording { .. } | RecordingState::Paused { .. })
    }

    pub fn is_recording(&self) -> bool {
        matches!(self, RecordingState::Recording { .. })
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

/// Codec options for screen recording
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Codec {
    #[default]
    H264,
    H265,
    Vp9,
    Av1,
}

/// Container format options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Container {
    #[default]
    Mp4,
    Mkv,
    Webm,
}

/// Hardware acceleration setting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum HwAccel {
    #[default]
    Auto,
    Vaapi,
    None,
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

/// Recording stream configuration settings
#[derive(Debug, Clone)]
pub struct StreamConfig {
    pub framerate: u32,
    pub codec: Codec,
    pub container: Container,
    pub hardware_accel: HwAccel,
    pub quality: Quality,
    pub output_dir: PathBuf,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            framerate: 30,
            codec: Codec::H264,
            container: Container::Mp4,
            hardware_accel: HwAccel::Auto,
            quality: Quality::Balanced,
            output_dir: std::env::temp_dir(),
        }
    }
}
