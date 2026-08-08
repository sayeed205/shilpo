use shilpo_capture::{
    AudioSource, CaptureIntent, RecordingController, RecordingRequest, RecordingSource,
    RecordingState, capture_frame, copy_image_to_clipboard, frame_to_rgba,
};
use shilpo_config::CaptureConfig;

pub struct ShellCaptureRuntime {
    controller: RecordingController,
}

impl ShellCaptureRuntime {
    pub fn new() -> Self {
        Self {
            controller: RecordingController::new(),
        }
    }

    pub fn controller(&self) -> &RecordingController {
        &self.controller
    }

    pub fn state(&self) -> RecordingState {
        self.controller.state()
    }

    pub fn start(&self, source: RecordingSource, audio: AudioSource) -> anyhow::Result<()> {
        let req = RecordingRequest { source, audio };
        let config = shilpo_capture::StreamConfig::default();
        self.controller.start(req, config)
    }

    pub fn stop(&self) -> anyhow::Result<()> {
        self.controller.stop()
    }

    pub fn pause(&self) -> anyhow::Result<()> {
        self.controller.pause()
    }

    pub fn resume(&self) -> anyhow::Result<()> {
        self.controller.resume()
    }

    pub fn capture_screenshot(
        &self,
        intent: CaptureIntent,
        _config: &CaptureConfig,
    ) -> anyhow::Result<()> {
        let frame = capture_frame(None)?;
        let image = frame_to_rgba(&frame)?;
        match intent {
            CaptureIntent::Clipboard => copy_image_to_clipboard(&image)?,
            CaptureIntent::Annotation | CaptureIntent::Menu => {
                let dir = _config.ensure_screenshot_dir()?;
                let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
                image.save(dir.join(format!("Screenshot_{timestamp}.png")))?;
            }
            CaptureIntent::Ocr => anyhow::bail!("OCR capture requires the OCR feature"),
        }
        Ok(())
    }
}

impl Default for ShellCaptureRuntime {
    fn default() -> Self {
        Self::new()
    }
}
