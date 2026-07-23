use anyhow::Result;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use zbus::{Connection, interface};

/// Represents a single desktop notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub summary: String,
    pub body: String,
    pub app_icon: Option<String>,
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
            timestamp: chrono::Local::now(),
        }
    }
}

/// Dynamic Notification Daemon Service implementing org.freedesktop.Notifications.
pub struct NotificationService {
    notifications: Arc<Mutex<Vec<Notification>>>,
    new_notif_sender: Arc<Mutex<Option<std::sync::mpsc::Sender<Notification>>>>,
    _connection: Option<Connection>,
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
            new_notif_sender: Arc::new(Mutex::new(None)),
            _connection: None,
        }
    }

    /// Registers the notification daemon on an existing zbus Connection.
    pub async fn new_with_connection(connection: Connection) -> Result<Self> {
        let notifications = Arc::new(Mutex::new(Vec::new()));
        let next_id = Arc::new(Mutex::new(0));
        let new_notif_sender = Arc::new(Mutex::new(None));

        let service = Self {
            notifications: notifications.clone(),
            new_notif_sender: new_notif_sender.clone(),
            _connection: Some(connection.clone()),
        };

        let server = NotificationServer {
            notifications,
            next_id,
            new_notif_sender,
        };

        connection
            .object_server()
            .at("/org/freedesktop/Notifications", server)
            .await?;

        connection
            .request_name("org.freedesktop.Notifications")
            .await?;

        Ok(service)
    }

    /// Returns true if this service is connected to a live D-Bus session.
    pub fn is_dbus_connected(&self) -> bool {
        self._connection.is_some()
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
        let mut lock = self.notifications.lock().unwrap();
        lock.retain(|n| n.id != id);
    }

    /// Clears all active notifications.
    pub fn dismiss_all(&self) {
        let mut lock = self.notifications.lock().unwrap();
        lock.clear();
    }
}

struct NotificationServer {
    notifications: Arc<Mutex<Vec<Notification>>>,
    next_id: Arc<Mutex<u32>>,
    new_notif_sender: Arc<Mutex<Option<std::sync::mpsc::Sender<Notification>>>>,
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
        _actions: Vec<String>,
        _hints: HashMap<String, zbus::zvariant::Value<'_>>,
        _expire_timeout: i32,
    ) -> u32 {
        let mut id_lock = self.next_id.lock().unwrap();
        let id = if replaces_id == 0 {
            *id_lock += 1;
            *id_lock
        } else {
            replaces_id
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

        // Notify subscribers/OSD listeners
        if let Some(tx) = &*self.new_notif_sender.lock().unwrap() {
            let _ = tx.send(notification);
        }

        id
    }

    fn close_notification(&self, id: u32) {
        let mut list = self.notifications.lock().unwrap();
        list.retain(|n| n.id != id);
    }

    fn get_capabilities(&self) -> Vec<String> {
        vec!["body".to_string()]
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "shilpo-notification-daemon".to_string(),
            "Shilpo".to_string(),
            "0.1.0".to_string(),
            "1.2".to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_notification_creation_and_dismiss() {
        let service = NotificationService::new_offline();
        assert!(!service.is_dbus_connected());

        let (tx, rx) = std::sync::mpsc::channel();
        service.set_new_notification_sender(tx);

        let server = NotificationServer {
            notifications: service.notifications.clone(),
            next_id: Arc::new(Mutex::new(0)),
            new_notif_sender: service.new_notif_sender.clone(),
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

        // Verify receiver
        let received = rx.recv().unwrap();
        assert_eq!(received.id, 1);

        service.dismiss(1);
        assert!(service.notifications().is_empty());
    }

    #[test]
    fn test_notification_dismiss_all() {
        let service = NotificationService::new_offline();
        let server = NotificationServer {
            notifications: service.notifications.clone(),
            next_id: Arc::new(Mutex::new(0)),
            new_notif_sender: service.new_notif_sender.clone(),
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
            next_id: Arc::new(Mutex::new(0)),
            new_notif_sender: service.new_notif_sender.clone(),
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
            next_id: Arc::new(Mutex::new(0)),
            new_notif_sender: service.new_notif_sender.clone(),
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
            next_id: Arc::new(Mutex::new(0)),
            new_notif_sender: service.new_notif_sender.clone(),
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
            next_id: Arc::new(Mutex::new(0)),
            new_notif_sender: service.new_notif_sender.clone(),
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
}
