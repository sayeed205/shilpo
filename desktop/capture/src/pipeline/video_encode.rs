use crate::pipeline::transform::TransformedFrame;
use crate::types::{HwAccel, StreamConfig};

pub struct EncodedPacket {
    pub data: Vec<u8>,
    pub is_keyframe: bool,
    pub pts: i64,
    pub dts: i64,
}

pub struct VideoEncoder;

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

        let _ = (width, height, config, hw_accel);
        anyhow::bail!("FFmpeg video encoder is not initialized")
    }

    pub fn encode_frame(&mut self, frame: &TransformedFrame) -> anyhow::Result<Vec<EncodedPacket>> {
        let _ = frame;
        anyhow::bail!("FFmpeg video encoder is not initialized")
    }

    pub fn flush(&mut self) -> anyhow::Result<Vec<EncodedPacket>> {
        Ok(Vec::new())
    }
}
