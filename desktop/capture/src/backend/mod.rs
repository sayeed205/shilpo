pub mod ext_copy;
pub mod test;
pub mod wlr_screencopy;

use crate::types::{Frame, RecordingSource, StreamConfig};

/// Trait abstracting Wayland screen capture protocols
pub trait CaptureBackend: Send {
    /// Capture a single frame (for screenshots)
    fn capture_frame(&mut self, output: Option<&str>) -> anyhow::Result<Frame>;

    /// Start continuous frame capture stream (for recording)
    fn start_stream(
        &mut self,
        source: &RecordingSource,
        config: &StreamConfig,
    ) -> anyhow::Result<crossbeam_channel::Receiver<Frame>>;

    /// Enumerate available capture sources
    fn enumerate_sources(&self) -> anyhow::Result<Vec<RecordingSource>>;

    /// Stop active capture stream
    fn stop_stream(&mut self);
}

/// Factory function to create the appropriate backend
pub fn create_backend() -> anyhow::Result<Box<dyn CaptureBackend>> {
    let backend = wlr_screencopy::WlrScreencopyBackend::new()?;
    tracing::info!("Using wlr-screencopy-unstable-v1 Wayland backend");
    Ok(Box::new(backend))
}
