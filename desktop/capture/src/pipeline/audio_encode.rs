use crate::pipeline::audio_capture::AudioBuffer;
use crate::types::Container;
use ffmpeg::{Packet, Rational, codec, encoder, format, frame};

pub struct EncodedAudioPacket {
    pub packet: Packet,
}

pub struct AudioEncoder {
    pub(crate) encoder: encoder::Audio,
    sample_index: i64,
}

impl AudioEncoder {
    pub fn new(container: Container) -> anyhow::Result<Self> {
        ffmpeg::init()?;
        let codec_id = match container {
            Container::Webm => codec::Id::OPUS,
            Container::Mp4 | Container::Mkv => codec::Id::AAC,
        };
        let codec = encoder::find(codec_id)
            .ok_or_else(|| anyhow::anyhow!("requested FFmpeg audio encoder is unavailable"))?;
        let time_base = Rational(1, 48_000);
        let mut context = codec::context::Context::new_with_codec(codec)
            .encoder()
            .audio()?;
        context.set_rate(48_000);
        context.set_channel_layout(ffmpeg::channel_layout::ChannelLayout::STEREO);
        context.set_format(format::Sample::F32(format::sample::Type::Planar));
        context.set_time_base(time_base);
        let encoder = context.open_as(codec)?;
        Ok(Self {
            encoder,
            sample_index: 0,
        })
    }

    pub fn encode_buffer(
        &mut self,
        buffer: &AudioBuffer,
    ) -> anyhow::Result<Vec<EncodedAudioPacket>> {
        let samples = buffer.pcm_data.len() / buffer.channels.max(1) as usize;
        let mut input = frame::Audio::new(
            format::Sample::F32(format::sample::Type::Planar),
            samples,
            ffmpeg::channel_layout::ChannelLayout::STEREO,
        );
        for (index, sample) in buffer.pcm_data.iter().enumerate() {
            let channel = index % 2;
            let sample_index = index / 2;
            let start = sample_index * std::mem::size_of::<f32>();
            input.data_mut(channel)[start..start + 4].copy_from_slice(&sample.to_le_bytes());
        }
        input.set_pts(Some(self.sample_index));
        self.sample_index += samples as i64;
        self.encoder.send_frame(&input)?;
        let mut packets = Vec::new();
        loop {
            let mut packet = Packet::empty();
            match self.encoder.receive_packet(&mut packet) {
                Ok(()) => packets.push(EncodedAudioPacket { packet }),
                Err(ffmpeg::Error::Eof) | Err(ffmpeg::Error::Other { errno: 11 }) => break,
                Err(error) => return Err(error.into()),
            }
        }
        Ok(packets)
    }
}
