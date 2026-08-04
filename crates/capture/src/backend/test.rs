use super::{CaptureBackend, CaptureError, CapturedFrame, OutputInfo, OutputTransform};
use crate::Region;
use image::{Rgba, RgbaImage};

/// An in-memory capture backend that synthesizes deterministic test frames.
///
/// This is the production seam used by the test suite and by integration
/// tests that must run headless. Each output renders a colored gradient
/// canvas with a corner badge naming the output, so tests can assert that
/// pixels come from the right source.
pub struct TestCaptureBackend {
    outputs: Vec<OutputInfo>,
    /// If set, `capture_output` fails with this error.
    pub fail_with: Option<CaptureError>,
}

impl TestCaptureBackend {
    pub fn new() -> Self {
        Self {
            outputs: vec![
                OutputInfo {
                    name: "TEST-1".into(),
                    logical_position: (0, 0),
                    logical_size: (1600, 900),
                    scale: 1.0,
                    transform: OutputTransform::Normal,
                    physical_size: (1600, 900),
                },
                OutputInfo {
                    name: "TEST-2".into(),
                    logical_position: (1600, 0),
                    logical_size: (1920, 1080),
                    scale: 2.0,
                    transform: OutputTransform::Normal,
                    physical_size: (3840, 2160),
                },
            ],
            fail_with: None,
        }
    }

    /// Build a backend with a custom output set (including negative origins
    /// and rotations) for geometry tests.
    pub fn with_outputs(outputs: Vec<OutputInfo>) -> Self {
        Self {
            outputs,
            fail_with: None,
        }
    }

    /// Synthesize the frame for an output: a vertical gradient whose colors
    /// encode the output index and size.
    fn synthesize(&self, output: &OutputInfo) -> RgbaImage {
        let (w, h) = (output.physical_size.0, output.physical_size.1);
        let mut img = RgbaImage::new(w, h);
        let index = self
            .outputs
            .iter()
            .position(|o| o.name == output.name)
            .unwrap_or(0) as u8;
        for (x, y, px) in img.enumerate_pixels_mut() {
            let r = (x as f64 / w.max(1) as f64 * 255.0) as u8;
            let g = (y as f64 / h.max(1) as f64 * 255.0) as u8;
            let b = 40 + index * 50;
            *px = Rgba([r, g, b, 255]);
        }
        // Badge the top-left corner so origin/transform tests can locate it.
        for (x, y) in (0..16u32).flat_map(|x| (0..16u32).map(move |y| (x, y))) {
            img.put_pixel(x, y, Rgba([255, 0, 0, 255]));
        }
        img
    }
}

impl Default for TestCaptureBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptureBackend for TestCaptureBackend {
    fn name(&self) -> &'static str {
        "memory"
    }

    fn protocol_name(&self) -> &'static str {
        "memory-test"
    }

    fn outputs(&self) -> Vec<OutputInfo> {
        self.outputs.clone()
    }

    fn capture_output(
        &self,
        output: &str,
        _with_cursor: bool,
    ) -> Result<CapturedFrame, CaptureError> {
        if let Some(err) = &self.fail_with {
            return Err(match err {
                CaptureError::Unavailable(m) => CaptureError::Unavailable(m.clone()),
                CaptureError::OutputGone(m) => CaptureError::OutputGone(m.clone()),
                CaptureError::Rejected(m) => CaptureError::Rejected(m.clone()),
                CaptureError::Buffer(m) => CaptureError::Buffer(m.clone()),
                CaptureError::Protocol(m) => CaptureError::Protocol(m.clone()),
            });
        }
        let info = self
            .outputs
            .iter()
            .find(|o| o.name == output)
            .ok_or_else(|| CaptureError::OutputGone(format!("unknown output {output}")))?
            .clone();
        let raw = self.synthesize(&info);
        let image = if info.transform == OutputTransform::Normal {
            raw
        } else {
            info.transform.apply_to_image(&raw)
        };
        Ok(CapturedFrame {
            image,
            transform: info.transform,
            protocol: self.protocol_name(),
        })
    }

    fn capture_region(
        &self,
        output: &str,
        region: &Region,
        with_cursor: bool,
    ) -> Result<CapturedFrame, CaptureError> {
        // Mirror the default trait implementation so tests exercise the same
        // clamping path used by the production backend.
        let frame = self.capture_output(output, with_cursor)?;
        let (w, h) = (frame.image.width() as f64, frame.image.height() as f64);
        let x0 = region.x.max(0.0).min(w - 1.0) as u32;
        let y0 = region.y.max(0.0).min(h - 1.0) as u32;
        let x1 = (region.x + region.width).max(0.0).min(w) as u32;
        let y1 = (region.y + region.height).max(0.0).min(h) as u32;
        if x1 <= x0 || y1 <= y0 {
            return Err(CaptureError::Rejected("empty region".into()));
        }
        let image = image::imageops::crop_imm(&frame.image, x0, y0, x1 - x0, y1 - y0).to_image();
        Ok(CapturedFrame {
            image,
            transform: frame.transform,
            protocol: frame.protocol,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_lists_outputs() {
        let backend = TestCaptureBackend::new();
        assert_eq!(backend.outputs().len(), 2);
        assert_eq!(backend.outputs()[1].logical_size, (1920, 1080));
        assert_eq!(backend.outputs()[1].scale, 2.0);
    }

    #[test]
    fn test_backend_captures_expected_dimensions() {
        let backend = TestCaptureBackend::new();
        let frame = backend.capture_output("TEST-1", true).unwrap();
        assert_eq!(frame.image.dimensions(), (1600, 900));
        let frame = backend.capture_output("TEST-2", true).unwrap();
        // Physical size at scale 2, then transform is normal.
        assert_eq!(frame.image.dimensions(), (3840, 2160));
    }

    #[test]
    fn test_backend_region_crops_and_clamps() {
        let backend = TestCaptureBackend::new();
        let region = Region {
            x: 100.0,
            y: 50.0,
            width: 200.0,
            height: 100.0,
        };
        let frame = backend.capture_region("TEST-1", &region, false).unwrap();
        assert_eq!(frame.image.dimensions(), (200, 100));

        let region = Region {
            x: -50.0,
            y: 0.0,
            width: 200.0,
            height: 900.0,
        };
        let frame = backend.capture_region("TEST-1", &region, false).unwrap();
        assert_eq!(frame.image.dimensions(), (150, 900));
    }

    #[test]
    fn test_backend_missing_output() {
        let backend = TestCaptureBackend::new();
        assert!(backend.capture_output("NOPE", false).is_err());
    }

    #[test]
    fn test_backend_injected_failure() {
        let mut backend = TestCaptureBackend::new();
        backend.fail_with = Some(CaptureError::Rejected("denied".into()));
        assert!(backend.capture_output("TEST-1", false).is_err());
    }
}
