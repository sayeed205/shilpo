//! D-Bus wire types for org.shilpo.Shell.

use serde::{Deserialize, Serialize};

/// Compositor command terminal outcome wire type.
#[derive(Debug, Clone, Default, Serialize, Deserialize, zbus::zvariant::Type, PartialEq, Eq)]
pub struct CommandResult {
    pub outcome: String,
    pub owner_generation: u64,
    pub revision: u64,
    pub reason: String,
    pub last_observed_generation: u64,
    pub last_observed_revision: u64,
}

impl CommandResult {
    pub fn is_applied(&self) -> bool {
        matches!(self.outcome.as_str(), "applied" | "reconciled_applied")
    }
}

impl From<shilpo_services::CommandOutcome> for CommandResult {
    fn from(outcome: shilpo_services::CommandOutcome) -> Self {
        match outcome {
            shilpo_services::CommandOutcome::Applied { version } => Self {
                outcome: "applied".into(),
                owner_generation: version.owner_generation,
                revision: version.revision,
                reason: String::new(),
                last_observed_generation: 0,
                last_observed_revision: 0,
            },
            shilpo_services::CommandOutcome::ReconciledApplied { version } => Self {
                outcome: "reconciled_applied".into(),
                owner_generation: version.owner_generation,
                revision: version.revision,
                reason: String::new(),
                last_observed_generation: 0,
                last_observed_revision: 0,
            },
            shilpo_services::CommandOutcome::Rejected { reason } => Self {
                outcome: "rejected".into(),
                owner_generation: 0,
                revision: 0,
                reason: reason.to_string(),
                last_observed_generation: 0,
                last_observed_revision: 0,
            },
            shilpo_services::CommandOutcome::TimedOut {
                last_observed_version,
            } => Self {
                outcome: "timed_out".into(),
                owner_generation: 0,
                revision: 0,
                reason: String::new(),
                last_observed_generation: last_observed_version.owner_generation,
                last_observed_revision: last_observed_version.revision,
            },
            shilpo_services::CommandOutcome::Cancelled { reason } => Self {
                outcome: "cancelled".into(),
                owner_generation: 0,
                revision: 0,
                reason: reason.to_string(),
                last_observed_generation: 0,
                last_observed_revision: 0,
            },
        }
    }
}

/// Shell runtime status summary.
#[derive(Debug, Clone, Default, Serialize, Deserialize, zbus::zvariant::Type, PartialEq, Eq)]
pub struct ShellStatus {
    pub running: bool,
    pub instance_id: String,
    pub pid: u32,
    pub readiness: String,
    pub bar_state: String,
    pub overview_visible: bool,
}

/// Shell runtime and service health telemetry summary.
/// Note: `extension_host_diagnostics_json` contains the serialized diagnostic summary
/// string from the Wasmtime extension host runtime as an explicit narrow exception.
#[derive(Debug, Clone, Default, Serialize, Deserialize, zbus::zvariant::Type, PartialEq, Eq)]
pub struct ShellTelemetry {
    pub compositor_connected: bool,
    pub compositor_state: String,
    pub compositor_owner_generation: u64,
    pub compositor_revision: u64,
    pub compositor_reconnect_attempt: u32,
    pub compositor_last_error: String,
    pub battery_service_available: bool,
    pub battery_state: String,
    pub battery_last_error: String,
    pub audio_service_available: bool,
    pub audio_state: String,
    pub audio_last_error: String,
    pub network_service_available: bool,
    pub network_state: String,
    pub network_last_error: String,
    pub notification_service_available: bool,
    pub notification_state: String,
    pub notification_last_error: String,
    pub media_service_available: bool,
    pub media_state: String,
    pub media_last_error: String,
    pub brightness_service_available: bool,
    pub brightness_state: String,
    pub brightness_last_error: String,
    pub heed_store_available: bool,
    pub uptime_seconds: u64,
    pub extension_host_diagnostics_json: String,
}

/// Extension dev session reload terminal outcome wire type.
/// Wire signature: `(sttss)`
#[derive(Debug, Clone, Default, Serialize, Deserialize, zbus::zvariant::Type, PartialEq, Eq)]
pub struct DevReloadResult {
    pub outcome: String,
    pub host_generation: u64,
    pub engine_generation: u64,
    pub diagnostic_code: String,
    pub message: String,
}

impl From<shilpo_ext_runtime::DevReloadOutcome> for DevReloadResult {
    fn from(outcome: shilpo_ext_runtime::DevReloadOutcome) -> Self {
        Self {
            outcome: outcome.outcome,
            host_generation: outcome
                .update
                .as_ref()
                .map(|u| u.host_generation.0)
                .unwrap_or(0),
            engine_generation: outcome.engine_generation.0,
            diagnostic_code: outcome.diagnostic_code,
            message: outcome.message,
        }
    }
}
