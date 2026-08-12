use crate::config::ShellConfig;
use gpui::BackgroundExecutor;
use shilpo_services::DeviceClient;
pub use shilpo_services::DeviceCommand;
use shilpo_services::{self};
use shilpo_services::{AudioInfo, BatteryInfo, BrightnessInfo, MediaInfo, NetworkInfo};
use std::{path::PathBuf, sync::mpsc, time::Duration};

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
            WorkerUpdate::Battery(info) if !info.available && self.battery.is_present => {
                if self.battery.available {
                    self.battery.available = false;
                    true
                } else {
                    false
                }
            }
            WorkerUpdate::Battery(info) => replace_if_changed(&mut self.battery, info),
            WorkerUpdate::Audio(info) => replace_if_changed(&mut self.audio, info),
            WorkerUpdate::Network(info) => replace_if_changed(&mut self.network, info),
            WorkerUpdate::Media(info) => replace_if_changed(&mut self.media, info),
            WorkerUpdate::Brightness(info) => replace_if_changed(&mut self.brightness, info),
            WorkerUpdate::Config(_)
            | WorkerUpdate::CommandRejected { .. }
            | WorkerUpdate::CommandOutcome(_) => false,
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

fn replace_if_changed<T: PartialEq + Clone>(target: &mut T, value: &T) -> bool {
    if target == value {
        false
    } else {
        *target = value.clone();
        true
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
    CommandOutcome(shilpo_services::device_protocol::CommandOutcome),
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
    client: DeviceClient,
) -> gpui::Task<()> {
    let reconnect_client = client.clone();
    tokio::spawn(async move { reconnect_client.maintain_connection().await });
    executor
        .clone()
        .spawn(async move { run(executor.clone(), updates, commands, config_path, client).await })
}

async fn run(
    executor: BackgroundExecutor,
    updates: UpdateSender,
    commands: CommandReceiver,
    config_path: PathBuf,
    client: DeviceClient,
) {
    let resolver = crate::config::ConfigResolver::from_primary_path(&config_path);
    let mut committed_snapshot = match resolver.resolve_initial() {
        Ok((snapshot, _)) => {
            let _ = updates.try_send(WorkerUpdate::Config(ConfigUpdate::Loaded(Box::new(
                snapshot.config.clone(),
            ))));
            snapshot
        }
        Err(e) => {
            let _ = updates.try_send(WorkerUpdate::Config(ConfigUpdate::Failed(e.to_string())));
            crate::config::ConfigSnapshot::default()
        }
    };
    let mut versions = std::collections::HashMap::new();
    loop {
        while let Ok(command) = commands.try_recv() {
            match command {
                WorkerCommand::ReloadConfig => {
                    let (new_snapshot, _changeset, report) =
                        resolver.resolve_reload(&committed_snapshot);
                    if report.recovery_scope == Some(crate::config::RecoveryScope::RejectCandidate)
                    {
                        let msg = report
                            .diagnostics
                            .iter()
                            .map(|d| format!("{}: {}", d.path, d.message))
                            .collect::<Vec<_>>()
                            .join("; ");
                        let _ = updates.try_send(WorkerUpdate::Config(ConfigUpdate::Failed(msg)));
                    } else {
                        committed_snapshot = new_snapshot;
                        let _ = updates.try_send(WorkerUpdate::Config(ConfigUpdate::Loaded(
                            Box::new(committed_snapshot.config.clone()),
                        )));
                    }
                }
                WorkerCommand::Device(command) => {
                    let client = client.clone();
                    let updates = updates.clone();
                    tokio::spawn(async move {
                        match client.send_command(command.clone()).await {
                            Ok(outcome) => {
                                let _ = updates.try_send(WorkerUpdate::CommandOutcome(outcome));
                            }
                            Err(reason) => {
                                let _ = updates
                                    .try_send(WorkerUpdate::CommandRejected { command, reason });
                            }
                        }
                    });
                }
            }
        }
        emit_client_updates(&updates, &client, &mut versions);
        executor.timer(Duration::from_millis(100)).await;
    }
}

fn emit_client_updates(
    updates: &UpdateSender,
    client: &DeviceClient,
    versions: &mut std::collections::HashMap<
        shilpo_services::DeviceDomain,
        shilpo_device::DomainVersion,
    >,
) {
    use shilpo_services::DeviceDomain;
    for domain in [
        DeviceDomain::Battery,
        DeviceDomain::Audio,
        DeviceDomain::Network,
        DeviceDomain::Media,
        DeviceDomain::Brightness,
    ] {
        let state = client.get_domain_state(domain);
        if versions.get(&domain) == Some(&state.version) {
            continue;
        }
        versions.insert(domain, state.version);
        let update = match state.payload {
            shilpo_services::DomainPayload::Battery(payload) => {
                Some(WorkerUpdate::Battery(payload))
            }
            shilpo_services::DomainPayload::Audio(payload) => {
                Some(WorkerUpdate::Audio(audio_info(payload)))
            }
            shilpo_services::DomainPayload::Network(payload) => {
                Some(WorkerUpdate::Network(network_info(payload)))
            }
            shilpo_services::DomainPayload::Media(payload) => {
                Some(WorkerUpdate::Media(media_info(payload)))
            }
            shilpo_services::DomainPayload::Brightness(payload) => {
                Some(WorkerUpdate::Brightness(brightness_info(payload)))
            }
            _ => None,
        };
        if let Some(update) = update {
            let _ = updates.try_send(update);
        }
    }
}

fn optional(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn audio_info(payload: shilpo_services::AudioPayload) -> AudioInfo {
    fn device(v: shilpo_services::AudioDevicePayload) -> shilpo_services::AudioDevice {
        shilpo_services::AudioDevice {
            index: v.index,
            id: v.id,
            name: v.name,
            description: v.description,
            volume_percent: v.volume_percent,
            is_muted: v.is_muted,
            is_default: v.is_default,
            is_input: v.is_input,
            channels: v.channels,
            active_port: optional(v.active_port),
            ports: v
                .ports
                .into_iter()
                .map(|p| shilpo_services::AudioPort {
                    name: p.name,
                    description: p.description,
                    is_active: p.is_active,
                    available: p.available,
                })
                .collect(),
        }
    }
    AudioInfo {
        available: payload.available,
        default_sink_name: payload.default_sink_name,
        default_source_name: payload.default_source_name,
        volume: payload.volume,
        is_muted: payload.is_muted,
        input_volume: payload.input_volume,
        is_input_muted: payload.is_input_muted,
        sinks: payload.sinks.into_iter().map(device).collect(),
        sources: payload.sources.into_iter().map(device).collect(),
        app_streams: payload
            .app_streams
            .into_iter()
            .map(|s| shilpo_services::AudioStream {
                id: s.id,
                index: s.index,
                name: s.name,
                app_name: s.app_name,
                volume_percent: s.volume_percent,
                is_muted: s.is_muted,
            })
            .collect(),
    }
}

fn media_info(p: shilpo_services::MediaPayload) -> MediaInfo {
    let playback_state = match p.playback_state.as_str() {
        "playing" => shilpo_services::PlaybackState::Playing,
        "paused" => shilpo_services::PlaybackState::Paused,
        _ => shilpo_services::PlaybackState::Stopped,
    };
    MediaInfo {
        player_id: p.player_id,
        title: p.title,
        artist: p.artist,
        art_url: p.art_url,
        playback_state,
        can_play_pause: p.can_play_pause,
        can_go_next: p.can_go_next,
        position_secs: p.position_secs,
        length_secs: p.length_secs,
        rate: p.rate,
        observed_at: None,
    }
}

fn brightness_info(p: shilpo_services::BrightnessPayload) -> BrightnessInfo {
    BrightnessInfo {
        percentage: p.percentage,
        available: p.available,
        device_name: optional(p.device_name),
        displays: p
            .displays
            .into_iter()
            .map(|d| shilpo_services::brightness::DisplayBrightnessInfo {
                id: d.id,
                name: d.name,
                connector: optional(d.connector),
                percentage: d.percentage,
                is_primary: d.is_primary,
                backend: serde_json::from_str(&d.backend).unwrap_or_else(|_| {
                    shilpo_services::brightness::BrightnessBackend::DdcciSysfs {
                        sysfs_path: PathBuf::new(),
                    }
                }),
            })
            .collect(),
        primary_display_id: optional(p.primary_display_id),
        permissions_ok: p.permissions_ok,
    }
}

fn network_info(p: shilpo_services::NetworkPayload) -> NetworkInfo {
    use shilpo_services::network::{IpConfig, NetworkDevice, NetworkState, WifiAccessPoint};
    let state = match p.state.as_str() {
        "Asleep" | "asleep" => NetworkState::Asleep,
        "Disconnected" | "disconnected" => NetworkState::Disconnected,
        "Disconnecting" | "disconnecting" => NetworkState::Disconnecting,
        "Connecting" | "connecting" => NetworkState::Connecting,
        "ConnectedLocal" | "connected_local" => NetworkState::ConnectedLocal,
        "ConnectedSite" | "connected_site" => NetworkState::ConnectedSite,
        "ConnectedGlobal" | "connected_global" => NetworkState::ConnectedGlobal,
        _ => NetworkState::Unknown,
    };
    NetworkInfo {
        available: p.available,
        is_connected: p.is_connected,
        connection_type: p.connection_type,
        ssid: optional(p.ssid),
        wifi_enabled: p.wifi_enabled,
        wwan_enabled: p.wwan_enabled,
        airplane_mode: p.airplane_mode,
        state,
        access_points: p
            .access_points
            .into_iter()
            .map(|a| WifiAccessPoint {
                ssid: a.ssid,
                bssid: a.bssid,
                signal_percent: a.signal_percent,
                security_type: a.security_type,
                frequency_mhz: a.frequency_mhz,
                is_connected: a.is_connected,
                object_path: a.object_path,
            })
            .collect(),
        active_vpns: p
            .active_vpns
            .into_iter()
            .map(|v| shilpo_services::VpnConnection {
                id: v.id,
                uuid: v.uuid,
                vpn_type: v.vpn_type,
                is_active: v.is_active,
                object_path: v.object_path,
            })
            .collect(),
        devices: p
            .devices
            .into_iter()
            .map(|d| NetworkDevice {
                interface: d.interface,
                device_type: d.device_type,
                state: d.state,
                carrier: d.carrier,
                object_path: d.object_path,
            })
            .collect(),
        ip_config: p.has_ip_config.then(|| IpConfig {
            ipv4_address: optional(p.ip_config.ipv4_address),
            ipv4_gateway: optional(p.ip_config.ipv4_gateway),
            ipv6_address: optional(p.ip_config.ipv6_address),
            ipv6_gateway: optional(p.ip_config.ipv6_gateway),
            dns_servers: p.ip_config.dns_servers,
        }),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn device_snapshot_applies_updates_correctly() {
        let mut snapshot = DeviceSnapshot::default();
        let battery = BatteryInfo {
            percentage: 85,
            state: shilpo_services::BatteryChargeState::Charging,
            is_present: true,
            available: true,
            ..Default::default()
        };
        assert!(snapshot.apply(&WorkerUpdate::Battery(battery.clone())));
        assert_eq!(snapshot.battery, battery);
        assert!(!snapshot.apply(&WorkerUpdate::Battery(battery)));
    }

    #[test]
    fn battery_snapshot_keeps_last_known_data_when_service_becomes_unavailable() {
        let mut snapshot = DeviceSnapshot::default();
        snapshot.apply(&WorkerUpdate::Battery(BatteryInfo {
            available: true,
            is_present: true,
            percentage: 73,
            state: shilpo_services::BatteryChargeState::Discharging,
            ..Default::default()
        }));

        assert!(snapshot.apply(&WorkerUpdate::Battery(BatteryInfo::default())));
        assert_eq!(snapshot.battery.percentage, 73);
        assert!(snapshot.battery.is_present);
        assert!(!snapshot.battery.available);
    }
    #[test]
    fn backoff_caps_at_thirty_seconds() {
        assert_eq!(backoff_delay(99), Duration::from_secs(30));
    }
}
