use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tokio::sync::mpsc;
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_seat::{self, WlSeat};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle};
use wayland_protocols::ext::idle_notify::v1::client::ext_idle_notification_v1::{
    self, ExtIdleNotificationV1,
};
use wayland_protocols::ext::idle_notify::v1::client::ext_idle_notifier_v1::{
    self, ExtIdleNotifierV1,
};

/// Event delivered by an idle notifier backend when state changes for a registered notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleBackendEvent {
    Idled { id: u32 },
    Resumed { id: u32 },
}

/// Abstract backend interface for ext-idle-notify-v1 interactions.
pub trait IdleNotifierBackend: Send + Sync {
    /// Returns whether the backend is connected and the ext_idle_notifier_v1 global is available.
    fn is_available(&self) -> bool;

    /// Registers a notification at timeout_ms for the given numerical identifier.
    fn register_notification(&self, id: u32, timeout_ms: u32) -> Result<(), String>;

    /// Unregisters the notification with the given identifier.
    fn unregister_notification(&self, id: u32) -> Result<(), String>;

    /// Drops all registered notifications.
    fn unregister_all(&self);
}

// ---------------------------------------------------------------------------
// Mock Idle Notifier (for Tests)
// ---------------------------------------------------------------------------

/// Pure in-memory mock implementation for contract and domain testing.
pub struct MockIdleNotifier {
    available: AtomicBool,
    registered: Arc<Mutex<BTreeMap<u32, u32>>>,
    event_tx: Arc<Mutex<Option<mpsc::UnboundedSender<IdleBackendEvent>>>>,
}

impl Default for MockIdleNotifier {
    fn default() -> Self {
        Self::new()
    }
}

impl MockIdleNotifier {
    pub fn new() -> Self {
        Self {
            available: AtomicBool::new(true),
            registered: Arc::new(Mutex::new(BTreeMap::new())),
            event_tx: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_sender(event_tx: mpsc::UnboundedSender<IdleBackendEvent>) -> Self {
        Self {
            available: AtomicBool::new(true),
            registered: Arc::new(Mutex::new(BTreeMap::new())),
            event_tx: Arc::new(Mutex::new(Some(event_tx))),
        }
    }

    pub fn set_event_sender(&self, tx: mpsc::UnboundedSender<IdleBackendEvent>) {
        *self.event_tx.lock().unwrap() = Some(tx);
    }

    pub fn set_available(&self, available: bool) {
        self.available.store(available, Ordering::SeqCst);
    }

    pub fn registered_map(&self) -> BTreeMap<u32, u32> {
        self.registered.lock().unwrap().clone()
    }

    pub fn emit_idled(&self, id: u32) {
        if let Some(tx) = self.event_tx.lock().unwrap().as_ref() {
            let _ = tx.send(IdleBackendEvent::Idled { id });
        }
    }

    pub fn emit_resumed(&self, id: u32) {
        if let Some(tx) = self.event_tx.lock().unwrap().as_ref() {
            let _ = tx.send(IdleBackendEvent::Resumed { id });
        }
    }
}

impl IdleNotifierBackend for MockIdleNotifier {
    fn is_available(&self) -> bool {
        self.available.load(Ordering::SeqCst)
    }

    fn register_notification(&self, id: u32, timeout_ms: u32) -> Result<(), String> {
        if !self.is_available() {
            return Err("ext_idle_notifier_v1 global unavailable".into());
        }
        self.registered.lock().unwrap().insert(id, timeout_ms);
        Ok(())
    }

    fn unregister_notification(&self, id: u32) -> Result<(), String> {
        self.registered.lock().unwrap().remove(&id);
        Ok(())
    }

    fn unregister_all(&self) {
        self.registered.lock().unwrap().clear();
    }
}

// ---------------------------------------------------------------------------
// Real Wayland Idle Notifier (Dedicated Thread)
// ---------------------------------------------------------------------------

enum WaylandThreadCommand {
    Register { id: u32, timeout_ms: u32 },
    Unregister { id: u32 },
    UnregisterAll,
    Shutdown,
}

#[derive(Clone, Copy)]
struct NotificationUserData {
    id: u32,
}

struct WaylandIdleState {
    notifier: Option<ExtIdleNotifierV1>,
    seat: Option<WlSeat>,
    notifications: BTreeMap<u32, ExtIdleNotificationV1>,
    event_tx: mpsc::UnboundedSender<IdleBackendEvent>,
}

pub struct WaylandIdleNotifier {
    available: Arc<AtomicBool>,
    cmd_tx: std::sync::mpsc::Sender<WaylandThreadCommand>,
    _thread: Mutex<Option<JoinHandle<()>>>,
}

/// Interval the dedicated thread blocks on its command channel between Wayland dispatch
/// passes. Idle timeouts are minutes-scale, so this bounds command/event latency generously
/// without busy-polling.
const DISPATCH_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Delay between reconnect attempts after the Wayland connection is lost or never came up.
const RECONNECT_BACKOFF: Duration = Duration::from_secs(3);

impl WaylandIdleNotifier {
    pub fn new(event_tx: mpsc::UnboundedSender<IdleBackendEvent>) -> Result<Self, String> {
        let available = Arc::new(AtomicBool::new(false));
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();

        let available_clone = available.clone();
        let handle = thread::Builder::new()
            .name("shilpo-idle-wayland".into())
            .spawn(move || {
                Self::wayland_thread_main(cmd_rx, event_tx, available_clone);
            })
            .map_err(|e| format!("failed to spawn wayland idle thread: {e}"))?;

        // Non-blocking: the thread publishes `available` once it has connected and
        // discovered globals. Callers observe `Unavailable` until then, matching the
        // dedicated-thread backend precedent in `compositor/generic.rs`.
        Ok(Self {
            available,
            cmd_tx,
            _thread: Mutex::new(Some(handle)),
        })
    }

    /// Applies one command to the live Wayland state. Returns `false` on `Shutdown`.
    fn apply_command(
        cmd: WaylandThreadCommand,
        state: &mut WaylandIdleState,
        qh: &QueueHandle<WaylandIdleState>,
    ) -> bool {
        match cmd {
            WaylandThreadCommand::Register { id, timeout_ms } => {
                if let Some(ref notifier) = state.notifier
                    && let Some(ref seat) = state.seat
                {
                    if let Some(old) = state.notifications.remove(&id) {
                        old.destroy();
                    }
                    let notif = notifier.get_idle_notification(
                        timeout_ms,
                        seat,
                        qh,
                        NotificationUserData { id },
                    );
                    state.notifications.insert(id, notif);
                }
                true
            }
            WaylandThreadCommand::Unregister { id } => {
                if let Some(notif) = state.notifications.remove(&id) {
                    notif.destroy();
                }
                true
            }
            WaylandThreadCommand::UnregisterAll => {
                for (_, notif) in state.notifications.split_off(&0) {
                    notif.destroy();
                }
                true
            }
            WaylandThreadCommand::Shutdown => {
                for (_, notif) in state.notifications.split_off(&0) {
                    notif.destroy();
                }
                false
            }
        }
    }

    /// Drains any commands buffered while disconnected. Returns `true` if a shutdown was seen.
    fn drain_shutdown(cmd_rx: &std::sync::mpsc::Receiver<WaylandThreadCommand>) -> bool {
        while let Ok(cmd) = cmd_rx.try_recv() {
            if matches!(cmd, WaylandThreadCommand::Shutdown) {
                return true;
            }
        }
        false
    }

    fn wayland_thread_main(
        cmd_rx: std::sync::mpsc::Receiver<WaylandThreadCommand>,
        event_tx: mpsc::UnboundedSender<IdleBackendEvent>,
        available: Arc<AtomicBool>,
    ) {
        loop {
            let conn = match Connection::connect_to_env() {
                Ok(conn) => conn,
                Err(err) => {
                    tracing::debug!(%err, "wayland connection unavailable for idle notifier");
                    available.store(false, Ordering::SeqCst);
                    match cmd_rx.recv_timeout(RECONNECT_BACKOFF) {
                        Ok(WaylandThreadCommand::Shutdown) => return,
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
                        _ => continue,
                    }
                }
            };

            let mut event_queue: EventQueue<WaylandIdleState> = conn.new_event_queue();
            let qh = event_queue.handle();

            let display = conn.display();
            let _registry = display.get_registry(&qh, ());

            let mut state = WaylandIdleState {
                notifier: None,
                seat: None,
                notifications: BTreeMap::new(),
                event_tx: event_tx.clone(),
            };

            // Roundtrip to bind globals
            let _ = event_queue.roundtrip(&mut state);
            let _ = event_queue.roundtrip(&mut state);

            let is_ready = state.notifier.is_some() && state.seat.is_some();
            available.store(is_ready, Ordering::SeqCst);

            let mut lost_connection = true;
            'dispatch: loop {
                // Block on the command channel rather than busy-polling; idle timeouts are
                // minutes-scale so bounded command/event latency here is not user-visible.
                match cmd_rx.recv_timeout(DISPATCH_POLL_INTERVAL) {
                    Ok(cmd) => {
                        if !Self::apply_command(cmd, &mut state, &qh) {
                            lost_connection = false;
                            break 'dispatch;
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        lost_connection = false;
                        break 'dispatch;
                    }
                }
                // Drain any remaining buffered commands without waiting again.
                while let Ok(cmd) = cmd_rx.try_recv() {
                    if !Self::apply_command(cmd, &mut state, &qh) {
                        lost_connection = false;
                        break 'dispatch;
                    }
                }

                // Dispatch Wayland events
                if let Err(e) = event_queue.dispatch_pending(&mut state) {
                    tracing::warn!(%e, "idle wayland event dispatch error; reconnecting");
                    break 'dispatch;
                }
                if let Err(e) = conn.flush() {
                    tracing::debug!(%e, "idle wayland connection flush error; reconnecting");
                    break 'dispatch;
                }
            }

            available.store(false, Ordering::SeqCst);
            if !lost_connection {
                return;
            }
            if Self::drain_shutdown(&cmd_rx) {
                return;
            }
            thread::sleep(RECONNECT_BACKOFF);
        }
    }
}

impl IdleNotifierBackend for WaylandIdleNotifier {
    fn is_available(&self) -> bool {
        self.available.load(Ordering::SeqCst)
    }

    fn register_notification(&self, id: u32, timeout_ms: u32) -> Result<(), String> {
        if !self.is_available() {
            return Err("ext_idle_notifier_v1 global unavailable".into());
        }
        self.cmd_tx
            .send(WaylandThreadCommand::Register { id, timeout_ms })
            .map_err(|e| e.to_string())
    }

    fn unregister_notification(&self, id: u32) -> Result<(), String> {
        self.cmd_tx
            .send(WaylandThreadCommand::Unregister { id })
            .map_err(|e| e.to_string())
    }

    fn unregister_all(&self) {
        let _ = self.cmd_tx.send(WaylandThreadCommand::UnregisterAll);
    }
}

impl Drop for WaylandIdleNotifier {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(WaylandThreadCommand::Shutdown);
        if let Ok(mut guard) = self._thread.lock()
            && let Some(handle) = guard.take()
        {
            let _ = handle.join();
        }
    }
}

// ---------------------------------------------------------------------------
// Wayland Client Dispatch Implementations
// ---------------------------------------------------------------------------

impl Dispatch<WlRegistry, ()> for WaylandIdleState {
    fn event(
        state: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "ext_idle_notifier_v1" => {
                    let notifier =
                        registry.bind::<ExtIdleNotifierV1, _, _>(name, version.min(1), qh, ());
                    state.notifier = Some(notifier);
                }
                "wl_seat" => {
                    let seat = registry.bind::<WlSeat, _, _>(name, version.min(1), qh, ());
                    state.seat = Some(seat);
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<ExtIdleNotifierV1, ()> for WaylandIdleState {
    fn event(
        _state: &mut Self,
        _proxy: &ExtIdleNotifierV1,
        _event: ext_idle_notifier_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSeat, ()> for WaylandIdleState {
    fn event(
        _state: &mut Self,
        _proxy: &WlSeat,
        _event: wl_seat::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtIdleNotificationV1, NotificationUserData> for WaylandIdleState {
    fn event(
        state: &mut Self,
        _proxy: &ExtIdleNotificationV1,
        event: ext_idle_notification_v1::Event,
        data: &NotificationUserData,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            ext_idle_notification_v1::Event::Idled => {
                let _ = state.event_tx.send(IdleBackendEvent::Idled { id: data.id });
            }
            ext_idle_notification_v1::Event::Resumed => {
                let _ = state
                    .event_tx
                    .send(IdleBackendEvent::Resumed { id: data.id });
            }
            _ => {}
        }
    }
}
