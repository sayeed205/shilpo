use std::time::Instant;

use crate::capture::backend::CaptureBackend;
use crate::capture::types::{Frame, FrameFormat};

pub struct TestBackend;

impl TestBackend {
    pub fn new() -> Self {
        Self
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
            data,
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
}
