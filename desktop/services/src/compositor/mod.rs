pub mod broker;
pub mod niri;
pub mod test_adapter;

use std::sync::Arc;

pub use broker::{
    BrokerOptions, CancellationReason, CommandCancellation, CommandExecutorFn, CommandOutcome,
    CommandTicket, CompositorBrokerTelemetry, CompositorCommandBroker, CompositorTarget,
    ExecutorAck,
};
pub use niri::NiriCompositorService;
pub use test_adapter::TestCompositorAdapter;
use tokio::sync::watch;

/// DomainVersion tuple containing owner_generation and revision with strict lexicographical ordering.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct DomainVersion {
    pub owner_generation: u64,
    pub revision: u64,
}

impl DomainVersion {
    pub const ZERO: Self = Self {
        owner_generation: 0,
        revision: 0,
    };

    pub fn new(owner_generation: u64, revision: u64) -> Self {
        Self {
            owner_generation,
            revision,
        }
    }
}

impl std::fmt::Display for DomainVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "g{}.r{}", self.owner_generation, self.revision)
    }
}

/// Consumer-facing connection status of the compositor adapter.
#[derive(Clone, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompositorConnection {
    #[default]
    Unavailable,
    Connecting,
    Ready,
    Reconnecting,
    Degraded,
}

impl CompositorConnection {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    pub fn state_name(&self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::Connecting => "connecting",
            Self::Ready => "ready",
            Self::Reconnecting => "reconnecting",
            Self::Degraded => "degraded",
        }
    }
}

/// Supervisor operational state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupervisorState {
    Starting,
    Running,
    Backoff { attempt: u32, retry_at_ms: u64 },
    Quarantined,
    Stopping,
    Stopped,
}

/// Error returned when publishing a stale or conflicting snapshot update.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StaleUpdateError {
    StaleVersion {
        current: DomainVersion,
        attempted: DomainVersion,
    },
    ConflictingSnapshot {
        version: DomainVersion,
    },
    UninstalledGeneration {
        installed: u64,
        attempted: u64,
    },
}

/// Bounded command mailbox policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MailboxPolicy {
    Lossless,
    ReplaceLatest { key: String },
}

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

/// Compositor capabilities descriptor.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CompositorCapabilities {
    pub can_create_workspace: bool,
    pub can_move_window: bool,
    pub can_focus_window: bool,
    pub can_focus_workspace: bool,
    pub can_close_window: bool,
}

impl Default for CompositorCapabilities {
    fn default() -> Self {
        Self {
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
    pub idx: u8,
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
    pub column: Option<usize>,
    pub row: Option<usize>,
}

/// Revisioned atomic snapshot of the compositor state.
#[derive(Clone, Debug, PartialEq)]
pub struct CompositorSnapshot {
    pub version: DomainVersion,
    pub connection: CompositorConnection,
    pub capabilities: CompositorCapabilities,
    pub outputs: Vec<CompositorOutput>,
    pub workspaces: Vec<WorkspaceInfo>,
    pub windows: Vec<WindowInfo>,
    pub focused_output: Option<String>,
    pub focused_workspace_id: Option<u64>,
    pub focused_window_id: Option<u64>,
    pub active_keyboard_layout: Option<String>,
    pub last_error: Option<String>,
}

impl Default for CompositorSnapshot {
    fn default() -> Self {
        Self {
            version: DomainVersion::ZERO,
            connection: CompositorConnection::Unavailable,
            capabilities: CompositorCapabilities::default(),
            outputs: Vec::new(),
            workspaces: Vec::new(),
            windows: Vec::new(),
            focused_output: None,
            focused_workspace_id: None,
            focused_window_id: None,
            active_keyboard_layout: None,
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
