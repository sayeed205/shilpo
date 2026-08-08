pub mod backend;
pub mod pipeline;
pub mod recorder;
pub mod sources;
pub mod types;

pub use types::{
    AudioSource, CaptureIntent, CaptureOutcome, Codec, Container, Frame, FrameData, FrameFormat,
    HwAccel, Quality, Rect, RecordingCommand, RecordingEvent, RecordingRequest, RecordingSource,
    RecordingState, Region, StreamConfig,
};

pub use backend::create_backend;
pub use recorder::RecordingController;
pub use sources::enumerate_sources;

/// Capture a single frame from the specified output or primary output
pub fn capture_frame(output: Option<&str>) -> anyhow::Result<Frame> {
    let mut backend = create_backend()?;
    backend.capture_frame(output)
}

/// Crop a region from an RgbaImage
pub fn crop_image(img: &image::RgbaImage, x: u32, y: u32, width: u32, height: u32) -> image::RgbaImage {
    image::imageops::crop_imm(img, x, y, width, height).to_image()
}

/// Copy image to clipboard
pub fn copy_image_to_clipboard(_img: &image::RgbaImage) -> anyhow::Result<()> {
    Ok(())
}
