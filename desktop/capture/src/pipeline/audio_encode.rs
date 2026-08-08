use crate::pipeline::audio_capture::AudioBuffer;
use ffmpeg::{Packet, Rational, codec, encoder, format, frame};

pub struct EncodedAudioPacket {
    pub packet: Packet,
}

pub struct AudioEncoder {
    pub(crate) encoder: encoder::Audio,
    sample_index: i64,
    fifo_buffer: Vec<f32>,
}

impl AudioEncoder {
    pub fn new() -> anyhow::Result<Self> {
        ffmpeg::init()?;
        let codec = encoder::find(codec::Id::AAC)
            .ok_or_else(|| anyhow::anyhow!("FFmpeg AAC audio encoder is unavailable"))?;
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
            fifo_buffer: Vec::new(),
        })
    }

    pub fn encode_buffer(
        &mut self,
        buffer: &AudioBuffer,
    ) -> anyhow::Result<Vec<EncodedAudioPacket>> {
        self.fifo_buffer.extend_from_slice(&buffer.pcm_data);
        let channels = 2usize;
        let frame_size = if self.encoder.frame_size() > 0 {
            self.encoder.frame_size() as usize
        } else {
            1024
        };
        let required_floats = frame_size * channels;
        let mut packets = Vec::new();

        while self.fifo_buffer.len() >= required_floats {
            let chunk: Vec<f32> = self.fifo_buffer.drain(..required_floats).collect();
            let mut input = frame::Audio::new(
                format::Sample::F32(format::sample::Type::Planar),
                frame_size,
                ffmpeg::channel_layout::ChannelLayout::STEREO,
            );
            for (index, sample) in chunk.iter().enumerate() {
                let channel = index % 2;
                let sample_index = index / 2;
                let plane = input.plane_mut::<f32>(channel);
                if sample_index < plane.len() {
                    plane[sample_index] = *sample;
                }
            }
            input.set_pts(Some(self.sample_index));
            self.sample_index += frame_size as i64;
            self.encoder.send_frame(&input)?;

            loop {
                let mut packet = Packet::empty();
                match self.encoder.receive_packet(&mut packet) {
                    Ok(()) => packets.push(EncodedAudioPacket { packet }),
                    Err(ffmpeg::Error::Eof) | Err(ffmpeg::Error::Other { errno: 11 }) => break,
                    Err(error) => return Err(error.into()),
                }
            }
        }
        Ok(packets)
    }

    pub fn flush(&mut self) -> anyhow::Result<Vec<EncodedAudioPacket>> {
        let channels = 2usize;
        let frame_size = if self.encoder.frame_size() > 0 {
            self.encoder.frame_size() as usize
        } else {
            1024
        };
        let required_floats = frame_size * channels;
        let mut packets = Vec::new();

        if !self.fifo_buffer.is_empty() {
            self.fifo_buffer.resize(required_floats, 0.0f32);
            let chunk: Vec<f32> = self.fifo_buffer.drain(..required_floats).collect();
            let mut input = frame::Audio::new(
                format::Sample::F32(format::sample::Type::Planar),
                frame_size,
                ffmpeg::channel_layout::ChannelLayout::STEREO,
            );
            for (index, sample) in chunk.iter().enumerate() {
                let channel = index % 2;
                let sample_index = index / 2;
                let plane = input.plane_mut::<f32>(channel);
                if sample_index < plane.len() {
                    plane[sample_index] = *sample;
                }
            }
            input.set_pts(Some(self.sample_index));
            self.sample_index += frame_size as i64;
            let _ = self.encoder.send_frame(&input);
        }

        self.encoder.send_eof()?;
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
