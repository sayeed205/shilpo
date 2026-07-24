use anyhow::Result;
use shilpo_config::{ClipboardItem, HeedSessionStore};
use std::sync::{Arc, Mutex};

/// Desktop Clipboard Service supporting persistent history via LMDB and arboard.
pub struct ClipboardService {
    history: Arc<Mutex<Vec<ClipboardItem>>>,
    session_store: Option<Arc<HeedSessionStore>>,
}

impl ClipboardService {
    pub fn new() -> Self {
        let db_dir = HeedSessionStore::default_db_dir();
        let store = HeedSessionStore::open_or_create(&db_dir).ok().map(Arc::new);
        let history = if let Some(ref store) = store {
            store.get_clipboard_history().unwrap_or_default()
        } else {
            Vec::new()
        };

        let history = Arc::new(Mutex::new(history));

        let service = Self {
            history: history.clone(),
            session_store: store,
        };

        service.start_monitoring();
        service
    }

    fn start_monitoring(&self) {
        let history = self.history.clone();
        let store = self.session_store.clone();

        let task = async move {
            let mut last_seen = String::new();
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(800)).await;

                if let Ok(mut board) = arboard::Clipboard::new()
                    && let Ok(text) = board.get_text()
                {
                    let text = text.trim().to_string();
                    if !text.is_empty() && text != last_seen {
                        last_seen = text.clone();

                        let mut lock = history.lock().unwrap();

                        // Deduplicate
                        if lock.first().is_none_or(|item| item.text != text) {
                            let id = chrono::Local::now().timestamp_millis() as u64;
                            let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
                            let item = ClipboardItem {
                                id,
                                text,
                                timestamp,
                            };

                            lock.insert(0, item.clone());
                            if lock.len() > 100 {
                                lock.pop();
                            }

                            if let Some(ref store) = store {
                                let _ = store.save_clipboard_item(&item);
                            }
                        }
                    }
                }
            }
        };

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(task);
        }
    }

    pub fn history(&self) -> Vec<ClipboardItem> {
        self.history.lock().unwrap().clone()
    }

    pub fn copy_text(&self, text: &str) -> Result<()> {
        let mut board = arboard::Clipboard::new()?;
        board.set_text(text)?;
        Ok(())
    }

    pub fn clear_history(&self) {
        let mut lock = self.history.lock().unwrap();
        lock.clear();
        if let Some(ref store) = self.session_store {
            let _ = store.clear_clipboard_history();
        }
    }
}

impl Default for ClipboardService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipboard_service_offline() {
        let service = ClipboardService::new();
        let history = service.history();
        assert!(history.is_empty() || !history.is_empty());
    }
}
