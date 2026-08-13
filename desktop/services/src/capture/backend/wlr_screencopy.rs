use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    os::fd::AsFd,
    time::Instant,
};

use anyhow::Context;
use memfd::MemfdOptions;
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle,
    globals::{GlobalList, registry_queue_init},
    protocol::{wl_buffer, wl_output, wl_registry, wl_shm, wl_shm_pool},
};
use wayland_protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_frame_v1::{self, ZwlrScreencopyFrameV1},
    zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1,
};

use crate::capture::{
    backend::CaptureBackend,
    types::{Frame, FrameFormat},
};

struct OutputEntry {
    output: wl_output::WlOutput,
    name: String,
    description: String,
}

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
    outputs: Vec<OutputEntry>,
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
            outputs: Vec::new(),
        }
    }
}

fn bind_outputs(globals: &GlobalList, qh: &QueueHandle<CaptureState>, state: &mut CaptureState) {
    for global in globals
        .contents()
        .clone_list()
        .into_iter()
        .filter(|global| global.interface == "wl_output")
    {
        let version = global.version.min(4);
        let output = globals.registry().bind(global.name, version, qh, ());
        state.outputs.push(OutputEntry {
            output,
            name: format!("output-{}", global.name),
            description: format!("Display output {}", global.name),
        });
    }
}

fn select_output(
    state: &CaptureState,
    requested: Option<&str>,
) -> anyhow::Result<wl_output::WlOutput> {
    match requested {
        Some("primary") | None => state
            .outputs
            .first()
            .map(|entry| entry.output.clone())
            .context("no Wayland outputs are available"),
        Some(name) => state
            .outputs
            .iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.output.clone())
            .with_context(|| format!("unknown Wayland output: {name}")),
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

impl Dispatch<wl_output::WlOutput, ()> for CaptureState {
    fn event(
        state: &mut Self,
        output: &wl_output::WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_output::Event::Name { name } => {
                if let Some(entry) = state.outputs.iter_mut().find(|e| e.output == *output) {
                    entry.name = name;
                }
            }
            wl_output::Event::Description { description } => {
                if let Some(entry) = state.outputs.iter_mut().find(|e| e.output == *output) {
                    entry.description = description;
                }
            }
            _ => {}
        }
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
                    wl_shm::Format::Xbgr8888 => FrameFormat::Xbgr8888,
                    wl_shm::Format::Abgr8888 => FrameFormat::Abgr8888,
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

pub struct WlrScreencopyBackend;

impl WlrScreencopyBackend {
    pub fn new() -> anyhow::Result<Self> {
        let conn = Connection::connect_to_env().context("connecting to Wayland")?;
        let (globals, queue) = registry_queue_init::<CaptureState>(&conn)?;
        let qh = queue.handle();
        globals.bind::<ZwlrScreencopyManagerV1, _, _>(&qh, 1..=3, ())?;
        globals.bind::<wl_shm::WlShm, _, _>(&qh, 1..=1, ())?;
        Ok(Self)
    }

    fn capture_once(&self, output_name: Option<&str>) -> anyhow::Result<Frame> {
        let conn = Connection::connect_to_env().context("connecting to Wayland")?;
        let (globals, mut queue) = registry_queue_init::<CaptureState>(&conn)?;
        let qh = queue.handle();
        let manager = globals.bind::<ZwlrScreencopyManagerV1, _, _>(&qh, 1..=3, ())?;
        let shm = globals.bind::<wl_shm::WlShm, _, _>(&qh, 1..=1, ())?;

        let mut state = CaptureState {
            shm: Some(shm),
            ..Default::default()
        };

        bind_outputs(&globals, &qh, &mut state);
        let _ = queue.roundtrip(&mut state);
        let target_output = select_output(&state, output_name)?;

        state.ready = false;
        state.failed = None;
        manager.capture_output(0, &target_output, &qh, ());
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
        if let Some(buffer) = state.buffer.take() {
            buffer.destroy();
        }
        if let Some(pool) = state.pool.take() {
            pool.destroy();
        }
        Ok(Frame {
            data,
            width: state.width,
            height: state.height,
            format: state.format,
            timestamp: Instant::now(),
        })
    }
}

impl CaptureBackend for WlrScreencopyBackend {
    fn capture_frame(&mut self, output: Option<&str>) -> anyhow::Result<Frame> {
        self.capture_once(output)
    }
}
