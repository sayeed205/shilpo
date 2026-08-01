use anyhow::Result;
use std::sync::{Arc, Mutex};
use zbus::Connection;

/// Active VPN connection status.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VpnConnection {
    pub id: String,
    pub vpn_type: String,
    pub is_active: bool,
    pub object_path: String,
}

/// Network connection status.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetworkInfo {
    pub is_connected: bool,
    pub ssid: Option<String>,
    pub wifi_enabled: bool,
    pub airplane_mode: bool,
    pub active_vpns: Vec<VpnConnection>,
    pub available: bool,
}

use tokio::sync::mpsc;

#[derive(Clone, Debug, PartialEq, Eq)]
enum NetworkCommand {
    SetWifiEnabled(bool),
    DeactivateConnection(String),
    ConnectVpn(String),
    DisconnectVpn(String),
    SetAirplaneModeEnabled(bool),
}

/// NetworkManager service for network status tracking and zbus control.
pub struct NetworkService {
    info: Arc<Mutex<NetworkInfo>>,
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
        Self {
            info: Arc::new(Mutex::new(NetworkInfo::default())),
            _task: None,
            command_tx: None,
        }
    }

    pub fn new() -> Result<Self> {
        let info = Arc::new(Mutex::new(NetworkInfo::default()));
        let (command_tx, mut command_rx) = mpsc::channel::<NetworkCommand>(32);

        let info_clone = info.clone();
        let task = tokio::spawn(async move {
            let connection_opt = Connection::system().await.ok();

            loop {
                tokio::select! {
                    cmd = command_rx.recv() => {
                        match cmd {
                            Some(cmd) => {
                                match cmd {
                                    NetworkCommand::SetWifiEnabled(enabled) => {
                                        if let Some(ref connection) = connection_opt {
                                            let _ = connection
                                                .call_method(
                                                    Some("org.freedesktop.NetworkManager"),
                                                    "/org/freedesktop/NetworkManager",
                                                    Some("org.freedesktop.DBus.Properties"),
                                                    "Set",
                                                    &(
                                                        "org.freedesktop.NetworkManager",
                                                        "WirelessEnabled",
                                                        zbus::zvariant::Value::Bool(enabled),
                                                    ),
                                                )
                                                .await;
                                        }
                                    }
                                    NetworkCommand::DeactivateConnection(path) => {
                                        if let Some(ref connection) = connection_opt
                                            && let Ok(obj_path) =
                                                zbus::zvariant::ObjectPath::try_from(path)
                                        {
                                            let _ = connection
                                                .call_method(
                                                    Some("org.freedesktop.NetworkManager"),
                                                    "/org/freedesktop/NetworkManager",
                                                    Some("org.freedesktop.NetworkManager"),
                                                    "DeactivateConnection",
                                                    &(obj_path,),
                                                )
                                                .await;
                                        }
                                    }
                                    NetworkCommand::ConnectVpn(name) => {
                                        let _ = std::process::Command::new("nmcli")
                                            .args(["connection", "up", &name])
                                            .status();
                                    }
                                    NetworkCommand::DisconnectVpn(name) => {
                                        let _ = std::process::Command::new("nmcli")
                                            .args(["connection", "down", &name])
                                            .status();
                                    }
                                    NetworkCommand::SetAirplaneModeEnabled(enabled) => {
                                        if let Some(ref connection) = connection_opt {
                                            let _ = connection
                                                .call_method(
                                                    Some("org.freedesktop.NetworkManager"),
                                                    "/org/freedesktop/NetworkManager",
                                                    Some("org.freedesktop.DBus.Properties"),
                                                    "Set",
                                                    &(
                                                        "org.freedesktop.NetworkManager",
                                                        "WirelessEnabled",
                                                        zbus::zvariant::Value::Bool(!enabled),
                                                    ),
                                                )
                                                .await;
                                            let _ = connection
                                                .call_method(
                                                    Some("org.freedesktop.NetworkManager"),
                                                    "/org/freedesktop/NetworkManager",
                                                    Some("org.freedesktop.DBus.Properties"),
                                                    "Set",
                                                    &(
                                                        "org.freedesktop.NetworkManager",
                                                        "WwanEnabled",
                                                        zbus::zvariant::Value::Bool(!enabled),
                                                    ),
                                                )
                                                .await;
                                        }
                                    }
                                }
                            }
                            None => break, // Channel closed
                        }
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(3)) => {
                        if let Some(ref connection) = connection_opt {
                            let mut is_connected = false;
                            let mut wifi_enabled = true;
                            let active_vpns = Vec::new();

                            if let Ok(reply) = connection
                                .call_method(
                                    Some("org.freedesktop.NetworkManager"),
                                    "/org/freedesktop/NetworkManager",
                                    Some("org.freedesktop.NetworkManager"),
                                    "state",
                                    &(),
                                )
                                .await
                                && let Ok(state) = reply.body().deserialize::<u32>()
                            {
                                is_connected = state == 70;
                            }

                            // Query WirelessEnabled property via zbus
                            if let Ok(reply) = connection
                                .call_method(
                                    Some("org.freedesktop.NetworkManager"),
                                    "/org/freedesktop/NetworkManager",
                                    Some("org.freedesktop.DBus.Properties"),
                                    "Get",
                                    &("org.freedesktop.NetworkManager", "WirelessEnabled"),
                                )
                                .await
                                && let Ok(val) = reply.body().deserialize::<zbus::zvariant::Value>()
                                && let Ok(enabled) = bool::try_from(val)
                            {
                                wifi_enabled = enabled;
                            }

                            let airplane_mode = {
                                info_clone.lock().unwrap().airplane_mode
                            };

                            let info = NetworkInfo {
                                is_connected,
                                ssid: None,
                                wifi_enabled,
                                airplane_mode,
                                active_vpns,
                                available: true,
                            };
                            *info_clone.lock().unwrap() = info;
                        }
                    }
                }
            }
        });

        Ok(Self {
            info,
            _task: Some(task),
            command_tx: Some(command_tx),
        })
    }

    pub fn network_info(&self) -> NetworkInfo {
        self.info.lock().unwrap().clone()
    }

    pub fn set_wifi_enabled(&self, enabled: bool) -> Result<()> {
        if let Some(tx) = &self.command_tx {
            tx.try_send(NetworkCommand::SetWifiEnabled(enabled))
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

    pub fn connect_vpn(&self, name: &str) -> Result<()> {
        let vpn_name = name.to_string();
        if let Some(tx) = &self.command_tx {
            tx.try_send(NetworkCommand::ConnectVpn(vpn_name))
                .map_err(|e| anyhow::anyhow!("Failed to send command: {}", e))?;
        }
        Ok(())
    }

    pub fn disconnect_vpn(&self, name: &str) -> Result<()> {
        let vpn_name = name.to_string();
        if let Some(tx) = &self.command_tx {
            tx.try_send(NetworkCommand::DisconnectVpn(vpn_name))
                .map_err(|e| anyhow::anyhow!("Failed to send command: {}", e))?;
        }
        Ok(())
    }

    pub fn set_airplane_mode_enabled(&self, enabled: bool) -> Result<()> {
        if let Some(tx) = &self.command_tx {
            tx.try_send(NetworkCommand::SetAirplaneModeEnabled(enabled))
                .map_err(|e| anyhow::anyhow!("Failed to send command: {}", e))?;
        }
        let mut lock = self.info.lock().unwrap();
        lock.airplane_mode = enabled;
        if enabled {
            lock.wifi_enabled = false;
        }
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
                ssid: None,
                wifi_enabled: false,
                airplane_mode: false,
                active_vpns: Vec::new(),
                available: false,
            }
        );
    }

    #[test]
    fn test_vpn_connection_struct() {
        let vpn = VpnConnection {
            id: "Corporate-VPN".to_string(),
            vpn_type: "wireguard".to_string(),
            is_active: true,
            object_path: "/org/freedesktop/NetworkManager/ActiveConnection/1".to_string(),
        };
        assert_eq!(vpn.id, "Corporate-VPN");
        assert!(vpn.is_active);
    }

    #[tokio::test]
    async fn test_vpn_connection_selection_and_status() {
        let service = NetworkService::new_offline();
        let vpn = VpnConnection {
            id: "Corporate VPN".to_string(),
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
        let service = NetworkService {
            info: Arc::new(Mutex::new(NetworkInfo::default())),
            _task: None,
            command_tx: Some(command_tx),
        };

        service.set_wifi_enabled(true).unwrap();
        assert_eq!(
            command_rx.try_recv().unwrap(),
            NetworkCommand::SetWifiEnabled(true)
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

    #[tokio::test]
    async fn test_network_task_cancellation() {
        let (tx, mut rx) = mpsc::channel::<()>(1);
        let task = tokio::spawn(async move {
            let _sentinel = tx;
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        });

        let service = NetworkService {
            info: Arc::new(Mutex::new(NetworkInfo::default())),
            _task: Some(task),
            command_tx: None,
        };

        tokio::task::yield_now().await;
        drop(service);
        tokio::task::yield_now().await;

        assert!(
            rx.recv().await.is_none(),
            "Sentinel should be dropped, channel closed"
        );
    }
}
