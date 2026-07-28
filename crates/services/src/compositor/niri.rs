use super::{
    CompositorAdapter, CompositorCapabilities, CompositorCommand, CompositorConnection,
    CompositorOutput, CompositorSnapshot, WindowInfo, WorkspaceInfo,
};
use anyhow::{Context, Result};
use niri_ipc::{
    Event, Reply, Request, Response,
    socket::Socket,
    state::{EventStreamState, EventStreamStatePart},
};
use std::{
    env,
    io::{BufRead, BufReader, Write as _},
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use tokio::sync::watch;

/// Resolves `NIRI_SOCKET`, then `NIRI_SOCKET_PATH`.
pub fn resolve_niri_socket_path() -> Option<PathBuf> {
    env::var_os("NIRI_SOCKET")
        .or_else(|| env::var_os("NIRI_SOCKET_PATH"))
        .map(PathBuf::from)
}

/// Niri Compositor IPC service for publishing revisioned, deterministic snapshots and managing reconnection.
pub struct NiriCompositorService {
    tx: watch::Sender<Arc<CompositorSnapshot>>,
    rx: watch::Receiver<Arc<CompositorSnapshot>>,
    stop_flag: Arc<AtomicBool>,
    handle: Mutex<Option<thread::JoinHandle<()>>>,
}

impl NiriCompositorService {
    /// Non-failing constructor. Publishes `Connecting` immediately and starts background listener thread.
    pub fn new() -> Arc<Self> {
        let initial = CompositorSnapshot {
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
        };
        let (tx, rx) = watch::channel(Arc::new(initial));
        let stop_flag = Arc::new(AtomicBool::new(false));

        let tx_clone = tx.clone();
        let stop_clone = stop_flag.clone();

        let handle = thread::spawn(move || {
            run_niri_listener(tx_clone, stop_clone);
        });

        Arc::new(Self {
            tx,
            rx,
            stop_flag,
            handle: Mutex::new(Some(handle)),
        })
    }

    /// Construct offline instance for testing with specified initial snapshot.
    pub fn new_offline(snapshot: CompositorSnapshot) -> Arc<Self> {
        let (tx, rx) = watch::channel(Arc::new(snapshot));
        let stop_flag = Arc::new(AtomicBool::new(true));
        Arc::new(Self {
            tx,
            rx,
            stop_flag,
            handle: Mutex::new(None),
        })
    }

    pub fn update_snapshot(&self, snapshot: CompositorSnapshot) {
        let _ = self.tx.send(Arc::new(snapshot));
    }
}

impl CompositorAdapter for NiriCompositorService {
    fn current(&self) -> Arc<CompositorSnapshot> {
        self.rx.borrow().clone()
    }

    fn subscribe(&self) -> watch::Receiver<Arc<CompositorSnapshot>> {
        self.rx.clone()
    }

    fn execute(&self, command: CompositorCommand) -> Result<()> {
        let current = self.current();
        if !current.connection.is_ready() {
            anyhow::bail!(
                "Compositor is unavailable: status is {:?}",
                current.connection
            );
        }

        let socket_path = resolve_niri_socket_path()
            .ok_or_else(|| anyhow::anyhow!("Niri socket path not set"))?;

        let mut socket =
            Socket::connect_to(&socket_path).context("Failed to connect to Niri IPC socket")?;

        let req = match command {
            CompositorCommand::FocusWorkspace(id) => {
                Request::Action(niri_ipc::Action::FocusWorkspace {
                    reference: niri_ipc::WorkspaceReferenceArg::Id(id),
                })
            }
            CompositorCommand::FocusWindow(id) => {
                Request::Action(niri_ipc::Action::FocusWindow { id })
            }
            CompositorCommand::FocusPreviousWindow => {
                Request::Action(niri_ipc::Action::FocusWindowPrevious {})
            }
            CompositorCommand::CreateWorkspace { name: None } => {
                Request::Action(niri_ipc::Action::FocusWorkspaceDown {})
            }
            CompositorCommand::CreateWorkspace { name: Some(name) } => {
                anyhow::bail!("Niri does not support naming workspaces through IPC: {name}");
            }
            CompositorCommand::MoveWindowToWorkspace {
                window_id,
                workspace_id,
            } => Request::Action(niri_ipc::Action::MoveWindowToWorkspace {
                window_id: Some(window_id),
                reference: niri_ipc::WorkspaceReferenceArg::Id(workspace_id),
                focus: true,
            }),
        };

        match socket.send(req) {
            Ok(Ok(Response::Handled)) => Ok(()),
            Ok(Ok(resp)) => {
                anyhow::bail!("Niri action returned unexpected response: {resp:?}");
            }
            Ok(Err(err)) => {
                anyhow::bail!("Niri action failed: {err}");
            }
            Err(err) => Err(err).context("Failed to send Niri action"),
        }
    }
}

impl Drop for NiriCompositorService {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Ok(mut guard) = self.handle.lock()
            && let Some(handle) = guard.take()
        {
            let _ = handle.join();
        }
    }
}

fn sleep_with_stop_flag(duration: Duration, stop_flag: &AtomicBool) {
    let start = std::time::Instant::now();
    while start.elapsed() < duration {
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn query_outputs_from_socket_path(socket_path: &PathBuf) -> Result<Vec<CompositorOutput>> {
    let mut socket = Socket::connect_to(socket_path)?;
    let resp = socket.send(Request::Outputs)?;
    match resp {
        Ok(Response::Outputs(outputs)) => {
            let mut list: Vec<CompositorOutput> = outputs
                .into_iter()
                .map(|(name, o)| CompositorOutput {
                    name,
                    make: Some(o.make),
                    model: Some(o.model),
                    logical_position: o.logical.as_ref().map(|l| (l.x, l.y)).unwrap_or((0, 0)),
                    logical_size: o
                        .logical
                        .as_ref()
                        .map(|l| (l.width, l.height))
                        .unwrap_or((0, 0)),
                    scale: o.logical.as_ref().map(|l| l.scale).unwrap_or(1.0),
                })
                .collect();
            list.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(list)
        }
        Ok(other) => anyhow::bail!("Niri returned an unexpected Outputs response: {other:?}"),
        Err(error) => anyhow::bail!("Niri failed to query outputs: {error}"),
    }
}

fn publish_reconnecting(
    tx: &watch::Sender<Arc<CompositorSnapshot>>,
    revision: &mut u64,
    attempt: u32,
    last_error: Option<String>,
) {
    let previous = tx.borrow().clone();
    let mut current = (*previous).clone();
    current.revision = previous.revision;
    current.connection = CompositorConnection::Reconnecting {
        attempt,
        last_error,
    };
    if *previous != current {
        current.revision = previous.revision.saturating_add(1);
        *revision = current.revision;
        let _ = tx.send(Arc::new(current));
    }
}

fn run_niri_listener(tx: watch::Sender<Arc<CompositorSnapshot>>, stop_flag: Arc<AtomicBool>) {
    let mut backoff = Duration::from_millis(250);
    let max_backoff = Duration::from_secs(5);
    let mut attempt = 0u32;
    let mut revision = 0u64;

    while !stop_flag.load(Ordering::Relaxed) {
        attempt += 1;

        let socket_path = match resolve_niri_socket_path() {
            Some(path) => path,
            None => {
                publish_reconnecting(
                    &tx,
                    &mut revision,
                    attempt,
                    Some("Neither NIRI_SOCKET nor NIRI_SOCKET_PATH is set".to_string()),
                );
                sleep_with_stop_flag(backoff, &stop_flag);
                backoff = (backoff * 2).min(max_backoff);
                continue;
            }
        };

        let stream = match UnixStream::connect(&socket_path) {
            Ok(s) => s,
            Err(err) => {
                publish_reconnecting(
                    &tx,
                    &mut revision,
                    attempt,
                    Some(format!(
                        "Failed to connect to Niri socket at {}: {}",
                        socket_path.display(),
                        err
                    )),
                );
                sleep_with_stop_flag(backoff, &stop_flag);
                backoff = (backoff * 2).min(max_backoff);
                continue;
            }
        };

        let outputs = match query_outputs_from_socket_path(&socket_path) {
            Ok(outputs) => outputs,
            Err(err) => {
                publish_reconnecting(
                    &tx,
                    &mut revision,
                    attempt,
                    Some(format!("Failed to query Niri outputs: {err}")),
                );
                sleep_with_stop_flag(backoff, &stop_flag);
                backoff = (backoff * 2).min(max_backoff);
                continue;
            }
        };

        if let Err(err) = stream.set_read_timeout(Some(Duration::from_millis(200))) {
            tracing::warn!(error = %err, "failed to set read timeout on Niri stream");
        }

        let mut reader = BufReader::new(stream);
        let request_json = match serde_json::to_string(&Request::EventStream) {
            Ok(json) => json + "\n",
            Err(err) => {
                publish_reconnecting(&tx, &mut revision, attempt, Some(err.to_string()));
                sleep_with_stop_flag(backoff, &stop_flag);
                backoff = (backoff * 2).min(max_backoff);
                continue;
            }
        };

        if let Err(err) = reader
            .get_mut()
            .write_all(request_json.as_bytes())
            .and_then(|_| reader.get_mut().flush())
        {
            publish_reconnecting(
                &tx,
                &mut revision,
                attempt,
                Some(format!("Handshake write error: {}", err)),
            );
            sleep_with_stop_flag(backoff, &stop_flag);
            backoff = (backoff * 2).min(max_backoff);
            continue;
        }

        let mut line = String::new();
        let mut handshake_ok = false;
        let mut timeout_count = 0;

        while !stop_flag.load(Ordering::Relaxed) {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let reply: Result<Reply, _> = serde_json::from_str(&line);
                    match reply {
                        Ok(Ok(_)) => {
                            handshake_ok = true;
                            break;
                        }
                        Ok(Err(e)) => {
                            publish_reconnecting(
                                &tx,
                                &mut revision,
                                attempt,
                                Some(format!("Niri refused EventStream: {}", e)),
                            );
                            break;
                        }
                        Err(e) => {
                            publish_reconnecting(
                                &tx,
                                &mut revision,
                                attempt,
                                Some(format!("Failed to parse handshake reply: {}", e)),
                            );
                            break;
                        }
                    }
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    timeout_count += 1;
                    if timeout_count > 15 {
                        publish_reconnecting(
                            &tx,
                            &mut revision,
                            attempt,
                            Some("Handshake read timeout".into()),
                        );
                        break;
                    }
                    continue;
                }
                Err(e) => {
                    publish_reconnecting(
                        &tx,
                        &mut revision,
                        attempt,
                        Some(format!("Handshake read error: {}", e)),
                    );
                    break;
                }
            }
        }

        if !handshake_ok {
            sleep_with_stop_flag(backoff, &stop_flag);
            backoff = (backoff * 2).min(max_backoff);
            continue;
        }

        reader.get_ref().shutdown(std::net::Shutdown::Write).ok();

        let mut state = EventStreamState::default();
        let mut current_outputs = outputs;
        let mut initial_sync = true;
        let initial_sync_deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut rate_limit_timer = std::time::Instant::now();
        let mut warning_count = 0u32;

        while !stop_flag.load(Ordering::Relaxed) {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let event: Event = match serde_json::from_str(&line) {
                        Ok(ev) => ev,
                        Err(err) => {
                            warning_count += 1;
                            if rate_limit_timer.elapsed() >= Duration::from_secs(5) {
                                tracing::warn!(
                                    count = warning_count,
                                    last_error = %err,
                                    "ignored malformed Niri event line(s)"
                                );
                                warning_count = 0;
                                rate_limit_timer = std::time::Instant::now();
                            }
                            continue;
                        }
                    };

                    let initial_sync_boundary = matches!(event, Event::ConfigLoaded { .. });
                    let refresh_outputs_needed = matches!(
                        event,
                        Event::WorkspacesChanged { .. } | Event::ConfigLoaded { .. }
                    );

                    state.apply(event);

                    if refresh_outputs_needed {
                        match query_outputs_from_socket_path(&socket_path) {
                            Ok(new_outputs) => current_outputs = new_outputs,
                            Err(err) => {
                                publish_reconnecting(
                                    &tx,
                                    &mut revision,
                                    attempt,
                                    Some(format!("Failed to refresh Niri outputs: {err}")),
                                );
                                break;
                            }
                        }
                    }

                    if !initial_sync || initial_sync_boundary {
                        if initial_sync {
                            initial_sync = false;
                            attempt = 0;
                            backoff = Duration::from_millis(250);
                        }

                        publish_snapshot_from_state(
                            &tx,
                            &mut revision,
                            CompositorConnection::Ready,
                            &current_outputs,
                            &state,
                        );
                    }
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    if initial_sync && std::time::Instant::now() >= initial_sync_deadline {
                        publish_reconnecting(
                            &tx,
                            &mut revision,
                            attempt,
                            Some("Timed out waiting for Niri initial state".into()),
                        );
                        break;
                    }
                    continue;
                }
                Err(err) => {
                    tracing::warn!(error = %err, "Niri event stream read error");
                    break;
                }
            }
        }

        if !stop_flag.load(Ordering::Relaxed) {
            publish_reconnecting(
                &tx,
                &mut revision,
                attempt,
                Some("Event stream disconnected".into()),
            );
            sleep_with_stop_flag(backoff, &stop_flag);
            backoff = (backoff * 2).min(max_backoff);
        }
    }
}

pub fn publish_snapshot_from_state(
    tx: &watch::Sender<Arc<CompositorSnapshot>>,
    revision: &mut u64,
    connection: CompositorConnection,
    outputs: &[CompositorOutput],
    state: &EventStreamState,
) {
    let mut workspaces: Vec<WorkspaceInfo> = state
        .workspaces
        .workspaces
        .values()
        .map(|w| WorkspaceInfo {
            id: w.id,
            name: w.name.clone(),
            idx: w.idx,
            is_active: w.is_active,
            is_focused: w.is_focused,
            is_urgent: w.is_urgent,
            output_name: w.output.clone(),
            active_window_id: w.active_window_id,
        })
        .collect();

    workspaces.sort_by(|a, b| {
        a.output_name
            .as_deref()
            .unwrap_or("")
            .cmp(b.output_name.as_deref().unwrap_or(""))
            .then_with(|| a.idx.cmp(&b.idx))
            .then_with(|| a.id.cmp(&b.id))
    });

    let mut windows: Vec<WindowInfo> = state
        .windows
        .windows
        .values()
        .map(|w| WindowInfo {
            id: w.id,
            title: w.title.clone(),
            app_id: w.app_id.clone(),
            workspace_id: w.workspace_id,
            is_focused: w.is_focused,
            is_floating: w.is_floating,
            is_urgent: w.is_urgent,
        })
        .collect();

    windows.sort_by_key(|w| w.id);

    let focused_workspace_id = workspaces.iter().find(|w| w.is_focused).map(|w| w.id);
    let focused_window_id = windows.iter().find(|w| w.is_focused).map(|w| w.id);
    let focused_output = workspaces
        .iter()
        .find(|w| w.is_focused)
        .and_then(|w| w.output_name.clone());

    let active_keyboard_layout = state
        .keyboard_layouts
        .keyboard_layouts
        .as_ref()
        .and_then(|kb| kb.names.get(kb.current_idx as usize).cloned());

    let mut sorted_outputs = outputs.to_vec();
    sorted_outputs.sort_by(|a, b| a.name.cmp(&b.name));

    let prev = tx.borrow().clone();
    let new_snapshot = CompositorSnapshot {
        revision: prev.revision,
        connection,
        capabilities: CompositorCapabilities::default(),
        outputs: sorted_outputs,
        workspaces,
        windows,
        focused_output,
        focused_workspace_id,
        focused_window_id,
        active_keyboard_layout,
    };

    if *prev != new_snapshot {
        let mut new_snapshot = new_snapshot;
        new_snapshot.revision = prev.revision.saturating_add(1);
        *revision = new_snapshot.revision;
        let _ = tx.send(Arc::new(new_snapshot));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_niri_service_non_failing_construction() {
        let service = NiriCompositorService::new();
        assert_eq!(
            service.current().connection,
            CompositorConnection::Connecting
        );
    }

    #[test]
    fn test_niri_service_offline_construction() {
        let snapshot = CompositorSnapshot {
            revision: 1,
            connection: CompositorConnection::Ready,
            capabilities: CompositorCapabilities::default(),
            outputs: vec![CompositorOutput {
                name: "DP-1".into(),
                make: Some("Dell".into()),
                model: Some("UltraSharp".into()),
                logical_position: (0, 0),
                logical_size: (1920, 1080),
                scale: 1.0,
            }],
            workspaces: vec![WorkspaceInfo {
                id: 1,
                name: Some("1".into()),
                idx: 1,
                is_active: true,
                is_focused: true,
                is_urgent: false,
                output_name: Some("DP-1".into()),
                active_window_id: None,
            }],
            windows: Vec::new(),
            focused_output: Some("DP-1".into()),
            focused_workspace_id: Some(1),
            focused_window_id: None,
            active_keyboard_layout: Some("us".into()),
        };
        let service = NiriCompositorService::new_offline(snapshot);
        assert_eq!(service.current().connection, CompositorConnection::Ready);
        assert_eq!(service.current().workspaces.len(), 1);
    }

    #[test]
    fn test_execute_fails_when_not_ready() {
        let service = NiriCompositorService::new();
        assert!(
            service
                .execute(CompositorCommand::FocusWorkspace(1))
                .is_err()
        );
    }

    #[test]
    fn identical_snapshot_payload_does_not_advance_revision() {
        let (tx, rx) = watch::channel(Arc::new(CompositorSnapshot::default()));
        let state = EventStreamState::default();
        let mut revision = 0;

        publish_snapshot_from_state(&tx, &mut revision, CompositorConnection::Ready, &[], &state);
        let first = rx.borrow().clone();

        publish_snapshot_from_state(&tx, &mut revision, CompositorConnection::Ready, &[], &state);
        let second = rx.borrow().clone();

        assert_eq!(first.revision, second.revision);
        assert_eq!(*first, *second);
    }
}
