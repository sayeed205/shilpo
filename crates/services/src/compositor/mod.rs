pub mod niri;

pub use niri::{NiriCompositorService, NiriWorkspaceInfo};

/// Compositor capabilities descriptor.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CompositorCapabilities {
    pub can_create_workspace: bool,
    pub can_move_window: bool,
    pub can_focus_window: bool,
    pub can_focus_workspace: bool,
}

/// Generic workspace information.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceInfo {
    pub id: u64,
    pub name: Option<String>,
    pub idx: u8,
    pub is_active: bool,
    pub is_focused: bool,
}

/// Generic window information.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowInfo {
    pub id: u64,
    pub title: Option<String>,
    pub app_id: Option<String>,
    pub workspace_id: Option<u64>,
    pub is_focused: bool,
}

/// Compositor-agnostic interface for window managers and shell integrations.
pub trait CompositorAdapter: Send + Sync {
    fn capabilities(&self) -> CompositorCapabilities;
    fn workspaces(&self) -> Vec<WorkspaceInfo>;
    fn windows(&self) -> Vec<WindowInfo>;
    fn active_window_id(&self) -> Option<u64>;
    fn active_window_title(&self) -> Option<String>;
    fn app_id(&self) -> Option<String>;
    fn keyboard_layout(&self) -> String;

    fn focus_workspace(&self, id: u64) -> anyhow::Result<()>;
    fn focus_window(&self, id: u64) -> anyhow::Result<()>;
    fn create_workspace(&self, name: Option<String>) -> anyhow::Result<()>;
    fn move_window_to_workspace(&self, window_id: u64, workspace_id: u64) -> anyhow::Result<()>;
}
