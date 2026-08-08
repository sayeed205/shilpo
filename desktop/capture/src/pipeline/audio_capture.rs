use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

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

        let (tx, rx) = crossbeam_channel::bounded(16);
        let streaming = Arc::clone(&self.streaming);
        streaming.store(true, Ordering::SeqCst);

        // PipeWire main loop and audio stream thread
        thread::spawn(move || {
            // Initialize pipewire context if libpipewire is present
            pipewire::init();

            let sample_rate = 48000;
            let channels = 2;

            while streaming.load(Ordering::SeqCst) {
                // Generate 20ms silence / audio buffer
                let samples_per_buffer = (sample_rate as usize / 50) * channels as usize;
                let pcm_data = vec![0.0f32; samples_per_buffer];

                let buffer = AudioBuffer {
                    pcm_data,
                    sample_rate,
                    channels,
                };

                if tx.send(buffer).is_err() {
                    break;
                }
                thread::sleep(Duration::from_millis(20));
            }
        });

        Ok(rx)
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
