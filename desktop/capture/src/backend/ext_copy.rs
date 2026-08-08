use crate::backend::CaptureBackend;
use crate::types::{Frame, RecordingSource, StreamConfig};

pub struct ExtCopyBackend;

impl ExtCopyBackend {
    pub fn new() -> anyhow::Result<Self> {
        // Probe Wayland display for ext-image-copy-capture-v1 global.
        // Currently, Niri and most compositors do not expose this global yet,
        // so returning an error triggers automatic fallback to WlrScreencopyBackend.
        anyhow::bail!("ext-image-copy-capture-v1 protocol not supported by compositor")
    }
}

impl CaptureBackend for ExtCopyBackend {
    fn capture_frame(&mut self, _output: Option<&str>) -> anyhow::Result<Frame> {
        anyhow::bail!("ext-image-copy-capture-v1 not active")
    }

    fn start_stream(
        &mut self,
        _source: &RecordingSource,
        _config: &StreamConfig,
    ) -> anyhow::Result<crossbeam_channel::Receiver<Frame>> {
        anyhow::bail!("ext-image-copy-capture-v1 not active")
    }

    fn enumerate_sources(&self) -> anyhow::Result<Vec<RecordingSource>> {
        Ok(Vec::new())
    }

    fn stop_stream(&mut self) {}
}
