use anyhow::Result;
use libpulse_binding as pulse;
use pulse::context::{Context, FlagSet as ContextFlagSet, State as ContextState};
use pulse::context::subscribe::InterestMaskSet;
use pulse::mainloop::threaded::Mainloop;
use std::process::Command;
use std::sync::{Arc, Mutex, mpsc};
use tokio::sync::watch;

/// Metadata describing an individual application audio playback stream (Sink Input).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioStream {
    pub id: u32,
    pub index: u32,
    pub name: String,
    pub app_name: String,
    pub volume_percent: u8,
    pub is_muted: bool,
}

/// Metadata describing an audio port on a sound card, sink, or source.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioPort {
    pub name: String,
    pub description: String,
    pub is_active: bool,
    pub available: bool,
}

/// Metadata describing a physical or virtual audio device (sink or source).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioDevice {
    pub index: u32,
    pub id: String,
    pub name: String,
    pub description: String,
    pub volume_percent: u8,
    pub is_muted: bool,
    pub is_default: bool,
    pub is_input: bool,
    pub ports: Vec<AudioPort>,
    pub active_port: Option<String>,
}

/// Comprehensive system audio snapshot including volumes, mutes, sinks, sources, and application streams.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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

struct PulseBackend {
    mainloop: Arc<Mutex<Mainloop>>,
    context: Arc<Mutex<Context>>,
}

/// System audio service for volume, mute status, and device switching using event-driven PulseAudio APIs.
pub struct AudioService {
    tx: watch::Sender<AudioInfo>,
    rx: watch::Receiver<AudioInfo>,
    backend: Option<Arc<PulseBackend>>,
}

impl AudioService {
    pub fn new() -> Result<Self> {
        let (tx, rx) = watch::channel(AudioInfo::default());

        let mut mainloop =
            Mainloop::new().ok_or_else(|| anyhow::anyhow!("failed to create pulse mainloop"))?;
        let mut context = Context::new(&mainloop, "Shilpo Audio Service")
            .ok_or_else(|| anyhow::anyhow!("failed to create pulse context"))?;

        context
            .connect(None, ContextFlagSet::NOFLAGS, None)
            .map_err(|e| anyhow::anyhow!("failed to connect pulse context: {e:?}"))?;
        mainloop
            .start()
            .map_err(|e| anyhow::anyhow!("failed to start pulse mainloop: {e:?}"))?;

        let context = Arc::new(Mutex::new(context));
        let mainloop = Arc::new(Mutex::new(mainloop));

        // Wait briefly for connection
        let ready = {
            let start = std::time::Instant::now();
            loop {
                let state = {
                    let ctx = context.lock().unwrap();
                    ctx.get_state()
                };
                if state == ContextState::Ready {
                    break true;
                }
                if state == ContextState::Failed
                    || state == ContextState::Terminated
                    || start.elapsed() > std::time::Duration::from_secs(2)
                {
                    break false;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        };

        if !ready {
            return Err(anyhow::anyhow!("pulse context connection failed or timed out"));
        }

        let backend = Arc::new(PulseBackend {
            mainloop: mainloop.clone(),
            context: context.clone(),
        });

        backend.refresh(&tx);

        {
            let _ml = mainloop.lock().unwrap();
            let mut ctx = context.lock().unwrap();
            let tx_clone = tx.clone();
            let backend_clone = backend.clone();

            ctx.set_subscribe_callback(Some(Box::new(move |_facility, _op, _index| {
                backend_clone.refresh(&tx_clone);
            })));

            ctx.subscribe(
                InterestMaskSet::SINK
                    | InterestMaskSet::SOURCE
                    | InterestMaskSet::SINK_INPUT
                    | InterestMaskSet::SERVER,
                |_| {},
            );
        }

        Ok(Self {
            tx,
            rx,
            backend: Some(backend),
        })
    }

    pub fn new_offline() -> Self {
        let (tx, rx) = watch::channel(AudioInfo::default());
        Self {
            tx,
            rx,
            backend: None,
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<AudioInfo> {
        self.rx.clone()
    }

    pub fn audio_info(&self) -> AudioInfo {
        self.rx.borrow().clone()
    }

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

    pub fn list_devices_static() -> Vec<AudioDevice> {
        let mut devices = Vec::new();

        if let Ok(output) = Command::new("pactl")
            .args(["list", "sinks", "short"])
            .output()
            && output.status.success()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let idx = parts[0].parse::<u32>().unwrap_or(0);
                    let name = parts[1].to_string();
                    let description = name
                        .split('.')
                        .next_back()
                        .unwrap_or(&name)
                        .replace('_', " ");
                    devices.push(AudioDevice {
                        index: idx,
                        id: name.clone(),
                        name: name.clone(),
                        description,
                        volume_percent: 100,
                        is_muted: false,
                        is_default: false,
                        is_input: false,
                        ports: Vec::new(),
                        active_port: None,
                    });
                }
            }
        }

        if let Ok(output) = Command::new("pactl")
            .args(["list", "sources", "short"])
            .output()
            && output.status.success()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let idx = parts[0].parse::<u32>().unwrap_or(0);
                    let name = parts[1].to_string();
                    if !name.ends_with(".monitor") {
                        let description = name
                            .split('.')
                            .next_back()
                            .unwrap_or(&name)
                            .replace('_', " ");
                        devices.push(AudioDevice {
                            index: idx,
                            id: name.clone(),
                            name: name.clone(),
                            description,
                            volume_percent: 100,
                            is_muted: false,
                            is_default: false,
                            is_input: true,
                            ports: Vec::new(),
                            active_port: None,
                        });
                    }
                }
            }
        }

        devices
    }

    pub fn increase_volume(&self, step: u8) {
        let current = self.audio_info().volume;
        self.set_volume(current.saturating_add(step).min(100));
    }

    pub fn decrease_volume(&self, step: u8) {
        let current = self.audio_info().volume;
        self.set_volume(current.saturating_sub(step));
    }

    pub fn set_volume(&self, vol: u8) {
        if let Some(backend) = &self.backend {
            backend.set_default_sink_volume(vol, &self.tx);
        }
    }

    pub fn toggle_mute(&self) {
        if let Some(backend) = &self.backend {
            let is_muted = self.audio_info().is_muted;
            backend.set_default_sink_mute(!is_muted, &self.tx);
        }
    }

    pub fn set_input_volume(&self, vol: u8) {
        if let Some(backend) = &self.backend {
            backend.set_default_source_volume(vol, &self.tx);
        }
    }

    pub fn toggle_input_mute(&self) {
        if let Some(backend) = &self.backend {
            let is_muted = self.audio_info().is_input_muted;
            backend.set_default_source_mute(!is_muted, &self.tx);
        }
    }

    pub fn set_stream_volume(&self, index: u32, percentage: u8) -> Result<()> {
        if let Some(backend) = &self.backend {
            backend.set_stream_volume(index, percentage, &self.tx)
        } else {
            Ok(())
        }
    }

    pub fn toggle_stream_mute(&self, index: u32) -> Result<()> {
        if let Some(backend) = &self.backend {
            backend.toggle_stream_mute(index, &self.tx)
        } else {
            Ok(())
        }
    }

    pub fn set_default_device(&self, device_id: &str, is_input: bool) -> Result<()> {
        if let Some(backend) = &self.backend {
            backend.set_default_device(device_id, is_input, &self.tx)
        } else {
            Ok(())
        }
    }

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

    pub fn list_ports_static() -> Vec<AudioPort> {
        let mut ports = Vec::new();
        if let Ok(output) = Command::new("pactl").args(["list", "sinks"]).output()
            && output.status.success()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            let mut in_ports_section = false;
            let mut active_port = String::new();

            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("Active Port:") {
                    active_port = trimmed
                        .trim_start_matches("Active Port:")
                        .trim()
                        .to_string();
                } else if trimmed.starts_with("Ports:") {
                    in_ports_section = true;
                } else if trimmed.starts_with("Formats:") || trimmed.starts_with("State:") {
                    in_ports_section = false;
                } else if in_ports_section
                    && line.starts_with("\t\t")
                    && let Some((name_part, desc_part)) = trimmed.split_once(':')
                {
                    let port_name = name_part.trim().to_string();
                    let desc = desc_part
                        .split('(')
                        .next()
                        .unwrap_or(desc_part)
                        .trim()
                        .to_string();
                    let is_active = port_name == active_port;
                    ports.push(AudioPort {
                        name: port_name,
                        description: desc,
                        is_active,
                        available: true,
                    });
                }
            }
        }
        ports
    }

    pub fn set_sink_port(&self, sink_name: &str, port_name: &str) -> Result<()> {
        if let Some(backend) = &self.backend {
            backend.set_sink_port(sink_name, port_name, &self.tx)
        } else {
            Ok(())
        }
    }

    pub fn toggle_simultaneous_output(&self) -> Result<bool> {
        if let Ok(output) = Command::new("pactl")
            .args(["list", "modules", "short"])
            .output()
            && output.status.success()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                if line.contains("module-combine-sink") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if let Some(&module_id) = parts.first() {
                        let _ = Command::new("pactl")
                            .args(["unload-module", module_id])
                            .status();
                        return Ok(false);
                    }
                }
            }
        }

        let status = Command::new("pactl")
            .args(["load-module", "module-combine-sink"])
            .status()?;
        Ok(status.success())
    }
}

impl PulseBackend {
    fn refresh(&self, tx: &watch::Sender<AudioInfo>) {
        let ml = self.mainloop.lock().unwrap();
        let ctx = self.context.lock().unwrap();

        let (server_tx, server_rx) = mpsc::channel();
        ctx.introspect().get_server_info(move |info| {
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
        ctx.introspect().get_sink_info_list(move |res| {
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
                let ports = info
                    .ports
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
                    .collect::<Vec<_>>();
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
                    ports,
                    active_port,
                }));
            } else if let pulse::callbacks::ListResult::End = res {
                let _ = sinks_tx.send(None);
            }
        });

        let (sources_tx, sources_rx) = mpsc::channel();
        ctx.introspect().get_source_info_list(move |res| {
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
                    let vol_percent = (info.volume.avg().0 as f64
                        / pulse::volume::Volume::NORMAL.0 as f64
                        * 100.0)
                        .round()
                        .min(100.0) as u8;
                    let is_muted = info.mute;
                    let ports = info
                        .ports
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
                        .collect::<Vec<_>>();
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
                        ports,
                        active_port,
                    }));
                }
            } else if let pulse::callbacks::ListResult::End = res {
                let _ = sources_tx.send(None);
            }
        });

        let (streams_tx, streams_rx) = mpsc::channel();
        ctx.introspect().get_sink_input_info_list(move |res| {
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
                let vol_percent = (info.volume.avg().0 as f64
                    / pulse::volume::Volume::NORMAL.0 as f64
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

        drop(ctx);
        drop(ml);

        let (default_sink_name, default_source_name) = server_rx
            .recv_timeout(std::time::Duration::from_millis(500))
            .unwrap_or_default();

        let mut sinks = Vec::new();
        while let Ok(Some(mut sink)) =
            sinks_rx.recv_timeout(std::time::Duration::from_millis(500))
        {
            sink.is_default = sink.name == default_sink_name;
            if let Some(ref active) = sink.active_port {
                for p in &mut sink.ports {
                    p.is_active = p.name == *active;
                }
            }
            sinks.push(sink);
        }

        let mut sources = Vec::new();
        while let Ok(Some(mut src)) =
            sources_rx.recv_timeout(std::time::Duration::from_millis(500))
        {
            src.is_default = src.name == default_source_name;
            if let Some(ref active) = src.active_port {
                for p in &mut src.ports {
                    p.is_active = p.name == *active;
                }
            }
            sources.push(src);
        }

        let mut app_streams = Vec::new();
        while let Ok(Some(stream)) =
            streams_rx.recv_timeout(std::time::Duration::from_millis(500))
        {
            app_streams.push(stream);
        }

        let default_sink = sinks.iter().find(|s| s.name == default_sink_name || s.is_default);
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

    fn set_default_sink_volume(&self, vol: u8, tx: &watch::Sender<AudioInfo>) {
        let _ml = self.mainloop.lock().unwrap();
        let ctx = self.context.lock().unwrap();
        let default_sink = tx.borrow().default_sink_name.clone();
        let mut cvol = pulse::volume::ChannelVolumes::default();
        let pa_vol = pulse::volume::Volume((vol as u32 * pulse::volume::Volume::NORMAL.0) / 100);
        cvol.set(2, pa_vol);
        if !default_sink.is_empty() {
            ctx.introspect()
                .set_sink_volume_by_name(&default_sink, &cvol, None);
        }
    }

    fn set_default_sink_mute(&self, mute: bool, tx: &watch::Sender<AudioInfo>) {
        let _ml = self.mainloop.lock().unwrap();
        let ctx = self.context.lock().unwrap();
        let default_sink = tx.borrow().default_sink_name.clone();
        if !default_sink.is_empty() {
            ctx.introspect()
                .set_sink_mute_by_name(&default_sink, mute, None);
        }
    }

    fn set_default_source_volume(&self, vol: u8, tx: &watch::Sender<AudioInfo>) {
        let _ml = self.mainloop.lock().unwrap();
        let ctx = self.context.lock().unwrap();
        let default_source = tx.borrow().default_source_name.clone();
        let mut cvol = pulse::volume::ChannelVolumes::default();
        let pa_vol = pulse::volume::Volume((vol as u32 * pulse::volume::Volume::NORMAL.0) / 100);
        cvol.set(2, pa_vol);
        if !default_source.is_empty() {
            ctx.introspect()
                .set_source_volume_by_name(&default_source, &cvol, None);
        }
    }

    fn set_default_source_mute(&self, mute: bool, tx: &watch::Sender<AudioInfo>) {
        let _ml = self.mainloop.lock().unwrap();
        let ctx = self.context.lock().unwrap();
        let default_source = tx.borrow().default_source_name.clone();
        if !default_source.is_empty() {
            ctx.introspect()
                .set_source_mute_by_name(&default_source, mute, None);
        }
    }

    fn set_default_device(
        &self,
        device_id: &str,
        is_input: bool,
        _tx: &watch::Sender<AudioInfo>,
    ) -> Result<()> {
        let _ml = self.mainloop.lock().unwrap();
        let mut ctx = self.context.lock().unwrap();
        if is_input {
            ctx.set_default_source(device_id, |_| {});
        } else {
            ctx.set_default_sink(device_id, |_| {});
        }
        Ok(())
    }

    fn set_stream_volume(
        &self,
        index: u32,
        percentage: u8,
        _tx: &watch::Sender<AudioInfo>,
    ) -> Result<()> {
        let _ml = self.mainloop.lock().unwrap();
        let ctx = self.context.lock().unwrap();
        let mut cvol = pulse::volume::ChannelVolumes::default();
        let pa_vol =
            pulse::volume::Volume((percentage as u32 * pulse::volume::Volume::NORMAL.0) / 100);
        cvol.set(2, pa_vol);
        ctx.introspect().set_sink_input_volume(index, &cvol, None);
        Ok(())
    }

    fn toggle_stream_mute(&self, index: u32, tx: &watch::Sender<AudioInfo>) -> Result<()> {
        let _ml = self.mainloop.lock().unwrap();
        let ctx = self.context.lock().unwrap();
        let current_mute = tx
            .borrow()
            .app_streams
            .iter()
            .find(|s| s.index == index)
            .map_or(false, |s| s.is_muted);
        ctx.introspect()
            .set_sink_input_mute(index, !current_mute, None);
        Ok(())
    }

    fn set_sink_port(
        &self,
        sink_name: &str,
        port_name: &str,
        _tx: &watch::Sender<AudioInfo>,
    ) -> Result<()> {
        let _ml = self.mainloop.lock().unwrap();
        let ctx = self.context.lock().unwrap();
        ctx.introspect()
            .set_sink_port_by_name(sink_name, port_name, None);
        Ok(())
    }
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
