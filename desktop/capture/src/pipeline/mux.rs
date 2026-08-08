use std::path::PathBuf;
use std::time::Duration;

use crate::pipeline::audio_encode::EncodedAudioPacket;
use crate::pipeline::video_encode::EncodedPacket;
use crate::types::StreamConfig;

pub struct Muxer {}

impl Muxer {
    pub fn new(config: &StreamConfig) -> anyhow::Result<Self> {
        let _ = config;
        anyhow::bail!("FFmpeg muxer is not initialized")
    }

    pub fn write_video_packet(&mut self, _packet: &EncodedPacket) -> anyhow::Result<()> {
        anyhow::bail!("FFmpeg video muxer is not initialized")
    }

    pub fn write_audio_packet(&mut self, _packet: &EncodedAudioPacket) -> anyhow::Result<()> {
        anyhow::bail!("FFmpeg audio muxer is not initialized")
    }

    pub fn finalize(self) -> anyhow::Result<(PathBuf, Duration)> {
        let _ = self;
        anyhow::bail!("recording could not be finalized: FFmpeg muxer produced no container");
    }
}
