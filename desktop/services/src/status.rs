//! Service health and status data types.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BarState {
    #[default]
    Starting,
    Visible,
    Hidden,
    OpenFailed,
}

impl BarState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Visible => "visible",
            Self::Hidden => "hidden",
            Self::OpenFailed => "open_failed",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessState {
    #[default]
    Starting,
    Ready,
    Degraded,
    Failed,
}

impl ReadinessState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Ready => "ready",
            Self::Degraded => "degraded",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "state", content = "attempt")]
pub enum ServiceLifecycle {
    #[default]
    Unavailable,
    Connecting {
        attempt: u32,
    },
    Ready,
}

impl ServiceLifecycle {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::Connecting { .. } => "connecting",
            Self::Ready => "ready",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ServiceHealth {
    pub compositor_connected: bool,
    #[serde(default)]
    pub compositor_state: String,
    #[serde(default)]
    pub compositor_owner_generation: u64,
    #[serde(default)]
    pub compositor_revision: u64,
    #[serde(default)]
    pub compositor_reconnect_attempt: u32,
    #[serde(default)]
    pub compositor_last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compositor_telemetry: Option<crate::compositor::CompositorBrokerTelemetry>,
    pub battery_service_available: bool,
    #[serde(default)]
    pub battery_state: ServiceLifecycle,
    #[serde(default)]
    pub battery_last_error: Option<String>,
    pub audio_service_available: bool,
    #[serde(default)]
    pub audio_state: ServiceLifecycle,
    #[serde(default)]
    pub audio_last_error: Option<String>,
    pub network_service_available: bool,
    #[serde(default)]
    pub network_state: ServiceLifecycle,
    #[serde(default)]
    pub network_last_error: Option<String>,
    pub notification_service_available: bool,
    #[serde(default)]
    pub notification_state: ServiceLifecycle,
    #[serde(default)]
    pub notification_last_error: Option<String>,
    #[serde(default)]
    pub media_service_available: bool,
    #[serde(default)]
    pub media_state: ServiceLifecycle,
    #[serde(default)]
    pub media_last_error: Option<String>,
    #[serde(default)]
    pub brightness_service_available: bool,
    #[serde(default)]
    pub brightness_state: ServiceLifecycle,
    #[serde(default)]
    pub brightness_last_error: Option<String>,
    pub heed_store_available: bool,
    pub uptime_seconds: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension_host: Option<serde_json::Value>,
}
