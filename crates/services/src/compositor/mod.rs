pub mod niri;
pub mod test_adapter;

pub use niri::NiriCompositorService;
pub use test_adapter::TestCompositorAdapter;

use std::sync::Arc;
use tokio::sync::watch;

/// Connection status of the compositor adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompositorConnection {
    Connecting,
    Ready,
    Reconnecting {
        attempt: u32,
        last_error: Option<String>,
    },
    Stopped,
}

impl CompositorConnection {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    pub fn state_name(&self) -> &'static str {
        match self {
            Self::Connecting => "connecting",
            Self::Ready => "ready",
            Self::Reconnecting { .. } => "reconnecting",
            Self::Stopped => "stopped",
        }
    }
}

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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositorCapabilities {
    pub can_create_workspace: bool,
    pub can_move_window: bool,
    pub can_focus_window: bool,
    pub can_focus_workspace: bool,
}

impl Default for CompositorCapabilities {
    fn default() -> Self {
        Self {
            can_create_workspace: true,
            can_move_window: true,
            can_focus_window: true,
            can_focus_workspace: true,
        }
    }
}

/// Generic workspace information.
#[derive(Clone, Debug, PartialEq, Eq)]
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowInfo {
    pub id: u64,
    pub title: Option<String>,
    pub app_id: Option<String>,
    pub workspace_id: Option<u64>,
    pub is_focused: bool,
    pub is_floating: bool,
    pub is_urgent: bool,
}

/// Revisioned atomic snapshot of the compositor state.
#[derive(Clone, Debug, PartialEq)]
pub struct CompositorSnapshot {
    pub revision: u64,
    pub connection: CompositorConnection,
    pub capabilities: CompositorCapabilities,
    pub outputs: Vec<CompositorOutput>,
    pub workspaces: Vec<WorkspaceInfo>,
    pub windows: Vec<WindowInfo>,
    pub focused_output: Option<String>,
    pub focused_workspace_id: Option<u64>,
    pub focused_window_id: Option<u64>,
    pub active_keyboard_layout: Option<String>,
}

impl Default for CompositorSnapshot {
    fn default() -> Self {
        Self {
            revision: 0,
            connection: CompositorConnection::Connecting,
            capabilities: CompositorCapabilities::default(),
            outputs: Vec::new(),
            workspaces: Vec::new(),
            windows: Vec::new(),
            focused_output: None,
            focused_workspace_id: None,
            focused_window_id: None,
            active_keyboard_layout: None,
        }
    }
}

/// Operations sent to the active compositor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompositorCommand {
    FocusWorkspace(u64),
    FocusWindow(u64),
    FocusPreviousWindow,
    CreateWorkspace { name: Option<String> },
    MoveWindowToWorkspace { window_id: u64, workspace_id: u64 },
}

/// Compositor-agnostic interface for window managers and shell integrations.
pub trait CompositorAdapter: Send + Sync {
    fn current(&self) -> Arc<CompositorSnapshot>;
    fn subscribe(&self) -> watch::Receiver<Arc<CompositorSnapshot>>;
    fn execute(&self, command: CompositorCommand) -> anyhow::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_compositor_adapter() {
        let adapter = TestCompositorAdapter::new_default();
        assert_eq!(
            adapter.current().connection,
            CompositorConnection::Connecting
        );

        let snapshot = CompositorSnapshot {
            connection: CompositorConnection::Ready,
            revision: 1,
            ..Default::default()
        };
        adapter.update(snapshot);

        assert_eq!(adapter.current().connection, CompositorConnection::Ready);
        assert_eq!(adapter.current().revision, 1);

        assert!(
            adapter
                .execute(CompositorCommand::FocusWorkspace(1))
                .is_ok()
        );
        assert_eq!(
            adapter.executed_commands(),
            vec![CompositorCommand::FocusWorkspace(1)]
        );
    }
}
