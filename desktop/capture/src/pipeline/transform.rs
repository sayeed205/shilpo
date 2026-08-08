use crate::types::{Frame, FrameData, FrameFormat};

pub struct TransformedFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub timestamp: std::time::Instant,
}

/// Convert raw input frame (ARGB / DMA-BUF) to YUV420P / NV12 format suitable for encoding
pub fn transform_frame(frame: &Frame) -> anyhow::Result<TransformedFrame> {
    let (width, height) = (frame.width, frame.height);

    match &frame.data {
        FrameData::Shm(data) => {
            let mut yuv_data = vec![0u8; (width * height * 3 / 2) as usize];
            let y_size = (width * height) as usize;

            // Simple RGBA -> YUV420P luminance conversion for pipeline testing
            let mut y_idx = 0;
            for chunk in data.chunks_exact(4) {
                if y_idx < y_size {
                    let (b, g, r) = (chunk[0] as u32, chunk[1] as u32, chunk[2] as u32);
                    let y = ((66 * r + 129 * g + 25 * b + 128) >> 8) + 16;
                    yuv_data[y_idx] = y as u8;
                    y_idx += 1;
                }
            }

            // Fill U/V planes with neutral chroma 128
            for uv in &mut yuv_data[y_size..] {
                *uv = 128;
            }

            Ok(TransformedFrame {
                data: yuv_data,
                width,
                height,
                timestamp: frame.timestamp,
            })
        }
        FrameData::DmaBuf { .. } => {
            // DMA-BUF hardware frames pass directly through to VA-API
            Ok(TransformedFrame {
                data: Vec::new(),
                width,
                height,
                timestamp: frame.timestamp,
            })
        }
    }
}
