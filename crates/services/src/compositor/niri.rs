use super::{
    BrokerOptions, CancellationReason, CompositorAdapter, CompositorCapabilities,
    CompositorCommand, CompositorCommandBroker, CompositorCommandError, CompositorConnection,
    CompositorOutput, CompositorSnapshot, WindowInfo, WorkspaceInfo,
    broker::{CommandCancellation, StreamCancelHandle, create_stream_cancel_handle},
};
use anyhow::Result;
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
    broker: Arc<CompositorCommandBroker>,
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

        let broker =
            CompositorCommandBroker::new(BrokerOptions::default(), Box::new(execute_niri_command));

        let tx_clone = tx.clone();
        let stop_clone = stop_flag.clone();
        let broker_clone = broker.clone();

        let handle = thread::spawn(move || {
            run_niri_listener(tx_clone, stop_clone, broker_clone);
        });

        Arc::new(Self {
            tx,
            rx,
            stop_flag,
            broker,
            handle: Mutex::new(Some(handle)),
        })
    }

    /// Construct offline instance for testing with specified initial snapshot.
    pub fn new_offline(snapshot: CompositorSnapshot) -> Arc<Self> {
        let (tx, rx) = watch::channel(Arc::new(snapshot.clone()));
        let stop_flag = Arc::new(AtomicBool::new(true));

        let broker = CompositorCommandBroker::new(
            BrokerOptions::default(),
            Box::new(|_cmd, _timeout, _cancel, _register| Ok(())),
        );
        broker.update_connection(snapshot.connection, snapshot.capabilities);

        Arc::new(Self {
            tx,
            rx,
            stop_flag,
            broker,
            handle: Mutex::new(None),
        })
    }

    pub fn update_snapshot(&self, snapshot: CompositorSnapshot) {
        self.broker
            .update_connection(snapshot.connection.clone(), snapshot.capabilities.clone());
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

    fn command_broker(&self) -> Arc<CompositorCommandBroker> {
        self.broker.clone()
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

fn execute_niri_command(
    command: &CompositorCommand,
    timeout: Duration,
    cancel: Arc<CommandCancellation>,
    register_cancel: &dyn Fn(Arc<dyn StreamCancelHandle>),
) -> Result<(), CompositorCommandError> {
    let socket_path = match resolve_niri_socket_path() {
        Some(p) => p,
        None => {
            return Err(CompositorCommandError::Unavailable {
                state: CompositorConnection::Stopped,
            });
        }
    };

    let stream = match UnixStream::connect(&socket_path) {
        Ok(s) => s,
        Err(err) => {
            return Err(CompositorCommandError::Transport {
                message: format!(
                    "failed to connect to socket {}: {err}",
                    socket_path.display()
                ),
            });
        }
    };

    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));

    let cancel_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            return Err(CompositorCommandError::Transport {
                message: e.to_string(),
            });
        }
    };

    register_cancel(create_stream_cancel_handle(cancel_stream));

    let req = match command {
        CompositorCommand::FocusWorkspace(id) => {
            Request::Action(niri_ipc::Action::FocusWorkspace {
                reference: niri_ipc::WorkspaceReferenceArg::Id(*id),
            })
        }
        CompositorCommand::FocusWindow(id) => {
            Request::Action(niri_ipc::Action::FocusWindow { id: *id })
        }
        CompositorCommand::FocusPreviousWindow => {
            Request::Action(niri_ipc::Action::FocusWindowPrevious {})
        }
        CompositorCommand::CreateWorkspace { name: None } => {
            Request::Action(niri_ipc::Action::FocusWorkspaceDown {})
        }
        CompositorCommand::CreateWorkspace { name: Some(_) } => {
            return Err(CompositorCommandError::Unsupported);
        }
        CompositorCommand::MoveWindowToWorkspace {
            window_id,
            workspace_id,
        } => Request::Action(niri_ipc::Action::MoveWindowToWorkspace {
            window_id: Some(*window_id),
            reference: niri_ipc::WorkspaceReferenceArg::Id(*workspace_id),
            focus: true,
        }),
    };

    let mut json = match serde_json::to_string(&req) {
        Ok(j) => j,
        Err(err) => {
            return Err(CompositorCommandError::Transport {
                message: err.to_string(),
            });
        }
    };
    json.push('\n');

    let mut stream_writer = stream;
    if let Err(err) = stream_writer.write_all(json.as_bytes()) {
        if cancel.is_cancelled() {
            return Err(CompositorCommandError::Cancelled {
                reason: cancel.reason().unwrap_or(CancellationReason::User),
            });
        }
        if err.kind() == std::io::ErrorKind::TimedOut
            || err.kind() == std::io::ErrorKind::WouldBlock
        {
            return Err(CompositorCommandError::Timeout { duration: timeout });
        }
        return Err(CompositorCommandError::Transport {
            message: err.to_string(),
        });
    }

    let mut reader = BufReader::new(&stream_writer);
    let mut line = String::new();
    if let Err(err) = reader.read_line(&mut line) {
        if cancel.is_cancelled() {
            return Err(CompositorCommandError::Cancelled {
                reason: cancel.reason().unwrap_or(CancellationReason::User),
            });
        }
        if err.kind() == std::io::ErrorKind::TimedOut
            || err.kind() == std::io::ErrorKind::WouldBlock
        {
            return Err(CompositorCommandError::Timeout { duration: timeout });
        }
        return Err(CompositorCommandError::Transport {
            message: err.to_string(),
        });
    }

    let reply: Reply = match serde_json::from_str(&line) {
        Ok(r) => r,
        Err(err) => {
            return Err(CompositorCommandError::Transport {
                message: format!("malformed reply from niri: {err}"),
            });
        }
    };

    match reply {
        Ok(Response::Handled) => Ok(()),
        Ok(resp) => Err(CompositorCommandError::BackendRejected {
            message: format!("unexpected niri response: {resp:?}"),
        }),
        Err(err) => Err(CompositorCommandError::BackendRejected {
            message: err.to_string(),
        }),
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
    broker: &CompositorCommandBroker,
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
        broker.update_connection(current.connection.clone(), current.capabilities.clone());
        let _ = tx.send(Arc::new(current));
    }
}

fn run_niri_listener(
    tx: watch::Sender<Arc<CompositorSnapshot>>,
    stop_flag: Arc<AtomicBool>,
    broker: Arc<CompositorCommandBroker>,
) {
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
                    &broker,
                    &mut revision,
                    attempt,
                    Some("Neither NIRI_SOCKET nor NIRI_SOCKET_PATH is set".to_string()),
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
                    &broker,
                    &mut revision,
                    attempt,
                    Some(format!("Failed to query Niri outputs: {err}")),
                );
                sleep_with_stop_flag(backoff, &stop_flag);
                backoff = (backoff * 2).min(max_backoff);
                continue;
            }
        };

        let mut event_stream = match UnixStream::connect(&socket_path) {
            Ok(s) => s,
            Err(err) => {
                publish_reconnecting(
                    &tx,
                    &broker,
                    &mut revision,
                    attempt,
                    Some(format!("Failed to connect to Niri event socket: {err}")),
                );
                sleep_with_stop_flag(backoff, &stop_flag);
                backoff = (backoff * 2).min(max_backoff);
                continue;
            }
        };

        let req_json = match serde_json::to_string(&Request::EventStream) {
            Ok(mut j) => {
                j.push('\n');
                j
            }
            Err(err) => {
                publish_reconnecting(
                    &tx,
                    &broker,
                    &mut revision,
                    attempt,
                    Some(format!("Failed to serialize EventStream request: {err}")),
                );
                sleep_with_stop_flag(backoff, &stop_flag);
                backoff = (backoff * 2).min(max_backoff);
                continue;
            }
        };

        if let Err(err) = event_stream.write_all(req_json.as_bytes()) {
            publish_reconnecting(
                &tx,
                &broker,
                &mut revision,
                attempt,
                Some(format!("Failed to send EventStream request: {err}")),
            );
            sleep_with_stop_flag(backoff, &stop_flag);
            backoff = (backoff * 2).min(max_backoff);
            continue;
        }

        if event_stream
            .set_read_timeout(Some(Duration::from_millis(200)))
            .is_err()
        {
            publish_reconnecting(
                &tx,
                &broker,
                &mut revision,
                attempt,
                Some("Failed to set read timeout on Niri socket".to_string()),
            );
            sleep_with_stop_flag(backoff, &stop_flag);
            backoff = (backoff * 2).min(max_backoff);
            continue;
        }

        let mut state = EventStreamState::default();
        let mut reader = BufReader::new(event_stream);
        let mut line = String::new();
        let mut initial_sync = true;
        let mut current_outputs = outputs;
        let mut warning_count = 0u32;
        let mut rate_limit_timer = std::time::Instant::now();

        while !stop_flag.load(Ordering::Relaxed) {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    publish_reconnecting(
                        &tx,
                        &broker,
                        &mut revision,
                        attempt,
                        Some("Niri socket EOF reached".to_string()),
                    );
                    break;
                }
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

                    if refresh_outputs_needed
                        && let Ok(new_outputs) = query_outputs_from_socket_path(&socket_path)
                    {
                        current_outputs = new_outputs;
                    }

                    if !initial_sync || initial_sync_boundary {
                        if initial_sync {
                            initial_sync = false;
                            attempt = 0;
                            backoff = Duration::from_millis(250);
                        }

                        publish_snapshot_from_state(
                            &tx,
                            &broker,
                            &mut revision,
                            CompositorConnection::Ready,
                            &current_outputs,
                            &state,
                        );
                    }
                }
                Err(err)
                    if err.kind() == std::io::ErrorKind::WouldBlock
                        || err.kind() == std::io::ErrorKind::TimedOut =>
                {
                    continue;
                }
                Err(err) => {
                    publish_reconnecting(
                        &tx,
                        &broker,
                        &mut revision,
                        attempt,
                        Some(format!("Niri socket read error: {err}")),
                    );
                    break;
                }
            }
        }

        sleep_with_stop_flag(backoff, &stop_flag);
        backoff = (backoff * 2).min(max_backoff);
    }

    publish_reconnecting(
        &tx,
        &broker,
        &mut revision,
        attempt,
        Some("Niri listener thread stopped".to_string()),
    );
}

fn publish_snapshot_from_state(
    tx: &watch::Sender<Arc<CompositorSnapshot>>,
    broker: &CompositorCommandBroker,
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
        broker.update_connection(
            new_snapshot.connection.clone(),
            new_snapshot.capabilities.clone(),
        );
        let _ = tx.send(Arc::new(new_snapshot));
    }
}
