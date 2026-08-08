use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::pipeline::audio_encode::EncodedAudioPacket;
use crate::pipeline::video_encode::EncodedPacket;
use crate::types::StreamConfig;

pub struct Muxer {
    output_path: PathBuf,
    temp_path: PathBuf,
    start_time: Instant,
}

impl Muxer {
    pub fn new(config: &StreamConfig) -> anyhow::Result<Self> {
        let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
        let ext = match config.container {
            crate::types::Container::Mp4 => "mp4",
            crate::types::Container::Mkv => "mkv",
            crate::types::Container::Webm => "webm",
        };

        let filename = format!("Recording_{timestamp}.{ext}");
        let output_path = config.output_dir.join(&filename);
        let temp_path = config.output_dir.join(format!(".tmp_{filename}"));

        fs::create_dir_all(&config.output_dir)?;

        Ok(Self {
            output_path,
            temp_path,
            start_time: Instant::now(),
        })
    }

    pub fn write_video_packet(&mut self, _packet: &EncodedPacket) -> anyhow::Result<()> {
        Ok(())
    }

    pub fn write_audio_packet(&mut self, _packet: &EncodedAudioPacket) -> anyhow::Result<()> {
        Ok(())
    }

    pub fn finalize(self) -> anyhow::Result<(PathBuf, Duration)> {
        let duration = self.start_time.elapsed();

        // Write container header / mock file if empty
        if !self.temp_path.exists() {
            fs::write(&self.temp_path, b"SHILPO_CAPTURE_MP4_HEADER")?;
        }

        // Atomically move temporary file to final target destination
        fs::rename(&self.temp_path, &self.output_path)?;

        Ok((self.output_path, duration))
    }
}
