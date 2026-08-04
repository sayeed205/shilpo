use crate::{CaptureOutcome, CaptureResult};
use image::RgbaImage;
use std::fs;
use std::process::Command;

pub struct OcrEngine;

impl OcrEngine {
    /// Execute Tesseract on an in-memory RGBA image.
    ///
    /// Saves the image to a temporary PNG, runs `tesseract <temp_path> stdout`,
    /// cleans up the temporary file on all code paths, and returns the outcome.
    pub fn recognize(image: &RgbaImage) -> CaptureOutcome {
        let temp_dir = std::env::temp_dir();
        let temp_filename = format!("shilpo_ocr_{}.png", uuid::Uuid::new_v4());
        let temp_path = temp_dir.join(temp_filename);

        if let Err(err) = image.save(&temp_path) {
            return CaptureOutcome::EncodingFailed(format!("failed to write temp PNG: {err}"));
        }

        // Scope execution to ensure temp_path is cleaned up even if error occurs
        let result = (|| {
            let output = Command::new("tesseract")
                .arg(&temp_path)
                .arg("stdout")
                .output()
                .map_err(|e| format!("failed to execute tesseract: {e}"))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(format!("tesseract error: {}", stderr.trim()));
            }

            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok(text)
        })();

        // Clean up temporary PNG
        let _ = fs::remove_file(&temp_path);

        match result {
            Ok(text) => {
                if text.is_empty() {
                    CaptureOutcome::OcrEmpty
                } else {
                    let mut clipboard = match arboard::Clipboard::new() {
                        Ok(clipboard) => clipboard,
                        Err(error) => return CaptureOutcome::ClipboardFailed(error.to_string()),
                    };
                    if let Err(error) = clipboard.set_text(&text) {
                        return CaptureOutcome::ClipboardFailed(error.to_string());
                    }
                    CaptureOutcome::Success(CaptureResult {
                        saved_path: None,
                        to_clipboard: true,
                        ocr_text: Some(text),
                        recording: None,
                    })
                }
            }
            Err(err) => CaptureOutcome::OcrFailed(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    #[test]
    fn test_ocr_missing_binary_returns_failure() {
        let image = RgbaImage::new(10, 10);
        let outcome = OcrEngine::recognize(&image);
        // On systems without tesseract, returns OcrFailed or OcrEmpty
        assert!(!matches!(outcome, CaptureOutcome::Success(_)));
    }
}
