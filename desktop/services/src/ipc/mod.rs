//! Authenticated, bounded Unix-socket IPC for the shell.
//!
//! libc is used only for effective-uid lookup, Linux peer credentials, and
//! advisory locking. All calls below check their return values; raw file
//! descriptors are borrowed from Rust-owned objects and never closed here.

use crate::compositor::{
    CommandOutcome, CompositorCommand, CompositorCommandBroker, CompositorCommandError,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    env, fmt,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::{
        fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        io::AsRawFd,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::Duration,
};

const APP_DIR: &str = "shilpo-shell";
const SOCKET: &str = "ipc.sock";
const LOCK: &str = "instance.lock";
const MAX_FRAME: usize = 16 * 1024;
const MAX_QUEUE: usize = 128;
const IO_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(test)]
const CLIENT_WORKERS: usize = 1;
#[cfg(not(test))]
const CLIENT_WORKERS: usize = 2;
const CLIENT_QUEUE: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "payload")]
pub enum IpcRequest {
    Compositor(CompositorCommand),
    ReloadConfig,
    ShowBar,
    HideBar,
    ToggleBar,
    ShowControlCenter,
    HideControlCenter,
    ToggleControlCenter,
    ShowOverview,
    HideOverview,
    ToggleOverview,
    SetBrightness(u8),
    SetDisplayBrightness { id: String, percentage: u8 },
    GetStatus,
    GetTelemetry,
    Capture(shilpo_capture::CaptureIntent),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BarState {
    #[default]
    Starting,
    Visible,
    Hidden,
    OpenFailed,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessState {
    #[default]
    Starting,
    Ready,
    Degraded,
    Failed,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "state", content = "attempt")]
pub enum ServiceLifecycle {
    #[default]
    Unavailable,
    Connecting {
        attempt: u32,
    },
    Ready,
}

impl ServiceLifecycle {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ServiceHealth {
    pub compositor_connected: bool,
    #[serde(default)]
    pub compositor_state: String,
    #[serde(default)]
    pub compositor_revision: u64,
    #[serde(default)]
    pub compositor_reconnect_attempt: u32,
    #[serde(default)]
    pub compositor_last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compositor_telemetry: Option<crate::compositor::CompositorBrokerTelemetry>,
    pub battery_service_available: bool,
    #[serde(default)]
    pub battery_state: ServiceLifecycle,
    #[serde(default)]
    pub battery_last_error: Option<String>,
    pub audio_service_available: bool,
    #[serde(default)]
    pub audio_state: ServiceLifecycle,
    #[serde(default)]
    pub audio_last_error: Option<String>,
    pub network_service_available: bool,
    #[serde(default)]
    pub network_state: ServiceLifecycle,
    #[serde(default)]
    pub network_last_error: Option<String>,
    pub notification_service_available: bool,
    #[serde(default)]
    pub notification_state: ServiceLifecycle,
    #[serde(default)]
    pub notification_last_error: Option<String>,
    #[serde(default)]
    pub media_service_available: bool,
    #[serde(default)]
    pub media_state: ServiceLifecycle,
    #[serde(default)]
    pub media_last_error: Option<String>,
    #[serde(default)]
    pub brightness_service_available: bool,
    #[serde(default)]
    pub brightness_state: ServiceLifecycle,
    #[serde(default)]
    pub brightness_last_error: Option<String>,
    pub heed_store_available: bool,
    pub uptime_seconds: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct IpcStatus {
    pub running: bool,
    #[serde(default)]
    pub instance_id: String,
    #[serde(default)]
    pub pid: u32,
    pub readiness: ReadinessState,
    pub bar: BarState,
    pub overview_visible: bool,
    pub control_center_visible: bool,
    #[serde(default)]
    pub health: ServiceHealth,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum IpcResult {
    Accepted,
    CommandApplied(CommandOutcome),
    Status(IpcStatus),
    Telemetry(ServiceHealth),
    Capture(shilpo_capture::CaptureOutcome),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcErrorBody {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcResponse {
    pub protocol: String,
    pub version: u16,
    pub request_id: u64,
    pub ok: bool,
    pub result: Option<IpcResult>,
    pub error: Option<IpcErrorBody>,
    #[serde(skip)]
    pub success: bool,
    #[serde(skip)]
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Envelope {
    protocol: String,
    version: u16,
    request_id: u64,
    request: IpcRequest,
}

#[derive(Debug)]
pub enum IpcError {
    AlreadyRunning,
    InvalidPath(String),
    Code { code: String, message: String },
    Io(io::Error),
    Json(serde_json::Error),
}
impl fmt::Display for IpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyRunning => write!(f, "already running"),
            Self::InvalidPath(s) => write!(f, "invalid IPC path: {s}"),
            Self::Code { code, message } => write!(f, "{code}: {message}"),
            Self::Io(e) => e.fmt(f),
            Self::Json(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for IpcError {}
impl From<io::Error> for IpcError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<serde_json::Error> for IpcError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

pub fn get_socket_path() -> Result<PathBuf, IpcError> {
    paths_from_env().map(|p| p.1)
}

fn paths_from_env() -> Result<(PathBuf, PathBuf), IpcError> {
    if unsafe { libc::getuid() } != unsafe { libc::geteuid() } {
        return Err(IpcError::InvalidPath(
            "setuid execution is not supported".into(),
        ));
    }
    let runtime = env::var_os("XDG_RUNTIME_DIR")
        .ok_or_else(|| IpcError::InvalidPath("XDG_RUNTIME_DIR missing".into()))?;
    let runtime = PathBuf::from(runtime);
    validate_dir(&runtime)?;
    let app = runtime.join(APP_DIR);
    prepare_app_dir(&runtime, &app)?;
    Ok((app.clone(), app.join(SOCKET)))
}

fn validate_dir(path: &Path) -> Result<(), IpcError> {
    if !path.is_absolute() || path.as_os_str().is_empty() {
        return Err(IpcError::InvalidPath(path.display().to_string()));
    }
    let m = fs::symlink_metadata(path).map_err(IpcError::Io)?;
    if !m.is_dir() || m.file_type().is_symlink() {
        return Err(IpcError::InvalidPath(format!(
            "{} is not a directory",
            path.display()
        )));
    }
    if m.uid() != unsafe { libc::geteuid() } as u32 || m.mode() & 0o077 != 0 {
        return Err(IpcError::InvalidPath(format!(
            "{} is not private",
            path.display()
        )));
    }
    Ok(())
}
fn prepare_app_dir(runtime: &Path, app: &Path) -> Result<(), IpcError> {
    if !app.exists() {
        fs::create_dir(app)?;
        fs::set_permissions(app, fs::Permissions::from_mode(0o700))?;
    }
    let _ = runtime;
    validate_dir(app)
}

fn lock_instance(app: &Path) -> Result<File, IpcError> {
    let lock_path = app.join(LOCK);
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .mode(0o600)
        .open(&lock_path)?;
    let m = fs::symlink_metadata(&lock_path)?;
    if !m.is_file()
        || m.file_type().is_symlink()
        || m.uid() != unsafe { libc::geteuid() } as u32
        || m.mode() & 0o077 != 0
    {
        return Err(IpcError::InvalidPath("unsafe instance.lock".into()));
    }
    let rc = unsafe {
        libc::flock(
            std::os::unix::io::AsRawFd::as_raw_fd(&file),
            libc::LOCK_EX | libc::LOCK_NB,
        )
    };
    if rc != 0 {
        return Err(
            if io::Error::last_os_error().raw_os_error() == Some(libc::EWOULDBLOCK) {
                IpcError::AlreadyRunning
            } else {
                IpcError::Io(io::Error::last_os_error())
            },
        );
    }
    Ok(file)
}

fn valid_socket(path: &Path) -> Result<Option<(u64, u64)>, IpcError> {
    match fs::symlink_metadata(path) {
        Ok(m) => {
            if m.file_type().is_symlink()
                || !m.file_type().is_socket()
                || m.uid() != unsafe { libc::geteuid() } as u32
                || m.mode() & 0o077 != 0
            {
                return Err(IpcError::InvalidPath("unsafe existing IPC socket".into()));
            }
            Ok(Some((m.dev(), m.ino())))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn peer_ok(stream: &UnixStream) -> io::Result<bool> {
    #[cfg(target_os = "linux")]
    {
        let mut cred = libc::ucred {
            pid: 0,
            uid: 0,
            gid: 0,
        };
        let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        let rc = unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                &mut cred as *mut libc::ucred as *mut libc::c_void,
                &mut len,
            )
        };
        Ok(rc == 0 && cred.uid == unsafe { libc::geteuid() } as u32)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = stream;
        Ok(false)
    }
}

fn write_frame<W: Write>(w: &mut W, bytes: &[u8]) -> io::Result<()> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "frame limit"));
    }
    w.write_all(&(bytes.len() as u32).to_be_bytes())?;
    w.write_all(bytes)?;
    w.flush()
}
fn read_frame<R: Read>(r: &mut R) -> Result<Vec<u8>, IpcError> {
    let mut h = [0; 4];
    r.read_exact(&mut h).map_err(|e| {
        if e.kind() == io::ErrorKind::TimedOut || e.kind() == io::ErrorKind::WouldBlock {
            return IpcError::Code {
                code: "timeout".into(),
                message: "frame read deadline exceeded".into(),
            };
        }
        if e.kind() == io::ErrorKind::UnexpectedEof {
            IpcError::Code {
                code: "bad_request".into(),
                message: "truncated frame".into(),
            }
        } else {
            e.into()
        }
    })?;
    let n = u32::from_be_bytes(h) as usize;
    if n == 0 || n > MAX_FRAME {
        return Err(IpcError::Code {
            code: "frame_too_large".into(),
            message: "invalid frame length".into(),
        });
    }
    let mut b = vec![0; n];
    r.read_exact(&mut b).map_err(|e| IpcError::Code {
        code: if e.kind() == io::ErrorKind::TimedOut || e.kind() == io::ErrorKind::WouldBlock {
            "timeout"
        } else {
            "bad_request"
        }
        .into(),
        message: e.to_string(),
    })?;
    Ok(b)
}

pub struct ShellIpcServer {
    socket_path: PathBuf,
    socket_identity: (u64, u64),
    pending: Arc<Mutex<VecDeque<IpcRequest>>>,
    status: Arc<Mutex<IpcStatus>>,
    broker: Arc<Mutex<Option<Arc<CompositorCommandBroker>>>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    thread: Option<JoinHandle<()>>,
    client_workers: Vec<JoinHandle<()>>,
    _lock: File,
}

impl ShellIpcServer {
    pub fn new() -> Result<Self, IpcError> {
        let (app, path) = paths_from_env()?;
        Self::new_at(&app, &path)
    }

    pub fn new_at(runtime_or_app: &Path, socket_path: &Path) -> Result<Self, IpcError> {
        if unsafe { libc::getuid() } != unsafe { libc::geteuid() } {
            return Err(IpcError::InvalidPath(
                "setuid execution is not supported".into(),
            ));
        }
        let app = if runtime_or_app.file_name().and_then(|n| n.to_str()) == Some(APP_DIR) {
            runtime_or_app.to_path_buf()
        } else {
            let app = runtime_or_app.join(APP_DIR);
            prepare_app_dir(runtime_or_app, &app)?;
            app
        };
        validate_dir(&app)?;
        if socket_path != app.join(SOCKET) {
            return Err(IpcError::InvalidPath(
                "socket must be app-dir/ipc.sock".into(),
            ));
        }
        let lock = lock_instance(&app)?;
        if let Some(identity) = valid_socket(socket_path)? {
            let stale = match UnixStream::connect(socket_path) {
                Ok(_) => false,
                Err(e) => e.kind() == io::ErrorKind::ConnectionRefused,
            };
            if !stale {
                return Err(IpcError::AlreadyRunning);
            }
            let m = fs::symlink_metadata(socket_path)?;
            if (m.dev(), m.ino()) != identity {
                return Err(IpcError::InvalidPath(
                    "socket changed during stale check".into(),
                ));
            }
            fs::remove_file(socket_path)?;
        }
        let listener = UnixListener::bind(socket_path)?;
        fs::set_permissions(socket_path, fs::Permissions::from_mode(0o600))?;
        let m = fs::symlink_metadata(socket_path)?;
        if !m.file_type().is_socket()
            || m.uid() != unsafe { libc::geteuid() } as u32
            || m.mode() & 0o077 != 0
        {
            return Err(IpcError::InvalidPath("bound socket is unsafe".into()));
        }
        let identity = (m.dev(), m.ino());
        listener.set_nonblocking(true)?;
        let pending = Arc::new(Mutex::new(VecDeque::new()));
        let instance_id = uuid::Uuid::new_v4().to_string();
        let pid = std::process::id();
        let status = Arc::new(Mutex::new(IpcStatus {
            running: true,
            instance_id,
            pid,
            ..IpcStatus::default()
        }));
        let broker = Arc::new(Mutex::new(None));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let (client_tx, client_rx) = mpsc::sync_channel(CLIENT_QUEUE);
        let client_rx = Arc::new(Mutex::new(client_rx));
        let mut client_workers = Vec::with_capacity(CLIENT_WORKERS);
        for index in 0..CLIENT_WORKERS {
            let rx = client_rx.clone();
            let p_c = pending.clone();
            let s_c = status.clone();
            let b_c = broker.clone();
            let quit = stop.clone();
            client_workers.push(
                thread::Builder::new()
                    .name(format!("shilpo-ipc-client-{index}"))
                    .spawn(move || {
                        loop {
                            if quit.load(std::sync::atomic::Ordering::Acquire) {
                                break;
                            }
                            let stream =
                                match rx.lock().unwrap().recv_timeout(Duration::from_millis(50)) {
                                    Ok(stream) => stream,
                                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                                };
                            let _ = handle_client(stream, &p_c, &s_c, &b_c);
                        }
                    })
                    .map_err(IpcError::Io)?,
            );
        }

        let quit = stop.clone();

        let thread = thread::Builder::new()
            .name("shilpo-ipc".into())
            .spawn(move || {
                while !quit.load(std::sync::atomic::Ordering::Acquire) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let mut stream = stream;
                            loop {
                                match client_tx.try_send(stream) {
                                    Ok(()) => break,
                                    Err(mpsc::TrySendError::Full(returned)) => {
                                        if quit.load(std::sync::atomic::Ordering::Acquire) {
                                            break;
                                        }
                                        stream = returned;
                                        thread::sleep(Duration::from_millis(10));
                                    }
                                    Err(mpsc::TrySendError::Disconnected(_)) => break,
                                }
                            }
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10));
                        }
                        Err(_) => break,
                    }
                }
            })
            .map_err(IpcError::Io)?;

        Ok(Self {
            socket_path: socket_path.to_path_buf(),
            socket_identity: identity,
            pending,
            status,
            broker,
            stop,
            thread: Some(thread),
            client_workers,
            _lock: lock,
        })
    }

    pub fn attach_broker(&self, broker: Arc<CompositorCommandBroker>) {
        *self.broker.lock().unwrap() = Some(broker);
    }

    pub fn pop_pending_requests(&self) -> Vec<IpcRequest> {
        self.pending.lock().unwrap().drain(..).collect()
    }

    pub fn update_status(&self, mut status: IpcStatus) {
        let mut current = self.status.lock().unwrap();
        if status.instance_id.is_empty() {
            status.instance_id = current.instance_id.clone();
        }
        if status.pid == 0 {
            status.pid = current.pid;
        }
        *current = status;
    }

    pub fn send_command(req: IpcRequest) -> Result<IpcResponse, IpcError> {
        let path = get_socket_path()?;
        Self::send_command_at(&path, req)
    }

    pub fn send_command_at(path: &Path, req: IpcRequest) -> Result<IpcResponse, IpcError> {
        let mut stream = UnixStream::connect(path)?;
        stream.set_read_timeout(Some(IO_TIMEOUT))?;
        stream.set_write_timeout(Some(IO_TIMEOUT))?;
        if !peer_ok(&stream)? {
            return Err(IpcError::Code {
                code: "unavailable".into(),
                message: "peer uid mismatch".into(),
            });
        }
        let env = Envelope {
            protocol: "shilpo-shell".into(),
            version: 1,
            request_id: 1,
            request: req,
        };
        write_frame(&mut stream, &serde_json::to_vec(&env)?)?;
        let mut response: IpcResponse = serde_json::from_slice(&read_frame(&mut stream)?)?;
        response.success = response.ok;
        response.message = response
            .error
            .as_ref()
            .map(|e| e.message.clone())
            .unwrap_or_else(|| "accepted".into());
        Ok(response)
    }
}

/// Typed IPC client interface for shell control and status queries.
#[derive(Debug, Clone, Default)]
pub struct ShellIpcClient;

impl ShellIpcClient {
    pub fn new() -> Self {
        Self
    }

    pub fn send(&self, request: IpcRequest) -> Result<IpcResponse, IpcError> {
        ShellIpcServer::send_command(request)
    }

    pub fn send_at(&self, path: &Path, request: IpcRequest) -> Result<IpcResponse, IpcError> {
        ShellIpcServer::send_command_at(path, request)
    }

    pub fn status(&self) -> Result<IpcStatus, IpcError> {
        let resp = self.send(IpcRequest::GetStatus)?;
        if !resp.ok {
            let err = resp.error.unwrap_or(IpcErrorBody {
                code: "ipc_error".into(),
                message: "status request failed".into(),
            });
            return Err(IpcError::Code {
                code: err.code,
                message: err.message,
            });
        }
        match resp.result {
            Some(IpcResult::Status(status)) => Ok(status),
            _ => Err(IpcError::Code {
                code: "invalid_response".into(),
                message: "unexpected result from status request".into(),
            }),
        }
    }

    pub fn telemetry(&self) -> Result<ServiceHealth, IpcError> {
        let resp = self.send(IpcRequest::GetTelemetry)?;
        if !resp.ok {
            let err = resp.error.unwrap_or(IpcErrorBody {
                code: "ipc_error".into(),
                message: "telemetry request failed".into(),
            });
            return Err(IpcError::Code {
                code: err.code,
                message: err.message,
            });
        }
        match resp.result {
            Some(IpcResult::Telemetry(health)) => Ok(health),
            _ => Err(IpcError::Code {
                code: "invalid_response".into(),
                message: "unexpected result from telemetry request".into(),
            }),
        }
    }

    pub fn is_socket_available() -> bool {
        if let Ok(path) = get_socket_path() {
            path.exists()
        } else {
            false
        }
    }
}

impl Drop for ShellIpcServer {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
        if let Ok(s) = UnixStream::connect(&self.socket_path) {
            drop(s);
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        for worker in self.client_workers.drain(..) {
            let _ = worker.join();
        }
        if let Ok(m) = fs::symlink_metadata(&self.socket_path)
            && (m.dev(), m.ino()) == self.socket_identity
        {
            let _ = fs::remove_file(&self.socket_path);
        }
    }
}

fn response(id: u64, result: Option<IpcResult>, error: Option<IpcErrorBody>) -> IpcResponse {
    let ok = error.is_none();
    IpcResponse {
        protocol: "shilpo-shell".into(),
        version: 1,
        request_id: id,
        ok,
        result,
        message: if ok {
            "accepted".into()
        } else {
            error.as_ref().unwrap().message.clone()
        },
        success: ok,
        error,
    }
}

fn map_broker_error(err: &CompositorCommandError) -> IpcErrorBody {
    let (code, message) = match err {
        CompositorCommandError::Unavailable { state } => (
            "compositor_unavailable",
            format!("compositor is unavailable (state: {})", state.state_name()),
        ),
        CompositorCommandError::Busy { queue_len } => (
            "compositor_busy",
            format!("compositor queue full (length: {})", queue_len),
        ),
        CompositorCommandError::Unsupported => (
            "compositor_unsupported",
            "compositor command is unsupported".into(),
        ),
        CompositorCommandError::BackendRejected { message } => (
            "compositor_command_failed",
            format!("backend rejected command: {}", message),
        ),
        CompositorCommandError::Transport { message } => (
            "compositor_command_failed",
            format!("transport error: {}", message),
        ),
        CompositorCommandError::Timeout { duration } => (
            "compositor_timeout",
            format!("command timed out after {:?}", duration),
        ),
        CompositorCommandError::Cancelled { reason } => (
            "compositor_cancelled",
            format!("command cancelled: {}", reason),
        ),
        CompositorCommandError::InvalidTarget(target) => {
            ("invalid_target", format!("invalid target: {}", target))
        }
        CompositorCommandError::TargetDisappeared(target) => (
            "target_disappeared",
            format!("target disappeared before application: {}", target),
        ),
        CompositorCommandError::ApplyTimeout {
            duration,
            last_revision,
        } => (
            "apply_timeout",
            format!(
                "command application timed out after {:?} (last revision: {})",
                duration, last_revision
            ),
        ),
    };
    IpcErrorBody {
        code: code.to_string(),
        message,
    }
}

fn handle_client(
    mut stream: UnixStream,
    pending: &Arc<Mutex<VecDeque<IpcRequest>>>,
    status: &Arc<Mutex<IpcStatus>>,
    broker: &Arc<Mutex<Option<Arc<CompositorCommandBroker>>>>,
) -> Result<(), IpcError> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    if !peer_ok(&stream)? {
        return Ok(());
    }
    let bytes = match read_frame(&mut stream) {
        Ok(b) => b,
        Err(e) => {
            let body = IpcErrorBody {
                code: match &e {
                    IpcError::Code { code, .. } => code.clone(),
                    _ => "bad_request".into(),
                },
                message: e.to_string(),
            };
            let _ = write_frame(
                &mut stream,
                &serde_json::to_vec(&response(0, None, Some(body)))?,
            );
            return Ok(());
        }
    };
    let env: Envelope = match serde_json::from_slice(&bytes) {
        Ok(e) => e,
        Err(e) => {
            let _ = write_frame(
                &mut stream,
                &serde_json::to_vec(&response(
                    0,
                    None,
                    Some(IpcErrorBody {
                        code: "bad_request".into(),
                        message: e.to_string(),
                    }),
                ))?,
            );
            return Ok(());
        }
    };
    if env.protocol != "shilpo-shell" {
        let _ = write_frame(
            &mut stream,
            &serde_json::to_vec(&response(
                env.request_id,
                None,
                Some(IpcErrorBody {
                    code: "bad_request".into(),
                    message: "unknown protocol".into(),
                }),
            ))?,
        );
        return Ok(());
    }
    if env.version != 1 {
        let _ = write_frame(
            &mut stream,
            &serde_json::to_vec(&response(
                env.request_id,
                None,
                Some(IpcErrorBody {
                    code: "unsupported_version".into(),
                    message: "protocol version unsupported".into(),
                }),
            ))?,
        );
        return Ok(());
    }

    let is_status = matches!(&env.request, IpcRequest::GetStatus);
    let is_telemetry = matches!(&env.request, IpcRequest::GetTelemetry);
    let is_compositor = matches!(&env.request, IpcRequest::Compositor(_));
    let queue_full =
        !is_status && !is_telemetry && !is_compositor && pending.lock().unwrap().len() >= MAX_QUEUE;

    let mut err_body: Option<IpcErrorBody> = None;

    let result = if is_status {
        Some(IpcResult::Status(status.lock().unwrap().clone()))
    } else if is_telemetry {
        let mut health = status.lock().unwrap().health.clone();
        if let Some(b) = broker.lock().unwrap().as_ref() {
            health.compositor_telemetry = Some(b.telemetry());
        }
        Some(IpcResult::Telemetry(health))
    } else if is_compositor {
        if let IpcRequest::Compositor(ref cmd) = env.request {
            let b = broker.lock().unwrap().clone();
            if let Some(broker_arc) = b {
                match broker_arc.submit(cmd.clone()) {
                    Ok(ticket) => match ticket.wait_timeout(IO_TIMEOUT) {
                        Ok(outcome) => Some(IpcResult::CommandApplied(outcome)),
                        Err(err) => {
                            err_body = Some(map_broker_error(&err));
                            None
                        }
                    },
                    Err(err) => {
                        err_body = Some(map_broker_error(&err));
                        None
                    }
                }
            } else {
                err_body = Some(IpcErrorBody {
                    code: "compositor_unavailable".into(),
                    message: "compositor command broker is not attached".into(),
                });
                None
            }
        } else {
            unreachable!()
        }
    } else if queue_full {
        err_body = Some(IpcErrorBody {
            code: "busy".into(),
            message: "request queue full".into(),
        });
        None
    } else {
        let mut q = pending.lock().unwrap();
        q.push_back(env.request);
        Some(IpcResult::Accepted)
    };

    write_frame(
        &mut stream,
        &serde_json::to_vec(&response(env.request_id, result, err_body))?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn serial_guard() -> std::sync::MutexGuard<'static, ()> {
        crate::test_support::serial_guard()
    }

    fn fixture() -> (PathBuf, PathBuf) {
        let dir = env::temp_dir().join(format!("shilpo-ipc-test-{}", rand_id()));
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        let socket = dir.join(APP_DIR).join(SOCKET);
        (dir, socket)
    }

    fn rand_id() -> u64 {
        use std::time::SystemTime;
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    }

    #[test]
    fn get_status_returns_current_snapshot() {
        let _guard = serial_guard();
        let (root, path) = fixture();
        let server = ShellIpcServer::new_at(&root, &path).unwrap();

        let health = ServiceHealth {
            compositor_connected: true,
            compositor_state: "ready".into(),
            compositor_revision: 1,
            compositor_reconnect_attempt: 0,
            compositor_last_error: None,
            compositor_telemetry: None,
            battery_service_available: true,
            audio_service_available: true,
            network_service_available: true,
            notification_service_available: true,
            heed_store_available: true,
            uptime_seconds: 42,
            ..Default::default()
        };

        server.update_status(IpcStatus {
            running: true,
            readiness: ReadinessState::Ready,
            bar: BarState::Visible,
            overview_visible: true,
            control_center_visible: false,
            health: health.clone(),
            ..Default::default()
        });

        let response = ShellIpcServer::send_command_at(&path, IpcRequest::GetStatus).unwrap();

        assert!(response.ok);
        let Some(IpcResult::Status(status_resp)) = response.result else {
            panic!("expected status response");
        };
        assert!(status_resp.running);
        assert_eq!(status_resp.readiness, ReadinessState::Ready);
        assert_eq!(status_resp.bar, BarState::Visible);
        assert!(status_resp.overview_visible);
        assert!(!status_resp.control_center_visible);
        assert!(!status_resp.instance_id.is_empty());
        assert!(status_resp.pid > 0);

        assert!(server.pop_pending_requests().is_empty());
        drop(server);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn get_telemetry_returns_health() {
        let _guard = serial_guard();
        let (root, path) = fixture();
        let server = ShellIpcServer::new_at(&root, &path).unwrap();

        let health = ServiceHealth {
            compositor_connected: true,
            compositor_state: "ready".into(),
            compositor_revision: 1,
            compositor_reconnect_attempt: 0,
            compositor_last_error: None,
            compositor_telemetry: None,
            battery_service_available: true,
            audio_service_available: true,
            network_service_available: true,
            notification_service_available: true,
            heed_store_available: true,
            uptime_seconds: 42,
            ..Default::default()
        };

        server.update_status(IpcStatus {
            running: true,
            health: health.clone(),
            ..Default::default()
        });

        let response = ShellIpcServer::send_command_at(&path, IpcRequest::GetTelemetry).unwrap();

        assert_eq!(response.result, Some(IpcResult::Telemetry(health)));
        assert!(server.pop_pending_requests().is_empty());
        drop(server);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn compositor_command_is_rejected_while_disconnected() {
        let _guard = serial_guard();
        let (root, path) = fixture();
        let server = ShellIpcServer::new_at(&root, &path).unwrap();

        let response = ShellIpcServer::send_command_at(
            &path,
            IpcRequest::Compositor(CompositorCommand::FocusWorkspace(1)),
        )
        .unwrap();

        assert!(!response.ok);
        assert_eq!(
            response.error.as_ref().map(|error| error.code.as_str()),
            Some("compositor_unavailable")
        );
        assert!(server.pop_pending_requests().is_empty());
        drop(server);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn compositor_command_succeeds_with_attached_broker() {
        let _guard = serial_guard();
        let (root, path) = fixture();
        let server = ShellIpcServer::new_at(&root, &path).unwrap();

        let executor: crate::compositor::broker::CommandExecutorFn =
            Box::new(|_cmd, _timeout, _cancel, _register| {
                Ok(crate::compositor::ExecutorAck::Success)
            });
        let broker =
            CompositorCommandBroker::new(crate::compositor::BrokerOptions::default(), executor);
        let snapshot = crate::compositor::CompositorSnapshot {
            connection: crate::compositor::CompositorConnection::Ready,
            workspaces: vec![crate::compositor::WorkspaceInfo {
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

        server.attach_broker(broker);

        let response = ShellIpcServer::send_command_at(
            &path,
            IpcRequest::Compositor(CompositorCommand::FocusWorkspace(1)),
        )
        .unwrap();

        assert!(response.ok);
        assert_eq!(response.version, 1);
        assert_eq!(
            response.result,
            Some(IpcResult::CommandApplied(CommandOutcome::Applied {
                revision: 0
            }))
        );

        let telemetry = ShellIpcServer::send_command_at(&path, IpcRequest::GetTelemetry).unwrap();
        let Some(IpcResult::Telemetry(health)) = telemetry.result else {
            panic!("expected telemetry response");
        };
        let broker_telemetry = health
            .compositor_telemetry
            .expect("attached broker telemetry");
        assert_eq!(broker_telemetry.accepted, 1);
        assert_eq!(broker_telemetry.succeeded, 1);

        drop(server);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn frame_limits_are_bounded() {
        let _guard = serial_guard();
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &[1]).unwrap();
        assert_eq!(read_frame(&mut bytes.as_slice()).unwrap(), vec![1]);
        assert!(write_frame(&mut Vec::new(), &vec![0; MAX_FRAME + 1]).is_err());
        let mut truncated: &[u8] = &[0, 0, 0, 0];
        assert!(read_frame(&mut truncated).is_err());
    }

    #[test]
    fn status_wire_fields_are_stable() {
        let _guard = serial_guard();
        let status = IpcStatus {
            running: true,
            readiness: ReadinessState::Degraded,
            bar: BarState::OpenFailed,
            overview_visible: false,
            control_center_visible: true,
            health: ServiceHealth {
                compositor_connected: true,
                compositor_state: "ready".into(),
                compositor_revision: 1,
                compositor_reconnect_attempt: 0,
                compositor_last_error: None,
                compositor_telemetry: None,
                battery_service_available: false,
                audio_service_available: true,
                network_service_available: true,
                notification_service_available: true,
                heed_store_available: true,
                uptime_seconds: 120,
                ..Default::default()
            },
            ..Default::default()
        };
        let value = serde_json::to_value(&status).unwrap();
        assert_eq!(value["readiness"], "degraded");
        assert_eq!(value["bar"], "open_failed");
        assert_eq!(value["overview_visible"], false);
        assert_eq!(value["control_center_visible"], true);
        assert_eq!(value["health"]["compositor_connected"], true);
        assert_eq!(value["health"]["uptime_seconds"], 120);
        assert!(value.get("message").is_none());
    }

    #[test]
    fn live_instance_and_non_socket_are_not_removed() {
        let _guard = serial_guard();
        let (root, path) = fixture();
        let server = ShellIpcServer::new_at(&root, &path).unwrap();
        assert!(matches!(
            ShellIpcServer::new_at(&root, &path),
            Err(IpcError::AlreadyRunning)
        ));
        drop(server);
        let other = root.join(APP_DIR).join("not-a-socket");
        fs::write(&other, b"not socket").unwrap();
        assert!(ShellIpcServer::new_at(&root, &other).is_err());
        assert!(other.is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn test_ipc_server_high_concurrency_bench() {
        let _guard = serial_guard();
        let (root, path) = fixture();
        let server = ShellIpcServer::new_at(&root, &path).unwrap();

        let mut handles = Vec::new();
        for _ in 0..10 {
            let socket_path = path.clone();
            handles.push(std::thread::spawn(move || {
                let req = IpcRequest::ToggleOverview;
                let _ = ShellIpcServer::send_command_at(&socket_path, req);
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
        let requests = server.pop_pending_requests();
        assert_eq!(requests.len(), 10);
        drop(server);
        fs::remove_dir_all(root).unwrap();
    }
}
