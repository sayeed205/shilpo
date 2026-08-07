//! Virtual Private Network (VPN) domain structures and connection metadata.

use serde::{Deserialize, Serialize};

/// Active or configured VPN connection status.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VpnConnection {
    /// Human-readable VPN connection profile name (e.g. "Corporate-VPN").
    pub id: String,
    /// Unique identifier for the VPN connection profile.
    pub uuid: String,
    /// Type of VPN technology (e.g. "wireguard", "openvpn", "vpn").
    pub vpn_type: String,
    /// Whether the VPN connection is currently active/connected.
    pub is_active: bool,
    /// DBus object path of the active connection or connection setting.
    pub object_path: String,
}

