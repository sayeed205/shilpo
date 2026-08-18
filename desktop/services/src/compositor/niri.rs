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

use anyhow::Result;
use niri_ipc::{
    Event, Reply, Request, Response,
    socket::Socket,
    state::{EventStreamState, EventStreamStatePart},
};
use tokio::sync::watch;

use super::{
    BrokerOptions, CompositorAdapter, CompositorCapabilities, CompositorCommand,
    CompositorCommandBroker, CompositorOutput, CompositorSnapshot, DomainLifecycle, DomainVersion,
    ExecutorAck, RejectionReason, StaleUpdateError, SupervisorState, WindowInfo, WorkspaceInfo,
    broker::{CommandCancellation, StreamCancelHandle, create_stream_cancel_handle},
};

use crate::domain::{
    FAILURE_WINDOW_MS, MonotonicTimeSource, QUARANTINE_FAILURES, STABLE_RESET_MS, TimeSource,
    reconnect_backoff_ms,
};

#[derive(Debug)]
struct CompositorSupervisorData {
    state: SupervisorState,
    failures_ms: Vec<u64>,
    stable_since_ms: Option<u64>,
}

/// Resolves `NIRI_SOCKET`, then `NIRI_SOCKET_PATH`, falling back to scanning `XDG_RUNTIME_DIR`.
pub fn resolve_niri_socket_path() -> Option<PathBuf> {
    env::var_os("NIRI_SOCKET")
        .or_else(|| env::var_os("NIRI_SOCKET_PATH"))
        .map(PathBuf::from)
        .or_else(|| {
            let runtime_dir = dirs::runtime_dir()?;
            let entries = std::fs::read_dir(&runtime_dir).ok()?;
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with("niri") && name_str.ends_with(".sock") {
                    return Some(entry.path());
                }
            }
            None
        })
}

/// Niri Compositor IPC service for publishing revisioned, deterministic snapshots and managing reconnection.
pub struct NiriCompositorService {
    tx: watch::Sender<Arc<CompositorSnapshot>>,
    rx: watch::Receiver<Arc<CompositorSnapshot>>,
    stop_flag: Arc<AtomicBool>,
    supervisor: Arc<Mutex<CompositorSupervisorData>>,
    time_source: Arc<dyn TimeSource>,
    broker: Arc<CompositorCommandBroker>,
    handle: Mutex<Option<thread::JoinHandle<()>>>,
}

impl NiriCompositorService {
    /// Non-failing constructor. Publishes `Unavailable` immediately and starts background listener thread.
    pub fn new() -> Arc<Self> {
        let initial = CompositorSnapshot {
            version: DomainVersion::ZERO,
            connection: DomainLifecycle::Unavailable,
            capabilities: CompositorCapabilities::default(),
            outputs: Vec::new(),
            workspaces: Vec::new(),
            windows: Vec::new(),
            focused_output: None,
            focused_workspace_id: None,
            focused_window_id: None,
            active_keyboard_layout: None,
            last_error: None,
        };
        let (tx, rx) = watch::channel(Arc::new(initial));
        let stop_flag = Arc::new(AtomicBool::new(false));
        let time_source: Arc<dyn TimeSource> = Arc::new(MonotonicTimeSource::new());
        let supervisor = Arc::new(Mutex::new(CompositorSupervisorData {
            state: SupervisorState::Starting,
            failures_ms: Vec::new(),
            stable_since_ms: None,
        }));

        let broker =
            CompositorCommandBroker::new(BrokerOptions::default(), Box::new(execute_niri_command));

        let tx_clone = tx.clone();
        let stop_clone = stop_flag.clone();
        let supervisor_clone = supervisor.clone();
        let time_source_clone = time_source.clone();
        let broker_clone = broker.clone();

        let handle = thread::spawn(move || {
            run_niri_listener(
                tx_clone,
                stop_clone,
                supervisor_clone,
                time_source_clone,
                broker_clone,
            );
        });

        Arc::new(Self {
            tx,
            rx,
            stop_flag,
            supervisor,
            time_source,
            broker,
            handle: Mutex::new(Some(handle)),
        })
    }

    /// Construct offline instance with injected clock and broker.
    pub fn new_offline_with(
        snapshot: CompositorSnapshot,
        time_source: Arc<dyn TimeSource>,
        broker: Arc<CompositorCommandBroker>,
    ) -> Arc<Self> {
        let snap_arc = Arc::new(snapshot);
        let (tx, rx) = watch::channel(snap_arc.clone());
        let stop_flag = Arc::new(AtomicBool::new(true));
        let supervisor = Arc::new(Mutex::new(CompositorSupervisorData {
            state: SupervisorState::Starting,
            failures_ms: Vec::new(),
            stable_since_ms: None,
        }));

        if snap_arc.version.owner_generation > 0 {
            broker.set_installed_generation(snap_arc.version.owner_generation);
        }
        let _ = broker.observe_snapshot(snap_arc);

        Arc::new(Self {
            tx,
            rx,
            stop_flag,
            supervisor,
            time_source,
            broker,
            handle: Mutex::new(None),
        })
    }

    /// Construct offline instance for testing with specified initial snapshot.
    pub fn new_offline(snapshot: CompositorSnapshot) -> Arc<Self> {
        let broker = CompositorCommandBroker::new(
            BrokerOptions::default(),
            Box::new(|_cmd, _timeout, _cancel, _register| Ok(ExecutorAck::Success)),
        );
        Self::new_offline_with(snapshot, Arc::new(MonotonicTimeSource::new()), broker)
    }

    pub fn supervisor_state(&self) -> SupervisorState {
        self.supervisor.lock().unwrap().state
    }

    pub fn time_source(&self) -> &Arc<dyn TimeSource> {
        &self.time_source
    }

    pub fn begin_start(&self) {
        let mut supervisor = self.supervisor.lock().unwrap();
        supervisor.state = SupervisorState::Starting;
        drop(supervisor);

        let previous = self.rx.borrow().clone();
        let mut current = (*previous).clone();
        // The very first `begin_start` (version still `DomainVersion::ZERO`)
        // must stay at ZERO: `observe_snapshot`'s initial-snapshot exemption
        // (broker.rs) only applies then, and callers depend on the first
        // successful start landing at exactly `(1, 0)`. Any later
        // `begin_start` (e.g. after a `reset_quarantine`, which already
        // published a real version through the broker) must bump the
        // revision, or this publish collides with the broker's last observed
        // snapshot at the same version.
        if current.version != DomainVersion::ZERO {
            let next_rev = current.version.revision.saturating_add(1);
            current.version = DomainVersion::new(current.version.owner_generation, next_rev);
        }
        current.connection = DomainLifecycle::Connecting;
        let snap_arc = Arc::new(current);
        if self.broker.observe_snapshot(snap_arc.clone()).is_ok() {
            let _ = self.tx.send(snap_arc);
        }

        tracing::info!(target: "shilpo_profile", lifecycle = "starting", "compositor supervisor transition");
    }

    pub fn mark_ready(&self, now_ms: u64) {
        let mut supervisor = self.supervisor.lock().unwrap();
        supervisor.state = SupervisorState::Running;
        supervisor.stable_since_ms = Some(now_ms);
        tracing::info!(target: "shilpo_profile", lifecycle = "ready", "compositor supervisor transition");
    }

    pub fn report_owner_failure(&self, error: String, now_ms: u64) {
        let version = self.rx.borrow().version;
        let owner_generation = version.owner_generation;
        let mut revision = version.revision;
        record_supervisor_failure(
            &self.supervisor,
            &self.broker,
            &self.tx,
            owner_generation,
            &mut revision,
            error,
            now_ms,
        );
    }

    pub fn tick(&self, now_ms: u64) {
        apply_tick(&self.supervisor, now_ms);
    }

    pub fn update_snapshot(&self, snapshot: CompositorSnapshot) -> Result<(), StaleUpdateError> {
        let snap_arc = Arc::new(snapshot);
        self.broker.observe_snapshot(snap_arc.clone())?;
        let _ = self.tx.send(snap_arc);
        Ok(())
    }

    pub fn set_reconnecting_generation(&self, generation: u64) {
        let previous = self.rx.borrow().clone();
        let mut current = (*previous).clone();
        current.version = DomainVersion::new(generation, 0);
        let snap_arc = Arc::new(current);
        let _ = self.tx.send(snap_arc);
    }

    /// Explicitly clears quarantine and permits the supervisor to retry ownership.
    pub fn reset_quarantine(&self) {
        let mut supervisor = self.supervisor.lock().unwrap();
        supervisor.failures_ms.clear();
        supervisor.stable_since_ms = None;
        supervisor.state = SupervisorState::Starting;
        drop(supervisor);

        let previous = self.rx.borrow().clone();
        let mut current = (*previous).clone();
        let next_gen = current.version.owner_generation.saturating_add(1);
        current.version = DomainVersion::new(next_gen, 0);
        current.connection = DomainLifecycle::Reconnecting;
        let snap_arc = Arc::new(current);
        self.broker.set_installed_generation(next_gen);
        if self.broker.observe_snapshot(snap_arc.clone()).is_ok() {
            let _ = self.tx.send(snap_arc);
        }
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
) -> Result<ExecutorAck, RejectionReason> {
    let socket_path = match resolve_niri_socket_path() {
        Some(p) => p,
        None => {
            return Err(RejectionReason::Unavailable);
        }
    };
    execute_niri_command_on_socket(&socket_path, command, timeout, cancel, register_cancel)
}

fn execute_niri_command_on_socket(
    socket_path: &std::path::Path,
    command: &CompositorCommand,
    timeout: Duration,
    cancel: Arc<CommandCancellation>,
    register_cancel: &dyn Fn(Arc<dyn StreamCancelHandle>),
) -> Result<ExecutorAck, RejectionReason> {
    let start_time = std::time::Instant::now();
    let deadline = start_time + timeout;

    if cancel.is_cancelled() {
        return Err(RejectionReason::Cancelled(
            cancel.reason().unwrap_or(super::CancellationReason::User),
        ));
    }

    let remaining_timeout = || -> Result<Duration, RejectionReason> {
        deadline
            .checked_duration_since(std::time::Instant::now())
            .ok_or(RejectionReason::TimedOut)
    };

    let remaining = remaining_timeout()?;

    let stream = match UnixStream::connect(socket_path) {
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

    let send_request = |stream: &mut UnixStream, req: &Request| -> Result<Reply, RejectionReason> {
        let current_rem = remaining_timeout()?;
        let _ = stream.set_read_timeout(Some(current_rem));
        let _ = stream.set_write_timeout(Some(current_rem));

        let mut json = match serde_json::to_string(req) {
            Ok(j) => j,
            Err(err) => {
                return Err(RejectionReason::Transport {
                    message: err.to_string(),
                });
            }
        };
        json.push('\n');

        if let Err(err) = stream.write_all(json.as_bytes()) {
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

        let mut reader = BufReader::new(&*stream);
        let mut line = String::new();
        if let Err(err) = reader.read_line(&mut line) {
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

        if line.is_empty() {
            return Err(RejectionReason::Transport {
                message: "niri socket closed unexpectedly (EOF)".into(),
            });
        }

        let reply: Reply = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(err) => {
                return Err(RejectionReason::Transport {
                    message: format!("malformed reply from niri: {err}"),
                });
            }
        };

        Ok(reply)
    };

    let mut stream_writer = stream;

    match command {
        CompositorCommand::FocusWorkspace(id) => {
            let req = Request::Action(niri_ipc::Action::FocusWorkspace {
                reference: niri_ipc::WorkspaceReferenceArg::Id(*id),
            });
            let reply = send_request(&mut stream_writer, &req)?;
            match reply {
                Ok(Response::Handled) => Ok(ExecutorAck::Success),
                Ok(resp) => Err(RejectionReason::BackendRejected {
                    message: format!("unexpected niri response: {resp:?}"),
                }),
                Err(err) => Err(RejectionReason::BackendRejected {
                    message: err.to_string(),
                }),
            }
        }
        CompositorCommand::FocusWindow(id) => {
            let req = Request::Action(niri_ipc::Action::FocusWindow { id: *id });
            let reply = send_request(&mut stream_writer, &req)?;
            match reply {
                Ok(Response::Handled) => Ok(ExecutorAck::Success),
                Ok(resp) => Err(RejectionReason::BackendRejected {
                    message: format!("unexpected niri response: {resp:?}"),
                }),
                Err(err) => Err(RejectionReason::BackendRejected {
                    message: err.to_string(),
                }),
            }
        }
        CompositorCommand::FocusPreviousWindow => {
            let req = Request::Action(niri_ipc::Action::FocusWindowPrevious {});
            let reply = send_request(&mut stream_writer, &req)?;
            match reply {
                Ok(Response::Handled) => Ok(ExecutorAck::Success),
                Ok(resp) => Err(RejectionReason::BackendRejected {
                    message: format!("unexpected niri response: {resp:?}"),
                }),
                Err(err) => Err(RejectionReason::BackendRejected {
                    message: err.to_string(),
                }),
            }
        }
        CompositorCommand::CloseWindow(id) => {
            let req = Request::Action(niri_ipc::Action::CloseWindow { id: Some(*id) });
            let reply = send_request(&mut stream_writer, &req)?;
            match reply {
                Ok(Response::Handled) => Ok(ExecutorAck::Success),
                Ok(resp) => Err(RejectionReason::BackendRejected {
                    message: format!("unexpected niri response: {resp:?}"),
                }),
                Err(err) => Err(RejectionReason::BackendRejected {
                    message: err.to_string(),
                }),
            }
        }
        CompositorCommand::MoveWindowToWorkspace {
            window_id,
            workspace_id,
        } => {
            let req = Request::Action(niri_ipc::Action::MoveWindowToWorkspace {
                window_id: Some(*window_id),
                reference: niri_ipc::WorkspaceReferenceArg::Id(*workspace_id),
                focus: true,
            });
            let reply = send_request(&mut stream_writer, &req)?;
            match reply {
                Ok(Response::Handled) => Ok(ExecutorAck::Success),
                Ok(resp) => Err(RejectionReason::BackendRejected {
                    message: format!("unexpected niri response: {resp:?}"),
                }),
                Err(err) => Err(RejectionReason::BackendRejected {
                    message: err.to_string(),
                }),
            }
        }
        CompositorCommand::CreateWorkspace => {
            let ws_reply = send_request(&mut stream_writer, &Request::Workspaces)?;
            let workspaces = match ws_reply {
                Ok(Response::Workspaces(ws)) => ws,
                Ok(resp) => {
                    return Err(RejectionReason::BackendRejected {
                        message: format!("unexpected response to Workspaces query: {resp:?}"),
                    });
                }
                Err(err) => {
                    return Err(RejectionReason::BackendRejected {
                        message: format!("failed to query workspaces: {err}"),
                    });
                }
            };

            let win_reply = send_request(&mut stream_writer, &Request::Windows)?;
            let windows = match win_reply {
                Ok(Response::Windows(win)) => win,
                Ok(resp) => {
                    return Err(RejectionReason::BackendRejected {
                        message: format!("unexpected response to Windows query: {resp:?}"),
                    });
                }
                Err(err) => {
                    return Err(RejectionReason::BackendRejected {
                        message: format!("failed to query windows: {err}"),
                    });
                }
            };

            let focused_ws = workspaces.iter().find(|w| w.is_focused);
            let focused_output = match focused_ws.and_then(|w| w.output.as_ref()) {
                Some(o) => o,
                None => {
                    return Err(RejectionReason::BackendRejected {
                        message: "no focused workspace/output found".into(),
                    });
                }
            };

            let output_workspaces: Vec<_> = workspaces
                .iter()
                .filter(|w| w.output.as_ref() == Some(focused_output))
                .collect();

            let empty_workspaces: Vec<_> = output_workspaces
                .into_iter()
                .filter(|w| {
                    w.name.is_none() && !windows.iter().any(|win| win.workspace_id == Some(w.id))
                })
                .collect();

            let target_ws = empty_workspaces.into_iter().max_by_key(|w| w.idx);

            match target_ws {
                Some(ws) => {
                    if ws.is_focused {
                        Ok(ExecutorAck::WorkspaceCreated {
                            workspace_id: ws.id,
                        })
                    } else {
                        let focus_req = Request::Action(niri_ipc::Action::FocusWorkspace {
                            reference: niri_ipc::WorkspaceReferenceArg::Id(ws.id),
                        });
                        let reply = send_request(&mut stream_writer, &focus_req)?;
                        match reply {
                            Ok(Response::Handled) => Ok(ExecutorAck::WorkspaceCreated {
                                workspace_id: ws.id,
                            }),
                            Ok(resp) => Err(RejectionReason::BackendRejected {
                                message: format!(
                                    "unexpected response focusing empty workspace: {resp:?}"
                                ),
                            }),
                            Err(err) => Err(RejectionReason::BackendRejected {
                                message: err.to_string(),
                            }),
                        }
                    }
                }
                None => Err(RejectionReason::BackendRejected {
                    message: "No eligible trailing empty workspace available on focused output"
                        .into(),
                }),
            }
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
    broker: &CompositorCommandBroker,
    owner_generation: u64,
    revision: &mut u64,
    last_error: Option<String>,
) {
    let previous = tx.borrow().clone();
    let mut current = (*previous).clone();
    *revision = revision.saturating_add(1);
    current.version = DomainVersion::new(owner_generation, *revision);
    current.connection = DomainLifecycle::Reconnecting;
    current.last_error = last_error;
    let snap_arc = Arc::new(current);
    if broker.observe_snapshot(snap_arc.clone()).is_ok() {
        let _ = tx.send(snap_arc);
    }
}

fn record_supervisor_failure(
    supervisor: &Arc<Mutex<CompositorSupervisorData>>,
    broker: &Arc<CompositorCommandBroker>,
    tx: &watch::Sender<Arc<CompositorSnapshot>>,
    owner_generation: u64,
    revision: &mut u64,
    error: String,
    now_ms: u64,
) {
    let (new_state, target_lifecycle, error_msg) = {
        let mut sup = supervisor.lock().unwrap();
        sup.stable_since_ms = None;
        sup.failures_ms
            .retain(|ts| now_ms.saturating_sub(*ts) <= FAILURE_WINDOW_MS);
        sup.failures_ms.push(now_ms);
        let failure_count = sup.failures_ms.len();
        if failure_count >= QUARANTINE_FAILURES {
            sup.state = SupervisorState::Quarantined;
            tracing::warn!(target: "shilpo_profile", lifecycle = "quarantined", "compositor supervisor transition");
            (
                SupervisorState::Quarantined,
                DomainLifecycle::Unavailable,
                "Quarantined after five failures in 60s".to_string(),
            )
        } else {
            let attempt = failure_count as u32;
            let retry_at_ms = now_ms.saturating_add(reconnect_backoff_ms(attempt));
            sup.state = SupervisorState::Backoff {
                attempt,
                retry_at_ms,
            };
            tracing::info!(target: "shilpo_profile", lifecycle = "backoff", attempt, "compositor supervisor transition");
            (sup.state, DomainLifecycle::Reconnecting, error)
        }
    };

    if new_state == SupervisorState::Quarantined {
        broker.record_quarantine_trip();
    }

    *revision = revision.saturating_add(1);
    let previous = tx.borrow().clone();
    let mut current = (*previous).clone();
    current.version = DomainVersion::new(owner_generation, *revision);
    current.connection = target_lifecycle;
    current.last_error = Some(error_msg);
    let snap_arc = Arc::new(current);
    if broker.observe_snapshot(snap_arc.clone()).is_ok() {
        let _ = tx.send(snap_arc);
    }
}

/// Applies the clock-driven `Backoff -> Starting` and stable-window-clear
/// transitions. Shared by `NiriCompositorService::tick` and the listener loop
/// so there is exactly one implementation of the tick-only discipline.
fn apply_tick(supervisor: &Arc<Mutex<CompositorSupervisorData>>, now_ms: u64) {
    let mut sup = supervisor.lock().unwrap();
    match sup.state {
        SupervisorState::Backoff { retry_at_ms, .. } => {
            if now_ms >= retry_at_ms {
                sup.state = SupervisorState::Starting;
            }
        }
        SupervisorState::Running => {
            if let Some(started) = sup.stable_since_ms
                && now_ms.saturating_sub(started) >= STABLE_RESET_MS
            {
                sup.failures_ms.clear();
            }
        }
        _ => {}
    }
}

fn run_niri_listener(
    tx: watch::Sender<Arc<CompositorSnapshot>>,
    stop_flag: Arc<AtomicBool>,
    supervisor: Arc<Mutex<CompositorSupervisorData>>,
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
        let state = supervisor.lock().unwrap().state;
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
            let mut sup = supervisor.lock().unwrap();
            sup.state = SupervisorState::Starting;
            tracing::info!(target: "shilpo_profile", lifecycle = "starting", "compositor supervisor transition");
        }
        broker.set_installed_generation(owner_generation);
        broker.record_restart();

        let previous = tx.borrow().clone();
        let mut connecting = (*previous).clone();
        revision = revision.saturating_add(1);
        connecting.version = DomainVersion::new(owner_generation, revision);
        connecting.connection = DomainLifecycle::Connecting;
        connecting.last_error = None;
        let connecting = Arc::new(connecting);
        if broker.observe_snapshot(connecting.clone()).is_ok() {
            let _ = tx.send(connecting);
        }

        let socket_path = match resolve_niri_socket_path() {
            Some(path) => path,
            None => {
                let err_msg = "Neither NIRI_SOCKET nor NIRI_SOCKET_PATH is set".to_string();
                let now_ms = time_source.now_ms();
                record_supervisor_failure(
                    &supervisor,
                    &broker,
                    &tx,
                    owner_generation,
                    &mut revision,
                    err_msg,
                    now_ms,
                );
                continue;
            }
        };

        let outputs = match query_outputs_from_socket_path(&socket_path) {
            Ok(outputs) => outputs,
            Err(err) => {
                let err_msg = format!("Failed to query Niri outputs: {err}");
                let now_ms = time_source.now_ms();
                record_supervisor_failure(
                    &supervisor,
                    &broker,
                    &tx,
                    owner_generation,
                    &mut revision,
                    err_msg,
                    now_ms,
                );
                continue;
            }
        };

        let mut event_stream = match UnixStream::connect(&socket_path) {
            Ok(s) => s,
            Err(err) => {
                let err_msg = format!("Failed to connect to Niri event socket: {err}");
                let now_ms = time_source.now_ms();
                record_supervisor_failure(
                    &supervisor,
                    &broker,
                    &tx,
                    owner_generation,
                    &mut revision,
                    err_msg,
                    now_ms,
                );
                continue;
            }
        };

        let req_json = match serde_json::to_string(&Request::EventStream) {
            Ok(mut j) => {
                j.push('\n');
                j
            }
            Err(err) => {
                let err_msg = format!("Failed to serialize EventStream request: {err}");
                let now_ms = time_source.now_ms();
                record_supervisor_failure(
                    &supervisor,
                    &broker,
                    &tx,
                    owner_generation,
                    &mut revision,
                    err_msg,
                    now_ms,
                );
                continue;
            }
        };

        if let Err(err) = event_stream.write_all(req_json.as_bytes()) {
            let err_msg = format!("Failed to send EventStream request: {err}");
            let now_ms = time_source.now_ms();
            record_supervisor_failure(
                &supervisor,
                &broker,
                &tx,
                owner_generation,
                &mut revision,
                err_msg,
                now_ms,
            );
            continue;
        }

        if event_stream
            .set_read_timeout(Some(Duration::from_millis(200)))
            .is_err()
        {
            let err_msg = "Failed to set read timeout on Niri socket".to_string();
            let now_ms = time_source.now_ms();
            record_supervisor_failure(
                &supervisor,
                &broker,
                &tx,
                owner_generation,
                &mut revision,
                err_msg,
                now_ms,
            );
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
                    let err_msg = "Niri socket EOF reached".to_string();
                    let now_ms = time_source.now_ms();
                    record_supervisor_failure(
                        &supervisor,
                        &broker,
                        &tx,
                        owner_generation,
                        &mut revision,
                        err_msg,
                        now_ms,
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
                            let now_ms = time_source.now_ms();
                            let mut sup = supervisor.lock().unwrap();
                            sup.state = SupervisorState::Running;
                            sup.stable_since_ms = Some(now_ms);
                            tracing::info!(target: "shilpo_profile", lifecycle = "ready", "compositor supervisor transition");
                        }

                        publish_snapshot_from_state(
                            &tx,
                            &broker,
                            owner_generation,
                            &mut revision,
                            DomainLifecycle::Ready,
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
                    let err_msg = format!("Niri socket read error: {err}");
                    let now_ms = time_source.now_ms();
                    record_supervisor_failure(
                        &supervisor,
                        &broker,
                        &tx,
                        owner_generation,
                        &mut revision,
                        err_msg,
                        now_ms,
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
        Some("Niri listener thread stopped".to_string()),
    );
}

fn publish_snapshot_from_state(
    tx: &watch::Sender<Arc<CompositorSnapshot>>,
    broker: &CompositorCommandBroker,
    owner_generation: u64,
    revision: &mut u64,
    connection: DomainLifecycle,
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
        .map(|w| {
            let (layout_x, layout_y) = w
                .layout
                .tile_pos_in_workspace_view
                .map(|(x, y)| (Some(x), Some(y)))
                .unwrap_or((None, None));
            let (column, row) = w
                .layout
                .pos_in_scrolling_layout
                .map(|(column, row)| (Some(column), Some(row)))
                .unwrap_or((None, None));
            WindowInfo {
                id: w.id,
                title: w.title.clone(),
                app_id: w.app_id.clone(),
                workspace_id: w.workspace_id,
                is_focused: w.is_focused,
                is_floating: w.is_floating,
                is_urgent: w.is_urgent,
                layout_x,
                layout_y,
                column,
                row,
            }
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
        version: DomainVersion::new(owner_generation, *revision),
        connection,
        capabilities: CompositorCapabilities::default(),
        outputs: sorted_outputs,
        workspaces,
        windows,
        focused_output,
        focused_workspace_id,
        focused_window_id,
        active_keyboard_layout,
        last_error: None,
    };

    let mut comparable = new_snapshot.clone();
    comparable.version = prev.version;
    if *prev != comparable {
        let mut new_snapshot = new_snapshot;
        *revision = revision.saturating_add(1);
        new_snapshot.version = DomainVersion::new(owner_generation, *revision);
        let snap_arc = Arc::new(new_snapshot);
        if broker.observe_snapshot(snap_arc.clone()).is_ok() {
            let _ = tx.send(snap_arc);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs::OpenOptions,
        os::unix::{io::AsRawFd, net::UnixListener},
    };

    use super::*;
    use crate::compositor::{CancellationReason, CommandOutcome};

    fn fake_niri_root() -> PathBuf {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("target")
            .join("fake-niri");
        std::fs::create_dir_all(&root).expect("failed to create fake Niri fixture root");
        root
    }

    struct TempSocketDir {
        path: PathBuf,
    }

    impl TempSocketDir {
        fn new() -> Self {
            static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

            for _ in 0..100 {
                let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
                let path =
                    fake_niri_root().join(format!("fake-niri-{}-{}", std::process::id(), id));
                match std::fs::create_dir(&path) {
                    Ok(()) => return Self { path },
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("failed to create fake Niri socket directory: {error}"),
                }
            }
            panic!("could not allocate a fake Niri socket directory");
        }
    }

    impl Drop for TempSocketDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    struct FakeNiriServer {
        _socket_lock: std::fs::File,
        _dir: TempSocketDir,
        socket_path: PathBuf,
        stop_flag: Arc<AtomicBool>,
        thread: Option<thread::JoinHandle<()>>,
        requests: Arc<Mutex<Vec<Request>>>,
    }

    impl FakeNiriServer {
        fn start<F>(handler: F) -> Self
        where
            F: Fn(Request, usize) -> Option<String> + Send + Sync + 'static,
        {
            let socket_lock = OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .truncate(false)
                .open(fake_niri_root().join("fixture.lock"))
                .expect("failed to open fake Niri socket lock");
            // SAFETY: `socket_lock` owns the valid file descriptor for the lock lifetime.
            if unsafe { libc::flock(socket_lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
                panic!("failed to lock fake Niri socket fixture");
            }
            let dir = TempSocketDir::new();
            let socket_path = dir.path.join("niri.sock");
            let listener = UnixListener::bind(&socket_path).unwrap_or_else(|error| {
                panic!(
                    "failed to bind fake Niri socket {}: {error}",
                    socket_path.display()
                )
            });
            listener.set_nonblocking(true).unwrap();

            let stop_flag = Arc::new(AtomicBool::new(false));
            let stop_clone = stop_flag.clone();
            let requests = Arc::new(Mutex::new(Vec::new()));
            let requests_clone = requests.clone();

            let thread = thread::spawn(move || {
                let mut req_counter = 0usize;
                while !stop_clone.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
                            let _ = stream.set_write_timeout(Some(Duration::from_millis(200)));
                            let mut writer = stream.try_clone().unwrap();
                            let mut reader = BufReader::new(stream);
                            loop {
                                let mut line = String::new();
                                match reader.read_line(&mut line) {
                                    Ok(0) => break,
                                    Ok(_) if line.trim().is_empty() => continue,
                                    Ok(_) => {
                                        req_counter += 1;
                                        if let Ok(req) = serde_json::from_str::<Request>(&line) {
                                            requests_clone.lock().unwrap().push(req.clone());
                                            if let Some(reply_str) = handler(req, req_counter) {
                                                let mut resp_str = reply_str;
                                                resp_str.push('\n');
                                                if writer.write_all(resp_str.as_bytes()).is_err() {
                                                    break;
                                                }
                                            } else {
                                                break;
                                            }
                                        } else {
                                            let _ = writer.write_all(b"NOT_VALID_JSON\n");
                                            break;
                                        }
                                    }
                                    Err(e)
                                        if e.kind() == std::io::ErrorKind::WouldBlock
                                            || e.kind() == std::io::ErrorKind::TimedOut =>
                                    {
                                        continue;
                                    }
                                    Err(_) => break,
                                }
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10));
                        }
                        Err(_) => break,
                    }
                }
            });

            Self {
                _socket_lock: socket_lock,
                _dir: dir,
                socket_path,
                stop_flag,
                thread: Some(thread),
                requests,
            }
        }

        fn requests(&self) -> Vec<Request> {
            self.requests.lock().unwrap().clone()
        }
    }

    impl Drop for FakeNiriServer {
        fn drop(&mut self) {
            self.stop_flag.store(true, Ordering::Relaxed);
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
        }
    }

    type StreamCancelRegisterFn = Box<dyn Fn(Arc<dyn StreamCancelHandle>)>;

    fn dummy_cancel() -> (Arc<CommandCancellation>, StreamCancelRegisterFn) {
        let cancel = CommandCancellation::new();
        let dummy_reg = Box::new(|_: Arc<dyn StreamCancelHandle>| {});
        (cancel, dummy_reg)
    }

    fn serial_guard() -> std::sync::MutexGuard<'static, ()> {
        crate::test_support::serial_guard()
    }

    fn req_json(req: &Request) -> String {
        serde_json::to_string(req).unwrap()
    }

    #[test]
    fn test_fake_niri_focus_workspace_mapping() {
        let _guard = serial_guard();
        let server = FakeNiriServer::start(|_req, _| {
            Some(serde_json::to_string(&Reply::Ok(Response::Handled)).unwrap())
        });
        let (cancel, reg) = dummy_cancel();

        let res = execute_niri_command_on_socket(
            &server.socket_path,
            &CompositorCommand::FocusWorkspace(42),
            Duration::from_secs(1),
            cancel,
            &reg,
        );
        assert_eq!(res, Ok(ExecutorAck::Success));

        let reqs = server.requests();
        assert_eq!(reqs.len(), 1);
        let expected = Request::Action(niri_ipc::Action::FocusWorkspace {
            reference: niri_ipc::WorkspaceReferenceArg::Id(42),
        });
        assert_eq!(req_json(&reqs[0]), req_json(&expected));
    }

    #[test]
    fn test_fake_niri_focus_window_and_previous_mapping() {
        let _guard = serial_guard();
        let server = FakeNiriServer::start(|_req, _| {
            Some(serde_json::to_string(&Reply::Ok(Response::Handled)).unwrap())
        });
        let (cancel1, reg1) = dummy_cancel();
        let (cancel2, reg2) = dummy_cancel();

        let res1 = execute_niri_command_on_socket(
            &server.socket_path,
            &CompositorCommand::FocusWindow(101),
            Duration::from_secs(1),
            cancel1,
            &reg1,
        );
        assert_eq!(res1, Ok(ExecutorAck::Success));

        let res2 = execute_niri_command_on_socket(
            &server.socket_path,
            &CompositorCommand::FocusPreviousWindow,
            Duration::from_secs(1),
            cancel2,
            &reg2,
        );
        assert_eq!(res2, Ok(ExecutorAck::Success));

        let reqs = server.requests();
        assert_eq!(reqs.len(), 2);
        let expected1 = Request::Action(niri_ipc::Action::FocusWindow { id: 101 });
        let expected2 = Request::Action(niri_ipc::Action::FocusWindowPrevious {});
        assert_eq!(req_json(&reqs[0]), req_json(&expected1));
        assert_eq!(req_json(&reqs[1]), req_json(&expected2));
    }

    #[test]
    fn test_fake_niri_close_window_mapping() {
        let _guard = serial_guard();
        let server = FakeNiriServer::start(|_req, _| {
            Some(serde_json::to_string(&Reply::Ok(Response::Handled)).unwrap())
        });
        let (cancel, reg) = dummy_cancel();
        let result = execute_niri_command_on_socket(
            &server.socket_path,
            &CompositorCommand::CloseWindow(101),
            Duration::from_secs(1),
            cancel,
            &reg,
        );
        assert_eq!(result, Ok(ExecutorAck::Success));
        let requests = server.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            req_json(&requests[0]),
            req_json(&Request::Action(niri_ipc::Action::CloseWindow {
                id: Some(101)
            }))
        );
    }

    #[test]
    fn test_fake_niri_move_window_mapping() {
        let _guard = serial_guard();
        let server = FakeNiriServer::start(|_req, _| {
            Some(serde_json::to_string(&Reply::Ok(Response::Handled)).unwrap())
        });
        let (cancel, reg) = dummy_cancel();

        let res = execute_niri_command_on_socket(
            &server.socket_path,
            &CompositorCommand::MoveWindowToWorkspace {
                window_id: 10,
                workspace_id: 20,
            },
            Duration::from_secs(1),
            cancel,
            &reg,
        );
        assert_eq!(res, Ok(ExecutorAck::Success));

        let reqs = server.requests();
        assert_eq!(reqs.len(), 1);
        let expected = Request::Action(niri_ipc::Action::MoveWindowToWorkspace {
            window_id: Some(10),
            reference: niri_ipc::WorkspaceReferenceArg::Id(20),
            focus: true,
        });
        assert_eq!(req_json(&reqs[0]), req_json(&expected));
    }

    #[test]
    fn test_fake_niri_create_workspace_activation() {
        let _guard = serial_guard();
        let server = FakeNiriServer::start(|req, _| match req {
            Request::Workspaces => {
                let ws = vec![
                    niri_ipc::Workspace {
                        id: 1,
                        name: None,
                        output: Some("DP-1".into()),
                        is_active: true,
                        is_focused: true,
                        is_urgent: false,
                        idx: 1,
                        active_window_id: Some(100),
                    },
                    niri_ipc::Workspace {
                        id: 2,
                        name: Some("Notes".into()),
                        output: Some("DP-1".into()),
                        is_active: false,
                        is_focused: false,
                        is_urgent: false,
                        idx: 2,
                        active_window_id: None,
                    },
                    niri_ipc::Workspace {
                        id: 3,
                        name: None,
                        output: Some("DP-1".into()),
                        is_active: false,
                        is_focused: false,
                        is_urgent: false,
                        idx: 3,
                        active_window_id: None,
                    },
                ];
                Some(serde_json::to_string(&Reply::Ok(Response::Workspaces(ws))).unwrap())
            }
            Request::Windows => {
                let win_json = r#"[{"id":100,"title":"Terminal","app_id":"alacritty","workspace_id":1,"is_focused":true,"is_floating":false,"is_urgent":false,"focus_timestamp":null,"pid":null,"layout":{"tile_size":[1.0,1.0],"window_size":[800,600],"window_offset_in_tile":[0,0]}}]"#;
                let win: Vec<niri_ipc::Window> = serde_json::from_str(win_json).unwrap();
                Some(serde_json::to_string(&Reply::Ok(Response::Windows(win))).unwrap())
            }
            Request::Action(niri_ipc::Action::FocusWorkspace { .. }) => {
                Some(serde_json::to_string(&Reply::Ok(Response::Handled)).unwrap())
            }
            _ => None,
        });

        let (cancel, reg) = dummy_cancel();
        let res = execute_niri_command_on_socket(
            &server.socket_path,
            &CompositorCommand::CreateWorkspace,
            Duration::from_secs(1),
            cancel,
            &reg,
        );
        assert_eq!(res, Ok(ExecutorAck::WorkspaceCreated { workspace_id: 3 }));

        let reqs = server.requests();
        assert_eq!(reqs.len(), 3);
        assert_eq!(req_json(&reqs[0]), req_json(&Request::Workspaces));
        assert_eq!(req_json(&reqs[1]), req_json(&Request::Windows));
        let expected_action = Request::Action(niri_ipc::Action::FocusWorkspace {
            reference: niri_ipc::WorkspaceReferenceArg::Id(3),
        });
        assert_eq!(req_json(&reqs[2]), req_json(&expected_action));
    }

    #[test]
    fn test_fake_niri_backend_rejection() {
        let _guard = serial_guard();
        let server = FakeNiriServer::start(|_req, _| {
            Some(serde_json::to_string(&Reply::Err("niri rejected action".into())).unwrap())
        });
        let (cancel, reg) = dummy_cancel();

        let res = execute_niri_command_on_socket(
            &server.socket_path,
            &CompositorCommand::FocusWorkspace(1),
            Duration::from_secs(1),
            cancel,
            &reg,
        );
        assert!(matches!(res, Err(RejectionReason::BackendRejected { .. })));
    }

    #[test]
    fn test_fake_niri_malformed_reply() {
        let _guard = serial_guard();
        let server = FakeNiriServer::start(|_req, _| Some("NOT_VALID_JSON".into()));
        let (cancel, reg) = dummy_cancel();

        let res = execute_niri_command_on_socket(
            &server.socket_path,
            &CompositorCommand::FocusWorkspace(1),
            Duration::from_secs(1),
            cancel,
            &reg,
        );
        assert!(matches!(res, Err(RejectionReason::Transport { .. })));
    }

    #[test]
    fn test_fake_niri_disconnect_reply() {
        let _guard = serial_guard();
        let server = FakeNiriServer::start(|_req, _| None);
        let (cancel, reg) = dummy_cancel();

        let res = execute_niri_command_on_socket(
            &server.socket_path,
            &CompositorCommand::FocusWorkspace(1),
            Duration::from_secs(1),
            cancel,
            &reg,
        );
        assert!(matches!(res, Err(RejectionReason::Transport { .. })));
    }

    #[test]
    fn test_fake_niri_timeout_uses_command_deadline() {
        let _guard = serial_guard();
        let server = FakeNiriServer::start(|_req, _| {
            thread::sleep(Duration::from_millis(300));
            Some(serde_json::to_string(&Reply::Ok(Response::Handled)).unwrap())
        });
        let (cancel, reg) = dummy_cancel();

        let res = execute_niri_command_on_socket(
            &server.socket_path,
            &CompositorCommand::FocusWorkspace(1),
            Duration::from_millis(50),
            cancel,
            &reg,
        );
        assert!(matches!(res, Err(RejectionReason::TimedOut)));
    }

    #[test]
    fn test_fake_niri_recovers_after_backend_failure() {
        let _guard = serial_guard();
        let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = call_count.clone();
        let server = FakeNiriServer::start(move |_req, _| {
            if count.fetch_add(1, Ordering::SeqCst) == 0 {
                Some(serde_json::to_string(&Reply::Err("first failure".into())).unwrap())
            } else {
                Some(serde_json::to_string(&Reply::Ok(Response::Handled)).unwrap())
            }
        });

        let (cancel1, reg1) = dummy_cancel();
        let first = execute_niri_command_on_socket(
            &server.socket_path,
            &CompositorCommand::FocusWorkspace(1),
            Duration::from_secs(1),
            cancel1,
            &reg1,
        );
        assert!(matches!(
            first,
            Err(RejectionReason::BackendRejected { .. })
        ));

        let (cancel2, reg2) = dummy_cancel();
        let second = execute_niri_command_on_socket(
            &server.socket_path,
            &CompositorCommand::FocusWorkspace(1),
            Duration::from_secs(1),
            cancel2,
            &reg2,
        );
        assert!(second.is_ok());
    }

    #[test]
    fn test_broker_cancels_active_niri_command() {
        let _guard = serial_guard();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let server = FakeNiriServer::start(move |_req, _| {
            started_tx.send(()).unwrap();
            thread::sleep(Duration::from_millis(500));
            Some(serde_json::to_string(&Reply::Ok(Response::Handled)).unwrap())
        });
        let socket_path = server.socket_path.clone();
        let executor: crate::compositor::broker::CommandExecutorFn =
            Box::new(move |cmd, timeout, cancel, register| {
                execute_niri_command_on_socket(&socket_path, cmd, timeout, cancel, register)
            });
        let broker = CompositorCommandBroker::new(
            BrokerOptions {
                timeout: Duration::from_secs(1),
                max_queue_len: 4,
            },
            executor,
        );
        broker.set_installed_generation(1);
        let ready_snap = CompositorSnapshot {
            version: DomainVersion::new(1, 1),
            connection: DomainLifecycle::Ready,
            workspaces: vec![super::WorkspaceInfo {
                id: 1,
                name: None,
                idx: 1,
                is_active: true,
                is_focused: true,
                is_urgent: false,
                output_name: None,
                active_window_id: None,
            }],
            focused_workspace_id: Some(1),
            ..Default::default()
        };
        broker.observe_snapshot(Arc::new(ready_snap)).unwrap();

        let mut ticket = broker.submit(CompositorCommand::FocusWorkspace(1)).unwrap();
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        ticket.cancel();
        assert_eq!(
            ticket.wait_timeout(Duration::from_secs(1)),
            CommandOutcome::Cancelled {
                reason: CancellationReason::User
            }
        );
    }

    #[test]
    fn test_broker_reconnect_interrupts_active_niri_command() {
        let _guard = serial_guard();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let server = FakeNiriServer::start(move |_req, _| {
            started_tx.send(()).unwrap();
            thread::sleep(Duration::from_millis(500));
            Some(serde_json::to_string(&Reply::Ok(Response::Handled)).unwrap())
        });
        let socket_path = server.socket_path.clone();
        let executor: crate::compositor::broker::CommandExecutorFn =
            Box::new(move |cmd, timeout, cancel, register| {
                execute_niri_command_on_socket(&socket_path, cmd, timeout, cancel, register)
            });
        let broker = CompositorCommandBroker::new(BrokerOptions::default(), executor);
        broker.set_installed_generation(1);
        let ready_snap = CompositorSnapshot {
            version: DomainVersion::new(1, 1),
            connection: DomainLifecycle::Ready,
            workspaces: vec![super::WorkspaceInfo {
                id: 1,
                name: None,
                idx: 1,
                is_active: true,
                is_focused: true,
                is_urgent: false,
                output_name: None,
                active_window_id: None,
            }],
            focused_workspace_id: Some(1),
            ..Default::default()
        };
        broker.observe_snapshot(Arc::new(ready_snap)).unwrap();

        let ticket = broker.submit(CompositorCommand::FocusWorkspace(1)).unwrap();
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        broker
            .observe_snapshot(Arc::new(CompositorSnapshot {
                version: DomainVersion::new(1, 2),
                connection: DomainLifecycle::Reconnecting,
                ..Default::default()
            }))
            .unwrap();
        assert_eq!(
            ticket.wait_timeout(Duration::from_secs(1)),
            CommandOutcome::Cancelled {
                reason: CancellationReason::Reconnect
            }
        );
    }

    #[test]
    fn supervisor_uses_capped_exponential_backoff_and_quarantines() {
        let service = NiriCompositorService::new_offline(CompositorSnapshot::default());
        service.report_owner_failure("test".into(), 0);
        assert_eq!(
            service.supervisor_state(),
            SupervisorState::Backoff {
                attempt: 1,
                retry_at_ms: 250
            }
        );
        service.report_owner_failure("test".into(), 1_000);
        assert_eq!(
            service.supervisor_state(),
            SupervisorState::Backoff {
                attempt: 2,
                retry_at_ms: 1_500
            }
        );
        service.report_owner_failure("test".into(), 2_000);
        service.report_owner_failure("test".into(), 3_000);
        service.report_owner_failure("test".into(), 4_000);
        assert_eq!(service.supervisor_state(), SupervisorState::Quarantined);
    }

    #[test]
    fn supervisor_clears_failures_after_stable_period_and_supports_reset() {
        let service = NiriCompositorService::new_offline(CompositorSnapshot::default());
        service.report_owner_failure("test".into(), 0);
        service.mark_ready(1_000);
        service.tick(302_000);
        service.report_owner_failure("test".into(), 303_000);
        assert_eq!(
            service.supervisor_state(),
            SupervisorState::Backoff {
                attempt: 1,
                retry_at_ms: 303_250
            }
        );
        service.reset_quarantine();
        assert_eq!(service.supervisor_state(), SupervisorState::Starting);
        service.report_owner_failure("test".into(), 400_000);
        assert_eq!(
            service.supervisor_state(),
            SupervisorState::Backoff {
                attempt: 1,
                retry_at_ms: 400_250
            }
        );
    }

    #[test]
    fn supervisor_tick_only_discipline_getters_do_not_mutate_state() {
        let service = NiriCompositorService::new_offline(CompositorSnapshot::default());
        service.report_owner_failure("test".into(), 0);
        assert_eq!(
            service.supervisor_state(),
            SupervisorState::Backoff {
                attempt: 1,
                retry_at_ms: 250
            }
        );

        // Advance past retry_at_ms without calling tick()
        // Repeatedly calling getters must NOT cause Backoff -> Starting transition.
        for _ in 0..10 {
            assert_eq!(
                service.supervisor_state(),
                SupervisorState::Backoff {
                    attempt: 1,
                    retry_at_ms: 250
                }
            );
            assert_eq!(service.command_broker().telemetry().owner_generation, 0);
            assert_eq!(service.current().connection, DomainLifecycle::Reconnecting);
        }

        // Only explicit tick() causes the clock-driven transition to Starting
        service.tick(250);
        assert_eq!(service.supervisor_state(), SupervisorState::Starting);
    }
}
