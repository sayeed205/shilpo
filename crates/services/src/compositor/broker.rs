use super::{CompositorCapabilities, CompositorCommand, CompositorConnection};
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
    Unavailable { state: CompositorConnection },
    Busy { queue_len: usize },
    Unsupported,
    BackendRejected { message: String },
    Transport { message: String },
    Timeout { duration: Duration },
    Cancelled { reason: CancellationReason },
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
}

impl CommandCancellation {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            cancelled: AtomicBool::new(false),
            reason: Mutex::new(None),
            handle: Mutex::new(None),
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
    rx: oneshot::Receiver<Result<(), CompositorCommandError>>,
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
        rx: oneshot::Receiver<Result<(), CompositorCommandError>>,
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
    pub fn wait_timeout(mut self, timeout: Duration) -> Result<(), CompositorCommandError> {
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
    pub fn wait(self) -> Result<(), CompositorCommandError> {
        self.wait_timeout(Duration::from_secs(60))
    }
}

impl std::future::Future for CommandTicket {
    type Output = Result<(), CompositorCommandError>;

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
    pub current_queue_depth: usize,
    pub in_flight: bool,
    pub last_latency_ms: Option<f64>,
    pub avg_latency_ms: Option<f64>,
}

pub type CommandExecutorFn = Box<
    dyn Fn(
            &CompositorCommand,
            Duration,
            Arc<CommandCancellation>,
            &dyn Fn(Arc<dyn StreamCancelHandle>),
        ) -> Result<(), CompositorCommandError>
        + Send
        + Sync,
>;

struct PendingCommand {
    _id: u64,
    epoch: u64,
    command: CompositorCommand,
    submitted_at: Instant,
    deadline: Instant,
    tx: oneshot::Sender<Result<(), CompositorCommandError>>,
    cancellation: Arc<CommandCancellation>,
}

struct BrokerState {
    connection: CompositorConnection,
    capabilities: CompositorCapabilities,
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
    shutdown: AtomicBool,
    cv: std::sync::Condvar,
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
    latency_stats: Mutex<LatencyStats>,
}

impl BrokerInner {
    fn record_terminal_outcome(
        &self,
        submitted_at: Instant,
        result: &Result<(), CompositorCommandError>,
    ) {
        let latency_ms = submitted_at.elapsed().as_secs_f64() * 1000.0;
        {
            let mut stats = self.latency_stats.lock().unwrap();
            stats.total_ms_sum += latency_ms;
            stats.count += 1;
            stats.last_ms = Some(latency_ms);
        }
        match result {
            Ok(()) => {
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
            epoch: AtomicU64::new(1),
            state: Mutex::new(BrokerState {
                connection: CompositorConnection::Connecting,
                capabilities: CompositorCapabilities::default(),
                queue: VecDeque::new(),
            }),
            active: Mutex::new(None),
            shutdown: AtomicBool::new(false),
            cv: std::sync::Condvar::new(),
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
            current_queue_depth: state.queue.len(),
            in_flight,
            last_latency_ms: stats.last_ms,
            avg_latency_ms,
        }
    }

    /// Notifies the broker of a connection state and capabilities update.
    pub fn update_connection(
        &self,
        connection: CompositorConnection,
        capabilities: CompositorCapabilities,
    ) {
        let is_ready = connection.is_ready();
        let mut state = self.inner.state.lock().unwrap();

        let prev_ready = state.connection.is_ready();
        state.connection = connection;
        state.capabilities = capabilities;

        if prev_ready && !is_ready {
            self.inner
                .reconnect_transitions
                .fetch_add(1, Ordering::Relaxed);
        }

        if !is_ready || !prev_ready {
            // Reconnect or disconnect: increment epoch & cancel all queued commands
            self.inner.epoch.fetch_add(1, Ordering::SeqCst);

            let drained: Vec<_> = state.queue.drain(..).collect();
            // Hold the state lock while cancelling the active command. The worker
            // acquires these locks in the same order before starting I/O, closing
            // the reconnect/admission race.
            if let Some(control) = self.inner.active.lock().unwrap().take() {
                control.cancel(CancellationReason::Reconnect);
            }
            drop(state);

            for item in drained {
                let err = CompositorCommandError::Cancelled {
                    reason: CancellationReason::Reconnect,
                };
                self.inner
                    .record_terminal_outcome(item.submitted_at, &Err(err.clone()));
                let _ = item.tx.send(Err(err));
            }
        } else {
            self.inner.cv.notify_one();
        }
    }

    /// Submits a command to the broker FIFO queue.
    pub fn submit(
        &self,
        command: CompositorCommand,
    ) -> Result<CommandTicket, CompositorCommandError> {
        let mut state = self.inner.state.lock().unwrap();
        let current_epoch = self.inner.epoch.load(Ordering::Acquire);

        if !state.connection.is_ready() {
            let err = CompositorCommandError::Unavailable {
                state: state.connection.clone(),
            };
            self.inner
                .rejected_unavailable
                .fetch_add(1, Ordering::Relaxed);
            return Err(err);
        }

        // Validate capability
        let cap_ok = match &command {
            CompositorCommand::CreateWorkspace => state.capabilities.can_create_workspace,
            CompositorCommand::MoveWindowToWorkspace { .. } => state.capabilities.can_move_window,
            CompositorCommand::FocusWindow(_) => state.capabilities.can_focus_window,
            CompositorCommand::FocusPreviousWindow => state.capabilities.can_focus_window,
            CompositorCommand::FocusWorkspace(_) => state.capabilities.can_focus_workspace,
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

    fn worker_loop(weak_broker: std::sync::Weak<BrokerInner>, executor: CommandExecutorFn) {
        loop {
            let broker = match weak_broker.upgrade() {
                Some(b) => b,
                None => break,
            };

            if broker.shutdown.load(Ordering::Acquire) {
                break;
            }

            let pending = {
                let mut state = broker.state.lock().unwrap();
                loop {
                    if broker.shutdown.load(Ordering::Acquire) {
                        break None;
                    }
                    if !state.connection.is_ready() {
                        state = broker.cv.wait(state).unwrap();
                        continue;
                    }
                    if let Some(item) = state.queue.pop_front() {
                        break Some(item);
                    }
                    state = broker.cv.wait(state).unwrap();
                }
            };

            let item = match pending {
                Some(item) => item,
                None => break,
            };

            let state = broker.state.lock().unwrap();
            let current_epoch = broker.epoch.load(Ordering::Acquire);
            if item.epoch != current_epoch {
                drop(state);
                let err = CompositorCommandError::Cancelled {
                    reason: CancellationReason::Reconnect,
                };
                broker.record_terminal_outcome(item.submitted_at, &Err(err.clone()));
                let _ = item.tx.send(Err(err));
                continue;
            }

            if item.cancellation.is_cancelled() {
                drop(state);
                let err = CompositorCommandError::Cancelled {
                    reason: item
                        .cancellation
                        .reason()
                        .unwrap_or(CancellationReason::User),
                };
                broker.record_terminal_outcome(item.submitted_at, &Err(err.clone()));
                let _ = item.tx.send(Err(err));
                continue;
            }

            let now = Instant::now();
            if now >= item.deadline {
                drop(state);
                let err = CompositorCommandError::Timeout {
                    duration: broker.options.timeout,
                };
                broker.record_terminal_outcome(item.submitted_at, &Err(err.clone()));
                let _ = item.tx.send(Err(err));
                continue;
            }

            let remaining = item.deadline - now;
            let cancellation = item.cancellation.clone();
            *broker.active.lock().unwrap() = Some(cancellation.clone());
            drop(state);

            let register_cancel = move |handle: Arc<dyn StreamCancelHandle>| {
                cancellation.register(handle);
            };

            let result = executor(
                &item.command,
                remaining,
                item.cancellation.clone(),
                &register_cancel,
            );

            item.cancellation.clear_handle();
            *broker.active.lock().unwrap() = None;

            // Cancellation is authoritative once it has been observed.  The
            // transport may race with shutdown and still return a successful
            // reply; exposing that reply would make reconnect/user-cancel
            // semantics nondeterministic.
            let final_res = if item.cancellation.is_cancelled() {
                Err(CompositorCommandError::Cancelled {
                    reason: item
                        .cancellation
                        .reason()
                        .unwrap_or(CancellationReason::User),
                })
            } else {
                result
            };

            broker.record_terminal_outcome(item.submitted_at, &final_res);
            let _ = item.tx.send(final_res);
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

    fn serial_guard() -> std::sync::MutexGuard<'static, ()> {
        crate::test_support::serial_guard()
    }

    #[test]
    fn test_broker_options_and_submission() {
        let _guard = serial_guard();
        let executor: CommandExecutorFn = Box::new(|_cmd, _timeout, _cancel, _register| Ok(()));
        let options = BrokerOptions {
            timeout: Duration::from_millis(500),
            max_queue_len: 2,
        };
        let broker = CompositorCommandBroker::new(options, executor);
        broker.update_connection(
            CompositorConnection::Ready,
            CompositorCapabilities::default(),
        );

        let t1 = broker.submit(CompositorCommand::FocusWorkspace(1)).unwrap();
        assert!(t1.wait_timeout(Duration::from_secs(1)).is_ok());
    }

    #[test]
    fn test_submission_when_not_ready() {
        let _guard = serial_guard();
        let executor: CommandExecutorFn = Box::new(|_cmd, _timeout, _cancel, _register| Ok(()));
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
            Ok(())
        });
        let broker = CompositorCommandBroker::new(BrokerOptions::default(), executor);
        broker.update_connection(
            CompositorConnection::Ready,
            CompositorCapabilities::default(),
        );

        let t1 = broker.submit(CompositorCommand::FocusWorkspace(1)).unwrap();
        let t2 = broker.submit(CompositorCommand::FocusWorkspace(2)).unwrap();

        broker.update_connection(
            CompositorConnection::Connecting,
            CompositorCapabilities::default(),
        );

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
    fn test_reconnect_interrupts_active_command() {
        let _guard = serial_guard();
        struct TestHandle(Arc<AtomicBool>);
        impl StreamCancelHandle for TestHandle {
            fn interrupt(&self) {
                self.0.store(true, Ordering::Release);
            }
        }

        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let executor: CommandExecutorFn = Box::new(move |_cmd, _timeout, cancel, register| {
            let interrupted = Arc::new(AtomicBool::new(false));
            register(Arc::new(TestHandle(interrupted.clone())));
            started_tx.send(()).unwrap();
            while !cancel.is_cancelled() && !interrupted.load(Ordering::Acquire) {
                thread::yield_now();
            }
            Err(CompositorCommandError::Cancelled {
                reason: cancel.reason().unwrap_or(CancellationReason::User),
            })
        });
        let broker = CompositorCommandBroker::new(BrokerOptions::default(), executor);
        broker.update_connection(
            CompositorConnection::Ready,
            CompositorCapabilities::default(),
        );
        let ticket = broker.submit(CompositorCommand::FocusWorkspace(1)).unwrap();
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();

        broker.update_connection(
            CompositorConnection::Reconnecting {
                attempt: 1,
                last_error: Some("test".into()),
            },
            CompositorCapabilities::default(),
        );

        assert_eq!(
            ticket.wait_timeout(Duration::from_secs(1)),
            Err(CompositorCommandError::Cancelled {
                reason: CancellationReason::Reconnect,
            })
        );
    }

    #[test]
    fn test_queue_overflow_returns_busy() {
        let _guard = serial_guard();
        let started = Arc::new(std::sync::Barrier::new(2));
        let started_executor = started.clone();
        let executor: CommandExecutorFn = Box::new(move |cmd, _timeout, _cancel, _register| {
            if matches!(cmd, CompositorCommand::FocusWorkspace(1)) {
                started_executor.wait();
            }
            thread::sleep(Duration::from_millis(200));
            Ok(())
        });
        let options = BrokerOptions {
            timeout: Duration::from_secs(2),
            max_queue_len: 1,
        };
        let broker = CompositorCommandBroker::new(options, executor);
        broker.update_connection(
            CompositorConnection::Ready,
            CompositorCapabilities::default(),
        );

        let _t1 = broker.submit(CompositorCommand::FocusWorkspace(1)).unwrap();
        started.wait();
        let _t2 = broker.submit(CompositorCommand::FocusWorkspace(2)).unwrap();
        let err = broker
            .submit(CompositorCommand::FocusWorkspace(3))
            .unwrap_err();

        assert!(matches!(err, CompositorCommandError::Busy { queue_len: 1 }));
    }

    #[test]
    fn test_unsupported_capability_returns_unsupported() {
        let _guard = serial_guard();
        let executor: CommandExecutorFn = Box::new(|_cmd, _timeout, _cancel, _register| Ok(()));
        let broker = CompositorCommandBroker::new(BrokerOptions::default(), executor);

        let caps = CompositorCapabilities {
            can_create_workspace: false,
            can_move_window: true,
            can_focus_window: true,
            can_focus_workspace: true,
        };
        broker.update_connection(CompositorConnection::Ready, caps);

        let err = broker
            .submit(CompositorCommand::CreateWorkspace)
            .unwrap_err();
        assert_eq!(err, CompositorCommandError::Unsupported);
    }

    #[test]
    fn test_explicit_cancellation_and_drop() {
        let _guard = serial_guard();
        let executor: CommandExecutorFn = Box::new(|_cmd, _timeout, _cancel, _register| {
            thread::sleep(Duration::from_millis(50));
            Ok(())
        });
        let broker = CompositorCommandBroker::new(BrokerOptions::default(), executor);
        broker.update_connection(
            CompositorConnection::Ready,
            CompositorCapabilities::default(),
        );

        let mut t1 = broker.submit(CompositorCommand::FocusWorkspace(1)).unwrap();
        t1.cancel();
        let res = t1.wait_timeout(Duration::from_secs(1));
        assert!(matches!(
            res,
            Err(CompositorCommandError::Cancelled {
                reason: CancellationReason::User
            })
        ));

        let t2 = broker.submit(CompositorCommand::FocusWorkspace(2)).unwrap();
        drop(t2);
    }

    #[test]
    fn test_fifo_execution_order() {
        let _guard = serial_guard();
        let executed = Arc::new(Mutex::new(Vec::new()));
        let exec_clone = executed.clone();

        let executor: CommandExecutorFn = Box::new(move |cmd, _timeout, _cancel, _register| {
            if let CompositorCommand::FocusWorkspace(id) = cmd {
                exec_clone.lock().unwrap().push(*id);
            }
            Ok(())
        });

        let broker = CompositorCommandBroker::new(BrokerOptions::default(), executor);
        broker.update_connection(
            CompositorConnection::Ready,
            CompositorCapabilities::default(),
        );

        let t1 = broker
            .submit(CompositorCommand::FocusWorkspace(10))
            .unwrap();
        let t2 = broker
            .submit(CompositorCommand::FocusWorkspace(20))
            .unwrap();
        let t3 = broker
            .submit(CompositorCommand::FocusWorkspace(30))
            .unwrap();

        assert!(t1.wait_timeout(Duration::from_secs(1)).is_ok());
        assert!(t2.wait_timeout(Duration::from_secs(1)).is_ok());
        assert!(t3.wait_timeout(Duration::from_secs(1)).is_ok());

        assert_eq!(*executed.lock().unwrap(), vec![10, 20, 30]);
    }

    #[test]
    fn test_telemetry_counters_and_metrics() {
        let _guard = serial_guard();
        let executor: CommandExecutorFn = Box::new(|cmd, _timeout, _cancel, _register| {
            if let CompositorCommand::FocusWorkspace(999) = cmd {
                Err(CompositorCommandError::BackendRejected {
                    message: "rejected test".into(),
                })
            } else {
                thread::sleep(Duration::from_millis(10));
                Ok(())
            }
        });
        let broker = CompositorCommandBroker::new(BrokerOptions::default(), executor);

        // Test rejected_unavailable counter
        let err = broker
            .submit(CompositorCommand::FocusWorkspace(1))
            .unwrap_err();
        assert!(matches!(err, CompositorCommandError::Unavailable { .. }));
        let telem = broker.telemetry();
        assert_eq!(telem.rejected_unavailable, 1);
        assert_eq!(telem.accepted, 0);

        // Transition to Ready and exercise unsupported admission.
        broker.update_connection(
            CompositorConnection::Ready,
            CompositorCapabilities {
                can_create_workspace: false,
                ..CompositorCapabilities::default()
            },
        );
        assert!(matches!(
            broker.submit(CompositorCommand::CreateWorkspace),
            Err(CompositorCommandError::Unsupported)
        ));

        // Enable the normal command capabilities.
        broker.update_connection(
            CompositorConnection::Ready,
            CompositorCapabilities::default(),
        );

        // Submit successful command
        let t1 = broker.submit(CompositorCommand::FocusWorkspace(1)).unwrap();
        assert!(t1.wait_timeout(Duration::from_secs(1)).is_ok());

        // Submit failing command
        let t2 = broker
            .submit(CompositorCommand::FocusWorkspace(999))
            .unwrap();
        assert!(t2.wait_timeout(Duration::from_secs(1)).is_err());

        let telem2 = broker.telemetry();
        assert_eq!(telem2.accepted, 2);
        assert_eq!(telem2.succeeded, 1);
        assert_eq!(telem2.backend_failed, 1);
        assert_eq!(telem2.rejected_unsupported, 1);
        assert!(telem2.last_latency_ms.is_some());
        assert!(telem2.avg_latency_ms.is_some());

        // Test reconnect transition counter
        broker.update_connection(
            CompositorConnection::Reconnecting {
                attempt: 1,
                last_error: Some("test error".into()),
            },
            CompositorCapabilities::default(),
        );
        let telem3 = broker.telemetry();
        assert_eq!(telem3.reconnect_transitions, 1);
    }

    #[test]
    fn test_telemetry_tracks_queue_busy_and_cancellation() {
        let _guard = serial_guard();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let executor: CommandExecutorFn = Box::new(move |cmd, _timeout, _cancel, _register| {
            if matches!(cmd, CompositorCommand::FocusWorkspace(1)) {
                started_tx.send(()).unwrap();
                thread::sleep(Duration::from_millis(100));
            }
            Ok(())
        });
        let broker = CompositorCommandBroker::new(
            BrokerOptions {
                timeout: Duration::from_secs(1),
                max_queue_len: 1,
            },
            executor,
        );
        broker.update_connection(
            CompositorConnection::Ready,
            CompositorCapabilities::default(),
        );

        let t1 = broker.submit(CompositorCommand::FocusWorkspace(1)).unwrap();
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let mut t2 = broker.submit(CompositorCommand::FocusWorkspace(2)).unwrap();
        assert!(matches!(
            broker.submit(CompositorCommand::FocusWorkspace(3)),
            Err(CompositorCommandError::Busy { .. })
        ));

        let during = broker.telemetry();
        assert_eq!(during.current_queue_depth, 1);
        assert!(during.in_flight);
        assert_eq!(during.rejected_busy, 1);

        t2.cancel();
        assert!(t1.wait_timeout(Duration::from_secs(1)).is_ok());
        assert!(matches!(
            t2.wait_timeout(Duration::from_secs(1)),
            Err(CompositorCommandError::Cancelled {
                reason: CancellationReason::User
            })
        ));

        let after = broker.telemetry();
        assert_eq!(after.current_queue_depth, 0);
        assert!(!after.in_flight);
        assert_eq!(after.cancelled, 1);
    }
}
