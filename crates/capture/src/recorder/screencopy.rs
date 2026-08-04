//! Native `wlr-screencopy` compatibility backend for compositors that do not
//! yet advertise the newer image-copy protocol family.
//!
//! Adapted from `wl-screenrec` (`src/cap_wlr_screencopy.rs`), licensed under
//! the Apache License, Version 2.0. Copyright (c) wl-screenrec contributors.

use std::path::PathBuf;

use drm::{
    buffer::DrmFourcc,
    node::{DrmNode, NodeType},
};
use libc::dev_t;
use log::{debug, warn};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, WEnum,
    globals::GlobalList,
    protocol::{wl_buffer::WlBuffer, wl_output::WlOutput},
};
use wayland_protocols::wp::linux_dmabuf::zv1::client::{
    zwp_linux_dmabuf_feedback_v1::{self, ZwpLinuxDmabufFeedbackV1},
    zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1,
};
use wayland_protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_frame_v1::{self, ZwlrScreencopyFrameV1},
    zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1,
};

use crate::recorder::{encode::DmabufPotentialFormat, worker::WorkerState};

impl Dispatch<ZwlrScreencopyManagerV1, ()> for WorkerState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwlrScreencopyManagerV1,
        _event: <ZwlrScreencopyManagerV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrScreencopyFrameV1, ()> for WorkerState {
    fn event(
        state: &mut Self,
        _frame: &ZwlrScreencopyFrameV1,
        event: <ZwlrScreencopyFrameV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_screencopy_frame_v1::Event::Ready {
                tv_sec_hi,
                tv_sec_lo,
                tv_nsec,
            } => state.on_copy_complete(qh, tv_sec_hi, tv_sec_lo, tv_nsec),
            zwlr_screencopy_frame_v1::Event::BufferDone => {
                let constraints = state
                    .cap
                    .as_mut()
                    .and_then(|capture| capture.screencopy_mut())
                    .map(ScreencopyCapture::take_constraints);
                let Some((formats, size, device)) = constraints else {
                    state.fail(
                        "screencopy constraints arrived without a capture session",
                        qh,
                    );
                    return;
                };
                let Some(size) = size else {
                    state.fail(
                        "compositor did not advertise a DMA-BUF screencopy format",
                        qh,
                    );
                    return;
                };
                state.negotiate_format(&formats, size, device.as_deref(), qh);
            }
            zwlr_screencopy_frame_v1::Event::LinuxDmabuf {
                format,
                width,
                height,
            } => {
                let Ok(fourcc) = DrmFourcc::try_from(format) else {
                    warn!("unknown screencopy DRM FourCC: 0x{format:08x}");
                    return;
                };
                if let Some(capture) = state
                    .cap
                    .as_mut()
                    .and_then(|capture| capture.screencopy_mut())
                {
                    capture.add_format(
                        DmabufPotentialFormat {
                            fourcc,
                            modifiers: vec![0],
                        },
                        (width, height),
                    );
                }
            }
            zwlr_screencopy_frame_v1::Event::Flags { flags } => match flags {
                WEnum::Value(flags) => state
                    .on_frame_y_invert(flags.contains(zwlr_screencopy_frame_v1::Flags::YInvert)),
                WEnum::Unknown(value) => debug!("unknown screencopy frame flags: {value}"),
            },
            zwlr_screencopy_frame_v1::Event::Failed => state.on_copy_fail(qh),
            zwlr_screencopy_frame_v1::Event::Buffer { .. }
            | zwlr_screencopy_frame_v1::Event::Damage { .. } => {}
            _ => {}
        }
    }
}

impl Dispatch<ZwpLinuxDmabufFeedbackV1, ()> for WorkerState {
    fn event(
        state: &mut Self,
        _proxy: &ZwpLinuxDmabufFeedbackV1,
        event: <ZwpLinuxDmabufFeedbackV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let zwp_linux_dmabuf_feedback_v1::Event::MainDevice { device } = event {
            let Ok(bytes) = <[u8; std::mem::size_of::<dev_t>()]>::try_from(device.as_slice())
            else {
                warn!("compositor sent an invalid DMA-BUF main-device identifier");
                return;
            };
            let device_id = dev_t::from_ne_bytes(bytes);
            let render_path = DrmNode::from_dev_id(device_id)
                .ok()
                .and_then(|node| node.node_with_type(NodeType::Render).and_then(Result::ok))
                .and_then(|node| node.dev_path());
            if let Some(capture) = state
                .cap
                .as_mut()
                .and_then(|capture| capture.screencopy_mut())
            {
                capture.drm_device = render_path;
            }
        }
    }
}

pub struct ScreencopyCapture {
    formats: Vec<DmabufPotentialFormat>,
    size: Option<(u32, u32)>,
    manager: ZwlrScreencopyManagerV1,
    output: WlOutput,
    pub drm_device: Option<PathBuf>,
    paint_cursor: bool,
}

impl ScreencopyCapture {
    pub fn new(
        globals: &GlobalList,
        qh: &QueueHandle<WorkerState>,
        output: WlOutput,
        paint_cursor: bool,
    ) -> Result<Self, String> {
        let manager: ZwlrScreencopyManagerV1 = globals
            .bind(qh, 3..=ZwlrScreencopyManagerV1::interface().version, ())
            .map_err(|_| "compositor does not support zwlr-screencopy-manager-v1 version 3")?;
        let dmabuf: ZwpLinuxDmabufV1 = globals
            .bind(qh, 4..=ZwpLinuxDmabufV1::interface().version, ())
            .map_err(|_| {
                "compositor does not support zwp-linux-dmabuf-v1 version 4 feedback required by screencopy"
            })?;
        dmabuf.get_default_feedback(qh, ());

        Ok(Self {
            formats: Vec::new(),
            size: None,
            manager,
            output,
            drm_device: None,
            paint_cursor,
        })
    }

    pub fn alloc_frame(&mut self, qh: &QueueHandle<WorkerState>) -> ZwlrScreencopyFrameV1 {
        self.formats.clear();
        self.size = None;
        self.manager
            .capture_output(self.paint_cursor.into(), &self.output, qh, ())
    }

    pub fn add_format(&mut self, format: DmabufPotentialFormat, size: (u32, u32)) {
        self.formats.push(format);
        self.size = Some(size);
    }

    pub fn take_constraints(
        &mut self,
    ) -> (
        Vec<DmabufPotentialFormat>,
        Option<(u32, u32)>,
        Option<PathBuf>,
    ) {
        (
            std::mem::take(&mut self.formats),
            self.size.take(),
            self.drm_device.clone(),
        )
    }

    pub fn queue_copy(&self, buffer: &WlBuffer, frame: &ZwlrScreencopyFrameV1) {
        frame.copy_with_damage(buffer);
    }

    pub fn on_done_with_frame(&self, frame: &ZwlrScreencopyFrameV1) {
        frame.destroy();
    }
}
