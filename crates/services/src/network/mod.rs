use anyhow::Result;
use std::sync::{Arc, Mutex};
use zbus::Connection;

/// Network connection status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkInfo {
    pub is_connected: bool,
    pub ssid: Option<String>,
}

impl Default for NetworkInfo {
    fn default() -> Self {
        Self {
            is_connected: true,
            ssid: Some("WiFi".into()),
        }
    }
}

/// NetworkManager service for network status tracking.
pub struct NetworkService {
    info: Arc<Mutex<NetworkInfo>>,
}

impl NetworkService {
    pub fn new() -> Result<Self> {
        let info = Arc::new(Mutex::new(NetworkInfo::default()));
        let service = Self { info };

        let info_clone = service.info.clone();
        tokio::spawn(async move {
            if let Ok(connection) = Connection::system().await
                && let Ok(reply) = connection
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
                let mut lock = info_clone.lock().unwrap();
                lock.is_connected = state == 70; // NM_STATE_CONNECTED_GLOBAL
            }
        });

        Ok(service)
    }

    pub fn network_info(&self) -> NetworkInfo {
        self.info.lock().unwrap().clone()
    }
}
