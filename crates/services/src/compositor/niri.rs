use super::{CompositorAdapter, CompositorCapabilities, WindowInfo, WorkspaceInfo};
use anyhow::{Context, Result, anyhow};
use niri_ipc::{
    Event, Reply, Request, Response,
    socket::Socket,
    state::{EventStreamState, EventStreamStatePart},
};
use std::{
    env,
    io::{BufRead, BufReader, Write as _},
    os::unix::net::UnixStream,
    sync::{Arc, Mutex},
    thread,
};

/// Active workspace representation from Niri IPC.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NiriWorkspaceInfo {
    pub id: u64,
    pub name: Option<String>,
    pub idx: u8,
    pub is_active: bool,
    pub is_focused: bool,
}

/// Niri Compositor IPC service for tracking workspaces and window focus in real time.
pub struct NiriCompositorService {
    workspaces: Arc<Mutex<Vec<NiriWorkspaceInfo>>>,
    windows: Arc<Mutex<Vec<WindowInfo>>>,
    active_window_id: Arc<Mutex<Option<u64>>>,
    app_id: Arc<Mutex<Option<String>>>,
    active_window_title: Arc<Mutex<Option<String>>>,
    keyboard_layout: Arc<Mutex<String>>,
}

impl NiriCompositorService {
    /// Connects to Niri IPC socket, queries initial state, and spawns a background event listener thread.
    pub fn new() -> Result<Self> {
        let workspaces = Arc::new(Mutex::new(Vec::new()));
        let windows = Arc::new(Mutex::new(Vec::new()));
        let active_window_id = Arc::new(Mutex::new(None));
        let app_id = Arc::new(Mutex::new(None));
        let active_window_title = Arc::new(Mutex::new(None));
        let keyboard_layout = Arc::new(Mutex::new("us".into()));

        let service = Self {
            workspaces,
            windows,
            active_window_id,
            app_id,
            active_window_title,
            keyboard_layout,
        };

        // Query initial workspace list
        if let Ok(mut socket) = Socket::connect()
            && let Ok(Ok(Response::Workspaces(ws_list))) = socket.send(Request::Workspaces)
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

        // Spawn background thread to stream Niri events
        let ws_clone = service.workspaces.clone();
        let win_list_clone = service.windows.clone();
        let win_id_clone = service.active_window_id.clone();
        let app_id_clone = service.app_id.clone();
        let title_clone = service.active_window_title.clone();
        let kb_clone = service.keyboard_layout.clone();

        thread::spawn(move || {
            let mut backoff = std::time::Duration::from_millis(100);
            let max_backoff = std::time::Duration::from_secs(5);
            let max_attempts = 10;
            let mut attempts = 0;

            loop {
                match run_niri_listener(
                    ws_clone.clone(),
                    win_list_clone.clone(),
                    win_id_clone.clone(),
                    app_id_clone.clone(),
                    title_clone.clone(),
                    kb_clone.clone(),
                ) {
                    Ok(()) => {
                        attempts = 0;
                        backoff = std::time::Duration::from_millis(100);
                    }
                    Err(e) => {
                        attempts += 1;
                        tracing::warn!(
                            error = %e,
                            attempt = attempts,
                            max = max_attempts,
                            "Niri listener disconnected or failed; retrying with backoff"
                        );
                        if attempts >= max_attempts {
                            tracing::error!(
                                "Niri listener failed permanently after {max_attempts} attempts"
                            );
                            break;
                        }
                        thread::sleep(backoff);
                        backoff = (backoff * 2).min(max_backoff);
                    }
                }
            }
        });

        Ok(service)
    }

    /// Returns the current list of Niri workspaces.
    pub fn workspaces(&self) -> Vec<NiriWorkspaceInfo> {
        self.workspaces.lock().unwrap().clone()
    }

    /// Returns active window ID.
    pub fn active_window_id(&self) -> Option<u64> {
        *self.active_window_id.lock().unwrap()
    }

    /// Returns active window app ID.
    pub fn app_id(&self) -> Option<String> {
        self.app_id.lock().unwrap().clone()
    }

    /// Returns active window title.
    pub fn active_window_title(&self) -> Option<String> {
        self.active_window_title.lock().unwrap().clone()
    }

    /// Returns current keyboard layout.
    pub fn keyboard_layout(&self) -> String {
        self.keyboard_layout.lock().unwrap().clone()
    }

    /// Switches focus to the workspace with the specified ID.
    pub fn focus_workspace(&self, id: u64) -> Result<()> {
        let mut socket = Socket::connect().context("Failed to connect to Niri IPC socket")?;
        let req = Request::Action(niri_ipc::Action::FocusWorkspace {
            reference: niri_ipc::WorkspaceReferenceArg::Id(id),
        });
        match socket.send(req) {
            Ok(Ok(Response::Handled)) => Ok(()),
            Ok(Ok(resp)) => {
                anyhow::bail!("Niri action FocusWorkspace returned unexpected response: {resp:?}");
            }
            Ok(Err(err)) => {
                anyhow::bail!("Niri action FocusWorkspace for workspace {id} failed: {err}");
            }
            Err(err) => Err(err).with_context(|| {
                format!("Failed to send FocusWorkspace action for workspace {id}")
            }),
        }
    }

    /// Switches focus to the window with the specified ID.
    pub fn focus_window(&self, id: u64) -> Result<()> {
        let mut socket = Socket::connect().context("Failed to connect to Niri IPC socket")?;
        let req = Request::Action(niri_ipc::Action::FocusWindow { id });
        match socket.send(req) {
            Ok(Ok(Response::Handled)) => Ok(()),
            Ok(Ok(resp)) => {
                anyhow::bail!("Niri action FocusWindow returned unexpected response: {resp:?}");
            }
            Ok(Err(err)) => {
                anyhow::bail!("Niri action FocusWindow for window {id} failed: {err}");
            }
            Err(err) => Err(err)
                .with_context(|| format!("Failed to send FocusWindow action for window {id}")),
        }
    }

    /// Moves a window to the target workspace.
    pub fn move_window_to_workspace(&self, window_id: u64, workspace_id: u64) -> Result<()> {
        let mut socket = Socket::connect().context("Failed to connect to Niri IPC socket")?;
        let req = Request::Action(niri_ipc::Action::MoveWindowToWorkspace {
            window_id: Some(window_id),
            reference: niri_ipc::WorkspaceReferenceArg::Id(workspace_id),
            focus: true,
        });
        match socket.send(req) {
            Ok(Ok(Response::Handled)) => Ok(()),
            Ok(Ok(resp)) => {
                anyhow::bail!("Niri action MoveWindowToWorkspace returned unexpected response: {resp:?}");
            }
            Ok(Err(err)) => {
                anyhow::bail!("Niri action MoveWindowToWorkspace failed: {err}");
            }
            Err(err) => Err(err).with_context(|| {
                format!("Failed to send MoveWindowToWorkspace action for window {window_id} -> workspace {workspace_id}")
            }),
        }
    }

    /// Creates a new workspace.
    pub fn create_workspace(&self, _name: Option<String>) -> Result<()> {
        let mut socket = Socket::connect().context("Failed to connect to Niri IPC socket")?;
        let req = Request::Action(niri_ipc::Action::FocusWorkspaceDown {});
        match socket.send(req) {
            Ok(Ok(Response::Handled)) => Ok(()),
            Ok(Ok(resp)) => {
                anyhow::bail!(
                    "Niri action FocusWorkspaceDown returned unexpected response: {resp:?}"
                );
            }
            Ok(Err(err)) => {
                anyhow::bail!("Niri action FocusWorkspaceDown failed: {err}");
            }
            Err(err) => Err(err).with_context(|| "Failed to send FocusWorkspaceDown action"),
        }
    }
}

impl CompositorAdapter for NiriCompositorService {
    fn capabilities(&self) -> CompositorCapabilities {
        CompositorCapabilities {
            can_create_workspace: true,
            can_move_window: true,
            can_focus_window: true,
            can_focus_workspace: true,
        }
    }

    fn workspaces(&self) -> Vec<WorkspaceInfo> {
        self.workspaces
            .lock()
            .unwrap()
            .iter()
            .map(|w| WorkspaceInfo {
                id: w.id,
                name: w.name.clone(),
                idx: w.idx,
                is_active: w.is_active,
                is_focused: w.is_focused,
            })
            .collect()
    }

    fn windows(&self) -> Vec<WindowInfo> {
        self.windows.lock().unwrap().clone()
    }

    fn active_window_id(&self) -> Option<u64> {
        self.active_window_id()
    }

    fn active_window_title(&self) -> Option<String> {
        self.active_window_title()
    }

    fn app_id(&self) -> Option<String> {
        self.app_id()
    }

    fn keyboard_layout(&self) -> String {
        self.keyboard_layout()
    }

    fn focus_workspace(&self, id: u64) -> Result<()> {
        self.focus_workspace(id)
    }

    fn focus_window(&self, id: u64) -> Result<()> {
        self.focus_window(id)
    }

    fn create_workspace(&self, name: Option<String>) -> Result<()> {
        self.create_workspace(name)
    }

    fn move_window_to_workspace(&self, window_id: u64, workspace_id: u64) -> Result<()> {
        self.move_window_to_workspace(window_id, workspace_id)
    }
}

fn connect_socket() -> Result<UnixStream> {
    let socket_path = env::var_os("NIRI_SOCKET")
        .or_else(|| env::var_os("NIRI_SOCKET_PATH"))
        .ok_or_else(|| anyhow!("NIRI_SOCKET not set"))?;
    UnixStream::connect(socket_path).context("Failed to connect to Niri socket")
}

fn run_niri_listener(
    workspaces: Arc<Mutex<Vec<NiriWorkspaceInfo>>>,
    win_list: Arc<Mutex<Vec<WindowInfo>>>,
    active_window_id: Arc<Mutex<Option<u64>>>,
    app_id: Arc<Mutex<Option<String>>>,
    title: Arc<Mutex<Option<String>>>,
    kb_layout: Arc<Mutex<String>>,
) -> Result<()> {
    let mut stream = connect_socket()?;
    let request_json = serde_json::to_string(&Request::EventStream)? + "\n";
    stream.write_all(request_json.as_bytes())?;
    stream.flush()?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;

    let reply: Reply = serde_json::from_str(&line).context("Failed to parse handshake")?;
    if let Err(e) = reply {
        anyhow::bail!("Niri refused EventStream: {}", e);
    }

    reader.get_ref().shutdown(std::net::Shutdown::Write).ok();
    let mut state = EventStreamState::default();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line)?;
        if bytes_read == 0 {
            break;
        }

        let event: Event = match serde_json::from_str(&line) {
            Ok(ev) => ev,
            Err(_) => continue,
        };

        state.apply(event);

        // Update workspace list
        let mut list: Vec<NiriWorkspaceInfo> = state
            .workspaces
            .workspaces
            .values()
            .map(|w| NiriWorkspaceInfo {
                id: w.id,
                name: w.name.clone(),
                idx: w.idx,
                is_active: w.is_active,
                is_focused: w.is_focused,
            })
            .collect();
        list.sort_by_key(|w| w.idx);

        let mut ws_guard = workspaces.lock().unwrap();
        *ws_guard = list;

        // Update window list
        let windows: Vec<WindowInfo> = state
            .windows
            .windows
            .values()
            .map(|w| WindowInfo {
                id: w.id,
                title: w.title.clone(),
                app_id: w.app_id.clone(),
                workspace_id: w.workspace_id,
                is_focused: w.is_focused,
            })
            .collect();

        let mut win_list_guard = win_list.lock().unwrap();
        *win_list_guard = windows;

        // Update active window title, app ID & window ID
        let focused_win = state.windows.windows.values().find(|w| w.is_focused);
        let mut win_id_guard = active_window_id.lock().unwrap();
        let mut app_id_guard = app_id.lock().unwrap();
        let mut title_guard = title.lock().unwrap();

        if let Some(win) = focused_win {
            *win_id_guard = Some(win.id);
            *app_id_guard = win.app_id.clone();
            *title_guard = win.title.clone();
        } else {
            *win_id_guard = None;
            *app_id_guard = None;
            *title_guard = None;
        }

        // Update keyboard layout
        if let Some(kb) = &state.keyboard_layouts.keyboard_layouts
            && let Some(name) = kb.names.get(kb.current_idx as usize)
        {
            let mut kb_guard = kb_layout.lock().unwrap();
            *kb_guard = name.clone();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_focus_workspace_fails_without_niri() {
        let orig = env::var_os("NIRI_SOCKET");
        unsafe {
            env::set_var("NIRI_SOCKET", "/tmp/non_existent_niri_socket.sock");
        }

        let service = NiriCompositorService {
            workspaces: Arc::new(Mutex::new(Vec::new())),
            windows: Arc::new(Mutex::new(Vec::new())),
            active_window_id: Arc::new(Mutex::new(None)),
            app_id: Arc::new(Mutex::new(None)),
            active_window_title: Arc::new(Mutex::new(None)),
            keyboard_layout: Arc::new(Mutex::new("us".into())),
        };

        let err = service.focus_workspace(1).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("Niri IPC socket") || msg.contains("Failed to connect"),
            "expected socket connection error context, got: {msg}"
        );

        unsafe {
            if let Some(val) = orig {
                env::set_var("NIRI_SOCKET", val);
            } else {
                env::remove_var("NIRI_SOCKET");
            }
        }
    }

    #[test]
    fn test_focus_window_fails_without_niri() {
        let orig = env::var_os("NIRI_SOCKET");
        unsafe {
            env::set_var("NIRI_SOCKET", "/tmp/non_existent_niri_socket.sock");
        }

        let service = NiriCompositorService {
            workspaces: Arc::new(Mutex::new(Vec::new())),
            windows: Arc::new(Mutex::new(Vec::new())),
            active_window_id: Arc::new(Mutex::new(None)),
            app_id: Arc::new(Mutex::new(None)),
            active_window_title: Arc::new(Mutex::new(None)),
            keyboard_layout: Arc::new(Mutex::new("us".into())),
        };

        let err = service.focus_window(10).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("Niri IPC socket") || msg.contains("Failed to connect"),
            "expected socket connection error context, got: {msg}"
        );

        unsafe {
            if let Some(val) = orig {
                env::set_var("NIRI_SOCKET", val);
            } else {
                env::remove_var("NIRI_SOCKET");
            }
        }
    }
}
