use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::fd::AsFd;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use anyhow::Context;
use memfd::MemfdOptions;
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_buffer, wl_output, wl_registry, wl_shm, wl_shm_pool};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_frame_v1::{self, ZwlrScreencopyFrameV1},
    zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1,
};

use crate::backend::CaptureBackend;
use crate::types::{Frame, FrameData, FrameFormat, RecordingSource, StreamConfig};

struct CaptureState {
    shm: Option<wl_shm::WlShm>,
    pool: Option<wl_shm_pool::WlShmPool>,
    buffer: Option<wl_buffer::WlBuffer>,
    file: Option<File>,
    width: u32,
    height: u32,
    stride: u32,
    format: FrameFormat,
    ready: bool,
    failed: Option<String>,
}

impl Default for CaptureState {
    fn default() -> Self {
        Self {
            shm: None,
            pool: None,
            buffer: None,
            file: None,
            width: 0,
            height: 0,
            stride: 0,
            format: FrameFormat::Argb8888,
            ready: false,
            failed: None,
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, wayland_client::globals::GlobalListContents>
    for CaptureState
{
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &wayland_client::globals::GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrScreencopyManagerV1, ()> for CaptureState {
    fn event(
        _: &mut Self,
        _: &ZwlrScreencopyManagerV1,
        _: <ZwlrScreencopyManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrScreencopyFrameV1, ()> for CaptureState {
    fn event(
        state: &mut Self,
        frame: &ZwlrScreencopyFrameV1,
        event: zwlr_screencopy_frame_v1::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_screencopy_frame_v1::Event::Buffer {
                format,
                width,
                height,
                stride,
            } => {
                let Some(shm) = state.shm.as_ref() else {
                    state.failed = Some("wl_shm unavailable".into());
                    return;
                };
                let Ok(memfd) = MemfdOptions::default().create("shilpo-capture") else {
                    state.failed = Some("memfd creation failed".into());
                    return;
                };
                let size = (stride as u64) * (height as u64);
                if memfd.as_file().set_len(size).is_err() {
                    state.failed = Some("resizing capture buffer failed".into());
                    return;
                }
                let Ok(file) = memfd.into_file().try_clone() else {
                    state.failed = Some("duplicating capture buffer failed".into());
                    return;
                };
                let shm_format = match format {
                    wayland_client::WEnum::Value(value) => value,
                    _ => {
                        state.failed = Some("unsupported wl_shm format".into());
                        return;
                    }
                };
                let pool = shm.create_pool(file.as_fd(), size as i32, qh, ());
                let buffer = pool.create_buffer(
                    0,
                    width as i32,
                    height as i32,
                    stride as i32,
                    shm_format,
                    qh,
                    (),
                );
                frame.copy(&buffer);
                state.pool = Some(pool);
                state.buffer = Some(buffer);
                state.file = Some(file);
                state.width = width;
                state.height = height;
                state.stride = stride;
                state.format = match shm_format {
                    wl_shm::Format::Argb8888 => FrameFormat::Argb8888,
                    wl_shm::Format::Xrgb8888 => FrameFormat::Xrgb8888,
                    _ => {
                        state.failed = Some("unsupported wl_shm format".into());
                        return;
                    }
                };
            }
            zwlr_screencopy_frame_v1::Event::Ready { .. } => state.ready = true,
            zwlr_screencopy_frame_v1::Event::Failed => {
                state.failed = Some("compositor rejected screencopy request".into())
            }
            _ => {}
        }
    }
}

wayland_client::delegate_noop!(CaptureState: ignore wl_shm::WlShm);
wayland_client::delegate_noop!(CaptureState: ignore wl_shm_pool::WlShmPool);
wayland_client::delegate_noop!(CaptureState: ignore wl_buffer::WlBuffer);
wayland_client::delegate_noop!(CaptureState: ignore wl_output::WlOutput);

pub struct WlrScreencopyBackend {
    streaming: Arc<AtomicBool>,
}

impl WlrScreencopyBackend {
    pub fn new() -> anyhow::Result<Self> {
        let conn = Connection::connect_to_env().context("connecting to Wayland")?;
        let (globals, queue) = registry_queue_init::<CaptureState>(&conn)?;
        let qh = queue.handle();
        globals.bind::<ZwlrScreencopyManagerV1, _, _>(&qh, 1..=3, ())?;
        globals.bind::<wl_shm::WlShm, _, _>(&qh, 1..=1, ())?;
        Ok(Self {
            streaming: Arc::new(AtomicBool::new(false)),
        })
    }

    fn capture_once(&self) -> anyhow::Result<Frame> {
        let conn = Connection::connect_to_env().context("connecting to Wayland")?;
        let (globals, mut queue) = registry_queue_init::<CaptureState>(&conn)?;
        let qh = queue.handle();
        let manager = globals.bind::<ZwlrScreencopyManagerV1, _, _>(&qh, 1..=3, ())?;
        let shm = globals.bind::<wl_shm::WlShm, _, _>(&qh, 1..=1, ())?;
        let output = globals.bind::<wl_output::WlOutput, _, _>(&qh, 1..=4, ())?;
        let mut state = CaptureState {
            shm: Some(shm),
            ..Default::default()
        };
        manager.capture_output(0, &output, &qh, ());
        while !state.ready && state.failed.is_none() {
            queue.blocking_dispatch(&mut state)?;
        }
        if let Some(error) = state.failed {
            anyhow::bail!(error);
        }
        let mut file = state.file.take().context("capture buffer missing")?;
        file.seek(SeekFrom::Start(0))?;
        let mut raw = vec![0; (state.stride * state.height) as usize];
        file.read_exact(&mut raw)?;
        let row_bytes = (state.width * 4) as usize;
        let mut data = Vec::with_capacity(row_bytes * state.height as usize);
        for row in raw.chunks_exact(state.stride as usize) {
            data.extend_from_slice(&row[..row_bytes]);
        }
        Ok(Frame {
            data: FrameData::Shm(data),
            width: state.width,
            height: state.height,
            format: state.format,
            timestamp: Instant::now(),
        })
    }
}

impl CaptureBackend for WlrScreencopyBackend {
    fn capture_frame(&mut self, _: Option<&str>) -> anyhow::Result<Frame> {
        self.capture_once()
    }
    fn start_stream(
        &mut self,
        source: &RecordingSource,
        config: &StreamConfig,
    ) -> anyhow::Result<crossbeam_channel::Receiver<Frame>> {
        let _ = source;
        let (tx, rx) = crossbeam_channel::bounded(4);
        let running = Arc::clone(&self.streaming);
        running.store(true, Ordering::SeqCst);
        let frame_delay =
            std::time::Duration::from_micros(1_000_000 / config.framerate.max(1) as u64);
        std::thread::spawn(move || {
            let backend = WlrScreencopyBackend {
                streaming: Arc::clone(&running),
            };
            while running.load(Ordering::SeqCst) {
                match backend.capture_once() {
                    Ok(frame) => {
                        if tx.send(frame).is_err() {
                            break;
                        }
                        std::thread::sleep(frame_delay);
                    }
                    Err(error) => {
                        tracing::error!(%error, "Wayland screencopy stream failed");
                        break;
                    }
                }
            }
        });
        Ok(rx)
    }
    fn enumerate_sources(&self) -> anyhow::Result<Vec<RecordingSource>> {
        Ok(vec![RecordingSource::primary()])
    }
    fn stop_stream(&mut self) {
        self.streaming.store(false, Ordering::SeqCst);
    }
}
