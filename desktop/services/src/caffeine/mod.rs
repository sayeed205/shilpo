use std::process::{Child, Command};
use std::sync::{Arc, Mutex};

use tokio::sync::watch;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CaffeineInfo {
    pub active: bool,
}

pub struct CaffeineService {
    tx: watch::Sender<CaffeineInfo>,
    process: Arc<Mutex<Option<Child>>>,
}

impl Default for CaffeineService {
    fn default() -> Self {
        Self::new()
    }
}

impl CaffeineService {
    pub fn new() -> Self {
        let (tx, _) = watch::channel(CaffeineInfo { active: false });
        let service = Self {
            tx,
            process: Arc::new(Mutex::new(None)),
        };

        let state_path = shilpo_config::caffeine_state_path();
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
        let current_active = self.tx.borrow().active;
        let mut proc_lock = self.process.lock().unwrap();

        if active == current_active {
            return current_active;
        }

        let new_active = if active {
            if let Ok(child) = Command::new("systemd-inhibit")
                .args([
                    "--what=idle:sleep:handle-lid-switch",
                    "--who=Shilpo",
                    "--why=Caffeine active",
                    "sleep",
                    "infinity",
                ])
                .spawn()
            {
                *proc_lock = Some(child);
                true
            } else {
                tracing::warn!("systemd-inhibit unavailable, caffeine active state fallback");
                true
            }
        } else {
            if let Some(mut child) = proc_lock.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
            false
        };

        let _ = self.tx.send_replace(CaffeineInfo { active: new_active });

        let state_path = shilpo_config::caffeine_state_path();
        if let Some(parent) = state_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&state_path, if new_active { "true" } else { "false" });

        new_active
    }

    pub fn toggle(&self) -> bool {
        let current = self.is_active();
        self.set_active(!current)
    }
}

impl Drop for CaffeineService {
    fn drop(&mut self) {
        if let Ok(mut proc_lock) = self.process.lock()
            && let Some(mut child) = proc_lock.take()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_caffeine_service_toggle() {
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
