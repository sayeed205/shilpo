pub mod backend;
pub mod pipeline;
pub mod recorder;
pub mod sources;
pub mod types;

pub use types::{
    AudioSource, CaptureIntent, Codec, Container, Frame, FrameData, FrameFormat, HwAccel, Quality,
    Rect, RecordingCommand, RecordingEvent, RecordingRequest, RecordingSource, RecordingState,
    StreamConfig,
};

pub use backend::create_backend;
pub use recorder::RecordingController;
pub use sources::enumerate_sources;

/// Capture a single frame from the specified output or primary output
pub fn capture_frame(output: Option<&str>) -> anyhow::Result<Frame> {
    let mut backend = create_backend()?;
    backend.capture_frame(output)
}
