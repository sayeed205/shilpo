use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// The intent that drives a capture selector session.
///
/// The selector is opened with one intent; every intent leads to a distinct
/// outcome pipeline (clipboard, annotation editor, OCR, or recording start).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureIntent {
    /// Select a rectangle and copy the PNG to the clipboard immediately on
    /// mouse release. Nothing is saved to disk.
    Clipboard,
    /// Select a region and open the annotation editor.
    Annotation,
    /// Select a region and run OCR, copying the recognized text.
    Ocr,
    /// Open the capture menu (selection shape chooser) before selecting.
    Menu,
}

/// The region selected on an output, in logical coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct Region {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Region {
    pub fn is_empty(&self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }
}

/// The shape used for the selection marquee.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SelectionShape {
    /// Axis-aligned rectangle selection.
    #[default]
    Rectangle,
    /// Elliptical (freeform) selection.
    Ellipse,
}

/// The result carried by a successful capture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct CaptureResult {
    /// The path the PNG was saved to, when the intent saves a file.
    pub saved_path: Option<PathBuf>,
    /// Whether the PNG was placed on the regular clipboard.
    pub to_clipboard: bool,
    /// The recognized text, when the intent was OCR.
    pub ocr_text: Option<String>,
    /// The recording state, when the intent was a recording.
    pub recording: Option<RecordingState>,
}

/// The outcome of a capture selector session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "payload")]
pub enum CaptureOutcome {
    /// The capture completed and was delivered (clipboard and/or file).
    Success(CaptureResult),
    /// The user cancelled (Escape, discard). A normal outcome, never an error.
    Cancelled,
    /// No usable capture backend is available (no Wayland capture protocol).
    Unavailable,
    /// The Wayland capture failed before a frame was produced.
    CaptureFailed(String),
    /// The frame could not be encoded as PNG.
    EncodingFailed(String),
    /// The native annotation editor could not be opened.
    EditorUnavailable(String),
    /// The PNG could not be written to the clipboard.
    ClipboardFailed(String),
    /// The OCR engine failed.
    OcrFailed(String),
    /// OCR ran but recognized no text.
    OcrEmpty,
}

impl CaptureOutcome {
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Success(_))
    }

    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }

    pub fn error_code(&self) -> Option<&'static str> {
        match self {
            Self::Success(_) | Self::Cancelled => None,
            Self::Unavailable => Some("capture_unavailable"),
            Self::CaptureFailed(_) => Some("capture_failed"),
            Self::EncodingFailed(_) => Some("capture_encode_failed"),
            Self::EditorUnavailable(_) => Some("annotation_editor_unavailable"),
            Self::ClipboardFailed(_) => Some("clipboard_failed"),
            Self::OcrFailed(_) => Some("ocr_failed"),
            Self::OcrEmpty => Some("ocr_empty"),
        }
    }
}

/// The compositor surface a recording is captured from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "name")]
pub enum RecordingSource {
    /// Capture an entire output. An empty name, or `"primary"`, selects the
    /// focused/primary output automatically.
    Output(String),
    /// Capture a single window matched by its compositor metadata.
    Window {
        /// Stable identifier from `ext-foreign-toplevel-list`.
        identifier: String,
        /// Application identifier reported by the compositor.
        app_id: String,
        /// Window title reported by the compositor.
        title: String,
    },
}

/// One output advertised by the recorder's Wayland source catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordableOutput {
    pub name: String,
    pub make: Option<String>,
    pub model: Option<String>,
    pub logical_size: (i32, i32),
}

impl RecordableOutput {
    pub fn source(&self) -> RecordingSource {
        RecordingSource::Output(self.name.clone())
    }
}

/// One uniquely identified toplevel advertised by the recorder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordableWindow {
    pub identifier: String,
    pub app_id: String,
    pub title: String,
}

impl RecordableWindow {
    pub fn source(&self) -> RecordingSource {
        RecordingSource::Window {
            identifier: self.identifier.clone(),
            app_id: self.app_id.clone(),
            title: self.title.clone(),
        }
    }
}

/// Point-in-time list of sources obtained from the same Wayland protocols the
/// recording worker uses.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RecordingSourceCatalog {
    pub outputs: Vec<RecordableOutput>,
    pub windows: Vec<RecordableWindow>,
}

impl RecordingSource {
    /// A source that requests the default output.
    pub fn primary() -> Self {
        Self::Output("primary".into())
    }

    /// Whether this source refers to a window.
    pub fn is_window(&self) -> bool {
        matches!(self, Self::Window { .. })
    }
}

/// Everything required to start one recording session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RecordingRequest {
    /// The output or window to capture.
    pub source: RecordingSource,
    /// Audio inputs to include in the recording.
    pub audio: RecordingAudio,
    /// Optional explicit destination. The configured recordings directory is
    /// used when this is absent.
    pub path: Option<PathBuf>,
}

/// Audio inputs included in a recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RecordingAudio {
    /// Resolve inputs from the current shell recording configuration.
    #[default]
    Configured,
    /// Record video without audio.
    None,
    /// Record the default desktop sink monitor.
    Desktop,
    /// Record the default microphone source.
    Microphone,
    /// Mix desktop and microphone input into one audio stream.
    DesktopAndMicrophone,
}

/// Commands accepted by the recording controller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "command", content = "payload")]
pub enum RecordingCommand {
    /// Begin a recording.
    Start(RecordingRequest),
    /// Pause the active recording.
    Pause,
    /// Resume the paused recording.
    Resume,
    /// Stop and finalize the recording.
    Stop,
    /// Cancel and discard the recording.
    Cancel,
    /// Query the current state.
    Status,
}

impl RecordingCommand {
    /// Check whether this command is valid for the current lifecycle state.
    pub fn validate(&self, state: &RecordingState) -> Result<(), RecordingCommandError> {
        let valid = match self {
            Self::Start(_) => !state.is_busy() || matches!(state, RecordingState::Selecting),
            Self::Pause => matches!(state, RecordingState::Recording { .. }),
            Self::Resume => matches!(state, RecordingState::Paused { .. }),
            Self::Stop => matches!(
                state,
                RecordingState::Recording { .. } | RecordingState::Paused { .. }
            ),
            Self::Cancel => matches!(
                state,
                RecordingState::Selecting
                    | RecordingState::Starting { .. }
                    | RecordingState::Recording { .. }
                    | RecordingState::Paused { .. }
            ),
            Self::Status => true,
        };
        if valid {
            Ok(())
        } else {
            Err(RecordingCommandError {
                command: self.name(),
                state: state.summary(),
            })
        }
    }

    /// Stable human-readable command name used by diagnostics.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Start(_) => "start",
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Stop => "stop",
            Self::Cancel => "cancel",
            Self::Status => "status",
        }
    }
}

/// A recording command that is invalid for the current lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordingCommandError {
    pub command: &'static str,
    pub state: &'static str,
}

impl std::fmt::Display for RecordingCommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "cannot {} recording while it is {}",
            self.command, self.state
        )
    }
}

impl std::error::Error for RecordingCommandError {}

/// Runtime counters reported by the recording worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct RecordingStats {
    /// Number of frames the compositor delivered to the encoder.
    pub captured_frames: u64,
    /// Number of frames written to the output stream.
    pub encoded_frames: u64,
    /// Number of frames dropped by the frame-rate limiter.
    pub dropped_frames: u64,
    /// Number of audio samples fed into the audio encoder.
    pub audio_samples: u64,
    /// Bytes written to the output container so far.
    pub bytes_written: u64,
}

/// The state machine of the recording controller.
///
/// Transitions:
/// `Idle -> Selecting -> Starting -> Recording <-> Paused -> Finalizing -> Finished`
/// plus explicit `Cancelled` and `Failed` terminal states.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case", tag = "state", content = "payload")]
pub enum RecordingState {
    /// No recording is active.
    #[default]
    Idle,
    /// A recording was requested and the capture source is being resolved
    /// (output/window lookup) before the pipeline starts.
    Selecting,
    /// The pipeline is starting: Wayland capture, VAAPI encoder, muxer, and
    /// audio graph are being initialized.
    Starting {
        /// The output file that will be written.
        path: PathBuf,
    },
    /// A recording is active.
    Recording {
        /// Elapsed wall-clock time.
        elapsed: Duration,
        /// The file being written.
        path: PathBuf,
        /// The compositor surface being captured.
        source: RecordingSource,
        /// Runtime counters.
        stats: RecordingStats,
    },
    /// The pipeline is paused.
    Paused {
        /// Elapsed wall-clock time before pausing.
        elapsed: Duration,
        /// The file being written.
        path: PathBuf,
        /// The compositor surface being captured.
        source: RecordingSource,
        /// Runtime counters at the latest captured frame.
        stats: RecordingStats,
    },
    /// The pipeline is flushing the final buffers and sealing the container.
    Finalizing {
        /// The file being finalized.
        path: PathBuf,
    },
    /// The recording finished and was written to `path`.
    Finished {
        /// The finalized file.
        path: PathBuf,
        /// Total recorded duration.
        duration: Duration,
        /// A non-fatal reason the session ended earlier than requested.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        warning: Option<String>,
    },
    /// The recording was cancelled; nothing was kept.
    Cancelled,
    /// The recording failed. `path` may contain a preserved partial recording.
    Failed {
        /// The reason the recording failed.
        reason: String,
        /// A partial file that may be recoverable, if the failure happened
        /// after the file was opened.
        partial_path: Option<PathBuf>,
    },
}

impl RecordingState {
    /// True while a recording is producing a file (paused or not).
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Recording { .. } | Self::Paused { .. })
    }

    /// True when the current session can be stopped or cancelled.
    pub fn is_stoppable(&self) -> bool {
        matches!(
            self,
            Self::Starting { .. } | Self::Recording { .. } | Self::Paused { .. }
        )
    }

    /// True while a recording operation is in flight; new `Start` commands
    /// are rejected in these states.
    pub fn is_busy(&self) -> bool {
        matches!(
            self,
            Self::Selecting
                | Self::Starting { .. }
                | Self::Recording { .. }
                | Self::Paused { .. }
                | Self::Finalizing { .. }
        )
    }

    /// Command used by the shell's single start/stop shortcut.
    pub fn toggle_command(&self) -> Option<RecordingCommand> {
        match self {
            Self::Selecting | Self::Starting { .. } => Some(RecordingCommand::Cancel),
            Self::Recording { .. } | Self::Paused { .. } => Some(RecordingCommand::Stop),
            _ => None,
        }
    }

    pub fn elapsed(&self) -> Duration {
        match self {
            Self::Recording { elapsed, .. } | Self::Paused { elapsed, .. } => *elapsed,
            _ => Duration::ZERO,
        }
    }

    pub fn summary(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Selecting => "selecting",
            Self::Starting { .. } => "starting",
            Self::Recording { .. } => "recording",
            Self::Paused { .. } => "paused",
            Self::Finalizing { .. } => "finalizing",
            Self::Finished { .. } => "finished",
            Self::Cancelled => "cancelled",
            Self::Failed { .. } => "failed",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_intent_helpers() {
        assert_ne!(CaptureIntent::Clipboard, CaptureIntent::Annotation);
    }

    #[test]
    fn test_region_geometry() {
        let region = Region {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 50.0,
        };
        assert!(!region.is_empty());
        assert!(region.contains(15.0, 25.0));
        assert!(!region.contains(5.0, 5.0));
    }

    #[test]
    fn test_outcome_helpers() {
        assert!(CaptureOutcome::Cancelled.is_cancelled());
        assert_eq!(
            CaptureOutcome::Unavailable.error_code(),
            Some("capture_unavailable")
        );
    }

    #[test]
    fn test_state_machine_helpers() {
        let recording = RecordingState::Recording {
            elapsed: Duration::ZERO,
            path: PathBuf::from("/tmp/x.webm"),
            source: RecordingSource::primary(),
            stats: RecordingStats::default(),
        };
        assert!(recording.is_active());
        assert!(recording.is_busy());
        assert!(!RecordingState::Idle.is_active());
        assert!(!RecordingState::Idle.is_busy());
        let selecting = RecordingState::Selecting;
        assert!(!selecting.is_active());
        assert!(selecting.is_busy());
    }

    #[test]
    fn recording_commands_enforce_lifecycle_transitions() {
        assert!(
            RecordingCommand::Stop
                .validate(&RecordingState::Idle)
                .is_err()
        );
        assert!(
            RecordingCommand::Pause
                .validate(&RecordingState::Idle)
                .is_err()
        );

        let recording = RecordingState::Recording {
            elapsed: Duration::ZERO,
            path: PathBuf::from("/tmp/x.webm"),
            source: RecordingSource::primary(),
            stats: RecordingStats::default(),
        };
        assert!(RecordingCommand::Pause.validate(&recording).is_ok());
        assert!(RecordingCommand::Stop.validate(&recording).is_ok());
        assert!(RecordingCommand::Resume.validate(&recording).is_err());

        let paused = RecordingState::Paused {
            elapsed: Duration::ZERO,
            path: PathBuf::from("/tmp/x.webm"),
            source: RecordingSource::primary(),
            stats: RecordingStats::default(),
        };
        assert!(RecordingCommand::Resume.validate(&paused).is_ok());
        assert_eq!(paused.toggle_command(), Some(RecordingCommand::Stop));
    }
}
