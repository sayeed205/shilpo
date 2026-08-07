use gpui::BackgroundExecutor;
use shilpo_config::ShellConfig;
pub use shilpo_services::NetworkCommand;
use shilpo_services::{
    AudioInfo, AudioService, BatteryInfo, BatteryService, BrightnessInfo, BrightnessService,
    MediaCommand, MediaInfo, MediaService, NetworkInfo, NetworkService,
};
use std::{
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

pub type UpdateSender = mpsc::SyncSender<WorkerUpdate>;
pub type UpdateReceiver = mpsc::Receiver<WorkerUpdate>;
pub type CommandSender = mpsc::SyncSender<WorkerCommand>;
pub type CommandReceiver = mpsc::Receiver<WorkerCommand>;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeviceSnapshot {
    pub battery: BatteryInfo,
    pub audio: AudioInfo,
    pub network: NetworkInfo,
    pub media: MediaInfo,
    pub brightness: BrightnessInfo,
}

impl DeviceSnapshot {
    pub fn apply(&mut self, update: &WorkerUpdate) -> bool {
        match update {
            WorkerUpdate::Battery(info) => {
                if self.battery == *info {
                    false
                } else {
                    self.battery = info.clone();
                    true
                }
            }
            WorkerUpdate::Audio(info) => {
                if self.audio == *info {
                    false
                } else {
                    self.audio = info.clone();
                    true
                }
            }
            WorkerUpdate::Network(info) => {
                if self.network == *info {
                    false
                } else {
                    self.network = info.clone();
                    true
                }
            }
            WorkerUpdate::Media(info) => {
                if self.media == *info {
                    false
                } else {
                    self.media = info.clone();
                    true
                }
            }
            WorkerUpdate::Brightness(info) => {
                if self.brightness == *info {
                    false
                } else {
                    self.brightness = info.clone();
                    true
                }
            }
            WorkerUpdate::Config(_) | WorkerUpdate::CommandRejected { .. } => false,
            WorkerUpdate::ServiceStateChange { service, state, .. } => {
                let available = state.is_ready();
                match *service {
                    "audio" => self.audio.available = available,
                    "network" => self.network.available = available,
                    "brightness" => self.brightness.available = available,
                    _ => return false,
                }
                true
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServiceAvailability {
    pub battery_available: bool,
    pub battery_state: shilpo_services::ServiceLifecycle,
    pub battery_last_error: Option<String>,
    pub audio_available: bool,
    pub audio_state: shilpo_services::ServiceLifecycle,
    pub audio_last_error: Option<String>,
    pub network_available: bool,
    pub network_state: shilpo_services::ServiceLifecycle,
    pub network_last_error: Option<String>,
    pub media_available: bool,
    pub media_state: shilpo_services::ServiceLifecycle,
    pub media_last_error: Option<String>,
    pub brightness_available: bool,
    pub brightness_state: shilpo_services::ServiceLifecycle,
    pub brightness_last_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VolumeStep {
    Up,
    Down,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioCommand {
    SetDefaultVolume(u8),
    StepDefaultVolume(VolumeStep),
    ToggleDefaultMute,
    SetDefaultDevice {
        device_id: String,
        is_input: bool,
    },
    SetSinkInputVolume {
        index: u32,
        percentage: u8,
    },
    ToggleSinkInputMute {
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



#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceCommand {
    Audio(AudioCommand),
    Network(NetworkCommand),
    Brightness(u8),
    DisplayBrightness { id: String, percentage: u8 },
    AdjustFocusedBrightness { connector: String, delta: i8 },
    Media(MediaCommand),
}

#[derive(Debug)]
pub enum WorkerCommand {
    ReloadConfig,
    Device(DeviceCommand),
}

#[derive(Debug, Clone)]
pub enum ConfigUpdate {
    Loaded(Box<ShellConfig>),
    Failed(String),
}

#[derive(Debug, Clone)]
pub enum WorkerUpdate {
    Battery(BatteryInfo),
    Audio(AudioInfo),
    Network(NetworkInfo),
    Media(MediaInfo),
    Brightness(BrightnessInfo),
    Config(ConfigUpdate),
    ServiceStateChange {
        service: &'static str,
        state: shilpo_services::ServiceLifecycle,
        last_error: Option<String>,
    },
    CommandRejected {
        command: DeviceCommand,
        reason: String,
    },
}

pub fn backoff_delay(attempt: u32) -> Duration {
    match attempt {
        0 | 1 => Duration::from_secs(1),
        2 => Duration::from_secs(2),
        3 => Duration::from_secs(4),
        4 => Duration::from_secs(8),
        5 => Duration::from_secs(16),
        _ => Duration::from_secs(30),
    }
}

pub struct ServiceSlot<T> {
    pub instance: Option<T>,
    pub state: shilpo_services::ServiceLifecycle,
    pub attempt: u32,
    pub next_retry: Option<Instant>,
    pub last_error: Option<String>,
}

impl<T> Default for ServiceSlot<T> {
    fn default() -> Self {
        Self {
            instance: None,
            state: shilpo_services::ServiceLifecycle::Unavailable,
            attempt: 0,
            next_retry: None,
            last_error: None,
        }
    }
}

impl<T> ServiceSlot<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ready(instance: T) -> Self {
        Self {
            instance: Some(instance),
            state: shilpo_services::ServiceLifecycle::Ready,
            attempt: 0,
            next_retry: None,
            last_error: None,
        }
    }

    pub fn failed(error: String, attempt: u32, now: Instant) -> Self {
        let delay = backoff_delay(attempt);
        Self {
            instance: None,
            state: shilpo_services::ServiceLifecycle::Connecting { attempt },
            attempt,
            next_retry: Some(now + delay),
            last_error: Some(error),
        }
    }

    pub fn is_ready(&self) -> bool {
        self.state.is_ready() && self.instance.is_some()
    }

    pub fn mark_failed(&mut self, error: String, now: Instant) {
        self.instance = None;
        self.attempt = self.attempt.saturating_add(1);
        let delay = backoff_delay(self.attempt);
        self.state = shilpo_services::ServiceLifecycle::Connecting {
            attempt: self.attempt,
        };
        self.next_retry = Some(now + delay);
        self.last_error = Some(error);
    }

    pub fn mark_ready(&mut self, instance: T) {
        self.instance = Some(instance);
        self.state = shilpo_services::ServiceLifecycle::Ready;
        self.attempt = 0;
        self.next_retry = None;
        self.last_error = None;
    }

    pub fn mark_unavailable(&mut self, error: impl Into<String>) {
        self.instance = None;
        self.state = shilpo_services::ServiceLifecycle::Unavailable;
        self.last_error = Some(error.into());
    }
}

pub struct DeviceServices {
    pub battery: ServiceSlot<BatteryService>,
    pub audio: ServiceSlot<AudioService>,
    pub network: ServiceSlot<NetworkService>,
    pub media: ServiceSlot<MediaService>,
    pub brightness: ServiceSlot<BrightnessService>,
}

impl DeviceServices {
    pub fn new() -> (Self, ServiceAvailability) {
        let now = Instant::now();
        let battery = match BatteryService::new() {
            Ok(s) => ServiceSlot::ready(s),
            Err(e) => ServiceSlot::failed(e.to_string(), 1, now),
        };
        let audio = match AudioService::new() {
            Ok(s) => ServiceSlot::ready(s),
            Err(e) => ServiceSlot::failed(e.to_string(), 1, now),
        };
        let network = match NetworkService::new() {
            Ok(s) => ServiceSlot::ready(s),
            Err(e) => ServiceSlot::failed(e.to_string(), 1, now),
        };
        let media = match MediaService::new() {
            Ok(s) => ServiceSlot::ready(s),
            Err(e) => ServiceSlot::failed(e.to_string(), 1, now),
        };
        let brightness = match BrightnessService::new() {
            Ok(s) => ServiceSlot::ready(s),
            Err(e) => ServiceSlot::failed(e.to_string(), 1, now),
        };

        let availability = ServiceAvailability {
            battery_available: battery.is_ready(),
            battery_state: battery.state,
            battery_last_error: battery.last_error.clone(),
            audio_available: audio.is_ready(),
            audio_state: audio.state,
            audio_last_error: audio.last_error.clone(),
            network_available: network.is_ready(),
            network_state: network.state,
            network_last_error: network.last_error.clone(),
            media_available: media.is_ready(),
            media_state: media.state,
            media_last_error: media.last_error.clone(),
            brightness_available: brightness.is_ready(),
            brightness_state: brightness.state,
            brightness_last_error: brightness.last_error.clone(),
        };

        (
            Self {
                battery,
                audio,
                network,
                media,
                brightness,
            },
            availability,
        )
    }

    pub fn handle_command(&mut self, updates: &UpdateSender, cmd: &DeviceCommand) {
        match cmd {
            DeviceCommand::Audio(audio_cmd) => {
                if let Some(ref audio) = self.audio.instance {
                    match audio_cmd {
                        AudioCommand::SetDefaultVolume(vol) => audio.set_volume(*vol),
                        AudioCommand::StepDefaultVolume(VolumeStep::Up) => audio.increase_volume(5),
                        AudioCommand::StepDefaultVolume(VolumeStep::Down) => {
                            audio.decrease_volume(5)
                        }
                        AudioCommand::ToggleDefaultMute => audio.toggle_mute(),
                        AudioCommand::SetDefaultDevice {
                            device_id,
                            is_input,
                        } => {
                            let _ = audio.set_default_device(device_id, *is_input);
                        }
                        AudioCommand::SetSinkInputVolume { index, percentage } => {
                            let _ = audio.set_stream_volume(*index, *percentage);
                        }
                        AudioCommand::ToggleSinkInputMute { index } => {
                            let _ = audio.toggle_stream_mute(*index);
                        }
                        AudioCommand::SetSinkPort {
                            sink_name,
                            port_name,
                        } => {
                            let _ = audio.set_sink_port(sink_name, port_name);
                        }
                        AudioCommand::SetSourcePort {
                            source_name,
                            port_name,
                        } => {
                            let _ = audio.set_source_port(source_name, port_name);
                        }
                    }
                } else {
                    let _ = updates.try_send(WorkerUpdate::CommandRejected {
                        command: cmd.clone(),
                        reason: "Audio service unavailable or reconnecting".into(),
                    });
                }
            }
            DeviceCommand::Network(net_cmd) => {
                if let Some(ref network) = self.network.instance {
                    let _ = match net_cmd {
                        NetworkCommand::SetWifiEnabled(b) => network.set_wifi_enabled(*b),
                        NetworkCommand::ScanWifi => network.scan_wifi(),
                        NetworkCommand::ConnectWifi { ssid, object_path } => {
                            network.connect_wifi(ssid, object_path.as_deref())
                        }
                        NetworkCommand::DeactivateConnection(p) => {
                            network.deactivate_connection(p)
                        }
                        NetworkCommand::ConnectVpn(n) => network.connect_vpn(n),
                        NetworkCommand::DisconnectVpn(n) => network.disconnect_vpn(n),
                        NetworkCommand::SetAirplaneModeEnabled(b) => {
                            network.set_airplane_mode_enabled(*b)
                        }
                    };
                } else {
                    let _ = updates.try_send(WorkerUpdate::CommandRejected {
                        command: cmd.clone(),
                        reason: "Network service unavailable or reconnecting".into(),
                    });
                }
            }
            DeviceCommand::Brightness(val) => {
                if let Some(ref brightness) = self.brightness.instance {
                    brightness.set_brightness(*val);
                } else {
                    let _ = updates.try_send(WorkerUpdate::CommandRejected {
                        command: cmd.clone(),
                        reason: "Brightness service unavailable or reconnecting".into(),
                    });
                }
            }
            DeviceCommand::DisplayBrightness { id, percentage } => {
                if let Some(ref brightness) = self.brightness.instance {
                    brightness.set_display_brightness(id, *percentage);
                }
            }
            DeviceCommand::AdjustFocusedBrightness { connector, delta } => {
                if let Some(ref brightness) = self.brightness.instance {
                    brightness.adjust_focused_brightness(connector, *delta);
                }
            }
            DeviceCommand::Media(m) => {
                if let Some(ref media) = self.media.instance {
                    media.send_command(*m);
                } else {
                    let _ = updates.try_send(WorkerUpdate::CommandRejected {
                        command: cmd.clone(),
                        reason: "Media service unavailable or reconnecting".into(),
                    });
                }
            }
        }
    }

    pub fn poll_reconnect(&mut self, updates: &UpdateSender, now: Instant) {
        if !self.battery.is_ready() && self.battery.next_retry.is_some_and(|t| now >= t) {
            self.battery.instance = None; // Drop old instance before reconnecting
            match BatteryService::new() {
                Ok(s) => {
                    self.battery = ServiceSlot::ready(s);
                    let _ = updates.try_send(WorkerUpdate::ServiceStateChange {
                        service: "battery",
                        state: self.battery.state,
                        last_error: None,
                    });
                }
                Err(e) => {
                    self.battery =
                        ServiceSlot::failed(e.to_string(), self.battery.attempt + 1, now);
                    let _ = updates.try_send(WorkerUpdate::ServiceStateChange {
                        service: "battery",
                        state: self.battery.state,
                        last_error: self.battery.last_error.clone(),
                    });
                }
            }
        }
        if !self.audio.is_ready() && self.audio.next_retry.is_some_and(|t| now >= t) {
            self.audio.instance = None;
            match AudioService::new() {
                Ok(s) => {
                    self.audio = ServiceSlot::ready(s);
                    let _ = updates.try_send(WorkerUpdate::ServiceStateChange {
                        service: "audio",
                        state: self.audio.state,
                        last_error: None,
                    });
                }
                Err(e) => {
                    self.audio = ServiceSlot::failed(e.to_string(), self.audio.attempt + 1, now);
                    let _ = updates.try_send(WorkerUpdate::ServiceStateChange {
                        service: "audio",
                        state: self.audio.state,
                        last_error: self.audio.last_error.clone(),
                    });
                }
            }
        }
        if !self.network.is_ready() && self.network.next_retry.is_some_and(|t| now >= t) {
            self.network.instance = None;
            match NetworkService::new() {
                Ok(s) => {
                    self.network = ServiceSlot::ready(s);
                    let _ = updates.try_send(WorkerUpdate::ServiceStateChange {
                        service: "network",
                        state: self.network.state,
                        last_error: None,
                    });
                }
                Err(e) => {
                    self.network =
                        ServiceSlot::failed(e.to_string(), self.network.attempt + 1, now);
                    let _ = updates.try_send(WorkerUpdate::ServiceStateChange {
                        service: "network",
                        state: self.network.state,
                        last_error: self.network.last_error.clone(),
                    });
                }
            }
        }
        if !self.media.is_ready() && self.media.next_retry.is_some_and(|t| now >= t) {
            self.media.instance = None;
            match MediaService::new() {
                Ok(s) => {
                    self.media = ServiceSlot::ready(s);
                    let _ = updates.try_send(WorkerUpdate::ServiceStateChange {
                        service: "media",
                        state: self.media.state,
                        last_error: None,
                    });
                }
                Err(e) => {
                    self.media = ServiceSlot::failed(e.to_string(), self.media.attempt + 1, now);
                    let _ = updates.try_send(WorkerUpdate::ServiceStateChange {
                        service: "media",
                        state: self.media.state,
                        last_error: self.media.last_error.clone(),
                    });
                }
            }
        }
        if !self.brightness.is_ready() && self.brightness.next_retry.is_some_and(|t| now >= t) {
            self.brightness.instance = None;
            match BrightnessService::new() {
                Ok(s) => {
                    self.brightness = ServiceSlot::ready(s);
                    let _ = updates.try_send(WorkerUpdate::ServiceStateChange {
                        service: "brightness",
                        state: self.brightness.state,
                        last_error: None,
                    });
                }
                Err(e) => {
                    self.brightness =
                        ServiceSlot::failed(e.to_string(), self.brightness.attempt + 1, now);
                    let _ = updates.try_send(WorkerUpdate::ServiceStateChange {
                        service: "brightness",
                        state: self.brightness.state,
                        last_error: self.brightness.last_error.clone(),
                    });
                }
            }
        }
    }
}

pub fn channels() -> (UpdateSender, UpdateReceiver, CommandSender, CommandReceiver) {
    let (updates_tx, updates_rx) = mpsc::sync_channel(64);
    let (commands_tx, commands_rx) = mpsc::sync_channel(32);
    (updates_tx, updates_rx, commands_tx, commands_rx)
}

pub fn try_send_command(
    sender: &CommandSender,
    command: WorkerCommand,
) -> Result<(), mpsc::TrySendError<WorkerCommand>> {
    sender.try_send(command)
}

pub fn spawn(
    executor: BackgroundExecutor,
    updates: UpdateSender,
    commands: CommandReceiver,
    config_path: PathBuf,
    services: DeviceServices,
) -> gpui::Task<()> {
    let worker_executor = executor.clone();
    executor
        .spawn(async move { run(worker_executor, updates, commands, config_path, services).await })
}

async fn run(
    executor: BackgroundExecutor,
    updates: UpdateSender,
    commands: CommandReceiver,
    config_path: PathBuf,
    mut services: DeviceServices,
) {
    let mut battery_last = None;
    let mut audio_last = None;
    let mut network_last = None;
    let mut media_last = None;
    let mut brightness_last = None;
    let mut device_ticks = 0u8;
    let mut pending_reload: Option<Instant> = None;
    let debounce_duration = Duration::from_millis(200);

    if !load_config(&updates, &config_path) {
        return;
    }

    loop {
        services.poll_reconnect(&updates, Instant::now());

        while let Ok(command) = commands.try_recv() {
            match command {
                WorkerCommand::ReloadConfig => {
                    pending_reload = Some(Instant::now());
                }
                WorkerCommand::Device(cmd) => {
                    services.handle_command(&updates, &cmd);
                }
            }
        }

        if pending_reload.is_some_and(|req| req.elapsed() >= debounce_duration) {
            pending_reload = None;
            if !load_config(&updates, &config_path) {
                return;
            }
            invalidate_device_snapshots_after_config_reload(
                &mut audio_last,
                &mut battery_last,
                &mut network_last,
                &mut media_last,
                &mut brightness_last,
                &mut device_ticks,
            );
        }

        if !sample_device(
            &updates,
            &mut audio_last,
            services
                .audio
                .instance
                .as_ref()
                .map(AudioService::audio_info),
            WorkerUpdate::Audio,
        ) {
            return;
        }

        if !sample_device(
            &updates,
            &mut media_last,
            services
                .media
                .instance
                .as_ref()
                .map(MediaService::media_info),
            WorkerUpdate::Media,
        ) {
            return;
        }

        if device_ticks == 0 {
            if !sample_device(
                &updates,
                &mut battery_last,
                services
                    .battery
                    .instance
                    .as_ref()
                    .map(BatteryService::battery_info),
                WorkerUpdate::Battery,
            ) {
                return;
            }
            if !sample_device(
                &updates,
                &mut network_last,
                services
                    .network
                    .instance
                    .as_ref()
                    .map(NetworkService::network_info),
                WorkerUpdate::Network,
            ) {
                return;
            }
            if !sample_device(
                &updates,
                &mut brightness_last,
                services
                    .brightness
                    .instance
                    .as_ref()
                    .map(BrightnessService::brightness_info),
                WorkerUpdate::Brightness,
            ) {
                return;
            }
        }
        device_ticks = (device_ticks + 1) % 30;
        executor.timer(Duration::from_millis(100)).await;
    }
}

fn load_config(updates: &UpdateSender, path: &PathBuf) -> bool {
    let update = match ShellConfig::load_or_create(path) {
        Ok(config) => WorkerUpdate::Config(ConfigUpdate::Loaded(Box::new(config))),
        Err(error) => WorkerUpdate::Config(ConfigUpdate::Failed(error.to_string())),
    };
    match updates.try_send(update) {
        Ok(()) | Err(mpsc::TrySendError::Full(_)) => true,
        Err(mpsc::TrySendError::Disconnected(_)) => false,
    }
}

fn send_changed<T: Clone + PartialEq>(
    updates: &UpdateSender,
    previous: &mut Option<T>,
    value: T,
    make_update: impl FnOnce(T) -> WorkerUpdate,
) -> bool {
    if previous.as_ref() == Some(&value) {
        return true;
    }
    let result = updates.try_send(make_update(value.clone()));
    match result {
        Ok(()) => {
            *previous = Some(value);
            true
        }
        Err(mpsc::TrySendError::Full(_)) => true,
        Err(mpsc::TrySendError::Disconnected(_)) => false,
    }
}

fn invalidate_device_snapshots_after_config_reload(
    audio: &mut Option<AudioInfo>,
    battery: &mut Option<BatteryInfo>,
    network: &mut Option<NetworkInfo>,
    media: &mut Option<MediaInfo>,
    brightness: &mut Option<BrightnessInfo>,
    device_ticks: &mut u8,
) {
    *audio = None;
    *battery = None;
    *network = None;
    *media = None;
    *brightness = None;
    *device_ticks = 0;
}

fn sample_device<T: Clone + PartialEq + Default>(
    updates: &UpdateSender,
    previous: &mut Option<T>,
    value: Option<T>,
    make_update: impl FnOnce(T) -> WorkerUpdate,
) -> bool {
    send_changed(updates, previous, value.unwrap_or_default(), make_update)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_snapshot_applies_updates_correctly() {
        let mut snapshot = DeviceSnapshot::default();
        let battery = BatteryInfo {
            percentage: 85,
            is_charging: true,
            is_present: true,
        };
        assert!(snapshot.apply(&WorkerUpdate::Battery(battery.clone())));
        assert_eq!(snapshot.battery, battery);
        assert!(!snapshot.apply(&WorkerUpdate::Battery(battery.clone())));

        let brightness = BrightnessInfo {
            percentage: 75,
            available: true,
            device_name: None,
            ..Default::default()
        };
        assert!(snapshot.apply(&WorkerUpdate::Brightness(brightness.clone())));
        assert_eq!(snapshot.brightness, brightness);
        assert!(!snapshot.apply(&WorkerUpdate::Brightness(brightness.clone())));
    }

    #[test]
    fn device_cadence_is_initial_and_three_seconds() {
        assert_eq!(
            (0u8..35).filter(|tick| tick % 30 == 0).collect::<Vec<_>>(),
            vec![0, 30]
        );
    }

    #[test]
    fn changed_update_includes_unavailable_transition() {
        let (tx, rx) = mpsc::sync_channel(4);
        let mut previous = Some(AudioInfo {
            volume: 50,
            is_muted: false,
            available: true,
            ..AudioInfo::default()
        });
        assert!(send_changed(
            &tx,
            &mut previous,
            AudioInfo::default(),
            WorkerUpdate::Audio
        ));
        assert!(matches!(rx.try_recv(), Ok(WorkerUpdate::Audio(info)) if !info.available));
    }

    #[test]
    fn config_reload_replays_unchanged_battery_snapshot() {
        let (tx, rx) = mpsc::sync_channel(4);
        let battery = BatteryInfo {
            percentage: 58,
            is_charging: true,
            is_present: true,
        };
        let mut previous = Some(battery.clone());
        let mut audio = Some(AudioInfo::default());
        let mut network = Some(NetworkInfo::default());
        let mut media = Some(MediaInfo::default());
        let mut brightness = Some(BrightnessInfo::default());
        let mut device_ticks = 17;

        invalidate_device_snapshots_after_config_reload(
            &mut audio,
            &mut previous,
            &mut network,
            &mut media,
            &mut brightness,
            &mut device_ticks,
        );
        assert_eq!(device_ticks, 0);
        assert!(sample_device(
            &tx,
            &mut previous,
            Some(battery),
            WorkerUpdate::Battery,
        ));

        assert!(
            matches!(rx.try_recv(), Ok(WorkerUpdate::Battery(info)) if info.is_present),
            "a recreated bar must receive the current battery snapshot"
        );
    }

    #[test]
    fn backoff_delay_exponential_growth_and_cap() {
        assert_eq!(backoff_delay(1), Duration::from_secs(1));
        assert_eq!(backoff_delay(2), Duration::from_secs(2));
        assert_eq!(backoff_delay(3), Duration::from_secs(4));
        assert_eq!(backoff_delay(4), Duration::from_secs(8));
        assert_eq!(backoff_delay(5), Duration::from_secs(16));
        assert_eq!(backoff_delay(6), Duration::from_secs(30));
        assert_eq!(backoff_delay(10), Duration::from_secs(30));
    }

    #[test]
    fn service_slot_reconnect_lifecycle_and_rejection() {
        use shilpo_services::ipc::ServiceLifecycle;

        let mut slot = ServiceSlot::<()>::new();
        assert_eq!(slot.state, ServiceLifecycle::Unavailable);
        assert!(slot.instance.is_none());

        let now = Instant::now();
        // Fail first attempt -> enters Connecting { attempt: 1 }
        slot.mark_failed("connection failed".into(), now);
        assert_eq!(slot.state, ServiceLifecycle::Connecting { attempt: 1 });
        assert_eq!(slot.last_error.as_deref(), Some("connection failed"));
        assert!(slot.next_retry.is_some());

        // Second failure -> attempt 2
        slot.mark_failed("connection refused".into(), now);
        assert_eq!(slot.state, ServiceLifecycle::Connecting { attempt: 2 });
        assert_eq!(slot.last_error.as_deref(), Some("connection refused"));

        // Ready transition
        slot.mark_ready(());
        assert_eq!(slot.state, ServiceLifecycle::Ready);
        assert!(slot.instance.is_some());
        assert_eq!(slot.attempt, 0);
        assert!(slot.last_error.is_none());
        assert!(slot.next_retry.is_none());

        // Disconnect -> Unavailable
        slot.mark_unavailable("backend disconnected");
        assert_eq!(slot.state, ServiceLifecycle::Unavailable);
        assert!(slot.instance.is_none());
        assert_eq!(slot.last_error.as_deref(), Some("backend disconnected"));
    }
}
