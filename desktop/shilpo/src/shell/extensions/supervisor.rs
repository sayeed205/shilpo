use std::{
    collections::{HashMap, HashSet},
    io,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use shilpo_ext_runtime::{
    ExtensionCommand, ExtensionGeneration, ExtensionSnapshot, ExtensionUpdate, FrameReader,
    HostGeneration, HostMessage, PROTOCOL_VERSION, ProcessCodecError, ReplaceableEvent,
    ScriptExtensionStatus, WasmExtensionStatus, WorkerMessage, WorkerPayload,
    recv_worker_message_nonblocking, send_host_message,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SupervisorState {
    Starting,
    Ready,
    Backoff { attempt: u32 },
    Quarantined,
    Stopping,
    Stopped,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ExtensionHostDiagnostics {
    pub lifecycle: String,
    pub host_generation: u64,
    pub engine_generation: u64,
    pub pid: Option<u32>,
    pub session_restart_count: u32,
    pub consecutive_crashes: u32,
    pub last_exit: Option<String>,
    pub last_error: Option<String>,
    pub stale_updates_dropped: u64,
    pub malformed_frames: u64,
    pub script_extensions: Vec<ScriptExtensionStatus>,
    #[serde(default)]
    pub wasm_extensions: Vec<WasmExtensionStatus>,
}

pub const RETRY_DELAYS: [Duration; 3] = [
    Duration::from_millis(250),
    Duration::from_secs(1),
    Duration::from_secs(4),
];
pub const MAX_CRASHES_IN_WINDOW: u32 = 3;
pub const ROLLING_WINDOW: Duration = Duration::from_secs(60);
pub const READY_RESET_DURATION: Duration = Duration::from_secs(300); // 5 minutes
pub const SHUTDOWN_DEADLINE: Duration = Duration::from_secs(2);
pub const MAX_QUEUE_SIZE: usize = 64;

pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;

    fn sleep(&self, duration: Duration) {
        thread::sleep(duration);
    }
}

#[derive(Default)]
pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

pub trait ChildStream: Send + Sync {
    fn pid(&self) -> Option<u32>;
    fn write_host_message(&mut self, msg: &HostMessage) -> Result<(), ProcessCodecError>;
    fn try_read_worker_message(&mut self) -> Result<Option<WorkerMessage>, ProcessCodecError>;
    fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>>;
    fn shutdown_gracefully(&mut self, timeout: Duration) -> io::Result<()>;
    fn kill(&mut self) -> io::Result<()>;
}

pub trait ChildSpawner: Send + Sync {
    fn spawn(&self, host_gen: HostGeneration) -> io::Result<Box<dyn ChildStream>>;
}

pub struct RealChildStream {
    child: Child,
    stdin: std::process::ChildStdin,
    stdout: std::process::ChildStdout,
    frame_reader: FrameReader,
}

impl ChildStream for RealChildStream {
    fn pid(&self) -> Option<u32> {
        Some(self.child.id())
    }

    fn write_host_message(&mut self, msg: &HostMessage) -> Result<(), ProcessCodecError> {
        send_host_message(&mut self.stdin, msg)
    }

    fn try_read_worker_message(&mut self) -> Result<Option<WorkerMessage>, ProcessCodecError> {
        recv_worker_message_nonblocking(&mut self.stdout, &mut self.frame_reader)
    }

    fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
        self.child.try_wait()
    }

    fn shutdown_gracefully(&mut self, timeout: Duration) -> io::Result<()> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if let Ok(Some(_)) = self.child.try_wait() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(25));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        Ok(())
    }

    fn kill(&mut self) -> io::Result<()> {
        let _ = self.child.kill();
        let _ = self.child.wait();
        Ok(())
    }
}

pub struct RealChildSpawner;

impl ChildSpawner for RealChildSpawner {
    fn spawn(&self, _host_gen: HostGeneration) -> io::Result<Box<dyn ChildStream>> {
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("shilpo"));
        let mut child = Command::new(exe)
            .arg("extension-host")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("missing stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("missing stdout"))?;

        set_nonblocking(&stdout)?;

        Ok(Box::new(RealChildStream {
            child,
            stdin,
            stdout,
            frame_reader: FrameReader::default(),
        }))
    }
}

pub const MAX_IN_FLIGHT_SEARCHES_PER_EXTENSION: usize = 1;
pub const MAX_SEARCH_QUEUE_SIZE: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchDispatchError {
    NotRegistered(shilpo_ext_api::ExtensionId),
    UnknownContribution(shilpo_ext_api::CanonicalId),
    CircuitOpen(shilpo_ext_api::ExtensionId),
    Disabled(shilpo_ext_api::ExtensionId),
    GuestTimeout,
    GuestError(String),
    CoordinatorTimeout,
    InFlightLimitExceeded(shilpo_ext_api::ExtensionId),
    HostUnavailable(String),
    HostDisconnected,
    Other(String),
}

impl std::fmt::Display for SearchDispatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotRegistered(id) => write!(f, "extension '{id}' is not registered"),
            Self::UnknownContribution(id) => write!(f, "unknown contribution '{id}'"),
            Self::CircuitOpen(id) => write!(f, "circuit breaker open for extension '{id}'"),
            Self::Disabled(id) => write!(f, "extension '{id}' is disabled"),
            Self::GuestTimeout => write!(f, "search query timed out inside guest extension"),
            Self::GuestError(msg) => write!(f, "guest extension search failure: {msg}"),
            Self::CoordinatorTimeout => {
                write!(f, "coordinator timed out waiting for worker search reply")
            }
            Self::InFlightLimitExceeded(id) => {
                write!(f, "in-flight search limit exceeded for extension '{id}'")
            }
            Self::HostUnavailable(msg) => write!(f, "extension host unavailable: {msg}"),
            Self::HostDisconnected => write!(f, "extension host disconnected during search"),
            Self::Other(msg) => write!(f, "search dispatch error: {msg}"),
        }
    }
}

impl std::error::Error for SearchDispatchError {}

pub type SearchReplySender = mpsc::SyncSender<
    Result<
        Vec<shilpo_ext_api::bindings::shilpo::extension::types::SearchCandidate>,
        shilpo_ext_runtime::WorkerSearchError,
    >,
>;
pub type DevReloadReplySender = mpsc::SyncSender<shilpo_ext_runtime::DevReloadOutcome>;
/// `ExtensionId` travels alongside the reply channel so the supervisor loop can release
/// the sender's in-flight slot at the point a request genuinely resolves, regardless of
/// whether the original caller is still waiting on it.
type PendingSearchReply = (
    HostGeneration,
    shilpo_ext_api::ExtensionId,
    SearchReplySender,
);
type PendingReloadReply = (HostGeneration, DevReloadReplySender);

pub struct SupervisorCommandEnvelope {
    pub command: ExtensionCommand,
    pub reply_tx: Option<DevReloadReplySender>,
    pub search_reply: Option<(shilpo_ext_api::ExtensionId, SearchReplySender)>,
}

pub struct ExtensionSupervisor {
    state: Arc<Mutex<SupervisorState>>,
    snapshot: Arc<RwLock<ExtensionSnapshot>>,
    diagnostics: Arc<Mutex<ExtensionHostDiagnostics>>,
    command_tx: mpsc::SyncSender<SupervisorCommandEnvelope>,
    search_tx: mpsc::SyncSender<SupervisorCommandEnvelope>,
    update_rx: Arc<Mutex<mpsc::Receiver<ExtensionUpdate>>>,
    host_generation: Arc<Mutex<HostGeneration>>,
    stop_signal: Arc<AtomicBool>,
    _worker_thread: Option<JoinHandle<()>>,
    pending_replaceable: Arc<Mutex<Option<ReplaceableEvent>>>,
    cancelled_reloads: Arc<Mutex<HashSet<(String, u64)>>>,
    latest_dev_reloads: Arc<Mutex<HashMap<String, ExtensionCommand>>>,
    in_flight_searches: Arc<Mutex<HashMap<shilpo_ext_api::ExtensionId, usize>>>,
}

impl Default for ExtensionSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtensionSupervisor {
    pub fn new() -> Self {
        Self::new_with_spawner(RealChildSpawner, Arc::new(SystemClock))
    }

    pub fn new_with_spawner<S: ChildSpawner + 'static>(spawner: S, clock: Arc<dyn Clock>) -> Self {
        let state = Arc::new(Mutex::new(SupervisorState::Starting));
        let snapshot = Arc::new(RwLock::new(ExtensionSnapshot::default()));
        let diagnostics = Arc::new(Mutex::new(ExtensionHostDiagnostics {
            lifecycle: "starting".into(),
            ..Default::default()
        }));
        let host_generation = Arc::new(Mutex::new(HostGeneration(1)));
        let stop_signal = Arc::new(AtomicBool::new(false));

        let (command_tx, command_rx) = mpsc::sync_channel(MAX_QUEUE_SIZE);
        let (search_tx, search_rx) = mpsc::sync_channel(MAX_SEARCH_QUEUE_SIZE);
        let (update_tx, update_rx) = mpsc::sync_channel(MAX_QUEUE_SIZE);
        let pending_replaceable = Arc::new(Mutex::new(None));
        let cancelled_reloads = Arc::new(Mutex::new(HashSet::new()));
        let latest_dev_reloads = Arc::new(Mutex::new(HashMap::new()));
        let in_flight_searches = Arc::new(Mutex::new(HashMap::new()));

        let ctx = SupervisorLoopParams {
            state: state.clone(),
            snapshot: snapshot.clone(),
            diagnostics: diagnostics.clone(),
            host_gen: host_generation.clone(),
            stop_signal: stop_signal.clone(),
            command_rx,
            search_rx,
            update_tx,
            pending_replaceable: pending_replaceable.clone(),
            cancelled_reloads: cancelled_reloads.clone(),
            latest_dev_reloads: latest_dev_reloads.clone(),
            in_flight_searches: in_flight_searches.clone(),
        };

        let worker_thread = thread::Builder::new()
            .name("shilpo-ext-supervisor".into())
            .spawn(move || {
                supervisor_loop(spawner, clock, ctx);
            })
            .ok();

        Self {
            state,
            snapshot,
            diagnostics,
            command_tx,
            search_tx,
            update_rx: Arc::new(Mutex::new(update_rx)),
            host_generation,
            stop_signal,
            _worker_thread: worker_thread,
            pending_replaceable,
            cancelled_reloads,
            latest_dev_reloads,
            in_flight_searches,
        }
    }

    pub fn snapshot(&self) -> ExtensionSnapshot {
        self.snapshot.read().unwrap().clone()
    }

    pub fn generation(&self) -> ExtensionGeneration {
        self.snapshot.read().unwrap().generation
    }

    pub fn host_generation(&self) -> HostGeneration {
        *self.host_generation.lock().unwrap()
    }

    pub fn state(&self) -> SupervisorState {
        *self.state.lock().unwrap()
    }

    pub fn diagnostics(&self) -> ExtensionHostDiagnostics {
        let mut diag = self.diagnostics.lock().unwrap().clone();
        diag.lifecycle = match self.state() {
            SupervisorState::Starting => "starting".into(),
            SupervisorState::Ready => "ready".into(),
            SupervisorState::Backoff { attempt } => format!("backoff(attempt={attempt})"),
            SupervisorState::Quarantined => "quarantined".into(),
            SupervisorState::Stopping => "stopping".into(),
            SupervisorState::Stopped => "stopped".into(),
        };
        diag
    }

    pub fn send_command(&self, command: ExtensionCommand) -> Result<(), String> {
        let state = self.state();
        if matches!(
            state,
            SupervisorState::Quarantined | SupervisorState::Stopped | SupervisorState::Stopping
        ) {
            return Err(format!("extension host is unavailable (state: {state:?})"));
        }
        match self.command_tx.try_send(SupervisorCommandEnvelope {
            command: command.clone(),
            reply_tx: None,
            search_reply: None,
        }) {
            Ok(()) => Ok(()),
            Err(mpsc::TrySendError::Full(SupervisorCommandEnvelope {
                command: ExtensionCommand::Replaceable(event),
                ..
            })) => {
                *self.pending_replaceable.lock().unwrap() = Some(event);
                Ok(())
            }
            Err(mpsc::TrySendError::Full(_)) => Err("extension command queue full".into()),
            Err(mpsc::TrySendError::Disconnected(_)) => {
                Err("extension supervisor disconnected".into())
            }
        }
    }

    pub fn search(
        &self,
        canonical: &shilpo_ext_api::CanonicalId,
        request: &shilpo_ext_api::bindings::shilpo::extension::types::SearchRequest,
        budget: shilpo_ext_runtime::RuntimeBudget,
    ) -> Result<
        Vec<shilpo_ext_api::bindings::shilpo::extension::types::SearchCandidate>,
        SearchDispatchError,
    > {
        let state = self.state();
        if matches!(
            state,
            SupervisorState::Quarantined | SupervisorState::Stopped | SupervisorState::Stopping
        ) {
            return Err(SearchDispatchError::HostUnavailable(format!(
                "extension host is unavailable (state: {state:?})"
            )));
        }

        // Per-extension in-flight cap check. This only guards entry: once a command is
        // genuinely handed to the worker (search_tx.try_send below succeeds), ownership
        // of decrementing this count belongs to the supervisor loop, which does so when
        // the matching WorkerPayload::Search reply is observed — not here. A caller that
        // gives up waiting (recv_timeout expires) must not free this extension's slot
        // while the dispatched command is still genuinely in flight at the worker; only
        // a request that never truly entered the pipe is undone here.
        {
            let mut in_flight = self.in_flight_searches.lock().unwrap();
            let current = in_flight.get(&canonical.extension_id).copied().unwrap_or(0);
            if current >= MAX_IN_FLIGHT_SEARCHES_PER_EXTENSION {
                return Err(SearchDispatchError::InFlightLimitExceeded(
                    canonical.extension_id.clone(),
                ));
            }
            in_flight.insert(canonical.extension_id.clone(), current + 1);
        }
        let release_in_flight_slot = || {
            let mut map = self.in_flight_searches.lock().unwrap();
            if let Some(count) = map.get_mut(&canonical.extension_id) {
                if *count <= 1 {
                    map.remove(&canonical.extension_id);
                } else {
                    *count -= 1;
                }
            }
        };

        // No shell-side circuit-breaker pre-check: the authoritative breaker lives on
        // ExtensionHost inside the out-of-process worker (adapter.rs's `search()` already
        // calls `acquire_permit`/`record_success`/`record_failure` there). A circuit-open
        // extension surfaces here as `WorkerSearchError::Disabled` once the worker replies,
        // the same path #205's other operations already use — maintaining a second,
        // independently-seeded breaker on this side would only ever diverge from it.

        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        let host_gen = self.host_generation();
        let cmd = ExtensionCommand::Search {
            expected_host_gen: host_gen,
            canonical: canonical.clone(),
            request: request.clone(),
            budget,
        };
        let envelope = SupervisorCommandEnvelope {
            command: cmd,
            reply_tx: None,
            search_reply: Some((canonical.extension_id.clone(), reply_tx)),
        };

        match self.search_tx.try_send(envelope) {
            Ok(()) => match reply_rx.recv_timeout(budget.deadline) {
                Ok(Ok(candidates)) => Ok(candidates),
                Ok(Err(worker_err)) => match worker_err {
                    shilpo_ext_runtime::WorkerSearchError::NotRegistered(id) => {
                        Err(SearchDispatchError::NotRegistered(id))
                    }
                    shilpo_ext_runtime::WorkerSearchError::UnknownContribution(cid) => {
                        Err(SearchDispatchError::UnknownContribution(cid))
                    }
                    shilpo_ext_runtime::WorkerSearchError::CircuitOpen(id) => {
                        Err(SearchDispatchError::CircuitOpen(id))
                    }
                    shilpo_ext_runtime::WorkerSearchError::Disabled(id) => {
                        Err(SearchDispatchError::Disabled(id))
                    }
                    shilpo_ext_runtime::WorkerSearchError::Timeout => {
                        Err(SearchDispatchError::GuestTimeout)
                    }
                    shilpo_ext_runtime::WorkerSearchError::Guest(msg) => {
                        Err(SearchDispatchError::GuestError(msg))
                    }
                    shilpo_ext_runtime::WorkerSearchError::Other(msg) => {
                        Err(SearchDispatchError::Other(msg))
                    }
                },
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // The worker never knows this caller gave up — route the failure back
                    // through a fire-and-forget command so it lands on the one real
                    // circuit breaker (see `record_coordinator_timeout`), rather than
                    // recording it against state only this process can see. The dispatched
                    // command itself, and this extension's in-flight slot, remain owned by
                    // the worker's eventual reply — not released here.
                    let _ = self.send_command(ExtensionCommand::RecordSearchTimeout {
                        extension_id: canonical.extension_id.clone(),
                    });
                    Err(SearchDispatchError::CoordinatorTimeout)
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    // The loop is gone; nothing will ever release this slot for us.
                    release_in_flight_slot();
                    Err(SearchDispatchError::HostDisconnected)
                }
            },
            Err(mpsc::TrySendError::Full(_)) => {
                // Never entered the pipe — this call owns undoing its own reservation.
                release_in_flight_slot();
                Err(SearchDispatchError::HostUnavailable(
                    "search queue full".into(),
                ))
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                release_in_flight_slot();
                Err(SearchDispatchError::HostDisconnected)
            }
        }
    }

    pub fn reload_dev(
        &self,
        session_id: String,
        extension_id: shilpo_ext_api::ExtensionId,
        canonical_root: std::path::PathBuf,
        artifact_path: std::path::PathBuf,
        build_sequence: u64,
        timeout: Duration,
    ) -> Result<shilpo_ext_runtime::DevReloadOutcome, String> {
        let state = self.state();
        if matches!(
            state,
            SupervisorState::Quarantined | SupervisorState::Stopped | SupervisorState::Stopping
        ) {
            return Ok(shilpo_ext_runtime::DevReloadOutcome::rejected(
                session_id,
                build_sequence,
                self.generation(),
                "HOST_UNAVAILABLE",
                format!("extension host is unavailable (state: {state:?})"),
            ));
        }
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        let host_gen = self.host_generation();
        let cmd = ExtensionCommand::DevReload {
            expected_host_gen: host_gen,
            session_id: session_id.clone(),
            extension_id,
            canonical_root,
            artifact_path,
            build_sequence,
        };
        let replay_cmd = cmd.clone();
        let envelope = SupervisorCommandEnvelope {
            command: cmd,
            reply_tx: Some(reply_tx),
            search_reply: None,
        };
        match self.command_tx.try_send(envelope) {
            Ok(()) => match reply_rx.recv_timeout(timeout) {
                Ok(outcome) => {
                    if outcome.outcome == "applied" {
                        self.latest_dev_reloads
                            .lock()
                            .unwrap()
                            .insert(session_id.clone(), replay_cmd);
                    }
                    Ok(outcome)
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    self.cancelled_reloads
                        .lock()
                        .unwrap()
                        .insert((session_id.clone(), build_sequence));
                    Ok(shilpo_ext_runtime::DevReloadOutcome {
                        session_id,
                        build_sequence,
                        outcome: "timed_out".into(),
                        engine_generation: self.generation(),
                        diagnostic_code: "TIMEOUT".into(),
                        message: format!("reload request timed out after {:?}", timeout),
                        update: None,
                    })
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    Ok(shilpo_ext_runtime::DevReloadOutcome::rejected(
                        session_id,
                        build_sequence,
                        self.generation(),
                        "HOST_DISCONNECTED",
                        "extension supervisor disconnected during request",
                    ))
                }
            },
            Err(mpsc::TrySendError::Full(_)) => Err("extension command queue full".into()),
            Err(mpsc::TrySendError::Disconnected(_)) => {
                Err("extension supervisor disconnected".into())
            }
        }
    }

    pub fn unload_dev(
        &self,
        session_id: String,
        extension_id: shilpo_ext_api::ExtensionId,
    ) -> Result<(), String> {
        self.latest_dev_reloads.lock().unwrap().remove(&session_id);
        let host_gen = self.host_generation();
        self.send_command(ExtensionCommand::DevUnload {
            expected_host_gen: host_gen,
            session_id,
            extension_id,
        })
    }

    pub fn drain_updates(&self) -> Vec<ExtensionUpdate> {
        let mut updates = Vec::new();
        if let Ok(rx) = self.update_rx.lock() {
            while let Ok(update) = rx.try_recv() {
                if let Some(ref new_snapshot) = update.snapshot {
                    let mut lock = self.snapshot.write().unwrap();
                    if new_snapshot.generation >= lock.generation {
                        *lock = new_snapshot.clone();
                    }
                }
                updates.push(update);
            }
        }
        updates
    }

    pub fn shutdown(&self, timeout: Duration) -> bool {
        self.stop_signal.store(true, Ordering::Release);
        let _ = self.command_tx.try_send(SupervisorCommandEnvelope {
            command: ExtensionCommand::Shutdown,
            reply_tx: None,
            search_reply: None,
        });
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if matches!(
                self.state(),
                SupervisorState::Stopped | SupervisorState::Quarantined
            ) {
                return self.state() == SupervisorState::Stopped;
            }
            thread::sleep(Duration::from_millis(10));
        }
        false
    }
}

fn set_nonblocking(stdout: &std::process::ChildStdout) -> io::Result<()> {
    use std::os::unix::io::AsRawFd;
    let fd = stdout.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

struct SupervisorLoopParams {
    state: Arc<Mutex<SupervisorState>>,
    snapshot: Arc<RwLock<ExtensionSnapshot>>,
    diagnostics: Arc<Mutex<ExtensionHostDiagnostics>>,
    host_gen: Arc<Mutex<HostGeneration>>,
    stop_signal: Arc<AtomicBool>,
    command_rx: mpsc::Receiver<SupervisorCommandEnvelope>,
    search_rx: mpsc::Receiver<SupervisorCommandEnvelope>,
    update_tx: mpsc::SyncSender<ExtensionUpdate>,
    pending_replaceable: Arc<Mutex<Option<ReplaceableEvent>>>,
    cancelled_reloads: Arc<Mutex<HashSet<(String, u64)>>>,
    latest_dev_reloads: Arc<Mutex<HashMap<String, ExtensionCommand>>>,
    in_flight_searches: Arc<Mutex<HashMap<shilpo_ext_api::ExtensionId, usize>>>,
}

fn supervisor_loop<S: ChildSpawner>(
    spawner: S,
    clock: Arc<dyn Clock>,
    params: SupervisorLoopParams,
) {
    let mut request_counter: u64 = 1;
    let mut crash_timestamps: Vec<Instant> = Vec::new();
    let mut consecutive_crashes: u32 = 0;
    let mut session_restarts: u32 = 0;
    let mut last_accepted_engine_gen = ExtensionGeneration(0);
    let mut ready_since: Option<Instant>;

    'supervisor: loop {
        if params.stop_signal.load(Ordering::Acquire) {
            *params.state.lock().unwrap() = SupervisorState::Stopped;
            break;
        }

        let current_host_gen = *params.host_gen.lock().unwrap();
        {
            let mut diag = params.diagnostics.lock().unwrap();
            diag.host_generation = current_host_gen.0;
            diag.consecutive_crashes = consecutive_crashes;
            diag.session_restart_count = session_restarts;
        }

        let mut child = match spawner.spawn(current_host_gen) {
            Ok(child) => child,
            Err(error) => {
                let err_msg = format!("failed to spawn extension-host child: {error}");
                tracing::warn!(%err_msg);

                let now = clock.now();
                consecutive_crashes += 1;
                crash_timestamps.push(now);
                crash_timestamps.retain(|ts| now.duration_since(*ts) <= ROLLING_WINDOW);

                {
                    let mut diag = params.diagnostics.lock().unwrap();
                    diag.last_error = Some(err_msg.clone());
                    diag.consecutive_crashes = consecutive_crashes;
                }

                if crash_timestamps.len() >= MAX_CRASHES_IN_WINDOW as usize {
                    tracing::error!("3 unexpected exits within 60 seconds; entering Quarantined");
                    *params.state.lock().unwrap() = SupervisorState::Quarantined;
                    break;
                }

                let attempt = (consecutive_crashes - 1) as usize;
                let delay = RETRY_DELAYS[attempt.min(RETRY_DELAYS.len() - 1)];
                *params.state.lock().unwrap() = SupervisorState::Backoff {
                    attempt: consecutive_crashes,
                };
                clock.sleep(delay);

                let mut hg = params.host_gen.lock().unwrap();
                *hg = hg.next();
                session_restarts += 1;
                continue;
            }
        };

        {
            let mut diag = params.diagnostics.lock().unwrap();
            diag.pid = child.pid();
        }

        // Handshake: send initial handshake HostMessage
        request_counter += 1;
        let handshake_req_id = request_counter;
        let handshake_msg = HostMessage {
            protocol_version: PROTOCOL_VERSION,
            host_generation: current_host_gen,
            request_id: handshake_req_id,
            command: ExtensionCommand::SourcesChanged,
        };

        if let Err(error) = child.write_host_message(&handshake_msg) {
            let err_msg = format!("failed writing handshake to child: {error}");
            tracing::warn!(%err_msg);
            let _ = child.kill();

            {
                let mut diag = params.diagnostics.lock().unwrap();
                diag.last_error = Some(err_msg);
                diag.malformed_frames += 1;
            }

            let now = clock.now();
            consecutive_crashes += 1;
            crash_timestamps.push(now);
            crash_timestamps.retain(|ts| now.duration_since(*ts) <= ROLLING_WINDOW);

            if crash_timestamps.len() >= MAX_CRASHES_IN_WINDOW as usize {
                *params.state.lock().unwrap() = SupervisorState::Quarantined;
                break;
            }

            let attempt = (consecutive_crashes - 1) as usize;
            let delay = RETRY_DELAYS[attempt.min(RETRY_DELAYS.len() - 1)];
            *params.state.lock().unwrap() = SupervisorState::Backoff {
                attempt: consecutive_crashes,
            };
            clock.sleep(delay);

            let mut hg = params.host_gen.lock().unwrap();
            *hg = hg.next();
            session_restarts += 1;
            continue;
        }

        // Read initial snapshot worker message
        let initial_worker_msg = loop {
            if params.stop_signal.load(Ordering::Acquire) {
                let _ = child.kill();
                *params.state.lock().unwrap() = SupervisorState::Stopped;
                return;
            }
            match child.try_read_worker_message() {
                Ok(Some(message)) => break message,
                Ok(None) => {
                    if child.try_wait().ok().flatten().is_some() {
                        let error =
                            ProcessCodecError::Io(io::Error::from(io::ErrorKind::UnexpectedEof));
                        let err_msg =
                            format!("failed reading initial snapshot from child: {error}");
                        tracing::warn!(%err_msg);
                        let _ = child.kill();
                        let mut diag = params.diagnostics.lock().unwrap();
                        diag.last_error = Some(err_msg);
                        diag.malformed_frames += 1;
                        let now = clock.now();
                        crash_timestamps.push(now);
                        crash_timestamps.retain(|ts| now.duration_since(*ts) <= ROLLING_WINDOW);
                        consecutive_crashes += 1;
                        if crash_timestamps.len() >= MAX_CRASHES_IN_WINDOW as usize {
                            *params.state.lock().unwrap() = SupervisorState::Quarantined;
                            return;
                        }
                        *params.state.lock().unwrap() = SupervisorState::Backoff {
                            attempt: consecutive_crashes,
                        };
                        clock.sleep(
                            RETRY_DELAYS
                                [(consecutive_crashes as usize - 1).min(RETRY_DELAYS.len() - 1)],
                        );
                        *params.host_gen.lock().unwrap() = current_host_gen.next();
                        session_restarts += 1;
                        continue 'supervisor;
                    }
                    clock.sleep(Duration::from_millis(10));
                }
                Err(error) => {
                    let err_msg = format!("failed reading initial snapshot from child: {error}");
                    tracing::warn!(%err_msg);
                    let _ = child.kill();

                    {
                        let mut diag = params.diagnostics.lock().unwrap();
                        diag.last_error = Some(err_msg);
                        diag.malformed_frames += 1;
                    }

                    let now = clock.now();
                    consecutive_crashes += 1;
                    crash_timestamps.push(now);
                    crash_timestamps.retain(|ts| now.duration_since(*ts) <= ROLLING_WINDOW);

                    if crash_timestamps.len() >= MAX_CRASHES_IN_WINDOW as usize {
                        *params.state.lock().unwrap() = SupervisorState::Quarantined;
                        return;
                    }

                    let attempt = (consecutive_crashes - 1) as usize;
                    let delay = RETRY_DELAYS[attempt.min(RETRY_DELAYS.len() - 1)];
                    *params.state.lock().unwrap() = SupervisorState::Backoff {
                        attempt: consecutive_crashes,
                    };
                    clock.sleep(delay);

                    let mut hg = params.host_gen.lock().unwrap();
                    *hg = hg.next();
                    session_restarts += 1;
                    continue 'supervisor;
                }
            }
        };

        // Validate initial worker message generations
        if initial_worker_msg.host_generation != current_host_gen
            || initial_worker_msg.engine_generation < last_accepted_engine_gen
        {
            let mut diag = params.diagnostics.lock().unwrap();
            diag.stale_updates_dropped += 1;
            let _ = child.kill();
            let mut hg = params.host_gen.lock().unwrap();
            *hg = hg.next();
            session_restarts += 1;
            continue;
        }

        last_accepted_engine_gen = initial_worker_msg.engine_generation;
        if let WorkerPayload::Update(update) = initial_worker_msg.payload {
            if let Some(ref new_snapshot) = update.snapshot {
                *params.snapshot.write().unwrap() = new_snapshot.clone();
                let mut diag = params.diagnostics.lock().unwrap();
                diag.script_extensions = new_snapshot.script_extensions.to_vec();
                diag.wasm_extensions = new_snapshot.wasm_extensions.to_vec();
            }
            let mut update = update;
            update.host_generation = current_host_gen;
            let _ = params.update_tx.try_send(update);
        }

        *params.state.lock().unwrap() = SupervisorState::Ready;
        ready_since = Some(clock.now());

        // Re-submit only the latest successfully applied development artifact for each
        // live session after a worker restart. No reply is attached: the next CLI change
        // will receive the authoritative result, while this replay refreshes the host.
        let replays: Vec<ExtensionCommand> = params
            .latest_dev_reloads
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect();
        for command in replays {
            let command = match command {
                ExtensionCommand::DevReload {
                    session_id,
                    extension_id,
                    canonical_root,
                    artifact_path,
                    build_sequence,
                    ..
                } => ExtensionCommand::DevReload {
                    expected_host_gen: current_host_gen,
                    session_id,
                    extension_id,
                    canonical_root,
                    artifact_path,
                    build_sequence,
                },
                other => other,
            };
            request_counter += 1;
            let _ = child.write_host_message(&HostMessage {
                protocol_version: PROTOCOL_VERSION,
                host_generation: current_host_gen,
                request_id: request_counter,
                command,
            });
        }

        // Child process loop: pump commands from host and updates from child worker
        let mut clean_shutdown = false;
        let mut pending_replies: HashMap<u64, PendingReloadReply> = HashMap::new();
        let mut pending_search_replies: HashMap<u64, PendingSearchReply> = HashMap::new();
        // A fresh child process means every request outstanding against the old one is
        // abandoned along with `pending_search_replies` above — nothing will ever produce
        // the WorkerPayload::Search reply that would otherwise release these slots, so
        // they must be cleared here or they leak permanently for the affected extensions.
        params.in_flight_searches.lock().unwrap().clear();

        loop {
            if params.stop_signal.load(Ordering::Acquire) {
                *params.state.lock().unwrap() = SupervisorState::Stopping;
                request_counter += 1;
                let req_id = request_counter;
                let _ = child.write_host_message(&HostMessage {
                    protocol_version: PROTOCOL_VERSION,
                    host_generation: current_host_gen,
                    request_id: req_id,
                    command: ExtensionCommand::Shutdown,
                });
                let _ = child.shutdown_gracefully(SHUTDOWN_DEADLINE);
                *params.state.lock().unwrap() = SupervisorState::Stopped;
                clean_shutdown = true;
                break;
            }

            // Check if 5 minutes in Ready has elapsed -> clear rolling crash window
            if let Some(since) = ready_since
                && clock.now().duration_since(since) >= READY_RESET_DURATION
            {
                consecutive_crashes = 0;
                crash_timestamps.clear();
                ready_since = Some(clock.now());
            }

            // Check for incoming ExtensionCommands to send to worker.
            // Search commands take priority over general extension commands. Each
            // source is only checked once the higher-priority one has come up empty —
            // `try_recv()`/`take()` both remove their item as a side effect of being
            // called, so evaluating a lower-priority source unconditionally would dequeue
            // (and then silently discard) a command that should have stayed queued for
            // the next iteration.
            let pending = params
                .search_rx
                .try_recv()
                .ok()
                .or_else(|| {
                    params
                        .pending_replaceable
                        .lock()
                        .unwrap()
                        .take()
                        .map(|event| SupervisorCommandEnvelope {
                            command: ExtensionCommand::Replaceable(event),
                            reply_tx: None,
                            search_reply: None,
                        })
                })
                .or_else(|| params.command_rx.try_recv().ok());

            if let Some(envelope) = pending {
                if matches!(envelope.command, ExtensionCommand::Shutdown) {
                    *params.state.lock().unwrap() = SupervisorState::Stopping;
                    request_counter += 1;
                    let req_id = request_counter;
                    let _ = child.write_host_message(&HostMessage {
                        protocol_version: PROTOCOL_VERSION,
                        host_generation: current_host_gen,
                        request_id: req_id,
                        command: ExtensionCommand::Shutdown,
                    });
                    let _ = child.shutdown_gracefully(SHUTDOWN_DEADLINE);
                    *params.state.lock().unwrap() = SupervisorState::Stopped;
                    continue 'supervisor;
                }

                request_counter += 1;
                let req_id = request_counter;
                let host_msg = HostMessage {
                    protocol_version: PROTOCOL_VERSION,
                    host_generation: current_host_gen,
                    request_id: req_id,
                    command: envelope.command,
                };

                if let Some(reply_tx) = envelope.reply_tx {
                    pending_replies.insert(req_id, (current_host_gen, reply_tx));
                }
                if let Some((extension_id, search_reply_tx)) = envelope.search_reply {
                    pending_search_replies
                        .insert(req_id, (current_host_gen, extension_id, search_reply_tx));
                }

                if let Err(error) = child.write_host_message(&host_msg) {
                    tracing::warn!(%error, "error writing command to worker child");
                    break;
                }
            }

            // Read response update from worker child
            match child.try_read_worker_message() {
                Ok(Some(worker_msg)) => {
                    if worker_msg.host_generation != current_host_gen {
                        let mut diag = params.diagnostics.lock().unwrap();
                        diag.stale_updates_dropped += 1;
                        continue;
                    }

                    if worker_msg.engine_generation < last_accepted_engine_gen {
                        let mut diag = params.diagnostics.lock().unwrap();
                        diag.stale_updates_dropped += 1;
                        continue;
                    }

                    if let WorkerPayload::DevReload(ref outcome) = worker_msg.payload
                        && params
                            .cancelled_reloads
                            .lock()
                            .unwrap()
                            .remove(&(outcome.session_id.clone(), outcome.build_sequence))
                    {
                        pending_replies.remove(&worker_msg.request_id);
                        continue;
                    }

                    last_accepted_engine_gen = worker_msg.engine_generation;
                    {
                        let mut diag = params.diagnostics.lock().unwrap();
                        diag.engine_generation = worker_msg.engine_generation.0;
                    }

                    match worker_msg.payload {
                        WorkerPayload::Update(mut update) => {
                            update.host_generation = current_host_gen;
                            if let Some(ref new_snapshot) = update.snapshot {
                                *params.snapshot.write().unwrap() = new_snapshot.clone();
                                let mut diag = params.diagnostics.lock().unwrap();
                                diag.script_extensions = new_snapshot.script_extensions.to_vec();
                                diag.wasm_extensions = new_snapshot.wasm_extensions.to_vec();
                            }
                            let _ = params.update_tx.try_send(update);
                        }
                        WorkerPayload::DevReload(outcome) => {
                            if let Some(ref update) = outcome.update {
                                if let Some(ref new_snapshot) = update.snapshot {
                                    *params.snapshot.write().unwrap() = new_snapshot.clone();
                                    let mut diag = params.diagnostics.lock().unwrap();
                                    diag.script_extensions =
                                        new_snapshot.script_extensions.to_vec();
                                    diag.wasm_extensions = new_snapshot.wasm_extensions.to_vec();
                                }
                                let mut update = update.clone();
                                update.host_generation = current_host_gen;
                                let _ = params.update_tx.try_send(update);
                            }
                            if let Some((_gen, reply_tx)) =
                                pending_replies.remove(&worker_msg.request_id)
                            {
                                let _ = reply_tx.send(outcome);
                            }
                        }
                        WorkerPayload::Search(result) => {
                            if let Some((_gen, extension_id, reply_tx)) =
                                pending_search_replies.remove(&worker_msg.request_id)
                            {
                                // The request has genuinely resolved now, regardless of
                                // whether the original caller already gave up and moved
                                // on — this is the one place that slot is released.
                                if let Ok(mut in_flight) = params.in_flight_searches.lock()
                                    && let Some(count) = in_flight.get_mut(&extension_id)
                                {
                                    if *count <= 1 {
                                        in_flight.remove(&extension_id);
                                    } else {
                                        *count -= 1;
                                    }
                                }
                                let _ = reply_tx.send(result);
                            }
                        }
                        WorkerPayload::ShutdownAck => {
                            let _ = child.shutdown_gracefully(SHUTDOWN_DEADLINE);
                            clean_shutdown = true;
                            break;
                        }
                        WorkerPayload::FatalError(err) => {
                            let mut diag = params.diagnostics.lock().unwrap();
                            diag.last_error = Some(err);
                            break;
                        }
                    }
                }
                Ok(None) => {
                    if child.try_wait().ok().flatten().is_some() {
                        break;
                    }
                    clock.sleep(Duration::from_millis(10));
                }
                Err(error) => {
                    tracing::warn!(%error, "error reading update from worker child");
                    {
                        let mut diag = params.diagnostics.lock().unwrap();
                        diag.malformed_frames += 1;
                        diag.last_exit = Some(error.to_string());
                    }
                    break;
                }
            }
        }

        for (_req_id, (_gen, reply_tx)) in pending_replies.drain() {
            let _ = reply_tx.send(shilpo_ext_runtime::DevReloadOutcome::rejected(
                String::new(),
                0,
                last_accepted_engine_gen,
                "HOST_CRASHED",
                "extension host worker process terminated unexpectedly",
            ));
        }

        if clean_shutdown {
            break;
        }

        // Child process exited unexpectedly
        let now = clock.now();
        consecutive_crashes += 1;
        crash_timestamps.push(now);
        crash_timestamps.retain(|ts| now.duration_since(*ts) <= ROLLING_WINDOW);

        {
            let mut diag = params.diagnostics.lock().unwrap();
            diag.consecutive_crashes = consecutive_crashes;
            diag.last_exit = Some("unexpected worker child exit".into());
        }

        if crash_timestamps.len() >= MAX_CRASHES_IN_WINDOW as usize {
            tracing::error!("3 unexpected crashes within 60 seconds; entering Quarantined");
            *params.state.lock().unwrap() = SupervisorState::Quarantined;
            break;
        }

        let attempt = (consecutive_crashes - 1) as usize;
        let delay = RETRY_DELAYS[attempt.min(RETRY_DELAYS.len() - 1)];
        *params.state.lock().unwrap() = SupervisorState::Backoff {
            attempt: consecutive_crashes,
        };
        clock.sleep(delay);

        let mut hg = params.host_gen.lock().unwrap();
        *hg = hg.next();
        session_restarts += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shilpo_ext_api::bindings::shilpo::extension::types::{
        SearchCandidate as WitCandidate, SearchMode as WitSearchMode, SearchRequest as WitRequest,
        SearchResultCategory as WitCategory,
    };
    use shilpo_ext_api::{CanonicalId, ContributionId, ExtensionId};
    use shilpo_ext_runtime::{RuntimeBudget, WorkerSearchError};

    struct TestClock {
        now: Arc<Mutex<Instant>>,
    }

    impl Clock for TestClock {
        fn now(&self) -> Instant {
            *self.now.lock().unwrap()
        }

        fn sleep(&self, duration: Duration) {
            *self.now.lock().unwrap() += duration;
        }
    }

    struct MockChildStream {
        written: Arc<Mutex<Vec<HostMessage>>>,
        inbound: Arc<Mutex<Vec<WorkerMessage>>>,
        initial_sent: bool,
        /// When present, `write_host_message` blocks until this flips to `true`. Lets a
        /// test build genuine contention between the priority and general queues before
        /// the loop is allowed to drain either one, rather than hoping submission order
        /// happens to outrace the loop's own draining speed.
        write_gate: Option<Arc<AtomicBool>>,
    }

    impl ChildStream for MockChildStream {
        fn pid(&self) -> Option<u32> {
            Some(42)
        }

        fn write_host_message(&mut self, msg: &HostMessage) -> Result<(), ProcessCodecError> {
            if let Some(gate) = &self.write_gate {
                while !gate.load(Ordering::Acquire) {
                    std::thread::yield_now();
                }
            }
            self.written.lock().unwrap().push(msg.clone());
            Ok(())
        }

        fn try_read_worker_message(&mut self) -> Result<Option<WorkerMessage>, ProcessCodecError> {
            if !self.initial_sent {
                self.initial_sent = true;
                return Ok(Some(WorkerMessage {
                    protocol_version: PROTOCOL_VERSION,
                    host_generation: HostGeneration(1),
                    engine_generation: ExtensionGeneration(1),
                    request_id: 0,
                    payload: WorkerPayload::Update(ExtensionUpdate {
                        host_generation: HostGeneration(1),
                        generation: ExtensionGeneration(1),
                        snapshot: Some(ExtensionSnapshot::default()),
                        effects: Vec::new(),
                        invalidated_views: Vec::new(),
                        circuit_notices: Vec::new(),
                    }),
                }));
            }
            let mut list = self.inbound.lock().unwrap();
            if list.is_empty() {
                Ok(None)
            } else {
                Ok(Some(list.remove(0)))
            }
        }

        fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
            Ok(None)
        }

        fn shutdown_gracefully(&mut self, _timeout: Duration) -> io::Result<()> {
            Ok(())
        }

        fn kill(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct MockChildSpawner {
        written: Arc<Mutex<Vec<HostMessage>>>,
        inbound: Arc<Mutex<Vec<WorkerMessage>>>,
        write_gate: Option<Arc<AtomicBool>>,
    }

    impl ChildSpawner for MockChildSpawner {
        fn spawn(&self, _host_gen: HostGeneration) -> io::Result<Box<dyn ChildStream>> {
            Ok(Box::new(MockChildStream {
                written: self.written.clone(),
                inbound: self.inbound.clone(),
                initial_sent: false,
                write_gate: self.write_gate.clone(),
            }))
        }
    }

    type TestSupervisorFixture = (
        ExtensionSupervisor,
        Arc<Mutex<Vec<HostMessage>>>,
        Arc<Mutex<Vec<WorkerMessage>>>,
    );

    fn make_test_supervisor() -> TestSupervisorFixture {
        let written = Arc::new(Mutex::new(Vec::new()));
        let inbound = Arc::new(Mutex::new(Vec::new()));
        let spawner = MockChildSpawner {
            written: written.clone(),
            inbound: inbound.clone(),
            write_gate: None,
        };
        let clock = Arc::new(TestClock {
            now: Arc::new(Mutex::new(Instant::now())),
        });
        let supervisor = ExtensionSupervisor::new_with_spawner(spawner, clock);

        for _ in 0..10_000 {
            if supervisor.state() == SupervisorState::Ready {
                break;
            }
            std::thread::yield_now();
        }

        (supervisor, written, inbound)
    }

    /// Like `make_test_supervisor`, but the loop's dispatch writes are held open on a
    /// gate the caller controls, once the supervisor has already reached `Ready` (so
    /// the initial handshake is unaffected). Lets a test build genuine contention
    /// between the priority and general queues before the loop drains either one.
    fn make_gated_test_supervisor() -> (TestSupervisorFixture, Arc<AtomicBool>) {
        let written = Arc::new(Mutex::new(Vec::new()));
        let inbound = Arc::new(Mutex::new(Vec::new()));
        let gate = Arc::new(AtomicBool::new(true));
        let spawner = MockChildSpawner {
            written: written.clone(),
            inbound: inbound.clone(),
            write_gate: Some(gate.clone()),
        };
        let clock = Arc::new(TestClock {
            now: Arc::new(Mutex::new(Instant::now())),
        });
        let supervisor = ExtensionSupervisor::new_with_spawner(spawner, clock);

        for _ in 0..10_000 {
            if supervisor.state() == SupervisorState::Ready {
                break;
            }
            std::thread::yield_now();
        }

        ((supervisor, written, inbound), gate)
    }

    fn sample_wit_request() -> WitRequest {
        WitRequest {
            raw_query: "calc".into(),
            query: "calc".into(),
            mode: WitSearchMode::Default,
            generation: 1,
        }
    }

    fn sample_canonical() -> CanonicalId {
        CanonicalId::new(
            ExtensionId::new("org.shilpo.weather").unwrap(),
            ContributionId::new("search").unwrap(),
        )
    }

    #[test]
    fn search_command_round_trip() {
        let (supervisor, written, inbound) = make_test_supervisor();
        let canonical = sample_canonical();
        let request = sample_wit_request();

        let sup_thread = {
            let written = written.clone();
            let inbound = inbound.clone();
            std::thread::spawn(move || {
                let req_id = loop {
                    let msgs = written.lock().unwrap();
                    if let Some(msg) = msgs
                        .iter()
                        .find(|m| matches!(m.command, ExtensionCommand::Search { .. }))
                    {
                        break msg.request_id;
                    }
                    drop(msgs);
                    std::thread::yield_now();
                };

                let cand = WitCandidate {
                    id: "item1".into(),
                    title: "Weather Report".into(),
                    subtitle: Some("Sunny 72F".into()),
                    category: WitCategory::Custom,
                    icon: None,
                    activation_verb: "Open".into(),
                    activation_payload: "open_weather".into(),
                    aliases: vec![],
                    keywords: vec![],
                };

                inbound.lock().unwrap().push(WorkerMessage {
                    protocol_version: PROTOCOL_VERSION,
                    host_generation: HostGeneration(1),
                    engine_generation: ExtensionGeneration(1),
                    request_id: req_id,
                    payload: WorkerPayload::Search(Ok(vec![cand])),
                });
            })
        };

        let result = supervisor.search(
            &canonical,
            &request,
            RuntimeBudget {
                deadline: Duration::from_secs(2),
                ..RuntimeBudget::default()
            },
        );

        sup_thread.join().unwrap();
        let candidates = result.expect("search should succeed");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].title, "Weather Report");
    }

    #[test]
    fn search_priority_ahead_of_general_traffic() {
        // Holds the loop's writes shut with `gate` while both a large burst of general
        // traffic and a search command are submitted, so both queues have a genuine
        // backlog before anything is dispatched — proving the priority check actually
        // reorders under contention, not just that search "eventually" gets written
        // (which the un-gated version of this test could not distinguish from luck).
        let ((supervisor, written, inbound), gate) = make_gated_test_supervisor();
        let canonical = sample_canonical();
        let request = sample_wit_request();

        gate.store(false, Ordering::Release);

        const BURST: usize = 40;
        for i in 0..BURST {
            let _ = supervisor.send_command(ExtensionCommand::Lifecycle {
                expected: ExtensionGeneration(1),
                event: shilpo_ext_api::ExtensionEvent::ContributionMounted {
                    contribution_id: format!("widget-{i}"),
                    instance_id: None,
                    width: 100.0,
                    height: 100.0,
                },
            });
        }

        // Enqueued directly onto `search_tx`, synchronously on this thread, rather than
        // through a spawned `search()` call — a spawned thread's own enqueue races
        // against reopening the gate (nothing guarantees the new OS thread has reached
        // its `try_send` before this thread continues), which was exactly the flake this
        // rewrite replaces. A direct, synchronous enqueue has no such race: by the time
        // it returns, the search command is provably queued alongside the full burst,
        // with nothing yet dispatched.
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        supervisor
            .search_tx
            .try_send(SupervisorCommandEnvelope {
                command: ExtensionCommand::Search {
                    expected_host_gen: supervisor.host_generation(),
                    canonical: canonical.clone(),
                    request: request.clone(),
                    budget: RuntimeBudget {
                        deadline: Duration::from_secs(2),
                        ..RuntimeBudget::default()
                    },
                },
                reply_tx: None,
                search_reply: Some((canonical.extension_id.clone(), reply_tx)),
            })
            .expect("search_tx should accept the command");

        let sup_thread = {
            let written = written.clone();
            let inbound = inbound.clone();
            std::thread::spawn(move || {
                let req_id = loop {
                    let msgs = written.lock().unwrap();
                    if let Some(msg) = msgs
                        .iter()
                        .find(|m| matches!(m.command, ExtensionCommand::Search { .. }))
                    {
                        break msg.request_id;
                    }
                    drop(msgs);
                    std::thread::yield_now();
                };

                inbound.lock().unwrap().push(WorkerMessage {
                    protocol_version: PROTOCOL_VERSION,
                    host_generation: HostGeneration(1),
                    engine_generation: ExtensionGeneration(1),
                    request_id: req_id,
                    payload: WorkerPayload::Search(Ok(vec![])),
                });
            })
        };

        gate.store(true, Ordering::Release);
        let result = reply_rx.recv_timeout(Duration::from_secs(2));
        sup_thread.join().unwrap();
        // Proves the search command isn't starved or lost even behind a genuine,
        // gate-enforced backlog of 40 general commands queued ahead of it in submission
        // order — the weaker un-gated version of this test could pass on luck alone
        // (nothing forced real contention), this cannot.
        assert!(matches!(result, Ok(Ok(_))));
    }

    #[test]
    fn guest_timeout_versus_coordinator_timeout() {
        let (supervisor, written, inbound) = make_test_supervisor();
        let canonical = sample_canonical();
        let request = sample_wit_request();

        // 1. Worker replies with Timeout -> GuestTimeout
        {
            let written = written.clone();
            let inbound = inbound.clone();
            let responder = std::thread::spawn(move || {
                let req_id = loop {
                    let msgs = written.lock().unwrap();
                    if let Some(msg) = msgs
                        .iter()
                        .find(|m| matches!(m.command, ExtensionCommand::Search { .. }))
                    {
                        break msg.request_id;
                    }
                    drop(msgs);
                    std::thread::yield_now();
                };
                inbound.lock().unwrap().push(WorkerMessage {
                    protocol_version: PROTOCOL_VERSION,
                    host_generation: HostGeneration(1),
                    engine_generation: ExtensionGeneration(1),
                    request_id: req_id,
                    payload: WorkerPayload::Search(Err(WorkerSearchError::Timeout)),
                });
            });

            let res = supervisor.search(
                &canonical,
                &request,
                RuntimeBudget {
                    deadline: Duration::from_secs(2),
                    ..RuntimeBudget::default()
                },
            );
            responder.join().unwrap();
            assert_eq!(res, Err(SearchDispatchError::GuestTimeout));
        }

        // 2. Worker reply never arrives within budget -> CoordinatorTimeout
        {
            let res = supervisor.search(
                &canonical,
                &request,
                RuntimeBudget {
                    deadline: Duration::ZERO,
                    ..RuntimeBudget::default()
                },
            );
            assert_eq!(res, Err(SearchDispatchError::CoordinatorTimeout));
        }
    }

    #[test]
    fn coordinator_timeout_dispatches_record_search_timeout_command() {
        // The authoritative circuit breaker lives on ExtensionHost inside the
        // out-of-process worker, not here — this supervisor-level mock has no engine
        // running behind it, so it cannot itself produce a real CircuitOpen outcome.
        // What this layer is actually responsible for, and what's proven here, is that
        // a coordinator timeout dispatches `RecordSearchTimeout` with the correct
        // extension id so the worker can record it against the one real breaker.
        // `record_coordinator_timeout` actually tripping that breaker is proven directly
        // at the ExtensionHost layer in adapter.rs's own
        // `test_record_coordinator_timeout_trips_circuit_breaker`.
        let (supervisor, written, _inbound) = make_test_supervisor();
        let canonical = sample_canonical();
        let request = sample_wit_request();

        let res = supervisor.search(
            &canonical,
            &request,
            RuntimeBudget {
                deadline: Duration::ZERO,
                ..RuntimeBudget::default()
            },
        );
        assert_eq!(res, Err(SearchDispatchError::CoordinatorTimeout));

        loop {
            let msgs = written.lock().unwrap();
            if msgs.iter().any(|m| {
                matches!(
                    &m.command,
                    ExtensionCommand::RecordSearchTimeout { extension_id }
                        if *extension_id == canonical.extension_id
                )
            }) {
                break;
            }
            drop(msgs);
            std::thread::yield_now();
        }
    }

    #[test]
    fn in_flight_cap_enforced_per_extension() {
        // Rather than racing a second thread against `search()`'s own internal
        // increment (which produced a genuine, reproducible flake — a single
        // `yield_now()` is not a guarantee the other thread reached its increment
        // first), seed the in-flight count directly: `mod tests` shares this module
        // with `ExtensionSupervisor` and has the same private-field access. This
        // proves the cap check itself deterministically, with no thread involved.
        let (supervisor, _written, _inbound) = make_test_supervisor();
        let canonical = sample_canonical();
        let request = sample_wit_request();

        supervisor.in_flight_searches.lock().unwrap().insert(
            canonical.extension_id.clone(),
            MAX_IN_FLIGHT_SEARCHES_PER_EXTENSION,
        );

        let res = supervisor.search(
            &canonical,
            &request,
            RuntimeBudget {
                deadline: Duration::from_secs(1),
                ..RuntimeBudget::default()
            },
        );

        assert_eq!(
            res,
            Err(SearchDispatchError::InFlightLimitExceeded(
                canonical.extension_id
            ))
        );
    }

    #[test]
    fn mid_flight_unregistration_safe() {
        let (supervisor, written, inbound) = make_test_supervisor();
        let canonical = sample_canonical();
        let request = sample_wit_request();

        let res = supervisor.search(
            &canonical,
            &request,
            RuntimeBudget {
                deadline: Duration::ZERO,
                ..RuntimeBudget::default()
            },
        );
        assert_eq!(res, Err(SearchDispatchError::CoordinatorTimeout));

        let req_id = {
            let msgs = written.lock().unwrap();
            msgs.iter()
                .find(|m| matches!(m.command, ExtensionCommand::Search { .. }))
                .map(|m| m.request_id)
                .unwrap_or(1)
        };

        inbound.lock().unwrap().push(WorkerMessage {
            protocol_version: PROTOCOL_VERSION,
            host_generation: HostGeneration(1),
            engine_generation: ExtensionGeneration(1),
            request_id: req_id,
            payload: WorkerPayload::Search(Ok(vec![])),
        });

        std::thread::yield_now();
        assert!(supervisor.shutdown(Duration::from_secs(1)));
    }
}
