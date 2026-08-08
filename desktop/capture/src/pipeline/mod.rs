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

use crate::backend::CaptureBackend;
use crate::backend::create_backend;
use crate::pipeline::audio_capture::PipeWireAudioCapture;
use crate::pipeline::audio_encode::AudioEncoder;
use crate::pipeline::mux::Muxer;
use crate::pipeline::transform::transform_frame;
use crate::pipeline::video_encode::VideoEncoder;
use crate::types::{RecordingEvent, RecordingRequest, StreamConfig};

pub struct RecordingPipeline {
    running: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
    worker_handle: Option<JoinHandle<anyhow::Result<(PathBuf, Duration)>>>,
}

struct CaptureCleanup<'a> {
    backend: &'a mut dyn CaptureBackend,
    audio: &'a mut PipeWireAudioCapture,
}

impl Drop for CaptureCleanup<'_> {
    fn drop(&mut self) {
        self.backend.stop_stream();
        self.audio.stop();
    }
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
        let paused = Arc::new(AtomicBool::new(false));
        let cancelled = Arc::new(AtomicBool::new(false));
        let running_flag = Arc::clone(&running);
        let paused_flag = Arc::clone(&paused);
        let cancelled_flag = Arc::clone(&cancelled);

        let handle = thread::spawn(move || {
            let start_inst = std::time::Instant::now();
            let mut backend = create_backend()?;
            let frame_rx = backend.start_stream(&request.source, &config)?;

            let mut audio_capture = PipeWireAudioCapture::new();
            let audio_rx = audio_capture.start_capture(request.audio)?;
            let _cleanup = CaptureCleanup {
                backend: backend.as_mut(),
                audio: &mut audio_capture,
            };

            let first_frame = match frame_rx.recv_timeout(Duration::from_secs(3)) {
                Ok(frame) => frame,
                Err(e) => {
                    let err_msg = format!("Capture stream timeout: {e}");
                    let _ = event_tx.send(RecordingEvent::Error(err_msg.clone()));
                    anyhow::bail!(err_msg);
                }
            };

            let mut encoder = VideoEncoder::new(first_frame.width, first_frame.height, &config)?;
            let mut audio_encoder = (request.audio != crate::types::AudioSource::None)
                .then(AudioEncoder::new)
                .transpose()?;
            let mut muxer = Muxer::new(&config, &encoder, audio_encoder.as_ref())?;

            // Encode initial frame
            let transformed = transform_frame(&first_frame)?;
            for packet in encoder.encode_frame(&transformed)? {
                muxer.write_video_packet(&packet)?;
            }

            let mut paused_duration = Duration::ZERO;
            let mut pause_start: Option<std::time::Instant> = None;

            while running_flag.load(Ordering::SeqCst) {
                if paused_flag.load(Ordering::SeqCst) {
                    if pause_start.is_none() {
                        pause_start = Some(std::time::Instant::now());
                    }
                    // Drain frame and audio channels while paused without encoding
                    while frame_rx.try_recv().is_ok() {}
                    while audio_rx.try_recv().is_ok() {}
                    thread::sleep(Duration::from_millis(10));
                    continue;
                } else if let Some(p_start) = pause_start.take() {
                    paused_duration += p_start.elapsed();
                }

                // Drain video frames
                while let Ok(frame) = frame_rx.try_recv() {
                    let transformed = transform_frame(&frame)?;
                    for packet in encoder.encode_frame(&transformed)? {
                        muxer.write_video_packet(&packet)?;
                    }
                }

                // Drain audio buffers
                while let Ok(audio_buf) = audio_rx.try_recv() {
                    if let Some(audio_encoder) = audio_encoder.as_mut() {
                        for packet in audio_encoder.encode_buffer(&audio_buf)? {
                            muxer.write_audio_packet(&packet)?;
                        }
                    }
                }

                thread::sleep(Duration::from_millis(5));
            }

            if cancelled_flag.load(Ordering::SeqCst) {
                muxer.cleanup();
                anyhow::bail!("recording cancelled");
            }

            if let Some(p_start) = pause_start.take() {
                paused_duration += p_start.elapsed();
            }
            let active_duration = start_inst.elapsed().saturating_sub(paused_duration);

            for packet in encoder.flush()? {
                muxer.write_video_packet(&packet)?;
            }

            if let Some(audio_encoder) = audio_encoder.as_mut() {
                for packet in audio_encoder.flush()? {
                    muxer.write_audio_packet(&packet)?;
                }
            }

            match muxer.finalize(active_duration) {
                Ok((final_path, duration)) => {
                    let _ = event_tx.send(RecordingEvent::Completed {
                        path: final_path.clone(),
                        duration,
                    });
                    Ok((final_path, duration))
                }
                Err(e) => {
                    let err_msg = format!("Muxer finalize error: {e}");
                    let _ = event_tx.send(RecordingEvent::Error(err_msg.clone()));
                    anyhow::bail!(err_msg);
                }
            }
        });

        Ok(Self {
            running,
            paused,
            cancelled,
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

    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    pub fn cancel(&mut self) -> anyhow::Result<()> {
        self.cancelled.store(true, Ordering::SeqCst);
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.worker_handle.take() {
            handle
                .join()
                .map_err(|_| anyhow::anyhow!("Pipeline thread panicked"))??;
        }
        Ok(())
    }
}
