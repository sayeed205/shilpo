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

impl WaylandIdleNotifier {
    pub fn new(event_tx: mpsc::UnboundedSender<IdleBackendEvent>) -> Result<Self, String> {
        let available = Arc::new(AtomicBool::new(false));
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();

        let available_clone = available.clone();
        let handle = thread::Builder::new()
            .name("shilpo-idle-wayland".into())
            .spawn(move || {
                Self::wayland_thread_main(cmd_rx, event_tx, available_clone, ready_tx);
            })
            .map_err(|e| format!("failed to spawn wayland idle thread: {e}"))?;

        // Wait for initial connection & global discovery
        let _ = ready_rx.recv_timeout(Duration::from_secs(2));

        Ok(Self {
            available,
            cmd_tx,
            _thread: Mutex::new(Some(handle)),
        })
    }

    fn wayland_thread_main(
        cmd_rx: std::sync::mpsc::Receiver<WaylandThreadCommand>,
        event_tx: mpsc::UnboundedSender<IdleBackendEvent>,
        available: Arc<AtomicBool>,
        ready_tx: std::sync::mpsc::Sender<()>,
    ) {
        let conn = match Connection::connect_to_env() {
            Ok(conn) => conn,
            Err(err) => {
                tracing::debug!(%err, "wayland connection unavailable for idle notifier");
                let _ = ready_tx.send(());
                return;
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
            event_tx,
        };

        // Roundtrip to bind globals
        let _ = event_queue.roundtrip(&mut state);
        let _ = event_queue.roundtrip(&mut state);

        let is_ready = state.notifier.is_some() && state.seat.is_some();
        available.store(is_ready, Ordering::SeqCst);
        let _ = ready_tx.send(());

        loop {
            // Process commands from caller
            while let Ok(cmd) = cmd_rx.try_recv() {
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
                                &qh,
                                NotificationUserData { id },
                            );
                            state.notifications.insert(id, notif);
                        }
                    }
                    WaylandThreadCommand::Unregister { id } => {
                        if let Some(notif) = state.notifications.remove(&id) {
                            notif.destroy();
                        }
                    }
                    WaylandThreadCommand::UnregisterAll => {
                        for (_, notif) in state.notifications.split_off(&0) {
                            notif.destroy();
                        }
                    }
                    WaylandThreadCommand::Shutdown => {
                        for (_, notif) in state.notifications.split_off(&0) {
                            notif.destroy();
                        }
                        return;
                    }
                }
            }

            // Dispatch Wayland events
            if let Err(e) = event_queue.dispatch_pending(&mut state) {
                tracing::warn!(%e, "idle wayland event dispatch error");
                break;
            }
            if let Err(e) = conn.flush() {
                tracing::debug!(%e, "idle wayland connection flush error");
                break;
            }

            // Short sleep between event poll iterations
            thread::sleep(Duration::from_millis(16));
        }

        available.store(false, Ordering::SeqCst);
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
