use serde::{Deserialize, Serialize};

/// Active or configured VPN connection status.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VpnConnection {
    pub id: String,
    pub uuid: String,
    pub vpn_type: String,
    pub is_active: bool,
    pub object_path: String,
}
