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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, zvariant::Type)]
#[serde(rename_all = "snake_case")]
pub enum BatteryChargeState {
    #[default]
    Unknown,
    Charging,
    Discharging,
    Empty,
    FullyCharged,
    PendingCharge,
    PendingDischarge,
}

impl BatteryChargeState {
    pub fn is_charging(&self) -> bool {
        matches!(self, Self::Charging | Self::PendingCharge)
    }

    pub fn from_u32(val: u32) -> Self {
        match val {
            1 => Self::Charging,
            2 => Self::Discharging,
            3 => Self::Empty,
            4 => Self::FullyCharged,
            5 => Self::PendingCharge,
            6 => Self::PendingDischarge,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, zvariant::Type)]
#[serde(rename_all = "snake_case")]
pub enum BatteryTechnology {
    #[default]
    Unknown,
    LithiumIon,
    LithiumPolymer,
    LithiumIronPhosphate,
    LeadAcid,
    NickelCadmium,
    NickelMetalHydride,
}

impl BatteryTechnology {
    pub fn from_u32(val: u32) -> Self {
        match val {
            1 => Self::LithiumIon,
            2 => Self::LithiumPolymer,
            3 => Self::LithiumIronPhosphate,
            4 => Self::LeadAcid,
            5 => Self::NickelCadmium,
            6 => Self::NickelMetalHydride,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, zvariant::Type)]
#[serde(rename_all = "snake_case")]
pub enum BatteryWarningLevel {
    #[default]
    Unknown,
    None,
    Discharging,
    Low,
    Critical,
    Action,
}

impl BatteryWarningLevel {
    pub fn from_u32(val: u32) -> Self {
        match val {
            1 => Self::None,
            2 => Self::Discharging,
            3 => Self::Low,
            4 => Self::Critical,
            5 => Self::Action,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, zvariant::Type)]
#[serde(rename_all = "snake_case")]
pub enum BatteryCoarseLevel {
    #[default]
    Unknown,
    None,
    Low,
    CriticallyLow,
    Action,
    Normal,
    High,
    Full,
}

impl BatteryCoarseLevel {
    pub fn from_u32(val: u32) -> Self {
        match val {
            1 => Self::None,
            3 => Self::Low,
            4 => Self::CriticallyLow,
            5 => Self::Action,
            6 => Self::Normal,
            7 => Self::High,
            8 => Self::Full,
            _ => Self::Unknown,
        }
    }
}

/// DBus-safe optional floating-point battery metric.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, zvariant::Type)]
pub struct OptionalF64 {
    present: bool,
    value: f64,
}

impl OptionalF64 {
    pub const fn none() -> Self {
        Self {
            present: false,
            value: 0.0,
        }
    }

    pub const fn some(value: f64) -> Self {
        Self {
            present: true,
            value,
        }
    }

    pub const fn get(self) -> Option<f64> {
        if self.present { Some(self.value) } else { None }
    }
}

/// DBus-safe optional unsigned integer battery metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, zvariant::Type)]
pub struct OptionalU64 {
    present: bool,
    value: u64,
}

impl OptionalU64 {
    pub const fn none() -> Self {
        Self {
            present: false,
            value: 0,
        }
    }

    pub const fn some(value: u64) -> Self {
        Self {
            present: true,
            value,
        }
    }

    pub const fn get(self) -> Option<u64> {
        if self.present { Some(self.value) } else { None }
    }
}

/// DBus-safe optional boolean battery property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, zvariant::Type)]
pub struct OptionalBool {
    present: bool,
    value: bool,
}

impl OptionalBool {
    pub const fn none() -> Self {
        Self {
            present: false,
            value: false,
        }
    }

    pub const fn some(value: bool) -> Self {
        Self {
            present: true,
            value,
        }
    }

    pub const fn get(self) -> Option<bool> {
        if self.present { Some(self.value) } else { None }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, zvariant::Type)]
pub struct BatteryDevicePayload {
    /// Stable UPower object path; used for selection identity, never diagnostics.
    pub id: String,
    pub vendor: String,
    pub model: String,
    pub serial: String,
    pub native_path: String,
    pub is_present: bool,
    pub power_supply: bool,
    pub percentage: OptionalF64,
    pub technology: BatteryTechnology,
    pub charge_state: BatteryChargeState,
    pub time_to_empty_secs: OptionalU64,
    pub time_to_full_secs: OptionalU64,
    pub energy_wh: OptionalF64,
    pub energy_empty_wh: OptionalF64,
    pub energy_full_wh: OptionalF64,
    pub energy_full_design_wh: OptionalF64,
    pub energy_rate_w: OptionalF64,
    pub capacity_percent: OptionalF64,
    pub voltage_v: OptionalF64,
    pub voltage_min_design_v: OptionalF64,
    pub voltage_max_design_v: OptionalF64,
    pub temperature_c: OptionalF64,
    pub cycle_count: OptionalU64,
    pub update_time: OptionalU64,
    pub is_rechargeable: OptionalBool,
    pub warning_level: BatteryWarningLevel,
    pub coarse_level: BatteryCoarseLevel,
    pub has_history: bool,
    pub has_statistics: bool,
    pub charge_start_threshold: OptionalU64,
    pub charge_end_threshold: OptionalU64,
    pub charge_threshold_supported: OptionalBool,
    pub charge_threshold_enabled: OptionalBool,
    /// UPower capability bitmask: start=1, end=2, firmware-managed=4.
    pub charge_threshold_settings_supported: OptionalU64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, zvariant::Type)]
pub struct BatteryPayload {
    pub available: bool,
    pub is_present: bool,
    pub percentage: u8,
    pub state: BatteryChargeState,
    pub time_to_full_secs: OptionalU64,
    pub time_to_empty_secs: OptionalU64,
    pub energy_wh: OptionalF64,
    pub energy_empty_wh: OptionalF64,
    pub energy_full_wh: OptionalF64,
    pub energy_full_design_wh: OptionalF64,
    pub energy_rate_w: OptionalF64,
    pub capacity_percent: OptionalF64,
    pub voltage_v: OptionalF64,
    pub temperature_c: OptionalF64,
    pub warning_level: BatteryWarningLevel,
    pub coarse_level: BatteryCoarseLevel,
    pub update_time: OptionalU64,
    pub devices: Vec<BatteryDevicePayload>,
}

impl BatteryPayload {
    pub fn is_charging(&self) -> bool {
        self.state.is_charging()
    }

    pub fn is_low_battery(&self) -> bool {
        self.is_present && !self.is_charging() && self.percentage < 15
    }

    pub fn low_power_mode(&self) -> bool {
        self.is_present && !self.is_charging() && self.percentage < 20
    }
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

    #[test]
    fn test_battery_charge_state_conversions() {
        assert_eq!(BatteryChargeState::from_u32(0), BatteryChargeState::Unknown);
        assert_eq!(
            BatteryChargeState::from_u32(1),
            BatteryChargeState::Charging
        );
        assert_eq!(
            BatteryChargeState::from_u32(2),
            BatteryChargeState::Discharging
        );
        assert_eq!(BatteryChargeState::from_u32(3), BatteryChargeState::Empty);
        assert_eq!(
            BatteryChargeState::from_u32(4),
            BatteryChargeState::FullyCharged
        );
        assert_eq!(
            BatteryChargeState::from_u32(5),
            BatteryChargeState::PendingCharge
        );
        assert_eq!(
            BatteryChargeState::from_u32(6),
            BatteryChargeState::PendingDischarge
        );
        assert_eq!(
            BatteryChargeState::from_u32(99),
            BatteryChargeState::Unknown
        );

        assert!(BatteryChargeState::Charging.is_charging());
        assert!(BatteryChargeState::PendingCharge.is_charging());
        assert!(!BatteryChargeState::Discharging.is_charging());
    }

    #[test]
    fn test_battery_technology_conversions() {
        assert_eq!(BatteryTechnology::from_u32(0), BatteryTechnology::Unknown);
        assert_eq!(
            BatteryTechnology::from_u32(1),
            BatteryTechnology::LithiumIon
        );
        assert_eq!(
            BatteryTechnology::from_u32(2),
            BatteryTechnology::LithiumPolymer
        );
        assert_eq!(
            BatteryTechnology::from_u32(3),
            BatteryTechnology::LithiumIronPhosphate
        );
        assert_eq!(BatteryTechnology::from_u32(4), BatteryTechnology::LeadAcid);
        assert_eq!(
            BatteryTechnology::from_u32(5),
            BatteryTechnology::NickelCadmium
        );
        assert_eq!(
            BatteryTechnology::from_u32(6),
            BatteryTechnology::NickelMetalHydride
        );
        assert_eq!(BatteryTechnology::from_u32(999), BatteryTechnology::Unknown);
    }

    #[test]
    fn test_battery_warning_level_conversions() {
        assert_eq!(
            BatteryWarningLevel::from_u32(0),
            BatteryWarningLevel::Unknown
        );
        assert_eq!(BatteryWarningLevel::from_u32(1), BatteryWarningLevel::None);
        assert_eq!(
            BatteryWarningLevel::from_u32(2),
            BatteryWarningLevel::Discharging
        );
        assert_eq!(BatteryWarningLevel::from_u32(3), BatteryWarningLevel::Low);
        assert_eq!(
            BatteryWarningLevel::from_u32(4),
            BatteryWarningLevel::Critical
        );
        assert_eq!(
            BatteryWarningLevel::from_u32(5),
            BatteryWarningLevel::Action
        );
        assert_eq!(
            BatteryWarningLevel::from_u32(100),
            BatteryWarningLevel::Unknown
        );
    }

    #[test]
    fn test_battery_coarse_level_conversions() {
        assert_eq!(BatteryCoarseLevel::from_u32(0), BatteryCoarseLevel::Unknown);
        assert_eq!(BatteryCoarseLevel::from_u32(1), BatteryCoarseLevel::None);
        assert_eq!(BatteryCoarseLevel::from_u32(2), BatteryCoarseLevel::Unknown);
        assert_eq!(BatteryCoarseLevel::from_u32(3), BatteryCoarseLevel::Low);
        assert_eq!(
            BatteryCoarseLevel::from_u32(4),
            BatteryCoarseLevel::CriticallyLow
        );
        assert_eq!(BatteryCoarseLevel::from_u32(5), BatteryCoarseLevel::Action);
        assert_eq!(BatteryCoarseLevel::from_u32(6), BatteryCoarseLevel::Normal);
        assert_eq!(BatteryCoarseLevel::from_u32(7), BatteryCoarseLevel::High);
        assert_eq!(BatteryCoarseLevel::from_u32(8), BatteryCoarseLevel::Full);
        assert_eq!(
            BatteryCoarseLevel::from_u32(42),
            BatteryCoarseLevel::Unknown
        );
    }

    #[test]
    fn test_battery_payload_roundtrip() {
        let device = BatteryDevicePayload {
            id: "/org/freedesktop/UPower/devices/battery_BAT0".to_string(),
            vendor: "LGC".to_string(),
            model: "L19M4P72".to_string(),
            serial: "1234".to_string(),
            native_path: "/sys/class/power_supply/BAT0".to_string(),
            technology: BatteryTechnology::LithiumIon,
            charge_state: BatteryChargeState::Discharging,
            energy_wh: OptionalF64::some(45.2),
            capacity_percent: OptionalF64::some(94.5),
            ..Default::default()
        };
        let payload = BatteryPayload {
            available: true,
            is_present: true,
            percentage: 82,
            state: BatteryChargeState::Discharging,
            time_to_empty_secs: OptionalU64::some(14400),
            devices: vec![device],
            ..Default::default()
        };

        let json = serde_json::to_string(&payload).expect("serialize JSON");
        let deserialized: BatteryPayload = serde_json::from_str(&json).expect("deserialize JSON");
        assert_eq!(payload, deserialized);
        assert!(deserialized.is_present);
        assert!(!deserialized.is_charging());
    }

    #[test]
    fn optional_battery_metrics_preserve_zero_and_absence_distinctly() {
        assert_eq!(OptionalF64::some(0.0).get(), Some(0.0));
        assert_eq!(OptionalF64::none().get(), None);
        assert_eq!(OptionalU64::some(0).get(), Some(0));
        assert_eq!(OptionalU64::none().get(), None);
        assert_eq!(OptionalBool::some(false).get(), Some(false));
        assert_eq!(OptionalBool::none().get(), None);
    }
}
