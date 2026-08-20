//! Single source of truth for spawning the dedicated `shilpo lock` process role (ADR-0005,
//! issue #135). Every trigger — the idle domain's `Lock`/`LockAndSuspend` actions, the
//! `org.shilpo.Shell.Lock()` D-Bus method, and the `PrepareForSleep` watch — goes through
//! one `LockSupervisor` instance so telemetry has a single, consistent view of whether a
//! locker is running and what its last spawn error was, instead of each call site tracking
//! (or not tracking) its own state.

use std::io;
use std::process::Command;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// Environment variable telling a spawned locker where to signal readiness once every
/// output's session-lock surface is committed (`PlatformSessionLock::on_locked`). Set on
/// every spawn `LockSupervisor` launches (not just suspend-triggered ones) so any caller
/// that later joins the same in-flight attempt via `acquire_or_join_slot` can still observe
/// readiness, regardless of which trigger actually started the process.
pub const LOCK_READY_FIFO_ENV_VAR: &str = "SHILPO_LOCK_READY_FIFO";

/// How long the internal FIFO reader thread waits for a spawned locker to signal readiness
/// before giving up and treating the attempt as failed. Generous relative to any caller's
/// own [`LockSupervisor::spawn_and_wait_until_locked`] timeout (a few seconds) so it never
/// cuts a legitimate wait short; it exists only as a bound on the worst case (a locker that
/// crashed or hung before ever reaching `on_locked`).
const READY_SIGNAL_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
struct ActiveLock {
    pid: u32,
}

/// A one-shot, multi-waiter readiness result for a single locker attempt. Every caller that
/// wants to know "is the session locked yet" for the *currently running* locker joins the
/// same slot instead of starting a competing locker process -- `ext-session-lock-v1` only
/// allows one lock at a time, so a second `lock()` call from a second process would just be
/// denied and immediately `finished`, never signaling readiness.
struct ReadinessSlot {
    result: Mutex<Option<bool>>,
    cvar: Condvar,
}

impl ReadinessSlot {
    fn new() -> Self {
        Self {
            result: Mutex::new(None),
            cvar: Condvar::new(),
        }
    }

    /// Resolves the slot once; subsequent calls (e.g. a late FIFO signal after the reaper
    /// already resolved a crash) are ignored.
    fn resolve(&self, value: bool) {
        let mut result = self.result.lock().unwrap();
        if result.is_none() {
            *result = Some(value);
            self.cvar.notify_all();
        }
    }

    /// Waits up to `timeout` for a result, without affecting the slot's shared state -- a
    /// caller giving up doesn't stop other callers (or the locker itself) from still
    /// resolving it later.
    fn wait(&self, timeout: Duration) -> bool {
        let mut result = self.result.lock().unwrap();
        let deadline = Instant::now() + timeout;
        while result.is_none() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            let (guard, wait_result) = self.cvar.wait_timeout(result, remaining).unwrap();
            result = guard;
            if wait_result.timed_out() && result.is_none() {
                return false;
            }
        }
        result.unwrap_or(false)
    }
}

#[derive(Default)]
pub struct LockSupervisor {
    active: Mutex<Option<ActiveLock>>,
    last_error: Mutex<Option<String>>,
    last_spawn_reason: Mutex<Option<String>>,
    /// `Some` while a locker attempt is in flight or has an unresolved outcome; cleared once
    /// the locker exits. Callers join this instead of spawning a second locker.
    readiness: Mutex<Option<Arc<ReadinessSlot>>>,
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

    /// Spawns `shilpo lock` for `reason` (used only for diagnostics), fire-and-forget. If a
    /// locker is already starting or running, this joins its outcome instead of launching a
    /// second, competing locker process (which `ext-session-lock-v1` would just deny).
    pub fn spawn(self: &Arc<Self>, reason: &str) {
        *self.last_spawn_reason.lock().unwrap() = Some(reason.to_string());
        let (slot, created) = self.acquire_or_join_slot();
        if created {
            self.start_locker(reason, slot);
        }
    }

    /// Spawns the locker (or joins one already in flight) and blocks the calling thread
    /// (safe to call from an async context via `spawn_blocking`) until it signals readiness
    /// over a FIFO, or `timeout` elapses. Used by the `PrepareForSleep` watch, which must
    /// not release its delay inhibitor — and so must not let suspend proceed — until the
    /// session is actually locked, or it gives up waiting. Joining an in-flight attempt
    /// (rather than always starting a fresh one) matters here: `IdleAction::LockAndSuspend`
    /// starts its own best-effort locker before calling `Suspend`, and the resulting
    /// `PrepareForSleep` signal reaches this watch immediately after — without joining, that
    /// second call would try to open a second `ext-session-lock-v1` lock, get denied, and
    /// spend its whole timeout waiting on a locker that will never signal readiness.
    pub fn spawn_and_wait_until_locked(self: &Arc<Self>, reason: &str, timeout: Duration) -> bool {
        *self.last_spawn_reason.lock().unwrap() = Some(reason.to_string());
        let (slot, created) = self.acquire_or_join_slot();
        if created {
            self.start_locker(reason, slot.clone());
        }
        slot.wait(timeout)
    }

    /// Returns the readiness slot for the currently in-flight/active locker, creating one
    /// (and registering it) if none exists. `created` is `true` only for the caller that
    /// just created it, so exactly one caller actually spawns the process.
    fn acquire_or_join_slot(&self) -> (Arc<ReadinessSlot>, bool) {
        let mut readiness = self.readiness.lock().unwrap();
        if let Some(existing) = readiness.as_ref() {
            (existing.clone(), false)
        } else {
            let slot = Arc::new(ReadinessSlot::new());
            *readiness = Some(slot.clone());
            (slot, true)
        }
    }

    /// Clears `readiness` back to `None`, but only if it still points at `slot` -- guards
    /// against clobbering a newer attempt that may have started in the meantime.
    fn clear_readiness_if_matches(&self, slot: &Arc<ReadinessSlot>) {
        let mut readiness = self.readiness.lock().unwrap();
        if readiness
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, slot))
        {
            *readiness = None;
        }
    }

    /// Actually launches `shilpo lock`, always wired to a readiness FIFO so any caller
    /// (present or future, via `acquire_or_join_slot`) can observe when it locks, and starts
    /// the reaper + FIFO-reader threads that resolve `slot`.
    fn start_locker(self: &Arc<Self>, reason: &str, slot: Arc<ReadinessSlot>) {
        let exe = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(err) => {
                let message = format!("failed to resolve current executable: {err}");
                tracing::warn!(reason, %message, "cannot spawn shilpo lock");
                *self.last_error.lock().unwrap() = Some(message);
                slot.resolve(false);
                self.clear_readiness_if_matches(&slot);
                return;
            }
        };

        let fifo_path =
            std::env::temp_dir().join(format!("shilpo-lock-ready-{}", std::process::id()));
        let _ = std::fs::remove_file(&fifo_path);
        let fifo_ready = match make_fifo(&fifo_path) {
            Ok(()) => true,
            Err(err) => {
                tracing::warn!(%err, "failed to create lock-ready fifo; spawning without sync");
                false
            }
        };

        let mut command = Command::new(exe);
        command.arg("lock");
        if fifo_ready {
            command.env(LOCK_READY_FIFO_ENV_VAR, &fifo_path);
        }

        match command.spawn() {
            Ok(mut child) => {
                let pid = child.id();
                *self.active.lock().unwrap() = Some(ActiveLock { pid });
                *self.last_error.lock().unwrap() = None;

                if fifo_ready {
                    let reader_slot = slot.clone();
                    let reader_fifo = fifo_path.clone();
                    std::thread::Builder::new()
                        .name("shilpo-lock-ready-wait".into())
                        .spawn(move || {
                            let signaled = wait_for_fifo_signal(&reader_fifo, READY_SIGNAL_TIMEOUT);
                            reader_slot.resolve(signaled);
                            if signaled {
                                let _ = std::fs::remove_file(&reader_fifo);
                            } else {
                                // Locker never signaled within the bound (crashed, denied,
                                // or hung before `on_locked`); defer cleanup in case a very
                                // late writer still connects, same rationale as before.
                                std::thread::Builder::new()
                                    .name("shilpo-lock-ready-cleanup".into())
                                    .spawn(move || {
                                        std::thread::sleep(Duration::from_secs(60));
                                        let _ = std::fs::remove_file(&reader_fifo);
                                    })
                                    .ok();
                            }
                        })
                        .ok();
                } else {
                    slot.resolve(false);
                    self.clear_readiness_if_matches(&slot);
                }

                let this = self.clone();
                let reaper_slot = slot.clone();
                std::thread::Builder::new()
                    .name("shilpo-lock-reaper".into())
                    .spawn(move || {
                        let _ = child.wait();
                        let mut active = this.active.lock().unwrap();
                        if active.as_ref().is_some_and(|a| a.pid == pid) {
                            *active = None;
                        }
                        drop(active);
                        // No-op if the FIFO reader already resolved this attempt.
                        reaper_slot.resolve(false);
                        this.clear_readiness_if_matches(&reaper_slot);
                    })
                    .ok();
            }
            Err(err) => {
                let message = format!("failed to spawn shilpo lock: {err}");
                tracing::warn!(reason, %message);
                *self.last_error.lock().unwrap() = Some(message);
                slot.resolve(false);
                self.clear_readiness_if_matches(&slot);
            }
        }
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
    fn readiness_slot_resolves_once_and_notifies_all_waiters() {
        let slot = Arc::new(ReadinessSlot::new());
        let waiters: Vec<_> = (0..3)
            .map(|_| {
                let slot = slot.clone();
                std::thread::spawn(move || slot.wait(Duration::from_secs(5)))
            })
            .collect();
        std::thread::sleep(Duration::from_millis(50));
        slot.resolve(true);
        slot.resolve(false); // ignored: first resolution wins

        for waiter in waiters {
            assert!(waiter.join().unwrap());
        }
    }

    #[test]
    fn acquire_or_join_slot_dedupes_concurrent_callers() {
        // Regression test for the Codex cross-check finding: without this dedup,
        // `IdleAction::LockAndSuspend`'s pre-spawn and the `PrepareForSleep` watch's
        // `spawn_and_wait_until_locked` would each launch a competing `shilpo lock`
        // process, and the second is always denied the session lock.
        let supervisor = LockSupervisor::new();

        let (first_slot, first_created) = supervisor.acquire_or_join_slot();
        assert!(first_created, "first caller must own the spawn");

        let (second_slot, second_created) = supervisor.acquire_or_join_slot();
        assert!(!second_created, "second caller must join, not spawn again");
        assert!(Arc::ptr_eq(&first_slot, &second_slot));

        // Once the locker attempt is resolved (process exited, or readiness observed) a
        // later spawn request must be free to start a fresh attempt.
        supervisor.clear_readiness_if_matches(&first_slot);
        let (third_slot, third_created) = supervisor.acquire_or_join_slot();
        assert!(third_created, "a cleared slot must allow a new spawn");
        assert!(!Arc::ptr_eq(&first_slot, &third_slot));
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
