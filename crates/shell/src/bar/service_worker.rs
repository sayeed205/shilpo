use gpui::BackgroundExecutor;
use shilpo_config::ShellConfig;
use shilpo_services::{
    AudioDevice, AudioInfo, AudioPort, AudioService, BatteryInfo, BatteryService, BrightnessInfo,
    BrightnessService, MediaCommand, MediaInfo, MediaService, NetworkInfo, NetworkService,
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioHardwareSnapshot {
    pub devices: Vec<AudioDevice>,
    pub ports: Vec<AudioPort>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeviceSnapshot {
    pub battery: BatteryInfo,
    pub audio: AudioInfo,
    pub audio_hardware: AudioHardwareSnapshot,
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
            WorkerUpdate::AudioHardware(info) => {
                if self.audio_hardware == *info {
                    false
                } else {
                    self.audio_hardware = info.clone();
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
            WorkerUpdate::Config(_) => false,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServiceAvailability {
    pub battery_available: bool,
    pub audio_available: bool,
    pub network_available: bool,
    pub media_available: bool,
    pub brightness_available: bool,
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
    ToggleSimultaneousOutput,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetworkCommand {
    SetWifiEnabled(bool),
    DeactivateConnection(String),
    ConnectVpn(String),
    DisconnectVpn(String),
    SetAirplaneModeEnabled(bool),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceCommand {
    Audio(AudioCommand),
    Network(NetworkCommand),
    Brightness(u8),
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
    AudioHardware(AudioHardwareSnapshot),
    Config(ConfigUpdate),
}

pub struct DeviceServices {
    pub battery: Option<BatteryService>,
    pub audio: Option<AudioService>,
    pub network: Option<NetworkService>,
    pub media: Option<MediaService>,
    pub brightness: Option<BrightnessService>,
}

impl DeviceServices {
    pub fn new() -> (Self, ServiceAvailability) {
        let battery = BatteryService::new().ok();
        let audio = AudioService::new().ok();
        let network = NetworkService::new().ok();
        let media = MediaService::new().ok();
        let brightness = BrightnessService::new().ok();

        let availability = ServiceAvailability {
            battery_available: battery.is_some(),
            audio_available: audio.is_some(),
            network_available: network.is_some(),
            media_available: media.is_some(),
            brightness_available: brightness.is_some(),
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

    pub fn handle_command(&self, cmd: &DeviceCommand) {
        match cmd {
            DeviceCommand::Audio(audio_cmd) => {
                if let Some(ref audio) = self.audio {
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
                        AudioCommand::ToggleSimultaneousOutput => {
                            let _ = audio.toggle_simultaneous_output();
                        }
                    }
                }
            }
            DeviceCommand::Network(net_cmd) => {
                if let Some(ref network) = self.network {
                    let _ = match net_cmd {
                        NetworkCommand::SetWifiEnabled(b) => network.set_wifi_enabled(*b),
                        NetworkCommand::DeactivateConnection(p) => network.deactivate_connection(p),
                        NetworkCommand::ConnectVpn(n) => network.connect_vpn(n),
                        NetworkCommand::DisconnectVpn(n) => network.disconnect_vpn(n),
                        NetworkCommand::SetAirplaneModeEnabled(b) => {
                            network.set_airplane_mode_enabled(*b)
                        }
                    };
                }
            }
            DeviceCommand::Brightness(val) => {
                if let Some(ref brightness) = self.brightness {
                    brightness.set_brightness(*val);
                }
            }
            DeviceCommand::Media(m) => {
                if let Some(ref media) = self.media {
                    media.send_command(*m);
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
    services: DeviceServices,
) {
    let mut battery_last = None;
    let mut audio_last = None;
    let mut network_last = None;
    let mut media_last = None;
    let mut brightness_last = None;
    let mut audio_hardware_last = None;
    let mut device_ticks = 0u8;
    let mut pending_reload: Option<Instant> = None;
    let debounce_duration = Duration::from_millis(200);

    if !load_config(&updates, &config_path) {
        return;
    }

    loop {
        while let Ok(command) = commands.try_recv() {
            match command {
                WorkerCommand::ReloadConfig => {
                    pending_reload = Some(Instant::now());
                }
                WorkerCommand::Device(cmd) => {
                    services.handle_command(&cmd);
                    if let DeviceCommand::Audio(
                        AudioCommand::SetDefaultDevice { .. }
                        | AudioCommand::SetSinkPort { .. }
                        | AudioCommand::ToggleSimultaneousOutput,
                    ) = &cmd
                    {
                        audio_hardware_last = None;
                        if !sample_device(
                            &updates,
                            &mut audio_hardware_last,
                            services.audio.as_ref().map(|a| AudioHardwareSnapshot {
                                devices: a.list_devices(),
                                ports: a.list_ports(),
                            }),
                            WorkerUpdate::AudioHardware,
                        ) {
                            return;
                        }
                    }
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
                &mut audio_hardware_last,
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
            services.audio.as_ref().map(AudioService::audio_info),
            WorkerUpdate::Audio,
        ) {
            return;
        }

        if !sample_device(
            &updates,
            &mut media_last,
            services.media.as_ref().map(MediaService::media_info),
            WorkerUpdate::Media,
        ) {
            return;
        }

        if device_ticks == 0 {
            if !sample_device(
                &updates,
                &mut battery_last,
                services.battery.as_ref().map(BatteryService::battery_info),
                WorkerUpdate::Battery,
            ) {
                return;
            }
            if !sample_device(
                &updates,
                &mut network_last,
                services.network.as_ref().map(NetworkService::network_info),
                WorkerUpdate::Network,
            ) {
                return;
            }
            if !sample_device(
                &updates,
                &mut audio_hardware_last,
                services.audio.as_ref().map(|a| AudioHardwareSnapshot {
                    devices: a.list_devices(),
                    ports: a.list_ports(),
                }),
                WorkerUpdate::AudioHardware,
            ) {
                return;
            }
            if !sample_device(
                &updates,
                &mut brightness_last,
                services
                    .brightness
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
    audio_hardware: &mut Option<AudioHardwareSnapshot>,
    battery: &mut Option<BatteryInfo>,
    network: &mut Option<NetworkInfo>,
    media: &mut Option<MediaInfo>,
    brightness: &mut Option<BrightnessInfo>,
    device_ticks: &mut u8,
) {
    *audio = None;
    *audio_hardware = None;
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
            app_streams: Vec::new(),
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
        let mut audio_hardware = Some(AudioHardwareSnapshot::default());
        let mut network = Some(NetworkInfo::default());
        let mut media = Some(MediaInfo::default());
        let mut brightness = Some(BrightnessInfo::default());
        let mut device_ticks = 17;

        invalidate_device_snapshots_after_config_reload(
            &mut audio,
            &mut audio_hardware,
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
}
