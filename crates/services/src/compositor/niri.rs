use anyhow::Result;
use niri_ipc::{Response, socket::Socket};
use std::sync::{Arc, Mutex};

/// Active workspace representation from Niri IPC.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NiriWorkspaceInfo {
    pub id: u64,
    pub name: Option<String>,
    pub idx: u8,
    pub is_active: bool,
    pub is_focused: bool,
}

/// Niri Compositor IPC service for tracking workspaces and window focus.
pub struct NiriCompositorService {
    workspaces: Arc<Mutex<Vec<NiriWorkspaceInfo>>>,
    active_window_title: Arc<Mutex<Option<String>>>,
}

impl NiriCompositorService {
    /// Connects to Niri IPC socket and initializes state.
    pub fn new() -> Result<Self> {
        let workspaces = Arc::new(Mutex::new(Vec::new()));
        let active_window_title = Arc::new(Mutex::new(None));

        let service = Self {
            workspaces,
            active_window_title,
        };

        // Query initial workspace list if Niri socket is connected
        if let Ok(mut socket) = Socket::connect()
            && let Ok(Ok(Response::Workspaces(ws_list))) =
                socket.send(niri_ipc::Request::Workspaces)
        {
            let mut ws_guard = service.workspaces.lock().unwrap();
            *ws_guard = ws_list
                .into_iter()
                .map(|w| NiriWorkspaceInfo {
                    id: w.id,
                    name: w.name,
                    idx: w.idx,
                    is_active: w.is_active,
                    is_focused: w.is_focused,
                })
                .collect();
        }

        Ok(service)
    }

    /// Returns the current list of Niri workspaces.
    pub fn workspaces(&self) -> Vec<NiriWorkspaceInfo> {
        self.workspaces.lock().unwrap().clone()
    }

    /// Returns active window title.
    pub fn active_window_title(&self) -> Option<String> {
        self.active_window_title.lock().unwrap().clone()
    }

    /// Switches focus to the workspace with the specified ID.
    pub fn focus_workspace(&self, id: u64) -> Result<()> {
        if let Ok(mut socket) = Socket::connect() {
            let req = niri_ipc::Request::Action(niri_ipc::Action::FocusWorkspace {
                reference: niri_ipc::WorkspaceReferenceArg::Id(id),
            });
            let _ = socket.send(req);
        }
        Ok(())
    }
}
