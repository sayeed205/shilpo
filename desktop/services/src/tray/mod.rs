use std::sync::{Arc, Mutex};

use anyhow::Result;
use zbus::{Connection, interface};

/// Represents a menu item in a System Tray DBusMenu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayMenuItem {
    pub id: i32,
    pub label: String,
    pub enabled: bool,
    pub children: Vec<TrayMenuItem>,
}

/// Represents an active System Tray StatusNotifierItem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayItem {
    pub service: String,
    pub title: String,
    pub icon_name: Option<String>,
    pub status: String,
    pub badge_count: Option<u32>,
    pub menu_path: Option<String>,
    pub menu_items: Vec<TrayMenuItem>,
}

impl TrayItem {
    pub fn new(service: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            service: service.into(),
            title: title.into(),
            icon_name: None,
            status: "Active".into(),
            badge_count: None,
            menu_path: None,
            menu_items: Vec::new(),
        }
    }
}

use tokio::sync::watch;

/// System Tray Daemon implementing org.kde.StatusNotifierWatcher.
pub struct TrayService {
    items: Arc<Mutex<Vec<String>>>,
    tx: watch::Sender<Vec<TrayItem>>,
    _connection: Option<Connection>,
}

impl TrayService {
    pub async fn new_async() -> Result<Self> {
        let connection = Connection::session().await?;
        Self::new_with_connection(connection).await
    }

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

    pub fn new_offline() -> Self {
        let (tx, _) = watch::channel(Vec::new());
        Self {
            items: Arc::new(Mutex::new(Vec::new())),
            tx,
            _connection: None,
        }
    }

    pub async fn new_with_connection(connection: Connection) -> Result<Self> {
        let items = Arc::new(Mutex::new(Vec::new()));
        let (tx, _) = watch::channel(Vec::new());
        let service = Self {
            items: items.clone(),
            tx: tx.clone(),
            _connection: Some(connection.clone()),
        };

        let server = StatusNotifierWatcherServer { items, tx };

        connection
            .object_server()
            .at("/StatusNotifierWatcher", server)
            .await?;

        connection
            .request_name("org.kde.StatusNotifierWatcher")
            .await?;

        Ok(service)
    }

    pub fn subscribe(&self) -> watch::Receiver<Vec<TrayItem>> {
        self.tx.subscribe()
    }

    pub fn is_dbus_connected(&self) -> bool {
        self._connection.is_some()
    }

    pub fn items(&self) -> Vec<TrayItem> {
        let lock = self.items.lock().unwrap();
        lock.iter().map(|s| TrayItem::new(s, s)).collect()
    }
}

struct StatusNotifierWatcherServer {
    items: Arc<Mutex<Vec<String>>>,
    tx: watch::Sender<Vec<TrayItem>>,
}

#[interface(name = "org.kde.StatusNotifierWatcher")]
impl StatusNotifierWatcherServer {
    fn register_status_notifier_item(&self, service: String) {
        let mut lock = self.items.lock().unwrap();
        if !lock.contains(&service) {
            lock.push(service);
            let tray_items = lock.iter().map(|s| TrayItem::new(s, s)).collect();
            let _ = self.tx.send_replace(tray_items);
        }
    }

    fn register_status_notifier_host(&self, _service: String) {}

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.items.lock().unwrap().clone()
    }

    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tray_service_offline_creation() {
        let service = TrayService::new_offline();
        assert!(!service.is_dbus_connected());
        assert!(service.items().is_empty());
    }

    #[test]
    fn test_register_status_notifier_item() {
        let service = TrayService::new_offline();
        let server = StatusNotifierWatcherServer {
            items: service.items.clone(),
            tx: service.tx.clone(),
        };

        server.register_status_notifier_item("org.kde.StatusNotifierItem-1234-1".to_string());
        let items = service.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].service, "org.kde.StatusNotifierItem-1234-1");
    }

    #[test]
    fn test_tray_menu_item_badges_and_submenus() {
        let mut item = TrayItem::new("org.test.Tray", "Test Tray");
        item.badge_count = Some(5);
        item.menu_path = Some("/MenuBar".to_string());
        item.menu_items = vec![TrayMenuItem {
            id: 1,
            label: "File".to_string(),
            enabled: true,
            children: vec![
                TrayMenuItem {
                    id: 2,
                    label: "Open".to_string(),
                    enabled: true,
                    children: Vec::new(),
                },
                TrayMenuItem {
                    id: 3,
                    label: "Save".to_string(),
                    enabled: false,
                    children: Vec::new(),
                },
            ],
        }];

        assert_eq!(item.badge_count, Some(5));
        assert_eq!(item.menu_items[0].children.len(), 2);
        assert!(!item.menu_items[0].children[1].enabled);
    }
}
