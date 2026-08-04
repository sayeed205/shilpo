mod test;
mod wayland;

pub use test::TestCaptureBackend;
pub use wayland::{
    WaylandCaptureBackend, capture_for_selector, capture_via_grim, capture_via_niri,
    capture_via_portal,
};

use crate::Region;
use image::RgbaImage;
use std::fmt;

/// The rotation/reflection transform applied by an output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputTransform {
    Normal,
    R90,
    R180,
    R270,
    Flipped,
    Flipped90,
    Flipped180,
    Flipped270,
}

impl OutputTransform {
    pub fn from_wl(transform: u32) -> Self {
        match transform {
            1 => Self::R90,
            2 => Self::R180,
            3 => Self::R270,
            4 => Self::Flipped,
            5 => Self::Flipped90,
            6 => Self::Flipped180,
            7 => Self::Flipped270,
            _ => Self::Normal,
        }
    }

    /// Apply the transform to an image, returning a new normalized image in
    /// logical orientation.
    pub fn apply_to_image(&self, image: &RgbaImage) -> RgbaImage {
        let w = image.width() as i64;
        let h = image.height() as i64;
        let _ = (w, h);
        let rotated = match self {
            Self::Normal => image.clone(),
            Self::R90 => {
                let mut out = RgbaImage::new(image.height(), image.width());
                for (x, y, px) in image.enumerate_pixels() {
                    out.put_pixel(y, image.width() - 1 - x, *px);
                }
                out
            }
            Self::R180 => {
                let mut out = RgbaImage::new(image.width(), image.height());
                for (x, y, px) in image.enumerate_pixels() {
                    out.put_pixel(image.width() - 1 - x, image.height() - 1 - y, *px);
                }
                out
            }
            Self::R270 => {
                let mut out = RgbaImage::new(image.height(), image.width());
                for (x, y, px) in image.enumerate_pixels() {
                    out.put_pixel(image.height() - 1 - y, x, *px);
                }
                out
            }
            Self::Flipped => {
                let mut out = RgbaImage::new(image.width(), image.height());
                for (x, y, px) in image.enumerate_pixels() {
                    out.put_pixel(image.width() - 1 - x, y, *px);
                }
                out
            }
            Self::Flipped90 => {
                let mut out = RgbaImage::new(image.height(), image.width());
                for (x, y, px) in image.enumerate_pixels() {
                    out.put_pixel(y, x, *px);
                }
                out
            }
            Self::Flipped180 => {
                let mut out = RgbaImage::new(image.width(), image.height());
                for (x, y, px) in image.enumerate_pixels() {
                    out.put_pixel(x, image.height() - 1 - y, *px);
                }
                out
            }
            Self::Flipped270 => {
                let mut out = RgbaImage::new(image.height(), image.width());
                for (x, y, px) in image.enumerate_pixels() {
                    out.put_pixel(image.height() - 1 - y, image.width() - 1 - x, *px);
                }
                out
            }
        };
        let _ = (w, h);
        rotated
    }
}

impl fmt::Display for OutputTransform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Normal => "normal",
            Self::R90 => "90",
            Self::R180 => "180",
            Self::R270 => "270",
            Self::Flipped => "flipped",
            Self::Flipped90 => "flipped-90",
            Self::Flipped180 => "flipped-180",
            Self::Flipped270 => "flipped-270",
        };
        write!(f, "{name}")
    }
}

/// Static geometry of an output as seen by the capture backend.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputInfo {
    /// Output name, e.g. "eDP-1".
    pub name: String,
    /// Logical position in the global compositor coordinate space.
    pub logical_position: (i32, i32),
    /// Logical size in logical pixels.
    pub logical_size: (u32, u32),
    /// Fractional scale (physical pixels per logical pixel).
    pub scale: f64,
    /// Transform applied by the output.
    pub transform: OutputTransform,
    /// Physical (buffer) size in physical pixels.
    pub physical_size: (u32, u32),
}

/// A captured frame, normalized to logical orientation as RGBA8.
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    pub image: RgbaImage,
    /// The buffer transform reported by the protocol before normalization.
    pub transform: OutputTransform,
    /// The protocol that produced this frame.
    pub protocol: &'static str,
}

/// Errors surfaced by capture backends.
#[derive(Debug)]
pub enum CaptureError {
    /// The Wayland connection or required globals are unavailable.
    Unavailable(String),
    /// The output no longer exists.
    OutputGone(String),
    /// The compositor rejected the capture.
    Rejected(String),
    /// A buffer/shared-memory allocation failure.
    Buffer(String),
    /// A protocol error or timeout.
    Protocol(String),
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(m) => write!(f, "capture unavailable: {m}"),
            Self::OutputGone(m) => write!(f, "output gone: {m}"),
            Self::Rejected(m) => write!(f, "capture rejected: {m}"),
            Self::Buffer(m) => write!(f, "buffer allocation failed: {m}"),
            Self::Protocol(m) => write!(f, "capture protocol error: {m}"),
        }
    }
}

impl std::error::Error for CaptureError {}

/// Internal seam between production Wayland capture and in-memory test
/// adapters. Callers of the capture module only use the module interface and
/// never interact with this trait directly.
pub trait CaptureBackend: Send + Sync {
    /// Human-readable backend name for diagnostics.
    fn name(&self) -> &'static str;
    /// A stable identifier of the capture protocol in use.
    fn protocol_name(&self) -> &'static str;
    /// List the currently known outputs with static geometry.
    fn outputs(&self) -> Vec<OutputInfo>;
    /// Capture the full contents of `output` into a normalized RGBA image.
    fn capture_output(
        &self,
        output: &str,
        with_cursor: bool,
    ) -> Result<CapturedFrame, CaptureError>;
    /// Capture a rectangular region of `output`. Regions are in logical
    /// coordinates and are clamped to the output bounds.
    fn capture_region(
        &self,
        output: &str,
        region: &Region,
        with_cursor: bool,
    ) -> Result<CapturedFrame, CaptureError> {
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

/// Convenience helpers shared by backends.
pub(crate) mod util {
    use crate::CaptureError;
    use image::RgbaImage;

    /// Convert a stride-padded XRGB8888 buffer into a tightly packed RGBA8
    /// image, honoring the y-invert flag.
    #[allow(dead_code)]
    pub fn xrgb8888_to_rgba(
        data: &[u8],
        width: u32,
        height: u32,
        stride: u32,
        y_invert: bool,
    ) -> Result<RgbaImage, CaptureError> {
        let stride = stride as usize;
        let min_stride = (width as usize) * 4;
        if stride < min_stride {
            return Err(CaptureError::Buffer(format!(
                "stride {stride} smaller than width*4 {min_stride}"
            )));
        }
        let need = stride
            .checked_mul(height as usize)
            .ok_or_else(|| CaptureError::Buffer("stride overflow".into()))?;
        if data.len() < need {
            return Err(CaptureError::Buffer(format!(
                "buffer too small: {} < {need}",
                data.len()
            )));
        }
        let mut out = RgbaImage::new(width, height);
        for y in 0..height {
            let src_y = if y_invert { height - 1 - y } else { y } as usize;
            let row = &data[src_y * stride..src_y * stride + min_stride];
            for x in 0..width {
                let i = x as usize * 4;
                let px = image::Rgba([row[i + 2], row[i + 1], row[i], 255]);
                out.put_pixel(x, y, px);
            }
        }
        Ok(out)
    }

    /// Convert a stride-padded ARGB8888 buffer (in-memory byte order
    /// `B,G,R,A`) into RGBA8.
    #[allow(dead_code)]
    pub fn argb8888_to_rgba(
        data: &[u8],
        width: u32,
        height: u32,
        stride: u32,
        y_invert: bool,
    ) -> Result<RgbaImage, CaptureError> {
        let stride = stride as usize;
        let min_stride = (width as usize) * 4;
        if stride < min_stride {
            return Err(CaptureError::Buffer(format!(
                "stride {stride} smaller than width*4 {min_stride}"
            )));
        }
        let need = stride
            .checked_mul(height as usize)
            .ok_or_else(|| CaptureError::Buffer("stride overflow".into()))?;
        if data.len() < need {
            return Err(CaptureError::Buffer(format!(
                "buffer too small: {} < {need}",
                data.len()
            )));
        }
        let mut out = RgbaImage::new(width, height);
        for y in 0..height {
            let src_y = if y_invert { height - 1 - y } else { y } as usize;
            let row = &data[src_y * stride..src_y * stride + min_stride];
            for x in 0..width {
                let i = x as usize * 4;
                let px = image::Rgba([row[i + 2], row[i + 1], row[i], row[i + 3]]);
                out.put_pixel(x, y, px);
            }
        }
        Ok(out)
    }
}

/// Create the production backend on a Wayland session, if available.
pub fn create_production_backend() -> Option<Box<dyn CaptureBackend>> {
    match WaylandCaptureBackend::connect() {
        Ok(backend) => Some(Box::new(backend)),
        Err(err) => {
            log::debug!("wayland capture backend unavailable: {err}");
            None
        }
    }
}
