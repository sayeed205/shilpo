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

use crate::runtime::{StateContext, StateRuntime};
use tokio::sync::watch;

/// Desktop Clipboard Service supporting persistent history via LMDB and Wayland ext-data-control.
#[derive(Clone)]
pub struct ClipboardService {
    runtime: StateRuntime<Vec<ClipboardItem>>,
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

        let store_clone = store.clone();
        let last_error_clone = last_error.clone();

        let runtime = StateRuntime::spawn(initial_history, move |ctx| async move {
            run_clipboard_monitoring(ctx, store_clone, last_error_clone).await;
        });

        Self {
            runtime,
            store,
            last_error,
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<Vec<ClipboardItem>> {
        self.runtime.subscribe()
    }

    pub fn last_error(&self) -> Option<String> {
        self.last_error.lock().unwrap().clone()
    }

    pub fn history(&self) -> Vec<ClipboardItem> {
        self.runtime.get()
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
        self.runtime.send_replace(Vec::new());
        Ok(())
    }
}

async fn run_clipboard_monitoring(
    ctx: StateContext<Vec<ClipboardItem>>,
    store: Option<Arc<dyn ClipboardStore>>,
    last_error: Arc<Mutex<Option<String>>>,
) {
    #[cfg(target_os = "linux")]
    {
        let res =
            tokio::task::spawn_blocking(move || wayland_data_control_loop(ctx, store, last_error))
                .await;
        if let Err(err) = res {
            tracing::debug!("Wayland data-control thread exited: {err}");
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        if let Ok(mut lock) = last_error.lock() {
            *lock = Some("Wayland ext-data-control protocol unavailable".to_string());
        }
    }
}

#[cfg(target_os = "linux")]
fn wayland_data_control_loop(
    ctx: StateContext<Vec<ClipboardItem>>,
    store: Option<Arc<dyn ClipboardStore>>,
    last_error: Arc<Mutex<Option<String>>>,
) {
    use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, protocol::wl_seat};
    use wayland_protocols_wlr::data_control::v1::client::{
        zwlr_data_control_device_v1::{self, ZwlrDataControlDeviceV1},
        zwlr_data_control_manager_v1::{self, ZwlrDataControlManagerV1},
        zwlr_data_control_offer_v1::{self, ZwlrDataControlOfferV1},
    };

    struct AppState {
        manager: Option<ZwlrDataControlManagerV1>,
        seat: Option<wl_seat::WlSeat>,
        device: Option<ZwlrDataControlDeviceV1>,
        current_offer: Option<ZwlrDataControlOfferV1>,
        offer_mime_types: Vec<String>,
        ctx: StateContext<Vec<ClipboardItem>>,
        store: Option<Arc<dyn ClipboardStore>>,
        #[allow(dead_code)]
        last_error: Arc<Mutex<Option<String>>>,
    }

    impl Dispatch<wayland_client::protocol::wl_registry::WlRegistry, ()> for AppState {
        fn event(
            state: &mut Self,
            proxy: &wayland_client::protocol::wl_registry::WlRegistry,
            event: wayland_client::protocol::wl_registry::Event,
            _: &(),
            _: &Connection,
            qh: &QueueHandle<Self>,
        ) {
            if let wayland_client::protocol::wl_registry::Event::Global {
                name,
                interface,
                version,
            } = event
            {
                if interface == "zwlr_data_control_manager_v1" {
                    state.manager = Some(proxy.bind::<ZwlrDataControlManagerV1, _, _>(
                        name,
                        version.min(2),
                        qh,
                        (),
                    ));
                } else if interface == "wl_seat" && state.seat.is_none() {
                    state.seat =
                        Some(proxy.bind::<wl_seat::WlSeat, _, _>(name, version.min(1), qh, ()));
                }
            }
        }
    }

    impl Dispatch<ZwlrDataControlManagerV1, ()> for AppState {
        fn event(
            _: &mut Self,
            _: &ZwlrDataControlManagerV1,
            _: zwlr_data_control_manager_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<wl_seat::WlSeat, ()> for AppState {
        fn event(
            _: &mut Self,
            _: &wl_seat::WlSeat,
            _: wl_seat::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<ZwlrDataControlDeviceV1, ()> for AppState {
        fn event(
            state: &mut Self,
            _: &ZwlrDataControlDeviceV1,
            event: zwlr_data_control_device_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            match event {
                zwlr_data_control_device_v1::Event::DataOffer { id } => {
                    state.current_offer = Some(id);
                    state.offer_mime_types.clear();
                }
                zwlr_data_control_device_v1::Event::Selection { id: Some(offer) } => {
                    let has_text = state.offer_mime_types.iter().any(|mime| {
                        mime == "text/plain;charset=utf-8"
                            || mime == "text/plain"
                            || mime == "UTF8_STRING"
                            || mime == "STRING"
                    });
                    if has_text {
                        let (read_fd, write_fd) = match rustix::pipe::pipe() {
                            Ok(fds) => fds,
                            Err(_) => return,
                        };

                        let target_mime = state
                            .offer_mime_types
                            .iter()
                            .find(|m| m.as_str() == "text/plain;charset=utf-8")
                            .cloned()
                            .unwrap_or_else(|| "text/plain".to_string());

                        use std::os::fd::AsFd;
                        offer.receive(target_mime, write_fd.as_fd());
                        drop(write_fd);

                        use std::io::Read;
                        let mut file = std::fs::File::from(read_fd);
                        let mut text = String::new();
                        if file.read_to_string(&mut text).is_ok() {
                            let text = text.trim().to_string();
                            if !text.is_empty() {
                                let mut current = state.ctx.get();
                                if current.first().is_none_or(|item| item.text != text) {
                                    let id = chrono::Local::now().timestamp_millis() as u64;
                                    let timestamp =
                                        chrono::Local::now().format("%H:%M:%S").to_string();
                                    let item = ClipboardItem {
                                        id,
                                        text,
                                        timestamp,
                                    };
                                    current.insert(0, item.clone());
                                    if current.len() > DEFAULT_CLIPBOARD_HISTORY_LIMIT {
                                        current.pop();
                                    }
                                    state.ctx.send_replace(current);
                                    if let Some(ref store) = state.store {
                                        let _ =
                                            store.record(&item, DEFAULT_CLIPBOARD_HISTORY_LIMIT);
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    impl Dispatch<ZwlrDataControlOfferV1, ()> for AppState {
        fn event(
            state: &mut Self,
            _: &ZwlrDataControlOfferV1,
            event: zwlr_data_control_offer_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if let zwlr_data_control_offer_v1::Event::Offer { mime_type } = event {
                state.offer_mime_types.push(mime_type);
            }
        }
    }

    let conn = match Connection::connect_to_env() {
        Ok(c) => c,
        Err(err) => {
            tracing::debug!("Wayland connection unavailable for clipboard monitoring: {err}");
            if let Ok(mut lock) = last_error.lock() {
                *lock = Some("Wayland ext-data-control protocol unavailable".to_string());
            }
            return;
        }
    };

    let mut event_queue: EventQueue<AppState> = conn.new_event_queue();
    let qh = event_queue.handle();
    let _display = conn.display();
    let _registry = conn.display().get_registry(&qh, ());

    let mut app_state = AppState {
        manager: None,
        seat: None,
        device: None,
        current_offer: None,
        offer_mime_types: Vec::new(),
        ctx,
        store,
        last_error: last_error.clone(),
    };

    if event_queue.roundtrip(&mut app_state).is_err() {
        if let Ok(mut lock) = last_error.lock() {
            *lock = Some("Wayland ext-data-control protocol unavailable".to_string());
        }
        return;
    }

    let (Some(manager), Some(seat)) = (app_state.manager.take(), app_state.seat.take()) else {
        tracing::debug!("Wayland compositor does not support zwlr_data_control_manager_v1");
        if let Ok(mut lock) = last_error.lock() {
            *lock = Some("Wayland ext-data-control protocol unavailable".to_string());
        }
        return;
    };

    let device = manager.get_data_device(&seat, &qh, ());

    app_state.device = Some(device);

    loop {
        if event_queue.blocking_dispatch(&mut app_state).is_err() {
            break;
        }
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
