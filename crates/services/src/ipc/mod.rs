use anyhow::{Context, Result};
use std::{
    env,
    fs::remove_file,
    io::{BufRead, BufReader, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
};

/// IPC command request.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum IpcRequest {
    FocusWorkspace(u64),
    ReloadConfig,
    ToggleBar,
    GetStatus,
}

/// IPC command response.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IpcResponse {
    pub success: bool,
    pub message: String,
}

/// Helper function to resolve default socket path.
pub fn get_socket_path() -> PathBuf {
    if let Ok(dir) = env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(dir).join("shilpo-shell.sock")
    } else {
        PathBuf::from("/tmp/shilpo-shell.sock")
    }
}

/// Inter-Process Communication (IPC) Socket Server for Shilpo Desktop Shell.
pub struct ShellIpcServer {
    socket_path: PathBuf,
    pending_commands: Arc<Mutex<Vec<IpcRequest>>>,
}

impl ShellIpcServer {
    /// Starts the Unix domain socket server.
    pub fn new() -> Result<Self> {
        let socket_path = get_socket_path();
        if socket_path.exists() {
            let _ = remove_file(&socket_path);
        }

        let listener = UnixListener::bind(&socket_path)
            .with_context(|| format!("Failed to bind IPC socket at {:?}", socket_path))?;

        let pending_commands = Arc::new(Mutex::new(Vec::new()));
        let pending_clone = pending_commands.clone();

        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let pending_clone = pending_clone.clone();
                thread::spawn(move || {
                    let _ = handle_client(stream, pending_clone);
                });
            }
        });

        Ok(Self {
            socket_path,
            pending_commands,
        })
    }

    /// Pops and returns any pending IPC requests sent by CLI clients.
    pub fn pop_pending_requests(&self) -> Vec<IpcRequest> {
        let mut lock = self.pending_commands.lock().unwrap();
        std::mem::take(&mut *lock)
    }

    /// Sends an IPC command to a running Shilpo shell instance.
    pub fn send_command(req: IpcRequest) -> Result<IpcResponse> {
        let socket_path = get_socket_path();
        let mut stream = UnixStream::connect(&socket_path)
            .with_context(|| format!("Shilpo Shell is not running at {:?}", socket_path))?;

        let json = serde_json::to_string(&req)? + "\n";
        stream.write_all(json.as_bytes())?;
        stream.flush()?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line)?;

        let resp: IpcResponse = serde_json::from_str(&line)?;
        Ok(resp)
    }
}

impl Drop for ShellIpcServer {
    fn drop(&mut self) {
        if self.socket_path.exists() {
            let _ = remove_file(&self.socket_path);
        }
    }
}

fn handle_client(stream: UnixStream, pending: Arc<Mutex<Vec<IpcRequest>>>) -> Result<()> {
    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;

    let req: IpcRequest = serde_json::from_str(&line)?;
    pending.lock().unwrap().push(req.clone());

    let resp = IpcResponse {
        success: true,
        message: format!("Command {:?} enqueued successfully", req),
    };

    let mut writer = &stream;
    let resp_json = serde_json::to_string(&resp)? + "\n";
    writer.write_all(resp_json.as_bytes())?;
    writer.flush()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_server_roundtrip() {
        let server = ShellIpcServer::new().unwrap();
        let resp = ShellIpcServer::send_command(IpcRequest::FocusWorkspace(3)).unwrap();
        assert!(resp.success);

        let pending = server.pop_pending_requests();
        assert_eq!(pending.len(), 1);
        match &pending[0] {
            IpcRequest::FocusWorkspace(id) => assert_eq!(*id, 3),
            _ => panic!("Expected FocusWorkspace request"),
        }
    }
}
