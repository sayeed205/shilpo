use crate::pipeline::transform::TransformedFrame;
use crate::types::StreamConfig;
use ffmpeg::{Packet, Rational, codec, encoder, format, frame, software};

pub struct EncodedPacket {
    pub packet: Packet,
}

pub struct VideoEncoder {
    pub(crate) encoder: encoder::Video,
    pub(crate) time_base: Rational,
    scaler: software::scaling::Context,
    frame_index: i64,
}

impl VideoEncoder {
    pub fn new(width: u32, height: u32, config: &StreamConfig) -> anyhow::Result<Self> {
        ffmpeg::init()?;
        let codec = encoder::find(codec::Id::H264)
            .ok_or_else(|| anyhow::anyhow!("FFmpeg H.264 encoder is unavailable"))?;
        let time_base = Rational(1, config.framerate.max(1) as i32);
        let mut context = codec::context::Context::new_with_codec(codec)
            .encoder()
            .video()?;
        context.set_width(width);
        context.set_height(height);
        context.set_format(format::Pixel::YUV420P);
        context.set_time_base(time_base);
        context.set_frame_rate(Some(time_base));
        let encoder = context.open_as(codec)?;
        let scaler = software::scaling::Context::get(
            format::Pixel::YUV420P,
            width,
            height,
            format::Pixel::YUV420P,
            width,
            height,
            software::scaling::flag::Flags::FAST_BILINEAR,
        )?;
        Ok(Self {
            encoder,
            time_base,
            scaler,
            frame_index: 0,
        })
    }

    pub fn encode_frame(&mut self, frame: &TransformedFrame) -> anyhow::Result<Vec<EncodedPacket>> {
        let mut input = frame::Video::new(format::Pixel::YUV420P, frame.width, frame.height);
        let y_size = (frame.width * frame.height) as usize;
        let uv_size = y_size / 4;
        input.data_mut(0)[..y_size].copy_from_slice(&frame.data[..y_size]);
        input.data_mut(1)[..uv_size].copy_from_slice(&frame.data[y_size..y_size + uv_size]);
        input.data_mut(2)[..uv_size]
            .copy_from_slice(&frame.data[y_size + uv_size..y_size + 2 * uv_size]);
        input.set_pts(Some(self.frame_index));
        self.frame_index += 1;
        let mut converted = frame::Video::empty();
        self.scaler.run(&input, &mut converted)?;
        self.encoder.send_frame(&converted)?;
        let mut packets = Vec::new();
        loop {
            let mut packet = Packet::empty();
            match self.encoder.receive_packet(&mut packet) {
                Ok(()) => {
                    packet.set_pts(Some(self.frame_index - 1));
                    packet.set_dts(Some(self.frame_index - 1));
                    packets.push(EncodedPacket { packet });
                }
                Err(ffmpeg::Error::Eof) | Err(ffmpeg::Error::Other { errno: 11 }) => break,
                Err(error) => return Err(error.into()),
            }
        }
        Ok(packets)
    }

    pub fn flush(&mut self) -> anyhow::Result<Vec<EncodedPacket>> {
        self.encoder.send_eof()?;
        let mut packets = Vec::new();
        loop {
            let mut packet = Packet::empty();
            match self.encoder.receive_packet(&mut packet) {
                Ok(()) => {
                    packet.set_pts(Some(self.frame_index));
                    packet.set_dts(Some(self.frame_index));
                    packets.push(EncodedPacket { packet });
                }
                Err(ffmpeg::Error::Eof) | Err(ffmpeg::Error::Other { errno: 11 }) => break,
                Err(error) => return Err(error.into()),
            }
        }
        Ok(packets)
    }
}
