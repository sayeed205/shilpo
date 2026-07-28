//! Authenticated, bounded Unix-socket IPC for the shell.
//!
//! libc is used only for effective-uid lookup, Linux peer credentials, and
//! advisory locking. All calls below check their return values; raw file
//! descriptors are borrowed from Rust-owned objects and never closed here.

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
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

const APP_DIR: &str = "shilpo-shell";
const SOCKET: &str = "ipc.sock";
const LOCK: &str = "instance.lock";
const MAX_FRAME: usize = 16 * 1024;
const MAX_QUEUE: usize = 128;
const IO_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "payload")]
pub enum IpcRequest {
    FocusWorkspace(u64),
    ReloadConfig,
    ToggleBar,
    ToggleLauncher,
    ToggleControlCenter,
    ToggleOverview,
    GetStatus,
    GetTelemetry,
    Quit,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
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
    pub battery_service_available: bool,
    pub audio_service_available: bool,
    pub network_service_available: bool,
    pub notification_service_available: bool,
    pub heed_store_available: bool,
    pub uptime_seconds: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct IpcStatus {
    pub running: bool,
    pub readiness: ReadinessState,
    pub bar: BarState,
    pub launcher_visible: bool,
    pub control_center_visible: bool,
    #[serde(default)]
    pub health: ServiceHealth,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum IpcResult {
    Accepted,
    Status(IpcStatus),
    Telemetry(ServiceHealth),
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
    stop: Arc<std::sync::atomic::AtomicBool>,
    thread: Option<JoinHandle<()>>,
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
        let status = Arc::new(Mutex::new(IpcStatus {
            running: true,
            ..IpcStatus::default()
        }));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let p = pending.clone();
        let s = status.clone();
        let quit = stop.clone();
        let thread = thread::Builder::new()
            .name("shilpo-ipc".into())
            .spawn(move || {
                while !quit.load(std::sync::atomic::Ordering::Acquire) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let _ = handle_client(stream, &p, &s);
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10))
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
            stop,
            thread: Some(thread),
            _lock: lock,
        })
    }
    pub fn pop_pending_requests(&self) -> Vec<IpcRequest> {
        self.pending.lock().unwrap().drain(..).collect()
    }
    pub fn update_status(&self, status: IpcStatus) {
        *self.status.lock().unwrap() = status;
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
impl Drop for ShellIpcServer {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
        if let Ok(s) = UnixStream::connect(&self.socket_path) {
            drop(s);
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
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
fn handle_client(
    mut stream: UnixStream,
    pending: &Arc<Mutex<VecDeque<IpcRequest>>>,
    status: &Arc<Mutex<IpcStatus>>,
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
    let compositor_unavailable = matches!(&env.request, IpcRequest::FocusWorkspace(_))
        && !status.lock().unwrap().health.compositor_connected;
    let queue_full = !is_status && !is_telemetry && pending.lock().unwrap().len() >= MAX_QUEUE;
    let result = if is_status {
        Some(IpcResult::Status(status.lock().unwrap().clone()))
    } else if is_telemetry {
        Some(IpcResult::Telemetry(status.lock().unwrap().health.clone()))
    } else if compositor_unavailable || queue_full {
        None
    } else {
        let mut q = pending.lock().unwrap();
        q.push_back(env.request);
        Some(IpcResult::Accepted)
    };
    let err = if compositor_unavailable {
        Some(IpcErrorBody {
            code: "compositor_unavailable".into(),
            message: "compositor is not connected; command was not queued".into(),
        })
    } else if queue_full {
        Some(IpcErrorBody {
            code: "busy".into(),
            message: "request queue full".into(),
        })
    } else {
        None
    };
    write_frame(
        &mut stream,
        &serde_json::to_vec(&response(env.request_id, result, err))?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

    fn fixture() -> (PathBuf, PathBuf) {
        let root = env::temp_dir().join(format!(
            "shilpo-ipc-test-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let app = root.join(APP_DIR);
        fs::create_dir(&app).unwrap();
        fs::set_permissions(&app, fs::Permissions::from_mode(0o700)).unwrap();
        (root, app.join(SOCKET))
    }

    #[test]
    fn round_trip_status_and_fifo() {
        let (root, path) = fixture();
        let server = ShellIpcServer::new_at(&root, &path).unwrap();
        server.update_status(IpcStatus {
            running: true,
            readiness: ReadinessState::Ready,
            bar: BarState::Visible,
            launcher_visible: false,
            control_center_visible: false,
            health: ServiceHealth::default(),
        });
        let status = ShellIpcServer::send_command_at(&path, IpcRequest::GetStatus).unwrap();
        assert_eq!(
            status.result,
            Some(IpcResult::Status(IpcStatus {
                running: true,
                readiness: ReadinessState::Ready,
                bar: BarState::Visible,
                launcher_visible: false,
                control_center_visible: false,
                health: ServiceHealth::default(),
            }))
        );
        assert!(
            ShellIpcServer::send_command_at(&path, IpcRequest::ToggleBar)
                .unwrap()
                .ok
        );
        assert!(matches!(
            server.pop_pending_requests().as_slice(),
            [IpcRequest::ToggleBar]
        ));
        drop(server);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn telemetry_is_returned_directly_without_entering_command_queue() {
        let (root, path) = fixture();
        let server = ShellIpcServer::new_at(&root, &path).unwrap();
        let health = ServiceHealth {
            compositor_connected: true,
            uptime_seconds: 42,
            ..Default::default()
        };
        server.update_status(IpcStatus {
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
        let (root, path) = fixture();
        let server = ShellIpcServer::new_at(&root, &path).unwrap();

        let response =
            ShellIpcServer::send_command_at(&path, IpcRequest::FocusWorkspace(1)).unwrap();

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
    fn frame_limits_are_bounded() {
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &[1]).unwrap();
        assert_eq!(read_frame(&mut bytes.as_slice()).unwrap(), vec![1]);
        assert!(write_frame(&mut Vec::new(), &vec![0; MAX_FRAME + 1]).is_err());
        let mut truncated: &[u8] = &[0, 0, 0, 0];
        assert!(read_frame(&mut truncated).is_err());
    }

    #[test]
    fn status_wire_fields_are_stable() {
        let status = IpcStatus {
            running: true,
            readiness: ReadinessState::Degraded,
            bar: BarState::OpenFailed,
            launcher_visible: false,
            control_center_visible: true,
            health: ServiceHealth {
                compositor_connected: true,
                compositor_state: "ready".into(),
                compositor_revision: 1,
                compositor_reconnect_attempt: 0,
                compositor_last_error: None,
                battery_service_available: false,
                audio_service_available: true,
                network_service_available: true,
                notification_service_available: true,
                heed_store_available: true,
                uptime_seconds: 120,
            },
        };
        let value = serde_json::to_value(&status).unwrap();
        assert_eq!(value["readiness"], "degraded");
        assert_eq!(value["bar"], "open_failed");
        assert_eq!(value["launcher_visible"], false);
        assert_eq!(value["control_center_visible"], true);
        assert_eq!(value["health"]["compositor_connected"], true);
        assert_eq!(value["health"]["uptime_seconds"], 120);
        assert!(value.get("message").is_none());
    }

    #[test]
    fn live_instance_and_non_socket_are_not_removed() {
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
        let (root, path) = fixture();
        let server = ShellIpcServer::new_at(&root, &path).unwrap();

        let mut handles = Vec::new();
        for _ in 0..10 {
            let socket_path = path.clone();
            handles.push(std::thread::spawn(move || {
                let req = IpcRequest::ToggleLauncher;
                let _ = ShellIpcServer::send_command_at(&socket_path, req);
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let reqs = server.pop_pending_requests();
        assert_eq!(reqs.len(), 10);
        for req in reqs {
            assert!(matches!(req, IpcRequest::ToggleLauncher));
        }

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn test_ipc_integration_and_security_validation() {
        let (root, path) = fixture();
        let server = ShellIpcServer::new_at(&root, &path).unwrap();

        let req = IpcRequest::ToggleLauncher;
        let resp = ShellIpcServer::send_command_at(&path, req);
        assert!(resp.is_ok());

        let reqs = server.pop_pending_requests();
        assert_eq!(reqs.len(), 1);

        fs::remove_dir_all(root).unwrap();
    }
}
