use super::{CompositorCommand, CompositorConnection, CompositorSnapshot};
use std::{
    collections::VecDeque,
    fmt,
    os::unix::net::UnixStream,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tokio::sync::oneshot;

/// Outcome returned when a compositor command successfully applies.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandOutcome {
    Applied { revision: u64 },
}

/// Compositor target descriptor for typed error reporting.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum CompositorTarget {
    Workspace(u64),
    Window(u64),
}

impl fmt::Display for CompositorTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Workspace(id) => write!(f, "workspace {}", id),
            Self::Window(id) => write!(f, "window {}", id),
        }
    }
}

/// Acknowledgement returned by backend command executor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExecutorAck {
    Success,
    WorkspaceCreated { workspace_id: u64 },
}

/// Reasons why a compositor command was cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancellationReason {
    User,
    Reconnect,
    Shutdown,
}

impl fmt::Display for CancellationReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::User => write!(f, "cancelled by user or dropped ticket"),
            Self::Reconnect => write!(f, "compositor reconnected or changed state"),
            Self::Shutdown => write!(f, "broker shutdown"),
        }
    }
}

/// Errors returned by the compositor command broker.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "detail")]
pub enum CompositorCommandError {
    Unavailable {
        state: CompositorConnection,
    },
    Busy {
        queue_len: usize,
    },
    Unsupported,
    BackendRejected {
        message: String,
    },
    Transport {
        message: String,
    },
    Timeout {
        duration: Duration,
    },
    Cancelled {
        reason: CancellationReason,
    },
    InvalidTarget(CompositorTarget),
    TargetDisappeared(CompositorTarget),
    ApplyTimeout {
        duration: Duration,
        last_revision: u64,
    },
}

impl fmt::Display for CompositorCommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable { state } => {
                write!(f, "compositor unavailable (state: {})", state.state_name())
            }
            Self::Busy { queue_len } => write!(f, "compositor busy (queue length: {})", queue_len),
            Self::Unsupported => write!(f, "compositor command unsupported"),
            Self::BackendRejected { message } => write!(f, "backend rejected: {}", message),
            Self::Transport { message } => write!(f, "transport error: {}", message),
            Self::Timeout { duration } => write!(f, "command timed out after {:?}", duration),
            Self::Cancelled { reason } => write!(f, "command cancelled: {}", reason),
            Self::InvalidTarget(target) => write!(f, "invalid target: {}", target),
            Self::TargetDisappeared(target) => {
                write!(f, "target disappeared before application: {}", target)
            }
            Self::ApplyTimeout {
                duration,
                last_revision,
            } => write!(
                f,
                "command application timed out after {:?} (last revision: {})",
                duration, last_revision
            ),
        }
    }
}

impl std::error::Error for CompositorCommandError {}

/// Configuration options for `CompositorCommandBroker`.
#[derive(Debug, Clone)]
pub struct BrokerOptions {
    pub timeout: Duration,
    pub max_queue_len: usize,
}

impl Default for BrokerOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_millis(1500),
            max_queue_len: 32,
        }
    }
}

/// Handle to cancel an in-flight socket operation.
pub trait StreamCancelHandle: Send + Sync {
    fn interrupt(&self);
}

/// Shared cancellation state for a command and its active transport.
pub struct CommandCancellation {
    cancelled: AtomicBool,
    reason: Mutex<Option<CancellationReason>>,
    handle: Mutex<Option<Arc<dyn StreamCancelHandle>>>,
    wake: Mutex<Option<Arc<std::sync::Condvar>>>,
}

impl CommandCancellation {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            cancelled: AtomicBool::new(false),
            reason: Mutex::new(None),
            handle: Mutex::new(None),
            wake: Mutex::new(None),
        })
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn reason(&self) -> Option<CancellationReason> {
        *self.reason.lock().unwrap()
    }

    fn cancel(&self, reason: CancellationReason) {
        self.cancelled.store(true, Ordering::Release);
        let mut reason_slot = self.reason.lock().unwrap();
        if reason_slot.is_none() {
            *reason_slot = Some(reason);
        }
        drop(reason_slot);
        if let Some(handle) = self.handle.lock().unwrap().take() {
            handle.interrupt();
        }
        if let Some(wake) = self.wake.lock().unwrap().as_ref() {
            wake.notify_all();
        }
    }

    fn register(&self, handle: Arc<dyn StreamCancelHandle>) {
        if self.is_cancelled() {
            handle.interrupt();
            return;
        }
        let mut slot = self.handle.lock().unwrap();
        if self.is_cancelled() {
            drop(slot);
            handle.interrupt();
        } else {
            *slot = Some(handle);
        }
    }

    fn clear_handle(&self) {
        self.handle.lock().unwrap().take();
    }

    fn attach_waker(&self, wake: Arc<std::sync::Condvar>) {
        *self.wake.lock().unwrap() = Some(wake);
        if self.is_cancelled() {
            self.wake.lock().unwrap().as_ref().unwrap().notify_all();
        }
    }
}

struct BasicStreamCancelHandle(Arc<Mutex<Option<UnixStream>>>);

impl StreamCancelHandle for BasicStreamCancelHandle {
    fn interrupt(&self) {
        if let Ok(guard) = self.0.lock()
            && let Some(ref stream) = *guard
        {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    }
}

/// Ticket returned upon submitting a command to the broker.
pub struct CommandTicket {
    rx: oneshot::Receiver<Result<CommandOutcome, CompositorCommandError>>,
    cancellation: Arc<CommandCancellation>,
    completed: bool,
}

impl fmt::Debug for CommandTicket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CommandTicket")
            .field("completed", &self.completed)
            .finish()
    }
}

impl CommandTicket {
    pub fn new(
        rx: oneshot::Receiver<Result<CommandOutcome, CompositorCommandError>>,
        cancellation: Arc<CommandCancellation>,
    ) -> Self {
        Self {
            rx,
            cancellation,
            completed: false,
        }
    }

    /// Explicitly cancels the command ticket.
    pub fn cancel(&mut self) {
        if !self.completed {
            self.cancellation.cancel(CancellationReason::User);
        }
    }

    /// Synchronously waits for command completion with a timeout.
    pub fn wait_timeout(
        mut self,
        timeout: Duration,
    ) -> Result<CommandOutcome, CompositorCommandError> {
        let start = Instant::now();
        loop {
            match self.rx.try_recv() {
                Ok(res) => {
                    self.completed = true;
                    return res;
                }
                Err(oneshot::error::TryRecvError::Closed) => {
                    self.completed = true;
                    return Err(CompositorCommandError::Cancelled {
                        reason: CancellationReason::Shutdown,
                    });
                }
                Err(oneshot::error::TryRecvError::Empty) => {
                    if start.elapsed() >= timeout {
                        self.cancel();
                        self.completed = true;
                        return Err(CompositorCommandError::Timeout { duration: timeout });
                    }
                    thread::sleep(Duration::from_millis(5));
                }
            }
        }
    }

    /// Synchronously waits until command completion.
    pub fn wait(self) -> Result<CommandOutcome, CompositorCommandError> {
        self.wait_timeout(Duration::from_secs(60))
    }
}

impl std::future::Future for CommandTicket {
    type Output = Result<CommandOutcome, CompositorCommandError>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        match std::pin::Pin::new(&mut self.rx).poll(cx) {
            std::task::Poll::Ready(res) => {
                self.completed = true;
                std::task::Poll::Ready(res.unwrap_or(Err(CompositorCommandError::Cancelled {
                    reason: CancellationReason::Shutdown,
                })))
            }
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

impl Drop for CommandTicket {
    fn drop(&mut self) {
        if !self.completed {
            self.cancellation.cancel(CancellationReason::User);
        }
    }
}

/// Telemetry metrics snapshot for `CompositorCommandBroker`.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CompositorBrokerTelemetry {
    pub accepted: u64,
    pub succeeded: u64,
    pub backend_failed: u64,
    pub transport_failed: u64,
    pub timed_out: u64,
    pub cancelled: u64,
    pub rejected_unavailable: u64,
    pub rejected_busy: u64,
    pub rejected_unsupported: u64,
    pub reconnect_transitions: u64,
    pub application_timeouts: u64,
    pub invalid_targets: u64,
    pub target_disappeared: u64,
    pub current_queue_depth: usize,
    pub in_flight: bool,
    pub last_latency_ms: Option<f64>,
    pub avg_latency_ms: Option<f64>,
}

pub(crate) type CommandExecutorFn = Box<
    dyn Fn(
            &CompositorCommand,
            Duration,
            Arc<CommandCancellation>,
            &dyn Fn(Arc<dyn StreamCancelHandle>),
        ) -> Result<ExecutorAck, CompositorCommandError>
        + Send
        + Sync,
>;

struct PendingCommand {
    _id: u64,
    epoch: u64,
    command: CompositorCommand,
    submitted_at: Instant,
    deadline: Instant,
    tx: oneshot::Sender<Result<CommandOutcome, CompositorCommandError>>,
    cancellation: Arc<CommandCancellation>,
}

#[derive(Clone)]
struct ActiveCommand {
    cancellation: Arc<CommandCancellation>,
    convergence: Arc<Mutex<ConvergenceState>>,
}

struct ConvergenceState {
    baseline: Arc<CompositorSnapshot>,
    command: CompositorCommand,
    already_satisfied: bool,
    expectation: Option<CommandExpectation>,
    observed_snapshots: Vec<Arc<CompositorSnapshot>>,
    terminal: Option<Result<CommandOutcome, CompositorCommandError>>,
    last_revision: u64,
}

enum CommandExpectation {
    FocusWorkspace(u64),
    FocusWindow(u64),
    FocusPreviousWindow {
        previous_window_id: Option<u64>,
    },
    CloseWindow {
        window_id: u64,
    },
    MoveWindowToWorkspace {
        window_id: u64,
        workspace_id: u64,
    },
    CreateWorkspace {
        workspace_id: u64,
        target_seen: bool,
    },
}

impl ActiveCommand {
    fn new(
        command: CompositorCommand,
        baseline: Arc<CompositorSnapshot>,
        already_satisfied: bool,
        cancellation: Arc<CommandCancellation>,
    ) -> Self {
        Self {
            cancellation,
            convergence: Arc::new(Mutex::new(ConvergenceState {
                last_revision: baseline.revision,
                baseline,
                command,
                already_satisfied,
                expectation: None,
                observed_snapshots: Vec::new(),
                terminal: None,
            })),
        }
    }

    fn observe(&self, snapshot: Arc<CompositorSnapshot>) {
        let mut convergence = self.convergence.lock().unwrap();
        if snapshot.revision <= convergence.baseline.revision || convergence.terminal.is_some() {
            return;
        }
        convergence.last_revision = snapshot.revision;
        if convergence.expectation.is_some() {
            convergence.evaluate(&snapshot);
        } else {
            convergence.observed_snapshots.push(snapshot);
        }
    }

    fn acknowledge(&self, ack: ExecutorAck) -> Result<(), CompositorCommandError> {
        let mut convergence = self.convergence.lock().unwrap();
        if convergence.already_satisfied {
            convergence.terminal = Some(Ok(CommandOutcome::Applied {
                revision: convergence.baseline.revision,
            }));
            return Ok(());
        }

        convergence.expectation = Some(CommandExpectation::from_ack(
            &convergence.command,
            &ack,
            &convergence.baseline,
        )?);
        let observed = std::mem::take(&mut convergence.observed_snapshots);
        for snapshot in observed {
            convergence.evaluate(&snapshot);
            if convergence.terminal.is_some() {
                break;
            }
        }
        Ok(())
    }

    fn terminal(&self) -> Option<Result<CommandOutcome, CompositorCommandError>> {
        self.convergence.lock().unwrap().terminal.clone()
    }

    fn last_revision(&self) -> u64 {
        self.convergence.lock().unwrap().last_revision
    }
}

impl CommandExpectation {
    fn from_ack(
        command: &CompositorCommand,
        ack: &ExecutorAck,
        baseline: &CompositorSnapshot,
    ) -> Result<Self, CompositorCommandError> {
        match (command, ack) {
            (CompositorCommand::FocusWorkspace(id), ExecutorAck::Success) => {
                Ok(Self::FocusWorkspace(*id))
            }
            (CompositorCommand::FocusWindow(id), ExecutorAck::Success) => {
                Ok(Self::FocusWindow(*id))
            }
            (CompositorCommand::FocusPreviousWindow, ExecutorAck::Success) => {
                Ok(Self::FocusPreviousWindow {
                    previous_window_id: baseline.focused_window_id,
                })
            }
            (CompositorCommand::CloseWindow(id), ExecutorAck::Success) => {
                Ok(Self::CloseWindow { window_id: *id })
            }
            (
                CompositorCommand::MoveWindowToWorkspace {
                    window_id,
                    workspace_id,
                },
                ExecutorAck::Success,
            ) => Ok(Self::MoveWindowToWorkspace {
                window_id: *window_id,
                workspace_id: *workspace_id,
            }),
            (
                CompositorCommand::CreateWorkspace,
                ExecutorAck::WorkspaceCreated { workspace_id },
            ) => Ok(Self::CreateWorkspace {
                workspace_id: *workspace_id,
                target_seen: baseline
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.id == *workspace_id),
            }),
            (CompositorCommand::CreateWorkspace, ExecutorAck::Success) => {
                Err(CompositorCommandError::BackendRejected {
                    message: "create-workspace acknowledgement did not identify a workspace".into(),
                })
            }
            _ => Err(CompositorCommandError::BackendRejected {
                message: "backend acknowledgement does not match the requested command".into(),
            }),
        }
    }
}

impl ConvergenceState {
    fn evaluate(&mut self, snapshot: &CompositorSnapshot) {
        let Some(expectation) = self.expectation.as_mut() else {
            return;
        };

        let applied = match expectation {
            CommandExpectation::FocusWorkspace(workspace_id) => {
                snapshot.focused_workspace_id == Some(*workspace_id)
            }
            CommandExpectation::FocusWindow(window_id) => {
                snapshot.focused_window_id == Some(*window_id)
            }
            CommandExpectation::FocusPreviousWindow { previous_window_id } => {
                snapshot.focused_window_id.is_some()
                    && snapshot.focused_window_id != *previous_window_id
            }
            CommandExpectation::CloseWindow { window_id } => !snapshot
                .windows
                .iter()
                .any(|window| window.id == *window_id),
            CommandExpectation::MoveWindowToWorkspace {
                window_id,
                workspace_id,
            } => snapshot.windows.iter().any(|window| {
                window.id == *window_id && window.workspace_id == Some(*workspace_id)
            }),
            CommandExpectation::CreateWorkspace { workspace_id, .. } => {
                snapshot.focused_workspace_id == Some(*workspace_id)
            }
        };
        if applied {
            self.terminal = Some(Ok(CommandOutcome::Applied {
                revision: snapshot.revision,
            }));
            return;
        }

        let disappeared = match expectation {
            CommandExpectation::FocusWorkspace(workspace_id) => (!snapshot
                .workspaces
                .iter()
                .any(|workspace| workspace.id == *workspace_id))
            .then_some(CompositorTarget::Workspace(*workspace_id)),
            CommandExpectation::FocusWindow(window_id) => (!snapshot
                .windows
                .iter()
                .any(|window| window.id == *window_id))
            .then_some(CompositorTarget::Window(*window_id)),
            CommandExpectation::MoveWindowToWorkspace {
                window_id,
                workspace_id,
            } => {
                if !snapshot
                    .windows
                    .iter()
                    .any(|window| window.id == *window_id)
                {
                    Some(CompositorTarget::Window(*window_id))
                } else if !snapshot
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.id == *workspace_id)
                {
                    Some(CompositorTarget::Workspace(*workspace_id))
                } else {
                    None
                }
            }
            CommandExpectation::CreateWorkspace {
                workspace_id,
                target_seen,
            } => {
                let present = snapshot
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.id == *workspace_id);
                if present {
                    *target_seen = true;
                    None
                } else if *target_seen {
                    Some(CompositorTarget::Workspace(*workspace_id))
                } else {
                    None
                }
            }
            CommandExpectation::FocusPreviousWindow { .. } => None,
            CommandExpectation::CloseWindow { .. } => None,
        };
        if let Some(target) = disappeared {
            self.terminal = Some(Err(CompositorCommandError::TargetDisappeared(target)));
        }
    }
}

struct BrokerState {
    snapshot: Arc<CompositorSnapshot>,
    queue: VecDeque<PendingCommand>,
}

#[derive(Default)]
struct LatencyStats {
    total_ms_sum: f64,
    count: u64,
    last_ms: Option<f64>,
}

struct BrokerInner {
    options: BrokerOptions,
    epoch: AtomicU64,
    state: Mutex<BrokerState>,
    active: Mutex<Option<Arc<CommandCancellation>>>,
    convergence: Mutex<Option<ActiveCommand>>,
    shutdown: AtomicBool,
    cv: Arc<std::sync::Condvar>,
    accepted: AtomicU64,
    succeeded: AtomicU64,
    backend_failed: AtomicU64,
    transport_failed: AtomicU64,
    timed_out: AtomicU64,
    cancelled: AtomicU64,
    rejected_unavailable: AtomicU64,
    rejected_busy: AtomicU64,
    rejected_unsupported: AtomicU64,
    reconnect_transitions: AtomicU64,
    application_timeouts: AtomicU64,
    invalid_targets: AtomicU64,
    target_disappeared: AtomicU64,
    latency_stats: Mutex<LatencyStats>,
}

impl BrokerInner {
    fn record_terminal_outcome(
        &self,
        submitted_at: Instant,
        result: &Result<CommandOutcome, CompositorCommandError>,
    ) {
        let latency_ms = submitted_at.elapsed().as_secs_f64() * 1000.0;
        {
            let mut stats = self.latency_stats.lock().unwrap();
            stats.total_ms_sum += latency_ms;
            stats.count += 1;
            stats.last_ms = Some(latency_ms);
        }
        match result {
            Ok(CommandOutcome::Applied { .. }) => {
                self.succeeded.fetch_add(1, Ordering::Relaxed);
            }
            Err(CompositorCommandError::BackendRejected { .. }) => {
                self.backend_failed.fetch_add(1, Ordering::Relaxed);
            }
            Err(CompositorCommandError::Transport { .. }) => {
                self.transport_failed.fetch_add(1, Ordering::Relaxed);
            }
            Err(CompositorCommandError::Timeout { .. }) => {
                self.timed_out.fetch_add(1, Ordering::Relaxed);
            }
            Err(CompositorCommandError::Cancelled { .. }) => {
                self.cancelled.fetch_add(1, Ordering::Relaxed);
            }
            Err(CompositorCommandError::Unavailable { .. }) => {
                self.rejected_unavailable.fetch_add(1, Ordering::Relaxed);
            }
            Err(CompositorCommandError::Busy { .. }) => {
                self.rejected_busy.fetch_add(1, Ordering::Relaxed);
            }
            Err(CompositorCommandError::Unsupported) => {
                self.rejected_unsupported.fetch_add(1, Ordering::Relaxed);
            }
            Err(CompositorCommandError::InvalidTarget(_)) => {
                self.invalid_targets.fetch_add(1, Ordering::Relaxed);
            }
            Err(CompositorCommandError::TargetDisappeared(_)) => {
                self.target_disappeared.fetch_add(1, Ordering::Relaxed);
            }
            Err(CompositorCommandError::ApplyTimeout { .. }) => {
                self.application_timeouts.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

/// Central serial FIFO command broker for compositor commands.
pub struct CompositorCommandBroker {
    inner: Arc<BrokerInner>,
    next_cmd_id: AtomicU64,
    worker_thread: Mutex<Option<JoinHandle<()>>>,
}

impl CompositorCommandBroker {
    pub(crate) fn new(options: BrokerOptions, executor: CommandExecutorFn) -> Arc<Self> {
        let inner = Arc::new(BrokerInner {
            options,
            epoch: AtomicU64::new(1),
            state: Mutex::new(BrokerState {
                snapshot: Arc::new(CompositorSnapshot::default()),
                queue: VecDeque::new(),
            }),
            active: Mutex::new(None),
            convergence: Mutex::new(None),
            shutdown: AtomicBool::new(false),
            cv: Arc::new(std::sync::Condvar::new()),
            accepted: AtomicU64::new(0),
            succeeded: AtomicU64::new(0),
            backend_failed: AtomicU64::new(0),
            transport_failed: AtomicU64::new(0),
            timed_out: AtomicU64::new(0),
            cancelled: AtomicU64::new(0),
            rejected_unavailable: AtomicU64::new(0),
            rejected_busy: AtomicU64::new(0),
            rejected_unsupported: AtomicU64::new(0),
            reconnect_transitions: AtomicU64::new(0),
            application_timeouts: AtomicU64::new(0),
            invalid_targets: AtomicU64::new(0),
            target_disappeared: AtomicU64::new(0),
            latency_stats: Mutex::new(LatencyStats::default()),
        });

        let broker = Arc::new(Self {
            inner: inner.clone(),
            next_cmd_id: AtomicU64::new(1),
            worker_thread: Mutex::new(None),
        });
        let b_weak = Arc::downgrade(&inner);
        let worker = thread::Builder::new()
            .name("compositor-broker-worker".into())
            .spawn(move || {
                Self::worker_loop(b_weak, executor);
            })
            .expect("failed to spawn compositor broker worker thread");

        *broker.worker_thread.lock().unwrap() = Some(worker);

        broker
    }

    pub fn telemetry(&self) -> CompositorBrokerTelemetry {
        let state = self.inner.state.lock().unwrap();
        let in_flight = self.inner.active.lock().unwrap().is_some();
        let stats = self.inner.latency_stats.lock().unwrap();
        let avg_latency_ms = if stats.count > 0 {
            Some(stats.total_ms_sum / stats.count as f64)
        } else {
            None
        };
        CompositorBrokerTelemetry {
            accepted: self.inner.accepted.load(Ordering::Relaxed),
            succeeded: self.inner.succeeded.load(Ordering::Relaxed),
            backend_failed: self.inner.backend_failed.load(Ordering::Relaxed),
            transport_failed: self.inner.transport_failed.load(Ordering::Relaxed),
            timed_out: self.inner.timed_out.load(Ordering::Relaxed),
            cancelled: self.inner.cancelled.load(Ordering::Relaxed),
            rejected_unavailable: self.inner.rejected_unavailable.load(Ordering::Relaxed),
            rejected_busy: self.inner.rejected_busy.load(Ordering::Relaxed),
            rejected_unsupported: self.inner.rejected_unsupported.load(Ordering::Relaxed),
            reconnect_transitions: self.inner.reconnect_transitions.load(Ordering::Relaxed),
            application_timeouts: self.inner.application_timeouts.load(Ordering::Relaxed),
            invalid_targets: self.inner.invalid_targets.load(Ordering::Relaxed),
            target_disappeared: self.inner.target_disappeared.load(Ordering::Relaxed),
            current_queue_depth: state.queue.len(),
            in_flight,
            last_latency_ms: stats.last_ms,
            avg_latency_ms,
        }
    }

    /// Observes an updated compositor snapshot, waking the worker to evaluate state convergence.
    pub fn observe_snapshot(&self, snapshot: Arc<CompositorSnapshot>) {
        let is_ready = snapshot.connection.is_ready();
        let mut state = self.inner.state.lock().unwrap();

        let is_initial_snapshot = state.snapshot.revision == 0
            && matches!(state.snapshot.connection, CompositorConnection::Connecting);
        if !is_initial_snapshot && snapshot.revision <= state.snapshot.revision {
            return;
        }

        let prev_ready = state.snapshot.connection.is_ready();
        state.snapshot = snapshot.clone();

        if prev_ready && !is_ready {
            self.inner
                .reconnect_transitions
                .fetch_add(1, Ordering::Relaxed);
        }

        let drained = if !is_ready || !prev_ready {
            // Reconnect or disconnect: increment epoch & cancel all queued commands
            self.inner.epoch.fetch_add(1, Ordering::SeqCst);

            let drained: Vec<_> = state.queue.drain(..).collect();
            if let Some(control) = self.inner.active.lock().unwrap().take() {
                control.cancel(CancellationReason::Reconnect);
            }
            drained
        } else {
            Vec::new()
        };
        let convergence = self.inner.convergence.lock().unwrap().clone();
        drop(state);
        for item in drained {
            let err = CompositorCommandError::Cancelled {
                reason: CancellationReason::Reconnect,
            };
            self.inner
                .record_terminal_outcome(item.submitted_at, &Err(err.clone()));
            let _ = item.tx.send(Err(err));
        }
        if let Some(convergence) = convergence
            && !convergence.cancellation.is_cancelled()
        {
            convergence.observe(snapshot);
        }
        self.inner.cv.notify_all();
    }

    /// Submits a command to the broker FIFO queue.
    pub fn submit(
        &self,
        command: CompositorCommand,
    ) -> Result<CommandTicket, CompositorCommandError> {
        let mut state = self.inner.state.lock().unwrap();
        let current_epoch = self.inner.epoch.load(Ordering::Acquire);

        if !state.snapshot.connection.is_ready() {
            let err = CompositorCommandError::Unavailable {
                state: state.snapshot.connection.clone(),
            };
            self.inner
                .rejected_unavailable
                .fetch_add(1, Ordering::Relaxed);
            return Err(err);
        }

        // Validate capability
        let cap_ok = match &command {
            CompositorCommand::CreateWorkspace => state.snapshot.capabilities.can_create_workspace,
            CompositorCommand::MoveWindowToWorkspace { .. } => {
                state.snapshot.capabilities.can_move_window
            }
            CompositorCommand::FocusWindow(_) => state.snapshot.capabilities.can_focus_window,
            CompositorCommand::FocusPreviousWindow => state.snapshot.capabilities.can_focus_window,
            CompositorCommand::FocusWorkspace(_) => state.snapshot.capabilities.can_focus_workspace,
            CompositorCommand::CloseWindow(_) => state.snapshot.capabilities.can_close_window,
        };

        if !cap_ok {
            let err = CompositorCommandError::Unsupported;
            self.inner
                .rejected_unsupported
                .fetch_add(1, Ordering::Relaxed);
            return Err(err);
        }

        if state.queue.len() >= self.inner.options.max_queue_len {
            let err = CompositorCommandError::Busy {
                queue_len: state.queue.len(),
            };
            self.inner.rejected_busy.fetch_add(1, Ordering::Relaxed);
            return Err(err);
        }

        self.inner.accepted.fetch_add(1, Ordering::Relaxed);

        let cmd_id = self.next_cmd_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        let cancellation = CommandCancellation::new();

        let pending = PendingCommand {
            _id: cmd_id,
            epoch: current_epoch,
            command,
            submitted_at: Instant::now(),
            deadline: Instant::now() + self.inner.options.timeout,
            tx,
            cancellation: cancellation.clone(),
        };

        state.queue.push_back(pending);
        self.inner.cv.notify_one();

        Ok(CommandTicket::new(rx, cancellation))
    }

    fn clear_active(broker: &BrokerInner, active: &ActiveCommand) {
        if broker
            .active
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &active.cancellation))
        {
            broker.active.lock().unwrap().take();
        }
        if broker
            .convergence
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(&current.convergence, &active.convergence))
        {
            broker.convergence.lock().unwrap().take();
        }
        broker.cv.notify_all();
    }

    fn target_invalid(
        command: &CompositorCommand,
        snapshot: &CompositorSnapshot,
    ) -> Option<CompositorCommandError> {
        match command {
            CompositorCommand::FocusWorkspace(workspace_id)
                if !snapshot
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.id == *workspace_id) =>
            {
                Some(CompositorCommandError::InvalidTarget(
                    CompositorTarget::Workspace(*workspace_id),
                ))
            }
            CompositorCommand::FocusWindow(window_id)
                if !snapshot
                    .windows
                    .iter()
                    .any(|window| window.id == *window_id) =>
            {
                Some(CompositorCommandError::InvalidTarget(
                    CompositorTarget::Window(*window_id),
                ))
            }
            CompositorCommand::MoveWindowToWorkspace {
                window_id,
                workspace_id,
            } if !snapshot
                .windows
                .iter()
                .any(|window| window.id == *window_id) =>
            {
                Some(CompositorCommandError::InvalidTarget(
                    CompositorTarget::Window(*window_id),
                ))
            }
            CompositorCommand::MoveWindowToWorkspace { workspace_id, .. }
                if !snapshot
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.id == *workspace_id) =>
            {
                Some(CompositorCommandError::InvalidTarget(
                    CompositorTarget::Workspace(*workspace_id),
                ))
            }
            _ => None,
        }
    }

    fn already_satisfied(command: &CompositorCommand, snapshot: &CompositorSnapshot) -> bool {
        match command {
            CompositorCommand::FocusWorkspace(workspace_id) => {
                snapshot.focused_workspace_id == Some(*workspace_id)
            }
            CompositorCommand::FocusWindow(window_id) => {
                snapshot.focused_window_id == Some(*window_id)
            }
            CompositorCommand::MoveWindowToWorkspace {
                window_id,
                workspace_id,
            } => snapshot.windows.iter().any(|window| {
                window.id == *window_id && window.workspace_id == Some(*workspace_id)
            }),
            CompositorCommand::FocusPreviousWindow => snapshot.windows.len() <= 1,
            CompositorCommand::CreateWorkspace => false,
            CompositorCommand::CloseWindow(_) => false,
        }
    }

    fn worker_loop(weak_broker: std::sync::Weak<BrokerInner>, executor: CommandExecutorFn) {
        loop {
            let broker = match weak_broker.upgrade() {
                Some(broker) => broker,
                None => break,
            };
            let pending = {
                let mut state = broker.state.lock().unwrap();
                loop {
                    if broker.shutdown.load(Ordering::Acquire) {
                        break None;
                    }
                    if !state.snapshot.connection.is_ready() {
                        state = broker.cv.wait(state).unwrap();
                        continue;
                    }
                    if let Some(item) = state.queue.pop_front() {
                        break Some((item, state.snapshot.clone()));
                    }
                    state = broker.cv.wait(state).unwrap();
                }
            };
            let Some((item, baseline)) = pending else {
                break;
            };

            let cancelled = || CompositorCommandError::Cancelled {
                reason: item
                    .cancellation
                    .reason()
                    .unwrap_or(CancellationReason::User),
            };
            if item.epoch != broker.epoch.load(Ordering::Acquire)
                || item.cancellation.is_cancelled()
            {
                let err = if item.cancellation.is_cancelled() {
                    cancelled()
                } else {
                    CompositorCommandError::Cancelled {
                        reason: CancellationReason::Reconnect,
                    }
                };
                broker.record_terminal_outcome(item.submitted_at, &Err(err.clone()));
                let _ = item.tx.send(Err(err));
                continue;
            }
            if Instant::now() >= item.deadline {
                let err = CompositorCommandError::Timeout {
                    duration: broker.options.timeout,
                };
                broker.record_terminal_outcome(item.submitted_at, &Err(err.clone()));
                let _ = item.tx.send(Err(err));
                continue;
            }
            if let Some(err) = Self::target_invalid(&item.command, &baseline) {
                broker.record_terminal_outcome(item.submitted_at, &Err(err.clone()));
                let _ = item.tx.send(Err(err));
                continue;
            }

            item.cancellation.attach_waker(broker.cv.clone());
            let already_satisfied = Self::already_satisfied(&item.command, &baseline);
            let active = ActiveCommand::new(
                item.command.clone(),
                baseline,
                already_satisfied,
                item.cancellation.clone(),
            );
            let activated = {
                let state = broker.state.lock().unwrap();
                if item.epoch != broker.epoch.load(Ordering::Acquire)
                    || !state.snapshot.connection.is_ready()
                {
                    false
                } else {
                    *broker.active.lock().unwrap() = Some(item.cancellation.clone());
                    *broker.convergence.lock().unwrap() = Some(active.clone());
                    true
                }
            };
            if !activated || item.cancellation.is_cancelled() {
                let err = if item.cancellation.is_cancelled() {
                    cancelled()
                } else {
                    CompositorCommandError::Cancelled {
                        reason: CancellationReason::Reconnect,
                    }
                };
                Self::clear_active(&broker, &active);
                broker.record_terminal_outcome(item.submitted_at, &Err(err.clone()));
                let _ = item.tx.send(Err(err));
                continue;
            }

            let remaining = item.deadline.saturating_duration_since(Instant::now());
            let execution = executor(
                &item.command,
                remaining,
                item.cancellation.clone(),
                &|handle| item.cancellation.register(handle),
            );
            item.cancellation.clear_handle();
            let result = if item.cancellation.is_cancelled() {
                Err(cancelled())
            } else {
                match execution {
                    Err(error) => Err(error),
                    Ok(_) if Instant::now() >= item.deadline => {
                        Err(CompositorCommandError::Timeout {
                            duration: broker.options.timeout,
                        })
                    }
                    Ok(ack) => match active.acknowledge(ack) {
                        Err(error) => Err(error),
                        Ok(()) => loop {
                            if item.cancellation.is_cancelled() {
                                break Err(cancelled());
                            }
                            if item.epoch != broker.epoch.load(Ordering::Acquire) {
                                break Err(CompositorCommandError::Cancelled {
                                    reason: CancellationReason::Reconnect,
                                });
                            }
                            if let Some(result) = active.terminal() {
                                break result;
                            }
                            let now = Instant::now();
                            if now >= item.deadline {
                                break Err(CompositorCommandError::ApplyTimeout {
                                    duration: broker.options.timeout,
                                    last_revision: active.last_revision(),
                                });
                            }

                            let state = broker.state.lock().unwrap();
                            if item.cancellation.is_cancelled() {
                                drop(state);
                                continue;
                            }
                            if let Some(result) = active.terminal() {
                                drop(state);
                                break result;
                            }
                            let remaining = item.deadline.saturating_duration_since(Instant::now());
                            let _ = broker.cv.wait_timeout(state, remaining).unwrap();
                        },
                    },
                }
            };
            Self::clear_active(&broker, &active);
            broker.record_terminal_outcome(item.submitted_at, &result);
            let _ = item.tx.send(result);
        }
    }
}

impl Drop for CompositorCommandBroker {
    fn drop(&mut self) {
        self.inner.shutdown.store(true, Ordering::Release);
        self.inner.cv.notify_all();

        if let Ok(mut state) = self.inner.state.lock() {
            for item in state.queue.drain(..) {
                let err = CompositorCommandError::Cancelled {
                    reason: CancellationReason::Shutdown,
                };
                self.inner
                    .record_terminal_outcome(item.submitted_at, &Err(err.clone()));
                let _ = item.tx.send(Err(err));
            }
        }

        if let Some(control) = self.inner.active.lock().unwrap().take() {
            control.cancel(CancellationReason::Shutdown);
        }

        if let Some(thread) = self.worker_thread.lock().unwrap().take() {
            let _ = thread.join();
        }
    }
}

/// Helper for standard socket stream cancellation registration.
pub fn create_stream_cancel_handle(stream: UnixStream) -> Arc<dyn StreamCancelHandle> {
    Arc::new(BasicStreamCancelHandle(Arc::new(Mutex::new(Some(stream)))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(id: u64, focused: bool) -> super::super::WorkspaceInfo {
        super::super::WorkspaceInfo {
            id,
            name: None,
            idx: id as u8,
            is_active: focused,
            is_focused: focused,
            is_urgent: false,
            output_name: None,
            active_window_id: None,
        }
    }

    fn window(id: u64, focused: bool) -> super::super::WindowInfo {
        super::super::WindowInfo {
            id,
            title: None,
            app_id: None,
            workspace_id: Some(1),
            is_focused: focused,
            is_floating: false,
            is_urgent: false,
            layout_x: None,
            layout_y: None,
            column: None,
            row: None,
        }
    }

    fn ready_snapshot(
        revision: u64,
        focused_workspace_id: Option<u64>,
        focused_window_id: Option<u64>,
    ) -> CompositorSnapshot {
        CompositorSnapshot {
            revision,
            connection: CompositorConnection::Ready,
            workspaces: vec![
                workspace(1, focused_workspace_id == Some(1)),
                workspace(2, focused_workspace_id == Some(2)),
            ],
            windows: vec![
                window(10, focused_window_id == Some(10)),
                window(20, focused_window_id == Some(20)),
            ],
            focused_workspace_id,
            focused_window_id,
            ..Default::default()
        }
    }

    fn wait_for_in_flight(broker: &CompositorCommandBroker) {
        let deadline = Instant::now() + Duration::from_secs(1);
        while !broker.telemetry().in_flight {
            assert!(
                Instant::now() < deadline,
                "broker did not start the command"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }

    fn wait_for_idle(broker: &CompositorCommandBroker) {
        let deadline = Instant::now() + Duration::from_secs(1);
        while broker.telemetry().in_flight {
            assert!(Instant::now() < deadline, "broker did not become idle");
            thread::sleep(Duration::from_millis(1));
        }
    }

    fn serial_guard() -> std::sync::MutexGuard<'static, ()> {
        crate::test_support::serial_guard()
    }

    #[test]
    fn test_broker_options_and_submission() {
        let _guard = serial_guard();
        let executor: CommandExecutorFn =
            Box::new(|_cmd, _timeout, _cancel, _register| Ok(ExecutorAck::Success));
        let options = BrokerOptions {
            timeout: Duration::from_millis(500),
            max_queue_len: 2,
        };
        let broker = CompositorCommandBroker::new(options, executor);
        let snapshot = CompositorSnapshot {
            connection: CompositorConnection::Ready,
            workspaces: vec![super::super::WorkspaceInfo {
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
        broker.observe_snapshot(Arc::new(snapshot));

        let t1 = broker.submit(CompositorCommand::FocusWorkspace(1)).unwrap();
        assert!(
            t1.wait_timeout(Duration::from_secs(1))
                .is_ok_and(|res| matches!(res, CommandOutcome::Applied { .. }))
        );
    }

    #[test]
    fn test_submission_when_not_ready() {
        let _guard = serial_guard();
        let executor: CommandExecutorFn =
            Box::new(|_cmd, _timeout, _cancel, _register| Ok(ExecutorAck::Success));
        let broker = CompositorCommandBroker::new(BrokerOptions::default(), executor);

        let err = broker
            .submit(CompositorCommand::FocusWorkspace(1))
            .unwrap_err();
        assert!(matches!(err, CompositorCommandError::Unavailable { .. }));
    }

    #[test]
    fn test_reconnect_cancels_pending() {
        let _guard = serial_guard();
        let executor: CommandExecutorFn = Box::new(|_cmd, _timeout, _cancel, _register| {
            thread::sleep(Duration::from_millis(100));
            Ok(ExecutorAck::Success)
        });
        let broker = CompositorCommandBroker::new(BrokerOptions::default(), executor);
        let snapshot = CompositorSnapshot {
            connection: CompositorConnection::Ready,
            workspaces: vec![
                super::super::WorkspaceInfo {
                    id: 1,
                    name: None,
                    idx: 1,
                    is_active: true,
                    is_focused: true,
                    is_urgent: false,
                    output_name: None,
                    active_window_id: None,
                },
                super::super::WorkspaceInfo {
                    id: 2,
                    name: None,
                    idx: 2,
                    is_active: false,
                    is_focused: false,
                    is_urgent: false,
                    output_name: None,
                    active_window_id: None,
                },
            ],
            focused_workspace_id: Some(1),
            ..Default::default()
        };
        broker.observe_snapshot(Arc::new(snapshot));

        let t1 = broker.submit(CompositorCommand::FocusWorkspace(1)).unwrap();
        let t2 = broker.submit(CompositorCommand::FocusWorkspace(2)).unwrap();

        broker.observe_snapshot(Arc::new(CompositorSnapshot {
            revision: 1,
            connection: CompositorConnection::Connecting,
            ..Default::default()
        }));

        let res = t2.wait_timeout(Duration::from_secs(1));
        assert!(matches!(
            res,
            Err(CompositorCommandError::Cancelled {
                reason: CancellationReason::Reconnect
            })
        ));
        let _ = t1.wait_timeout(Duration::from_secs(1));
    }

    #[test]
    fn test_invalid_target_rejected_immediately() {
        let _guard = serial_guard();
        let executor: CommandExecutorFn =
            Box::new(|_cmd, _timeout, _cancel, _register| Ok(ExecutorAck::Success));
        let broker = CompositorCommandBroker::new(BrokerOptions::default(), executor);
        let snapshot = CompositorSnapshot {
            connection: CompositorConnection::Ready,
            workspaces: vec![super::super::WorkspaceInfo {
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
        broker.observe_snapshot(Arc::new(snapshot));

        let t1 = broker
            .submit(CompositorCommand::FocusWorkspace(999))
            .unwrap();
        let res = t1.wait_timeout(Duration::from_secs(1));
        assert_eq!(
            res,
            Err(CompositorCommandError::InvalidTarget(
                CompositorTarget::Workspace(999)
            ))
        );
    }

    #[test]
    fn test_delayed_snapshot_convergence() {
        let _guard = serial_guard();
        let executor: CommandExecutorFn =
            Box::new(|_cmd, _timeout, _cancel, _register| Ok(ExecutorAck::Success));
        let broker = CompositorCommandBroker::new(
            BrokerOptions {
                timeout: Duration::from_secs(2),
                max_queue_len: 32,
            },
            executor,
        );
        let snap1 = CompositorSnapshot {
            revision: 1,
            connection: CompositorConnection::Ready,
            workspaces: vec![
                super::super::WorkspaceInfo {
                    id: 1,
                    name: None,
                    idx: 1,
                    is_active: true,
                    is_focused: true,
                    is_urgent: false,
                    output_name: None,
                    active_window_id: None,
                },
                super::super::WorkspaceInfo {
                    id: 2,
                    name: None,
                    idx: 2,
                    is_active: false,
                    is_focused: false,
                    is_urgent: false,
                    output_name: None,
                    active_window_id: None,
                },
            ],
            focused_workspace_id: Some(1),
            ..Default::default()
        };
        broker.observe_snapshot(Arc::new(snap1.clone()));

        let ticket = broker.submit(CompositorCommand::FocusWorkspace(2)).unwrap();

        thread::sleep(Duration::from_millis(50));

        let snap2 = CompositorSnapshot {
            revision: 2,
            focused_workspace_id: Some(2),
            ..snap1
        };
        broker.observe_snapshot(Arc::new(snap2));

        let res = ticket.wait_timeout(Duration::from_secs(1));
        assert_eq!(res, Ok(CommandOutcome::Applied { revision: 2 }));
    }

    #[test]
    fn test_target_disappeared_during_convergence() {
        let _guard = serial_guard();
        let executor: CommandExecutorFn =
            Box::new(|_cmd, _timeout, _cancel, _register| Ok(ExecutorAck::Success));
        let broker = CompositorCommandBroker::new(
            BrokerOptions {
                timeout: Duration::from_secs(2),
                max_queue_len: 32,
            },
            executor,
        );
        let snap1 = CompositorSnapshot {
            revision: 1,
            connection: CompositorConnection::Ready,
            workspaces: vec![
                super::super::WorkspaceInfo {
                    id: 1,
                    name: None,
                    idx: 1,
                    is_active: true,
                    is_focused: true,
                    is_urgent: false,
                    output_name: None,
                    active_window_id: None,
                },
                super::super::WorkspaceInfo {
                    id: 2,
                    name: None,
                    idx: 2,
                    is_active: false,
                    is_focused: false,
                    is_urgent: false,
                    output_name: None,
                    active_window_id: None,
                },
            ],
            focused_workspace_id: Some(1),
            ..Default::default()
        };
        broker.observe_snapshot(Arc::new(snap1.clone()));

        let ticket = broker.submit(CompositorCommand::FocusWorkspace(2)).unwrap();

        thread::sleep(Duration::from_millis(50));

        // Workspace 2 disappears before being focused
        let snap2 = CompositorSnapshot {
            revision: 2,
            workspaces: vec![super::super::WorkspaceInfo {
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
            ..snap1
        };
        broker.observe_snapshot(Arc::new(snap2));

        let res = ticket.wait_timeout(Duration::from_secs(1));
        assert_eq!(
            res,
            Err(CompositorCommandError::TargetDisappeared(
                CompositorTarget::Workspace(2)
            ))
        );
    }

    #[test]
    fn test_apply_timeout() {
        let _guard = serial_guard();
        let executor: CommandExecutorFn =
            Box::new(|_cmd, _timeout, _cancel, _register| Ok(ExecutorAck::Success));
        let broker = CompositorCommandBroker::new(
            BrokerOptions {
                timeout: Duration::from_millis(100),
                max_queue_len: 32,
            },
            executor,
        );
        let snap1 = CompositorSnapshot {
            revision: 1,
            connection: CompositorConnection::Ready,
            workspaces: vec![
                super::super::WorkspaceInfo {
                    id: 1,
                    name: None,
                    idx: 1,
                    is_active: true,
                    is_focused: true,
                    is_urgent: false,
                    output_name: None,
                    active_window_id: None,
                },
                super::super::WorkspaceInfo {
                    id: 2,
                    name: None,
                    idx: 2,
                    is_active: false,
                    is_focused: false,
                    is_urgent: false,
                    output_name: None,
                    active_window_id: None,
                },
            ],
            focused_workspace_id: Some(1),
            ..Default::default()
        };
        broker.observe_snapshot(Arc::new(snap1));

        let ticket = broker.submit(CompositorCommand::FocusWorkspace(2)).unwrap();
        let res = ticket.wait_timeout(Duration::from_secs(1));
        assert!(matches!(
            res,
            Err(CompositorCommandError::ApplyTimeout {
                duration: _,
                last_revision: 1
            })
        ));
    }

    #[test]
    fn test_convergence_preserves_a_matching_snapshot_seen_before_ack() {
        let _guard = serial_guard();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let release_rx = Arc::new(Mutex::new(release_rx));
        let executor: CommandExecutorFn = Box::new(move |_cmd, _timeout, _cancel, _register| {
            started_tx.send(()).unwrap();
            release_rx.lock().unwrap().recv().unwrap();
            Ok(ExecutorAck::Success)
        });
        let broker = CompositorCommandBroker::new(BrokerOptions::default(), executor);
        broker.observe_snapshot(Arc::new(ready_snapshot(1, Some(1), Some(10))));

        let ticket = broker.submit(CompositorCommand::FocusWorkspace(2)).unwrap();
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        broker.observe_snapshot(Arc::new(ready_snapshot(2, Some(2), Some(10))));
        broker.observe_snapshot(Arc::new(ready_snapshot(3, Some(1), Some(10))));
        release_tx.send(()).unwrap();

        assert_eq!(
            ticket.wait_timeout(Duration::from_secs(1)),
            Ok(CommandOutcome::Applied { revision: 2 })
        );
    }

    #[test]
    fn test_cancellation_wakes_convergence_without_waiting_for_deadline() {
        let _guard = serial_guard();
        let broker = CompositorCommandBroker::new(
            BrokerOptions {
                timeout: Duration::from_secs(2),
                max_queue_len: 32,
            },
            Box::new(|_cmd, _timeout, _cancel, _register| Ok(ExecutorAck::Success)),
        );
        broker.observe_snapshot(Arc::new(ready_snapshot(1, Some(1), Some(10))));

        let mut ticket = broker.submit(CompositorCommand::FocusWorkspace(2)).unwrap();
        thread::sleep(Duration::from_millis(25));
        ticket.cancel();
        let started = Instant::now();
        assert!(matches!(
            ticket.wait_timeout(Duration::from_millis(250)),
            Err(CompositorCommandError::Cancelled {
                reason: CancellationReason::User
            })
        ));
        assert!(started.elapsed() < Duration::from_millis(150));
    }

    #[test]
    fn test_dropping_a_ticket_releases_the_fifo_worker() {
        let _guard = serial_guard();
        let broker = CompositorCommandBroker::new(
            BrokerOptions {
                timeout: Duration::from_secs(2),
                max_queue_len: 32,
            },
            Box::new(|_cmd, _timeout, _cancel, _register| Ok(ExecutorAck::Success)),
        );
        broker.observe_snapshot(Arc::new(ready_snapshot(1, Some(1), Some(10))));

        let ticket = broker.submit(CompositorCommand::FocusWorkspace(2)).unwrap();
        wait_for_in_flight(&broker);
        let started = Instant::now();
        drop(ticket);
        wait_for_idle(&broker);
        assert!(started.elapsed() < Duration::from_millis(150));
    }

    #[test]
    fn test_deadline_includes_backend_ack_for_already_satisfied_commands() {
        let _guard = serial_guard();
        let broker = CompositorCommandBroker::new(
            BrokerOptions {
                timeout: Duration::from_millis(50),
                max_queue_len: 32,
            },
            Box::new(|_cmd, _timeout, _cancel, _register| {
                thread::sleep(Duration::from_millis(75));
                Ok(ExecutorAck::Success)
            }),
        );
        broker.observe_snapshot(Arc::new(ready_snapshot(1, Some(1), Some(10))));

        let ticket = broker.submit(CompositorCommand::FocusWorkspace(1)).unwrap();
        assert!(matches!(
            ticket.wait_timeout(Duration::from_secs(1)),
            Err(CompositorCommandError::Timeout { .. })
        ));
    }

    #[test]
    fn test_stale_reconnecting_snapshot_cannot_cancel_a_ready_command() {
        let _guard = serial_guard();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let release_rx = Arc::new(Mutex::new(release_rx));
        let broker = CompositorCommandBroker::new(
            BrokerOptions::default(),
            Box::new(move |_cmd, _timeout, _cancel, _register| {
                started_tx.send(()).unwrap();
                release_rx.lock().unwrap().recv().unwrap();
                Ok(ExecutorAck::Success)
            }),
        );
        broker.observe_snapshot(Arc::new(ready_snapshot(2, Some(1), Some(10))));

        let ticket = broker.submit(CompositorCommand::FocusWorkspace(1)).unwrap();
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        broker.observe_snapshot(Arc::new(CompositorSnapshot {
            revision: 1,
            connection: CompositorConnection::Reconnecting {
                attempt: 1,
                last_error: Some("stale".into()),
            },
            ..Default::default()
        }));
        release_tx.send(()).unwrap();

        assert_eq!(
            ticket.wait_timeout(Duration::from_secs(1)),
            Ok(CommandOutcome::Applied { revision: 2 })
        );
    }

    #[test]
    fn test_focus_previous_window_is_a_noop_when_no_alternative_exists() {
        let _guard = serial_guard();
        let broker = CompositorCommandBroker::new(
            BrokerOptions::default(),
            Box::new(|_cmd, _timeout, _cancel, _register| Ok(ExecutorAck::Success)),
        );
        let mut snapshot = ready_snapshot(1, Some(1), Some(10));
        snapshot.windows.truncate(1);
        broker.observe_snapshot(Arc::new(snapshot));

        let ticket = broker
            .submit(CompositorCommand::FocusPreviousWindow)
            .unwrap();
        assert_eq!(
            ticket.wait_timeout(Duration::from_secs(1)),
            Ok(CommandOutcome::Applied { revision: 1 })
        );
    }

    #[test]
    fn test_create_workspace_requires_a_resolved_workspace_acknowledgement() {
        let _guard = serial_guard();
        let broker = CompositorCommandBroker::new(
            BrokerOptions::default(),
            Box::new(|_cmd, _timeout, _cancel, _register| Ok(ExecutorAck::Success)),
        );
        broker.observe_snapshot(Arc::new(ready_snapshot(1, Some(1), Some(10))));

        let ticket = broker.submit(CompositorCommand::CreateWorkspace).unwrap();
        assert!(matches!(
            ticket.wait_timeout(Duration::from_secs(1)),
            Err(CompositorCommandError::BackendRejected { .. })
        ));
    }

    #[test]
    fn test_fifo_waits_for_convergence_before_starting_the_next_command() {
        let _guard = serial_guard();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let release_rx = Arc::new(Mutex::new(release_rx));
        let first = Arc::new(AtomicBool::new(true));
        let executor: CommandExecutorFn = {
            let first = first.clone();
            Box::new(move |command, _timeout, _cancel, _register| {
                started_tx.send(command.clone()).unwrap();
                if first.swap(false, Ordering::SeqCst) {
                    release_rx.lock().unwrap().recv().unwrap();
                }
                Ok(ExecutorAck::Success)
            })
        };
        let broker = CompositorCommandBroker::new(BrokerOptions::default(), executor);
        broker.observe_snapshot(Arc::new(ready_snapshot(1, Some(1), Some(10))));

        let first_ticket = broker.submit(CompositorCommand::FocusWorkspace(2)).unwrap();
        let second_ticket = broker.submit(CompositorCommand::FocusWorkspace(1)).unwrap();
        assert_eq!(
            started_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            CompositorCommand::FocusWorkspace(2)
        );
        assert!(started_rx.recv_timeout(Duration::from_millis(100)).is_err());

        release_tx.send(()).unwrap();
        broker.observe_snapshot(Arc::new(ready_snapshot(2, Some(2), Some(10))));
        assert_eq!(
            started_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            CompositorCommand::FocusWorkspace(1)
        );
        broker.observe_snapshot(Arc::new(ready_snapshot(3, Some(1), Some(10))));

        assert_eq!(
            first_ticket.wait_timeout(Duration::from_secs(1)),
            Ok(CommandOutcome::Applied { revision: 2 })
        );
        assert_eq!(
            second_ticket.wait_timeout(Duration::from_secs(1)),
            Ok(CommandOutcome::Applied { revision: 3 })
        );
    }

    #[test]
    fn test_focus_window_and_move_window_apply_only_after_their_postconditions() {
        let _guard = serial_guard();
        let broker = CompositorCommandBroker::new(
            BrokerOptions::default(),
            Box::new(|_cmd, _timeout, _cancel, _register| Ok(ExecutorAck::Success)),
        );
        broker.observe_snapshot(Arc::new(ready_snapshot(1, Some(1), Some(10))));

        let focus_ticket = broker.submit(CompositorCommand::FocusWindow(20)).unwrap();
        wait_for_in_flight(&broker);
        broker.observe_snapshot(Arc::new(ready_snapshot(2, Some(1), Some(20))));
        assert_eq!(
            focus_ticket.wait_timeout(Duration::from_secs(1)),
            Ok(CommandOutcome::Applied { revision: 2 })
        );

        let move_ticket = broker
            .submit(CompositorCommand::MoveWindowToWorkspace {
                window_id: 10,
                workspace_id: 2,
            })
            .unwrap();
        wait_for_in_flight(&broker);
        let mut moved = ready_snapshot(3, Some(2), Some(20));
        moved
            .windows
            .iter_mut()
            .find(|window| window.id == 10)
            .unwrap()
            .workspace_id = Some(2);
        broker.observe_snapshot(Arc::new(moved));
        assert_eq!(
            move_ticket.wait_timeout(Duration::from_secs(1)),
            Ok(CommandOutcome::Applied { revision: 3 })
        );
    }

    #[test]
    fn test_create_workspace_applies_to_its_acknowledged_workspace_and_detects_disappearance() {
        let _guard = serial_guard();
        let broker = CompositorCommandBroker::new(
            BrokerOptions::default(),
            Box::new(|_cmd, _timeout, _cancel, _register| {
                Ok(ExecutorAck::WorkspaceCreated { workspace_id: 2 })
            }),
        );
        broker.observe_snapshot(Arc::new(ready_snapshot(1, Some(1), Some(10))));

        let applied = broker.submit(CompositorCommand::CreateWorkspace).unwrap();
        wait_for_in_flight(&broker);
        broker.observe_snapshot(Arc::new(ready_snapshot(2, Some(2), Some(10))));
        assert_eq!(
            applied.wait_timeout(Duration::from_secs(1)),
            Ok(CommandOutcome::Applied { revision: 2 })
        );

        let disappeared = broker.submit(CompositorCommand::CreateWorkspace).unwrap();
        wait_for_in_flight(&broker);
        let mut missing_target = ready_snapshot(3, Some(1), Some(10));
        missing_target.workspaces.truncate(1);
        broker.observe_snapshot(Arc::new(missing_target));
        assert_eq!(
            disappeared.wait_timeout(Duration::from_secs(1)),
            Err(CompositorCommandError::TargetDisappeared(
                CompositorTarget::Workspace(2)
            ))
        );
    }
}
