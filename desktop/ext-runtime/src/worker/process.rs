use super::protocol::{ExtensionCommand, ExtensionGeneration, ExtensionUpdate};
use crate::{CatalogPaths, WasmRuntime};
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Read, Write},
    sync::mpsc,
    thread,
    time::Duration,
};

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_FRAME_SIZE: usize = 8 * 1024 * 1024; // 8MB
pub const MAX_QUEUE_BOUND: usize = 64;

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct HostGeneration(pub u64);

impl HostGeneration {
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostMessage {
    pub protocol_version: u16,
    pub host_generation: HostGeneration,
    pub request_id: u64,
    pub command: ExtensionCommand,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkerPayload {
    Update(ExtensionUpdate),
    DevReload(super::protocol::DevReloadOutcome),
    ShutdownAck,
    FatalError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerMessage {
    pub protocol_version: u16,
    pub host_generation: HostGeneration,
    pub engine_generation: ExtensionGeneration,
    pub request_id: u64,
    pub payload: WorkerPayload,
}

#[derive(Debug)]
pub enum ProcessCodecError {
    ZeroLengthFrame,
    FrameTooLarge { length: usize },
    ProtocolVersionMismatch { expected: u16, found: u16 },
    Io(io::Error),
    Json(serde_json::Error),
}

impl std::fmt::Display for ProcessCodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroLengthFrame => write!(f, "zero-length frame rejected"),
            Self::FrameTooLarge { length } => {
                write!(f, "frame length {length} exceeds maximum {MAX_FRAME_SIZE}")
            }
            Self::ProtocolVersionMismatch { expected, found } => {
                write!(
                    f,
                    "protocol version mismatch: expected {expected}, found {found}"
                )
            }
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::Json(err) => write!(f, "JSON serialization error: {err}"),
        }
    }
}

impl std::error::Error for ProcessCodecError {}

pub fn read_frame<R: Read>(reader: &mut R) -> Result<Vec<u8>, ProcessCodecError> {
    let mut header = [0u8; 4];
    reader
        .read_exact(&mut header)
        .map_err(ProcessCodecError::Io)?;
    let length = u32::from_be_bytes(header) as usize;
    if length == 0 {
        return Err(ProcessCodecError::ZeroLengthFrame);
    }
    if length > MAX_FRAME_SIZE {
        return Err(ProcessCodecError::FrameTooLarge { length });
    }
    let mut payload = vec![0u8; length];
    reader
        .read_exact(&mut payload)
        .map_err(ProcessCodecError::Io)?;
    Ok(payload)
}

pub fn write_frame<W: Write>(writer: &mut W, payload: &[u8]) -> Result<(), ProcessCodecError> {
    if payload.is_empty() {
        return Err(ProcessCodecError::ZeroLengthFrame);
    }
    if payload.len() > MAX_FRAME_SIZE {
        return Err(ProcessCodecError::FrameTooLarge {
            length: payload.len(),
        });
    }
    let length_header = (payload.len() as u32).to_be_bytes();
    writer
        .write_all(&length_header)
        .map_err(ProcessCodecError::Io)?;
    writer.write_all(payload).map_err(ProcessCodecError::Io)?;
    writer.flush().map_err(ProcessCodecError::Io)?;
    Ok(())
}

/// Incremental frame decoder used with the nonblocking worker stdout pipe.
/// Unlike `read_frame`, this preserves partial headers and payloads across
/// `WouldBlock` returns.
#[derive(Default)]
pub struct FrameReader {
    buffer: Vec<u8>,
}

impl FrameReader {
    pub fn try_read_frame<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> Result<Option<Vec<u8>>, ProcessCodecError> {
        let mut scratch = [0u8; 8192];
        loop {
            if self.buffer.len() >= 4 {
                let length = u32::from_be_bytes(self.buffer[..4].try_into().unwrap()) as usize;
                if length == 0 {
                    return Err(ProcessCodecError::ZeroLengthFrame);
                }
                if length > MAX_FRAME_SIZE {
                    return Err(ProcessCodecError::FrameTooLarge { length });
                }
                if self.buffer.len() >= 4 + length {
                    let payload = self.buffer[4..4 + length].to_vec();
                    self.buffer.drain(..4 + length);
                    return Ok(Some(payload));
                }
            }

            match reader.read(&mut scratch) {
                Ok(0) => {
                    return Err(ProcessCodecError::Io(io::Error::from(
                        io::ErrorKind::UnexpectedEof,
                    )));
                }
                Ok(read) => self.buffer.extend_from_slice(&scratch[..read]),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(None),
                Err(error) => return Err(ProcessCodecError::Io(error)),
            }
        }
    }
}

pub fn send_host_message<W: Write>(
    writer: &mut W,
    message: &HostMessage,
) -> Result<(), ProcessCodecError> {
    let bytes = serde_json::to_vec(message).map_err(ProcessCodecError::Json)?;
    write_frame(writer, &bytes)
}

pub fn recv_host_message<R: Read>(reader: &mut R) -> Result<HostMessage, ProcessCodecError> {
    let bytes = read_frame(reader)?;
    let message: HostMessage = serde_json::from_slice(&bytes).map_err(ProcessCodecError::Json)?;
    if message.protocol_version != PROTOCOL_VERSION {
        return Err(ProcessCodecError::ProtocolVersionMismatch {
            expected: PROTOCOL_VERSION,
            found: message.protocol_version,
        });
    }
    Ok(message)
}

pub fn send_worker_message<W: Write>(
    writer: &mut W,
    message: &WorkerMessage,
) -> Result<(), ProcessCodecError> {
    let bytes = serde_json::to_vec(message).map_err(ProcessCodecError::Json)?;
    write_frame(writer, &bytes)
}

pub fn recv_worker_message<R: Read>(reader: &mut R) -> Result<WorkerMessage, ProcessCodecError> {
    let bytes = read_frame(reader)?;
    let message: WorkerMessage = serde_json::from_slice(&bytes).map_err(ProcessCodecError::Json)?;
    if message.protocol_version != PROTOCOL_VERSION {
        return Err(ProcessCodecError::ProtocolVersionMismatch {
            expected: PROTOCOL_VERSION,
            found: message.protocol_version,
        });
    }
    Ok(message)
}

pub fn recv_worker_message_nonblocking<R: Read>(
    reader: &mut R,
    frame_reader: &mut FrameReader,
) -> Result<Option<WorkerMessage>, ProcessCodecError> {
    let Some(bytes) = frame_reader.try_read_frame(reader)? else {
        return Ok(None);
    };
    let message: WorkerMessage = serde_json::from_slice(&bytes).map_err(ProcessCodecError::Json)?;
    if message.protocol_version != PROTOCOL_VERSION {
        return Err(ProcessCodecError::ProtocolVersionMismatch {
            expected: PROTOCOL_VERSION,
            found: message.protocol_version,
        });
    }
    Ok(Some(message))
}

/// The main loop for the `shilpo extension-host` child process.
/// Reads framed `HostMessage`s from `stdin`, executes WASM runtime logic,
/// and writes framed `WorkerMessage`s to `stdout`.
pub fn run_extension_host() {
    tracing::info!("shilpo extension-host role started");
    let paths = CatalogPaths::platform_default();
    let runtime = match WasmRuntime::new_with_paths(&paths) {
        Ok(rt) => rt,
        Err(error) => {
            eprintln!("failed to initialize Wasmtime runtime: {error}");
            std::process::exit(1);
        }
    };
    let mut engine = match super::engine::ExtensionEngine::new(runtime, paths) {
        Ok(eng) => eng,
        Err(error) => {
            eprintln!("failed to initialize extension engine: {error}");
            std::process::exit(1);
        }
    };

    let stdout = io::stdout();
    let mut writer = stdout.lock();

    // Read initial HostMessage handshake to establish host_generation
    let first_msg = match recv_host_message(&mut io::stdin().lock()) {
        Ok(msg) => msg,
        Err(error) => {
            eprintln!("extension-host failed reading initial handshake: {error}");
            std::process::exit(1);
        }
    };

    let host_generation = first_msg.host_generation;
    let initial_update = engine.build_snapshot(true);

    let initial_worker_msg = WorkerMessage {
        protocol_version: PROTOCOL_VERSION,
        host_generation,
        engine_generation: engine.generation(),
        request_id: first_msg.request_id,
        payload: WorkerPayload::Update(ExtensionUpdate {
            host_generation: HostGeneration(0),
            generation: engine.generation(),
            snapshot: Some(initial_update),
            effects: Vec::new(),
            invalidated_views: Vec::new(),
            circuit_notices: Vec::new(),
        }),
    };

    if let Err(error) = send_worker_message(&mut writer, &initial_worker_msg) {
        eprintln!("extension-host failed writing initial snapshot: {error}");
        std::process::exit(1);
    }

    if !matches!(first_msg.command, ExtensionCommand::SourcesChanged)
        && let Some(update) = engine.handle_command(first_msg.command)
    {
        let msg = WorkerMessage {
            protocol_version: PROTOCOL_VERSION,
            host_generation,
            engine_generation: engine.generation(),
            request_id: first_msg.request_id,
            payload: WorkerPayload::Update(update),
        };
        let _ = send_worker_message(&mut writer, &msg);
    }

    let (command_tx, command_rx) = mpsc::sync_channel(64);
    thread::spawn(move || {
        let mut reader = io::stdin().lock();
        loop {
            match recv_host_message(&mut reader) {
                Ok(message) => {
                    if command_tx.send(Ok(message)).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    let _ = command_tx.send(Err(error));
                    break;
                }
            }
        }
    });

    loop {
        let timeout = engine
            .next_tick_deadline()
            .map_or(Duration::from_millis(20), |d| {
                d.min(Duration::from_millis(20))
            });
        let msg = match command_rx.recv_timeout(timeout) {
            Ok(Ok(message)) => message,
            Ok(Err(ProcessCodecError::Io(error)))
                if error.kind() == io::ErrorKind::UnexpectedEof =>
            {
                tracing::info!("stdin closed; shutting down extension-host");
                break;
            }
            Ok(Err(error)) => {
                eprintln!("extension-host error reading message: {error}");
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Some(update) = engine.tick() {
                    let notification = WorkerMessage {
                        protocol_version: PROTOCOL_VERSION,
                        host_generation,
                        engine_generation: engine.generation(),
                        request_id: 0,
                        payload: WorkerPayload::Update(update),
                    };
                    if send_worker_message(&mut writer, &notification).is_err() {
                        break;
                    }
                }
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };

        if matches!(msg.command, ExtensionCommand::Shutdown) {
            let _ = engine.handle_command(ExtensionCommand::Shutdown);
            let ack = WorkerMessage {
                protocol_version: PROTOCOL_VERSION,
                host_generation,
                engine_generation: engine.generation(),
                request_id: msg.request_id,
                payload: WorkerPayload::ShutdownAck,
            };
            let _ = send_worker_message(&mut writer, &ack);
            tracing::info!("extension-host received shutdown; exiting cleanly");
            break;
        }

        if let ExtensionCommand::DevReload {
            session_id,
            extension_id,
            canonical_root,
            artifact_path,
            build_sequence,
            ..
        } = msg.command
        {
            let outcome = engine.handle_dev_reload(
                session_id,
                extension_id,
                canonical_root,
                artifact_path,
                build_sequence,
            );
            let reply = WorkerMessage {
                protocol_version: PROTOCOL_VERSION,
                host_generation,
                engine_generation: engine.generation(),
                request_id: msg.request_id,
                payload: WorkerPayload::DevReload(outcome),
            };
            if let Err(error) = send_worker_message(&mut writer, &reply) {
                eprintln!("extension-host error sending dev reload reply: {error}");
                break;
            }
            continue;
        }

        if let Some(update) = engine.handle_command(msg.command) {
            let reply = WorkerMessage {
                protocol_version: PROTOCOL_VERSION,
                host_generation,
                engine_generation: engine.generation(),
                request_id: msg.request_id,
                payload: WorkerPayload::Update(update),
            };
            if let Err(error) = send_worker_message(&mut writer, &reply) {
                eprintln!("extension-host error sending update: {error}");
                break;
            }
        }
    }
}
