use crate::pipeline::audio_capture::AudioBuffer;
use crate::types::Container;

pub struct EncodedAudioPacket {
    pub data: Vec<u8>,
    pub pts: i64,
}

pub struct AudioEncoder {}

impl AudioEncoder {
    pub fn new(container: Container) -> anyhow::Result<Self> {
        let _codec_name = match container {
            Container::Webm => "libopus",
            Container::Mp4 | Container::Mkv => "aac",
        };

        anyhow::bail!("FFmpeg audio encoder is not initialized")
    }

    pub fn encode_buffer(
        &mut self,
        buffer: &AudioBuffer,
    ) -> anyhow::Result<Vec<EncodedAudioPacket>> {
        let _ = buffer;
        anyhow::bail!("FFmpeg audio encoder is not initialized")
    }
}
