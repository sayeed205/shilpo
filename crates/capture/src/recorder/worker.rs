//! The recording worker thread.
//!
//! Owns the Wayland connection, discovers outputs (via `wl_output` +
//! `zxdg-output`) or foreign toplevels, captures DMA-BUF frames through
//! `ext-image-copy-capture-v1`, feeds them into the in-process FFmpeg/VAAPI
//! encoder, and reacts to control commands (`pause`/`resume`/`stop`/`cancel`)
//! sent over a channel by the [`RecordingController`](crate::recorder::RecordingController).
//!
//! Adapted from `wl-screenrec` (`src/main.rs`), licensed under the Apache
//! License, Version 2.0. Copyright (c) wl-screenrec contributors.
//! This file is a derived work distributed under the Apache-2.0 license.

use std::{
    collections::HashMap,
    mem,
    os::fd::{AsRawFd, BorrowedFd},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, Sender},
    },
    time::Duration,
};

use drm::buffer::DrmFourcc;
use ffmpeg::{Rational, format::Pixel};
use libc::c_int;
use log::{debug, error, info, warn};
use mio::{Events, Interest, Poll, Token, unix::SourceFd};
use wayland_backend::client::ObjectId;
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, WEnum,
    globals::{GlobalList, GlobalListContents, registry_queue_init},
    protocol::{
        wl_buffer::WlBuffer,
        wl_output::{self, Transform, WlOutput},
        wl_registry::{self, WlRegistry},
    },
};
use wayland_protocols::{
    ext::{
        foreign_toplevel_list::v1::client::{
            ext_foreign_toplevel_handle_v1::{self, ExtForeignToplevelHandleV1},
            ext_foreign_toplevel_list_v1::{self, ExtForeignToplevelListV1},
        },
        image_capture_source::v1::client::{
            ext_foreign_toplevel_image_capture_source_manager_v1::ExtForeignToplevelImageCaptureSourceManagerV1,
            ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
        },
        image_copy_capture::v1::client::ext_image_copy_capture_manager_v1::ExtImageCopyCaptureManagerV1,
    },
    wp::linux_dmabuf::zv1::client::{
        zwp_linux_buffer_params_v1::{self, ZwpLinuxBufferParamsV1},
        zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1,
    },
    xdg::xdg_output::zv1::client::{
        zxdg_output_manager_v1::ZxdgOutputManagerV1,
        zxdg_output_v1::{self, ZxdgOutputV1},
    },
};
use wayland_protocols_wlr::screencopy::v1::client::zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1;

use crate::recorder::{
    capture::{CaptureFrame, CaptureSession},
    clock::SessionClock,
    encode::{DmabufFormat, DmabufPotentialFormat, EncState},
    finalization::{Finalization, finish},
    lock_recover,
    sources::{
        WorkerOutput as OutputInfo, WorkerOutputProbe as PartialOutputInfo,
        WorkerToplevelProbe as ToplevelProbe, select_output, select_toplevel,
    },
    transform::Rect,
};
use crate::types::{RecordingAudio, RecordingSource, RecordingState};

pub struct CompositorSupport {
    pub window_capture: bool,
}

struct SupportProbe;

impl Dispatch<WlRegistry, GlobalListContents> for SupportProbe {
    fn event(
        _state: &mut Self,
        _proxy: &WlRegistry,
        _event: <WlRegistry as Proxy>::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

pub fn compositor_support() -> Result<CompositorSupport, String> {
    let connection = Connection::connect_to_env()
        .map_err(|error| format!("could not connect to the Wayland compositor: {error}"))?;
    let (globals, _queue) = registry_queue_init::<SupportProbe>(&connection)
        .map_err(|error| format!("could not inspect Wayland recording protocols: {error}"))?;
    let advertised: std::collections::HashMap<_, _> = globals
        .contents()
        .clone_list()
        .into_iter()
        .map(|global| (global.interface, global.version))
        .collect();
    for required in [
        ZwpLinuxDmabufV1::interface().name,
        ZxdgOutputManagerV1::interface().name,
    ] {
        if !advertised.contains_key(required) {
            return Err(format!(
                "compositor does not advertise required recording protocol {required}"
            ));
        }
    }

    let image_copy_output = advertised
        .contains_key(ExtOutputImageCaptureSourceManagerV1::interface().name)
        && advertised.contains_key(ExtImageCopyCaptureManagerV1::interface().name);
    let screencopy_output = advertised
        .get(ZwlrScreencopyManagerV1::interface().name)
        .is_some_and(|version| *version >= 3)
        && advertised
            .get(ZwpLinuxDmabufV1::interface().name)
            .is_some_and(|version| *version >= 4);
    if !image_copy_output && !screencopy_output {
        return Err("compositor supports neither image-copy capture nor DMA-BUF screencopy".into());
    }

    Ok(CompositorSupport {
        window_capture: advertised.contains_key(ExtForeignToplevelListV1::interface().name)
            && advertised
                .contains_key(ExtForeignToplevelImageCaptureSourceManagerV1::interface().name)
            && advertised.contains_key(ExtImageCopyCaptureManagerV1::interface().name),
    })
}

/// Runtime configuration for one recording.
pub struct WorkerConfig {
    pub source: RecordingSource,
    pub path: std::path::PathBuf,
    pub path_part: std::path::PathBuf,
    pub audio: RecordingAudio,
    pub framerate: u32,
    pub bitrate_bytes_per_second: usize,
    pub gop_size: u32,
    pub delay: Duration,
    pub paint_cursor: bool,
}

/// Commands the controller sends to the worker thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerCommand {
    Pause,
    Resume,
    Stop,
    Cancel,
}

/// Handle to a running worker thread.
pub struct WorkerHandle {
    command_tx: Sender<WorkerCommand>,
    join: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl WorkerHandle {
    pub fn send(&self, command: WorkerCommand) {
        let _ = self.command_tx.send(command);
    }

    /// Wait until the worker has finalized or discarded its output.
    pub fn join(&self) {
        if let Some(join) = lock_recover(&self.join).take() {
            let _ = join.join();
        }
    }
}

/// Spawn the recording worker thread.
pub fn spawn(
    config: WorkerConfig,
    shared: Arc<Mutex<RecordingState>>,
) -> Result<WorkerHandle, String> {
    let (command_tx, command_rx) = std::sync::mpsc::channel();
    let partial_path = config.path_part.clone();
    let failure_state = shared.clone();
    let worker = Worker::new(config, shared, command_rx);

    let join = std::thread::Builder::new()
        .name("shilpo-recorder".into())
        .spawn(move || {
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| worker.run())).is_err() {
                let _ = std::fs::remove_file(partial_path);
                *lock_recover(&failure_state) = RecordingState::Failed {
                    reason: "recording worker panicked".into(),
                    partial_path: None,
                };
            }
        })
        .map_err(|error| format!("could not spawn recording thread: {error}"))?;

    Ok(WorkerHandle {
        command_tx,
        join: Mutex::new(Some(join)),
    })
}

enum InFlightSurface {
    None,
    AllocQueued,
    Allocd(CaptureFrame),
    CopyQueued {
        av_surface: ffmpeg::frame::Video,
        av_mapping: ffmpeg::frame::Video,
        capture_frame: CaptureFrame,
        wl_buffer: WlBuffer,
    },
}

struct PendingCaptureFormat {
    format: DmabufFormat,
    transform: Transform,
    device: PathBuf,
}

impl InFlightSurface {
    fn take(&mut self) -> InFlightSurface {
        mem::replace(self, InFlightSurface::None)
    }
}

pub struct Worker {
    config: WorkerConfig,
    shared: Arc<Mutex<RecordingState>>,
    command_rx: Option<Receiver<WorkerCommand>>,
}

pub struct WorkerState {
    gm: GlobalList,
    dma: ZwpLinuxDmabufV1,
    xdg_output_manager: ZxdgOutputManagerV1,
    config: WorkerConfig,
    shared: Arc<Mutex<RecordingState>>,
    command_rx: Receiver<WorkerCommand>,

    partial_outputs: HashMap<ObjectId, PartialOutputInfo>,
    outputs: HashMap<ObjectId, Option<OutputInfo>>,
    selected_output: Option<OutputInfo>,
    toplevel_list: Option<ExtForeignToplevelListV1>,
    toplevels: HashMap<ObjectId, ToplevelProbe>,

    pub(crate) cap: Option<CaptureSession>,
    in_flight_surface: InFlightSurface,
    enc: Option<EncState>,
    selected_format: Option<DmabufFormat>,
    selected_device: Option<PathBuf>,
    pending_capture_format: Option<PendingCaptureFormat>,
    pending_transform: Option<Transform>,
    pending_y_invert: Option<bool>,
    roi: Option<Rect>,
    starting_timestamp: Option<i64>,

    clock: SessionClock,
    done: bool,
    kept: bool,
    failure: Option<String>,
    completion_warning: Option<String>,
    consecutive_capture_failures: u8,
}

impl Worker {
    fn new(
        config: WorkerConfig,
        shared: Arc<Mutex<RecordingState>>,
        command_rx: Receiver<WorkerCommand>,
    ) -> Self {
        Self {
            config,
            shared,
            command_rx: Some(command_rx),
        }
    }

    fn run(mut self) {
        let path = self.config.path.clone();
        let path_part = self.config.path_part.clone();
        if let Err(reason) = self.run_inner() {
            error!("recording failed: {reason}");
            *lock_recover(&self.shared) = RecordingState::Failed {
                reason,
                partial_path: None,
            };
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::remove_file(&path_part);
        }
    }

    fn run_inner(&mut self) -> Result<(), String> {
        self.set_state(RecordingState::Starting {
            path: self.config.path.clone(),
        });
        if !self.wait_for_delay()? {
            return Ok(());
        }

        let conn = Connection::connect_to_env()
            .map_err(|error| format!("could not connect to the wayland compositor: {error}"))?;

        let (gm, mut queue) = registry_queue_init::<WorkerState>(&conn)
            .map_err(|error| format!("could not initialize the wayland registry: {error}"))?;
        let qh = queue.handle();

        // Protocol availability is checked up front and any missing required
        // protocol aborts the recording before a file is written.
        let dma: ZwpLinuxDmabufV1 = gm
            .bind(&qh, 1..=ZwpLinuxDmabufV1::interface().version, ())
            .map_err(|_| {
                "compositor does not support zwp-linux-dmabuf-v1; DMA-BUF capture is unavailable"
                    .to_string()
            })?;
        let xdg_output_manager: ZxdgOutputManagerV1 = gm
            .bind(&qh, 1..=ZxdgOutputManagerV1::interface().version, ())
            .map_err(|_| {
                "compositor does not support zxdg-output-manager-v1; output probing is unavailable"
                    .to_string()
            })?;

        // Pre-bind the managers needed for ext-image-copy-capture so missing
        // protocols fail fast.
        if self.config.source.is_window() {
            let _: ExtForeignToplevelListV1 = gm
                .bind(&qh, 1..=ExtForeignToplevelListV1::interface().version, ())
                .map_err(|_| {
                    "compositor does not support ext-foreign-toplevel-list-v1; window capture is unavailable"
                        .to_string()
                })?;
        }

        let mut state = WorkerState {
            gm,
            dma,
            xdg_output_manager,
            config: WorkerConfig {
                source: self.config.source.clone(),
                path: self.config.path.clone(),
                path_part: self.config.path_part.clone(),
                audio: self.config.audio,
                framerate: self.config.framerate,
                bitrate_bytes_per_second: self.config.bitrate_bytes_per_second,
                gop_size: self.config.gop_size,
                delay: self.config.delay,
                paint_cursor: self.config.paint_cursor,
            },
            shared: self.shared.clone(),
            command_rx: self.command_rx.take().unwrap(),
            partial_outputs: HashMap::new(),
            outputs: HashMap::new(),
            selected_output: None,
            toplevel_list: None,
            toplevels: HashMap::new(),
            cap: None,
            in_flight_surface: InFlightSurface::None,
            enc: None,
            selected_format: None,
            selected_device: None,
            pending_capture_format: None,
            pending_transform: None,
            pending_y_invert: None,
            roi: None,
            starting_timestamp: None,
            clock: SessionClock::default(),
            done: false,
            kept: false,
            failure: None,
            completion_warning: None,
            consecutive_capture_failures: 0,
        };

        state.bind_outputs(&qh);

        if state.config.source.is_window() {
            state.bind_toplevel_list(&qh);
        }

        queue
            .roundtrip(&mut state)
            .map_err(|error| format!("could not discover recording sources: {error}"))?;
        if state.config.source.is_window() && state.cap.is_none() && !state.done {
            state.fail("selected window is no longer available", &qh);
        }

        let mut poll = Poll::new().map_err(|error| format!("could not create poller: {error}"))?;
        let mut events = Events::with_capacity(64);

        let socket = {
            let guard = queue.prepare_read().expect("socket not yet readable");
            guard
                .connection_fd()
                .try_clone_to_owned()
                .map_err(|error| format!("could not clone wayland socket: {error}"))?
        };
        let mut source = SourceFd(&socket.as_raw_fd());
        poll.registry()
            .register(&mut source, Token(0), Interest::READABLE)
            .map_err(|error| format!("could not register wayland socket: {error}"))?;

        let mut result = Ok(());
        while !state.done {
            state.drain_commands();

            if let Err(error) = queue.dispatch_pending(&mut state) {
                result = Err(format!("wayland dispatch failed: {error}"));
                break;
            }
            let _ = queue.flush();
            state.publish_progress();

            if state.done {
                break;
            }

            if let Err(error) = poll.poll(&mut events, Some(Duration::from_millis(50))) {
                result = Err(format!("poll failed: {error}"));
                break;
            }
            for event in events.iter() {
                if event.token() == Token(0)
                    && event.is_readable()
                    && let Some(guard) = queue.prepare_read()
                {
                    match guard.read() {
                        Ok(_) => {}
                        Err(wayland_backend::client::WaylandError::Io(io))
                            if io.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(error) => {
                            result = Err(format!("wayland socket read failed: {error}"));
                            state.done = true;
                            break;
                        }
                    }
                }
            }
        }

        if let Err(reason) = &result {
            state.failure = Some(reason.clone());
            state.done = true;
            state.kept = false;
        }
        state.cleanup();
        let _ = queue.flush();
        result
    }

    fn wait_for_delay(&self) -> Result<bool, String> {
        let deadline = std::time::Instant::now() + self.config.delay;
        while std::time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            let timeout = remaining.min(Duration::from_millis(100));
            let Some(receiver) = self.command_rx.as_ref() else {
                return Err("recording command channel is unavailable".into());
            };
            match receiver.recv_timeout(timeout) {
                Ok(WorkerCommand::Cancel | WorkerCommand::Stop) => {
                    self.set_state(RecordingState::Cancelled);
                    return Ok(false);
                }
                Ok(WorkerCommand::Pause | WorkerCommand::Resume) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("recording controller disconnected during startup".into());
                }
            }
        }
        Ok(true)
    }

    fn set_state(&self, state: RecordingState) {
        *lock_recover(&self.shared) = state;
    }
}

impl WorkerState {
    fn bind_outputs(&mut self, qh: &QueueHandle<WorkerState>) {
        let registry = self.gm.registry().clone();
        for g in self.gm.contents().clone_list() {
            if g.interface == WlOutput::interface().name {
                self.create_wl_output(&registry, g.name, g.version, qh);
            }
        }
    }

    fn create_wl_output(
        &mut self,
        registry: &WlRegistry,
        name: u32,
        version: u32,
        qh: &QueueHandle<WorkerState>,
    ) {
        let output: WlOutput = registry.bind(name, version, qh, ());
        let _xdg = self
            .xdg_output_manager
            .get_xdg_output(&output, qh, output.id());
        self.partial_outputs.insert(
            output.id(),
            PartialOutputInfo {
                global_name: name,
                name: None,
                loc: None,
                size_pixels: None,
                refresh: None,
                output,
                has_recvd_done: false,
                transform: None,
            },
        );
    }

    fn bind_toplevel_list(&mut self, qh: &QueueHandle<WorkerState>) {
        let list: ExtForeignToplevelListV1 =
            match self
                .gm
                .bind(qh, 1..=ExtForeignToplevelListV1::interface().version, ())
            {
                Ok(list) => list,
                Err(_) => return,
            };
        self.toplevel_list = Some(list);
    }

    fn update_output_info(&mut self, id: &ObjectId, f: impl FnOnce(&mut PartialOutputInfo)) {
        if let Some(output) = self.partial_outputs.get_mut(id) {
            f(output);
        }
    }

    fn drain_commands(&mut self) {
        while let Ok(command) = self.command_rx.try_recv() {
            match command {
                WorkerCommand::Pause => self.on_pause(),
                WorkerCommand::Resume => self.on_resume(),
                WorkerCommand::Stop => self.on_stop(),
                WorkerCommand::Cancel => self.on_cancel(),
            }
        }
    }

    fn on_pause(&mut self) {
        if self.enc.is_none() || !self.clock.pause() {
            return;
        }
        if let Some(enc) = &self.enc {
            enc.audio_pause();
        }
        let elapsed = self.clock.elapsed();
        self.set_state(RecordingState::Paused {
            elapsed,
            path: self.config.path.clone(),
            source: self.config.source.clone(),
            stats: self.stats(),
        });
    }

    fn on_resume(&mut self) {
        if self.enc.is_none() {
            return;
        }
        if !self.clock.resume() {
            return;
        }
        let elapsed = self.clock.elapsed();
        if let Some(enc) = &self.enc {
            enc.audio_resume();
        }
        if self
            .enc
            .as_ref()
            .is_some_and(|enc| enc.encoded_frames() > 0)
        {
            self.set_state(RecordingState::Recording {
                elapsed,
                path: self.config.path.clone(),
                source: self.config.source.clone(),
                stats: self.stats(),
            });
        } else {
            self.set_state(RecordingState::Starting {
                path: self.config.path.clone(),
            });
        }
    }

    fn on_stop(&mut self) {
        if self.done {
            return;
        }
        self.done = true;
        self.kept = true;
    }

    fn on_cancel(&mut self) {
        if self.done {
            return;
        }
        self.done = true;
        self.kept = false;
    }

    fn stats(&self) -> crate::types::RecordingStats {
        let mut stats = crate::types::RecordingStats::default();
        if let Some(enc) = &self.enc {
            stats.captured_frames = enc.captured_frames();
            stats.encoded_frames = enc.encoded_frames();
            stats.dropped_frames = enc.dropped_frames();
            stats.audio_samples = enc.audio_samples();
            stats.bytes_written = enc.bytes_written();
        }
        stats
    }

    fn publish_progress(&mut self) {
        if self.done || self.enc.is_none() {
            return;
        }
        if let Some(reason) = self.enc.as_ref().and_then(EncState::take_audio_error) {
            self.done = true;
            self.kept = false;
            self.failure = Some(reason.clone());
            self.set_state(RecordingState::Failed {
                reason,
                partial_path: None,
            });
            return;
        }
        let elapsed = self.clock.elapsed();
        if self.clock.is_paused() {
            self.set_state(RecordingState::Paused {
                elapsed,
                path: self.config.path.clone(),
                source: self.config.source.clone(),
                stats: self.stats(),
            });
        } else if self
            .enc
            .as_ref()
            .is_some_and(|enc| enc.encoded_frames() > 0)
        {
            self.set_state(RecordingState::Recording {
                elapsed,
                path: self.config.path.clone(),
                source: self.config.source.clone(),
                stats: self.stats(),
            });
        }
    }

    fn set_state(&self, state: RecordingState) {
        *lock_recover(&self.shared) = state;
    }

    /// Stop requesting frames and seal (or discard) the output file.
    fn cleanup(&mut self) {
        let duration = self.clock.elapsed();
        let path = self.config.path.clone();
        let path_part = self.config.path_part.clone();

        if self.kept && self.failure.is_none() {
            self.set_state(RecordingState::Finalizing { path: path.clone() });
        }

        // Drop the capture session first so the compositor stops sending frames.
        self.cap = None;
        let terminal = finish(
            self.enc.take(),
            Finalization {
                path,
                partial_path: path_part,
                audio: self.config.audio,
                duration,
                keep: self.kept,
                failure: self.failure.take(),
                warning: self.completion_warning.take(),
            },
        );
        self.set_state(terminal);
    }

    fn start_if_output_probe_complete(&mut self, qh: &QueueHandle<WorkerState>) {
        if self.cap.is_some() {
            return;
        }
        if self.outputs.len() != self.partial_outputs.len() {
            return;
        }

        let target = match select_output(self.outputs.values(), &self.config.source) {
            Ok(Some(target)) => target,
            Ok(None) => return,
            Err(reason) => {
                self.fail(&reason, qh);
                return;
            }
        };

        info!("capturing output {}", target.name);
        self.selected_output = Some(target.clone());

        let (width, height) = target.size_screen_space();
        let roi = Rect::new((0, 0), (width, height));

        if roi.w == 0 || roi.h == 0 {
            self.fail("recording source has no captureable area", qh);
            return;
        }

        let cap = match CaptureSession::new_for_output(
            &self.gm,
            qh,
            target.output.clone(),
            self.config.paint_cursor,
        ) {
            Ok(cap) => cap,
            Err(reason) => {
                self.fail(&reason, qh);
                return;
            }
        };
        self.cap = Some(cap);
        self.roi = Some(roi);
        self.queue_alloc_frame(qh);
    }

    fn select_toplevel(&mut self, qh: &QueueHandle<WorkerState>) {
        if self.cap.is_some() {
            return;
        }
        let (target_identifier, target_app_id, target_title) = match &self.config.source {
            RecordingSource::Window {
                identifier,
                app_id,
                title,
            } => (identifier, app_id, title),
            _ => return,
        };

        let Some(handle) = select_toplevel(self.toplevels.values(), &self.config.source) else {
            return;
        };

        info!("capturing window {target_identifier} ({target_app_id}: {target_title})");
        let cap = match CaptureSession::new_for_toplevel(
            &self.gm,
            qh,
            handle,
            self.config.paint_cursor,
        ) {
            Ok(cap) => cap,
            Err(reason) => {
                self.fail(&reason, qh);
                return;
            }
        };
        self.cap = Some(cap);
        // Toplevel dimensions arrive with the capture constraints. Keeping the
        // ROI absent makes negotiation use that full, non-zero buffer size.
        self.roi = None;
        self.queue_alloc_frame(qh);
    }

    pub(crate) fn fail(&mut self, reason: &str, _qh: &QueueHandle<WorkerState>) {
        error!("{reason}");
        self.done = true;
        self.kept = false;
        self.failure = Some(reason.to_string());
        *lock_recover(&self.shared) = RecordingState::Failed {
            reason: reason.to_string(),
            partial_path: None,
        };
    }

    pub(crate) fn negotiate_format(
        &mut self,
        capture_formats: &[DmabufPotentialFormat],
        (w, h): (u32, u32),
        dri_device: Option<&Path>,
        qh: &QueueHandle<WorkerState>,
    ) {
        debug!("supported capture formats are {w}x{h} {capture_formats:?}");
        let Some(dri_device) = dri_device else {
            self.fail("compositor did not advertise a DRM render device", qh);
            return;
        };

        let selected_format = match negotiate_format_impl(w as i32, h as i32, capture_formats) {
            Ok(f) => f,
            Err(e) => {
                self.fail(&e.to_string(), qh);
                return;
            }
        };

        let (refresh, transform) = match self.output_info() {
            Some(o) => (o.refresh, o.transform),
            None => (Rational(60000, 1000), Transform::Normal),
        };

        if let Some(current_format) = &self.selected_format {
            if current_format == &selected_format {
                if matches!(self.in_flight_surface, InFlightSurface::Allocd(_)) {
                    self.queue_frame_capture(qh);
                }
                return;
            }
            if self
                .selected_device
                .as_deref()
                .is_some_and(|device| device != dri_device)
            {
                self.fail("capture source moved to a different DRM device", qh);
                return;
            }
            let pending = PendingCaptureFormat {
                format: selected_format,
                transform,
                device: dri_device.to_path_buf(),
            };
            if matches!(self.in_flight_surface, InFlightSurface::CopyQueued { .. }) {
                self.pending_capture_format = Some(pending);
            } else {
                self.apply_capture_format(pending, qh);
                if !self.done {
                    match &self.in_flight_surface {
                        InFlightSurface::None => self.queue_alloc_frame(qh),
                        InFlightSurface::Allocd(_) => self.queue_frame_capture(qh),
                        InFlightSurface::AllocQueued | InFlightSurface::CopyQueued { .. } => {}
                    }
                }
            }
            return;
        }

        let roi = self.roi.unwrap_or(Rect::new(
            (0, 0),
            (selected_format.width, selected_format.height),
        ));

        let enc = match EncState::new(
            &self.config.path_part,
            selected_format.clone(),
            refresh,
            transform,
            roi,
            dri_device,
            self.config.framerate,
            self.config.bitrate_bytes_per_second,
            self.config.gop_size,
            self.config.audio,
        ) {
            Ok(enc) => enc,
            Err(e) => {
                self.fail(&e, qh);
                return;
            }
        };

        self.enc = Some(enc);
        self.selected_format = Some(selected_format);
        self.selected_device = Some(dri_device.to_path_buf());
        self.clock.start();
        match &self.in_flight_surface {
            InFlightSurface::None => self.queue_alloc_frame(qh),
            InFlightSurface::AllocQueued | InFlightSurface::Allocd(_) => {
                self.queue_frame_capture(qh)
            }
            InFlightSurface::CopyQueued { .. } => {}
        }
    }

    fn apply_capture_format(
        &mut self,
        pending: PendingCaptureFormat,
        qh: &QueueHandle<WorkerState>,
    ) {
        let Some(enc) = self.enc.as_mut() else {
            return;
        };
        if let Err(reason) = enc.reconfigure_capture(pending.format.clone(), pending.transform) {
            self.fail(&reason, qh);
            return;
        }
        info!(
            "recording source changed to {}x{}; continuing on the stable encoded canvas",
            pending.format.width, pending.format.height
        );
        self.selected_format = Some(pending.format);
        self.selected_device = Some(pending.device);
    }

    fn apply_pending_capture_format(&mut self, qh: &QueueHandle<WorkerState>) -> bool {
        let Some(pending) = self.pending_capture_format.take() else {
            return false;
        };
        self.apply_capture_format(pending, qh);
        true
    }

    fn output_info(&self) -> Option<OutputInfo> {
        self.selected_output.clone()
    }

    fn queue_alloc_frame(&mut self, qh: &QueueHandle<WorkerState>) {
        assert!(matches!(self.in_flight_surface, InFlightSurface::None));
        let Some(cap) = &mut self.cap else {
            return;
        };
        let frame = cap.alloc_frame(qh);
        self.in_flight_surface = InFlightSurface::AllocQueued;
        self.on_frame_allocd(qh, &frame);
    }

    fn on_frame_allocd(&mut self, qh: &QueueHandle<WorkerState>, frame: &CaptureFrame) {
        assert!(matches!(
            self.in_flight_surface,
            InFlightSurface::AllocQueued
        ));
        self.in_flight_surface = InFlightSurface::Allocd(frame.clone());

        if self.enc.is_some() {
            self.queue_frame_capture(qh);
        }
    }

    fn queue_frame_capture(&mut self, qh: &QueueHandle<WorkerState>) {
        let (Some(enc), Some(cap)) = (self.enc.as_mut(), self.cap.as_ref()) else {
            return;
        };
        let InFlightSurface::Allocd(frame) = &self.in_flight_surface else {
            panic!("queue_frame_capture called in a strange state");
        };

        let mut av_surface = match enc.alloc_capture_surface() {
            Ok(s) => s,
            Err(e) => {
                error!("failed to allocate vaapi capture surface: {e}");
                self.fail(
                    &format!("failed to allocate vaapi capture surface: {e}"),
                    qh,
                );
                return;
            }
        };
        av_surface.set_color_space(ffmpeg::color::Space::RGB);

        let (desc, av_mapping) = map_drm(&av_surface);

        assert_eq!(desc.nb_layers, 1);

        let Some(format) = &self.selected_format else {
            return;
        };

        let wl_buffer_params = self.dma.create_params(qh, ());

        for i in 0..desc.layers[0].nb_planes {
            let oid = desc.layers[0].planes[i as usize].object_index;
            assert!(oid < desc.nb_objects);
            let object = &desc.objects[oid as usize];
            let plane = &desc.layers[0].planes[i as usize];
            let modifier = object.format_modifier.to_be_bytes();
            let fd = unsafe { BorrowedFd::borrow_raw(object.fd) };
            wl_buffer_params.add(
                fd,
                i as u32,
                plane.offset as u32,
                plane.pitch as u32,
                u32::from_be_bytes(modifier[..4].try_into().unwrap()),
                u32::from_be_bytes(modifier[4..].try_into().unwrap()),
            );
        }
        let wl_buffer = wl_buffer_params.create_immed(
            format.width,
            format.height,
            format.fourcc as u32,
            zwp_linux_buffer_params_v1::Flags::empty(),
            qh,
            (),
        );

        cap.queue_copy(&wl_buffer, (format.width, format.height), frame);

        self.in_flight_surface = InFlightSurface::CopyQueued {
            av_surface,
            av_mapping,
            capture_frame: frame.clone(),
            wl_buffer,
        };
    }

    pub(crate) fn on_copy_complete(
        &mut self,
        qh: &QueueHandle<WorkerState>,
        tv_sec_hi: u32,
        tv_sec_lo: u32,
        tv_nsec: u32,
    ) {
        self.consecutive_capture_failures = 0;
        let Some(cap) = self.cap.as_ref() else {
            return;
        };

        let mut surf = if let InFlightSurface::CopyQueued {
            av_surface,
            av_mapping,
            capture_frame,
            wl_buffer,
        } = self.in_flight_surface.take()
        {
            drop(av_mapping);
            cap.on_done_with_frame(&capture_frame);
            wl_buffer.destroy();
            av_surface
        } else {
            panic!("on_copy_complete called in a strange state");
        };

        let pending_transform = self.pending_transform.take();
        let pending_y_invert = self.pending_y_invert.take();
        if pending_transform.is_some() || pending_y_invert.is_some() {
            let result = self.enc.as_mut().map(|encoder| {
                let transform = pending_transform.unwrap_or_else(|| encoder.transform());
                let y_invert = pending_y_invert.unwrap_or_else(|| encoder.y_invert());
                encoder.reconfigure_view(transform, y_invert)
            });
            if let Some(Err(reason)) = result {
                self.fail(&reason, qh);
                return;
            }
            // The copied surface belongs to the old capture-side frame pool.
            // Request a fresh one after rebuilding the view transform.
            drop(surf);
            self.apply_pending_capture_format(qh);
            if !self.done {
                self.queue_alloc_frame(qh);
            }
            return;
        }

        let Some(enc) = self.enc.as_mut() else {
            return;
        };

        let secs = (i64::from(tv_sec_hi) << 32) + i64::from(tv_sec_lo);
        let pts_abs = secs * 1_000_000_000 + i64::from(tv_nsec);

        if self.starting_timestamp.is_none() {
            self.starting_timestamp = Some(pts_abs);
            enc.audio_start();
        }
        let pts = adjusted_presentation_time(
            pts_abs,
            self.starting_timestamp.unwrap(),
            self.clock.paused_total(),
        );
        surf.set_pts(Some(pts));
        unsafe {
            (*surf.as_mut_ptr()).time_base.num = 1;
            (*surf.as_mut_ptr()).time_base.den = 1_000_000_000;
        }

        if !self.clock.is_paused() {
            enc.push_with_fpslimit(surf);
        }
        let encoded_frames = enc.encoded_frames();

        let elapsed = self.clock.elapsed();
        if !self.clock.is_paused() && encoded_frames > 0 {
            self.set_state(RecordingState::Recording {
                elapsed,
                path: self.config.path.clone(),
                source: self.config.source.clone(),
                stats: self.stats(),
            });
        }

        self.apply_pending_capture_format(qh);
        if !self.done {
            self.queue_alloc_frame(qh);
        }
    }

    pub(crate) fn on_copy_fail(&mut self, qh: &QueueHandle<WorkerState>) {
        let Some(cap) = self.cap.as_ref() else {
            return;
        };
        match self.in_flight_surface.take() {
            InFlightSurface::CopyQueued {
                av_surface,
                av_mapping,
                capture_frame,
                wl_buffer,
            } => {
                drop(av_mapping);
                cap.on_done_with_frame(&capture_frame);
                wl_buffer.destroy();
                drop(av_surface);
            }
            InFlightSurface::Allocd(capture_frame) => {
                cap.on_done_with_frame(&capture_frame);
            }
            InFlightSurface::None | InFlightSurface::AllocQueued => {}
        }

        self.consecutive_capture_failures = self.consecutive_capture_failures.saturating_add(1);
        if self.consecutive_capture_failures >= 5 {
            self.fail("compositor repeatedly failed to copy recording frames", qh);
        } else {
            debug!("copy failed, trying to capture a new frame...");
            self.apply_pending_capture_format(qh);
            if !self.done {
                self.queue_alloc_frame(qh);
            }
        }
    }

    pub(crate) fn on_frame_transform(&mut self, transform: Transform) {
        let Some(enc) = &self.enc else {
            return;
        };
        if enc.transform() == transform {
            return;
        }
        self.pending_transform = Some(transform);
    }

    pub(crate) fn on_frame_y_invert(&mut self, y_invert: bool) {
        let Some(enc) = self.enc.as_ref() else {
            return;
        };
        if enc.y_invert() != y_invert {
            self.pending_y_invert = Some(y_invert);
        }
    }
}

fn adjusted_presentation_time(absolute: i64, origin: i64, paused: Duration) -> i64 {
    let paused_ns = paused.as_nanos().min(i64::MAX as u128) as i64;
    absolute
        .saturating_sub(origin)
        .saturating_sub(paused_ns)
        .max(0)
}

/// Map a VAAPI capture surface to a DRM descriptor for buffer import.
fn map_drm(
    frame: &ffmpeg::frame::Video,
) -> (ffmpeg::ffi::AVDRMFrameDescriptor, ffmpeg::frame::Video) {
    let mut dst = ffmpeg::frame::Video::empty();
    dst.set_format(Pixel::DRM_PRIME);
    unsafe {
        let sts = ffmpeg::ffi::av_hwframe_map(
            dst.as_mut_ptr(),
            frame.as_ptr(),
            ffmpeg::ffi::AV_HWFRAME_MAP_WRITE as c_int,
        );
        assert_eq!(sts, 0);
        (
            *((*dst.as_ptr()).data[0] as *const ffmpeg::ffi::AVDRMFrameDescriptor),
            dst,
        )
    }
}

/// Pick a preferred capture format from the compositor's offers.
fn negotiate_format_impl(
    width: i32,
    height: i32,
    capture_formats: &[DmabufPotentialFormat],
) -> Result<DmabufFormat, String> {
    for preferred_format in [
        DrmFourcc::Xrgb8888,
        DrmFourcc::Xbgr8888,
        DrmFourcc::Xrgb2101010,
    ] {
        let find = capture_formats
            .iter()
            .find(|p| p.fourcc == preferred_format && p.modifiers.contains(&0));

        if let Some(find) = find {
            return Ok(DmabufFormat {
                width,
                height,
                fourcc: find.fourcc,
                modifiers: find.modifiers.clone(),
            });
        }
    }
    Err(format!(
        "failed to select a viable capture format; compositor offered {capture_formats:?}"
    ))
}

impl Dispatch<WlRegistry, GlobalListContents> for WorkerState {
    fn event(
        state: &mut Self,
        proxy: &WlRegistry,
        event: <WlRegistry as Proxy>::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::GlobalRemove { name } => {
                for (id, info) in &state.partial_outputs {
                    if info.global_name == name {
                        state.outputs.insert(id.clone(), None);
                    }
                }
            }
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } if interface == WlOutput::interface().name => {
                state.create_wl_output(proxy, name, version, qh);
            }
            _ => {}
        }
    }
}

impl Dispatch<WlOutput, ()> for WorkerState {
    fn event(
        state: &mut Self,
        proxy: &WlOutput,
        event: <WlOutput as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let id = proxy.id();
        match event {
            wl_output::Event::Mode {
                refresh,
                flags: WEnum::Value(flags),
                width,
                height,
            } => {
                if flags.contains(wl_output::Mode::Current) {
                    state.update_output_info(&id, |info| {
                        info.refresh = Some(Rational(refresh, 1000));
                        info.size_pixels = Some((width, height));
                    });
                }
            }
            wl_output::Event::Geometry { transform, .. } => match transform {
                WEnum::Value(v) => {
                    state.update_output_info(&id, |info| info.transform = Some(v));
                }
                WEnum::Unknown(u) => warn!("unknown output transform value: {u}"),
            },
            wl_output::Event::Done => {
                state.on_output_done(id, qh);
            }
            _ => {}
        }
    }
}

impl Dispatch<ZxdgOutputV1, ObjectId> for WorkerState {
    fn event(
        state: &mut Self,
        _proxy: &ZxdgOutputV1,
        event: <ZxdgOutputV1 as Proxy>::Event,
        out_id: &ObjectId,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zxdg_output_v1::Event::Name { name } => {
                state.update_output_info(out_id, |info| info.name = Some(name));
            }
            zxdg_output_v1::Event::LogicalPosition { x, y } => {
                state.update_output_info(out_id, |info| info.loc = Some((x, y)));
            }
            zxdg_output_v1::Event::LogicalSize { .. } => {}
            zxdg_output_v1::Event::Done => {
                state.on_output_done(out_id.clone(), qh);
            }
            _ => {}
        }
    }
}

impl WorkerState {
    fn on_output_done(&mut self, id: ObjectId, qh: &QueueHandle<WorkerState>) {
        let output = match self.partial_outputs.get_mut(&id) {
            Some(o) => o,
            None => return,
        };
        if !output.has_recvd_done {
            output.has_recvd_done = true;
            return;
        }
        if let Some(info) = output.complete() {
            self.outputs.insert(id, Some(info));
        } else {
            self.outputs.insert(id, None);
        }

        self.start_if_output_probe_complete(qh);
    }
}

impl Dispatch<ZxdgOutputManagerV1, ()> for WorkerState {
    fn event(
        _state: &mut Self,
        _proxy: &ZxdgOutputManagerV1,
        _event: <ZxdgOutputManagerV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpLinuxDmabufV1, ()> for WorkerState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwpLinuxDmabufV1,
        _event: <ZwpLinuxDmabufV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpLinuxBufferParamsV1, ()> for WorkerState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwpLinuxBufferParamsV1,
        _event: <ZwpLinuxBufferParamsV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlBuffer, ()> for WorkerState {
    fn event(
        _state: &mut Self,
        _proxy: &WlBuffer,
        _event: <WlBuffer as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtForeignToplevelListV1, ()> for WorkerState {
    fn event(
        state: &mut Self,
        _proxy: &ExtForeignToplevelListV1,
        event: <ExtForeignToplevelListV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            ext_foreign_toplevel_list_v1::Event::Toplevel { toplevel } => {
                state.toplevels.insert(
                    toplevel.id(),
                    ToplevelProbe {
                        handle: toplevel,
                        identifier: String::new(),
                        title: String::new(),
                        app_id: String::new(),
                    },
                );
            }
            ext_foreign_toplevel_list_v1::Event::Finished => {}
            _ => {}
        }
        state.select_toplevel(qh);
    }
}

impl Dispatch<ExtForeignToplevelHandleV1, ()> for WorkerState {
    fn event(
        state: &mut Self,
        proxy: &ExtForeignToplevelHandleV1,
        event: <ExtForeignToplevelHandleV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let id = proxy.id();
        match event {
            ext_foreign_toplevel_handle_v1::Event::Identifier { identifier } => {
                if let Some(t) = state.toplevels.get_mut(&id) {
                    t.identifier = identifier;
                }
            }
            ext_foreign_toplevel_handle_v1::Event::Title { title } => {
                if let Some(t) = state.toplevels.get_mut(&id) {
                    t.title = title;
                }
            }
            ext_foreign_toplevel_handle_v1::Event::AppId { app_id } => {
                if let Some(t) = state.toplevels.get_mut(&id) {
                    t.app_id = app_id;
                }
            }
            _ => {}
        }
        state.select_toplevel(qh);
    }
}

#[cfg(test)]
mod tests {
    use super::adjusted_presentation_time;
    use std::time::Duration;

    #[test]
    fn paused_time_is_removed_from_media_timestamps() {
        assert_eq!(
            adjusted_presentation_time(9_000_000_000, 2_000_000_000, Duration::from_secs(3)),
            4_000_000_000
        );
    }

    #[test]
    fn media_timestamps_never_become_negative() {
        assert_eq!(
            adjusted_presentation_time(2_000, 1_000, Duration::from_secs(1)),
            0
        );
    }
}
