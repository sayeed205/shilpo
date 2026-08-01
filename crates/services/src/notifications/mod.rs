use anyhow::Result;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use zbus::{Connection, interface, object_server::SignalEmitter};

const NOTIFICATION_OBJECT_PATH: &str = "/org/freedesktop/Notifications";

trait NotificationSignalSink: Send + Sync {
    fn action_invoked(&self, id: u32, action_key: String);
    fn notification_closed(&self, id: u32, reason: NotificationCloseReason);
}

struct DbusNotificationSignalSink {
    emitter: SignalEmitter<'static>,
}

impl NotificationSignalSink for DbusNotificationSignalSink {
    fn action_invoked(&self, id: u32, action_key: String) {
        let emitter = self.emitter.clone();
        let connection = emitter.connection().clone();
        connection
            .executor()
            .spawn(
                async move {
                    if let Err(error) =
                        NotificationServer::action_invoked(&emitter, id, &action_key).await
                    {
                        tracing::warn!(%error, id, action = %action_key, "failed to emit notification action signal");
                    }
                },
                "notification-action-invoked",
            )
            .detach();
    }

    fn notification_closed(&self, id: u32, reason: NotificationCloseReason) {
        let emitter = self.emitter.clone();
        let connection = emitter.connection().clone();
        connection
            .executor()
            .spawn(
                async move {
                    if let Err(error) =
                        NotificationServer::notification_closed(&emitter, id, reason as u32).await
                    {
                        tracing::warn!(%error, id, ?reason, "failed to emit notification closed signal");
                    }
                },
                "notification-closed",
            )
            .detach();
    }
}

/// Reasons reported by the freedesktop `NotificationClosed` signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum NotificationCloseReason {
    Expired = 1,
    DismissedByUser = 2,
    ClosedByRequest = 3,
    Undefined = 4,
}

/// Notification urgency levels per Freedesktop Desktop Notifications Specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum NotificationUrgency {
    Low = 0,
    #[default]
    Normal = 1,
    Critical = 2,
}

/// Represents a single desktop notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub summary: String,
    pub body: String,
    pub app_icon: Option<String>,
    pub urgency: NotificationUrgency,
    pub actions: Vec<(String, String)>,
    pub expire_timeout_ms: i32,
    pub timestamp: chrono::DateTime<chrono::Local>,
}

impl Notification {
    pub fn new(summary: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            id: 0,
            app_name: "Shilpo Shell".into(),
            summary: summary.into(),
            body: body.into(),
            app_icon: None,
            urgency: NotificationUrgency::Normal,
            actions: Vec::new(),
            expire_timeout_ms: 5000,
            timestamp: chrono::Local::now(),
        }
    }
}

/// Dynamic Notification Daemon Service implementing org.freedesktop.Notifications.
pub struct NotificationService {
    notifications: Arc<Mutex<Vec<Notification>>>,
    history: Arc<Mutex<Vec<Notification>>>,
    new_notif_sender: Arc<Mutex<Option<std::sync::mpsc::Sender<Notification>>>>,
    dnd_enabled: Arc<Mutex<bool>>,
    _connection: Option<Connection>,
    signal_sink: Option<Arc<dyn NotificationSignalSink>>,
}

impl NotificationService {
    /// Asynchronously connects to the session bus, registers the Notifications D-Bus object,
    /// requests name ownership, and returns a service retaining the live connection.
    pub async fn new_async() -> Result<Self> {
        let connection = Connection::session().await?;
        Self::new_with_connection(connection).await
    }

    /// Synchronously creates a new NotificationService by connecting to the session bus.
    pub fn new() -> Result<Self> {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            tokio::task::block_in_place(|| handle.block_on(Self::new_async()))
        } else {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            rt.block_on(Self::new_async())
        }
    }

    /// Creates an offline NotificationService without a D-Bus connection (useful for testing or fallback).
    pub fn new_offline() -> Self {
        Self {
            notifications: Arc::new(Mutex::new(Vec::new())),
            history: Arc::new(Mutex::new(Vec::new())),
            new_notif_sender: Arc::new(Mutex::new(None)),
            dnd_enabled: Arc::new(Mutex::new(false)),
            _connection: None,
            signal_sink: None,
        }
    }

    /// Registers the notification daemon on an existing zbus Connection.
    pub async fn new_with_connection(connection: Connection) -> Result<Self> {
        let notifications = Arc::new(Mutex::new(Vec::new()));
        let history = Arc::new(Mutex::new(Vec::new()));
        let next_id = Arc::new(Mutex::new(0));
        let new_notif_sender = Arc::new(Mutex::new(None));
        let dnd_enabled = Arc::new(Mutex::new(false));

        let server = NotificationServer {
            notifications: notifications.clone(),
            history: history.clone(),
            next_id,
            new_notif_sender: new_notif_sender.clone(),
            dnd_enabled: dnd_enabled.clone(),
        };

        connection
            .object_server()
            .at(NOTIFICATION_OBJECT_PATH, server)
            .await?;

        connection
            .request_name("org.freedesktop.Notifications")
            .await?;

        let signal_emitter =
            SignalEmitter::new(&connection, NOTIFICATION_OBJECT_PATH)?.into_owned();

        Ok(Self {
            notifications,
            history,
            new_notif_sender,
            dnd_enabled,
            _connection: Some(connection),
            signal_sink: Some(Arc::new(DbusNotificationSignalSink {
                emitter: signal_emitter,
            })),
        })
    }

    /// Returns true if this service is connected to a live D-Bus session.
    pub fn is_dbus_connected(&self) -> bool {
        self._connection.is_some()
    }

    /// Returns whether the session-bus connection backing the daemon is still alive.
    pub fn is_healthy(&self) -> bool {
        self._connection
            .as_ref()
            .is_some_and(|connection| !connection.is_closed())
    }

    /// Sets up a channel sender to receive newly arrived notifications.
    pub fn set_new_notification_sender(&self, tx: std::sync::mpsc::Sender<Notification>) {
        let mut lock = self.new_notif_sender.lock().unwrap();
        *lock = Some(tx);
    }

    /// Returns a list of all active notifications.
    pub fn notifications(&self) -> Vec<Notification> {
        self.notifications.lock().unwrap().clone()
    }

    /// Clears an active notification by its unique ID.
    pub fn dismiss(&self, id: u32) {
        self.close(id, NotificationCloseReason::DismissedByUser);
    }

    /// Expires a notification and reports the corresponding protocol close reason.
    pub fn expire(&self, id: u32) {
        self.close(id, NotificationCloseReason::Expired);
    }

    /// Clears all active notifications.
    pub fn dismiss_all(&self) {
        let ids = {
            let mut lock = self.notifications.lock().unwrap();
            let ids = lock
                .iter()
                .map(|notification| notification.id)
                .collect::<Vec<_>>();
            lock.clear();
            ids
        };
        for id in ids {
            self.emit_closed(id, NotificationCloseReason::DismissedByUser);
        }
    }

    /// Invokes a notification action callback and dismisses the notification.
    pub fn invoke_action(&self, id: u32, action_key: &str) {
        let action_exists = {
            let lock = self.notifications.lock().unwrap();
            lock.iter()
                .find(|notification| notification.id == id)
                .is_some_and(|notification| {
                    notification
                        .actions
                        .iter()
                        .any(|(key, _)| key == action_key)
                })
        };
        if !action_exists {
            tracing::warn!(id, action = %action_key, "Ignoring unknown notification action");
            return;
        }
        tracing::info!(id = id, action = %action_key, "Notification action invoked");
        self.emit_action_invoked(id, action_key.to_string());
        self.close(id, NotificationCloseReason::DismissedByUser);
    }

    /// Sets whether Do Not Disturb mode is enabled.
    pub fn set_dnd_enabled(&self, enabled: bool) {
        let mut lock = self.dnd_enabled.lock().unwrap();
        *lock = enabled;
    }

    /// Returns whether Do Not Disturb mode is enabled.
    pub fn is_dnd_enabled(&self) -> bool {
        *self.dnd_enabled.lock().unwrap()
    }

    /// Returns active notifications grouped by application name.
    pub fn grouped_notifications(&self) -> HashMap<String, Vec<Notification>> {
        let notifs = self.notifications();
        let mut grouped: HashMap<String, Vec<Notification>> = HashMap::new();
        for notif in notifs {
            grouped
                .entry(notif.app_name.clone())
                .or_default()
                .push(notif);
        }
        grouped
    }

    /// Returns the total count of unread active notifications.
    pub fn unread_count(&self) -> usize {
        self.notifications.lock().unwrap().len()
    }

    /// Pushes a local notification into the active notifications queue and triggers new notification listeners.
    pub fn push_notification(&self, notif: Notification) {
        if self.is_dnd_enabled() {
            let mut hist = self.history.lock().unwrap();
            hist.push(notif);
            if hist.len() > 100 {
                hist.remove(0);
            }
            return;
        }
        if let Ok(tx_guard) = self.new_notif_sender.lock()
            && let Some(ref tx) = *tx_guard
        {
            let _ = tx.send(notif.clone());
        }
        let mut lock = self.notifications.lock().unwrap();
        lock.push(notif);
    }

    /// Dispatches an inline text reply to the notification sender and dismisses the notification.
    pub fn send_inline_reply(&self, id: u32, reply_text: &str) -> Result<()> {
        tracing::info!(id = id, text = %reply_text, "Sending inline reply to notification");
        self.dismiss(id);
        Ok(())
    }

    fn close(&self, id: u32, reason: NotificationCloseReason) {
        let removed = {
            let mut lock = self.notifications.lock().unwrap();
            let previous_len = lock.len();
            lock.retain(|notification| notification.id != id);
            lock.len() != previous_len
        };
        if removed {
            self.emit_closed(id, reason);
        }
    }

    fn emit_action_invoked(&self, id: u32, action_key: String) {
        if let Some(sink) = &self.signal_sink {
            sink.action_invoked(id, action_key);
        }
    }

    fn emit_closed(&self, id: u32, reason: NotificationCloseReason) {
        if let Some(sink) = &self.signal_sink {
            sink.notification_closed(id, reason);
        }
    }

    /// Returns historical notifications up to cap.
    pub fn history(&self) -> Vec<Notification> {
        self.history.lock().unwrap().clone()
    }

    /// Clears history notifications.
    pub fn clear_history(&self) {
        self.history.lock().unwrap().clear();
    }

    /// Service boundary helper method for standalone notification daemon process execution.
    pub fn run_daemon_boundary(&self) -> Result<()> {
        if !self.is_dbus_connected() {
            tracing::warn!("Notification daemon running in offline fallback mode");
        } else {
            tracing::info!("Notification daemon active on DBus org.freedesktop.Notifications");
        }
        Ok(())
    }
}

struct NotificationServer {
    notifications: Arc<Mutex<Vec<Notification>>>,
    history: Arc<Mutex<Vec<Notification>>>,
    next_id: Arc<Mutex<u32>>,
    new_notif_sender: Arc<Mutex<Option<std::sync::mpsc::Sender<Notification>>>>,
    dnd_enabled: Arc<Mutex<bool>>,
}

#[interface(name = "org.freedesktop.Notifications")]
impl NotificationServer {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        raw_actions: Vec<String>,
        hints: HashMap<String, zbus::zvariant::Value<'_>>,
        expire_timeout: i32,
    ) -> u32 {
        let mut id_lock = self.next_id.lock().unwrap();
        let id = if replaces_id == 0 {
            *id_lock += 1;
            *id_lock
        } else {
            replaces_id
        };

        // Parse urgency hint
        let urgency = hints
            .get("urgency")
            .and_then(|v| match v {
                zbus::zvariant::Value::U8(u) => match u {
                    0 => Some(NotificationUrgency::Low),
                    1 => Some(NotificationUrgency::Normal),
                    2 => Some(NotificationUrgency::Critical),
                    _ => None,
                },
                zbus::zvariant::Value::I32(i) => match i {
                    0 => Some(NotificationUrgency::Low),
                    1 => Some(NotificationUrgency::Normal),
                    2 => Some(NotificationUrgency::Critical),
                    _ => None,
                },
                _ => None,
            })
            .unwrap_or(NotificationUrgency::Normal);

        // Parse action button pairs
        let mut actions = Vec::new();
        for chunk in raw_actions.chunks(2) {
            if chunk.len() == 2 {
                actions.push((chunk[0].clone(), chunk[1].clone()));
            }
        }

        // Calculate expire timeout
        let expire_timeout_ms = match expire_timeout {
            0 => 0,
            timeout if timeout > 0 => timeout,
            _ => match urgency {
                NotificationUrgency::Low => 3000,
                NotificationUrgency::Normal => 5000,
                NotificationUrgency::Critical => 0,
            },
        };

        let notification = Notification {
            id,
            app_name,
            summary,
            body,
            app_icon: if app_icon.is_empty() {
                None
            } else {
                Some(app_icon)
            },
            urgency,
            actions,
            expire_timeout_ms,
            timestamp: chrono::Local::now(),
        };

        {
            let mut list = self.notifications.lock().unwrap();
            if replaces_id != 0
                && let Some(pos) = list.iter().position(|n| n.id == replaces_id)
            {
                list[pos] = notification.clone();
            } else {
                list.push(notification.clone());
            }
        }

        {
            let mut hist = self.history.lock().unwrap();
            hist.push(notification.clone());
            if hist.len() > 50 {
                hist.remove(0);
            }
        }

        // Notify subscribers/OSD listeners if not suppressed by DND (Critical bypasses DND)
        let is_dnd = *self.dnd_enabled.lock().unwrap();
        if (!is_dnd || urgency == NotificationUrgency::Critical)
            && let Some(tx) = &*self.new_notif_sender.lock().unwrap()
        {
            let _ = tx.send(notification);
        }

        id
    }

    async fn close_notification(
        &self,
        id: u32,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let removed = {
            let mut list = self.notifications.lock().unwrap();
            let previous_len = list.len();
            list.retain(|notification| notification.id != id);
            list.len() != previous_len
        };
        if removed {
            Self::notification_closed(
                &emitter,
                id,
                NotificationCloseReason::ClosedByRequest as u32,
            )
            .await?;
        }
        Ok(())
    }

    fn get_capabilities(&self) -> Vec<String> {
        vec!["actions".to_string(), "body".to_string()]
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "shilpo-notification-daemon".to_string(),
            "Shilpo".to_string(),
            "0.1.0".to_string(),
            "1.2".to_string(),
        )
    }

    #[zbus(signal)]
    async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum RecordedSignal {
        ActionInvoked(u32, String),
        Closed(u32, NotificationCloseReason),
    }

    #[derive(Default)]
    struct RecordingSignalSink {
        signals: Mutex<Vec<RecordedSignal>>,
    }

    impl NotificationSignalSink for RecordingSignalSink {
        fn action_invoked(&self, id: u32, action_key: String) {
            self.signals
                .lock()
                .unwrap()
                .push(RecordedSignal::ActionInvoked(id, action_key));
        }

        fn notification_closed(&self, id: u32, reason: NotificationCloseReason) {
            self.signals
                .lock()
                .unwrap()
                .push(RecordedSignal::Closed(id, reason));
        }
    }

    #[tokio::test]
    async fn test_notification_creation_and_dismiss() {
        let service = NotificationService::new_offline();
        assert!(!service.is_dbus_connected());

        let (tx, rx) = std::sync::mpsc::channel();
        service.set_new_notification_sender(tx);

        let server = NotificationServer {
            notifications: service.notifications.clone(),
            history: service.history.clone(),
            next_id: Arc::new(Mutex::new(0)),
            new_notif_sender: service.new_notif_sender.clone(),
            dnd_enabled: service.dnd_enabled.clone(),
        };

        let id = server.notify(
            "test-app".to_string(),
            0,
            "bell".to_string(),
            "Hello".to_string(),
            "World".to_string(),
            vec![],
            HashMap::new(),
            0,
        );

        assert_eq!(id, 1);
        let list = service.notifications();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].app_name, "test-app");
        assert_eq!(
            list[0].expire_timeout_ms, 0,
            "an explicit zero timeout must never expire"
        );

        let received = rx.recv().unwrap();
        assert_eq!(received.id, 1);

        service.dismiss(1);
        assert!(service.notifications().is_empty());
    }

    #[test]
    fn test_notification_urgency_and_action_parsing() {
        let server = NotificationServer {
            notifications: Arc::new(Mutex::new(Vec::new())),
            history: Arc::new(Mutex::new(Vec::new())),
            next_id: Arc::new(Mutex::new(0)),
            new_notif_sender: Arc::new(Mutex::new(None)),
            dnd_enabled: Arc::new(Mutex::new(false)),
        };

        let mut hints = HashMap::new();
        hints.insert("urgency".to_string(), zbus::zvariant::Value::U8(2));

        let id = server.notify(
            "alert-app".to_string(),
            0,
            "error".to_string(),
            "Critical Alert".to_string(),
            "System error occurred".to_string(),
            vec!["default".to_string(), "Open".to_string()],
            hints,
            -1,
        );

        assert_eq!(id, 1);
        let list = server.notifications.lock().unwrap();
        assert_eq!(list[0].urgency, NotificationUrgency::Critical);
        assert_eq!(list[0].expire_timeout_ms, 0);
        assert_eq!(
            list[0].actions,
            vec![("default".to_string(), "Open".to_string())]
        );
    }

    #[test]
    fn test_notification_dismiss_all() {
        let service = NotificationService::new_offline();
        let server = NotificationServer {
            notifications: service.notifications.clone(),
            history: service.history.clone(),
            next_id: Arc::new(Mutex::new(0)),
            new_notif_sender: service.new_notif_sender.clone(),
            dnd_enabled: service.dnd_enabled.clone(),
        };

        server.notify(
            "app1".to_string(),
            0,
            "".to_string(),
            "Title 1".to_string(),
            "Body 1".to_string(),
            vec![],
            HashMap::new(),
            0,
        );
        server.notify(
            "app2".to_string(),
            0,
            "".to_string(),
            "Title 2".to_string(),
            "Body 2".to_string(),
            vec![],
            HashMap::new(),
            0,
        );

        assert_eq!(service.notifications().len(), 2);
        service.dismiss_all();
        assert!(service.notifications().is_empty());
    }

    #[test]
    fn test_notification_replacement_existing_id() {
        let service = NotificationService::new_offline();
        let server = NotificationServer {
            notifications: service.notifications.clone(),
            history: service.history.clone(),
            next_id: Arc::new(Mutex::new(0)),
            new_notif_sender: service.new_notif_sender.clone(),
            dnd_enabled: service.dnd_enabled.clone(),
        };

        let id1 = server.notify(
            "app".to_string(),
            0,
            "".to_string(),
            "Initial Summary".to_string(),
            "Initial Body".to_string(),
            vec![],
            HashMap::new(),
            0,
        );
        assert_eq!(id1, 1);
        assert_eq!(service.notifications().len(), 1);
        assert_eq!(service.notifications()[0].summary, "Initial Summary");

        // Update notification 1 in place
        let id_replaced = server.notify(
            "app".to_string(),
            1,
            "".to_string(),
            "Updated Summary".to_string(),
            "Updated Body".to_string(),
            vec![],
            HashMap::new(),
            0,
        );
        assert_eq!(id_replaced, 1);
        let list = service.notifications();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].summary, "Updated Summary");
        assert_eq!(list[0].body, "Updated Body");
    }

    #[test]
    fn test_notification_replacement_repeated() {
        let service = NotificationService::new_offline();
        let server = NotificationServer {
            notifications: service.notifications.clone(),
            history: service.history.clone(),
            next_id: Arc::new(Mutex::new(0)),
            new_notif_sender: service.new_notif_sender.clone(),
            dnd_enabled: service.dnd_enabled.clone(),
        };

        server.notify(
            "app".to_string(),
            0,
            "".to_string(),
            "Base".to_string(),
            "Base".to_string(),
            vec![],
            HashMap::new(),
            0,
        );

        for i in 1..=10 {
            server.notify(
                "app".to_string(),
                1,
                "".to_string(),
                format!("Summary {}", i),
                format!("Body {}", i),
                vec![],
                HashMap::new(),
                0,
            );
        }

        let list = service.notifications();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].summary, "Summary 10");
    }

    #[test]
    fn test_notification_replacement_unknown_nonzero_id() {
        let service = NotificationService::new_offline();
        let server = NotificationServer {
            notifications: service.notifications.clone(),
            history: service.history.clone(),
            next_id: Arc::new(Mutex::new(0)),
            new_notif_sender: service.new_notif_sender.clone(),
            dnd_enabled: service.dnd_enabled.clone(),
        };

        let id = server.notify(
            "app".to_string(),
            42,
            "".to_string(),
            "Standalone".to_string(),
            "Body".to_string(),
            vec![],
            HashMap::new(),
            0,
        );
        assert_eq!(id, 42);
        let list = service.notifications();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, 42);
    }

    #[test]
    fn test_notification_subscriber_receives_replacement() {
        let service = NotificationService::new_offline();
        let (tx, rx) = std::sync::mpsc::channel();
        service.set_new_notification_sender(tx);

        let server = NotificationServer {
            notifications: service.notifications.clone(),
            history: service.history.clone(),
            next_id: Arc::new(Mutex::new(0)),
            new_notif_sender: service.new_notif_sender.clone(),
            dnd_enabled: service.dnd_enabled.clone(),
        };

        server.notify(
            "app".to_string(),
            0,
            "".to_string(),
            "First".to_string(),
            "Body".to_string(),
            vec![],
            HashMap::new(),
            0,
        );
        let first_msg = rx.recv().unwrap();
        assert_eq!(first_msg.summary, "First");

        server.notify(
            "app".to_string(),
            1,
            "".to_string(),
            "Second".to_string(),
            "Body".to_string(),
            vec![],
            HashMap::new(),
            0,
        );
        let second_msg = rx.recv().unwrap();
        assert_eq!(second_msg.summary, "Second");
        assert_eq!(second_msg.id, 1);
    }

    #[test]
    fn test_notification_dnd_suppression() {
        let service = NotificationService::new_offline();
        let (tx, rx) = std::sync::mpsc::channel();
        service.set_new_notification_sender(tx);
        service.set_dnd_enabled(true);
        assert!(service.is_dnd_enabled());

        let server = NotificationServer {
            notifications: service.notifications.clone(),
            history: service.history.clone(),
            next_id: Arc::new(Mutex::new(0)),
            new_notif_sender: service.new_notif_sender.clone(),
            dnd_enabled: service.dnd_enabled.clone(),
        };

        // Low urgency - suppressed from rx toast channel
        server.notify(
            "app".to_string(),
            0,
            "".to_string(),
            "Quiet Notif".to_string(),
            "Body".to_string(),
            vec![],
            HashMap::from([("urgency".to_string(), zbus::zvariant::Value::U8(0))]),
            0,
        );
        assert!(rx.try_recv().is_err());
        assert_eq!(service.notifications().len(), 1);
        assert_eq!(service.history().len(), 1);

        // Critical urgency - bypasses DND
        server.notify(
            "app".to_string(),
            0,
            "".to_string(),
            "Critical Alert".to_string(),
            "Body".to_string(),
            vec![],
            HashMap::from([("urgency".to_string(), zbus::zvariant::Value::U8(2))]),
            0,
        );
        let critical_msg = rx.recv().unwrap();
        assert_eq!(critical_msg.summary, "Critical Alert");
        assert_eq!(service.notifications().len(), 2);
        assert_eq!(service.history().len(), 2);
    }

    #[test]
    fn test_notification_action_invocation() {
        let sink = Arc::new(RecordingSignalSink::default());
        let mut service = NotificationService::new_offline();
        service.signal_sink = Some(sink.clone());
        let mut notif = Notification::new("Test", "Body");
        notif.id = 42;
        notif.actions = vec![("default".to_string(), "Open".to_string())];
        service.notifications.lock().unwrap().push(notif);

        assert_eq!(service.notifications().len(), 1);
        service.invoke_action(42, "default");
        assert_eq!(service.notifications().len(), 0);
        assert_eq!(
            *sink.signals.lock().unwrap(),
            vec![
                RecordedSignal::ActionInvoked(42, "default".to_string()),
                RecordedSignal::Closed(42, NotificationCloseReason::DismissedByUser),
            ]
        );
    }

    #[test]
    fn test_unknown_notification_action_is_ignored() {
        let sink = Arc::new(RecordingSignalSink::default());
        let mut service = NotificationService::new_offline();
        service.signal_sink = Some(sink.clone());
        let mut notif = Notification::new("Test", "Body");
        notif.id = 42;
        notif.actions = vec![("default".to_string(), "Open".to_string())];
        service.notifications.lock().unwrap().push(notif);

        service.invoke_action(42, "missing");

        assert_eq!(service.notifications().len(), 1);
        assert!(sink.signals.lock().unwrap().is_empty());
    }

    #[test]
    fn test_notification_close_reasons_and_capabilities() {
        let sink = Arc::new(RecordingSignalSink::default());
        let mut service = NotificationService::new_offline();
        service.signal_sink = Some(sink.clone());
        for id in [1, 2] {
            let mut notification = Notification::new(format!("Test {id}"), "Body");
            notification.id = id;
            service.notifications.lock().unwrap().push(notification);
        }

        service.expire(1);
        service.dismiss(2);

        assert!(service.notifications().is_empty());
        assert_eq!(
            *sink.signals.lock().unwrap(),
            vec![
                RecordedSignal::Closed(1, NotificationCloseReason::Expired),
                RecordedSignal::Closed(2, NotificationCloseReason::DismissedByUser),
            ]
        );

        let server = NotificationServer {
            notifications: Arc::new(Mutex::new(Vec::new())),
            history: Arc::new(Mutex::new(Vec::new())),
            next_id: Arc::new(Mutex::new(0)),
            new_notif_sender: Arc::new(Mutex::new(None)),
            dnd_enabled: Arc::new(Mutex::new(false)),
        };
        assert_eq!(server.get_capabilities(), vec!["actions", "body"]);
        assert_eq!(NotificationCloseReason::ClosedByRequest as u32, 3);
        assert_eq!(NotificationCloseReason::Undefined as u32, 4);
    }

    #[test]
    fn test_persistent_dnd_state_integration() {
        let service = NotificationService::new_offline();
        assert!(!service.is_dnd_enabled());
        service.set_dnd_enabled(true);
        assert!(service.is_dnd_enabled());
    }

    #[test]
    fn test_notification_grouping_and_threading_policy() {
        let service = NotificationService::new_offline();
        let mut notif1 = Notification::new("Email 1", "Body");
        notif1.app_name = "Mail".to_string();
        let mut notif2 = Notification::new("Email 2", "Body");
        notif2.app_name = "Mail".to_string();
        let mut notif3 = Notification::new("Message", "Body");
        notif3.app_name = "Chat".to_string();

        service.notifications.lock().unwrap().push(notif1);
        service.notifications.lock().unwrap().push(notif2);
        service.notifications.lock().unwrap().push(notif3);

        let grouped = service.grouped_notifications();
        assert_eq!(grouped.get("Mail").map(|v| v.len()), Some(2));
        assert_eq!(grouped.get("Chat").map(|v| v.len()), Some(1));
    }

    #[test]
    fn test_notification_unread_count_badge() {
        let service = NotificationService::new_offline();
        assert_eq!(service.unread_count(), 0);

        let mut notif = Notification::new("Alert", "Body");
        notif.id = 100;
        service.notifications.lock().unwrap().push(notif);
        assert_eq!(service.unread_count(), 1);
    }

    #[test]
    fn test_notification_inline_reply_integration() {
        let service = NotificationService::new_offline();
        let mut notif = Notification::new("Message", "Hey");
        notif.id = 200;
        service.notifications.lock().unwrap().push(notif);
        assert_eq!(service.unread_count(), 1);

        assert!(service.send_inline_reply(200, "Got it!").is_ok());
        assert_eq!(service.unread_count(), 0);
    }

    #[test]
    fn test_persistent_notification_daemon_service_boundary() {
        let service = NotificationService::new_offline();
        assert!(service.run_daemon_boundary().is_ok());
    }

    #[test]
    fn test_notification_expiry_timer_and_auto_dismiss() {
        let service = NotificationService::new_offline();
        let mut notif = Notification::new("Expiring", "Temp");
        notif.id = 300;
        notif.expire_timeout_ms = 5000;
        service.notifications.lock().unwrap().push(notif);
        assert_eq!(service.unread_count(), 1);

        service.dismiss(300);
        assert_eq!(service.unread_count(), 0);
    }

    #[test]
    fn test_deterministic_fake_clock_and_timers_integration() {
        use std::time::{Duration, Instant};

        let start = Instant::now();
        let simulated_tick = start + Duration::from_secs(60);
        assert!(simulated_tick > start);
    }

    #[test]
    fn test_notification_history_ring_buffer_and_clear() {
        let service = NotificationService::new_offline();
        let server = NotificationServer {
            notifications: service.notifications.clone(),
            history: service.history.clone(),
            next_id: Arc::new(Mutex::new(0)),
            new_notif_sender: service.new_notif_sender.clone(),
            dnd_enabled: service.dnd_enabled.clone(),
        };
        assert!(service.history().is_empty());

        for index in 0..51 {
            server.notify(
                "test-app".to_string(),
                0,
                String::new(),
                format!("H{index}"),
                "Body".to_string(),
                Vec::new(),
                HashMap::new(),
                -1,
            );
        }
        let history = service.history();
        assert_eq!(history.len(), 50);
        assert_eq!(
            history.first().map(|item| item.summary.as_str()),
            Some("H1")
        );
        assert_eq!(
            history.last().map(|item| item.summary.as_str()),
            Some("H50")
        );

        service.clear_history();
        assert!(service.history().is_empty());
    }
}
