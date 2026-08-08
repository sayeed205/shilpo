//! Wi-Fi domain structures and access point metadata.

use serde::{Deserialize, Serialize};

/// Wi-Fi access point metadata representing a discovered wireless network.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WifiAccessPoint {
    /// Service Set Identifier (network name).
    pub ssid: String,
    /// Basic Service Set Identifier (MAC address of the access point).
    pub bssid: String,
    /// Signal strength percentage (0-100%).
    pub signal_percent: u8,
    /// Security protocol description (e.g. "WPA2/WPA3", "WPA-Enterprise", "Open").
    pub security_type: String,
    /// Radio operating frequency in megahertz (e.g., 2412, 5240).
    pub frequency_mhz: u32,
    /// Whether this access point is currently active/connected.
    pub is_connected: bool,
    /// DBus object path for the access point.
    pub object_path: String,
}

/// Strongly-typed Wi-Fi security classification.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WifiSecurity {
    #[default]
    Open,
    Wpa,
    Wpa2Wpa3Personal,
    WpaEnterprise,
    Wpa2Wpa3Enterprise,
}

impl WifiAccessPoint {
    /// Classify the access point's security protocol as a strongly-typed enum.
    pub fn security(&self) -> WifiSecurity {
        if self.security_type.contains("Enterprise") || self.security_type.contains("802.1X") {
            if self.security_type.contains("WPA2") || self.security_type.contains("WPA3") {
                WifiSecurity::Wpa2Wpa3Enterprise
            } else {
                WifiSecurity::WpaEnterprise
            }
        } else if self.security_type.contains("WPA2") || self.security_type.contains("WPA3") {
            WifiSecurity::Wpa2Wpa3Personal
        } else if self.security_type.contains("WPA") {
            WifiSecurity::Wpa
        } else {
            WifiSecurity::Open
        }
    }

    /// Returns `true` if the access point requires authentication/encryption.
    pub fn is_secure(&self) -> bool {
        self.security() != WifiSecurity::Open
    }

    /// Returns `true` if the access point uses 802.1X / Enterprise authentication.
    pub fn is_enterprise(&self) -> bool {
        matches!(
            self.security(),
            WifiSecurity::WpaEnterprise | WifiSecurity::Wpa2Wpa3Enterprise
        )
    }
}
