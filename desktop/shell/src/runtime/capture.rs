use shilpo_capture::{
    AudioSource, CaptureIntent, RecordingController, RecordingRequest, RecordingSource,
    RecordingState, capture_frame,
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
        _intent: CaptureIntent,
        _config: &CaptureConfig,
    ) -> anyhow::Result<()> {
        let _frame = capture_frame(None)?;
        Ok(())
    }
}

impl Default for ShellCaptureRuntime {
    fn default() -> Self {
        Self::new()
    }
}
