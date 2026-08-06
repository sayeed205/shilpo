use std::process::{Child, Command};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CaffeineInfo {
    pub active: bool,
}

pub struct CaffeineService {
    info: Arc<Mutex<CaffeineInfo>>,
    process: Arc<Mutex<Option<Child>>>,
}

impl Default for CaffeineService {
    fn default() -> Self {
        Self::new()
    }
}

impl CaffeineService {
    pub fn new() -> Self {
        let service = Self {
            info: Arc::new(Mutex::new(CaffeineInfo { active: false })),
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

    pub fn is_active(&self) -> bool {
        self.info.lock().unwrap().active
    }

    pub fn info(&self) -> CaffeineInfo {
        self.info.lock().unwrap().clone()
    }

    pub fn set_active(&self, active: bool) -> bool {
        let mut info_lock = self.info.lock().unwrap();
        let mut proc_lock = self.process.lock().unwrap();

        if active == info_lock.active {
            return info_lock.active;
        }

        if active {
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
                info_lock.active = true;
            } else {
                tracing::warn!("systemd-inhibit unavailable, caffeine active state fallback");
                info_lock.active = true;
            }
        } else {
            if let Some(mut child) = proc_lock.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
            info_lock.active = false;
        }

        let state_path = shilpo_config::caffeine_state_path();
        if let Some(parent) = state_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&state_path, if info_lock.active { "true" } else { "false" });

        info_lock.active
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
