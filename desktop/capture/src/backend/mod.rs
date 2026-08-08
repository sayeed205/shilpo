pub mod test;
pub mod wlr_screencopy;

use crate::types::Frame;

/// Trait abstracting Wayland screen capture protocols
pub trait CaptureBackend: Send {
    /// Capture a single frame (for screenshots)
    fn capture_frame(&mut self, output: Option<&str>) -> anyhow::Result<Frame>;
}

/// Factory function to create the appropriate backend
pub fn create_backend() -> anyhow::Result<Box<dyn CaptureBackend>> {
    let backend = wlr_screencopy::WlrScreencopyBackend::new()?;
    tracing::info!("Using wlr-screencopy-unstable-v1 Wayland backend");
    Ok(Box::new(backend))
}
