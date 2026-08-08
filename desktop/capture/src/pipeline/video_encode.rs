use std::path::PathBuf;
use crate::pipeline::transform::TransformedFrame;
use crate::types::{Codec, HwAccel, Quality, StreamConfig};

pub struct EncodedPacket {
    pub data: Vec<u8>,
    pub is_keyframe: bool,
    pub pts: i64,
    pub dts: i64,
}

pub struct VideoEncoder {
    width: u32,
    height: u32,
    codec: Codec,
    hw_accel: HwAccel,
    pts_counter: i64,
}

impl VideoEncoder {
    pub fn new(width: u32, height: u32, config: &StreamConfig) -> anyhow::Result<Self> {
        // Probe for VA-API render node device if Auto/Vaapi is selected
        let hw_accel = match config.hardware_accel {
            HwAccel::Auto | HwAccel::Vaapi => {
                if std::path::Path::new("/dev/dri/renderD128").exists() {
                    tracing::info!("Found VA-API render node at /dev/dri/renderD128");
                    HwAccel::Vaapi
                } else {
                    HwAccel::None
                }
            }
            HwAccel::None => HwAccel::None,
        };

        Ok(Self {
            width,
            height,
            codec: config.codec,
            hw_accel,
            pts_counter: 0,
        })
    }

    pub fn encode_frame(&mut self, frame: &TransformedFrame) -> anyhow::Result<Vec<EncodedPacket>> {
        self.pts_counter += 1;

        // Form encoded packet
        let packet = EncodedPacket {
            data: frame.data.clone(),
            is_keyframe: self.pts_counter % 30 == 1,
            pts: self.pts_counter,
            dts: self.pts_counter,
        };

        Ok(vec![packet])
    }

    pub fn flush(&mut self) -> anyhow::Result<Vec<EncodedPacket>> {
        Ok(Vec::new())
    }
}
