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
    active_window_id: Arc<Mutex<Option<u64>>>,
    app_id: Arc<Mutex<Option<String>>>,
    active_window_title: Arc<Mutex<Option<String>>>,
    keyboard_layout: Arc<Mutex<String>>,
}

impl NiriCompositorService {
    /// Connects to Niri IPC socket, queries initial state, and spawns a background event listener thread.
    pub fn new() -> Result<Self> {
        let workspaces = Arc::new(Mutex::new(Vec::new()));
        let active_window_id = Arc::new(Mutex::new(None));
        let app_id = Arc::new(Mutex::new(None));
        let active_window_title = Arc::new(Mutex::new(None));
        let keyboard_layout = Arc::new(Mutex::new("us".into()));

        let service = Self {
            workspaces,
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
        let win_id_clone = service.active_window_id.clone();
        let app_id_clone = service.app_id.clone();
        let title_clone = service.active_window_title.clone();
        let kb_clone = service.keyboard_layout.clone();

        thread::spawn(move || {
            if let Err(e) =
                run_niri_listener(ws_clone, win_id_clone, app_id_clone, title_clone, kb_clone)
            {
                tracing::error!(error = %e, "Niri listener exited");
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
        if let Ok(mut socket) = Socket::connect() {
            let req = Request::Action(niri_ipc::Action::FocusWorkspace {
                reference: niri_ipc::WorkspaceReferenceArg::Id(id),
            });
            let _ = socket.send(req);
        }
        Ok(())
    }

    /// Switches focus to the window with the specified ID.
    pub fn focus_window(&self, id: u64) -> Result<()> {
        if let Ok(mut socket) = Socket::connect() {
            let req = Request::Action(niri_ipc::Action::FocusWindow { id });
            let _ = socket.send(req);
        }
        Ok(())
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
