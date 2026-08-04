//! `ext-image-copy-capture-v1` capture session: binds the manager globals,
//! creates a capture source for an output (or foreign toplevel), collects the
//! buffer constraints (size, DRM device, DRM formats + modifiers) and submits
//! DMA-BUF backed `wl_buffer`s for capture.
//!
//! Adapted from `wl-screenrec` (`src/cap_ext_image_copy.rs`), licensed under
//! the Apache License, Version 2.0. Copyright (c) wl-screenrec contributors.
//! This file is a derived work distributed under the Apache-2.0 license.

use std::path::PathBuf;

use drm::{buffer::DrmFourcc, node::DrmNode};
use libc::dev_t;
use log::{debug, warn};
use wayland_client::{
    Dispatch, Proxy, QueueHandle, WEnum, globals::GlobalList, protocol::wl_output::WlOutput,
};
use wayland_protocols::ext::{
    image_capture_source::v1::client::{
        ext_foreign_toplevel_image_capture_source_manager_v1::ExtForeignToplevelImageCaptureSourceManagerV1,
        ext_image_capture_source_v1::ExtImageCaptureSourceV1,
        ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
    },
    image_copy_capture::v1::client::{
        ext_image_copy_capture_frame_v1::ExtImageCopyCaptureFrameV1,
        ext_image_copy_capture_manager_v1::{self, ExtImageCopyCaptureManagerV1},
        ext_image_copy_capture_session_v1::{self, ExtImageCopyCaptureSessionV1},
    },
};

use crate::recorder::worker::WorkerState;
use crate::recorder::{encode::DmabufPotentialFormat, screencopy::ScreencopyCapture};

impl Dispatch<ExtImageCopyCaptureManagerV1, ()> for WorkerState {
    fn event(
        _state: &mut Self,
        _proxy: &ExtImageCopyCaptureManagerV1,
        _event: <ExtImageCopyCaptureManagerV1 as Proxy>::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtOutputImageCaptureSourceManagerV1, ()> for WorkerState {
    fn event(
        _state: &mut Self,
        _proxy: &ExtOutputImageCaptureSourceManagerV1,
        _event: <ExtOutputImageCaptureSourceManagerV1 as Proxy>::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtImageCaptureSourceV1, ()> for WorkerState {
    fn event(
        _state: &mut Self,
        _proxy: &ExtImageCaptureSourceV1,
        _event: <ExtImageCaptureSourceV1 as Proxy>::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtForeignToplevelImageCaptureSourceManagerV1, ()> for WorkerState {
    fn event(
        _state: &mut Self,
        _proxy: &ExtForeignToplevelImageCaptureSourceManagerV1,
        _event: <ExtForeignToplevelImageCaptureSourceManagerV1 as Proxy>::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtImageCopyCaptureSessionV1, ()> for WorkerState {
    fn event(
        state: &mut Self,
        _proxy: &ExtImageCopyCaptureSessionV1,
        event: <ExtImageCopyCaptureSessionV1 as Proxy>::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        match event {
            ext_image_copy_capture_session_v1::Event::BufferSize { width, height } => {
                state
                    .cap
                    .as_mut()
                    .and_then(CaptureSession::image_copy_mut)
                    .expect("no image-copy capture session")
                    .in_progress_constraints
                    .buffer_size = Some((width, height));
            }
            ext_image_copy_capture_session_v1::Event::ShmFormat { .. } => {}
            ext_image_copy_capture_session_v1::Event::DmabufDevice { device } => {
                let dev = dev_t::from_ne_bytes(device.try_into().unwrap());
                if let Ok(node) = DrmNode::from_dev_id(dev) {
                    let node = node
                        .node_with_type(drm::node::NodeType::Render)
                        .and_then(Result::ok);
                    if let Some(node) = node {
                        state
                            .cap
                            .as_mut()
                            .and_then(CaptureSession::image_copy_mut)
                            .expect("no image-copy capture session")
                            .in_progress_constraints
                            .dmabuf_device = node.dev_path();
                    }
                }
            }
            ext_image_copy_capture_session_v1::Event::DmabufFormat { format, modifiers } => {
                assert!(modifiers.len() % 8 == 0);
                let modifiers = modifiers
                    .chunks_exact(8)
                    .map(|b| u64::from_ne_bytes(b.try_into().unwrap()))
                    .collect();

                if let Ok(fourcc) = DrmFourcc::try_from(format) {
                    state
                        .cap
                        .as_mut()
                        .and_then(CaptureSession::image_copy_mut)
                        .expect("no image-copy capture session")
                        .in_progress_constraints
                        .dmabuf_formats
                        .push(DmabufPotentialFormat { fourcc, modifiers });
                } else {
                    warn!("Unknown DRM Fourcc: 0x{format:08x}")
                }
            }
            ext_image_copy_capture_session_v1::Event::Done => {
                let mut constraints = BufferConstraints::default();
                std::mem::swap(
                    &mut state
                        .cap
                        .as_mut()
                        .and_then(CaptureSession::image_copy_mut)
                        .expect("no image-copy capture session")
                        .in_progress_constraints,
                    &mut constraints,
                );

                let size = constraints
                    .buffer_size
                    .expect("Done received before BufferSize...");
                state.negotiate_format(
                    &constraints.dmabuf_formats,
                    size,
                    constraints.dmabuf_device.as_deref(),
                    qhandle,
                );
            }
            ext_image_copy_capture_session_v1::Event::Stopped => {
                state.fail("capture source stopped", qhandle);
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtImageCopyCaptureFrameV1, ()> for WorkerState {
    fn event(
        state: &mut Self,
        _proxy: &ExtImageCopyCaptureFrameV1,
        event: <ExtImageCopyCaptureFrameV1 as Proxy>::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        use wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_frame_v1::Event;
        match event {
            Event::Transform { transform } => match transform {
                WEnum::Value(transform) => state.on_frame_transform(transform),
                WEnum::Unknown(value) => {
                    state.fail(
                        &format!("unknown capture transform value: {value}"),
                        qhandle,
                    );
                }
            },
            Event::Damage { .. } => {}
            Event::PresentationTime {
                tv_sec_hi,
                tv_sec_lo,
                tv_nsec,
            } => {
                state
                    .cap
                    .as_mut()
                    .and_then(CaptureSession::image_copy_mut)
                    .expect("no image-copy capture session")
                    .time = Some((tv_sec_hi, tv_sec_lo, tv_nsec))
            }
            Event::Ready => {
                let (hi, lo, n) = state
                    .cap
                    .as_mut()
                    .and_then(CaptureSession::image_copy_mut)
                    .expect("no image-copy capture session")
                    .time
                    .take()
                    .unwrap();
                state.on_copy_complete(qhandle, hi, lo, n);
            }
            Event::Failed { reason } => {
                debug!("frame copy failed: {reason:?}");
                state.on_copy_fail(qhandle);
            }
            _ => {}
        }
    }
}

/// Accumulated buffer constraint information from the session events.
#[derive(Default)]
pub struct BufferConstraints {
    pub dmabuf_formats: Vec<DmabufPotentialFormat>,
    pub buffer_size: Option<(u32, u32)>,
    pub dmabuf_device: Option<PathBuf>,
}

/// The ext-image-copy-capture session bound to one capture source.
pub struct CapExtImageCopy {
    pub output_capture_session: ExtImageCopyCaptureSessionV1,
    pub time: Option<(u32, u32, u32)>,
    pub in_progress_constraints: BufferConstraints,
}

/// A frame allocated by one of the compositor capture protocols supported by
/// the native recorder.
#[derive(Clone)]
pub enum CaptureFrame {
    ImageCopy(ExtImageCopyCaptureFrameV1),
    Screencopy(
        wayland_protocols_wlr::screencopy::v1::client::zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1,
    ),
}

/// Deep capture seam used by the worker. Protocol-specific lifecycle and
/// constraint handling remain private to the capture modules.
pub enum CaptureSession {
    ImageCopy(CapExtImageCopy),
    Screencopy(ScreencopyCapture),
}

impl CaptureSession {
    pub fn new_for_output(
        globals: &GlobalList,
        qh: &QueueHandle<WorkerState>,
        output: WlOutput,
        paint_cursor: bool,
    ) -> Result<Self, String> {
        match CapExtImageCopy::new_for_output(globals, qh, output.clone(), paint_cursor) {
            Ok(capture) => Ok(Self::ImageCopy(capture)),
            Err(image_copy_error) => {
                ScreencopyCapture::new(globals, qh, output, paint_cursor)
                    .map(Self::Screencopy)
                    .map_err(|screencopy_error| {
                        format!(
                            "no supported output capture protocol: {image_copy_error}; {screencopy_error}"
                        )
                    })
            }
        }
    }

    pub fn new_for_toplevel(
        globals: &GlobalList,
        qh: &QueueHandle<WorkerState>,
        toplevel: wayland_protocols::ext::foreign_toplevel_list::v1::client::ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1,
        paint_cursor: bool,
    ) -> Result<Self, String> {
        CapExtImageCopy::new_for_toplevel(globals, qh, toplevel, paint_cursor).map(Self::ImageCopy)
    }

    pub fn alloc_frame(&mut self, qh: &QueueHandle<WorkerState>) -> CaptureFrame {
        match self {
            Self::ImageCopy(capture) => CaptureFrame::ImageCopy(capture.alloc_frame(qh)),
            Self::Screencopy(capture) => CaptureFrame::Screencopy(capture.alloc_frame(qh)),
        }
    }

    pub fn queue_copy(
        &self,
        buffer: &wayland_client::protocol::wl_buffer::WlBuffer,
        dimensions: (i32, i32),
        frame: &CaptureFrame,
    ) {
        match (self, frame) {
            (Self::ImageCopy(capture), CaptureFrame::ImageCopy(frame)) => {
                capture.queue_copy(buffer, dimensions, frame);
            }
            (Self::Screencopy(capture), CaptureFrame::Screencopy(frame)) => {
                capture.queue_copy(buffer, frame);
            }
            _ => unreachable!("capture frame does not belong to its capture session"),
        }
    }

    pub fn on_done_with_frame(&self, frame: &CaptureFrame) {
        match (self, frame) {
            (Self::ImageCopy(capture), CaptureFrame::ImageCopy(frame)) => {
                capture.on_done_with_frame(frame);
            }
            (Self::Screencopy(capture), CaptureFrame::Screencopy(frame)) => {
                capture.on_done_with_frame(frame);
            }
            _ => unreachable!("capture frame does not belong to its capture session"),
        }
    }

    pub fn image_copy_mut(&mut self) -> Option<&mut CapExtImageCopy> {
        match self {
            Self::ImageCopy(capture) => Some(capture),
            Self::Screencopy(_) => None,
        }
    }

    pub fn screencopy_mut(&mut self) -> Option<&mut ScreencopyCapture> {
        match self {
            Self::Screencopy(capture) => Some(capture),
            Self::ImageCopy(_) => None,
        }
    }
}

impl CapExtImageCopy {
    /// Create a capture session for an output. Binds both required manager
    /// globals and returns an error naming the missing protocol.
    pub fn new_for_output(
        gm: &GlobalList,
        eq: &QueueHandle<WorkerState>,
        output: WlOutput,
        paint_cursor: bool,
    ) -> Result<Self, String> {
        let capture_man: ExtOutputImageCaptureSourceManagerV1 = gm
            .bind(
                eq,
                1..=ExtOutputImageCaptureSourceManagerV1::interface().version,
                (),
            )
            .map_err(|_| {
                "compositor does not support ext-output-image-capture-source-manager-v1".to_string()
            })?;

        let capture_src = capture_man.create_source(&output, eq, ());

        let copy_man: ExtImageCopyCaptureManagerV1 = gm
            .bind(
                eq,
                1..=ExtImageCopyCaptureManagerV1::interface().version,
                (),
            )
            .map_err(|_| {
                "compositor does not support ext-image-copy-capture-manager-v1".to_string()
            })?;

        let options = if paint_cursor {
            ext_image_copy_capture_manager_v1::Options::PaintCursors
        } else {
            ext_image_copy_capture_manager_v1::Options::empty()
        };
        let output_capture_session = copy_man.create_session(&capture_src, options, eq, ());

        Ok(Self {
            output_capture_session,
            time: None,
            in_progress_constraints: BufferConstraints::default(),
        })
    }

    /// Create a capture session for a foreign toplevel (window).
    pub fn new_for_toplevel(
        gm: &GlobalList,
        eq: &QueueHandle<WorkerState>,
        toplevel: wayland_protocols::ext::foreign_toplevel_list::v1::client::ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1,
        paint_cursor: bool,
    ) -> Result<Self, String> {
        let capture_man: ExtForeignToplevelImageCaptureSourceManagerV1 = gm
            .bind(
                eq,
                1..=ExtForeignToplevelImageCaptureSourceManagerV1::interface().version,
                (),
            )
            .map_err(|_| {
                "compositor does not support ext-foreign-toplevel-image-capture-source-manager-v1"
                    .to_string()
            })?;

        let capture_src = capture_man.create_source(&toplevel, eq, ());

        let copy_man: ExtImageCopyCaptureManagerV1 = gm
            .bind(
                eq,
                1..=ExtImageCopyCaptureManagerV1::interface().version,
                (),
            )
            .map_err(|_| {
                "compositor does not support ext-image-copy-capture-manager-v1".to_string()
            })?;

        let options = if paint_cursor {
            ext_image_copy_capture_manager_v1::Options::PaintCursors
        } else {
            ext_image_copy_capture_manager_v1::Options::empty()
        };
        let output_capture_session = copy_man.create_session(&capture_src, options, eq, ());

        Ok(Self {
            output_capture_session,
            time: None,
            in_progress_constraints: BufferConstraints::default(),
        })
    }

    pub fn alloc_frame(&self, eq: &QueueHandle<WorkerState>) -> ExtImageCopyCaptureFrameV1 {
        debug!("ext_image_copy_capture_session_v1::create_frame");
        self.output_capture_session.create_frame(eq, ())
    }

    pub fn queue_copy(
        &self,
        buf: &wayland_client::protocol::wl_buffer::WlBuffer,
        (width, height): (i32, i32),
        cap: &ExtImageCopyCaptureFrameV1,
    ) {
        cap.attach_buffer(buf);
        cap.damage_buffer(0, 0, width, height);
        cap.capture();
    }

    pub fn on_done_with_frame(&self, f: &ExtImageCopyCaptureFrameV1) {
        debug!("ext_image_copy_capture_frame_v1::destroy");
        f.destroy();
    }
}
