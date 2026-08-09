use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceDomain {
    Audio,
    Bluetooth,
    Brightness,
    Network,
    NightLight,
    PowerProfile,
    Media,
    Battery,
    Caffeine,
}

impl DeviceDomain {
    pub const ALL: [Self; 9] = [
        Self::Audio,
        Self::Bluetooth,
        Self::Brightness,
        Self::Network,
        Self::NightLight,
        Self::PowerProfile,
        Self::Media,
        Self::Battery,
        Self::Caffeine,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DomainLifecycle {
    #[default]
    Unavailable,
    Connecting,
    Ready,
    Reconnecting,
    Degraded,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomainState {
    pub domain: DeviceDomain,
    pub revision: u64,
    pub lifecycle: DomainLifecycle,
    pub payload: DomainPayload,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "domain", content = "value", rename_all = "snake_case")]
pub enum DomainPayload {
    Audio(AudioPayload),
    Bluetooth(BluetoothPayload),
    Brightness(BrightnessPayload),
    Network(NetworkPayload),
    NightLight(NightLightPayload),
    PowerProfile(PowerProfilePayload),
    Media(MediaPayload),
    Battery(BatteryPayload),
    Caffeine(CaffeinePayload),
}

impl DomainPayload {
    pub fn empty(domain: DeviceDomain) -> Self {
        match domain {
            DeviceDomain::Audio => Self::Audio(Default::default()),
            DeviceDomain::Bluetooth => Self::Bluetooth(Default::default()),
            DeviceDomain::Brightness => Self::Brightness(Default::default()),
            DeviceDomain::Network => Self::Network(Default::default()),
            DeviceDomain::NightLight => Self::NightLight(Default::default()),
            DeviceDomain::PowerProfile => Self::PowerProfile(Default::default()),
            DeviceDomain::Media => Self::Media(Default::default()),
            DeviceDomain::Battery => Self::Battery(Default::default()),
            DeviceDomain::Caffeine => Self::Caffeine(Default::default()),
        }
    }

    pub fn as_value(&self) -> serde_json::Value {
        match self {
            Self::Audio(value) => serde_json::to_value(value),
            Self::Bluetooth(value) => serde_json::to_value(value),
            Self::Brightness(value) => serde_json::to_value(value),
            Self::Network(value) => serde_json::to_value(value),
            Self::NightLight(value) => serde_json::to_value(value),
            Self::PowerProfile(value) => serde_json::to_value(value),
            Self::Media(value) => serde_json::to_value(value),
            Self::Battery(value) => serde_json::to_value(value),
            Self::Caffeine(value) => serde_json::to_value(value),
        }
        .unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, zvariant::Type)]
pub struct AudioPortPayload {
    pub name: String,
    pub description: String,
    pub is_active: bool,
    pub available: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, zvariant::Type)]
pub struct AudioDevicePayload {
    pub index: u32,
    pub id: String,
    pub name: String,
    pub description: String,
    pub volume_percent: u8,
    pub is_muted: bool,
    pub is_default: bool,
    pub is_input: bool,
    pub channels: u8,
    pub ports: Vec<AudioPortPayload>,
    pub active_port: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, zvariant::Type)]
pub struct AudioStreamPayload {
    pub id: u32,
    pub index: u32,
    pub name: String,
    pub app_name: String,
    pub volume_percent: u8,
    pub is_muted: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, zvariant::Type)]
pub struct AudioPayload {
    pub available: bool,
    pub default_sink_name: String,
    pub default_source_name: String,
    pub volume: u8,
    pub is_muted: bool,
    pub input_volume: u8,
    pub is_input_muted: bool,
    pub sinks: Vec<AudioDevicePayload>,
    pub sources: Vec<AudioDevicePayload>,
    pub app_streams: Vec<AudioStreamPayload>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, zvariant::Type)]
pub struct BluetoothDevicePayload {
    pub address: String,
    pub name: String,
    pub icon: String,
    pub paired: bool,
    pub connected: bool,
    pub trusted: bool,
    pub blocked: bool,
    pub rssi: i16,
    pub has_rssi: bool,
    pub battery_percentage: u8,
    pub has_battery_percentage: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, zvariant::Type)]
pub struct BluetoothPayload {
    pub available: bool,
    pub powered: bool,
    pub discovering: bool,
    pub connected_devices_count: u32,
    pub connected: bool,
    pub devices: Vec<BluetoothDevicePayload>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, zvariant::Type)]
pub struct DisplayBrightnessPayload {
    pub id: String,
    pub name: String,
    pub connector: String,
    pub percentage: u8,
    pub is_primary: bool,
    pub backend: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, zvariant::Type)]
pub struct BrightnessPayload {
    pub available: bool,
    pub percentage: u8,
    pub device_name: String,
    pub displays: Vec<DisplayBrightnessPayload>,
    pub primary_display_id: String,
    pub permissions_ok: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, zvariant::Type)]
pub struct WifiAccessPointPayload {
    pub ssid: String,
    pub bssid: String,
    pub signal_percent: u8,
    pub security_type: String,
    pub frequency_mhz: u32,
    pub is_connected: bool,
    pub object_path: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, zvariant::Type)]
pub struct VpnPayload {
    pub id: String,
    pub uuid: String,
    pub vpn_type: String,
    pub is_active: bool,
    pub object_path: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, zvariant::Type)]
pub struct NetworkDevicePayload {
    pub interface: String,
    pub device_type: String,
    pub state: u32,
    pub carrier: bool,
    pub object_path: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, zvariant::Type)]
pub struct IpConfigPayload {
    pub ipv4_address: String,
    pub ipv4_gateway: String,
    pub ipv6_address: String,
    pub ipv6_gateway: String,
    pub dns_servers: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, zvariant::Type)]
pub struct NetworkPayload {
    pub available: bool,
    pub is_connected: bool,
    pub connection_type: String,
    pub ssid: String,
    pub wifi_enabled: bool,
    pub wwan_enabled: bool,
    pub airplane_mode: bool,
    pub access_points: Vec<WifiAccessPointPayload>,
    pub active_vpns: Vec<VpnPayload>,
    pub devices: Vec<NetworkDevicePayload>,
    pub state: String,
    pub ip_config: IpConfigPayload,
    pub has_ip_config: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, zvariant::Type)]
pub struct NightLightPayload {
    pub available: bool,
    pub enabled: bool,
    pub temperature: u32,
    pub backend_name: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, zvariant::Type)]
pub struct PowerProfilePayload {
    pub available: bool,
    pub profile: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, zvariant::Type)]
pub struct MediaPayload {
    pub available: bool,
    pub player_id: String,
    pub title: String,
    pub artist: String,
    pub art_url: String,
    pub playback_state: String,
    pub can_play_pause: bool,
    pub can_go_next: bool,
    pub position_secs: f64,
    pub length_secs: f64,
    pub rate: f64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, zvariant::Type)]
pub struct BatteryPayload {
    pub available: bool,
    pub percentage: u8,
    pub is_charging: bool,
    pub is_present: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, zvariant::Type)]
pub struct CaffeinePayload {
    pub available: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, zvariant::Type)]
pub struct CommandId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum CommandOutcome {
    Applied {
        command_id: CommandId,
        arrival_sequence: u64,
        domain: DeviceDomain,
        revision: u64,
    },
    Rejected {
        command_id: CommandId,
        arrival_sequence: u64,
        domain: DeviceDomain,
        reason: String,
    },
    Timeout {
        command_id: CommandId,
        arrival_sequence: u64,
        domain: DeviceDomain,
    },
    ReconciledApplied {
        command_id: CommandId,
        arrival_sequence: u64,
        domain: DeviceDomain,
        revision: u64,
    },
}

impl CommandOutcome {
    pub fn with_command(self, command_id: CommandId, arrival_sequence: u64) -> Self {
        match self {
            Self::Applied {
                domain, revision, ..
            } => Self::Applied {
                command_id,
                arrival_sequence,
                domain,
                revision,
            },
            Self::Rejected { domain, reason, .. } => Self::Rejected {
                command_id,
                arrival_sequence,
                domain,
                reason,
            },
            Self::Timeout { domain, .. } => Self::Timeout {
                command_id,
                arrival_sequence,
                domain,
            },
            Self::ReconciledApplied {
                domain, revision, ..
            } => Self::ReconciledApplied {
                command_id,
                arrival_sequence,
                domain,
                revision,
            },
        }
    }

    pub fn with_command_id(self, command_id: CommandId) -> Self {
        match self {
            Self::Applied {
                domain,
                revision,
                arrival_sequence,
                ..
            } => Self::Applied {
                command_id,
                arrival_sequence,
                domain,
                revision,
            },
            Self::Rejected {
                domain,
                reason,
                arrival_sequence,
                ..
            } => Self::Rejected {
                command_id,
                arrival_sequence,
                domain,
                reason,
            },
            Self::Timeout {
                domain,
                arrival_sequence,
                ..
            } => Self::Timeout {
                command_id,
                arrival_sequence,
                domain,
            },
            Self::ReconciledApplied {
                domain,
                revision,
                arrival_sequence,
                ..
            } => Self::ReconciledApplied {
                command_id,
                arrival_sequence,
                domain,
                revision,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, zvariant::Type)]
pub struct CommandOutcomeRecord {
    pub kind: u8,
    pub command_id: CommandId,
    pub arrival_sequence: u64,
    pub domain: u8,
    pub revision: u64,
    pub reason: String,
}

impl From<CommandOutcome> for CommandOutcomeRecord {
    fn from(outcome: CommandOutcome) -> Self {
        match outcome {
            CommandOutcome::Applied {
                command_id,
                arrival_sequence,
                domain,
                revision,
            } => Self {
                kind: 0,
                command_id,
                arrival_sequence,
                domain: domain_code(domain),
                revision,
                reason: String::new(),
            },
            CommandOutcome::Rejected {
                command_id,
                arrival_sequence,
                domain,
                reason,
            } => Self {
                kind: 1,
                command_id,
                arrival_sequence,
                domain: domain_code(domain),
                revision: 0,
                reason,
            },
            CommandOutcome::Timeout {
                command_id,
                arrival_sequence,
                domain,
            } => Self {
                kind: 2,
                command_id,
                arrival_sequence,
                domain: domain_code(domain),
                revision: 0,
                reason: String::new(),
            },
            CommandOutcome::ReconciledApplied {
                command_id,
                arrival_sequence,
                domain,
                revision,
            } => Self {
                kind: 3,
                command_id,
                arrival_sequence,
                domain: domain_code(domain),
                revision,
                reason: String::new(),
            },
        }
    }
}

impl TryFrom<CommandOutcomeRecord> for CommandOutcome {
    type Error = String;
    fn try_from(record: CommandOutcomeRecord) -> Result<Self, Self::Error> {
        let domain =
            domain_from_code(record.domain).ok_or_else(|| "invalid device domain".to_string())?;
        Ok(match record.kind {
            0 => Self::Applied {
                command_id: record.command_id,
                arrival_sequence: record.arrival_sequence,
                domain,
                revision: record.revision,
            },
            1 => Self::Rejected {
                command_id: record.command_id,
                arrival_sequence: record.arrival_sequence,
                domain,
                reason: record.reason,
            },
            2 => Self::Timeout {
                command_id: record.command_id,
                arrival_sequence: record.arrival_sequence,
                domain,
            },
            3 => Self::ReconciledApplied {
                command_id: record.command_id,
                arrival_sequence: record.arrival_sequence,
                domain,
                revision: record.revision,
            },
            _ => return Err("invalid command outcome kind".into()),
        })
    }
}

fn domain_code(domain: DeviceDomain) -> u8 {
    match domain {
        DeviceDomain::Audio => 0,
        DeviceDomain::Bluetooth => 1,
        DeviceDomain::Brightness => 2,
        DeviceDomain::Network => 3,
        DeviceDomain::NightLight => 4,
        DeviceDomain::PowerProfile => 5,
        DeviceDomain::Media => 6,
        DeviceDomain::Battery => 7,
        DeviceDomain::Caffeine => 8,
    }
}

fn domain_from_code(code: u8) -> Option<DeviceDomain> {
    Some(match code {
        0 => DeviceDomain::Audio,
        1 => DeviceDomain::Bluetooth,
        2 => DeviceDomain::Brightness,
        3 => DeviceDomain::Network,
        4 => DeviceDomain::NightLight,
        5 => DeviceDomain::PowerProfile,
        6 => DeviceDomain::Media,
        7 => DeviceDomain::Battery,
        8 => DeviceDomain::Caffeine,
        _ => return None,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "domain", content = "action")]
pub enum DeviceCommand {
    Audio(AudioAction),
    Bluetooth(BluetoothAction),
    Brightness(BrightnessAction),
    Network(NetworkAction),
    NightLight(NightLightAction),
    PowerProfile(PowerProfileAction),
    Media(MediaAction),
    Caffeine(CaffeineAction),
}

impl DeviceCommand {
    pub fn domain(&self) -> DeviceDomain {
        match self {
            Self::Audio(_) => DeviceDomain::Audio,
            Self::Bluetooth(_) => DeviceDomain::Bluetooth,
            Self::Brightness(_) => DeviceDomain::Brightness,
            Self::Network(_) => DeviceDomain::Network,
            Self::NightLight(_) => DeviceDomain::NightLight,
            Self::PowerProfile(_) => DeviceDomain::PowerProfile,
            Self::Media(_) => DeviceDomain::Media,
            Self::Caffeine(_) => DeviceDomain::Caffeine,
        }
    }

    /// Absolute set-value commands (sliders) can be coalesced in queue.
    pub fn is_coalescable(&self) -> bool {
        matches!(
            self,
            Self::Audio(AudioAction::SetVolume(_))
                | Self::Brightness(BrightnessAction::SetBrightness(_))
                | Self::NightLight(NightLightAction::SetTemperature(_))
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioAction {
    SetVolume(u8),
    SetMuted(bool),
    ToggleMute,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrightnessAction {
    SetBrightness(u8),
    StepUp,
    StepDown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BluetoothAction {
    SetPowered(bool),
    TogglePowered,
    Connect(String),
    Disconnect(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkAction {
    SetWifiEnabled(bool),
    ToggleWifi,
    ConnectWifi(String),
    ConnectVpn(String),
    DisconnectVpn(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NightLightAction {
    SetEnabled(bool),
    ToggleEnabled,
    SetTemperature(u32),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerProfileAction {
    SetProfile(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaAction {
    Play,
    Pause,
    PlayPause,
    Next,
    Previous,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaffeineAction {
    SetEnabled(bool),
    Toggle,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error(
        "Protocol version mismatch: client version {client_version} != daemon version {PROTOCOL_VERSION}"
    )]
    VersionMismatch { client_version: u32 },
}

pub fn check_protocol_version(client_version: u32) -> Result<(), ProtocolError> {
    if client_version == PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(ProtocolError::VersionMismatch { client_version })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_version_check() {
        assert!(check_protocol_version(1).is_ok());
        assert_eq!(
            check_protocol_version(2),
            Err(ProtocolError::VersionMismatch { client_version: 2 })
        );
    }

    #[test]
    fn test_coalescable_commands() {
        let set_vol = DeviceCommand::Audio(AudioAction::SetVolume(50));
        assert!(set_vol.is_coalescable());

        let toggle_mute = DeviceCommand::Audio(AudioAction::ToggleMute);
        assert!(!toggle_mute.is_coalescable());
    }
}
