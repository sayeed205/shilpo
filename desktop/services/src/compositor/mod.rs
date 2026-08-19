pub mod broker;
pub mod detect;
pub mod generic;
pub mod niri;
pub mod null;
pub mod registry;
pub mod test_adapter;

use std::sync::Arc;

pub use broker::ExecutorAck;
pub use broker::{
    BrokerOptions, CommandCancellation, CommandExecutorFn, CommandOutcome, CommandTicket,
    CompositorBrokerTelemetry, CompositorCommandBroker, CompositorTarget,
};
pub use detect::{CompositorKind, detect, detect_from};
pub use generic::{BoundProtocols, GenericWaylandCompositorBackend};
pub use niri::NiriCompositorService;
pub use null::NullCompositorBackend;
pub use registry::{
    BackendFactory, CandidateBackend, CompositorRegistry, init_compositor, init_compositor_with,
};
pub use shilpo_domain::{
    CancellationReason, DomainLifecycle, DomainVersion, MailboxPolicy, StaleUpdateError,
    SupervisorState,
};
pub use test_adapter::TestCompositorAdapter;
use tokio::sync::watch;

/// Typed rejection reasons for compositor commands.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectionReason {
    Unavailable,
    Overloaded,
    Unsupported,
    InvalidTarget(CompositorTarget),
    TargetDisappeared(CompositorTarget),
    BackendRejected { message: String },
    Transport { message: String },
    TimedOut,
    Cancelled(CancellationReason),
}

impl RejectionReason {
    pub fn message(&self) -> String {
        match self {
            Self::Unavailable => "compositor unavailable".into(),
            Self::Overloaded => "compositor queue overloaded".into(),
            Self::Unsupported => "compositor command unsupported".into(),
            Self::InvalidTarget(t) => format!("invalid target: {t}"),
            Self::TargetDisappeared(t) => format!("target disappeared before application: {t}"),
            Self::BackendRejected { message } => format!("backend rejected command: {message}"),
            Self::Transport { message } => format!("transport error: {message}"),
            Self::TimedOut => "compositor command timed out".into(),
            Self::Cancelled(reason) => format!("compositor command cancelled: {reason}"),
        }
    }
}

impl std::fmt::Display for RejectionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for RejectionReason {}

/// Errors returned by compositor command operations (alias for RejectionReason).
pub type CompositorCommandError = RejectionReason;

/// Compositor-neutral output metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct CompositorOutput {
    pub name: String,
    pub make: Option<String>,
    pub model: Option<String>,
    pub logical_position: (i32, i32),
    pub logical_size: (u32, u32),
    pub scale: f64,
}

/// Window identity level supported by a compositor backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowIdentity {
    /// No window model at all.
    None,
    /// Protocol handles only; focus/close work, but IDs are not stable across reconnects
    /// and cannot be joined with an external window model.
    Fuzzy,
    /// Compositor-assigned IDs, stable and addressable.
    Exact,
}

/// Compositor capabilities descriptor.
///
/// Default is all-false with WindowIdentity::None (degrade closed).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CompositorCapabilities {
    pub window_identity: WindowIdentity,
    pub can_create_workspace: bool,
    pub can_move_window: bool,
    pub can_focus_window: bool,
    pub can_focus_workspace: bool,
    pub can_close_window: bool,
}

impl Default for CompositorCapabilities {
    fn default() -> Self {
        Self {
            window_identity: WindowIdentity::None,
            can_create_workspace: false,
            can_move_window: false,
            can_focus_window: false,
            can_focus_workspace: false,
            can_close_window: false,
        }
    }
}

impl CompositorCapabilities {
    /// All capabilities enabled, for a backend that is fully connected and ready.
    pub fn full(window_identity: WindowIdentity) -> Self {
        Self {
            window_identity,
            can_create_workspace: true,
            can_move_window: true,
            can_focus_window: true,
            can_focus_workspace: true,
            can_close_window: true,
        }
    }
}

/// Generic workspace information.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceInfo {
    pub id: u64,
    pub name: Option<String>,
    pub idx: u32,
    pub is_active: bool,
    pub is_focused: bool,
    pub is_urgent: bool,
    pub output_name: Option<String>,
    pub active_window_id: Option<u64>,
}

/// Generic window information.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WindowInfo {
    pub id: u64,
    pub title: Option<String>,
    pub app_id: Option<String>,
    pub workspace_id: Option<u64>,
    pub is_focused: bool,
    pub is_floating: bool,
    pub is_urgent: bool,
    pub layout_x: Option<f64>,
    pub layout_y: Option<f64>,
}

/// Niri-specific layout extras.
#[derive(Clone, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct NiriExtras {
    pub window_positions: std::collections::HashMap<u64, (usize, usize)>,
}

/// Backend-specific extra data attached to a compositor snapshot.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum CompositorExtras {
    #[default]
    None,
    Niri(NiriExtras),
}

/// Revisioned atomic snapshot of the compositor state.
#[derive(Clone, Debug, PartialEq)]
pub struct CompositorSnapshot {
    pub version: DomainVersion,
    pub connection: DomainLifecycle,
    pub capabilities: CompositorCapabilities,
    pub outputs: Vec<CompositorOutput>,
    pub workspaces: Vec<WorkspaceInfo>,
    pub windows: Vec<WindowInfo>,
    pub focused_output: Option<String>,
    pub focused_workspace_id: Option<u64>,
    pub focused_window_id: Option<u64>,
    pub active_keyboard_layout: Option<String>,
    pub extras: CompositorExtras,
    pub last_error: Option<String>,
}

impl Default for CompositorSnapshot {
    fn default() -> Self {
        Self {
            version: DomainVersion::ZERO,
            connection: DomainLifecycle::Unavailable,
            capabilities: CompositorCapabilities::default(),
            outputs: Vec::new(),
            workspaces: Vec::new(),
            windows: Vec::new(),
            focused_output: None,
            focused_workspace_id: None,
            focused_window_id: None,
            active_keyboard_layout: None,
            extras: CompositorExtras::None,
            last_error: None,
        }
    }
}

impl CompositorSnapshot {
    pub fn revision(&self) -> u64 {
        self.version.revision
    }

    pub fn owner_generation(&self) -> u64 {
        self.version.owner_generation
    }
}

/// Operations sent to the active compositor.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "action", content = "payload")]
pub enum CompositorCommand {
    FocusWorkspace(u64),
    FocusWindow(u64),
    FocusPreviousWindow,
    CloseWindow(u64),
    CreateWorkspace,
    MoveWindowToWorkspace { window_id: u64, workspace_id: u64 },
}

/// Compositor-agnostic interface for window managers and shell integrations.
pub trait CompositorAdapter: Send + Sync {
    fn current(&self) -> Arc<CompositorSnapshot>;
    fn subscribe(&self) -> watch::Receiver<Arc<CompositorSnapshot>>;
    fn command_broker(&self) -> Arc<CompositorCommandBroker>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capabilities_default_all_false() {
        let caps = CompositorCapabilities::default();
        assert_eq!(caps.window_identity, WindowIdentity::None);
        assert!(!caps.can_create_workspace);
        assert!(!caps.can_move_window);
        assert!(!caps.can_focus_window);
        assert!(!caps.can_focus_workspace);
        assert!(!caps.can_close_window);
    }

    #[test]
    fn test_default_snapshot_rejects_every_command() {
        let broker = CompositorCommandBroker::new(
            BrokerOptions::default(),
            Box::new(|_cmd, _timeout, _cancel, _register| Ok(ExecutorAck::Success)),
        );
        let _ = broker.observe_snapshot(Arc::new(CompositorSnapshot::default()));

        let commands = [
            CompositorCommand::CreateWorkspace,
            CompositorCommand::FocusWorkspace(1),
            CompositorCommand::FocusWindow(10),
            CompositorCommand::FocusPreviousWindow,
            CompositorCommand::CloseWindow(10),
            CompositorCommand::MoveWindowToWorkspace {
                window_id: 10,
                workspace_id: 1,
            },
        ];

        for cmd in commands {
            let res = broker.submit_with_policy(cmd, MailboxPolicy::Lossless);
            assert!(
                matches!(
                    res,
                    Err(CommandOutcome::Rejected {
                        reason: RejectionReason::Unavailable | RejectionReason::Unsupported
                    })
                ),
                "command should be rejected on default snapshot"
            );
        }
    }
}
