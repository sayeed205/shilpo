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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositorCapability {
    CreateWorkspace,
    MoveWindow,
    FocusWindow,
    FocusWorkspace,
}

/// Compositor-agnostic interface for window managers and shell integrations.
pub trait CompositorAdapter: Send + Sync {
    fn capabilities(&self) -> CompositorCapabilities;
    fn disabled_reason(&self, capability: CompositorCapability) -> Option<&'static str> {
        let caps = self.capabilities();
        match capability {
            CompositorCapability::CreateWorkspace if !caps.can_create_workspace => {
                Some("Workspace creation is not supported by the active compositor")
            }
            CompositorCapability::MoveWindow if !caps.can_move_window => {
                Some("Moving windows is not supported by the active compositor")
            }
            CompositorCapability::FocusWindow if !caps.can_focus_window => {
                Some("Focusing windows is not supported by the active compositor")
            }
            CompositorCapability::FocusWorkspace if !caps.can_focus_workspace => {
                Some("Focusing workspaces is not supported by the active compositor")
            }
            _ => None,
        }
    }

    fn workspaces(&self) -> Vec<WorkspaceInfo>;
    fn windows(&self) -> Vec<WindowInfo>;
    fn active_window_id(&self) -> Option<u64>;
    fn active_window_title(&self) -> Option<String>;
    fn app_id(&self) -> Option<String>;
    fn keyboard_layout(&self) -> String;

    fn focus_workspace(&self, id: u64) -> anyhow::Result<()>;
    fn focus_window(&self, id: u64) -> anyhow::Result<()>;
    fn create_workspace(&self, name: Option<String>) -> anyhow::Result<()>;
    fn rename_workspace(&self, old_name: &str, new_name: &str) -> anyhow::Result<()>;
    fn delete_workspace(&self, name: &str) -> anyhow::Result<()>;
    fn move_window_to_workspace(&self, window_id: u64, workspace_id: u64) -> anyhow::Result<()>;
    fn reorder_workspace(&self, id: u64, new_index: u8) -> anyhow::Result<()>;
    fn move_workspace_to_output(&self, id: u64, output_name: &str) -> anyhow::Result<()>;
    fn restore_compositor_session(&self) -> anyhow::Result<()> {
        Ok(())
    }
    fn focus_previous_window(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestCompositor;
    impl CompositorAdapter for TestCompositor {
        fn capabilities(&self) -> CompositorCapabilities {
            CompositorCapabilities {
                can_create_workspace: false,
                can_move_window: true,
                can_focus_window: true,
                can_focus_workspace: true,
            }
        }
        fn workspaces(&self) -> Vec<WorkspaceInfo> {
            Vec::new()
        }
        fn windows(&self) -> Vec<WindowInfo> {
            Vec::new()
        }
        fn active_window_id(&self) -> Option<u64> {
            None
        }
        fn active_window_title(&self) -> Option<String> {
            None
        }
        fn app_id(&self) -> Option<String> {
            None
        }
        fn keyboard_layout(&self) -> String {
            "us".to_string()
        }
        fn focus_workspace(&self, _id: u64) -> anyhow::Result<()> {
            Ok(())
        }
        fn focus_window(&self, _id: u64) -> anyhow::Result<()> {
            Ok(())
        }
        fn create_workspace(&self, _name: Option<String>) -> anyhow::Result<()> {
            Ok(())
        }
        fn rename_workspace(&self, _old: &str, _new: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn delete_workspace(&self, _name: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn move_window_to_workspace(&self, _w: u64, _ws: u64) -> anyhow::Result<()> {
            Ok(())
        }
        fn reorder_workspace(&self, _id: u64, _new_index: u8) -> anyhow::Result<()> {
            Ok(())
        }
        fn move_workspace_to_output(&self, _id: u64, _output_name: &str) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_compositor_disabled_reason_feedback() {
        let compositor = TestCompositor;
        assert!(
            compositor
                .disabled_reason(CompositorCapability::CreateWorkspace)
                .is_some()
        );
        assert!(
            compositor
                .disabled_reason(CompositorCapability::MoveWindow)
                .is_none()
        );
        assert!(compositor.restore_compositor_session().is_ok());
    }
}
