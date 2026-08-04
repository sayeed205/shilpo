mod audio;
mod avhw;
mod capture;
mod clock;
mod encode;
mod fifo;
mod finalization;
mod fps_limit;
mod screencopy;
mod sources;
mod transform;
mod worker;

use crate::types::{
    RecordingAudio, RecordingCommand, RecordingCommandError, RecordingRequest, RecordingState,
};
use shilpo_config::RecordingConfig;
use std::sync::{Arc, Mutex};

pub(super) fn lock_recover<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Native Shilpo recording controller.
///
/// Each `Start` spawns a worker thread that owns the Wayland connection and
/// the FFmpeg/VAAPI pipeline; control commands (`pause`/`resume`/`stop`/
/// `cancel`) are forwarded to that worker. The authoritative state lives in a
/// shared mutex updated by the worker.
pub struct RecordingController {
    state: Arc<Mutex<RecordingState>>,
    config: Mutex<RecordingConfig>,
    worker: Arc<Mutex<Option<worker::WorkerHandle>>>,
}

/// Stable capability summary exposed to diagnostics without leaking native
/// FFmpeg or Wayland implementation types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingSupport {
    pub available: bool,
    pub window_capture: bool,
    pub encoder: Option<&'static str>,
    pub reason: Option<String>,
}

pub fn recording_support() -> RecordingSupport {
    let compositor = match worker::compositor_support() {
        Ok(compositor) => compositor,
        Err(reason) => {
            return RecordingSupport {
                available: false,
                window_capture: false,
                encoder: None,
                reason: Some(reason),
            };
        }
    };
    if let Err(reason) = encode::validate_runtime() {
        return RecordingSupport {
            available: false,
            window_capture: compositor.window_capture,
            encoder: None,
            reason: Some(reason),
        };
    }
    RecordingSupport {
        available: true,
        window_capture: compositor.window_capture,
        encoder: Some("VP8 VAAPI"),
        reason: None,
    }
}

/// Query the compositor for recordable sources using recorder-owned opaque
/// window identifiers.
pub fn discover_recording_sources() -> Result<crate::types::RecordingSourceCatalog, String> {
    sources::discover()
}

impl RecordingController {
    pub fn new(config: RecordingConfig) -> Self {
        Self {
            state: Arc::new(Mutex::new(RecordingState::Idle)),
            config: Mutex::new(config),
            worker: Arc::new(Mutex::new(None)),
        }
    }

    pub fn state(&self) -> RecordingState {
        lock_recover(&self.state).clone()
    }

    /// Apply configuration to future sessions. Active recordings retain the
    /// immutable settings they started with.
    pub fn update_config(&self, config: RecordingConfig) {
        *lock_recover(&self.config) = config;
    }

    /// Stop the active session and wait until its container is finalized.
    ///
    /// Shell shutdown uses this synchronous boundary from a background thread
    /// so the process cannot exit while WebM headers or trailers are pending.
    pub fn shutdown(&self) -> RecordingState {
        let state = self.state();
        if matches!(state, RecordingState::Selecting) {
            self.cancel_selection();
            return self.state();
        }

        if let Some(handle) = lock_recover(&self.worker).take() {
            if matches!(state, RecordingState::Starting { .. }) {
                handle.send(worker::WorkerCommand::Cancel);
            } else if state.is_active() {
                handle.send(worker::WorkerCommand::Stop);
            }
            handle.join();
        }
        self.state()
    }

    /// Reserve the controller while the shell presents its source chooser.
    pub fn begin_selection(&self) -> bool {
        let mut state = lock_recover(&self.state);
        if state.is_busy() {
            return false;
        }
        *state = RecordingState::Selecting;
        true
    }

    /// Return an abandoned source-selection session to idle.
    pub fn cancel_selection(&self) {
        let mut state = lock_recover(&self.state);
        if matches!(*state, RecordingState::Selecting) {
            *state = RecordingState::Idle;
        }
    }

    pub fn handle_command(
        &self,
        cmd: RecordingCommand,
    ) -> Result<RecordingState, RecordingCommandError> {
        cmd.validate(&self.state())?;
        match cmd {
            RecordingCommand::Start(request) => Ok(self.start(request)),
            RecordingCommand::Pause => {
                self.send(worker::WorkerCommand::Pause);
                Ok(self.state())
            }
            RecordingCommand::Resume => {
                self.send(worker::WorkerCommand::Resume);
                Ok(self.state())
            }
            RecordingCommand::Stop => {
                self.send(worker::WorkerCommand::Stop);
                Ok(self.state())
            }
            RecordingCommand::Cancel => {
                if matches!(self.state(), RecordingState::Selecting) {
                    self.cancel_selection();
                } else {
                    self.send(worker::WorkerCommand::Cancel);
                }
                Ok(self.state())
            }
            RecordingCommand::Status => Ok(self.state()),
        }
    }

    fn send(&self, command: worker::WorkerCommand) {
        if let Some(handle) = lock_recover(&self.worker).as_ref() {
            handle.send(command);
        }
    }

    fn start(&self, request: RecordingRequest) -> RecordingState {
        {
            let state = lock_recover(&self.state);
            if state.is_busy() && !matches!(*state, RecordingState::Selecting) {
                return state.clone();
            }
        }

        // Terminal worker threads are joined before their slot is reused so
        // every session has one explicit ownership boundary.
        if let Some(handle) = lock_recover(&self.worker).take() {
            handle.join();
        }

        let config = lock_recover(&self.config).clone();
        let target_path = match request.path {
            Some(path) if path.exists() || partial_recording_path(&path).exists() => {
                let state = RecordingState::Failed {
                    reason: format!("recording destination already exists: {}", path.display()),
                    partial_path: None,
                };
                *lock_recover(&self.state) = state.clone();
                return state;
            }
            Some(path) => path,
            None => {
                let directory = match config.ensure_directory() {
                    Ok(directory) => directory,
                    Err(error) => {
                        let state = RecordingState::Failed {
                            reason: format!("could not create recordings directory: {error}"),
                            partial_path: None,
                        };
                        *lock_recover(&self.state) = state.clone();
                        return state;
                    }
                };
                available_recording_path(&directory)
            }
        };
        let path_part = partial_recording_path(&target_path);
        let audio = match request.audio {
            RecordingAudio::Configured => match (config.desktop_audio, config.microphone) {
                (false, false) => RecordingAudio::None,
                (true, false) => RecordingAudio::Desktop,
                (false, true) => RecordingAudio::Microphone,
                (true, true) => RecordingAudio::DesktopAndMicrophone,
            },
            audio => audio,
        };

        *lock_recover(&self.state) = RecordingState::Starting {
            path: target_path.clone(),
        };

        let config = worker::WorkerConfig {
            source: request.source.clone(),
            path: target_path.clone(),
            path_part,
            audio,
            framerate: config.framerate.max(1),
            bitrate_bytes_per_second: 4_000_000,
            gop_size: config.framerate.max(1) * 2,
            delay: std::time::Duration::from_secs(config.delay_seconds),
            paint_cursor: config.show_pointer,
        };

        match worker::spawn(config, self.state.clone()) {
            Ok(handle) => {
                *lock_recover(&self.worker) = Some(handle);
            }
            Err(reason) => {
                *lock_recover(&self.state) = RecordingState::Failed {
                    reason,
                    partial_path: None,
                };
            }
        }

        self.state()
    }
}

impl Drop for RecordingController {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn partial_recording_path(target: &std::path::Path) -> std::path::PathBuf {
    let stem = target
        .file_stem()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Recording".into());
    target.with_file_name(format!("{stem}.part.webm"))
}

fn available_recording_path(directory: &std::path::Path) -> std::path::PathBuf {
    let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    for suffix in 1_u32.. {
        let name = if suffix == 1 {
            format!("Recording_{timestamp}.webm")
        } else {
            format!("Recording_{timestamp}_{suffix}.webm")
        };
        let candidate = directory.join(name);
        if !candidate.exists() && !partial_recording_path(&candidate).exists() {
            return candidate;
        }
    }
    unreachable!("u32 recording suffix space exhausted")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{RecordingAudio, RecordingSource};

    #[test]
    fn test_recording_controller_lifecycle() {
        let config = RecordingConfig::default();
        let controller = RecordingController::new(config);

        assert_eq!(controller.state(), RecordingState::Idle);

        let start_cmd = RecordingCommand::Start(RecordingRequest {
            source: RecordingSource::primary(),
            audio: RecordingAudio::Desktop,
            path: None,
        });

        let state = controller.handle_command(start_cmd).unwrap();
        assert!(matches!(
            state,
            RecordingState::Starting { .. }
                | RecordingState::Failed { .. }
                | RecordingState::Cancelled
        ));
    }

    #[test]
    fn selection_can_be_cancelled_without_starting_a_worker() {
        let controller = RecordingController::new(RecordingConfig::default());
        assert!(controller.begin_selection());
        assert!(!controller.begin_selection());
        assert_eq!(controller.state(), RecordingState::Selecting);
        controller.handle_command(RecordingCommand::Cancel).unwrap();
        assert_eq!(controller.state(), RecordingState::Idle);
    }

    #[test]
    fn partial_recording_keeps_the_webm_extension() {
        assert_eq!(
            partial_recording_path(std::path::Path::new("/tmp/Recording.webm")),
            std::path::PathBuf::from("/tmp/Recording.part.webm")
        );
    }

    #[test]
    fn invalid_control_command_returns_an_error() {
        let controller = RecordingController::new(RecordingConfig::default());
        let error = controller
            .handle_command(RecordingCommand::Stop)
            .unwrap_err();
        assert_eq!(error.command, "stop");
        assert_eq!(error.state, "idle");
    }

    #[test]
    fn generated_recording_path_never_overwrites_an_existing_file() {
        let directory = tempfile::tempdir().unwrap();
        let first = available_recording_path(directory.path());
        std::fs::write(&first, b"existing").unwrap();
        let second = available_recording_path(directory.path());
        assert_ne!(first, second);
        assert!(!second.exists());
    }
}
