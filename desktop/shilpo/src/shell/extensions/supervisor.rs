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

pub struct SupervisorCommandEnvelope {
    pub command: ExtensionCommand,
    pub reply_tx: Option<mpsc::SyncSender<shilpo_ext_runtime::DevReloadOutcome>>,
}

pub struct ExtensionSupervisor {
    state: Arc<Mutex<SupervisorState>>,
    snapshot: Arc<RwLock<ExtensionSnapshot>>,
    diagnostics: Arc<Mutex<ExtensionHostDiagnostics>>,
    command_tx: mpsc::SyncSender<SupervisorCommandEnvelope>,
    update_rx: Arc<Mutex<mpsc::Receiver<ExtensionUpdate>>>,
    host_generation: Arc<Mutex<HostGeneration>>,
    stop_signal: Arc<AtomicBool>,
    _worker_thread: Option<JoinHandle<()>>,
    pending_replaceable: Arc<Mutex<Option<ReplaceableEvent>>>,
    cancelled_reloads: Arc<Mutex<HashSet<(String, u64)>>>,
    latest_dev_reloads: Arc<Mutex<HashMap<String, ExtensionCommand>>>,
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
        let (update_tx, update_rx) = mpsc::sync_channel(MAX_QUEUE_SIZE);
        let pending_replaceable = Arc::new(Mutex::new(None));
        let cancelled_reloads = Arc::new(Mutex::new(HashSet::new()));
        let latest_dev_reloads = Arc::new(Mutex::new(HashMap::new()));

        let ctx = SupervisorLoopParams {
            state: state.clone(),
            snapshot: snapshot.clone(),
            diagnostics: diagnostics.clone(),
            host_gen: host_generation.clone(),
            stop_signal: stop_signal.clone(),
            command_rx,
            update_tx,
            pending_replaceable: pending_replaceable.clone(),
            cancelled_reloads: cancelled_reloads.clone(),
            latest_dev_reloads: latest_dev_reloads.clone(),
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
            update_rx: Arc::new(Mutex::new(update_rx)),
            host_generation,
            stop_signal,
            _worker_thread: worker_thread,
            pending_replaceable,
            cancelled_reloads,
            latest_dev_reloads,
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
    update_tx: mpsc::SyncSender<ExtensionUpdate>,
    pending_replaceable: Arc<Mutex<Option<ReplaceableEvent>>>,
    cancelled_reloads: Arc<Mutex<HashSet<(String, u64)>>>,
    latest_dev_reloads: Arc<Mutex<HashMap<String, ExtensionCommand>>>,
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
        let mut pending_replies: HashMap<
            u64,
            (
                HostGeneration,
                mpsc::SyncSender<shilpo_ext_runtime::DevReloadOutcome>,
            ),
        > = HashMap::new();

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

            // Check for incoming ExtensionCommands to send to worker
            let pending = params
                .pending_replaceable
                .lock()
                .unwrap()
                .take()
                .map(|event| SupervisorCommandEnvelope {
                    command: ExtensionCommand::Replaceable(event),
                    reply_tx: None,
                });
            if let Some(envelope) = pending.or_else(|| params.command_rx.try_recv().ok()) {
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
