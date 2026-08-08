use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::types::AudioSource;

pub struct AudioBuffer {
    pub pcm_data: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u32,
}

pub struct PipeWireAudioCapture {
    streaming: Arc<AtomicBool>,
}

impl PipeWireAudioCapture {
    pub fn new() -> Self {
        Self {
            streaming: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn start_capture(
        &mut self,
        source: AudioSource,
    ) -> anyhow::Result<crossbeam_channel::Receiver<AudioBuffer>> {
        if source == AudioSource::None {
            let (_tx, rx) = crossbeam_channel::bounded(1);
            return Ok(rx);
        }

        let _ = source;
        anyhow::bail!("PipeWire audio capture is not initialized")
    }

    pub fn stop(&mut self) {
        self.streaming.store(false, Ordering::SeqCst);
    }
}

impl Default for PipeWireAudioCapture {
    fn default() -> Self {
        Self::new()
    }
}
