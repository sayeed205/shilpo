use serde::{Deserialize, Serialize};

/// Wi-Fi access point metadata.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WifiAccessPoint {
    pub ssid: String,
    pub bssid: String,
    pub signal_percent: u8,
    pub security_type: String,
    pub frequency_mhz: u32,
    pub is_connected: bool,
    pub object_path: String,
}

impl WifiAccessPoint {
    pub fn is_secure(&self) -> bool {
        self.security_type != "Open" && !self.security_type.is_empty()
    }
}
