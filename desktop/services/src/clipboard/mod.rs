use anyhow::Result;
use shilpo_config::{
    ClipboardItem, DEFAULT_CLIPBOARD_HISTORY_LIMIT, HeedSessionStore, SessionStoreError,
};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub enum ClipboardPersistenceError {
    Store(SessionStoreError),
    Disabled,
}

impl std::fmt::Display for ClipboardPersistenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(err) => write!(f, "clipboard persistence error: {err}"),
            Self::Disabled => write!(f, "clipboard persistence disabled"),
        }
    }
}

impl std::error::Error for ClipboardPersistenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Store(err) => Some(err),
            Self::Disabled => None,
        }
    }
}

impl From<SessionStoreError> for ClipboardPersistenceError {
    fn from(err: SessionStoreError) -> Self {
        Self::Store(err)
    }
}

pub(crate) trait ClipboardStore: Send + Sync {
    fn history(&self, limit: usize) -> Result<Vec<ClipboardItem>, ClipboardPersistenceError>;
    fn record(&self, item: &ClipboardItem, limit: usize) -> Result<(), ClipboardPersistenceError>;
    fn clear(&self) -> Result<(), ClipboardPersistenceError>;
}

pub(crate) struct HeedClipboardStore(pub Arc<HeedSessionStore>);

impl ClipboardStore for HeedClipboardStore {
    fn history(&self, limit: usize) -> Result<Vec<ClipboardItem>, ClipboardPersistenceError> {
        self.0.clipboard_history(limit).map_err(Into::into)
    }

    fn record(&self, item: &ClipboardItem, limit: usize) -> Result<(), ClipboardPersistenceError> {
        self.0
            .record_clipboard_item(item, limit)
            .map_err(Into::into)
    }

    fn clear(&self) -> Result<(), ClipboardPersistenceError> {
        self.0.clear_clipboard_history().map_err(Into::into)
    }
}

use tokio::sync::watch;

/// Desktop Clipboard Service supporting persistent history via LMDB and arboard.
pub struct ClipboardService {
    history: Arc<Mutex<Vec<ClipboardItem>>>,
    tx: watch::Sender<Vec<ClipboardItem>>,
    store: Option<Arc<dyn ClipboardStore>>,
    last_error: Arc<Mutex<Option<String>>>,
}

impl ClipboardService {
    pub fn with_store(store: Option<Arc<HeedSessionStore>>) -> Self {
        let trait_store: Option<Arc<dyn ClipboardStore>> =
            store.map(|s| Arc::new(HeedClipboardStore(s)) as Arc<dyn ClipboardStore>);
        Self::with_custom_store(trait_store)
    }

    pub(crate) fn with_custom_store(store: Option<Arc<dyn ClipboardStore>>) -> Self {
        let mut initial_history = Vec::new();
        let last_error = Arc::new(Mutex::new(None));

        if let Some(ref store) = store {
            match store.history(DEFAULT_CLIPBOARD_HISTORY_LIMIT) {
                Ok(items) => initial_history = items,
                Err(err) => {
                    tracing::warn!(error = %err, "failed to load clipboard history from session store");
                    if let Ok(mut lock) = last_error.lock() {
                        *lock = Some(err.to_string());
                    }
                }
            }
        }

        let (tx, _) = watch::channel(initial_history.clone());
        let history = Arc::new(Mutex::new(initial_history));

        let service = Self {
            history: history.clone(),
            tx,
            store,
            last_error,
        };

        service.start_monitoring();
        service
    }

    pub fn subscribe(&self) -> watch::Receiver<Vec<ClipboardItem>> {
        self.tx.subscribe()
    }

    pub fn last_error(&self) -> Option<String> {
        self.last_error.lock().unwrap().clone()
    }

    fn start_monitoring(&self) {
        let history = self.history.clone();
        let tx = self.tx.clone();
        let store = self.store.clone();
        let last_error = self.last_error.clone();

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
                            if lock.len() > DEFAULT_CLIPBOARD_HISTORY_LIMIT {
                                lock.pop();
                            }
                            let _ = tx.send_replace(lock.clone());

                            if let Some(ref store) = store
                                && let Err(err) =
                                    store.record(&item, DEFAULT_CLIPBOARD_HISTORY_LIMIT)
                            {
                                tracing::warn!(error = %err, "failed to persist clipboard item");
                                if let Ok(mut err_lock) = last_error.lock() {
                                    *err_lock = Some(err.to_string());
                                }
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

    pub fn copy_image(&self, image: &image::RgbaImage) -> Result<()> {
        let mut board = arboard::Clipboard::new()?;
        let width = image.width() as usize;
        let height = image.height() as usize;
        let bytes = image.as_raw();
        let img_data = arboard::ImageData {
            width,
            height,
            bytes: std::borrow::Cow::Borrowed(bytes),
        };
        board.set_image(img_data)?;
        Ok(())
    }

    pub fn clear_history(&self) -> Result<(), ClipboardPersistenceError> {
        if let Some(ref store) = self.store {
            store.clear()?;
        }
        let mut lock = self.history.lock().unwrap();
        lock.clear();
        let _ = self.tx.send_replace(Vec::new());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockStore {
        history_result: Mutex<Result<Vec<ClipboardItem>, ClipboardPersistenceError>>,
        record_result: Mutex<Result<(), ClipboardPersistenceError>>,
        clear_result: Mutex<Result<(), ClipboardPersistenceError>>,
        records: Mutex<Vec<ClipboardItem>>,
    }

    impl MockStore {
        fn new() -> Self {
            Self {
                history_result: Mutex::new(Ok(Vec::new())),
                record_result: Mutex::new(Ok(())),
                clear_result: Mutex::new(Ok(())),
                records: Mutex::new(Vec::new()),
            }
        }
    }

    impl ClipboardStore for MockStore {
        fn history(&self, _limit: usize) -> Result<Vec<ClipboardItem>, ClipboardPersistenceError> {
            self.history_result.lock().unwrap().clone()
        }

        fn record(
            &self,
            item: &ClipboardItem,
            _limit: usize,
        ) -> Result<(), ClipboardPersistenceError> {
            let res = self.record_result.lock().unwrap().clone();
            if res.is_ok() {
                self.records.lock().unwrap().push(item.clone());
            }
            res
        }

        fn clear(&self) -> Result<(), ClipboardPersistenceError> {
            let res = self.clear_result.lock().unwrap().clone();
            if res.is_ok() {
                self.records.lock().unwrap().clear();
            }
            res
        }
    }

    impl Clone for ClipboardPersistenceError {
        fn clone(&self) -> Self {
            match self {
                Self::Disabled => Self::Disabled,
                Self::Store(_) => Self::Disabled,
            }
        }
    }

    #[test]
    fn test_clipboard_service_offline() {
        let service = ClipboardService::with_custom_store(None);
        let history = service.history();
        assert!(history.is_empty());
    }

    #[test]
    fn test_clipboard_clear_failure_preserves_memory() {
        let mock = Arc::new(MockStore::new());
        *mock.history_result.lock().unwrap() = Ok(vec![ClipboardItem {
            id: 1,
            text: "secret".into(),
            timestamp: "12:00:00".into(),
        }]);
        *mock.clear_result.lock().unwrap() = Err(ClipboardPersistenceError::Disabled);

        let service = ClipboardService::with_custom_store(Some(mock));
        assert_eq!(service.history().len(), 1);

        assert!(service.clear_history().is_err());
        assert_eq!(service.history().len(), 1);
    }

    #[test]
    fn test_clipboard_clear_success_updates_both() {
        let mock = Arc::new(MockStore::new());
        *mock.history_result.lock().unwrap() = Ok(vec![ClipboardItem {
            id: 1,
            text: "secret".into(),
            timestamp: "12:00:00".into(),
        }]);

        let service = ClipboardService::with_custom_store(Some(mock));
        assert_eq!(service.history().len(), 1);

        assert!(service.clear_history().is_ok());
        assert!(service.history().is_empty());
    }
}
