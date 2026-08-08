use std::path::PathBuf;
use std::time::Instant;

/// What to do with a captured screenshot
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CaptureIntent {
    Clipboard,
    Annotation,
    Ocr,
    Menu,
}

/// Result/outcome of a screenshot capture operation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum CaptureOutcome {
    Accepted,
    Copied,
    Saved(PathBuf),
    TextCopied(String),
}

/// Rectangular area definition
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }
}

pub type Region = Rect;

/// Pixel formats supported by frame buffers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameFormat {
    Argb8888,
    Xrgb8888,
    Xbgr8888,
    Abgr8888,
}

/// A single captured frame
#[derive(Debug)]
pub struct Frame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub format: FrameFormat,
    pub timestamp: Instant,
}
