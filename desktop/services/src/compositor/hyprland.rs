//! Hyprland IPC Compositor Backend (Tier 2).
//!
//! Connects directly to Hyprland's Unix domain sockets (`.socket.sock` for commands and queries,
//! and `.socket2.sock` for the event stream). Implements pure JSON and event-line parsing
//! for deterministic snapshot generation and testability without live sockets.

use std::{
    collections::HashSet,
    env,
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use anyhow::Result;
use serde::Deserialize;
use tokio::sync::watch;

use super::{
    BrokerOptions, CommandCancellation, CompositorAdapter, CompositorCapabilities,
    CompositorCommand, CompositorCommandBroker, CompositorExtras, CompositorOutput,
    CompositorSnapshot, DomainLifecycle, DomainVersion, ExecutorAck, HyprlandExtras,
    HyprlandSpecialWorkspace, RejectionReason, StaleUpdateError, SupervisorState, WindowIdentity,
    WindowInfo, WorkspaceInfo,
    broker::{StreamCancelHandle, create_stream_cancel_handle},
    supervision::{
        CapabilityProvider, CompositorSupervision, apply_tick, publish_reconnecting,
        record_supervisor_failure, sleep_with_stop_flag,
    },
};
use crate::domain::{DomainSupervisor, MonotonicTimeSource, TimeSource};

/// Capability provider for Hyprland.
#[derive(Clone, Copy, Debug, Default)]
pub struct HyprlandCapabilityProvider;

impl CapabilityProvider for HyprlandCapabilityProvider {
    fn capabilities_for(&self, lifecycle: DomainLifecycle) -> CompositorCapabilities {
        hyprland_capabilities(lifecycle)
    }
}

pub(crate) fn hyprland_capabilities(connection: DomainLifecycle) -> CompositorCapabilities {
    if connection == DomainLifecycle::Ready {
        CompositorCapabilities::full(WindowIdentity::Exact)
    } else {
        CompositorCapabilities::default()
    }
}

// ---------------------------------------------------------------------------
// Pure Parsing Layer (ADR-0006 / ADR-0017)
// ---------------------------------------------------------------------------

/// Parses a hexadecimal address string (`"0x55f81234abcd"` or `"55f81234abcd"`) into a `u64`.
pub fn parse_hex_address(addr: &str) -> Option<u64> {
    let s = addr.trim();
    if s.is_empty() {
        return None;
    }
    let hex = s.strip_prefix("0x").unwrap_or(s);
    u64::from_str_radix(hex, 16).ok()
}

/// Formats a `u64` address into Hyprland's canonical hex representation (`"0x..."`).
pub fn format_hex_address(addr: u64) -> String {
    format!("0x{addr:x}")
}

/// Parsed event from Hyprland `.socket2.sock`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HyprlandEvent {
    WorkspaceV2 {
        id: i64,
        name: String,
    },
    OpenWindow {
        address: u64,
        workspace: String,
        class: String,
        title: String,
    },
    CloseWindow {
        address: u64,
    },
    ActiveWindowV2 {
        address: Option<u64>,
    },
    MoveWindowV2 {
        address: u64,
        workspace_id: i64,
        workspace_name: String,
    },
    Urgent {
        address: u64,
    },
    WindowTitleV2 {
        address: u64,
        title: String,
    },
    FocusedMon {
        monitor_name: String,
        workspace_name: String,
    },
    CreateWorkspaceV2 {
        id: i64,
        name: String,
    },
    DestroyWorkspaceV2 {
        id: i64,
        name: String,
    },
    Other {
        name: String,
        data: String,
    },
}

/// Pure parser for a single line emitted on `.socket2.sock`.
pub fn parse_event_line(line: &str) -> Option<HyprlandEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (event_name, data) = trimmed.split_once(">>")?;
    match event_name {
        "workspacev2" => {
            let mut parts = data.splitn(2, ',');
            let id_str = parts.next()?;
            let name = parts.next().unwrap_or("").to_string();
            let id = id_str.parse::<i64>().ok()?;
            Some(HyprlandEvent::WorkspaceV2 { id, name })
        }
        "openwindow" => {
            let mut parts = data.splitn(4, ',');
            let addr_str = parts.next()?;
            let ws = parts.next().unwrap_or("").to_string();
            let class = parts.next().unwrap_or("").to_string();
            let title = parts.next().unwrap_or("").to_string();
            let address = parse_hex_address(addr_str)?;
            Some(HyprlandEvent::OpenWindow {
                address,
                workspace: ws,
                class,
                title,
            })
        }
        "closewindow" => {
            let address = parse_hex_address(data)?;
            Some(HyprlandEvent::CloseWindow { address })
        }
        "activewindowv2" => {
            let address = parse_hex_address(data);
            Some(HyprlandEvent::ActiveWindowV2 { address })
        }
        "activewindow" => {
            if data == "," || data.is_empty() {
                Some(HyprlandEvent::ActiveWindowV2 { address: None })
            } else {
                Some(HyprlandEvent::Other {
                    name: event_name.to_string(),
                    data: data.to_string(),
                })
            }
        }
        "movewindowv2" => {
            let mut parts = data.splitn(3, ',');
            let addr_str = parts.next()?;
            let wsid_str = parts.next().unwrap_or("");
            let wsname = parts.next().unwrap_or("").to_string();
            let address = parse_hex_address(addr_str)?;
            let workspace_id = wsid_str.parse::<i64>().unwrap_or(0);
            Some(HyprlandEvent::MoveWindowV2 {
                address,
                workspace_id,
                workspace_name: wsname,
            })
        }
        "urgent" => {
            let address = parse_hex_address(data)?;
            Some(HyprlandEvent::Urgent { address })
        }
        "windowtitlev2" => {
            let mut parts = data.splitn(2, ',');
            let addr_str = parts.next()?;
            let title = parts.next().unwrap_or("").to_string();
            let address = parse_hex_address(addr_str)?;
            Some(HyprlandEvent::WindowTitleV2 { address, title })
        }
        "focusedmon" => {
            let mut parts = data.splitn(2, ',');
            let monitor_name = parts.next()?.to_string();
            let workspace_name = parts.next().unwrap_or("").to_string();
            Some(HyprlandEvent::FocusedMon {
                monitor_name,
                workspace_name,
            })
        }
        "createworkspacev2" => {
            let mut parts = data.splitn(2, ',');
            let id_str = parts.next()?;
            let name = parts.next().unwrap_or("").to_string();
            let id = id_str.parse::<i64>().ok()?;
            Some(HyprlandEvent::CreateWorkspaceV2 { id, name })
        }
        "destroyworkspacev2" => {
            let mut parts = data.splitn(2, ',');
            let id_str = parts.next()?;
            let name = parts.next().unwrap_or("").to_string();
            let id = id_str.parse::<i64>().ok()?;
            Some(HyprlandEvent::DestroyWorkspaceV2 { id, name })
        }
        other => Some(HyprlandEvent::Other {
            name: other.to_string(),
            data: data.to_string(),
        }),
    }
}

// ---------------------------------------------------------------------------
// DTOs for JSON Queries
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize)]
pub struct HyprlandWorkspaceRefDto {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HyprlandClientDto {
    pub address: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub class: String,
    #[serde(default)]
    pub workspace: HyprlandWorkspaceRefDto,
    #[serde(default)]
    pub floating: bool,
    #[serde(default, rename = "focusHistoryID")]
    pub focus_history_id: i64,
    #[serde(default)]
    pub at: [i32; 2],
    #[serde(default)]
    pub size: [i32; 2],
    #[serde(default)]
    pub hidden: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HyprlandWorkspaceDto {
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub monitor: String,
    #[serde(default, rename = "monitorID")]
    pub monitor_id: Option<i64>,
    #[serde(default)]
    pub windows: u64,
    #[serde(default, rename = "hasfullscreen")]
    pub has_fullscreen: bool,
    #[serde(default, rename = "lastwindow")]
    pub last_window: String,
    #[serde(default, rename = "lastwindowtitle")]
    pub last_window_title: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HyprlandMonitorDto {
    #[serde(default)]
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub make: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub scale: f64,
    #[serde(default)]
    pub focused: bool,
    #[serde(default, rename = "activeWorkspace")]
    pub active_workspace: HyprlandWorkspaceRefDto,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct HyprlandActiveWindowDto {
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub class: String,
}

/// Pure JSON decoding for `j/clients`.
pub fn parse_clients_json(json: &str) -> Result<Vec<HyprlandClientDto>, serde_json::Error> {
    serde_json::from_str(json)
}

/// Pure JSON decoding for `j/workspaces`.
pub fn parse_workspaces_json(json: &str) -> Result<Vec<HyprlandWorkspaceDto>, serde_json::Error> {
    serde_json::from_str(json)
}

/// Pure JSON decoding for `j/monitors`.
pub fn parse_monitors_json(json: &str) -> Result<Vec<HyprlandMonitorDto>, serde_json::Error> {
    serde_json::from_str(json)
}

/// Pure JSON decoding for `j/activewindow`.
pub fn parse_active_window_json(json: &str) -> Result<Option<u64>, serde_json::Error> {
    let dto: HyprlandActiveWindowDto = serde_json::from_str(json)?;
    Ok(parse_hex_address(&dto.address))
}

/// Encodes a `CompositorCommand` into a Hyprland dispatcher command string.
/// Encodes a command for Hyprland's raw `.socket.sock` `dispatch` request. Since Hyprland
/// 0.55's move to Lua config, the compositor wraps whatever follows `dispatch ` into
/// `hl.dispatch(<that text>)` and evaluates it as Lua -- the old bare hyprlang dispatcher
/// strings (`dispatch focuswindow address:...`) are no longer valid syntax and are rejected
/// ("')' expected near 'address'"). Each form below was verified directly against a live
/// 0.56.2 instance over the raw socket before landing here.
pub fn encode_hyprland_command(cmd: &CompositorCommand) -> Result<String, RejectionReason> {
    match cmd {
        CompositorCommand::FocusWorkspace(id) => {
            Ok(format!("dispatch hl.dsp.focus({{workspace = {id}}})"))
        }
        CompositorCommand::FocusWindow(id) => Ok(format!(
            "dispatch hl.dsp.focus({{window = \"address:{}\"}})",
            format_hex_address(*id)
        )),
        CompositorCommand::FocusPreviousWindow => {
            Ok("dispatch hl.dsp.focus({last = true})".to_string())
        }
        CompositorCommand::CloseWindow(id) => Ok(format!(
            "dispatch hl.dsp.window.close({{window = \"address:{}\"}})",
            format_hex_address(*id)
        )),
        CompositorCommand::CreateWorkspace => {
            Ok("dispatch hl.dsp.focus({workspace = \"empty\"})".to_string())
        }
        CompositorCommand::MoveWindowToWorkspace {
            window_id,
            workspace_id,
        } => Ok(format!(
            "dispatch hl.dsp.window.move({{workspace = {workspace_id}, follow = false, window = \"address:{}\"}})",
            format_hex_address(*window_id)
        )),
    }
}

/// Updates urgency tracking state in response to a parsed event. Hyprland raises `urgent`
/// when a window demands attention, and clears implicitly once that window is focused or closed.
pub fn apply_urgent_event(urgent_windows: &mut HashSet<u64>, event: &HyprlandEvent) {
    match event {
        HyprlandEvent::Urgent { address } => {
            urgent_windows.insert(*address);
        }
        HyprlandEvent::ActiveWindowV2 {
            address: Some(addr),
        } => {
            urgent_windows.remove(addr);
        }
        HyprlandEvent::CloseWindow { address } => {
            urgent_windows.remove(address);
        }
        _ => {}
    }
}

/// Assembles a deterministic `CompositorSnapshot` from parsed Hyprland structures.
#[allow(clippy::too_many_arguments)]
pub fn build_hyprland_snapshot(
    version: DomainVersion,
    lifecycle: DomainLifecycle,
    clients: &[HyprlandClientDto],
    workspaces: &[HyprlandWorkspaceDto],
    monitors: &[HyprlandMonitorDto],
    active_window_addr: Option<u64>,
    urgent_windows: &HashSet<u64>,
    last_error: Option<String>,
) -> CompositorSnapshot {
    // 1. Outputs
    let mut outputs: Vec<CompositorOutput> = monitors
        .iter()
        .map(|m| CompositorOutput {
            name: m.name.clone(),
            make: if m.make.is_empty() {
                None
            } else {
                Some(m.make.clone())
            },
            model: if m.model.is_empty() {
                None
            } else {
                Some(m.model.clone())
            },
            logical_position: (m.x, m.y),
            logical_size: (m.width, m.height),
            scale: if m.scale > 0.0 { m.scale } else { 1.0 },
        })
        .collect();
    outputs.sort_by(|a, b| a.name.cmp(&b.name));

    // Focused output & active workspace id on focused monitor
    let focused_monitor = monitors.iter().find(|m| m.focused);
    let focused_output = focused_monitor.map(|m| m.name.clone());
    let focused_workspace_id = focused_monitor.and_then(|m| {
        if m.active_workspace.id > 0 {
            Some(m.active_workspace.id as u64)
        } else {
            None
        }
    });

    let active_monitors_workspaces: HashSet<i64> =
        monitors.iter().map(|m| m.active_workspace.id).collect();

    // 2. Workspaces vs Special Workspaces
    let mut standard_workspaces: Vec<WorkspaceInfo> = Vec::new();
    let mut special_workspaces: Vec<HyprlandSpecialWorkspace> = Vec::new();

    for ws in workspaces {
        if ws.id > 0 {
            let ws_id = ws.id as u64;
            let is_active = active_monitors_workspaces.contains(&ws.id);
            let is_focused = focused_workspace_id == Some(ws_id);
            // Hyprland reports "0x0" as the sentinel for "this workspace has no last
            // window", not a real address -- parse_hex_address alone can't tell the
            // difference (0x0 parses fine as address 0), so an empty workspace was being
            // read as occupied by whatever treated `active_window_id` as an occupancy
            // check (e.g. the trailing-empty-workspace synthesis below).
            let active_window_id = parse_hex_address(&ws.last_window).filter(|&addr| addr != 0);

            standard_workspaces.push(WorkspaceInfo {
                id: ws_id,
                name: if ws.name.is_empty() {
                    None
                } else {
                    Some(ws.name.clone())
                },
                idx: ws.id as u32,
                is_active,
                is_focused,
                is_urgent: false,
                output_name: if ws.monitor.is_empty() {
                    None
                } else {
                    Some(ws.monitor.clone())
                },
                active_window_id,
            });
        } else {
            special_workspaces.push(HyprlandSpecialWorkspace {
                id: ws.id,
                name: ws.name.clone(),
                monitor: if ws.monitor.is_empty() {
                    None
                } else {
                    Some(ws.monitor.clone())
                },
            });
        }
    }
    standard_workspaces.sort_by_key(|w| w.id);
    special_workspaces.sort_by_key(|w| w.id);

    // Unlike Niri's dynamic workspaces (which always keep one trailing empty workspace to
    // scroll/click into), Hyprland only reports workspaces that have been visited or have
    // windows. The bar's workspace pill renders exactly one dot per entry here, so without
    // this there is no dot for the next empty workspace to click -- Super+N still works
    // (Hyprland auto-creates a numbered workspace on focus) but clicking does not, since
    // there's nothing to click. Mirror Niri's convention by appending one synthetic empty
    // workspace, unless an empty one is already listed (e.g. the focused workspace has no
    // windows).
    if standard_workspaces
        .iter()
        .all(|w| w.active_window_id.is_some())
    {
        let next_id = standard_workspaces.iter().map(|w| w.id).max().unwrap_or(0) + 1;
        standard_workspaces.push(WorkspaceInfo {
            id: next_id,
            name: None,
            idx: next_id as u32,
            is_active: false,
            is_focused: false,
            is_urgent: false,
            output_name: focused_output.clone(),
            active_window_id: None,
        });
    }

    // 3. Windows
    let mut windows: Vec<WindowInfo> = Vec::new();
    for c in clients {
        let Some(window_id) = parse_hex_address(&c.address) else {
            continue;
        };

        let is_focused = c.focus_history_id == 0 || active_window_addr == Some(window_id);
        let is_urgent = urgent_windows.contains(&window_id);
        let workspace_id = if c.workspace.id > 0 {
            Some(c.workspace.id as u64)
        } else {
            None
        };

        windows.push(WindowInfo {
            id: window_id,
            title: if c.title.is_empty() {
                None
            } else {
                Some(c.title.clone())
            },
            app_id: if c.class.is_empty() {
                None
            } else {
                Some(c.class.clone())
            },
            workspace_id,
            is_focused,
            is_floating: c.floating,
            is_urgent,
            layout_x: Some(c.at[0] as f64),
            layout_y: Some(c.at[1] as f64),
        });
    }
    windows.sort_by_key(|w| w.id);

    let focused_window_id =
        active_window_addr.or_else(|| windows.iter().find(|w| w.is_focused).map(|w| w.id));

    let extras = if special_workspaces.is_empty() {
        CompositorExtras::Hyprland(HyprlandExtras::default())
    } else {
        CompositorExtras::Hyprland(HyprlandExtras { special_workspaces })
    };

    CompositorSnapshot {
        version,
        connection: lifecycle,
        capabilities: hyprland_capabilities(lifecycle),
        outputs,
        workspaces: standard_workspaces,
        windows,
        focused_output,
        focused_workspace_id,
        focused_window_id,
        active_keyboard_layout: None,
        extras,
        last_error,
    }
}

// ---------------------------------------------------------------------------
// Socket Path Discovery & IPC Execution
// ---------------------------------------------------------------------------

/// Locates Hyprland sockets using `$HYPRLAND_INSTANCE_SIGNATURE`.
pub fn resolve_hyprland_socket_dir() -> Option<PathBuf> {
    let his = env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    if his.is_empty() {
        return None;
    }

    if let Ok(runtime_dir) = env::var("XDG_RUNTIME_DIR") {
        let dir = PathBuf::from(runtime_dir).join("hypr").join(&his);
        if dir.exists() {
            return Some(dir);
        }
    }

    let tmp_dir = PathBuf::from("/tmp/hypr").join(&his);
    if tmp_dir.exists() {
        return Some(tmp_dir);
    }

    None
}

/// Executes a query on `.socket.sock` and returns the response string.
pub fn query_hyprland_socket(socket_path: &Path, query: &str) -> Result<String> {
    let mut stream = UnixStream::connect(socket_path)?;
    stream.set_read_timeout(Some(Duration::from_millis(500)))?;
    stream.set_write_timeout(Some(Duration::from_millis(500)))?;

    // Hyprland's `j/...` query commands reject a trailing newline outright ("unknown
    // request") -- unlike `dispatch ...` commands, which tolerate it fine.
    stream.write_all(query.as_bytes())?;
    stream.shutdown(std::net::Shutdown::Write)?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}

/// Executes a command on `.socket.sock` respecting timeout and cancellation.
pub fn execute_hyprland_command_on_socket(
    socket_path: &Path,
    cmd_str: &str,
    timeout: Duration,
    cancel: Arc<CommandCancellation>,
    register_cancel: &dyn Fn(Arc<dyn StreamCancelHandle>),
) -> Result<ExecutorAck, RejectionReason> {
    if cancel.is_cancelled() {
        return Err(RejectionReason::Cancelled(
            cancel.reason().unwrap_or(super::CancellationReason::User),
        ));
    }

    let start = std::time::Instant::now();
    let remaining_timeout = || -> Result<Duration, RejectionReason> {
        if cancel.is_cancelled() {
            return Err(RejectionReason::Cancelled(
                cancel.reason().unwrap_or(super::CancellationReason::User),
            ));
        }
        let elapsed = start.elapsed();
        if elapsed >= timeout {
            return Err(RejectionReason::TimedOut);
        }
        Ok(timeout - elapsed)
    };

    let remaining = remaining_timeout()?;
    let mut stream = match UnixStream::connect(socket_path) {
        Ok(s) => s,
        Err(err) => {
            return Err(RejectionReason::Transport {
                message: format!(
                    "failed to connect to socket {}: {err}",
                    socket_path.display()
                ),
            });
        }
    };

    let _ = stream.set_read_timeout(Some(remaining));
    let _ = stream.set_write_timeout(Some(remaining));

    let cancel_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            return Err(RejectionReason::Transport {
                message: e.to_string(),
            });
        }
    };

    register_cancel(create_stream_cancel_handle(cancel_stream));

    if let Err(err) = stream.write_all(cmd_str.as_bytes()) {
        if cancel.is_cancelled() {
            return Err(RejectionReason::Cancelled(
                cancel.reason().unwrap_or(super::CancellationReason::User),
            ));
        }
        if err.kind() == std::io::ErrorKind::TimedOut
            || err.kind() == std::io::ErrorKind::WouldBlock
        {
            return Err(RejectionReason::TimedOut);
        }
        return Err(RejectionReason::Transport {
            message: err.to_string(),
        });
    }

    if let Err(err) = stream.write_all(b"\n") {
        if cancel.is_cancelled() {
            return Err(RejectionReason::Cancelled(
                cancel.reason().unwrap_or(super::CancellationReason::User),
            ));
        }
        if err.kind() == std::io::ErrorKind::TimedOut
            || err.kind() == std::io::ErrorKind::WouldBlock
        {
            return Err(RejectionReason::TimedOut);
        }
        return Err(RejectionReason::Transport {
            message: err.to_string(),
        });
    }

    let _ = stream.shutdown(std::net::Shutdown::Write);

    let mut response = String::new();
    match stream.read_to_string(&mut response) {
        Ok(_) => {
            let reply = response.trim();
            if reply == "ok" {
                Ok(ExecutorAck::Success)
            } else {
                Err(RejectionReason::BackendRejected {
                    message: reply.to_string(),
                })
            }
        }
        Err(err) => {
            if cancel.is_cancelled() {
                return Err(RejectionReason::Cancelled(
                    cancel.reason().unwrap_or(super::CancellationReason::User),
                ));
            }
            if err.kind() == std::io::ErrorKind::TimedOut
                || err.kind() == std::io::ErrorKind::WouldBlock
            {
                return Err(RejectionReason::TimedOut);
            }
            Err(RejectionReason::Transport {
                message: format!("Hyprland command read error: {err}"),
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Backend Implementation
// ---------------------------------------------------------------------------

/// Hyprland compositor backend implementing `CompositorAdapter`.
pub struct HyprlandCompositorBackend {
    supervision: CompositorSupervision<HyprlandCapabilityProvider>,
    time_source: Arc<dyn TimeSource>,
    stop_flag: Arc<AtomicBool>,
    handle: Mutex<Option<thread::JoinHandle<()>>>,
}

impl HyprlandCompositorBackend {
    /// Non-failing constructor. Publishes `Unavailable` immediately and starts background listener thread.
    pub fn new() -> Arc<Self> {
        let initial = CompositorSnapshot {
            version: DomainVersion::ZERO,
            connection: DomainLifecycle::Unavailable,
            capabilities: hyprland_capabilities(DomainLifecycle::Unavailable),
            outputs: Vec::new(),
            workspaces: Vec::new(),
            windows: Vec::new(),
            focused_output: None,
            focused_workspace_id: None,
            focused_window_id: None,
            active_keyboard_layout: None,
            extras: CompositorExtras::Hyprland(HyprlandExtras::default()),
            last_error: None,
        };

        let stop_flag = Arc::new(AtomicBool::new(false));
        let time_source: Arc<dyn TimeSource> = Arc::new(MonotonicTimeSource::new());

        let broker = CompositorCommandBroker::new(
            BrokerOptions::default(),
            Box::new(move |cmd, timeout, cancel, register| {
                let Some(socket_dir) = resolve_hyprland_socket_dir() else {
                    return Err(RejectionReason::Unavailable);
                };
                let cmd_socket = socket_dir.join(".socket.sock");
                let cmd_str = encode_hyprland_command(cmd)?;
                execute_hyprland_command_on_socket(&cmd_socket, &cmd_str, timeout, cancel, register)
            }),
        );

        let supervision = CompositorSupervision::new(initial, broker, HyprlandCapabilityProvider);

        let tx_clone = supervision.tx.clone();
        let stop_clone = stop_flag.clone();
        let supervisor_clone = supervision.supervisor.clone();
        let time_source_clone = time_source.clone();
        let broker_clone = supervision.broker.clone();

        let handle = thread::spawn(move || {
            run_hyprland_listener(
                tx_clone,
                stop_clone,
                supervisor_clone,
                time_source_clone,
                broker_clone,
            );
        });

        Arc::new(Self {
            supervision,
            time_source,
            stop_flag,
            handle: Mutex::new(Some(handle)),
        })
    }

    /// Constructs an offline instance with injected clock, broker, and initial snapshot.
    pub fn new_offline_with(
        snapshot: CompositorSnapshot,
        time_source: Arc<dyn TimeSource>,
        broker: Arc<CompositorCommandBroker>,
    ) -> Arc<Self> {
        let supervision = CompositorSupervision::new(snapshot, broker, HyprlandCapabilityProvider);
        let stop_flag = Arc::new(AtomicBool::new(true));

        Arc::new(Self {
            supervision,
            time_source,
            stop_flag,
            handle: Mutex::new(None),
        })
    }

    /// Constructs an offline instance for testing with a specified initial snapshot.
    pub fn new_offline(snapshot: CompositorSnapshot) -> Arc<Self> {
        let broker = CompositorCommandBroker::new(
            BrokerOptions::default(),
            Box::new(|_cmd, _timeout, _cancel, _register| Ok(ExecutorAck::Success)),
        );
        Self::new_offline_with(snapshot, Arc::new(MonotonicTimeSource::new()), broker)
    }

    pub fn supervisor_state(&self) -> SupervisorState {
        self.supervision.supervisor_state()
    }

    pub fn time_source(&self) -> &Arc<dyn TimeSource> {
        &self.time_source
    }

    pub fn begin_start(&self) {
        self.supervision.begin_start();
    }

    pub fn mark_ready(&self, now_ms: u64) {
        self.supervision.mark_ready(now_ms);
    }

    pub fn report_owner_failure(&self, error: String, now_ms: u64) {
        self.supervision.report_owner_failure(error, now_ms);
    }

    pub fn tick(&self, now_ms: u64) {
        self.supervision.tick(now_ms);
    }

    pub fn update_snapshot(&self, snapshot: CompositorSnapshot) -> Result<(), StaleUpdateError> {
        self.supervision.update_snapshot(snapshot)
    }

    pub fn set_reconnecting_generation(&self, generation: u64) {
        self.supervision.set_reconnecting_generation(generation);
    }

    pub fn reset_quarantine(&self) {
        self.supervision.reset_quarantine();
    }
}

impl CompositorAdapter for HyprlandCompositorBackend {
    fn current(&self) -> Arc<CompositorSnapshot> {
        self.supervision.rx.borrow().clone()
    }

    fn subscribe(&self) -> watch::Receiver<Arc<CompositorSnapshot>> {
        self.supervision.rx.clone()
    }

    fn command_broker(&self) -> Arc<CompositorCommandBroker> {
        self.supervision.broker.clone()
    }
}

impl Drop for HyprlandCompositorBackend {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Ok(mut guard) = self.handle.lock()
            && let Some(handle) = guard.take()
        {
            let _ = handle.join();
        }
    }
}

type HyprlandStateTuple = (
    Vec<HyprlandClientDto>,
    Vec<HyprlandWorkspaceDto>,
    Vec<HyprlandMonitorDto>,
    Option<u64>,
);

fn fetch_full_hyprland_state(cmd_socket_path: &Path) -> Result<HyprlandStateTuple> {
    let clients_str = query_hyprland_socket(cmd_socket_path, "j/clients")?;
    let workspaces_str = query_hyprland_socket(cmd_socket_path, "j/workspaces")?;
    let monitors_str = query_hyprland_socket(cmd_socket_path, "j/monitors")?;
    let active_str = query_hyprland_socket(cmd_socket_path, "j/activewindow").unwrap_or_default();

    let clients = parse_clients_json(&clients_str)?;
    let workspaces = parse_workspaces_json(&workspaces_str)?;
    let monitors = parse_monitors_json(&monitors_str)?;
    let active_window = parse_active_window_json(&active_str).ok().flatten();

    Ok((clients, workspaces, monitors, active_window))
}

fn run_hyprland_listener(
    tx: watch::Sender<Arc<CompositorSnapshot>>,
    stop_flag: Arc<AtomicBool>,
    supervisor: Arc<Mutex<DomainSupervisor>>,
    time_source: Arc<dyn TimeSource>,
    broker: Arc<CompositorCommandBroker>,
) {
    let mut owner_generation = 0u64;
    let mut revision = 0u64;

    while !stop_flag.load(Ordering::Relaxed) {
        let now_ms = time_source.now_ms();

        // 1. Clock-driven transition via tick
        apply_tick(&supervisor, now_ms);

        // 2. Check supervisor state
        let state = supervisor.lock().unwrap().state();
        match state {
            SupervisorState::Quarantined => {
                sleep_with_stop_flag(Duration::from_millis(100), &stop_flag);
                continue;
            }
            SupervisorState::Backoff { retry_at_ms, .. } => {
                let remaining_ms = retry_at_ms.saturating_sub(now_ms);
                let sleep_duration = Duration::from_millis(remaining_ms.clamp(1, 100));
                sleep_with_stop_flag(sleep_duration, &stop_flag);
                continue;
            }
            SupervisorState::Starting | SupervisorState::Running => {}
            SupervisorState::Stopping | SupervisorState::Stopped => {
                break;
            }
        }

        // 3. Begin start
        owner_generation += 1;
        revision = 0;
        {
            supervisor.lock().unwrap().mark_starting();
            tracing::info!(target: "shilpo_profile", lifecycle = "starting", "hyprland supervisor transition");
        }
        broker.set_installed_generation(owner_generation);
        broker.record_restart();

        let previous = tx.borrow().clone();
        let mut connecting = (*previous).clone();
        revision = revision.saturating_add(1);
        connecting.version = DomainVersion::new(owner_generation, revision);
        connecting.connection = DomainLifecycle::Connecting;
        connecting.capabilities = hyprland_capabilities(DomainLifecycle::Connecting);
        connecting.last_error = None;
        let connecting = Arc::new(connecting);
        if broker.observe_snapshot(connecting.clone()).is_ok() {
            let _ = tx.send(connecting);
        }

        let socket_dir = match resolve_hyprland_socket_dir() {
            Some(dir) => dir,
            None => {
                let err_msg =
                    "HYPRLAND_INSTANCE_SIGNATURE is not set or socket directory does not exist"
                        .to_string();
                tracing::error!(%err_msg, "hyprland supervisor start failed");
                let now_ms = time_source.now_ms();
                record_supervisor_failure(
                    &supervisor,
                    &broker,
                    &tx,
                    owner_generation,
                    &mut revision,
                    err_msg,
                    now_ms,
                    &HyprlandCapabilityProvider,
                );
                continue;
            }
        };

        let cmd_socket = socket_dir.join(".socket.sock");
        let event_socket = socket_dir.join(".socket2.sock");

        // Initial full query
        let (clients, workspaces, monitors, active_window) =
            match fetch_full_hyprland_state(&cmd_socket) {
                Ok(state) => state,
                Err(err) => {
                    let err_msg = format!("Failed to query initial Hyprland state: {err}");
                    tracing::error!(%err_msg, "hyprland supervisor start failed");
                    let now_ms = time_source.now_ms();
                    record_supervisor_failure(
                        &supervisor,
                        &broker,
                        &tx,
                        owner_generation,
                        &mut revision,
                        err_msg,
                        now_ms,
                        &HyprlandCapabilityProvider,
                    );
                    continue;
                }
            };

        let event_stream = match UnixStream::connect(&event_socket) {
            Ok(s) => s,
            Err(err) => {
                let err_msg = format!("Failed to connect to Hyprland event socket: {err}");
                tracing::error!(%err_msg, "hyprland supervisor start failed");
                let now_ms = time_source.now_ms();
                record_supervisor_failure(
                    &supervisor,
                    &broker,
                    &tx,
                    owner_generation,
                    &mut revision,
                    err_msg,
                    now_ms,
                    &HyprlandCapabilityProvider,
                );
                continue;
            }
        };

        if event_stream
            .set_read_timeout(Some(Duration::from_millis(200)))
            .is_err()
        {
            let err_msg = "Failed to set read timeout on Hyprland event socket".to_string();
            let now_ms = time_source.now_ms();
            record_supervisor_failure(
                &supervisor,
                &broker,
                &tx,
                owner_generation,
                &mut revision,
                err_msg,
                now_ms,
                &HyprlandCapabilityProvider,
            );
            continue;
        }

        // Mark ready
        {
            let now_ms = time_source.now_ms();
            supervisor.lock().unwrap().mark_running(now_ms);
            tracing::info!(target: "shilpo_profile", lifecycle = "ready", "hyprland supervisor transition");
        }

        let mut urgent_windows: HashSet<u64> = HashSet::new();
        let mut current_active_addr = active_window;

        // Publish initial ready snapshot
        revision = revision.saturating_add(1);
        let snap = build_hyprland_snapshot(
            DomainVersion::new(owner_generation, revision),
            DomainLifecycle::Ready,
            &clients,
            &workspaces,
            &monitors,
            active_window,
            &urgent_windows,
            None,
        );
        let snap_arc = Arc::new(snap);
        if broker.observe_snapshot(snap_arc.clone()).is_ok() {
            let _ = tx.send(snap_arc);
        }

        let mut reader = BufReader::new(event_stream);
        let mut line = String::new();

        while !stop_flag.load(Ordering::Relaxed) {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    let err_msg = "Hyprland event socket EOF reached".to_string();
                    let now_ms = time_source.now_ms();
                    record_supervisor_failure(
                        &supervisor,
                        &broker,
                        &tx,
                        owner_generation,
                        &mut revision,
                        err_msg,
                        now_ms,
                        &HyprlandCapabilityProvider,
                    );
                    break;
                }
                Ok(_) => {
                    let Some(event) = parse_event_line(&line) else {
                        continue;
                    };

                    apply_urgent_event(&mut urgent_windows, &event);

                    // Check if event requires refreshing snapshot state. Hyprland co-emits
                    // activewindowv2/windowtitlev2 whenever the focused window's title text
                    // changes, not just on a real focus change (observed at 5/sec from a
                    // terminal with an animated title) -- treating every title update as
                    // needing a full clients/workspaces/monitors requery pegged a CPU core.
                    // Only refresh on ActiveWindowV2 when the address actually changed, and
                    // never refresh on a bare title change.
                    let needs_refresh = match &event {
                        HyprlandEvent::WorkspaceV2 { .. }
                        | HyprlandEvent::OpenWindow { .. }
                        | HyprlandEvent::CloseWindow { .. }
                        | HyprlandEvent::MoveWindowV2 { .. }
                        | HyprlandEvent::Urgent { .. }
                        | HyprlandEvent::FocusedMon { .. }
                        | HyprlandEvent::CreateWorkspaceV2 { .. }
                        | HyprlandEvent::DestroyWorkspaceV2 { .. } => true,
                        HyprlandEvent::ActiveWindowV2 { address } => {
                            *address != current_active_addr
                        }
                        _ => false,
                    };

                    if needs_refresh {
                        if let Ok((new_clients, new_workspaces, new_monitors, new_active)) =
                            fetch_full_hyprland_state(&cmd_socket)
                        {
                            current_active_addr = new_active;
                            revision = revision.saturating_add(1);
                            let snap = build_hyprland_snapshot(
                                DomainVersion::new(owner_generation, revision),
                                DomainLifecycle::Ready,
                                &new_clients,
                                &new_workspaces,
                                &new_monitors,
                                new_active,
                                &urgent_windows,
                                None,
                            );
                            let snap_arc = Arc::new(snap);
                            if broker.observe_snapshot(snap_arc.clone()).is_ok() {
                                let _ = tx.send(snap_arc);
                            }
                        } else {
                            // Non-fatal if query fails transiently; subsequent events will retry
                        }
                    }
                }
                Err(err)
                    if err.kind() == std::io::ErrorKind::WouldBlock
                        || err.kind() == std::io::ErrorKind::TimedOut =>
                {
                    continue;
                }
                Err(err) => {
                    let err_msg = format!("Hyprland event socket read error: {err}");
                    let now_ms = time_source.now_ms();
                    record_supervisor_failure(
                        &supervisor,
                        &broker,
                        &tx,
                        owner_generation,
                        &mut revision,
                        err_msg,
                        now_ms,
                        &HyprlandCapabilityProvider,
                    );
                    break;
                }
            }
        }
    }

    publish_reconnecting(
        &tx,
        &broker,
        owner_generation,
        &mut revision,
        Some("Hyprland listener thread stopped".to_string()),
        &HyprlandCapabilityProvider,
    );
}

// ---------------------------------------------------------------------------
// Unit Tests (Hermetic ADR-0006 invariant 8)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_address() {
        assert_eq!(parse_hex_address("0x55f8abcd"), Some(0x55f8abcd));
        assert_eq!(parse_hex_address("55f8abcd"), Some(0x55f8abcd));
        assert_eq!(parse_hex_address("0x0"), Some(0));
        assert_eq!(parse_hex_address(""), None);
        assert_eq!(parse_hex_address("invalid_hex"), None);
    }

    #[test]
    fn test_format_hex_address() {
        assert_eq!(format_hex_address(0x55f8abcd), "0x55f8abcd");
        assert_eq!(format_hex_address(0), "0x0");
    }

    #[test]
    fn test_parse_event_lines() {
        assert_eq!(
            parse_event_line("workspacev2>>1,1\n"),
            Some(HyprlandEvent::WorkspaceV2 {
                id: 1,
                name: "1".to_string()
            })
        );
        assert_eq!(
            parse_event_line("openwindow>>0x55f8abcd,1,kitty,kitty-terminal\n"),
            Some(HyprlandEvent::OpenWindow {
                address: 0x55f8abcd,
                workspace: "1".to_string(),
                class: "kitty".to_string(),
                title: "kitty-terminal".to_string(),
            })
        );
        assert_eq!(
            parse_event_line("closewindow>>0x55f8abcd\n"),
            Some(HyprlandEvent::CloseWindow {
                address: 0x55f8abcd
            })
        );
        assert_eq!(
            parse_event_line("activewindowv2>>0x55f8abcd\n"),
            Some(HyprlandEvent::ActiveWindowV2 {
                address: Some(0x55f8abcd)
            })
        );
        assert_eq!(
            parse_event_line("activewindow>>,\n"),
            Some(HyprlandEvent::ActiveWindowV2 { address: None })
        );
        assert_eq!(
            parse_event_line("movewindowv2>>0x55f8abcd,2,2\n"),
            Some(HyprlandEvent::MoveWindowV2 {
                address: 0x55f8abcd,
                workspace_id: 2,
                workspace_name: "2".to_string(),
            })
        );
        assert_eq!(
            parse_event_line("urgent>>0x55f8abcd\n"),
            Some(HyprlandEvent::Urgent {
                address: 0x55f8abcd
            })
        );
        assert_eq!(
            parse_event_line("windowtitlev2>>0x55f8abcd,New Title\n"),
            Some(HyprlandEvent::WindowTitleV2 {
                address: 0x55f8abcd,
                title: "New Title".to_string()
            })
        );
        assert_eq!(
            parse_event_line("focusedmon>>DP-1,1\n"),
            Some(HyprlandEvent::FocusedMon {
                monitor_name: "DP-1".to_string(),
                workspace_name: "1".to_string()
            })
        );
        assert_eq!(
            parse_event_line("createworkspacev2>>2,2\n"),
            Some(HyprlandEvent::CreateWorkspaceV2 {
                id: 2,
                name: "2".to_string()
            })
        );
        assert_eq!(
            parse_event_line("destroyworkspacev2>>2,2\n"),
            Some(HyprlandEvent::DestroyWorkspaceV2 {
                id: 2,
                name: "2".to_string()
            })
        );
        assert_eq!(
            parse_event_line("unknown_event>>some_data\n"),
            Some(HyprlandEvent::Other {
                name: "unknown_event".to_string(),
                data: "some_data".to_string()
            })
        );
        assert_eq!(parse_event_line(""), None);
        assert_eq!(parse_event_line("malformed_line_no_delimiter"), None);
    }

    #[test]
    fn test_parse_clients_json() {
        let json = r#"[
            {
                "address": "0x55f81234abcd",
                "title": "Alacritty",
                "class": "Alacritty",
                "workspace": { "id": 1, "name": "1" },
                "floating": false,
                "focusHistoryID": 0,
                "at": [100, 200],
                "size": [800, 600],
                "hidden": false
            },
            {
                "address": "invalid_address",
                "title": "Bad",
                "class": "Bad",
                "workspace": { "id": 1, "name": "1" }
            }
        ]"#;

        let clients = parse_clients_json(json).expect("valid clients json");
        assert_eq!(clients.len(), 2);
        assert_eq!(clients[0].address, "0x55f81234abcd");
        assert_eq!(clients[0].title, "Alacritty");
    }

    #[test]
    fn test_parse_workspaces_with_special_scratchpad() {
        let json = r#"[
            {
                "id": 1,
                "name": "1",
                "monitor": "DP-1",
                "windows": 2,
                "hasfullscreen": false,
                "lastwindow": "0x55f81234abcd"
            },
            {
                "id": -99,
                "name": "special:scratchpad",
                "monitor": "DP-1",
                "windows": 1,
                "hasfullscreen": false,
                "lastwindow": "0x55f899999999"
            }
        ]"#;

        let workspaces = parse_workspaces_json(json).expect("valid workspaces json");
        assert_eq!(workspaces.len(), 2);

        let snap = build_hyprland_snapshot(
            DomainVersion::new(1, 0),
            DomainLifecycle::Ready,
            &[],
            &workspaces,
            &[],
            None,
            &HashSet::new(),
            None,
        );

        // Standard workspaces should only have positive IDs. Workspace 1 is occupied, so a
        // synthetic empty workspace 2 is appended (mirrors Niri's always-one-trailing-empty
        // convention -- see build_hyprland_snapshot).
        assert_eq!(snap.workspaces.len(), 2);
        assert_eq!(snap.workspaces[0].id, 1);
        assert!(snap.workspaces[0].active_window_id.is_some());
        assert_eq!(snap.workspaces[1].id, 2);
        assert!(snap.workspaces[1].active_window_id.is_none());

        // Special workspaces should be populated in extras
        match snap.extras {
            CompositorExtras::Hyprland(extras) => {
                assert_eq!(extras.special_workspaces.len(), 1);
                assert_eq!(extras.special_workspaces[0].id, -99);
                assert_eq!(extras.special_workspaces[0].name, "special:scratchpad");
                assert_eq!(
                    extras.special_workspaces[0].monitor.as_deref(),
                    Some("DP-1")
                );
            }
            _ => panic!("expected Hyprland extras"),
        }
    }

    #[test]
    fn test_encode_hyprland_commands() {
        assert_eq!(
            encode_hyprland_command(&CompositorCommand::FocusWorkspace(3)).unwrap(),
            "dispatch hl.dsp.focus({workspace = 3})"
        );
        assert_eq!(
            encode_hyprland_command(&CompositorCommand::FocusWindow(0x55f8abcd)).unwrap(),
            "dispatch hl.dsp.focus({window = \"address:0x55f8abcd\"})"
        );
        assert_eq!(
            encode_hyprland_command(&CompositorCommand::FocusPreviousWindow).unwrap(),
            "dispatch hl.dsp.focus({last = true})"
        );
        assert_eq!(
            encode_hyprland_command(&CompositorCommand::CloseWindow(0x55f8abcd)).unwrap(),
            "dispatch hl.dsp.window.close({window = \"address:0x55f8abcd\"})"
        );
        assert_eq!(
            encode_hyprland_command(&CompositorCommand::CreateWorkspace).unwrap(),
            "dispatch hl.dsp.focus({workspace = \"empty\"})"
        );
        assert_eq!(
            encode_hyprland_command(&CompositorCommand::MoveWindowToWorkspace {
                window_id: 0x55f8abcd,
                workspace_id: 4,
            })
            .unwrap(),
            "dispatch hl.dsp.window.move({workspace = 4, follow = false, window = \"address:0x55f8abcd\"})"
        );
    }

    #[test]
    fn test_apply_urgent_event_tracks_and_clears() {
        let mut urgent: HashSet<u64> = HashSet::new();

        apply_urgent_event(&mut urgent, &HyprlandEvent::Urgent { address: 0x1 });
        assert!(urgent.contains(&0x1));

        // Focusing the urgent window clears it.
        apply_urgent_event(
            &mut urgent,
            &HyprlandEvent::ActiveWindowV2 { address: Some(0x1) },
        );
        assert!(!urgent.contains(&0x1));

        apply_urgent_event(&mut urgent, &HyprlandEvent::Urgent { address: 0x2 });
        assert!(urgent.contains(&0x2));

        // Closing an urgent window clears it too.
        apply_urgent_event(&mut urgent, &HyprlandEvent::CloseWindow { address: 0x2 });
        assert!(!urgent.contains(&0x2));

        // Unrelated events leave urgency untouched.
        apply_urgent_event(&mut urgent, &HyprlandEvent::Urgent { address: 0x3 });
        apply_urgent_event(
            &mut urgent,
            &HyprlandEvent::ActiveWindowV2 { address: None },
        );
        assert!(urgent.contains(&0x3));
    }

    #[test]
    fn test_build_hyprland_snapshot_marks_urgent_windows() {
        let json = r#"[
            {
                "address": "0x1",
                "title": "a",
                "class": "a",
                "workspace": { "id": 1, "name": "1" }
            },
            {
                "address": "0x2",
                "title": "b",
                "class": "b",
                "workspace": { "id": 1, "name": "1" }
            }
        ]"#;
        let clients = parse_clients_json(json).expect("valid clients json");

        let mut urgent_windows = HashSet::new();
        urgent_windows.insert(0x1u64);

        let snap = build_hyprland_snapshot(
            DomainVersion::new(1, 0),
            DomainLifecycle::Ready,
            &clients,
            &[],
            &[],
            None,
            &urgent_windows,
            None,
        );

        let urgent_window = snap.windows.iter().find(|w| w.id == 0x1).unwrap();
        let calm_window = snap.windows.iter().find(|w| w.id == 0x2).unwrap();
        assert!(urgent_window.is_urgent);
        assert!(!calm_window.is_urgent);
    }

    #[test]
    fn test_hyprland_capabilities() {
        let unavail = hyprland_capabilities(DomainLifecycle::Unavailable);
        assert_eq!(unavail.window_identity, WindowIdentity::None);
        assert!(!unavail.can_create_workspace);
        assert!(!unavail.can_move_window);
        assert!(!unavail.can_focus_window);
        assert!(!unavail.can_focus_workspace);
        assert!(!unavail.can_close_window);

        let ready = hyprland_capabilities(DomainLifecycle::Ready);
        assert_eq!(ready.window_identity, WindowIdentity::Exact);
        assert!(ready.can_create_workspace);
        assert!(ready.can_move_window);
        assert!(ready.can_focus_window);
        assert!(ready.can_focus_workspace);
        assert!(ready.can_close_window);
    }
}
