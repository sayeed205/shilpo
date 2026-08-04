//! PipeWire desktop audio capture feeding an in-process FFmpeg Opus encoder.
//!
//! The audio thread owns the PipeWire connection and drives the stream's
//! event loop manually (via `pw_loop_iterate`) so it can also drain control
//! commands from the recording worker. Encoded Opus packets are forwarded to
//! the worker over an MPSC channel; the worker owns the muxer.
//!
//! The encoder/filter structure follows `wl-screenrec` (`src/audio.rs`),
//! licensed under the Apache License, Version 2.0. Copyright (c) wl-screenrec
//! contributors. This file is a derived work distributed under the Apache-2.0
//! license.

use std::{
    cell::RefCell,
    rc::Rc,
    sync::mpsc::{Receiver, Sender},
    time::Duration,
};

use ffmpeg::{
    ChannelLayout, Packet, Rational,
    codec::{Context, Id},
    encoder, filter,
    format::{self, Sample},
    frame,
    util::format::sample::Type,
};
use pipewire as pw;
use pw::properties::properties;
use spa::{
    param::{
        ParamType,
        audio::{AudioFormat as SpaAudioFormat, AudioInfoRaw},
        format::{MediaSubtype, MediaType},
        format_utils,
    },
    pod::Pod,
    utils::{Direction, SpaTypes},
};

use crate::recorder::fifo::AudioFifo;
use crate::types::RecordingAudio;

const F32_PACKED: Sample = Sample::F32(Type::Packed);
const OPUS_RATE: i32 = 48_000;
const COMMAND_POLL: Duration = Duration::from_millis(50);

/// The audio format PipeWire negotiated, forwarded to the worker so it can
/// create the Opus encoder and output stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioFormat {
    pub rate: u32,
    pub channels: u32,
}

/// Commands the recording worker sends to the audio thread.
pub enum AudioCommand {
    /// The worker created the Opus encoder and output stream; start encoding.
    InitEncoder {
        enc: encoder::Audio,
        ost_idx: usize,
        ost_time_base: Rational,
    },
    /// The first video frame arrived; begin feeding audio.
    Start,
    /// Freeze audio (video is paused).
    Pause,
    /// Resume feeding audio.
    Resume,
    /// Flush the encoder and signal completion.
    Flush { done: Sender<()> },
}

/// Ready-made Opus encoder plus the output stream it belongs to.
pub struct AudioEncoderInit {
    pub enc: encoder::Audio,
    pub ost_idx: usize,
    pub ost_time_base: Rational,
}

/// Handle used by the recording worker to read the negotiated format and send
/// commands to the audio thread.
pub struct AudioHandle {
    /// Receiver for the negotiated `AudioFormat`.
    pub format_rx: Receiver<AudioFormat>,
    command_tx: Sender<AudioCommand>,
    error_rx: Receiver<String>,
}

impl AudioHandle {
    /// Spawn the audio capture thread.
    ///
    pub fn spawn(source: RecordingAudio, packet_tx: Sender<Packet>) -> Result<Self, String> {
        let (format_tx, format_rx) = std::sync::mpsc::channel();
        let (command_tx, command_rx) = std::sync::mpsc::channel();
        let (error_tx, error_rx) = std::sync::mpsc::channel();

        std::thread::Builder::new()
            .name("shilpo-audio".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_audio_thread(source, packet_tx, format_tx, command_rx)
                }));
                let error = match result {
                    Ok(Ok(())) => None,
                    Ok(Err(error)) => Some(error),
                    Err(_) => Some("audio capture thread panicked".into()),
                };
                if let Some(error) = error {
                    let _ = error_tx.send(error);
                }
            })
            .map_err(|error| format!("could not spawn audio thread: {error}"))?;

        Ok(AudioHandle {
            format_rx,
            command_tx,
            error_rx,
        })
    }

    pub fn send(&self, command: AudioCommand) -> Result<(), String> {
        self.command_tx
            .send(command)
            .map_err(|_| "audio thread exited".into())
    }

    pub fn take_error(&self) -> Option<String> {
        self.error_rx.try_recv().ok()
    }
}

impl Drop for AudioHandle {
    fn drop(&mut self) {
        let (done, done_rx) = std::sync::mpsc::channel();
        if self.command_tx.send(AudioCommand::Flush { done }).is_ok() {
            let _ = done_rx.recv_timeout(Duration::from_secs(1));
        }
    }
}

/// Create the Opus encoder for the muxer and add the audio output stream.
pub fn create_opus_encoder(
    octx: &mut format::context::Output,
    channels: u32,
) -> Result<AudioEncoderInit, String> {
    let codec = encoder::find(Id::OPUS)
        .ok_or_else(|| "opus encoder not available in this ffmpeg build".to_string())?
        .audio()
        .map_err(|error| format!("opus codec is not an audio codec: {error}"))?;

    let mut ost_audio = octx
        .add_stream(codec)
        .map_err(|error| format!("could not add audio stream: {error}"))?;
    let ost_idx = ost_audio.index();

    let layout = if channels >= 2 {
        ChannelLayout::STEREO
    } else {
        ChannelLayout::MONO
    };

    let mut enc_audio = Context::new()
        .encoder()
        .audio()
        .map_err(|error| format!("could not create opus encoder: {error}"))?;
    enc_audio.set_rate(OPUS_RATE);
    enc_audio.set_channel_layout(layout);
    enc_audio.set_format(F32_PACKED);
    enc_audio.set_time_base(Rational(1, OPUS_RATE));
    enc_audio.set_bit_rate(128_000);
    let enc_audio = enc_audio
        .open_as(codec)
        .map_err(|error| format!("could not open opus encoder: {error}"))?;

    ost_audio.set_parameters(&enc_audio);

    let ost_time_base = octx
        .stream(ost_idx)
        .map(|stream| stream.time_base())
        .unwrap_or(Rational(1, OPUS_RATE));

    Ok(AudioEncoderInit {
        enc: enc_audio,
        ost_idx,
        ost_time_base,
    })
}

struct Inner {
    format: Option<(u32, u32)>,
    enc: Option<encoder::Audio>,
    filter: Option<filter::Graph>,
    fifo: Option<AudioFifo>,
    /// PTS for frames pushed into the filter graph.
    input_pts: [i64; 2],
    /// PTS for frames pushed into the encoder.
    enc_pts: i64,
    started: bool,
    paused: bool,
    pts_at_pause: Option<[i64; 2]>,
    enc_pts_at_pause: Option<i64>,
    ost_idx: usize,
    ost_time_base: Rational,
    packet_tx: Sender<Packet>,
    source: RecordingAudio,
    stream_started: [bool; 2],
    runtime_error: Option<String>,
}

#[derive(Clone, Copy)]
enum AudioInput {
    Desktop,
    Microphone,
}

impl AudioInput {
    fn index(self) -> usize {
        match self {
            Self::Desktop => 0,
            Self::Microphone => 1,
        }
    }

    fn filter_name(self, mixed: bool) -> &'static str {
        if !mixed {
            "in"
        } else {
            match self {
                Self::Desktop => "desktop",
                Self::Microphone => "microphone",
            }
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Desktop => "desktop audio",
            Self::Microphone => "microphone",
        }
    }
}

struct UserData(Rc<RefCell<Inner>>, AudioInput);

fn run_audio_thread(
    source: RecordingAudio,
    packet_tx: Sender<Packet>,
    format_tx: Sender<AudioFormat>,
    command_rx: Receiver<AudioCommand>,
) -> Result<(), String> {
    let inner = Rc::new(RefCell::new(Inner {
        format: None,
        enc: None,
        filter: None,
        fifo: None,
        input_pts: [0, 0],
        enc_pts: 0,
        started: false,
        paused: false,
        pts_at_pause: None,
        enc_pts_at_pause: None,
        ost_idx: 0,
        ost_time_base: Rational(1, OPUS_RATE),
        packet_tx,
        source,
        stream_started: [false, false],
        runtime_error: None,
    }));

    pw::init();

    let mainloop = match pw::main_loop::MainLoopRc::new(None) {
        Ok(mainloop) => mainloop,
        Err(error) => {
            return Err(format!("could not create pipewire main loop: {error}"));
        }
    };
    let context = match pw::context::ContextRc::new(&mainloop, None) {
        Ok(context) => context,
        Err(error) => {
            return Err(format!("could not create pipewire context: {error}"));
        }
    };
    let core = match context.connect_rc(None) {
        Ok(core) => core,
        Err(error) => {
            return Err(format!("could not connect to pipewire: {error}"));
        }
    };

    let mut streams = Vec::new();
    let inputs: &[AudioInput] = match source {
        RecordingAudio::Desktop => &[AudioInput::Desktop],
        RecordingAudio::Microphone => &[AudioInput::Microphone],
        RecordingAudio::DesktopAndMicrophone => &[AudioInput::Desktop, AudioInput::Microphone],
        RecordingAudio::None => return Ok(()),
        RecordingAudio::Configured => {
            return Err("configured audio must be resolved before capture starts".into());
        }
    };
    for input in inputs {
        let capture_monitor = matches!(input, AudioInput::Desktop);
        match create_stream(capture_monitor, *input, &core, &inner, format_tx.clone()) {
            Ok(created) => streams.push(created),
            Err(error) => {
                return Err(format!("pipewire audio capture unavailable: {error}"));
            }
        }
    }

    // Drive the loop manually so commands from the worker are handled even
    // between PipeWire events.
    let mut flushed = false;
    while !flushed {
        let _ = mainloop
            .loop_()
            .iterate(pw::loop_::Timeout::Finite(COMMAND_POLL));

        if let Some(error) = inner.borrow_mut().runtime_error.take() {
            return Err(error);
        }

        while let Ok(command) = command_rx.try_recv() {
            let is_flush = matches!(&command, AudioCommand::Flush { .. });
            apply_command(&inner, command);
            if is_flush {
                flushed = true;
                break;
            }
        }
    }

    for (stream, _) in streams {
        let _ = stream.disconnect();
    }
    Ok(())
}

fn apply_command(inner: &Rc<RefCell<Inner>>, command: AudioCommand) {
    let mut inner = inner.borrow_mut();
    match command {
        AudioCommand::InitEncoder {
            enc,
            ost_idx,
            ost_time_base,
        } => {
            let frame_size = enc.frame_size();
            inner.enc = Some(enc);
            inner.ost_idx = ost_idx;
            inner.ost_time_base = ost_time_base;
            let (rate, channels) = inner.format.unwrap_or((OPUS_RATE as u32, 2));
            inner.filter = Some(build_filter(
                rate,
                channels,
                inner.source == RecordingAudio::DesktopAndMicrophone,
            ));
            inner.fifo = Some(AudioFifo::new(F32_PACKED, 2, frame_size.max(1024) * 2).unwrap());
        }
        AudioCommand::Start => inner.started = true,
        AudioCommand::Pause => {
            if !inner.paused {
                inner.pts_at_pause = Some(inner.input_pts);
                inner.enc_pts_at_pause = Some(inner.enc_pts);
                inner.paused = true;
            }
        }
        AudioCommand::Resume => {
            if inner.paused {
                if let Some(pts) = inner.pts_at_pause.take() {
                    inner.input_pts = pts;
                }
                if let Some(pts) = inner.enc_pts_at_pause.take() {
                    inner.enc_pts = pts;
                }
                inner.paused = false;
            }
        }
        AudioCommand::Flush { done } => {
            flush(&mut inner);
            let _ = done.send(());
        }
    }
}

fn create_stream<'a>(
    capture_monitor: bool,
    input: AudioInput,
    core: &'a pw::core::CoreRc,
    inner: &'a Rc<RefCell<Inner>>,
    format_tx: Sender<AudioFormat>,
) -> Result<
    (
        pw::stream::StreamBox<'a>,
        pw::stream::StreamListener<UserData>,
    ),
    String,
> {
    let mut props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Communication",
        *pw::keys::APP_NAME => "Shilpo",
        *pw::keys::NODE_NAME => "shilpo-audio-capture",
    };
    if capture_monitor {
        props.insert(*pw::keys::STREAM_CAPTURE_SINK, "true");
    }

    let stream = pw::stream::StreamBox::new(core, "shilpo-audio-capture", props)
        .map_err(|error| format!("could not create pipewire audio stream: {error}"))?;

    let user_data = UserData(inner.clone(), input);
    let _listener = stream
        .add_local_listener_with_user_data(user_data)
        .state_changed(|_, user_data, _, new| {
            let mut inner = user_data.0.borrow_mut();
            let input = user_data.1;
            let index = input.index();
            match new {
                pw::stream::StreamState::Streaming => inner.stream_started[index] = true,
                pw::stream::StreamState::Error(error) => {
                    inner.runtime_error =
                        Some(format!("{} pipewire stream failed: {error}", input.name()));
                }
                pw::stream::StreamState::Unconnected if inner.stream_started[index] => {
                    inner.runtime_error =
                        Some(format!("{} pipewire stream disconnected", input.name()));
                }
                _ => {}
            }
        })
        .param_changed(move |_, user_data, id, param| {
            let mut inner = user_data.0.borrow_mut();
            if id != ParamType::Format.as_raw() {
                return;
            }
            let Some(param) = param else {
                return;
            };
            let Ok((media_type, media_subtype)) = format_utils::parse_format(param) else {
                return;
            };
            if media_type != MediaType::Audio || media_subtype != MediaSubtype::Raw {
                return;
            }
            let mut info = AudioInfoRaw::new();
            if info.parse(param).is_err() {
                return;
            }
            let rate = info.rate();
            let channels = info.channels().max(1);
            inner.format = Some((rate, channels));
            let _ = format_tx.send(AudioFormat { rate, channels });
        })
        .process(|stream, user_data| {
            process_cb(stream, &user_data.0, user_data.1);
        })
        .register()
        .map_err(|error| format!("could not register pipewire audio listener: {error}"))?;

    let mut audio_info = AudioInfoRaw::new();
    audio_info.set_format(SpaAudioFormat::F32LE);
    audio_info.set_rate(OPUS_RATE as u32);
    audio_info.set_channels(2);

    let obj = pw::spa::pod::Object {
        type_: SpaTypes::ObjectParamFormat.as_raw(),
        id: ParamType::EnumFormat.as_raw(),
        properties: audio_info.into(),
    };
    let values = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(obj),
    )
    .map_err(|error| format!("could not serialize pipewire format: {error}"))?
    .0
    .into_inner();

    let mut params = [
        Pod::from_bytes(&values).ok_or_else(|| "could not parse pipewire format".to_string())?
    ];

    stream
        .connect(
            Direction::Input,
            None,
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
            &mut params,
        )
        .map_err(|error| format!("could not connect pipewire audio stream: {error}"))?;

    Ok((stream, _listener))
}

fn process_cb(stream: &pw::stream::Stream, inner: &Rc<RefCell<Inner>>, input: AudioInput) {
    let mut inner = inner.borrow_mut();
    let Some(mut buffer) = stream.dequeue_buffer() else {
        return;
    };
    if !inner.started || inner.paused || inner.enc.is_none() || inner.filter.is_none() {
        return;
    }
    let Some((rate, channels)) = inner.format else {
        return;
    };
    let datas = buffer.datas_mut();
    if datas.is_empty() {
        return;
    }
    let data = &mut datas[0];
    let used_bytes = data.chunk().size() as usize;
    let Some(samples) = data.data() else {
        return;
    };
    let bytes_per_sample = 4; // F32LE
    let sample_count = used_bytes.min(samples.len()) / bytes_per_sample;
    if sample_count == 0 {
        return;
    }
    let frames = sample_count / channels.max(1) as usize;
    if frames == 0 {
        return;
    }

    let layout = if channels >= 2 {
        ChannelLayout::STEREO
    } else {
        ChannelLayout::MONO
    };

    let mut frame = frame::Audio::new(F32_PACKED, frames, layout);
    frame.set_rate(rate);
    let input_index = input.index();
    frame.set_pts(Some(inner.input_pts[input_index]));
    let nbytes = frames * channels as usize * bytes_per_sample;
    frame.data_mut(0)[..nbytes].copy_from_slice(&samples[..nbytes]);
    inner.input_pts[input_index] += frames as i64;

    let mixed = inner.source == RecordingAudio::DesktopAndMicrophone;
    inner
        .filter
        .as_mut()
        .unwrap()
        .get(input.filter_name(mixed))
        .unwrap()
        .source()
        .add(&frame)
        .unwrap();

    pop_filter(&mut inner);
}

fn pop_filter(inner: &mut Inner) {
    let mut filtered = frame::Audio::empty();
    let enc_frame_size = inner.enc.as_ref().unwrap().frame_size() as usize;
    let enc_format = inner.enc.as_ref().unwrap().format();
    let enc_layout = inner.enc.as_ref().unwrap().channel_layout();
    loop {
        let has_frame = inner
            .filter
            .as_mut()
            .unwrap()
            .get("out")
            .unwrap()
            .sink()
            .frame(&mut filtered)
            .is_ok();
        if !has_frame {
            break;
        }
        inner.fifo.as_mut().unwrap().push(&filtered);
        loop {
            let fifo_size = inner.fifo.as_ref().unwrap().size();
            if fifo_size < enc_frame_size {
                break;
            }
            let mut frame_into_encoder = frame::Audio::new(enc_format, enc_frame_size, enc_layout);
            inner.fifo.as_mut().unwrap().pop(&mut frame_into_encoder);
            frame_into_encoder.set_rate(inner.enc.as_ref().unwrap().rate());
            frame_into_encoder.set_pts(Some(inner.enc_pts));
            inner.enc_pts += frame_into_encoder.samples() as i64;
            inner
                .enc
                .as_mut()
                .unwrap()
                .send_frame(&frame_into_encoder)
                .unwrap();
            pop_packets(inner);
        }
    }
}

fn pop_packets(inner: &mut Inner) {
    let mut pack = Packet::empty();
    while inner
        .enc
        .as_mut()
        .unwrap()
        .receive_packet(&mut pack)
        .is_ok()
    {
        pack.set_stream(inner.ost_idx);
        pack.rescale_ts(Rational(1, OPUS_RATE), inner.ost_time_base);
        inner
            .packet_tx
            .send(pack)
            .expect("worker exited before audio flush");
        pack = Packet::empty();
    }
}

fn flush(inner: &mut Inner) {
    if inner.enc.is_none() {
        return;
    }
    if let Some(ref mut filter) = inner.filter {
        if inner.source == RecordingAudio::DesktopAndMicrophone {
            filter.get("desktop").unwrap().source().flush().unwrap();
            filter.get("microphone").unwrap().source().flush().unwrap();
        } else {
            filter.get("in").unwrap().source().flush().unwrap();
        }
    }
    pop_filter(inner);
    if inner.fifo.as_ref().is_some_and(|fifo| fifo.size() > 0) {
        let (frame_size, format, layout, rate) = {
            let encoder = inner.enc.as_ref().unwrap();
            (
                encoder.frame_size() as usize,
                encoder.format(),
                encoder.channel_layout(),
                encoder.rate(),
            )
        };
        let mut final_frame = frame::Audio::new(format, frame_size, layout);
        final_frame.data_mut(0).fill(0);
        inner.fifo.as_mut().unwrap().pop(&mut final_frame);
        final_frame.set_rate(rate);
        final_frame.set_pts(Some(inner.enc_pts));
        inner.enc_pts += final_frame.samples() as i64;
        inner
            .enc
            .as_mut()
            .unwrap()
            .send_frame(&final_frame)
            .unwrap();
        pop_packets(inner);
    }
    inner.enc.as_mut().unwrap().send_eof().unwrap();
    pop_packets(inner);
}

fn build_filter(rate: u32, channels: u32, mixed: bool) -> filter::Graph {
    let mut g = ffmpeg::filter::graph::Graph::new();

    let layout = if channels >= 2 { "stereo" } else { "mono" };
    let input_args = format!("sample_rate={rate}:sample_fmt=flt:channel_layout={layout}");
    if mixed {
        g.add(
            &ffmpeg::filter::find("abuffer").unwrap(),
            "desktop",
            &input_args,
        )
        .unwrap();
        g.add(
            &ffmpeg::filter::find("abuffer").unwrap(),
            "microphone",
            &input_args,
        )
        .unwrap();
    } else {
        g.add(&ffmpeg::filter::find("abuffer").unwrap(), "in", &input_args)
            .unwrap();
    }

    g.add(&ffmpeg::filter::find("abuffersink").unwrap(), "out", "")
        .unwrap();

    let output = if mixed {
        g.output("desktop", 0)
            .unwrap()
            .output("microphone", 0)
            .unwrap()
    } else {
        g.output("in", 0).unwrap()
    };
    let filter = if mixed {
        format!(
            "[desktop][microphone]amix=inputs=2:duration=longest:normalize=0,aresample={OPUS_RATE},aformat=sample_fmts=flt:channel_layouts=stereo[out]"
        )
    } else {
        format!("aresample={OPUS_RATE},aformat=sample_fmts=flt:channel_layouts=stereo")
    };
    output.input("out", 0).unwrap().parse(&filter).unwrap();

    g.validate().unwrap();
    g
}

#[cfg(test)]
mod tests {
    use super::build_filter;

    #[test]
    fn mixed_audio_filter_accepts_two_inputs() {
        ffmpeg::init().unwrap();
        let mut graph = build_filter(48_000, 2, true);
        assert!(graph.get("desktop").is_some());
        assert!(graph.get("microphone").is_some());
        assert!(graph.get("out").is_some());
    }
}
