use anyhow::Result;
use std::sync::{Arc, Mutex};
use zbus::Connection;

/// Network connection status.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetworkInfo {
    pub is_connected: bool,
    pub ssid: Option<String>,
    pub available: bool,
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
            if let Ok(connection) = Connection::system().await {
                loop {
                    let info = if let Ok(reply) = connection
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
                        NetworkInfo {
                            is_connected: state == 70,
                            ssid: None,
                            available: true,
                        }
                    } else {
                        NetworkInfo::default()
                    };
                    *info_clone.lock().unwrap() = info;
                    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                }
            }
        });

        Ok(service)
    }

    pub fn network_info(&self) -> NetworkInfo {
        self.info.lock().unwrap().clone()
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
                available: false,
            }
        );
    }
}
