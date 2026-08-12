use super::{
    CompositorCommand, CompositorConnection, CompositorSnapshot, DomainVersion, MailboxPolicy,
    RejectionReason, StaleUpdateError,
};
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

/// Terminal outcome returned when a compositor command finishes.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandOutcome {
    Applied {
        version: DomainVersion,
    },
    ReconciledApplied {
        version: DomainVersion,
    },
    Rejected {
        reason: RejectionReason,
    },
    TimedOut {
        last_observed_version: DomainVersion,
    },
    Cancelled {
        reason: CancellationReason,
    },
}

impl fmt::Display for CommandOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Applied { version } => write!(f, "command applied at version {}", version),
            Self::ReconciledApplied { version } => {
                write!(f, "command reconciled applied at version {}", version)
            }
            Self::Rejected { reason } => write!(f, "command rejected: {}", reason),
            Self::TimedOut {
                last_observed_version,
            } => write!(
                f,
                "command timed out (last observed version: {})",
                last_observed_version
            ),
            Self::Cancelled { reason } => write!(f, "command cancelled: {}", reason),
        }
    }
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
pub enum ExecutorAck {
    Success,
    WorkspaceCreated { workspace_id: u64 },
}

/// Reasons why a compositor command was cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancellationReason {
    Shutdown,
    Reconnect,
    OwnerReplaced,
    Superseded,
    User,
    Timeout,
}

impl fmt::Display for CancellationReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::User => write!(f, "cancelled by user or dropped ticket"),
            Self::Reconnect => write!(f, "compositor reconnected or changed state"),
            Self::Shutdown => write!(f, "broker shutdown"),
            Self::OwnerReplaced => write!(f, "owner generation replaced"),
            Self::Superseded => write!(f, "superseded by newer command"),
            Self::Timeout => write!(f, "command deadline elapsed"),
        }
    }
}

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
    rx: oneshot::Receiver<CommandOutcome>,
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
        rx: oneshot::Receiver<CommandOutcome>,
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
    pub fn wait_timeout(mut self, timeout: Duration) -> CommandOutcome {
        let start = Instant::now();
        loop {
            match self.rx.try_recv() {
                Ok(outcome) => {
                    self.completed = true;
                    return outcome;
                }
                Err(oneshot::error::TryRecvError::Closed) => {
                    self.completed = true;
                    return CommandOutcome::Cancelled {
                        reason: CancellationReason::Shutdown,
                    };
                }
                Err(oneshot::error::TryRecvError::Empty) => {
                    if start.elapsed() >= timeout {
                        self.cancellation.cancel(CancellationReason::Timeout);
                        self.completed = true;
                        return CommandOutcome::TimedOut {
                            last_observed_version: DomainVersion::ZERO,
                        };
                    }
                    thread::sleep(Duration::from_millis(5));
                }
            }
        }
    }

    /// Synchronously waits until command completion.
    pub fn wait(self) -> CommandOutcome {
        self.wait_timeout(Duration::from_secs(60))
    }
}

impl std::future::Future for CommandTicket {
    type Output = CommandOutcome;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        match std::pin::Pin::new(&mut self.rx).poll(cx) {
            std::task::Poll::Ready(res) => {
                self.completed = true;
                std::task::Poll::Ready(res.unwrap_or(CommandOutcome::Cancelled {
                    reason: CancellationReason::Shutdown,
                }))
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
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CompositorBrokerTelemetry {
    pub owner_generation: u64,
    pub current_queue_depth: usize,
    pub queue_capacity: usize,
    pub overloads: u64,
    pub supersessions: u64,
    pub restarts: u64,
    pub stale_updates: u64,
    pub quarantine_trips: u64,
    pub last_error: Option<String>,
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
    pub in_flight: bool,
    pub last_latency_ms: Option<u64>,
    pub avg_latency_ms: Option<u64>,
}

pub type CommandExecutorFn = Box<
    dyn Fn(
            &CompositorCommand,
            Duration,
            Arc<CommandCancellation>,
            &dyn Fn(Arc<dyn StreamCancelHandle>),
        ) -> Result<ExecutorAck, RejectionReason>
        + Send
        + Sync,
>;

struct PendingCommand {
    _id: u64,
    generation: u64,
    epoch: u64,
    command: CompositorCommand,
    policy: MailboxPolicy,
    submitted_at: Instant,
    deadline: Instant,
    tx: oneshot::Sender<CommandOutcome>,
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
    terminal: Option<CommandOutcome>,
    last_version: DomainVersion,
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
                last_version: baseline.version,
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
        if snapshot.version <= convergence.baseline.version || convergence.terminal.is_some() {
            return;
        }
        convergence.last_version = snapshot.version;
        if convergence.expectation.is_some() {
            convergence.evaluate(&snapshot);
        } else {
            convergence.observed_snapshots.push(snapshot);
        }
    }

    fn acknowledge(&self, ack: ExecutorAck) -> Result<(), RejectionReason> {
        let mut convergence = self.convergence.lock().unwrap();
        if convergence.already_satisfied {
            convergence.terminal = Some(CommandOutcome::Applied {
                version: convergence.baseline.version,
            });
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

    fn terminal(&self) -> Option<CommandOutcome> {
        self.convergence.lock().unwrap().terminal.clone()
    }

    fn last_version(&self) -> DomainVersion {
        self.convergence.lock().unwrap().last_version
    }
}

impl CommandExpectation {
    fn from_ack(
        command: &CompositorCommand,
        ack: &ExecutorAck,
        baseline: &CompositorSnapshot,
    ) -> Result<Self, RejectionReason> {
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
                Err(RejectionReason::BackendRejected {
                    message: "create-workspace acknowledgement did not identify a workspace".into(),
                })
            }
            _ => Err(RejectionReason::BackendRejected {
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
            self.terminal = Some(CommandOutcome::Applied {
                version: snapshot.version,
            });
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
            self.terminal = Some(CommandOutcome::Rejected {
                reason: RejectionReason::TargetDisappeared(target),
            });
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
    last_ms: Option<u64>,
}

struct BrokerInner {
    options: BrokerOptions,
    installed_generation: AtomicU64,
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
    overloads: AtomicU64,
    supersessions: AtomicU64,
    restarts: AtomicU64,
    stale_updates: AtomicU64,
    quarantine_trips: AtomicU64,
    latency_stats: Mutex<LatencyStats>,
}

impl BrokerInner {
    fn record_terminal_outcome(&self, submitted_at: Instant, outcome: &CommandOutcome) {
        let latency_ms = (submitted_at.elapsed().as_secs_f64() * 1000.0) as u64;
        {
            let mut stats = self.latency_stats.lock().unwrap();
            stats.total_ms_sum += latency_ms as f64;
            stats.count += 1;
            stats.last_ms = Some(latency_ms);
        }
        match outcome {
            CommandOutcome::Applied { .. } | CommandOutcome::ReconciledApplied { .. } => {
                self.succeeded.fetch_add(1, Ordering::Relaxed);
            }
            CommandOutcome::Rejected { reason } => match reason {
                RejectionReason::BackendRejected { .. } => {
                    self.backend_failed.fetch_add(1, Ordering::Relaxed);
                }
                RejectionReason::Transport { .. } => {
                    self.transport_failed.fetch_add(1, Ordering::Relaxed);
                }
                RejectionReason::Unavailable => {
                    self.rejected_unavailable.fetch_add(1, Ordering::Relaxed);
                }
                RejectionReason::Overloaded => {
                    self.rejected_busy.fetch_add(1, Ordering::Relaxed);
                }
                RejectionReason::Unsupported => {
                    self.rejected_unsupported.fetch_add(1, Ordering::Relaxed);
                }
                RejectionReason::InvalidTarget(_) => {
                    self.invalid_targets.fetch_add(1, Ordering::Relaxed);
                }
                RejectionReason::TargetDisappeared(_) => {
                    self.target_disappeared.fetch_add(1, Ordering::Relaxed);
                }
                RejectionReason::TimedOut | RejectionReason::Cancelled(_) => {}
            },
            CommandOutcome::TimedOut { .. } => {
                self.timed_out.fetch_add(1, Ordering::Relaxed);
            }
            CommandOutcome::Cancelled { .. } => {
                self.cancelled.fetch_add(1, Ordering::Relaxed);
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
    pub fn new(options: BrokerOptions, executor: CommandExecutorFn) -> Arc<Self> {
        let inner = Arc::new(BrokerInner {
            options,
            installed_generation: AtomicU64::new(0),
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
            overloads: AtomicU64::new(0),
            supersessions: AtomicU64::new(0),
            restarts: AtomicU64::new(0),
            stale_updates: AtomicU64::new(0),
            quarantine_trips: AtomicU64::new(0),
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

    pub fn set_installed_generation(&self, generation: u64) {
        self.inner
            .installed_generation
            .store(generation, Ordering::Release);
        self.inner.epoch.fetch_add(1, Ordering::SeqCst);
        let mut state = self.inner.state.lock().unwrap();
        let drained: Vec<_> = state.queue.drain(..).collect();
        if let Some(control) = self.inner.active.lock().unwrap().take() {
            control.cancel(CancellationReason::OwnerReplaced);
        }
        drop(state);
        for item in drained {
            let outcome = CommandOutcome::Cancelled {
                reason: CancellationReason::OwnerReplaced,
            };
            self.inner
                .record_terminal_outcome(item.submitted_at, &outcome);
            let _ = item.tx.send(outcome);
        }
        self.inner.cv.notify_all();
    }

    pub fn record_restart(&self) {
        self.inner.restarts.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_quarantine_trip(&self) {
        self.inner.quarantine_trips.fetch_add(1, Ordering::Relaxed);
    }

    pub fn telemetry(&self) -> CompositorBrokerTelemetry {
        let state = self.inner.state.lock().unwrap();
        let in_flight = self.inner.active.lock().unwrap().is_some();
        let stats = self.inner.latency_stats.lock().unwrap();
        let avg_latency_ms = if stats.count > 0 {
            Some((stats.total_ms_sum / stats.count as f64) as u64)
        } else {
            None
        };
        CompositorBrokerTelemetry {
            owner_generation: self.inner.installed_generation.load(Ordering::Relaxed),
            current_queue_depth: state.queue.len(),
            queue_capacity: self.inner.options.max_queue_len,
            overloads: self.inner.overloads.load(Ordering::Relaxed),
            supersessions: self.inner.supersessions.load(Ordering::Relaxed),
            restarts: self.inner.restarts.load(Ordering::Relaxed),
            stale_updates: self.inner.stale_updates.load(Ordering::Relaxed),
            quarantine_trips: self.inner.quarantine_trips.load(Ordering::Relaxed),
            last_error: state.snapshot.last_error.clone(),
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
            in_flight,
            last_latency_ms: stats.last_ms,
            avg_latency_ms,
        }
    }

    /// Observes an updated compositor snapshot, waking the worker to evaluate state convergence.
    pub fn observe_snapshot(
        &self,
        snapshot: Arc<CompositorSnapshot>,
    ) -> Result<(), StaleUpdateError> {
        let installed_gen = self.inner.installed_generation.load(Ordering::Acquire);
        let mut state = self.inner.state.lock().unwrap();

        let is_initial_snapshot = state.snapshot.version == DomainVersion::ZERO
            && matches!(
                state.snapshot.connection,
                CompositorConnection::Unavailable | CompositorConnection::Connecting
            );

        if snapshot.version.owner_generation > installed_gen {
            self.inner.stale_updates.fetch_add(1, Ordering::Relaxed);
            return Err(StaleUpdateError::UninstalledGeneration {
                installed: installed_gen,
                attempted: snapshot.version.owner_generation,
            });
        }

        if !is_initial_snapshot {
            if snapshot.version < state.snapshot.version {
                self.inner.stale_updates.fetch_add(1, Ordering::Relaxed);
                return Err(StaleUpdateError::StaleVersion {
                    current: state.snapshot.version,
                    attempted: snapshot.version,
                });
            }

            if snapshot.version == state.snapshot.version {
                if *state.snapshot == *snapshot {
                    return Ok(());
                }
                self.inner.stale_updates.fetch_add(1, Ordering::Relaxed);
                return Err(StaleUpdateError::ConflictingSnapshot {
                    version: state.snapshot.version,
                });
            }
        }

        let prev_ready = state.snapshot.connection.is_ready();
        let is_ready = snapshot.connection.is_ready();
        state.snapshot = snapshot.clone();

        if prev_ready && !is_ready {
            self.inner
                .reconnect_transitions
                .fetch_add(1, Ordering::Relaxed);
        }

        let drained = if !is_ready || !prev_ready {
            if !is_ready {
                self.inner.epoch.fetch_add(1, Ordering::SeqCst);
                let drained: Vec<_> = state.queue.drain(..).collect();
                if let Some(control) = self.inner.active.lock().unwrap().take() {
                    control.cancel(CancellationReason::Reconnect);
                }
                drained
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        let convergence = self.inner.convergence.lock().unwrap().clone();
        drop(state);
        for item in drained {
            let outcome = CommandOutcome::Cancelled {
                reason: CancellationReason::Reconnect,
            };
            self.inner
                .record_terminal_outcome(item.submitted_at, &outcome);
            let _ = item.tx.send(outcome);
        }
        if let Some(convergence) = convergence
            && !convergence.cancellation.is_cancelled()
        {
            convergence.observe(snapshot);
        }
        self.inner.cv.notify_all();
        Ok(())
    }

    /// Submits a command to the broker FIFO queue with Lossless policy.
    pub fn submit(&self, command: CompositorCommand) -> Result<CommandTicket, CommandOutcome> {
        self.submit_with_policy(command, MailboxPolicy::Lossless)
    }

    /// Submits a command to the broker FIFO queue with specified MailboxPolicy.
    pub fn submit_with_policy(
        &self,
        command: CompositorCommand,
        policy: MailboxPolicy,
    ) -> Result<CommandTicket, CommandOutcome> {
        let operation = format!("{:?}", std::mem::discriminant(&command));
        let _span = tracing::info_span!(
            target: "shilpo_profile",
            "service_command",
            domain = "compositor",
            operation = %operation,
            outcome = tracing::field::Empty,
        );
        let _enter = _span.enter();
        let mut state = self.inner.state.lock().unwrap();
        let current_epoch = self.inner.epoch.load(Ordering::Acquire);
        let current_gen = self.inner.installed_generation.load(Ordering::Acquire);

        if !state.snapshot.connection.is_ready() {
            let outcome = CommandOutcome::Rejected {
                reason: RejectionReason::Unavailable,
            };
            self.inner
                .rejected_unavailable
                .fetch_add(1, Ordering::Relaxed);
            return Err(outcome);
        }

        // Validate capability
        let cap_ok = match &command {
            CompositorCommand::CreateWorkspace => state.snapshot.capabilities.can_create_workspace,
            CompositorCommand::MoveWindowToWorkspace { .. } => {
                state.snapshot.capabilities.can_move_window
            }
            CompositorCommand::FocusWindow(_) | CompositorCommand::FocusPreviousWindow => {
                state.snapshot.capabilities.can_focus_window
            }
            CompositorCommand::FocusWorkspace(_) => state.snapshot.capabilities.can_focus_workspace,
            CompositorCommand::CloseWindow(_) => state.snapshot.capabilities.can_close_window,
        };

        if !cap_ok {
            let outcome = CommandOutcome::Rejected {
                reason: RejectionReason::Unsupported,
            };
            self.inner
                .rejected_unsupported
                .fetch_add(1, Ordering::Relaxed);
            return Err(outcome);
        }

        if let MailboxPolicy::ReplaceLatest { ref key } = policy {
            let mut replaced_idx = None;
            for (idx, item) in state.queue.iter().enumerate() {
                if let MailboxPolicy::ReplaceLatest {
                    key: ref existing_key,
                } = item.policy
                    && existing_key == key
                {
                    replaced_idx = Some(idx);
                    break;
                }
            }
            if let Some(idx) = replaced_idx {
                let removed = state.queue.remove(idx).unwrap();
                let cancel_outcome = CommandOutcome::Cancelled {
                    reason: CancellationReason::Superseded,
                };
                self.inner
                    .record_terminal_outcome(removed.submitted_at, &cancel_outcome);
                let _ = removed.tx.send(cancel_outcome);
                self.inner.supersessions.fetch_add(1, Ordering::Relaxed);
            }
        }

        if state.queue.len() >= self.inner.options.max_queue_len {
            let outcome = CommandOutcome::Rejected {
                reason: RejectionReason::Overloaded,
            };
            self.inner.overloads.fetch_add(1, Ordering::Relaxed);
            self.inner.rejected_busy.fetch_add(1, Ordering::Relaxed);
            return Err(outcome);
        }

        self.inner.accepted.fetch_add(1, Ordering::Relaxed);

        let cmd_id = self.next_cmd_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        let cancellation = CommandCancellation::new();

        let pending = PendingCommand {
            _id: cmd_id,
            generation: current_gen,
            epoch: current_epoch,
            command,
            policy,
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
    ) -> Option<RejectionReason> {
        match command {
            CompositorCommand::FocusWorkspace(workspace_id)
                if !snapshot
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.id == *workspace_id) =>
            {
                Some(RejectionReason::InvalidTarget(CompositorTarget::Workspace(
                    *workspace_id,
                )))
            }
            CompositorCommand::FocusWindow(window_id)
                if !snapshot
                    .windows
                    .iter()
                    .any(|window| window.id == *window_id) =>
            {
                Some(RejectionReason::InvalidTarget(CompositorTarget::Window(
                    *window_id,
                )))
            }
            CompositorCommand::MoveWindowToWorkspace {
                window_id,
                workspace_id,
            } if !snapshot
                .windows
                .iter()
                .any(|window| window.id == *window_id) =>
            {
                Some(RejectionReason::InvalidTarget(CompositorTarget::Window(
                    *window_id,
                )))
            }
            CompositorCommand::MoveWindowToWorkspace { workspace_id, .. }
                if !snapshot
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.id == *workspace_id) =>
            {
                Some(RejectionReason::InvalidTarget(CompositorTarget::Workspace(
                    *workspace_id,
                )))
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

            let installed_gen = broker.installed_generation.load(Ordering::Acquire);
            if (installed_gen > 0 && item.generation != installed_gen)
                || item.epoch != broker.epoch.load(Ordering::Acquire)
            {
                let outcome = CommandOutcome::Cancelled {
                    reason: CancellationReason::OwnerReplaced,
                };
                broker.record_terminal_outcome(item.submitted_at, &outcome);
                let _ = item.tx.send(outcome);
                continue;
            }

            let baseline_version = baseline.version;
            let cancelled = || match item
                .cancellation
                .reason()
                .unwrap_or(CancellationReason::User)
            {
                CancellationReason::Timeout => CommandOutcome::TimedOut {
                    last_observed_version: baseline_version,
                },
                reason => CommandOutcome::Cancelled { reason },
            };
            if item.cancellation.is_cancelled() {
                let outcome = cancelled();
                broker.record_terminal_outcome(item.submitted_at, &outcome);
                let _ = item.tx.send(outcome);
                continue;
            }
            if Instant::now() >= item.deadline {
                let outcome = CommandOutcome::TimedOut {
                    last_observed_version: baseline.version,
                };
                broker.record_terminal_outcome(item.submitted_at, &outcome);
                let _ = item.tx.send(outcome);
                continue;
            }
            if let Some(reason) = Self::target_invalid(&item.command, &baseline) {
                let outcome = CommandOutcome::Rejected { reason };
                broker.record_terminal_outcome(item.submitted_at, &outcome);
                let _ = item.tx.send(outcome);
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
                let outcome = if item.cancellation.is_cancelled() {
                    cancelled()
                } else {
                    CommandOutcome::Cancelled {
                        reason: CancellationReason::Reconnect,
                    }
                };
                Self::clear_active(&broker, &active);
                broker.record_terminal_outcome(item.submitted_at, &outcome);
                let _ = item.tx.send(outcome);
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
            let outcome = if item.cancellation.is_cancelled() {
                cancelled()
            } else {
                match execution {
                    Err(RejectionReason::TimedOut) => CommandOutcome::TimedOut {
                        last_observed_version: active.last_version(),
                    },
                    Err(RejectionReason::Cancelled(reason)) => CommandOutcome::Cancelled { reason },
                    Err(reason) => CommandOutcome::Rejected { reason },
                    Ok(_) if Instant::now() >= item.deadline => CommandOutcome::TimedOut {
                        last_observed_version: active.last_version(),
                    },
                    Ok(ack) => match active.acknowledge(ack) {
                        Err(reason) => CommandOutcome::Rejected { reason },
                        Ok(()) => loop {
                            if item.cancellation.is_cancelled() {
                                break cancelled();
                            }
                            if item.epoch != broker.epoch.load(Ordering::Acquire) {
                                break CommandOutcome::Cancelled {
                                    reason: CancellationReason::Reconnect,
                                };
                            }
                            if let Some(outcome) = active.terminal() {
                                break outcome;
                            }
                            let now = Instant::now();
                            if now >= item.deadline {
                                break CommandOutcome::TimedOut {
                                    last_observed_version: active.last_version(),
                                };
                            }

                            let state = broker.state.lock().unwrap();
                            if item.cancellation.is_cancelled() {
                                drop(state);
                                continue;
                            }
                            if let Some(outcome) = active.terminal() {
                                drop(state);
                                break outcome;
                            }
                            let remaining = item.deadline.saturating_duration_since(Instant::now());
                            let _ = broker.cv.wait_timeout(state, remaining).unwrap();
                        },
                    },
                }
            };
            Self::clear_active(&broker, &active);
            broker.record_terminal_outcome(item.submitted_at, &outcome);
            let _ = item.tx.send(outcome);
        }
    }
}

impl Drop for CompositorCommandBroker {
    fn drop(&mut self) {
        self.inner.shutdown.store(true, Ordering::Release);
        self.inner.cv.notify_all();

        if let Ok(mut state) = self.inner.state.lock() {
            for item in state.queue.drain(..) {
                let outcome = CommandOutcome::Cancelled {
                    reason: CancellationReason::Shutdown,
                };
                self.inner
                    .record_terminal_outcome(item.submitted_at, &outcome);
                let _ = item.tx.send(outcome);
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

    #[allow(dead_code)]
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

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    fn ready_snapshot(
        revision: u64,
        focused_workspace_id: Option<u64>,
        focused_window_id: Option<u64>,
    ) -> CompositorSnapshot {
        CompositorSnapshot {
            version: DomainVersion::new(1, revision),
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

    #[allow(dead_code)]
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
        broker.set_installed_generation(1);
        let snapshot = CompositorSnapshot {
            version: DomainVersion::new(1, 1),
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
        broker.observe_snapshot(Arc::new(snapshot)).unwrap();

        let t1 = broker.submit(CompositorCommand::FocusWorkspace(1)).unwrap();
        assert!(matches!(
            t1.wait_timeout(Duration::from_secs(1)),
            CommandOutcome::Applied { .. }
        ));
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
        assert_eq!(
            err,
            CommandOutcome::Rejected {
                reason: RejectionReason::Unavailable
            }
        );
    }

    #[test]
    fn test_reconnect_cancels_pending() {
        let _guard = serial_guard();
        let executor: CommandExecutorFn = Box::new(|_cmd, _timeout, _cancel, _register| {
            thread::sleep(Duration::from_millis(100));
            Ok(ExecutorAck::Success)
        });
        let broker = CompositorCommandBroker::new(BrokerOptions::default(), executor);
        broker.set_installed_generation(1);
        let snapshot = CompositorSnapshot {
            version: DomainVersion::new(1, 1),
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
        broker.observe_snapshot(Arc::new(snapshot)).unwrap();

        let t1 = broker.submit(CompositorCommand::FocusWorkspace(1)).unwrap();
        let t2 = broker.submit(CompositorCommand::FocusWorkspace(2)).unwrap();

        broker
            .observe_snapshot(Arc::new(CompositorSnapshot {
                version: DomainVersion::new(1, 2),
                connection: CompositorConnection::Reconnecting,
                ..Default::default()
            }))
            .unwrap();

        let res = t2.wait_timeout(Duration::from_secs(1));
        assert_eq!(
            res,
            CommandOutcome::Cancelled {
                reason: CancellationReason::Reconnect
            }
        );
        let _ = t1.wait_timeout(Duration::from_secs(1));
    }

    #[test]
    fn test_invalid_target_rejected_immediately() {
        let _guard = serial_guard();
        let executor: CommandExecutorFn =
            Box::new(|_cmd, _timeout, _cancel, _register| Ok(ExecutorAck::Success));
        let broker = CompositorCommandBroker::new(BrokerOptions::default(), executor);
        broker.set_installed_generation(1);
        let snapshot = CompositorSnapshot {
            version: DomainVersion::new(1, 1),
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
        broker.observe_snapshot(Arc::new(snapshot)).unwrap();

        let t1 = broker
            .submit(CompositorCommand::FocusWorkspace(999))
            .unwrap();
        let res = t1.wait_timeout(Duration::from_secs(1));
        assert_eq!(
            res,
            CommandOutcome::Rejected {
                reason: RejectionReason::InvalidTarget(CompositorTarget::Workspace(999))
            }
        );
    }
}
