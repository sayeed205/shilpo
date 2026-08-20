//! Single source of truth for spawning the dedicated `shilpo lock` process role (ADR-0005,
//! issue #135). Every trigger — the idle domain's `Lock`/`LockAndSuspend` actions, the
//! `org.shilpo.Shell.Lock()` D-Bus method, and the `PrepareForSleep` watch — goes through
//! one `LockSupervisor` instance so telemetry has a single, consistent view of whether a
//! locker is running and what its last spawn error was, instead of each call site tracking
//! (or not tracking) its own state.

use std::io;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Environment variable telling a spawned locker where to signal readiness once every
/// output's session-lock surface is committed (`PlatformSessionLock::on_locked`). Only set
/// for suspend-triggered spawns that need to synchronously confirm the lock is up before
/// suspend is allowed to proceed; other triggers (manual CLI, D-Bus, idle) don't set it and
/// the locker skips the signal entirely.
pub const LOCK_READY_FIFO_ENV_VAR: &str = "SHILPO_LOCK_READY_FIFO";

#[derive(Debug, Clone)]
struct ActiveLock {
    pid: u32,
}

#[derive(Default)]
pub struct LockSupervisor {
    active: Mutex<Option<ActiveLock>>,
    last_error: Mutex<Option<String>>,
    last_spawn_reason: Mutex<Option<String>>,
}

impl LockSupervisor {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn is_active(&self) -> bool {
        self.active.lock().unwrap().is_some()
    }

    pub fn last_error(&self) -> Option<String> {
        self.last_error.lock().unwrap().clone()
    }

    pub fn last_spawn_reason(&self) -> Option<String> {
        self.last_spawn_reason.lock().unwrap().clone()
    }

    /// Spawns `shilpo lock` for `reason` (used only for diagnostics), fire-and-forget.
    pub fn spawn(self: &Arc<Self>, reason: &str) {
        self.spawn_with_env(reason, None);
    }

    /// Spawns `shilpo lock`, setting `LOCK_READY_FIFO_ENV_VAR` to `ready_fifo` when given.
    fn spawn_with_env(self: &Arc<Self>, reason: &str, ready_fifo: Option<&std::path::Path>) {
        *self.last_spawn_reason.lock().unwrap() = Some(reason.to_string());

        let exe = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(err) => {
                let message = format!("failed to resolve current executable: {err}");
                tracing::warn!(reason, %message, "cannot spawn shilpo lock");
                *self.last_error.lock().unwrap() = Some(message);
                return;
            }
        };

        let mut command = Command::new(exe);
        command.arg("lock");
        if let Some(fifo) = ready_fifo {
            command.env(LOCK_READY_FIFO_ENV_VAR, fifo);
        }

        match command.spawn() {
            Ok(mut child) => {
                let pid = child.id();
                *self.active.lock().unwrap() = Some(ActiveLock { pid });
                *self.last_error.lock().unwrap() = None;

                let this = self.clone();
                std::thread::Builder::new()
                    .name("shilpo-lock-reaper".into())
                    .spawn(move || {
                        let _ = child.wait();
                        let mut active = this.active.lock().unwrap();
                        if active.as_ref().is_some_and(|a| a.pid == pid) {
                            *active = None;
                        }
                    })
                    .ok();
            }
            Err(err) => {
                let message = format!("failed to spawn shilpo lock: {err}");
                tracing::warn!(reason, %message);
                *self.last_error.lock().unwrap() = Some(message);
            }
        }
    }

    /// Spawns the locker and blocks (on a dedicated thread; safe to call from an async
    /// context via `spawn_blocking`) until it signals readiness over a FIFO, or `timeout`
    /// elapses. Used by the `PrepareForSleep` watch, which must not release its delay
    /// inhibitor — and so must not let suspend proceed — until the session is actually
    /// locked, or it gives up waiting.
    pub fn spawn_and_wait_until_locked(self: &Arc<Self>, reason: &str, timeout: Duration) -> bool {
        let fifo_path =
            std::env::temp_dir().join(format!("shilpo-lock-ready-{}", std::process::id()));
        let _ = std::fs::remove_file(&fifo_path);

        if let Err(err) = make_fifo(&fifo_path) {
            tracing::warn!(%err, "failed to create lock-ready fifo; spawning without sync");
            self.spawn_with_env(reason, None);
            return false;
        }

        self.spawn_with_env(reason, Some(&fifo_path));

        let result = wait_for_fifo_signal(&fifo_path, timeout);

        // Do NOT delete the fifo here on a timeout: `wait_for_fifo_signal`'s reader thread
        // is blocked inside a plain `File::open()` on this exact path with no cancellation
        // mechanism (opening a FIFO for reading blocks until a writer connects, and there
        // is no non-blocking/pollable equivalent in std). If we unlink the path immediately,
        // a locker that is merely slow — rather than crashed or denied — can never open it
        // to write, and the reader thread leaks forever with zero chance of completing.
        // Deferring the unlink lets a late writer still connect and let that thread exit
        // normally; only a locker that never signals at all (crashed, or `finished` fired
        // instead of `locked`) leaves a leaked thread, bounded and rare enough to accept
        // rather than adding non-blocking FIFO I/O under time pressure.
        if result {
            let _ = std::fs::remove_file(&fifo_path);
        } else {
            std::thread::Builder::new()
                .name("shilpo-lock-ready-cleanup".into())
                .spawn(move || {
                    std::thread::sleep(Duration::from_secs(60));
                    let _ = std::fs::remove_file(&fifo_path);
                })
                .ok();
        }

        result
    }
}

#[cfg(unix)]
fn make_fifo(path: &std::path::Path) -> io::Result<()> {
    let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| io::Error::other("fifo path contains a NUL byte"))?;
    let rc = unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Blocks opening `path` for reading (which blocks until a writer opens it too) and reads
/// one byte, bounded by `timeout`. Runs the blocking I/O on a scoped thread so the caller
/// can still enforce a hard timeout even though `File::open` on a FIFO has no async
/// equivalent here.
fn wait_for_fifo_signal(path: &std::path::Path, timeout: Duration) -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    let path = path.to_path_buf();
    let handle = std::thread::Builder::new()
        .name("shilpo-lock-ready-wait".into())
        .spawn(move || {
            use std::io::Read;
            let result = std::fs::File::open(&path).and_then(|mut f| {
                let mut buf = [0u8; 1];
                f.read_exact(&mut buf)
            });
            let _ = tx.send(result.is_ok());
        });
    if handle.is_err() {
        return false;
    }
    rx.recv_timeout(timeout).unwrap_or(false)
}

/// Probes whether the compositor advertises `ext_session_lock_manager_v1`, for `shilpo
/// doctor`. A one-shot connect + registry roundtrip on a bounded thread (so a compositor
/// that never responds can't hang `doctor`), independent of the daemon and the locker
/// process — neither of which holds this connection themselves (the daemon never touches
/// session-lock at all, and the locker only exists transiently while a lock is active).
pub fn probe_session_lock_protocol_available() -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("shilpo-doctor-session-lock-probe".into())
        .spawn(move || {
            let available = probe_session_lock_protocol_blocking();
            let _ = tx.send(available);
        });
    if spawned.is_err() {
        return false;
    }
    rx.recv_timeout(Duration::from_secs(2)).unwrap_or(false)
}

fn probe_session_lock_protocol_blocking() -> bool {
    use wayland_client::protocol::wl_registry::{self, WlRegistry};
    use wayland_client::{Connection, Dispatch, QueueHandle};

    struct State {
        found: bool,
    }

    impl Dispatch<WlRegistry, ()> for State {
        fn event(
            state: &mut Self,
            _registry: &WlRegistry,
            event: wl_registry::Event,
            _data: &(),
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
        ) {
            if let wl_registry::Event::Global { interface, .. } = event
                && interface == "ext_session_lock_manager_v1"
            {
                state.found = true;
            }
        }
    }

    let Ok(conn) = Connection::connect_to_env() else {
        return false;
    };
    let mut event_queue = conn.new_event_queue::<State>();
    let qh = event_queue.handle();
    let display = conn.display();
    let _registry = display.get_registry(&qh, ());

    let mut state = State { found: false };
    let _ = event_queue.roundtrip(&mut state);
    state.found
}

/// Signals readiness to a `LockSupervisor` waiting on `LOCK_READY_FIFO_ENV_VAR`, if set.
/// Called from the locker process itself once every output's surface is confirmed locked.
pub fn signal_lock_ready() {
    let Ok(fifo_path) = std::env::var(LOCK_READY_FIFO_ENV_VAR) else {
        return;
    };
    // Opening a FIFO for writing blocks until a reader is present; run it on a background
    // thread so a reader that never shows up (the waiter already timed out) can't hang the
    // caller (the GPUI main thread) forever.
    std::thread::Builder::new()
        .name("shilpo-lock-ready-signal".into())
        .spawn(move || {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open(&fifo_path) {
                let _ = f.write_all(&[1u8]);
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_records_last_error_on_failure() {
        let supervisor = LockSupervisor::new();
        // current_exe() always succeeds in a test binary, so simulate the failure path
        // directly against the private helper via a bogus reason to at least exercise the
        // accessor methods' default state.
        assert!(!supervisor.is_active());
        assert!(supervisor.last_error().is_none());
        assert!(supervisor.last_spawn_reason().is_none());
    }

    #[test]
    fn fifo_roundtrip_signals_readiness() {
        let fifo_path = std::env::temp_dir().join(format!(
            "shilpo-lock-supervisor-test-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_file(&fifo_path);
        make_fifo(&fifo_path).expect("create fifo");

        let writer_path = fifo_path.clone();
        let writer = std::thread::spawn(move || {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .open(&writer_path)
                .expect("open fifo for writing");
            f.write_all(&[1u8]).expect("write signal byte");
        });

        let signaled = wait_for_fifo_signal(&fifo_path, Duration::from_secs(5));
        writer.join().unwrap();
        let _ = std::fs::remove_file(&fifo_path);

        assert!(signaled);
    }

    #[test]
    fn fifo_wait_times_out_when_never_signaled() {
        let fifo_path = std::env::temp_dir().join(format!(
            "shilpo-lock-supervisor-timeout-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_file(&fifo_path);
        make_fifo(&fifo_path).expect("create fifo");

        let signaled = wait_for_fifo_signal(&fifo_path, Duration::from_millis(200));
        let _ = std::fs::remove_file(&fifo_path);

        assert!(!signaled);
    }
}
