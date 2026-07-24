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
    pub active_vpns: Vec<VpnConnection>,
    pub available: bool,
}

/// NetworkManager service for network status tracking and zbus control.
pub struct NetworkService {
    info: Arc<Mutex<NetworkInfo>>,
    _task: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for NetworkService {
    fn drop(&mut self) {
        if let Some(task) = self._task.take() {
            task.abort();
        }
    }
}

impl NetworkService {
    pub fn new() -> Result<Self> {
        let info = Arc::new(Mutex::new(NetworkInfo::default()));

        let info_clone = info.clone();
        let task = tokio::spawn(async move {
            if let Ok(connection) = Connection::system().await {
                loop {
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

                    let info = NetworkInfo {
                        is_connected,
                        ssid: None,
                        wifi_enabled,
                        active_vpns,
                        available: true,
                    };
                    *info_clone.lock().unwrap() = info;
                    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                }
            }
        });

        Ok(Self {
            info,
            _task: Some(task),
        })
    }

    pub fn network_info(&self) -> NetworkInfo {
        self.info.lock().unwrap().clone()
    }

    pub fn set_wifi_enabled(&self, enabled: bool) -> Result<()> {
        tokio::spawn(async move {
            if let Ok(connection) = Connection::system().await {
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
        });
        Ok(())
    }

    pub fn deactivate_connection(&self, active_conn_path: &str) -> Result<()> {
        let path = active_conn_path.to_string();
        tokio::spawn(async move {
            if let Ok(connection) = Connection::system().await
                && let Ok(obj_path) = zbus::zvariant::ObjectPath::try_from(path)
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
        });
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
}
