use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::sync::watch;

use crate::idle::LogindInhibitHolder;

pub fn caffeine_state_path() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from(".local/state"))
        .join("shilpo")
        .join("caffeine.state")
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CaffeineInfo {
    pub active: bool,
}

/// Facade over the Idle domain and in-process logind inhibit file descriptor.
pub struct CaffeineService {
    tx: watch::Sender<CaffeineInfo>,
    holder: Arc<LogindInhibitHolder>,
    _active_lock: Arc<Mutex<()>>,
}

impl Default for CaffeineService {
    fn default() -> Self {
        Self::new()
    }
}

impl CaffeineService {
    pub fn new() -> Self {
        Self::with_holder(Arc::new(LogindInhibitHolder::default()))
    }

    pub fn with_holder(holder: Arc<LogindInhibitHolder>) -> Self {
        let (tx, _) = watch::channel(CaffeineInfo { active: false });
        let service = Self {
            tx,
            holder,
            _active_lock: Arc::new(Mutex::new(())),
        };

        let state_path = caffeine_state_path();
        if let Ok(content) = std::fs::read_to_string(&state_path)
            && content.trim() == "true"
        {
            service.set_active(true);
        }

        service
    }

    pub fn subscribe(&self) -> watch::Receiver<CaffeineInfo> {
        self.tx.subscribe()
    }

    pub fn is_active(&self) -> bool {
        self.tx.borrow().active
    }

    pub fn info(&self) -> CaffeineInfo {
        self.tx.borrow().clone()
    }

    pub fn set_active(&self, active: bool) -> bool {
        let _guard = self._active_lock.lock().unwrap();
        let current_active = self.tx.borrow().active;

        if active == current_active {
            return current_active;
        }

        let holder = self.holder.clone();
        // In-process logind inhibit management
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = holder.set_active(active).await;
            });
        }

        let _ = self.tx.send_replace(CaffeineInfo { active });

        let state_path = caffeine_state_path();
        if let Some(parent) = state_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&state_path, if active { "true" } else { "false" });

        active
    }

    pub fn toggle(&self) -> bool {
        let current = self.is_active();
        self.set_active(!current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_caffeine_service_toggle() {
        let _guard = crate::test_support::serial_guard();
        let service = CaffeineService::new();
        let initial = service.is_active();
        assert_eq!(service.toggle(), !initial);
        assert_eq!(service.is_active(), !initial);
        assert_eq!(service.toggle(), initial);
        assert_eq!(service.is_active(), initial);
    }

    #[test]
    fn test_caffeine_state_persistence() {
        let _guard = crate::test_support::serial_guard();
        let service = CaffeineService::new();
        service.set_active(true);
        assert!(service.is_active());

        // A new instance should load the persisted true state
        let reloaded_service = CaffeineService::new();
        assert!(reloaded_service.is_active());

        // Cleanup state
        reloaded_service.set_active(false);
        let reset_service = CaffeineService::new();
        assert!(!reset_service.is_active());
    }
}
