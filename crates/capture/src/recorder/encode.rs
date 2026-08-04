//! FFmpeg muxing + VAAPI VP8 encoding state for the in-process recorder.
//!
//! `EncState` owns the output container, the hardware (VAAPI) device and frame
//! contexts, the video filter graph (crop/scale/transpose on the GPU), the VP8
//! encoder, and forwards encoded video/audio packets into the muxer.
//!
//! Adapted from `wl-screenrec` (`src/main.rs`), licensed under the Apache
//! License, Version 2.0. Copyright (c) wl-screenrec contributors.
//! This file is a derived work distributed under the Apache-2.0 license.

use std::{path::Path, ptr::null_mut, sync::mpsc::Receiver, time::Duration};

use ffmpeg::{
    Packet, Rational, codec, dict, encoder, filter,
    format::{self, Pixel},
    frame,
};
use log::{info, warn};
use wayland_client::protocol::wl_output::Transform;

use crate::recorder::{
    audio::{AudioCommand, AudioHandle, create_opus_encoder},
    avhw::{AvHwDevCtx, AvHwFrameCtx, Tiling},
    fps_limit::FpsLimit,
    transform::{Rect, transpose_if_transform_transposed},
};
use crate::types::RecordingAudio;

/// Whether FFmpeg exposes a hardware VAAPI encoder for the codec.
fn hw_encoder_available(codec_name: &str) -> bool {
    encoder::find_by_name(codec_name).is_some()
}

/// Map a captured DRM fourcc to the FFmpeg pixel format the VAAPI surface
/// frame context should advertise as its software format.
fn dmabuf_to_av(fourcc: drm::buffer::DrmFourcc) -> Pixel {
    use drm::buffer::DrmFourcc;
    match fourcc {
        DrmFourcc::Xrgb8888 => Pixel::BGRZ,
        DrmFourcc::Xbgr8888 => Pixel::RGBZ,
        DrmFourcc::Xrgb2101010 => Pixel::X2RGB10LE,
        _ => Pixel::None,
    }
}

/// The negotiated capture format (dimensions + DRM modifier candidates).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DmabufFormat {
    pub width: i32,
    pub height: i32,
    pub fourcc: drm::buffer::DrmFourcc,
    pub modifiers: Vec<u64>,
}

impl DmabufFormat {
    /// The FFmpeg pixel format for this capture format, if supported.
    pub fn av_pixfmt(&self) -> Pixel {
        dmabuf_to_av(self.fourcc)
    }
}

/// A candidate capture format offered by the compositor.
#[derive(Clone, Debug)]
pub struct DmabufPotentialFormat {
    pub fourcc: drm::buffer::DrmFourcc,
    pub modifiers: Vec<u64>,
}

/// Video encoding state: muxer, hardware contexts, filter graph, encoder.
pub struct EncState {
    octx: format::context::Output,
    video_filter: filter::Graph,
    enc_video: encoder::Video,
    enc_video_has_been_fed_any_frames: bool,
    frames_rgb: AvHwFrameCtx,
    // Kept alive for the lifetime of the encode session: the VAAPI frame/device
    // contexts must outlive the filters and encoder that reference them.
    #[allow(dead_code)]
    frames_yuv: AvHwFrameCtx,
    #[allow(dead_code)]
    hw_device_ctx: AvHwDevCtx,
    filter_output_timebase: Rational,
    vid_stream_idx: usize,
    #[allow(dead_code)]
    roi_screen_coord: Rect,
    transform: Transform,
    y_invert: bool,
    capture_format: DmabufFormat,
    encode_size: (i32, i32),
    framerate: u32,
    fps_limit: Option<FpsLimit<frame::Video>>,
    audio: Option<AudioHandle>,
    audio_packet_rx: Option<Receiver<Packet>>,
    bytes_written: u64,
    encoded_frames: u64,
    captured_frames: u64,
    dropped_frames: u64,
    audio_samples: u64,
    audio_packets: u64,
    io_error: Option<String>,
    pending_video_packet: Option<Packet>,
}

fn supported_formats(codec: &ffmpeg::Codec) -> Vec<Pixel> {
    unsafe {
        let mut frmts = Vec::new();
        let mut fmt_ptr = (*codec.as_ptr()).pix_fmts;
        while !fmt_ptr.is_null() && *fmt_ptr as std::os::raw::c_int != -1 {
            frmts.push(Pixel::from(*fmt_ptr));
            fmt_ptr = fmt_ptr.add(1);
        }
        frmts
    }
}

/// Build the VAAPI VP8 encoder for a given resolution and frame rate.
#[allow(clippy::too_many_arguments)]
fn make_video_params(
    enc_pix_fmt: Pixel,
    (encode_w, encode_h): (i32, i32),
    framerate: Rational,
    global_header: bool,
    hw_device_ctx: &mut AvHwDevCtx,
    frames_yuv: &mut AvHwFrameCtx,
    bitrate_bytes_per_second: usize,
    gop_size: u32,
) -> Result<encoder::video::Video, String> {
    let codec = encoder::find_by_name("vp8_vaapi")
        .ok_or_else(|| "vp8_vaapi encoder is not available in this ffmpeg build".to_string())?;

    let mut enc = unsafe {
        codec::context::Context::wrap(ffmpeg::ffi::avcodec_alloc_context3(codec.as_ptr()), None)
    }
    .encoder()
    .video()
    .map_err(|error| format!("could not create vaapi video encoder: {error}"))?;

    enc.set_bit_rate(bitrate_bytes_per_second * 8);
    enc.set_width(encode_w as u32);
    enc.set_height(encode_h as u32);
    enc.set_time_base(Rational(1, 1_000_000_000));
    enc.set_frame_rate(Some(framerate));
    enc.set_gop(gop_size);

    if global_header {
        enc.set_flags(codec::Flags::GLOBAL_HEADER);
    }

    enc.set_format(Pixel::VAAPI);

    unsafe {
        (*enc.as_mut_ptr()).hw_device_ctx = ffmpeg::ffi::av_buffer_ref(hw_device_ctx.as_mut_ptr());
        (*enc.as_mut_ptr()).hw_frames_ctx = ffmpeg::ffi::av_buffer_ref(frames_yuv.as_mut_ptr());
        (*enc.as_mut_ptr()).sw_pix_fmt = enc_pix_fmt.into();
    }

    Ok(enc)
}

/// Build the video filter graph: crop the capture surface to the region,
/// transpose according to the output transform, scale to the encode
/// resolution, and convert to the encoder's pixel format (all in VAAPI).
///
/// The hardware frames context must be attached to the buffersrc so the graph
/// can allocate surfaces from the VAAPI pool; this is done through the raw
/// `av_buffersrc_parameters_set` API because the safe wrapper has no way to
/// set `hw_frames_ctx`.
fn video_filter(
    inctx: &mut AvHwFrameCtx,
    (capture_width, capture_height): (i32, i32),
    roi_screen_coord: Rect,
    (enc_w, enc_h): (i32, i32),
    transform: Transform,
    y_invert: bool,
) -> (filter::Graph, Rational) {
    use std::ffi::{CStr, c_int};

    let mut g = filter::graph::Graph::new();

    // buffersrc
    unsafe {
        let buffersrc_ctx = ffmpeg::ffi::avfilter_graph_alloc_filter(
            g.as_mut_ptr(),
            filter::find("buffer").unwrap().as_mut_ptr(),
            c"in".as_ptr() as _,
        );
        if buffersrc_ctx.is_null() {
            panic!("failed to alloc buffersrc filter");
        }

        let p = &mut *ffmpeg::ffi::av_buffersrc_parameters_alloc();

        p.width = capture_width;
        p.height = capture_height;
        p.format = Pixel::VAAPI as c_int;
        p.time_base.num = 1;
        p.time_base.den = 1_000_000_000;
        p.hw_frames_ctx = inctx.as_mut_ptr();

        let sts = ffmpeg::ffi::av_buffersrc_parameters_set(buffersrc_ctx, p as *mut _);
        assert_eq!(sts, 0);
        ffmpeg::ffi::av_free(p as *mut _ as *mut _);

        let sts = ffmpeg::ffi::avfilter_init_dict(buffersrc_ctx, null_mut());
        assert_eq!(sts, 0);
    }

    // buffersink: accept the encoder pixel format (VAAPI).
    let buffersink_args = format!("pixel_formats={}", {
        let c_name = unsafe { ffmpeg::ffi::av_get_pix_fmt_name(Pixel::VAAPI.into()) };
        assert!(!c_name.is_null());
        unsafe { CStr::from_ptr(c_name).to_str().unwrap().to_string() }
    });
    g.add(
        &filter::find("buffersink").unwrap(),
        "out",
        &buffersink_args,
    )
    .unwrap();

    let Rect {
        x: roi_x,
        y: roi_y,
        w: roi_w,
        h: roi_h,
    } = roi_screen_coord.screen_to_frame(capture_width, capture_height, transform);

    let (scale_w, scale_h) = transpose_if_transform_transposed((enc_w, enc_h), transform);

    let transpose = match transform {
        Transform::_90 => ",transpose_vaapi=dir=clock",
        Transform::_180 => ",transpose_vaapi=dir=reversal",
        Transform::_270 => ",transpose_vaapi=dir=cclock",
        Transform::Flipped => ",transpose_vaapi=dir=hflip",
        Transform::Flipped90 => ",transpose_vaapi=dir=cclock_flip",
        Transform::Flipped180 => ",transpose_vaapi=dir=vflip",
        Transform::Flipped270 => ",transpose_vaapi=dir=clock_flip",
        _ => "",
    };

    let scale = format!(",scale_vaapi=format=nv12:w={scale_w}:h={scale_h}");

    let y_flip = if y_invert {
        "transpose_vaapi=dir=vflip,"
    } else {
        ""
    };
    let filtergraph =
        format!("{y_flip}crop={roi_w}:{roi_h}:{roi_x}:{roi_y}:exact=1{scale}{transpose}");

    g.output("in", 0)
        .unwrap()
        .input("out", 0)
        .unwrap()
        .parse(&filtergraph)
        .unwrap();

    info!("video filter graph: {}", g.dump());

    g.validate().unwrap();

    (g, Rational(1, 1_000_000_000))
}

impl EncState {
    /// Open the output file, negotiate the hardware pipeline, and write the
    /// container header. Fails before anything is written to `path` if any
    /// required component (VAAPI, VP8, WebM muxer, audio encoder) is missing.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        path: &Path,
        capture_format: DmabufFormat,
        refresh: Rational,
        transform: Transform,
        roi_screen_coord: Rect,
        dri_device: &Path,
        framerate: u32,
        bitrate_bytes_per_second: usize,
        gop_size: u32,
        audio_source: RecordingAudio,
    ) -> Result<Self, String> {
        let mut octx = format::output(path)
            .map_err(|error| format!("could not open output file {}: {error}", path.display()))?;

        let encoder = encoder::find_by_name("vp8_vaapi")
            .ok_or_else(|| "vp8_vaapi encoder is not available".to_string())?;

        let enc_pixfmt = if supported_formats(&encoder).contains(&Pixel::VAAPI) {
            Pixel::NV12
        } else {
            return Err("vp8_vaapi does not advertise VAAPI input support".into());
        };

        let codec_id = encoder.id();
        match unsafe {
            ffmpeg::ffi::avformat_query_codec(
                octx.format().as_ptr(),
                codec_id.into(),
                ffmpeg::ffi::FF_COMPLIANCE_STRICT,
            )
        } {
            0 => {
                return Err(format!(
                    "muxer {} does not support the vp8 codec",
                    octx.format().name()
                ));
            }
            1 => (),
            e => warn!(
                "muxer {} may not support the vp8 codec ({e})",
                octx.format().name()
            ),
        }

        let global_header = octx.format().flags().contains(format::Flags::GLOBAL_HEADER);

        info!("opening libva device from {}", dri_device.display());
        let mut hw_device_ctx = AvHwDevCtx::new_libva(dri_device).map_err(|error| {
            format!(
                "failed to open vaapi device {}: {error}. Check your DRM render node and VAAPI drivers",
                dri_device.display()
            )
        })?;

        let capture_pixfmt = capture_format.av_pixfmt();
        if capture_pixfmt == Pixel::None {
            return Err(format!(
                "unsupported capture format {:?}",
                capture_format.fourcc
            ));
        }

        let mut frames_rgb = hw_device_ctx
            .create_frame_ctx(
                capture_pixfmt,
                capture_format.width,
                capture_format.height,
                Tiling::Drm(&capture_format.modifiers),
            )
            .map_err(|error| {
                format!(
                    "failed to create vaapi capture frame context ({capture_pixfmt:?} {}x{}): {error}",
                    capture_format.width, capture_format.height
                )
            })?;

        let (video_filter, filter_timebase) = video_filter(
            &mut frames_rgb,
            (capture_format.width, capture_format.height),
            roi_screen_coord,
            (roi_screen_coord.w, roi_screen_coord.h),
            transform,
            false,
        );

        let mut frames_yuv = hw_device_ctx
            .create_frame_ctx(
                Pixel::NV12,
                roi_screen_coord.w,
                roi_screen_coord.h,
                Tiling::Optimal,
            )
            .map_err(|error| {
                format!(
                    "failed to create vaapi encode frame context ({}x{}): {error}",
                    roi_screen_coord.w, roi_screen_coord.h
                )
            })?;

        let enc = make_video_params(
            enc_pixfmt,
            (roi_screen_coord.w, roi_screen_coord.h),
            Rational(refresh.0, refresh.1),
            global_header,
            &mut hw_device_ctx,
            &mut frames_yuv,
            bitrate_bytes_per_second,
            gop_size,
        )?;

        let low_power_opts = {
            let mut d = dict!();
            d.set("low_power", "1");
            d
        };

        let enc_video = match enc.open_with(low_power_opts.clone()) {
            Ok(enc) => enc,
            Err(error) => {
                warn!("failed to open vp8_vaapi in low_power mode ({error}), retrying without it");
                let enc = make_video_params(
                    enc_pixfmt,
                    (roi_screen_coord.w, roi_screen_coord.h),
                    Rational(refresh.0, refresh.1),
                    global_header,
                    &mut hw_device_ctx,
                    &mut frames_yuv,
                    bitrate_bytes_per_second,
                    gop_size,
                )?;
                enc.open_with(dict!())
                    .map_err(|error| format!("could not open vp8_vaapi encoder: {error}"))?
            }
        };

        let mut ost_video = octx
            .add_stream(encoder)
            .map_err(|error| format!("could not add video stream: {error}"))?;
        let vid_stream_idx = ost_video.index();
        ost_video.set_parameters(&enc_video);

        let mut audio = None;
        let mut audio_packets = None;
        if audio_source != RecordingAudio::None {
            let (packet_tx, packet_rx) = std::sync::mpsc::channel();
            match AudioHandle::spawn(audio_source, packet_tx) {
                Ok(handle) => {
                    // Wait for the negotiated PipeWire format, then create the
                    // Opus encoder and output stream before the header is
                    // written so the container describes the audio stream.
                    match handle.format_rx.recv_timeout(Duration::from_secs(10)) {
                        Ok(format) => match create_opus_encoder(&mut octx, format.channels) {
                            Ok(init) => {
                                if handle
                                    .send(AudioCommand::InitEncoder {
                                        enc: init.enc,
                                        ost_idx: init.ost_idx,
                                        ost_time_base: init.ost_time_base,
                                    })
                                    .is_ok()
                                {
                                    audio = Some(handle);
                                    audio_packets = Some(packet_rx);
                                }
                            }
                            Err(error) => return Err(format!("opus encoder unavailable: {error}")),
                        },
                        Err(error) => {
                            return Err(format!(
                                "timed out waiting for the pipewire audio format: {error}"
                            ));
                        }
                    }
                }
                Err(error) => {
                    return Err(format!("pipewire audio capture unavailable: {error}"));
                }
            }
        }

        octx.write_header()
            .map_err(|error| format!("could not write webm header: {error}"))?;

        let _ = framerate;
        Ok(Self {
            octx,
            video_filter,
            enc_video,
            enc_video_has_been_fed_any_frames: false,
            frames_rgb,
            frames_yuv,
            hw_device_ctx,
            filter_output_timebase: filter_timebase,
            vid_stream_idx,
            roi_screen_coord,
            transform,
            y_invert: false,
            capture_format,
            encode_size: (roi_screen_coord.w, roi_screen_coord.h),
            framerate,
            fps_limit: Some(FpsLimit::new(framerate.max(1) as f64)),
            audio,
            audio_packet_rx: audio_packets,
            bytes_written: 0,
            encoded_frames: 0,
            captured_frames: 0,
            dropped_frames: 0,
            audio_samples: 0,
            audio_packets: 0,
            io_error: None,
            pending_video_packet: None,
        })
    }

    /// Push a captured frame through the filter graph into the encoder.
    pub fn push(&mut self, surf: frame::Video) {
        self.video_filter
            .get("in")
            .unwrap()
            .source()
            .add(&surf)
            .unwrap();
        self.process_ready();
    }

    /// Enqueue a frame with frame-rate limiting (adds one frame of latency to
    /// make drop decisions).
    pub fn push_with_fpslimit(&mut self, surf: frame::Video) {
        self.captured_frames = self.captured_frames.saturating_add(1);
        let ts = Duration::from_nanos(surf.pts().unwrap_or(0) as u64);
        if let Some(limit) = &mut self.fps_limit {
            if let Some(to_enc) = limit.on_new_frame(surf, ts) {
                self.push(to_enc);
            }
        } else {
            self.push(surf);
        }
    }

    /// Drain ready frames and packets from the filter/encoder.
    pub fn process_ready(&mut self) {
        let mut yuv_frame = frame::Video::empty();
        while self
            .video_filter
            .get("out")
            .unwrap()
            .sink()
            .frame(&mut yuv_frame)
            .is_ok()
        {
            self.enc_video.send_frame(&yuv_frame).unwrap();
            self.enc_video_has_been_fed_any_frames = true;
        }

        let mut encoded = Packet::empty();
        while self.enc_video.receive_packet(&mut encoded).is_ok() {
            encoded.set_stream(self.vid_stream_idx);
            encoded.rescale_ts(
                self.filter_output_timebase,
                self.octx.stream(self.vid_stream_idx).unwrap().time_base(),
            );
            self.queue_video_packet(encoded);
            encoded = Packet::empty();
        }

        if let Some(packet_rx) = &self.audio_packet_rx {
            let packets: Vec<Packet> = packet_rx.try_iter().collect();
            for pack in packets {
                self.write_packet(pack);
            }
        }
    }

    fn write_packet(&mut self, packet: Packet) {
        self.bytes_written += packet.size() as u64;
        if packet.stream() != self.vid_stream_idx {
            self.audio_packets = self.audio_packets.saturating_add(1);
            self.audio_samples = self
                .audio_samples
                .saturating_add(packet.duration().max(0) as u64);
        }
        if let Err(error) = packet.write_interleaved(&mut self.octx) {
            let message = format!("failed to write interleaved packet: {error}");
            warn!("{message}");
            self.io_error = Some(message);
        }
    }

    fn queue_video_packet(&mut self, packet: Packet) {
        self.encoded_frames += 1;
        if let Some(mut previous) = self.pending_video_packet.take() {
            let duration = match (previous.pts(), packet.pts()) {
                (Some(previous_pts), Some(next_pts)) => next_pts.saturating_sub(previous_pts),
                _ => 0,
            };
            previous.set_duration(duration.max(1));
            self.write_packet(previous);
        }
        self.pending_video_packet = Some(packet);
    }

    fn flush_pending_video(&mut self, active_duration: Duration) {
        let Some(mut packet) = self.pending_video_packet.take() else {
            return;
        };
        let time_base = self.octx.stream(self.vid_stream_idx).unwrap().time_base();
        let denominator = (1_000_000_000_i128 * i128::from(time_base.0)).max(1);
        let end_pts = (active_duration.as_nanos() as i128 * i128::from(time_base.1) / denominator)
            .min(i128::from(i64::MAX)) as i64;
        let packet_pts = packet.pts().unwrap_or(0);
        packet.set_duration(end_pts.saturating_sub(packet_pts).max(1));
        self.write_packet(packet);
    }

    /// Signal the audio encoder to flush and drain remaining packets.
    fn flush_audio(&mut self) -> Result<(), String> {
        if let (Some(audio), Some(packet_rx)) = (&self.audio, &self.audio_packet_rx) {
            let (done, done_rx) = std::sync::mpsc::channel();
            audio.send(crate::recorder::audio::AudioCommand::Flush { done })?;
            done_rx
                .recv_timeout(Duration::from_secs(5))
                .map_err(|error| format!("audio encoder did not flush: {error}"))?;
            let packets: Vec<Packet> = packet_rx.try_iter().collect();
            for pack in packets {
                self.write_packet(pack);
            }
        }
        Ok(())
    }

    /// Flush remaining frames, close the muxer, and write the trailer.
    pub fn flush(&mut self, active_duration: Duration) -> Result<(), String> {
        if let Some(limit) = &mut self.fps_limit
            && let Some(f) = limit.flush()
        {
            self.push(f);
        }

        self.flush_audio()?;
        self.video_filter
            .get("in")
            .unwrap()
            .source()
            .flush()
            .map_err(|error| format!("failed to flush video filter: {error}"))?;
        self.process_ready();
        if self.enc_video_has_been_fed_any_frames {
            self.enc_video
                .send_eof()
                .map_err(|error| format!("failed to flush video encoder: {error}"))?;
            self.process_ready();
        }
        self.flush_pending_video(active_duration);
        self.octx
            .write_trailer()
            .map_err(|error| format!("failed to write webm trailer: {error}"))?;
        if let Some(error) = self.io_error.take() {
            return Err(error);
        }
        Ok(())
    }

    /// Allocate a VAAPI capture surface (one per in-flight DMA-BUF).
    pub fn alloc_capture_surface(&mut self) -> Result<frame::Video, ffmpeg::Error> {
        self.frames_rgb.alloc()
    }

    /// Begin feeding audio once the first video frame arrives.
    pub fn audio_start(&self) {
        if let Some(audio) = &self.audio {
            let _ = audio.send(AudioCommand::Start);
        }
    }

    /// Freeze audio while the recording is paused.
    pub fn audio_pause(&self) {
        if let Some(audio) = &self.audio {
            let _ = audio.send(AudioCommand::Pause);
        }
    }

    /// Resume audio after a pause.
    pub fn audio_resume(&self) {
        if let Some(audio) = &self.audio {
            let _ = audio.send(AudioCommand::Resume);
        }
    }

    pub fn take_audio_error(&self) -> Option<String> {
        self.audio.as_ref().and_then(AudioHandle::take_error)
    }

    /// Total bytes written to the container so far.
    pub fn bytes_written(&self) -> u64 {
        self.bytes_written
    }

    pub fn captured_frames(&self) -> u64 {
        self.captured_frames
    }

    pub fn dropped_frames(&self) -> u64 {
        self.dropped_frames + self.fps_limit.as_ref().map_or(0, FpsLimit::dropped_frames)
    }

    pub fn audio_samples(&self) -> u64 {
        self.audio_samples
    }

    pub fn audio_packets(&self) -> u64 {
        self.audio_packets
    }

    pub fn encoded_frames(&self) -> u64 {
        self.encoded_frames
    }

    pub fn transform(&self) -> Transform {
        self.transform
    }

    /// Rebuild only the capture-side VAAPI pool and filter graph while keeping
    /// the encoder, muxer, timestamps, and encoded canvas stable.
    pub fn reconfigure_capture(
        &mut self,
        capture_format: DmabufFormat,
        transform: Transform,
    ) -> Result<(), String> {
        let capture_pixfmt = capture_format.av_pixfmt();
        if capture_pixfmt == Pixel::None {
            return Err(format!(
                "unsupported capture format {:?}",
                capture_format.fourcc
            ));
        }
        let mut frames_rgb = self
            .hw_device_ctx
            .create_frame_ctx(
                capture_pixfmt,
                capture_format.width,
                capture_format.height,
                Tiling::Drm(&capture_format.modifiers),
            )
            .map_err(|error| {
                format!(
                    "failed to reconfigure vaapi capture surfaces ({}x{}): {error}",
                    capture_format.width, capture_format.height
                )
            })?;
        let source_size = transpose_if_transform_transposed(
            (capture_format.width, capture_format.height),
            transform,
        );
        let roi = Rect::new((0, 0), source_size);
        let (video_filter, time_base) = video_filter(
            &mut frames_rgb,
            (capture_format.width, capture_format.height),
            roi,
            self.encode_size,
            transform,
            self.y_invert,
        );

        if let Some(limit) = self.fps_limit.take() {
            self.dropped_frames = self.dropped_frames.saturating_add(limit.discarded_frames());
        }
        // Drop the old graph while its frame context is still alive, then
        // install the new context used by the replacement graph.
        self.video_filter = video_filter;
        self.frames_rgb = frames_rgb;
        self.filter_output_timebase = time_base;
        self.roi_screen_coord = roi;
        self.transform = transform;
        self.capture_format = capture_format;
        self.fps_limit = Some(FpsLimit::new(self.framerate.max(1) as f64));
        Ok(())
    }

    pub fn reconfigure_view(&mut self, transform: Transform, y_invert: bool) -> Result<(), String> {
        if self.y_invert == y_invert && self.transform == transform {
            return Ok(());
        }
        self.y_invert = y_invert;
        self.reconfigure_capture(self.capture_format.clone(), transform)
    }

    pub fn y_invert(&self) -> bool {
        self.y_invert
    }
}

/// Check that the WebM muxer is present (used by doctor).
fn webm_muxer_available() -> bool {
    use std::ffi::CString;
    unsafe {
        let short_name = CString::new("webm").unwrap();
        let file_name = CString::new("x.webm").unwrap();
        !ffmpeg::ffi::av_guess_format(short_name.as_ptr(), file_name.as_ptr(), std::ptr::null())
            .is_null()
    }
}

/// Ensure FFmpeg is initialized (idempotent).
pub(super) fn ensure_ffmpeg() -> Result<(), String> {
    ffmpeg::init().map_err(|error| format!("ffmpeg initialization failed: {error}"))
}

/// Validate that the full encode path is available: VAAPI device, vp8_vaapi
/// encoder, and the WebM muxer. Returns a human-readable error if not.
fn validate_encode_path(dri_device: &Path) -> Result<(), String> {
    ensure_ffmpeg()?;
    if !webm_muxer_available() {
        return Err("the WebM muxer is not available in this ffmpeg build".into());
    }
    if encoder::find_by_name("vp8_vaapi").is_none() {
        return Err("vp8_vaapi encoder is not available in this ffmpeg build".into());
    }
    let _ = AvHwDevCtx::new_libva(dri_device).map_err(|error| {
        format!(
            "failed to open vaapi device {}: {error}",
            dri_device.display()
        )
    })?;
    Ok(())
}

pub(super) fn validate_runtime() -> Result<(), String> {
    ensure_ffmpeg()?;
    if !webm_muxer_available() {
        return Err("the WebM muxer is unavailable".into());
    }
    if !hw_encoder_available("vp8_vaapi") {
        return Err("the VP8 VAAPI encoder is unavailable".into());
    }

    let render_nodes = std::fs::read_dir("/dev/dri")
        .map_err(|error| format!("could not inspect DRM render nodes: {error}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("renderD"))
        })
        .collect::<Vec<_>>();
    if render_nodes.is_empty() {
        return Err("no DRM render node is available".into());
    }
    if render_nodes
        .iter()
        .any(|path| validate_encode_path(path).is_ok())
    {
        Ok(())
    } else {
        Err("no DRM render node can initialize the VP8 VAAPI encoder".into())
    }
}

/// Re-open a finalized WebM and verify that its required streams and duration
/// are readable before it is promoted from the partial destination.
pub fn validate_recording(path: &Path, expect_audio: bool) -> Result<(), String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("could not inspect finalized recording: {error}"))?;
    if metadata.len() == 0 {
        return Err("finalized recording is empty".into());
    }

    let mut input = format::input(path)
        .map_err(|error| format!("could not reopen finalized recording: {error}"))?;
    let video_stream = input
        .streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or_else(|| "finalized recording has no video stream".to_string())?;
    let video_index = video_stream.index();
    let mut video_decoder = codec::context::Context::from_parameters(video_stream.parameters())
        .map_err(|error| format!("could not inspect finalized video codec: {error}"))?
        .decoder()
        .video()
        .map_err(|error| format!("could not open finalized video decoder: {error}"))?;

    let audio_stream = input.streams().best(ffmpeg::media::Type::Audio);
    if expect_audio && audio_stream.is_none() {
        return Err("finalized recording has no audio stream".into());
    }
    let audio_index = audio_stream.as_ref().map(ffmpeg::Stream::index);
    let mut audio_decoder = audio_stream
        .map(|stream| {
            codec::context::Context::from_parameters(stream.parameters())
                .map_err(|error| format!("could not inspect finalized audio codec: {error}"))?
                .decoder()
                .audio()
                .map_err(|error| format!("could not open finalized audio decoder: {error}"))
        })
        .transpose()?;
    if input.duration() <= 0 {
        return Err("finalized recording has no readable duration".into());
    }

    let mut decoded_video = false;
    let mut decoded_audio = !expect_audio;
    for (stream, packet) in input.packets() {
        if stream.index() == video_index && !decoded_video {
            video_decoder
                .send_packet(&packet)
                .map_err(|error| format!("finalized video packet is invalid: {error}"))?;
            let mut frame = frame::Video::empty();
            decoded_video = video_decoder.receive_frame(&mut frame).is_ok();
        } else if Some(stream.index()) == audio_index
            && !decoded_audio
            && let Some(decoder) = &mut audio_decoder
        {
            decoder
                .send_packet(&packet)
                .map_err(|error| format!("finalized audio packet is invalid: {error}"))?;
            let mut frame = frame::Audio::empty();
            decoded_audio = decoder.receive_frame(&mut frame).is_ok();
        }
        if decoded_video && decoded_audio {
            break;
        }
    }
    if !decoded_video {
        return Err("finalized recording has no decodable video frame".into());
    }
    if !decoded_audio {
        return Err("finalized recording has no decodable audio frame".into());
    }

    let mut seek_input = format::input(path)
        .map_err(|error| format!("could not reopen finalized recording for seeking: {error}"))?;
    let midpoint = seek_input.duration() / 2;
    seek_input
        .seek(midpoint, ..midpoint.saturating_add(1))
        .map_err(|error| format!("finalized recording is not seekable: {error}"))?;
    Ok(())
}
