use crate::session_store::{
    ClipboardItem, DEFAULT_CLIPBOARD_HISTORY_LIMIT, HeedSessionStore, SessionStoreError,
};
use anyhow::Result;
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
    fn prune(&self, limit: usize) -> Result<(), ClipboardPersistenceError>;
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

    fn prune(&self, limit: usize) -> Result<(), ClipboardPersistenceError> {
        self.0.prune_clipboard_history(limit).map_err(Into::into)
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
    history_limit: Arc<std::sync::atomic::AtomicUsize>,
}

impl ClipboardService {
    pub fn with_store(store: Option<Arc<HeedSessionStore>>) -> Self {
        Self::with_store_and_limit(store, DEFAULT_CLIPBOARD_HISTORY_LIMIT)
    }

    pub fn with_store_and_limit(store: Option<Arc<HeedSessionStore>>, limit: usize) -> Self {
        let trait_store: Option<Arc<dyn ClipboardStore>> =
            store.map(|s| Arc::new(HeedClipboardStore(s)) as Arc<dyn ClipboardStore>);
        Self::with_custom_store_and_limit(trait_store, limit)
    }

    #[allow(dead_code)]
    pub(crate) fn with_custom_store(store: Option<Arc<dyn ClipboardStore>>) -> Self {
        Self::with_custom_store_and_limit(store, DEFAULT_CLIPBOARD_HISTORY_LIMIT)
    }

    #[allow(dead_code)]
    pub(crate) fn with_custom_store_and_limit(
        store: Option<Arc<dyn ClipboardStore>>,
        limit: usize,
    ) -> Self {
        let limit = if limit == 0 {
            DEFAULT_CLIPBOARD_HISTORY_LIMIT
        } else {
            limit
        };
        let mut initial_history = Vec::new();
        let last_error = Arc::new(Mutex::new(None));

        if let Some(ref store) = store {
            match store.history(limit) {
                Ok(items) => initial_history = items,
                Err(err) => {
                    tracing::warn!(error = %err, "failed to load clipboard history from session store");
                    if let Ok(mut lock) = last_error.lock() {
                        *lock = Some(err.to_string());
                    }
                }
            }
        }

        let history_limit = Arc::new(std::sync::atomic::AtomicUsize::new(limit));
        let store_clone = store.clone();
        let last_error_clone = last_error.clone();
        let history_limit_clone = history_limit.clone();

        let runtime = StateRuntime::spawn(
            initial_history.clone(),
            initial_history,
            move |ctx| async move {
                run_clipboard_monitoring(ctx, store_clone, last_error_clone, history_limit_clone)
                    .await;
            },
        );

        Self {
            runtime,
            store,
            last_error,
            history_limit,
        }
    }

    pub fn set_history_limit(&self, limit: usize) -> Result<(), ClipboardPersistenceError> {
        if limit == 0 {
            return Err(ClipboardPersistenceError::Store(
                SessionStoreError::InvalidLimit,
            ));
        }
        self.history_limit
            .store(limit, std::sync::atomic::Ordering::SeqCst);
        let mut current = self.runtime.get();
        if current.len() > limit {
            current.truncate(limit);
            self.runtime.send_replace(current);
        }
        if let Some(ref store) = self.store {
            store.prune(limit)?;
        }
        Ok(())
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
    history_limit: Arc<std::sync::atomic::AtomicUsize>,
) {
    #[cfg(target_os = "linux")]
    {
        let res = tokio::task::spawn_blocking(move || {
            wayland_data_control_loop(ctx, store, last_error, history_limit)
        })
        .await;
        if let Err(err) = res {
            tracing::debug!("Wayland data-control thread exited: {err}");
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = history_limit;
        if let Ok(mut lock) = last_error.lock() {
            *lock = Some("Wayland ext-data-control protocol unavailable".to_string());
        }
    }
}

/// Maximum size in bytes a single clipboard offer's content is read up to before
/// being discarded as oversized.
const CLIPBOARD_MAX_BYTES: usize = 1_048_576;

/// The kind of clipboard offer selected for capture, and the MIME type to request it as.
#[derive(Debug, Clone, PartialEq, Eq)]
enum OfferKind {
    UriList,
    Text(String),
}

/// Returns true if the offer's MIME types carry the password-manager sensitive-content
/// hint. Such offers must never be persisted or published to the watch stream.
fn is_password_manager_offer(mime_types: &[String]) -> bool {
    mime_types.iter().any(|m| m == "x-kde-passwordManagerHint")
}

/// Classifies a selection offer's MIME types into a capturable kind, or `None` if the
/// offer carries no capturable content. URI lists take priority over plain text.
fn classify_offer(mime_types: &[String]) -> Option<OfferKind> {
    if mime_types.iter().any(|m| m == "text/uri-list") {
        return Some(OfferKind::UriList);
    }
    let text_mime = mime_types.iter().find(|mime| {
        mime.as_str() == "text/plain;charset=utf-8"
            || mime.as_str() == "text/plain"
            || mime.as_str() == "UTF8_STRING"
            || mime.as_str() == "STRING"
    })?;
    let target_mime = mime_types
        .iter()
        .find(|m| m.as_str() == "text/plain;charset=utf-8")
        .unwrap_or(text_mime)
        .clone();
    Some(OfferKind::Text(target_mime))
}

/// Parses a `text/uri-list` payload into file paths, skipping comments and blank lines
/// and percent-decoding `file://` URIs.
fn parse_uri_list(raw: &str) -> Vec<std::path::PathBuf> {
    raw.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|uri_str| {
            if let Some(file_path) = url::Url::parse(uri_str)
                .ok()
                .and_then(|u| u.to_file_path().ok())
            {
                return file_path;
            }
            if let Some(stripped) = uri_str.strip_prefix("file://") {
                std::path::PathBuf::from(stripped)
            } else {
                std::path::PathBuf::from(uri_str)
            }
        })
        .collect()
}

/// Reads `reader` to completion, retrying on `WouldBlock`, up to `cap` bytes.
/// Returns `None` if the stream is empty, exceeds `cap`, or errors; the caller must
/// discard the offer in either case rather than persisting a truncated payload.
fn read_bounded_stream(
    mut reader: impl std::io::Read,
    cap: usize,
    is_cancelled: impl Fn() -> bool,
) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 4096];
    while !is_cancelled() {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                bytes.extend_from_slice(&buffer[..read]);
                if bytes.len() > cap {
                    return None;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(_) => return None,
        }
    }
    if bytes.is_empty() { None } else { Some(bytes) }
}

#[cfg(target_os = "linux")]
fn wayland_data_control_loop(
    ctx: StateContext<Vec<ClipboardItem>>,
    store: Option<Arc<dyn ClipboardStore>>,
    last_error: Arc<Mutex<Option<String>>>,
    history_limit: Arc<std::sync::atomic::AtomicUsize>,
) {
    use wayland_client::{
        Connection, Dispatch, EventQueue, Proxy, QueueHandle, backend::ObjectId, protocol::wl_seat,
    };
    use wayland_protocols::ext::data_control::v1::client::{
        ext_data_control_device_v1::{self, ExtDataControlDeviceV1},
        ext_data_control_manager_v1::{self, ExtDataControlManagerV1},
        ext_data_control_offer_v1::{self, ExtDataControlOfferV1},
    };

    struct AppState {
        manager: Option<ExtDataControlManagerV1>,
        seat: Option<wl_seat::WlSeat>,
        device: Option<ExtDataControlDeviceV1>,
        offer_mime_types: std::collections::HashMap<ObjectId, Vec<String>>,
        ctx: StateContext<Vec<ClipboardItem>>,
        store: Option<Arc<dyn ClipboardStore>>,
        history_limit: Arc<std::sync::atomic::AtomicUsize>,
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
                if interface == "ext_data_control_manager_v1" {
                    state.manager = Some(proxy.bind::<ExtDataControlManagerV1, _, _>(
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

    impl Dispatch<ExtDataControlManagerV1, ()> for AppState {
        fn event(
            _: &mut Self,
            _: &ExtDataControlManagerV1,
            _: ext_data_control_manager_v1::Event,
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

    impl Dispatch<ExtDataControlDeviceV1, ()> for AppState {
        fn event(
            state: &mut Self,
            _: &ExtDataControlDeviceV1,
            event: ext_data_control_device_v1::Event,
            _: &(),
            conn: &Connection,
            _: &QueueHandle<Self>,
        ) {
            match event {
                ext_data_control_device_v1::Event::DataOffer { id } => {
                    state.offer_mime_types.insert(id.id(), Vec::new());
                }
                ext_data_control_device_v1::Event::Selection { id: Some(offer) } => {
                    let mime_types = state
                        .offer_mime_types
                        .get(&offer.id())
                        .cloned()
                        .unwrap_or_default();

                    // Sensitive-content exclusion: x-kde-passwordManagerHint must never be persisted or published
                    if is_password_manager_offer(&mime_types) {
                        return;
                    }

                    let Some(kind) = classify_offer(&mime_types) else {
                        return;
                    };

                    if !state.ctx.cancellation.is_cancelled() {
                        let (read_fd, write_fd) = match rustix::pipe::pipe() {
                            Ok(fds) => fds,
                            Err(_) => return,
                        };

                        let target_mime = match &kind {
                            OfferKind::UriList => "text/uri-list".to_string(),
                            OfferKind::Text(mime) => mime.clone(),
                        };

                        use std::os::fd::AsFd;
                        offer.receive(target_mime, write_fd.as_fd());
                        drop(write_fd);
                        // The receive request is buffered by Wayland; flush it before
                        // reading, otherwise the compositor cannot write the offer.
                        let _ = conn.flush();

                        if let Ok(flags) = rustix::fs::fcntl_getfl(&read_fd) {
                            let _ = rustix::fs::fcntl_setfl(
                                &read_fd,
                                flags | rustix::fs::OFlags::NONBLOCK,
                            );
                        }
                        let file = std::fs::File::from(read_fd);
                        let cancellation = state.ctx.cancellation.clone();
                        let Some(bytes) = read_bounded_stream(file, CLIPBOARD_MAX_BYTES, || {
                            cancellation.is_cancelled()
                        }) else {
                            return;
                        };

                        if let Ok(raw_text) = String::from_utf8(bytes) {
                            let limit = state
                                .history_limit
                                .load(std::sync::atomic::Ordering::SeqCst);
                            let now = chrono::Utc::now();

                            let item = match &kind {
                                OfferKind::UriList => {
                                    let paths = parse_uri_list(&raw_text);
                                    if paths.is_empty() {
                                        None
                                    } else {
                                        Some(ClipboardItem::new_file_reference(paths, now))
                                    }
                                }
                                OfferKind::Text(_) => {
                                    let text = raw_text.trim().to_string();
                                    if text.is_empty() {
                                        None
                                    } else {
                                        Some(ClipboardItem::new_text(text, now))
                                    }
                                }
                            };

                            if let Some(item) = item {
                                let mut current = state.ctx.get();
                                // Dedup across whole history with promote-on-repeat
                                if let Some(pos) = current.iter().position(|it| it.id == item.id) {
                                    current.remove(pos);
                                }
                                current.insert(0, item.clone());
                                if current.len() > limit {
                                    current.truncate(limit);
                                }
                                state.ctx.send_replace(current);
                                if let Some(ref store) = state.store {
                                    let _ = store.record(&item, limit);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    impl Dispatch<ExtDataControlOfferV1, ()> for AppState {
        fn event(
            state: &mut Self,
            proxy: &ExtDataControlOfferV1,
            event: ext_data_control_offer_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if let ext_data_control_offer_v1::Event::Offer { mime_type } = event {
                state
                    .offer_mime_types
                    .entry(proxy.id())
                    .or_default()
                    .push(mime_type);
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
        offer_mime_types: std::collections::HashMap::new(),
        ctx,
        store,
        history_limit,
        last_error: last_error.clone(),
    };

    if event_queue.roundtrip(&mut app_state).is_err() {
        if let Ok(mut lock) = last_error.lock() {
            *lock = Some("Wayland ext-data-control protocol unavailable".to_string());
        }
        return;
    }

    let (Some(manager), Some(seat)) = (app_state.manager.take(), app_state.seat.take()) else {
        tracing::debug!("Wayland compositor does not support ext_data_control_manager_v1");
        if let Ok(mut lock) = last_error.lock() {
            *lock = Some("Wayland ext-data-control protocol unavailable".to_string());
        }
        return;
    };

    let device = manager.get_data_device(&seat, &qh, ());

    app_state.device = Some(device);

    while !app_state.ctx.cancellation.is_cancelled() {
        if event_queue.dispatch_pending(&mut app_state).is_err() {
            break;
        }
        let _ = conn.flush();
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ClipboardContent;

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

        fn prune(&self, limit: usize) -> Result<(), ClipboardPersistenceError> {
            let mut records = self.records.lock().unwrap();
            if records.len() > limit {
                records.truncate(limit);
            }
            Ok(())
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
        *mock.history_result.lock().unwrap() = Ok(vec![ClipboardItem::new_text(
            "secret".into(),
            chrono::Utc::now(),
        )]);
        *mock.clear_result.lock().unwrap() = Err(ClipboardPersistenceError::Disabled);

        let service = ClipboardService::with_custom_store(Some(mock));
        assert_eq!(service.history().len(), 1);

        assert!(service.clear_history().is_err());
        assert_eq!(service.history().len(), 1);
    }

    #[test]
    fn test_clipboard_clear_success_updates_both() {
        let mock = Arc::new(MockStore::new());
        *mock.history_result.lock().unwrap() = Ok(vec![ClipboardItem::new_text(
            "secret".into(),
            chrono::Utc::now(),
        )]);

        let service = ClipboardService::with_custom_store(Some(mock));
        assert_eq!(service.history().len(), 1);

        assert!(service.clear_history().is_ok());
        assert!(service.history().is_empty());
    }

    #[test]
    fn test_runtime_limit_shrink_evicts_memory_and_store() {
        let mock = Arc::new(MockStore::new());
        let items: Vec<ClipboardItem> = (1..=10)
            .map(|i| {
                ClipboardItem::new_text(
                    format!("item_{i}"),
                    chrono::Utc::now() + chrono::Duration::seconds(i),
                )
            })
            .collect();
        *mock.history_result.lock().unwrap() = Ok(items.clone());
        *mock.records.lock().unwrap() = items;

        let service = ClipboardService::with_custom_store_and_limit(Some(mock.clone()), 10);
        assert_eq!(service.history().len(), 10);

        // Lower limit at runtime to 3
        service.set_history_limit(3).unwrap();
        assert_eq!(service.history().len(), 3);
        assert_eq!(mock.records.lock().unwrap().len(), 3);

        // Zero limit must be rejected
        assert!(service.set_history_limit(0).is_err());
    }

    #[test]
    fn test_uri_list_distinguishable_from_text() {
        let now = chrono::Utc::now();
        let text_item = ClipboardItem::new_text("file:///path/to/file.txt".into(), now);
        let uri_item = ClipboardItem::new_file_reference(
            vec![std::path::PathBuf::from("/path/to/file.txt")],
            now,
        );

        assert_ne!(text_item.content, uri_item.content);
        assert!(matches!(text_item.content, ClipboardContent::Text(_)));
        assert!(matches!(
            uri_item.content,
            ClipboardContent::FileReference(_)
        ));
        assert_ne!(text_item.id, uri_item.id);
    }

    #[test]
    fn test_sensitive_password_manager_hint_exclusion() {
        let mimes_with_hint = vec![
            "text/plain;charset=utf-8".to_string(),
            "x-kde-passwordManagerHint".to_string(),
        ];
        let mimes_without_hint = vec!["text/plain;charset=utf-8".to_string()];

        assert!(is_password_manager_offer(&mimes_with_hint));
        assert!(!is_password_manager_offer(&mimes_without_hint));
    }

    #[test]
    fn test_classify_offer_prefers_uri_list_over_text() {
        let mixed = vec![
            "text/plain".to_string(),
            "text/uri-list".to_string(),
            "text/plain;charset=utf-8".to_string(),
        ];
        assert_eq!(classify_offer(&mixed), Some(OfferKind::UriList));

        let text_only = vec!["STRING".to_string(), "text/plain;charset=utf-8".to_string()];
        assert_eq!(
            classify_offer(&text_only),
            Some(OfferKind::Text("text/plain;charset=utf-8".to_string()))
        );

        let uncapturable = vec!["image/png".to_string()];
        assert_eq!(classify_offer(&uncapturable), None);
    }

    #[test]
    fn test_uri_list_parser_decoding_and_filtering() {
        let raw_payload =
            "# Comment line\r\nfile:///home/user/document%201.pdf\r\nfile:///tmp/image.png\r\n\r\n";
        let paths = parse_uri_list(raw_payload);

        assert_eq!(paths.len(), 2);
        assert_eq!(
            paths[0],
            std::path::PathBuf::from("/home/user/document 1.pdf")
        );
        assert_eq!(paths[1], std::path::PathBuf::from("/tmp/image.png"));
    }

    #[test]
    fn test_read_bounded_stream_rejects_oversized_payload_without_corrupting_history() {
        let oversized = std::io::Cursor::new(vec![b'x'; CLIPBOARD_MAX_BYTES + 1]);
        assert_eq!(
            read_bounded_stream(oversized, CLIPBOARD_MAX_BYTES, || false),
            None,
            "content exceeding the cap must be rejected, not truncated and kept"
        );

        let within_cap = std::io::Cursor::new(b"hello clipboard".to_vec());
        assert_eq!(
            read_bounded_stream(within_cap, CLIPBOARD_MAX_BYTES, || false),
            Some(b"hello clipboard".to_vec())
        );

        let empty = std::io::Cursor::new(Vec::new());
        assert_eq!(
            read_bounded_stream(empty, CLIPBOARD_MAX_BYTES, || false),
            None
        );
    }
}
