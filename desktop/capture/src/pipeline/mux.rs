use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use crate::pipeline::audio_encode::{AudioEncoder, EncodedAudioPacket};
use crate::pipeline::video_encode::{EncodedPacket, VideoEncoder};
use crate::types::{Container, StreamConfig};
use anyhow::Context;

pub struct Muxer {
    output_path: PathBuf,
    temp_path: PathBuf,
    output: Option<ffmpeg::format::context::Output>,
    stream_index: usize,
    time_base: ffmpeg::Rational,
    audio_stream: Option<(usize, ffmpeg::Rational)>,
}

impl Muxer {
    pub fn new(
        config: &StreamConfig,
        encoder: &VideoEncoder,
        audio: Option<&AudioEncoder>,
    ) -> anyhow::Result<Self> {
        ffmpeg::init()?;
        let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S-%3f");
        let extension = match config.container {
            Container::Mp4 => "mp4",
            Container::Mkv => "mkv",
            Container::Webm => "webm",
        };
        fs::create_dir_all(&config.output_dir)?;
        let output_path = config
            .output_dir
            .join(format!("Recording_{timestamp}.{extension}"));
        let temp_path = config
            .output_dir
            .join(format!(".Recording_{timestamp}.tmp.{extension}"));
        let mut output = ffmpeg::format::output(&temp_path)?;
        let codec = ffmpeg::encoder::find(ffmpeg::codec::Id::H264);
        let mut stream = output.add_stream(codec)?;
        stream.set_parameters(&encoder.encoder);
        stream.set_time_base(encoder.time_base);
        let stream_index = stream.index();
        let audio_stream = audio
            .map(|audio| {
                let mut stream = output.add_stream(None)?;
                stream.set_parameters(&audio.encoder);
                stream.set_time_base(ffmpeg::Rational(1, 48_000));
                Ok::<_, ffmpeg::Error>((stream.index(), stream.time_base()))
            })
            .transpose()?;
        output.write_header()?;
        Ok(Self {
            output_path,
            temp_path,
            output: Some(output),
            stream_index,
            time_base: encoder.time_base,
            audio_stream,
        })
    }

    pub fn write_video_packet(&mut self, packet: &EncodedPacket) -> anyhow::Result<()> {
        let output = self
            .output
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("muxer is closed"))?;
        let mut packet = packet.packet.clone();
        packet.set_stream(self.stream_index);
        packet.rescale_ts(
            self.time_base,
            output.stream(self.stream_index).unwrap().time_base(),
        );
        packet.write_interleaved(output)?;
        Ok(())
    }

    pub fn write_audio_packet(&mut self, packet: &EncodedAudioPacket) -> anyhow::Result<()> {
        let Some((stream_index, time_base)) = self.audio_stream else {
            anyhow::bail!("audio stream was not configured");
        };
        let output = self
            .output
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("muxer is closed"))?;
        let mut packet = packet.packet.clone();
        packet.set_stream(stream_index);
        packet.rescale_ts(ffmpeg::Rational(1, 48_000), time_base);
        packet.write_interleaved(output)?;
        Ok(())
    }

    pub fn finalize(mut self) -> anyhow::Result<(PathBuf, Duration)> {
        let started = std::time::Instant::now();
        if let Some(mut output) = self.output.take() {
            output
                .write_trailer()
                .context("writing recording trailer")?;
        }
        fs::rename(&self.temp_path, &self.output_path)
            .context("atomically publishing recording")?;
        Ok((self.output_path, started.elapsed()))
    }
}
