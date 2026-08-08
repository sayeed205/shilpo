//! DBus-native NetworkManager service and network domain models.

pub mod dbus_client;
pub mod vpn;
pub mod wifi;

pub use vpn::VpnConnection;
pub use wifi::{WifiAccessPoint, WifiSecurity};

use anyhow::Result;
use futures_lite::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, watch};
use zbus::{Connection, MessageStream};

/// Physical or virtual network device interface description.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkDevice {
    /// Interface name (e.g., "wlan0", "eth0").
    pub interface: String,
    /// Type of device interface (e.g. "wifi", "ethernet", "bluetooth", "generic", "other").
    pub device_type: String,
    /// Numerical NetworkManager device state (e.g. 100 = activated).
    pub state: u32,
    /// Physical link carrier status (true if cable plugged in or wireless connected).
    pub carrier: bool,
    /// DBus object path for the device interface.
    pub object_path: String,
}

/// Strongly-typed network connectivity state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkState {
    #[default]
    Unknown = 0,
    Asleep = 10,
    Disconnected = 20,
    Disconnecting = 30,
    Connecting = 40,
    ConnectedLocal = 50,
    ConnectedSite = 60,
    ConnectedGlobal = 70,
}

impl From<u32> for NetworkState {
    fn from(val: u32) -> Self {
        match val {
            10 => NetworkState::Asleep,
            20 => NetworkState::Disconnected,
            30 => NetworkState::Disconnecting,
            40 => NetworkState::Connecting,
            50 => NetworkState::ConnectedLocal,
            60 => NetworkState::ConnectedSite,
            70 => NetworkState::ConnectedGlobal,
            _ => NetworkState::Unknown,
        }
    }
}

/// IP configuration details for IPv4 and IPv6 protocols.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpConfig {
    /// Active IPv4 address.
    pub ipv4_address: Option<String>,
    /// Active IPv4 default gateway.
    pub ipv4_gateway: Option<String>,
    /// Active IPv6 address.
    #[serde(default)]
    pub ipv6_address: Option<String>,
    /// Active IPv6 default gateway.
    #[serde(default)]
    pub ipv6_gateway: Option<String>,
    /// Configured DNS nameserver IP addresses.
    #[serde(default)]
    pub dns_servers: Vec<String>,
}

/// Overall network status information across Wi-Fi, Ethernet, VPN, and device interfaces.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkInfo {
    /// Whether global internet connectivity is active.
    pub is_connected: bool,
    /// Primary active connection type (e.g., "802-11-wireless", "802-3-ethernet", "none").
    pub connection_type: String,
    /// Currently connected Wi-Fi SSID if active.
    pub ssid: Option<String>,
    /// Whether the Wi-Fi radio is enabled.
    pub wifi_enabled: bool,
    /// Whether WWAN (Cellular) radio is enabled.
    #[serde(default)]
    pub wwan_enabled: bool,
    /// Whether airplane mode is currently active (true if both Wi-Fi and WWAN radios are disabled).
    pub airplane_mode: bool,
    /// Discovered Wi-Fi access points.
    pub access_points: Vec<WifiAccessPoint>,
    /// Active VPN connection profiles.
    pub active_vpns: Vec<VpnConnection>,
    /// List of physical and virtual network interface devices.
    #[serde(default)]
    pub devices: Vec<NetworkDevice>,
    /// High-level network state enum.
    #[serde(default)]
    pub state: NetworkState,
    /// Active IP configuration details.
    pub ip_config: Option<IpConfig>,
    /// Service availability flag.
    pub available: bool,
}

/// Commands for controlling network power, Wi-Fi scanning/connections, and VPN profiles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetworkCommand {
    /// Enable or disable the Wi-Fi radio.
    SetWifiEnabled(bool),
    /// Request an immediate background scan for Wi-Fi access points.
    ScanWifi,
    /// Connect to a Wi-Fi network by SSID and optional access point path.
    ConnectWifi {
        ssid: String,
        object_path: Option<String>,
    },
    /// Deactivate an active network connection by object path.
    DeactivateConnection(String),
    /// Connect to a VPN profile by profile name or UUID.
    ConnectVpn(String),
    /// Disconnect an active VPN connection by profile name, UUID, or path.
    DisconnectVpn(String),
    /// Toggle system airplane mode.
    SetAirplaneModeEnabled(bool),
}

use crate::runtime::{CommandContext, CommandRuntime, StateContext};

/// NetworkManager service providing zbus DBus control and real-time status updates.
#[derive(Clone)]
pub struct NetworkService {
    runtime: CommandRuntime<NetworkInfo, NetworkCommand>,
}

impl Default for NetworkService {
    fn default() -> Self {
        Self::new_offline()
    }
}

async fn fetch_and_publish(conn: &Connection, state: &StateContext<NetworkInfo>) {
    let nm_state = dbus_client::get_nm_state(conn).await.unwrap_or(0);
    let state_enum = NetworkState::from(nm_state);
    let is_connected = nm_state == dbus_client::NM_STATE_CONNECTED_GLOBAL;
    let wifi_enabled = dbus_client::get_wireless_enabled(conn)
        .await
        .unwrap_or(true);
    let wwan_enabled = dbus_client::get_wwan_enabled(conn).await.unwrap_or(false);
    let access_points = dbus_client::list_access_points(conn)
        .await
        .unwrap_or_default();
    let active_vpns = dbus_client::list_active_vpns(conn)
        .await
        .unwrap_or_default();
    let devices = dbus_client::list_network_devices(conn)
        .await
        .unwrap_or_default();
    let (connection_type, ip_config) = dbus_client::get_primary_connection_info(conn)
        .await
        .unwrap_or(("none".to_string(), None));

    let active_ssid = access_points
        .iter()
        .find(|ap| ap.is_connected)
        .map(|ap| ap.ssid.clone());
    let airplane_mode = !wifi_enabled && !wwan_enabled;

    let info = NetworkInfo {
        is_connected,
        connection_type,
        ssid: active_ssid,
        wifi_enabled,
        wwan_enabled,
        airplane_mode,
        access_points,
        active_vpns,
        devices,
        state: state_enum,
        ip_config,
        available: true,
    };
    state.send_replace(info);
}

async fn run_network_loop(mut ctx: CommandContext<NetworkInfo, NetworkCommand>) {
    loop {
        let connection = match Connection::system().await {
            Ok(conn) => conn,
            Err(err) => {
                tracing::debug!("NetworkManager D-Bus system connection failed: {err}; retrying");
                ctx.state.send_replace(NetworkInfo::default());
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                continue;
            }
        };

        fetch_and_publish(&connection, &ctx.state).await;

        let mut stream = MessageStream::from(&connection);

        loop {
            tokio::select! {
                cmd = ctx.command_rx.recv() => {
                    match cmd {
                        Some(cmd) => {
                            match cmd {
                                NetworkCommand::SetWifiEnabled(enabled) => {
                                    let _ = dbus_client::set_wireless_enabled(&connection, enabled).await;
                                }
                                NetworkCommand::ScanWifi => {
                                    let _ = dbus_client::request_wifi_scan(&connection).await;
                                }
                                NetworkCommand::ConnectWifi { ssid, object_path } => {
                                    let _ = dbus_client::connect_wifi_ap(&connection, &ssid, object_path.as_deref()).await;
                                }
                                NetworkCommand::DeactivateConnection(path) => {
                                    let _ = dbus_client::deactivate_connection(&connection, &path).await;
                                }
                                NetworkCommand::ConnectVpn(name_or_uuid) => {
                                    let _ = dbus_client::connect_vpn(&connection, &name_or_uuid).await;
                                }
                                NetworkCommand::DisconnectVpn(name_or_path) => {
                                    let _ = dbus_client::disconnect_vpn(&connection, &name_or_path).await;
                                }
                                NetworkCommand::SetAirplaneModeEnabled(enabled) => {
                                    let _ = dbus_client::set_wireless_enabled(&connection, !enabled).await;
                                    let _ = dbus_client::set_wwan_enabled(&connection, !enabled).await;
                                }
                            }
                            fetch_and_publish(&connection, &ctx.state).await;
                        }
                        None => return,
                    }
                }
                msg = stream.next() => {
                    if msg.is_some() {
                        fetch_and_publish(&connection, &ctx.state).await;
                    } else {
                        break;
                    }
                }
            }
        }

        ctx.state.send_replace(NetworkInfo::default());
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}

impl NetworkService {
    /// Create a dummy offline `NetworkService` instance for UI testing and headless CI.
    pub fn new_offline() -> Self {
        let runtime = CommandRuntime::new_offline(NetworkInfo::default());
        Self { runtime }
    }

    /// Instantiate a real `NetworkService` connected to system DBus with event-driven signal updates.
    pub fn new() -> Result<Self> {
        let runtime = CommandRuntime::spawn(NetworkInfo::default(), run_network_loop);
        Ok(Self { runtime })
    }

    /// Subscribe to real-time `NetworkInfo` updates via watch channel.
    pub fn subscribe(&self) -> watch::Receiver<NetworkInfo> {
        self.runtime.subscribe()
    }

    /// Retrieve the current snapshot of `NetworkInfo`.
    pub fn network_info(&self) -> NetworkInfo {
        self.runtime.get()
    }

    /// Enable or disable Wi-Fi radio.
    pub fn set_wifi_enabled(&self, enabled: bool) -> Result<()> {
        if self.runtime.send_command(NetworkCommand::SetWifiEnabled(enabled)) {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Failed to send command to NetworkService"))
        }
    }

    /// Request a Wi-Fi background scan.
    pub fn scan_wifi(&self) -> Result<()> {
        if self.runtime.send_command(NetworkCommand::ScanWifi) {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Failed to send command to NetworkService"))
        }
    }

    /// Connect to a Wi-Fi network by SSID and optional access point path.
    pub fn connect_wifi(&self, ssid: &str, object_path: Option<&str>) -> Result<()> {
        let cmd = NetworkCommand::ConnectWifi {
            ssid: ssid.to_string(),
            object_path: object_path.map(|s| s.to_string()),
        };
        if self.runtime.send_command(cmd) {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Failed to send command to NetworkService"))
        }
    }

    /// Deactivate an active network connection.
    pub fn deactivate_connection(&self, active_conn_path: &str) -> Result<()> {
        let path = active_conn_path.to_string();
        if self.runtime.send_command(NetworkCommand::DeactivateConnection(path)) {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Failed to send command to NetworkService"))
        }
    }

    /// Connect to a VPN connection profile by name or UUID.
    pub fn connect_vpn(&self, name_or_uuid: &str) -> Result<()> {
        let name = name_or_uuid.to_string();
        if self.runtime.send_command(NetworkCommand::ConnectVpn(name)) {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Failed to send command to NetworkService"))
        }
    }

    /// Disconnect an active VPN connection by profile name or path.
    pub fn disconnect_vpn(&self, name_or_path: &str) -> Result<()> {
        let name = name_or_path.to_string();
        if self.runtime.send_command(NetworkCommand::DisconnectVpn(name)) {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Failed to send command to NetworkService"))
        }
    }

    /// Enable or disable airplane mode (disabling airplane mode restores Wi-Fi and WWAN radio status).
    pub fn set_airplane_mode_enabled(&self, enabled: bool) -> Result<()> {
        if self.runtime.send_command(NetworkCommand::SetAirplaneModeEnabled(enabled)) {
            self.runtime.update(|info| {
                info.airplane_mode = enabled;
                info.wifi_enabled = !enabled;
                info.wwan_enabled = !enabled;
            });
            Ok(())
        } else {
            self.runtime.update(|info| {
                info.airplane_mode = enabled;
                info.wifi_enabled = !enabled;
                info.wwan_enabled = !enabled;
            });
            Ok(())
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_unavailable() {
        assert_eq!(
            NetworkInfo::default(),
            NetworkInfo {
                is_connected: false,
                connection_type: String::new(),
                ssid: None,
                wifi_enabled: false,
                wwan_enabled: false,
                airplane_mode: false,
                access_points: Vec::new(),
                active_vpns: Vec::new(),
                devices: Vec::new(),
                state: NetworkState::Unknown,
                ip_config: None,
                available: false,
            }
        );
    }

    #[test]
    fn test_vpn_connection_struct() {
        let vpn = VpnConnection {
            id: "Corporate-VPN".to_string(),
            uuid: "12345-abcde".to_string(),
            vpn_type: "wireguard".to_string(),
            is_active: true,
            object_path: "/org/freedesktop/NetworkManager/ActiveConnection/1".to_string(),
        };
        assert_eq!(vpn.id, "Corporate-VPN");
        assert_eq!(vpn.uuid, "12345-abcde");
        assert!(vpn.is_active);
    }

    #[test]
    fn test_wifi_access_point_struct() {
        let ap = WifiAccessPoint {
            ssid: "Home-WiFi".to_string(),
            bssid: "00:11:22:33:44:55".to_string(),
            signal_percent: 85,
            security_type: "WPA2/WPA3".to_string(),
            frequency_mhz: 5240,
            is_connected: true,
            object_path: "/org/freedesktop/NetworkManager/AccessPoint/10".to_string(),
        };
        assert_eq!(ap.ssid, "Home-WiFi");
        assert!(ap.is_secure());
        assert!(!ap.is_enterprise());

        let ent_ap = WifiAccessPoint {
            security_type: "WPA2/WPA3-Enterprise".to_string(),
            ..ap
        };
        assert!(ent_ap.is_enterprise());
    }

    #[test]
    fn test_network_state_conversion() {
        assert_eq!(NetworkState::from(70), NetworkState::ConnectedGlobal);
        assert_eq!(NetworkState::from(20), NetworkState::Disconnected);
        assert_eq!(NetworkState::from(40), NetworkState::Connecting);
        assert_eq!(NetworkState::from(999), NetworkState::Unknown);
    }

    #[test]
    fn test_serde_json_serialization_roundtrip() {
        let info = NetworkInfo {
            is_connected: true,
            connection_type: "802-11-wireless".to_string(),
            ssid: Some("Office-5G".to_string()),
            wifi_enabled: true,
            wwan_enabled: false,
            airplane_mode: false,
            access_points: vec![WifiAccessPoint {
                ssid: "Office-5G".to_string(),
                bssid: "11:22:33:44:55:66".to_string(),
                signal_percent: 92,
                security_type: "WPA2/WPA3".to_string(),
                frequency_mhz: 5180,
                is_connected: true,
                object_path: "/ap/1".to_string(),
            }],
            active_vpns: vec![VpnConnection {
                id: "Work-VPN".to_string(),
                uuid: "vpn-uuid-1".to_string(),
                vpn_type: "wireguard".to_string(),
                is_active: true,
                object_path: "/vpn/1".to_string(),
            }],
            devices: vec![NetworkDevice {
                interface: "wlan0".to_string(),
                device_type: "wifi".to_string(),
                state: 100,
                carrier: true,
                object_path: "/dev/1".to_string(),
            }],
            state: NetworkState::ConnectedGlobal,
            ip_config: Some(IpConfig {
                ipv4_address: Some("192.168.1.150".to_string()),
                ipv4_gateway: Some("192.168.1.1".to_string()),
                ipv6_address: Some("fe80::1".to_string()),
                ipv6_gateway: Some("fe80::gateway".to_string()),
                dns_servers: vec!["1.1.1.1".to_string(), "8.8.8.8".to_string()],
            }),
            available: true,
        };

        let json = serde_json::to_string(&info).expect("Failed to serialize NetworkInfo");
        let deserialized: NetworkInfo =
            serde_json::from_str(&json).expect("Failed to deserialize NetworkInfo");
        assert_eq!(info, deserialized);
    }

    #[tokio::test]
    async fn test_vpn_connection_selection_and_status() {
        let service = NetworkService::new_offline();
        let vpn = VpnConnection {
            id: "Corporate VPN".to_string(),
            uuid: "corp-vpn-uuid".to_string(),
            vpn_type: "wireguard".to_string(),
            is_active: true,
            object_path: "/org/freedesktop/NetworkManager/ActiveConnection/1".to_string(),
        };

        let mut info = NetworkInfo::default();
        info.active_vpns.push(vpn.clone());
        assert_eq!(info.active_vpns.len(), 1);
        assert_eq!(info.active_vpns[0].id, "Corporate VPN");

        assert!(service.connect_vpn("Corporate VPN").is_ok());
        assert!(service.disconnect_vpn("Corporate VPN").is_ok());
    }

    #[test]
    fn test_network_commands_enqueued() {
        let (command_tx, mut command_rx) = mpsc::channel::<NetworkCommand>(32);
        let (watch_tx, _) = watch::channel(NetworkInfo::default());
        let service = NetworkService {
            tx: watch_tx,
            _task: None,
            command_tx: Some(command_tx),
        };

        service.set_wifi_enabled(true).unwrap();
        assert_eq!(
            command_rx.try_recv().unwrap(),
            NetworkCommand::SetWifiEnabled(true)
        );

        service.scan_wifi().unwrap();
        assert_eq!(command_rx.try_recv().unwrap(), NetworkCommand::ScanWifi);

        service.connect_wifi("Home-WiFi", None).unwrap();
        assert_eq!(
            command_rx.try_recv().unwrap(),
            NetworkCommand::ConnectWifi {
                ssid: "Home-WiFi".to_string(),
                object_path: None,
            }
        );

        service.deactivate_connection("/path").unwrap();
        assert_eq!(
            command_rx.try_recv().unwrap(),
            NetworkCommand::DeactivateConnection("/path".to_string())
        );

        service.connect_vpn("VPN").unwrap();
        assert_eq!(
            command_rx.try_recv().unwrap(),
            NetworkCommand::ConnectVpn("VPN".to_string())
        );

        service.disconnect_vpn("VPN").unwrap();
        assert_eq!(
            command_rx.try_recv().unwrap(),
            NetworkCommand::DisconnectVpn("VPN".to_string())
        );

        service.set_airplane_mode_enabled(true).unwrap();
        assert_eq!(
            command_rx.try_recv().unwrap(),
            NetworkCommand::SetAirplaneModeEnabled(true)
        );

        let info = service.network_info();
        assert!(info.airplane_mode);
        assert!(!info.wifi_enabled);

        service.set_airplane_mode_enabled(false).unwrap();
        let info_off = service.network_info();
        assert!(!info_off.airplane_mode);
        assert!(info_off.wifi_enabled);
    }
}
