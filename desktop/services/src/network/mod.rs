pub mod dbus_client;
pub mod vpn;
pub mod wifi;

pub use vpn::VpnConnection;
pub use wifi::WifiAccessPoint;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, watch};
use zbus::Connection;

/// IP configuration details.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpConfig {
    pub ipv4_address: Option<String>,
    pub ipv4_gateway: Option<String>,
    pub dns_servers: Vec<String>,
}

/// Overall network status information.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkInfo {
    pub is_connected: bool,
    pub connection_type: String,
    pub ssid: Option<String>,
    pub wifi_enabled: bool,
    pub airplane_mode: bool,
    pub access_points: Vec<WifiAccessPoint>,
    pub active_vpns: Vec<VpnConnection>,
    pub ip_config: Option<IpConfig>,
    pub available: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetworkCommand {
    SetWifiEnabled(bool),
    ScanWifi,
    ConnectWifi {
        ssid: String,
        object_path: Option<String>,
    },
    DeactivateConnection(String),
    ConnectVpn(String),
    DisconnectVpn(String),
    SetAirplaneModeEnabled(bool),
}

/// NetworkManager service providing zbus DBus control and real-time status updates.
pub struct NetworkService {
    tx: watch::Sender<NetworkInfo>,
    _task: Option<tokio::task::JoinHandle<()>>,
    command_tx: Option<mpsc::Sender<NetworkCommand>>,
}

impl Drop for NetworkService {
    fn drop(&mut self) {
        if let Some(task) = self._task.take() {
            task.abort();
        }
    }
}

impl NetworkService {
    pub fn new_offline() -> Self {
        let (tx, _) = watch::channel(NetworkInfo::default());
        Self {
            tx,
            _task: None,
            command_tx: None,
        }
    }

    pub fn new() -> Result<Self> {
        let (tx, _) = watch::channel(NetworkInfo::default());
        let (command_tx, mut command_rx) = mpsc::channel::<NetworkCommand>(32);

        let tx_clone = tx.clone();
        let task = tokio::spawn(async move {
            let connection_opt = Connection::system().await.ok();

            loop {
                tokio::select! {
                    cmd = command_rx.recv() => {
                        match cmd {
                            Some(cmd) => {
                                if let Some(ref connection) = connection_opt {
                                    match cmd {
                                        NetworkCommand::SetWifiEnabled(enabled) => {
                                            let _ = dbus_client::set_wireless_enabled(connection, enabled).await;
                                        }
                                        NetworkCommand::ScanWifi => {
                                            let _ = dbus_client::request_wifi_scan(connection).await;
                                        }
                                        NetworkCommand::ConnectWifi { ssid, object_path } => {
                                            if let Some(path) = object_path {
                                                let _ = connection
                                                    .call_method(
                                                        Some("org.freedesktop.NetworkManager"),
                                                        "/org/freedesktop/NetworkManager",
                                                        Some("org.freedesktop.NetworkManager"),
                                                        "ActivateConnection",
                                                        &(
                                                            zbus::zvariant::ObjectPath::try_from(path).unwrap_or_default(),
                                                            zbus::zvariant::ObjectPath::try_from("/").unwrap_or_default(),
                                                            zbus::zvariant::ObjectPath::try_from("/").unwrap_or_default(),
                                                        ),
                                                    )
                                                    .await;
                                            } else {
                                                let _ = dbus_client::connect_vpn(connection, &ssid).await;
                                            }
                                        }
                                        NetworkCommand::DeactivateConnection(path) => {
                                            let _ = dbus_client::deactivate_connection(connection, &path).await;
                                        }
                                        NetworkCommand::ConnectVpn(name_or_uuid) => {
                                            let _ = dbus_client::connect_vpn(connection, &name_or_uuid).await;
                                        }
                                        NetworkCommand::DisconnectVpn(name_or_path) => {
                                            let _ = dbus_client::disconnect_vpn(connection, &name_or_path).await;
                                        }
                                        NetworkCommand::SetAirplaneModeEnabled(enabled) => {
                                            let _ = dbus_client::set_wireless_enabled(connection, !enabled).await;
                                            let _ = dbus_client::set_wwan_enabled(connection, !enabled).await;
                                        }
                                    }
                                }
                            }
                            None => break,
                        }
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(3)) => {
                        if let Some(ref connection) = connection_opt {
                            let nm_state = dbus_client::get_nm_state(connection).await.unwrap_or(0);
                            let is_connected = nm_state == 70;
                            let wifi_enabled = dbus_client::get_wireless_enabled(connection).await.unwrap_or(true);
                            let access_points = dbus_client::list_access_points(connection).await.unwrap_or_default();
                            let active_vpns = dbus_client::list_active_vpns(connection).await.unwrap_or_default();
                            let (connection_type, ip_config) = dbus_client::get_primary_connection_info(connection).await.unwrap_or(("none".to_string(), None));

                            let active_ssid = access_points.iter().find(|ap| ap.is_connected).map(|ap| ap.ssid.clone());
                            let airplane_mode = tx_clone.borrow().airplane_mode;

                            let info = NetworkInfo {
                                is_connected,
                                connection_type,
                                ssid: active_ssid,
                                wifi_enabled,
                                airplane_mode,
                                access_points,
                                active_vpns,
                                ip_config,
                                available: true,
                            };
                            let _ = tx_clone.send_replace(info);
                        }
                    }
                }
            }
        });

        Ok(Self {
            tx,
            _task: Some(task),
            command_tx: Some(command_tx),
        })
    }

    pub fn subscribe(&self) -> watch::Receiver<NetworkInfo> {
        self.tx.subscribe()
    }

    pub fn network_info(&self) -> NetworkInfo {
        self.tx.borrow().clone()
    }

    pub fn set_wifi_enabled(&self, enabled: bool) -> Result<()> {
        if let Some(tx) = &self.command_tx {
            tx.try_send(NetworkCommand::SetWifiEnabled(enabled))
                .map_err(|e| anyhow::anyhow!("Failed to send command: {}", e))?;
        }
        Ok(())
    }

    pub fn scan_wifi(&self) -> Result<()> {
        if let Some(tx) = &self.command_tx {
            tx.try_send(NetworkCommand::ScanWifi)
                .map_err(|e| anyhow::anyhow!("Failed to send command: {}", e))?;
        }
        Ok(())
    }

    pub fn connect_wifi(&self, ssid: &str, object_path: Option<&str>) -> Result<()> {
        if let Some(tx) = &self.command_tx {
            tx.try_send(NetworkCommand::ConnectWifi {
                ssid: ssid.to_string(),
                object_path: object_path.map(|s| s.to_string()),
            })
            .map_err(|e| anyhow::anyhow!("Failed to send command: {}", e))?;
        }
        Ok(())
    }

    pub fn deactivate_connection(&self, active_conn_path: &str) -> Result<()> {
        let path = active_conn_path.to_string();
        if let Some(tx) = &self.command_tx {
            tx.try_send(NetworkCommand::DeactivateConnection(path))
                .map_err(|e| anyhow::anyhow!("Failed to send command: {}", e))?;
        }
        Ok(())
    }

    pub fn connect_vpn(&self, name_or_uuid: &str) -> Result<()> {
        let name = name_or_uuid.to_string();
        if let Some(tx) = &self.command_tx {
            tx.try_send(NetworkCommand::ConnectVpn(name))
                .map_err(|e| anyhow::anyhow!("Failed to send command: {}", e))?;
        }
        Ok(())
    }

    pub fn disconnect_vpn(&self, name_or_path: &str) -> Result<()> {
        let name = name_or_path.to_string();
        if let Some(tx) = &self.command_tx {
            tx.try_send(NetworkCommand::DisconnectVpn(name))
                .map_err(|e| anyhow::anyhow!("Failed to send command: {}", e))?;
        }
        Ok(())
    }

    pub fn set_airplane_mode_enabled(&self, enabled: bool) -> Result<()> {
        if let Some(tx) = &self.command_tx {
            tx.try_send(NetworkCommand::SetAirplaneModeEnabled(enabled))
                .map_err(|e| anyhow::anyhow!("Failed to send command: {}", e))?;
        }
        let mut current = self.tx.borrow().clone();
        current.airplane_mode = enabled;
        if enabled {
            current.wifi_enabled = false;
        }
        let _ = self.tx.send_replace(current);
        Ok(())
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
                airplane_mode: false,
                access_points: Vec::new(),
                active_vpns: Vec::new(),
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
    }
}
