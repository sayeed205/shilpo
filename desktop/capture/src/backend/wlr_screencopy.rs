use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::backend::CaptureBackend;
use crate::types::{Frame, RecordingSource, StreamConfig};

pub struct WlrScreencopyBackend {
    streaming: Arc<AtomicBool>,
}

impl WlrScreencopyBackend {
    pub fn new() -> anyhow::Result<Self> {
        let _conn = wayland_client::Connection::connect_to_env()
            .map_err(|e| anyhow::anyhow!("Failed to connect to Wayland display: {e}"))?;

        anyhow::bail!(
            "zwlr-screencopy-manager-v1 is present but the native capture session is not initialized"
        )
    }
}

impl CaptureBackend for WlrScreencopyBackend {
    fn capture_frame(&mut self, _output: Option<&str>) -> anyhow::Result<Frame> {
        anyhow::bail!("wlr screencopy backend is unavailable")
    }

    fn start_stream(
        &mut self,
        source: &RecordingSource,
        config: &StreamConfig,
    ) -> anyhow::Result<crossbeam_channel::Receiver<Frame>> {
        let _ = (config, source);
        anyhow::bail!("wlr screencopy backend is unavailable")
    }

    fn enumerate_sources(&self) -> anyhow::Result<Vec<RecordingSource>> {
        anyhow::bail!("wlr screencopy backend is unavailable")
    }

    fn stop_stream(&mut self) {
        self.streaming.store(false, Ordering::SeqCst);
    }
}
