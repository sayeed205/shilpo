use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use tokio::sync::{mpsc, watch};

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
    /// Single-consumer worker channel so rapid toggles apply to the logind fd in the order
    /// they were requested, rather than racing across independently spawned tasks.
    op_tx: OnceLock<mpsc::UnboundedSender<bool>>,
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
            op_tx: OnceLock::new(),
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

        // In-process logind inhibit management, serialized through a single worker so
        // rapid toggles apply in request order rather than racing on D-Bus latency.
        if let Some(op_tx) = self.ensure_worker() {
            let _ = op_tx.send(active);
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

    /// Lazily starts the single worker task that applies logind inhibit changes in order.
    /// Returns `None` if no Tokio runtime is available yet (e.g. constructed outside an
    /// async context); a later call from within a runtime will still succeed.
    fn ensure_worker(&self) -> Option<&mpsc::UnboundedSender<bool>> {
        if let Some(tx) = self.op_tx.get() {
            return Some(tx);
        }
        let handle = tokio::runtime::Handle::try_current().ok()?;
        let (op_tx, mut op_rx) = mpsc::unbounded_channel::<bool>();
        let holder = self.holder.clone();
        handle.spawn(async move {
            while let Some(active) = op_rx.recv().await {
                let _ = holder.set_active(active).await;
            }
        });
        let _ = self.op_tx.set(op_tx);
        self.op_tx.get()
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
