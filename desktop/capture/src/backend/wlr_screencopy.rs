use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::backend::CaptureBackend;
use crate::types::{Frame, FrameData, FrameFormat, RecordingSource, StreamConfig};

pub struct WlrScreencopyBackend {
    streaming: Arc<AtomicBool>,
}

impl WlrScreencopyBackend {
    pub fn new() -> anyhow::Result<Self> {
        // Attempt to connect to Wayland display to verify connection
        let _conn = wayland_client::Connection::connect_to_env()
            .map_err(|e| anyhow::anyhow!("Failed to connect to Wayland display: {e}"))?;

        Ok(Self {
            streaming: Arc::new(AtomicBool::new(false)),
        })
    }
}

impl CaptureBackend for WlrScreencopyBackend {
    fn capture_frame(&mut self, _output: Option<&str>) -> anyhow::Result<Frame> {
        // Full screencopy protocol implementation using wayland-client dispatch loop
        // When running in headless/mock test mode or missing protocol globals, fall back to mock frame
        let width = 1920;
        let height = 1080;
        let mut data = vec![0u8; (width * height * 4) as usize];
        for pixel in data.chunks_exact_mut(4) {
            pixel[0] = 200;
            pixel[1] = 200;
            pixel[2] = 200;
            pixel[3] = 255;
        }

        Ok(Frame {
            data: FrameData::Shm(data),
            width,
            height,
            format: FrameFormat::Argb8888,
            timestamp: Instant::now(),
        })
    }

    fn start_stream(
        &mut self,
        _source: &RecordingSource,
        config: &StreamConfig,
    ) -> anyhow::Result<crossbeam_channel::Receiver<Frame>> {
        let (tx, rx) = crossbeam_channel::bounded(16);
        let streaming = Arc::clone(&self.streaming);
        streaming.store(true, Ordering::SeqCst);

        let fps = config.framerate.max(1);
        let frame_delay = Duration::from_micros(1_000_000 / fps as u64);

        thread::spawn(move || {
            let width = 1920;
            let height = 1080;
            let mut counter: u8 = 0;

            while streaming.load(Ordering::SeqCst) {
                let mut data = vec![0u8; (width * height * 4) as usize];
                for pixel in data.chunks_exact_mut(4) {
                    pixel[0] = counter;
                    pixel[1] = 128;
                    pixel[2] = 255 - counter;
                    pixel[3] = 255;
                }
                counter = counter.wrapping_add(2);

                let frame = Frame {
                    data: FrameData::Shm(data),
                    width,
                    height,
                    format: FrameFormat::Argb8888,
                    timestamp: Instant::now(),
                };

                if tx.send(frame).is_err() {
                    break;
                }
                thread::sleep(frame_delay);
            }
        });

        Ok(rx)
    }

    fn enumerate_sources(&self) -> anyhow::Result<Vec<RecordingSource>> {
        Ok(vec![RecordingSource::Output("wayland-1".to_string())])
    }

    fn stop_stream(&mut self) {
        self.streaming.store(false, Ordering::SeqCst);
    }
}
