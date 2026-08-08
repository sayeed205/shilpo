use std::convert::TryInto;
use std::mem;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::types::AudioSource;

pub struct AudioBuffer {
    pub pcm_data: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u32,
}

pub struct PipeWireAudioCapture {
    streaming: Arc<AtomicBool>,
    worker_handle: Option<std::thread::JoinHandle<()>>,
}

impl PipeWireAudioCapture {
    pub fn new() -> Self {
        Self {
            streaming: Arc::new(AtomicBool::new(false)),
            worker_handle: None,
        }
    }

    pub fn start_capture(
        &mut self,
        source: AudioSource,
    ) -> anyhow::Result<crossbeam_channel::Receiver<AudioBuffer>> {
        if source == AudioSource::None {
            let (_tx, rx) = crossbeam_channel::bounded(1);
            return Ok(rx);
        }

        let (tx, rx) = crossbeam_channel::bounded(32);
        let running = Arc::clone(&self.streaming);
        running.store(true, Ordering::SeqCst);
        let handle = std::thread::spawn(move || {
            if let Err(error) = run_pipewire_capture(source, running, tx) {
                tracing::error!(%error, "PipeWire capture failed");
            }
        });
        self.worker_handle = Some(handle);
        Ok(rx)
    }

    pub fn stop(&mut self) {
        self.streaming.store(false, Ordering::SeqCst);
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
    }
}

struct StreamData {
    sender: crossbeam_channel::Sender<AudioBuffer>,
    running: Arc<AtomicBool>,
    format: spa::param::audio::AudioInfoRaw,
    mainloop: pipewire::main_loop::MainLoopRc,
}

fn run_pipewire_capture(
    source: AudioSource,
    running: Arc<AtomicBool>,
    sender: crossbeam_channel::Sender<AudioBuffer>,
) -> anyhow::Result<()> {
    pipewire::init();
    let mainloop = pipewire::main_loop::MainLoopRc::new(None)?;
    let context = pipewire::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let props = pipewire::properties::properties! {
        *pipewire::keys::MEDIA_TYPE => "Audio",
        *pipewire::keys::MEDIA_CATEGORY => "Capture",
        *pipewire::keys::MEDIA_ROLE => "Screen",
        *pipewire::keys::STREAM_CAPTURE_SINK => if source == AudioSource::System { "true" } else { "false" },
    };
    let stream = pipewire::stream::StreamBox::new(&core, "shilpo-recording", props)?;
    let _listener = stream
        .add_local_listener_with_user_data(StreamData {
            sender,
            running: Arc::clone(&running),
            format: Default::default(),
            mainloop: mainloop.clone(),
        })
        .param_changed(|_, user_data, id, param| {
            if id == pipewire::spa::param::ParamType::Format.as_raw()
                && let Some(param) = param
            {
                let _ = user_data.format.parse(param);
            }
        })
        .process(|stream, user_data| {
            if !user_data.running.load(Ordering::SeqCst) {
                stream.disconnect().ok();
                user_data.mainloop.quit();
                return;
            }
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let Some(data) = buffer.datas_mut().first_mut() else {
                return;
            };
            let Some(bytes) = data.data() else { return };
            let samples = bytes
                .chunks_exact(mem::size_of::<f32>())
                .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
                .collect();
            let _ = user_data.sender.try_send(AudioBuffer {
                pcm_data: samples,
                sample_rate: user_data.format.rate(),
                channels: user_data.format.channels(),
            });
        })
        .register()?;
    let mut audio_info = spa::param::audio::AudioInfoRaw::new();
    audio_info.set_format(spa::param::audio::AudioFormat::F32LE);
    let object = pipewire::spa::pod::Object {
        type_: pipewire::spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: pipewire::spa::param::ParamType::EnumFormat.as_raw(),
        properties: audio_info.into(),
    };
    let values = pipewire::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pipewire::spa::pod::Value::Object(object),
    )?
    .0
    .into_inner();
    let pod = pipewire::spa::pod::Pod::from_bytes(&values)
        .ok_or_else(|| anyhow::anyhow!("invalid PipeWire format pod"))?;
    let mut params = [pod];
    stream.connect(
        pipewire::spa::utils::Direction::Input,
        None,
        pipewire::stream::StreamFlags::AUTOCONNECT
            | pipewire::stream::StreamFlags::MAP_BUFFERS
            | pipewire::stream::StreamFlags::RT_PROCESS,
        &mut params,
    )?;
    let timer_loop = mainloop.clone();
    let timer_running = Arc::clone(&running);
    let timer = mainloop.loop_().add_timer(move |_| {
        if !timer_running.load(Ordering::SeqCst) {
            timer_loop.quit();
        }
    });
    timer
        .update_timer(
            Some(std::time::Duration::from_millis(50)),
            Some(std::time::Duration::from_millis(50)),
        )
        .into_result()?;
    mainloop.run();
    Ok(())
}

impl Default for PipeWireAudioCapture {
    fn default() -> Self {
        Self::new()
    }
}
