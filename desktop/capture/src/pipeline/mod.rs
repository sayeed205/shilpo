pub mod audio_capture;
pub mod audio_encode;
pub mod clock;
pub mod fps_limit;
pub mod mux;
pub mod transform;
pub mod video_encode;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::Sender;

use crate::backend::create_backend;
use crate::pipeline::audio_capture::PipeWireAudioCapture;
use crate::pipeline::audio_encode::AudioEncoder;
use crate::pipeline::mux::Muxer;
use crate::pipeline::transform::transform_frame;
use crate::pipeline::video_encode::VideoEncoder;
use crate::types::{RecordingEvent, RecordingRequest, StreamConfig};

pub struct RecordingPipeline {
    running: Arc<AtomicBool>,
    worker_handle: Option<JoinHandle<anyhow::Result<(PathBuf, Duration)>>>,
}

impl RecordingPipeline {
    pub fn start(
        request: RecordingRequest,
        config: StreamConfig,
        event_tx: Sender<RecordingEvent>,
    ) -> anyhow::Result<Self> {
        // Fail synchronously when the compositor/backend is unavailable. This
        // prevents the shell from entering Recording while the worker is already
        // doomed and makes shortcut failures visible to callers.
        let _ = create_backend()?;
        let running = Arc::new(AtomicBool::new(true));
        let running_flag = Arc::clone(&running);

        let handle = thread::spawn(move || {
            let mut backend = create_backend()?;
            let frame_rx = backend.start_stream(&request.source, &config)?;

            let mut audio_capture = PipeWireAudioCapture::new();
            let audio_rx = audio_capture.start_capture(request.audio)?;

            let first_frame = frame_rx
                .recv_timeout(Duration::from_secs(3))
                .map_err(|e| anyhow::anyhow!("Capture stream timeout: {e}"))?;

            let mut encoder = VideoEncoder::new(first_frame.width, first_frame.height, &config)?;
            let mut audio_encoder = AudioEncoder::new(config.container)?;
            let mut muxer = Muxer::new(&config)?;

            // Encode initial frame
            let transformed = transform_frame(&first_frame)?;
            let packets = encoder.encode_frame(&transformed)?;
            for packet in packets {
                muxer.write_video_packet(&packet)?;
            }

            while running_flag.load(Ordering::SeqCst) {
                // Drain video frames
                while let Ok(frame) = frame_rx.try_recv() {
                    let transformed = transform_frame(&frame)?;
                    let packets = encoder.encode_frame(&transformed)?;
                    for packet in packets {
                        muxer.write_video_packet(&packet)?;
                    }
                }

                // Drain audio buffers
                while let Ok(audio_buf) = audio_rx.try_recv() {
                    let audio_packets = audio_encoder.encode_buffer(&audio_buf)?;
                    for packet in audio_packets {
                        muxer.write_audio_packet(&packet)?;
                    }
                }

                thread::sleep(Duration::from_millis(5));
            }

            backend.stop_stream();
            audio_capture.stop();

            for packet in encoder.flush()? {
                muxer.write_video_packet(&packet)?;
            }

            let (final_path, duration) = muxer.finalize()?;
            let _ = event_tx.send(RecordingEvent::Completed {
                path: final_path.clone(),
                duration,
            });

            Ok((final_path, duration))
        });

        Ok(Self {
            running,
            worker_handle: Some(handle),
        })
    }

    pub fn stop(&mut self) -> anyhow::Result<(PathBuf, Duration)> {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.worker_handle.take() {
            handle
                .join()
                .map_err(|_| anyhow::anyhow!("Pipeline thread panicked"))?
        } else {
            anyhow::bail!("Pipeline not running")
        }
    }
}
