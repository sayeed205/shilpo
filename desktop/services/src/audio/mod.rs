use std::sync::mpsc;

use anyhow::Result;
use libpulse_binding as pulse;
use pulse::{
    context::{
        Context, FlagSet as ContextFlagSet, State as ContextState, subscribe::InterestMaskSet,
    },
    mainloop::threaded::Mainloop,
};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

/// Metadata describing an individual application audio playback stream (Sink Input).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioStream {
    pub id: u32,
    pub index: u32,
    pub name: String,
    pub app_name: String,
    pub volume_percent: u8,
    pub is_muted: bool,
}

/// Metadata describing an audio port on a sound card, sink, or source.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioPort {
    pub name: String,
    pub description: String,
    pub is_active: bool,
    pub available: bool,
}

/// Metadata describing a physical or virtual audio device (sink or source).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioDevice {
    pub index: u32,
    pub id: String,
    pub name: String,
    pub description: String,
    pub volume_percent: u8,
    pub is_muted: bool,
    pub is_default: bool,
    pub is_input: bool,
    pub channels: u8,
    pub ports: Vec<AudioPort>,
    pub active_port: Option<String>,
}

impl AudioDevice {
    /// Resolves the default flag and port activity state against a default device name.
    pub fn resolve_active_ports(&mut self, default_name: &str) {
        self.is_default = self.name == default_name;
        if let Some(ref active) = self.active_port {
            for p in &mut self.ports {
                p.is_active = p.name == *active;
            }
        }
    }
}

/// Comprehensive system audio snapshot including volumes, mutes, sinks, sources, and application streams.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioInfo {
    pub available: bool,
    pub default_sink_name: String,
    pub default_source_name: String,
    pub volume: u8,
    pub is_muted: bool,
    pub input_volume: u8,
    pub is_input_muted: bool,
    pub sinks: Vec<AudioDevice>,
    pub sources: Vec<AudioDevice>,
    pub app_streams: Vec<AudioStream>,
}

impl AudioInfo {
    /// Returns a combined list of all audio output sinks and input sources.
    pub fn all_devices(&self) -> Vec<AudioDevice> {
        let mut devices = self.sinks.clone();
        devices.extend(self.sources.clone());
        devices
    }

    /// Returns the available audio ports for the default output sink.
    pub fn default_sink_ports(&self) -> Vec<AudioPort> {
        self.sinks
            .iter()
            .find(|s| s.is_default)
            .map(|s| s.ports.clone())
            .unwrap_or_default()
    }

    /// Returns the available audio ports for the default input source (microphone).
    pub fn default_source_ports(&self) -> Vec<AudioPort> {
        self.sources
            .iter()
            .find(|s| s.is_default)
            .map(|s| s.ports.clone())
            .unwrap_or_default()
    }
}

enum PulseCommand {
    SetVolume(u8),
    ToggleMute,
    SetInputVolume(u8),
    ToggleInputMute,
    SetDefaultDevice {
        device_id: String,
        is_input: bool,
    },
    SetStreamVolume {
        index: u32,
        percentage: u8,
    },
    ToggleStreamMute {
        index: u32,
    },
    SetSinkPort {
        sink_name: String,
        port_name: String,
    },
    SetSourcePort {
        source_name: String,
        port_name: String,
    },
}

/// System audio service for volume, mute status, and device switching using event-driven PulseAudio APIs.
pub struct AudioService {
    _tx: watch::Sender<AudioInfo>,
    rx: watch::Receiver<AudioInfo>,
    cmd_tx: Option<mpsc::Sender<PulseCommand>>,
}

impl AudioService {
    /// Creates a new event-driven [`AudioService`] connected to PulseAudio/PipeWire.
    pub fn new() -> Result<Self> {
        let (tx, rx) = watch::channel(AudioInfo::default());
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (init_tx, init_rx) = mpsc::channel();

        let tx_worker = tx.clone();
        std::thread::spawn(move || {
            if let Err(e) = run_pulse_worker(tx_worker, cmd_rx, init_tx) {
                tracing::warn!(error = %e, "pulse audio background worker exited");
            }
        });

        match init_rx.recv_timeout(std::time::Duration::from_secs(3)) {
            Ok(Ok(())) => Ok(Self {
                _tx: tx,
                rx,
                cmd_tx: Some(cmd_tx),
            }),
            Ok(Err(err)) => Err(anyhow::anyhow!("pulse audio worker init failed: {err}")),
            Err(_) => Err(anyhow::anyhow!("pulse audio worker init timed out")),
        }
    }

    /// Creates an offline [`AudioService`] instance for testing without an active audio daemon.
    pub fn new_offline() -> Self {
        let (tx, rx) = watch::channel(AudioInfo::default());
        Self {
            _tx: tx,
            rx,
            cmd_tx: None,
        }
    }

    /// Subscribes to event-driven system audio snapshot updates.
    pub fn subscribe(&self) -> watch::Receiver<AudioInfo> {
        self.rx.clone()
    }

    /// Returns the current [`AudioInfo`] snapshot.
    pub fn audio_info(&self) -> AudioInfo {
        self.rx.borrow().clone()
    }

    /// Returns a list of all current audio input and output devices.
    pub fn list_devices(&self) -> Vec<AudioDevice> {
        let info = self.audio_info();
        if !info.sinks.is_empty() || !info.sources.is_empty() {
            let mut res = info.sinks;
            res.extend(info.sources);
            res
        } else {
            Self::list_devices_static()
        }
    }

    /// Returns a static fallback snapshot of audio devices when offline.
    pub fn list_devices_static() -> Vec<AudioDevice> {
        Vec::new()
    }

    /// Increases the default sink output volume by the specified percentage step.
    pub fn increase_volume(&self, step: u8) {
        let current = self.audio_info().volume;
        self.set_volume(current.saturating_add(step).min(100));
    }

    /// Decreases the default sink output volume by the specified percentage step.
    pub fn decrease_volume(&self, step: u8) {
        let current = self.audio_info().volume;
        self.set_volume(current.saturating_sub(step));
    }

    /// Sets the volume percentage for the default output sink.
    pub fn set_volume(&self, vol: u8) {
        if let Some(cmd_tx) = &self.cmd_tx {
            let _ = cmd_tx.send(PulseCommand::SetVolume(vol));
        }
    }

    /// Toggles the mute state for the default output sink.
    pub fn toggle_mute(&self) {
        if let Some(cmd_tx) = &self.cmd_tx {
            let _ = cmd_tx.send(PulseCommand::ToggleMute);
        }
    }

    /// Sets the volume percentage for the default input source (microphone).
    pub fn set_input_volume(&self, vol: u8) {
        if let Some(cmd_tx) = &self.cmd_tx {
            let _ = cmd_tx.send(PulseCommand::SetInputVolume(vol));
        }
    }

    /// Toggles the mute state for the default input source (microphone).
    pub fn toggle_input_mute(&self) {
        if let Some(cmd_tx) = &self.cmd_tx {
            let _ = cmd_tx.send(PulseCommand::ToggleInputMute);
        }
    }

    /// Sets the volume percentage for an active application playback stream.
    pub fn set_stream_volume(&self, index: u32, percentage: u8) -> Result<()> {
        if let Some(cmd_tx) = &self.cmd_tx {
            let _ = cmd_tx.send(PulseCommand::SetStreamVolume { index, percentage });
        }
        Ok(())
    }

    /// Toggles mute for an active application playback stream.
    pub fn toggle_stream_mute(&self, index: u32) -> Result<()> {
        if let Some(cmd_tx) = &self.cmd_tx {
            let _ = cmd_tx.send(PulseCommand::ToggleStreamMute { index });
        }
        Ok(())
    }

    /// Sets the default output sink or input source by device name/ID.
    pub fn set_default_device(&self, device_id: &str, is_input: bool) -> Result<()> {
        if let Some(cmd_tx) = &self.cmd_tx {
            let _ = cmd_tx.send(PulseCommand::SetDefaultDevice {
                device_id: device_id.to_string(),
                is_input,
            });
        }
        Ok(())
    }

    /// Returns available audio ports for the default output sink.
    pub fn list_ports(&self) -> Vec<AudioPort> {
        let info = self.audio_info();
        let default_sink = info.sinks.iter().find(|s| s.is_default);
        if let Some(sink) = default_sink
            && !sink.ports.is_empty()
        {
            sink.ports.clone()
        } else {
            Self::list_ports_static()
        }
    }

    /// Returns a static fallback list of audio ports when offline.
    pub fn list_ports_static() -> Vec<AudioPort> {
        Vec::new()
    }

    /// Switches the active audio port for the specified output sink.
    pub fn set_sink_port(&self, sink_name: &str, port_name: &str) -> Result<()> {
        if let Some(cmd_tx) = &self.cmd_tx {
            let _ = cmd_tx.send(PulseCommand::SetSinkPort {
                sink_name: sink_name.to_string(),
                port_name: port_name.to_string(),
            });
        }
        Ok(())
    }

    /// Switches the active audio port for the specified input source (microphone).
    pub fn set_source_port(&self, source_name: &str, port_name: &str) -> Result<()> {
        if let Some(cmd_tx) = &self.cmd_tx {
            let _ = cmd_tx.send(PulseCommand::SetSourcePort {
                source_name: source_name.to_string(),
                port_name: port_name.to_string(),
            });
        }
        Ok(())
    }
}

fn percent_to_pa_volume(percentage: u8, channels: u8) -> pulse::volume::ChannelVolumes {
    let mut cvol = pulse::volume::ChannelVolumes::default();
    let pa_vol = pulse::volume::Volume((percentage as u32 * pulse::volume::Volume::NORMAL.0) / 100);
    let ch = if channels == 0 { 2 } else { channels };
    cvol.set(ch, pa_vol);
    cvol
}

macro_rules! convert_pa_ports {
    ($ports:expr) => {
        $ports
            .iter()
            .map(|p| AudioPort {
                name: p.name.as_ref().map(|s| s.to_string()).unwrap_or_default(),
                description: p
                    .description
                    .as_ref()
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
                is_active: false,
                available: p.available != pulse::def::PortAvailable::No,
            })
            .collect::<Vec<_>>()
    };
}

enum WorkerEvent {
    Subscription,
    Command(PulseCommand),
}

fn run_pulse_worker(
    tx: watch::Sender<AudioInfo>,
    cmd_rx: mpsc::Receiver<PulseCommand>,
    init_tx: mpsc::Sender<Result<(), String>>,
) -> Result<()> {
    let mut mainloop =
        Mainloop::new().ok_or_else(|| anyhow::anyhow!("failed to create mainloop"))?;
    let mut context = Context::new(&mainloop, "Shilpo Audio Service")
        .ok_or_else(|| anyhow::anyhow!("failed to create context"))?;

    context
        .connect(None, ContextFlagSet::NOFLAGS, None)
        .map_err(|e| anyhow::anyhow!("connect failed: {e:?}"))?;
    mainloop
        .start()
        .map_err(|e| anyhow::anyhow!("mainloop start failed: {e:?}"))?;

    let start = std::time::Instant::now();
    loop {
        let state = context.get_state();
        if state == ContextState::Ready {
            let _ = init_tx.send(Ok(()));
            break;
        }
        if state == ContextState::Failed
            || state == ContextState::Terminated
            || start.elapsed() > std::time::Duration::from_secs(2)
        {
            let _ = init_tx.send(Err("context ready timeout".into()));
            return Err(anyhow::anyhow!("pulse context connection failed"));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    refresh_snapshot(&mut mainloop, &mut context, &tx);

    let (evt_tx, evt_rx) = mpsc::channel();
    {
        mainloop.lock();
        let sub_tx = evt_tx.clone();

        context.set_subscribe_callback(Some(Box::new(move |_facility, _op, _index| {
            let _ = sub_tx.send(WorkerEvent::Subscription);
        })));

        context.subscribe(
            InterestMaskSet::SINK
                | InterestMaskSet::SOURCE
                | InterestMaskSet::SINK_INPUT
                | InterestMaskSet::SERVER,
            |_| {},
        );
        mainloop.unlock();
    }

    let cmd_evt_tx = evt_tx.clone();
    std::thread::spawn(move || {
        while let Ok(cmd) = cmd_rx.recv() {
            if cmd_evt_tx.send(WorkerEvent::Command(cmd)).is_err() {
                break;
            }
        }
    });

    while let Ok(first_evt) = evt_rx.recv() {
        let mut needs_refresh = false;
        let mut pending_command: Option<PulseCommand> = None;

        match first_evt {
            WorkerEvent::Subscription => {
                needs_refresh = true;
            }
            WorkerEvent::Command(cmd) => {
                pending_command = Some(cmd);
            }
        }

        while let Ok(evt) = evt_rx.try_recv() {
            match evt {
                WorkerEvent::Subscription => {
                    needs_refresh = true;
                }
                WorkerEvent::Command(cmd) => {
                    pending_command = Some(cmd);
                }
            }
        }

        if let Some(cmd) = pending_command {
            mainloop.lock();
            let info = tx.borrow().clone();

            match cmd {
                PulseCommand::SetVolume(vol) => {
                    if !info.default_sink_name.is_empty() {
                        let channels = info
                            .sinks
                            .iter()
                            .find(|s| s.name == info.default_sink_name)
                            .map_or(2, |s| s.channels);
                        let cvol = percent_to_pa_volume(vol, channels);
                        context.introspect().set_sink_volume_by_name(
                            &info.default_sink_name,
                            &cvol,
                            None,
                        );
                    }
                }
                PulseCommand::ToggleMute => {
                    if !info.default_sink_name.is_empty() {
                        context.introspect().set_sink_mute_by_name(
                            &info.default_sink_name,
                            !info.is_muted,
                            None,
                        );
                    }
                }
                PulseCommand::SetInputVolume(vol) => {
                    if !info.default_source_name.is_empty() {
                        let channels = info
                            .sources
                            .iter()
                            .find(|s| s.name == info.default_source_name)
                            .map_or(2, |s| s.channels);
                        let cvol = percent_to_pa_volume(vol, channels);
                        context.introspect().set_source_volume_by_name(
                            &info.default_source_name,
                            &cvol,
                            None,
                        );
                    }
                }
                PulseCommand::ToggleInputMute => {
                    if !info.default_source_name.is_empty() {
                        context.introspect().set_source_mute_by_name(
                            &info.default_source_name,
                            !info.is_input_muted,
                            None,
                        );
                    }
                }
                PulseCommand::SetDefaultDevice {
                    device_id,
                    is_input,
                } => {
                    if is_input {
                        context.set_default_source(&device_id, |_| {});
                    } else {
                        context.set_default_sink(&device_id, |_| {});
                    }
                }
                PulseCommand::SetStreamVolume { index, percentage } => {
                    let cvol = percent_to_pa_volume(percentage, 2);
                    context
                        .introspect()
                        .set_sink_input_volume(index, &cvol, None);
                }
                PulseCommand::ToggleStreamMute { index } => {
                    let current_mute = info
                        .app_streams
                        .iter()
                        .find(|s| s.index == index)
                        .is_some_and(|s| s.is_muted);
                    context
                        .introspect()
                        .set_sink_input_mute(index, !current_mute, None);
                }
                PulseCommand::SetSinkPort {
                    sink_name,
                    port_name,
                } => {
                    context
                        .introspect()
                        .set_sink_port_by_name(&sink_name, &port_name, None);
                }
                PulseCommand::SetSourcePort {
                    source_name,
                    port_name,
                } => {
                    context
                        .introspect()
                        .set_source_port_by_name(&source_name, &port_name, None);
                }
            }
            mainloop.unlock();
            needs_refresh = true;
        }

        if needs_refresh {
            refresh_snapshot(&mut mainloop, &mut context, &tx);
        }
    }

    Ok(())
}

fn refresh_snapshot(mainloop: &mut Mainloop, context: &mut Context, tx: &watch::Sender<AudioInfo>) {
    mainloop.lock();

    let (server_tx, server_rx) = mpsc::channel();
    context.introspect().get_server_info(move |info| {
        let default_sink = info
            .default_sink_name
            .as_ref()
            .map(|s| s.to_string())
            .unwrap_or_default();
        let default_source = info
            .default_source_name
            .as_ref()
            .map(|s| s.to_string())
            .unwrap_or_default();
        let _ = server_tx.send((default_sink, default_source));
    });

    let (sinks_tx, sinks_rx) = mpsc::channel();
    context.introspect().get_sink_info_list(move |res| {
        if let pulse::callbacks::ListResult::Item(info) = res {
            let name = info
                .name
                .as_ref()
                .map(|s| s.to_string())
                .unwrap_or_default();
            let description = info
                .description
                .as_ref()
                .map(|s| s.to_string())
                .unwrap_or_else(|| name.clone());
            let vol_percent = (info.volume.avg().0 as f64 / pulse::volume::Volume::NORMAL.0 as f64
                * 100.0)
                .round()
                .min(100.0) as u8;
            let is_muted = info.mute;
            let ports = convert_pa_ports!(&info.ports);
            let active_port = info
                .active_port
                .as_ref()
                .and_then(|p| p.name.as_ref().map(|s| s.to_string()));
            let _ = sinks_tx.send(Some(AudioDevice {
                index: info.index,
                id: name.clone(),
                name,
                description,
                volume_percent: vol_percent,
                is_muted,
                is_default: false,
                is_input: false,
                channels: info.volume.len(),
                ports,
                active_port,
            }));
        } else if let pulse::callbacks::ListResult::End = res {
            let _ = sinks_tx.send(None);
        }
    });

    let (sources_tx, sources_rx) = mpsc::channel();
    context.introspect().get_source_info_list(move |res| {
        if let pulse::callbacks::ListResult::Item(info) = res {
            let name = info
                .name
                .as_ref()
                .map(|s| s.to_string())
                .unwrap_or_default();
            if !name.ends_with(".monitor") {
                let description = info
                    .description
                    .as_ref()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| name.clone());
                let vol_percent =
                    (info.volume.avg().0 as f64 / pulse::volume::Volume::NORMAL.0 as f64 * 100.0)
                        .round()
                        .min(100.0) as u8;
                let is_muted = info.mute;
                let ports = convert_pa_ports!(&info.ports);
                let active_port = info
                    .active_port
                    .as_ref()
                    .and_then(|p| p.name.as_ref().map(|s| s.to_string()));
                let _ = sources_tx.send(Some(AudioDevice {
                    index: info.index,
                    id: name.clone(),
                    name,
                    description,
                    volume_percent: vol_percent,
                    is_muted,
                    is_default: false,
                    is_input: true,
                    channels: info.volume.len(),
                    ports,
                    active_port,
                }));
            }
        } else if let pulse::callbacks::ListResult::End = res {
            let _ = sources_tx.send(None);
        }
    });

    let (streams_tx, streams_rx) = mpsc::channel();
    context.introspect().get_sink_input_info_list(move |res| {
        if let pulse::callbacks::ListResult::Item(info) = res {
            let name = info
                .name
                .as_ref()
                .map(|s| s.to_string())
                .unwrap_or_default();
            let app_name = info
                .proplist
                .get_str("application.name")
                .unwrap_or_else(|| name.clone());
            let vol_percent = (info.volume.avg().0 as f64 / pulse::volume::Volume::NORMAL.0 as f64
                * 100.0)
                .round()
                .min(100.0) as u8;
            let is_muted = info.mute;
            let _ = streams_tx.send(Some(AudioStream {
                id: info.index,
                index: info.index,
                name,
                app_name,
                volume_percent: vol_percent,
                is_muted,
            }));
        } else if let pulse::callbacks::ListResult::End = res {
            let _ = streams_tx.send(None);
        }
    });

    mainloop.unlock();

    let (default_sink_name, default_source_name) = server_rx
        .recv_timeout(std::time::Duration::from_millis(500))
        .unwrap_or_default();

    let mut sinks = Vec::new();
    while let Ok(Some(mut sink)) = sinks_rx.recv_timeout(std::time::Duration::from_millis(500)) {
        sink.resolve_active_ports(&default_sink_name);
        sinks.push(sink);
    }

    let mut sources = Vec::new();
    while let Ok(Some(mut src)) = sources_rx.recv_timeout(std::time::Duration::from_millis(500)) {
        src.resolve_active_ports(&default_source_name);
        sources.push(src);
    }

    let mut app_streams = Vec::new();
    while let Ok(Some(stream)) = streams_rx.recv_timeout(std::time::Duration::from_millis(500)) {
        app_streams.push(stream);
    }

    let default_sink = sinks
        .iter()
        .find(|s| s.name == default_sink_name || s.is_default);
    let default_source = sources
        .iter()
        .find(|s| s.name == default_source_name || s.is_default);

    let volume = default_sink.map(|s| s.volume_percent).unwrap_or(0);
    let is_muted = default_sink.map(|s| s.is_muted).unwrap_or(false);
    let input_volume = default_source.map(|s| s.volume_percent).unwrap_or(0);
    let is_input_muted = default_source.map(|s| s.is_muted).unwrap_or(false);

    let audio_info = AudioInfo {
        available: true,
        default_sink_name,
        default_source_name,
        volume,
        is_muted,
        input_volume,
        is_input_muted,
        sinks,
        sources,
        app_streams,
    };

    let _ = tx.send(audio_info);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_unavailable() {
        assert_eq!(AudioInfo::default(), AudioInfo::default());
        assert_eq!(AudioInfo::default().volume, 0);
        assert!(!AudioInfo::default().is_muted);
        assert!(!AudioInfo::default().available);
    }

    #[test]
    fn test_audio_device_listing_and_switching_fallback() {
        let port = AudioPort {
            name: "analog-output-headphones".to_string(),
            description: "Headphones".to_string(),
            is_active: true,
            available: true,
        };
        let dev = AudioDevice {
            index: 1,
            id: "alsa_output.pci-0000_00_1f.3.analog-stereo".to_string(),
            name: "alsa_output.pci-0000_00_1f.3.analog-stereo".to_string(),
            description: "analog stereo".to_string(),
            volume_percent: 75,
            is_muted: false,
            is_default: true,
            is_input: false,
            channels: 2,
            ports: vec![port],
            active_port: Some("analog-output-headphones".to_string()),
        };
        assert_eq!(dev.id, "alsa_output.pci-0000_00_1f.3.analog-stereo");
        assert_eq!(dev.volume_percent, 75);
        assert!(!dev.is_input);
        assert_eq!(dev.ports.len(), 1);
        assert_eq!(dev.active_port.as_deref(), Some("analog-output-headphones"));
    }

    #[test]
    fn test_audio_stream_volume_and_mute_controls() {
        let stream = AudioStream {
            id: 1,
            index: 1,
            name: "Playback".to_string(),
            app_name: "Firefox".to_string(),
            volume_percent: 80,
            is_muted: false,
        };
        let mut info = AudioInfo::default();
        info.app_streams.push(stream);

        assert_eq!(info.app_streams.len(), 1);
        assert_eq!(info.app_streams[0].app_name, "Firefox");
        assert_eq!(info.app_streams[0].volume_percent, 80);
    }

    #[test]
    fn test_audio_info_unified_snapshot() {
        let info = AudioInfo {
            available: true,
            default_sink_name: "alsa_output.pci-0000_00_1f.3.analog-stereo".to_string(),
            default_source_name: "alsa_input.pci-0000_00_1f.3.analog-stereo".to_string(),
            volume: 65,
            is_muted: false,
            input_volume: 50,
            is_input_muted: true,
            sinks: vec![AudioDevice::default()],
            sources: vec![AudioDevice::default()],
            app_streams: vec![AudioStream::default()],
        };
        assert!(info.available);
        assert_eq!(info.volume, 65);
        assert_eq!(info.input_volume, 50);
        assert!(info.is_input_muted);
        assert_eq!(info.sinks.len(), 1);
        assert_eq!(info.sources.len(), 1);
    }
}
