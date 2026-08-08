use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::backend::CaptureBackend;
use crate::types::{Frame, FrameData, FrameFormat, RecordingSource, StreamConfig};

pub struct TestBackend {
    streaming: Arc<AtomicBool>,
}

impl TestBackend {
    pub fn new() -> Self {
        Self {
            streaming: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn generate_solid_frame(
        width: u32,
        height: u32,
        color_r: u8,
        color_g: u8,
        color_b: u8,
    ) -> Frame {
        let mut data = vec![0u8; (width * height * 4) as usize];
        for pixel in data.chunks_exact_mut(4) {
            pixel[0] = color_b;
            pixel[1] = color_g;
            pixel[2] = color_r;
            pixel[3] = 255;
        }

        Frame {
            data: FrameData::Shm(data),
            width,
            height,
            format: FrameFormat::Argb8888,
            timestamp: Instant::now(),
        }
    }
}

impl Default for TestBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptureBackend for TestBackend {
    fn capture_frame(&mut self, _output: Option<&str>) -> anyhow::Result<Frame> {
        Ok(Self::generate_solid_frame(1920, 1080, 100, 150, 200))
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
            let mut counter: u8 = 0;
            while streaming.load(Ordering::SeqCst) {
                let frame = Self::generate_solid_frame(1280, 720, counter, 120, 200 - counter);
                counter = counter.wrapping_add(5);

                if tx.send(frame).is_err() {
                    break;
                }
                thread::sleep(frame_delay);
            }
        });

        Ok(rx)
    }

    fn enumerate_sources(&self) -> anyhow::Result<Vec<RecordingSource>> {
        Ok(vec![
            RecordingSource::new("HEADLESS-1", "Headless Display 1"),
            RecordingSource::new("HEADLESS-2", "Headless Display 2"),
        ])
    }

    fn stop_stream(&mut self) {
        self.streaming.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_single_frame() {
        let mut backend = TestBackend::new();
        let frame = backend.capture_frame(None).unwrap();
        assert_eq!(frame.width, 1920);
        assert_eq!(frame.height, 1080);
    }

    #[test]
    fn test_backend_stream() {
        let mut backend = TestBackend::new();
        let config = StreamConfig {
            framerate: 30,
            ..Default::default()
        };
        let rx = backend
            .start_stream(&RecordingSource::primary(), &config)
            .unwrap();
        let frame = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(frame.width, 1280);
        backend.stop_stream();
    }
}
