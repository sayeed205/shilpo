use crate::daemon::{ChangeKind, DaemonState, ThemeUpdate};
use crate::dbus::ThemeDbusProxy;
use crate::persistence::read_state_snapshot;
use anyhow::{Context, Result, anyhow};
use futures_lite::stream::StreamExt;
use shilpo_theme::{ColorSource, ThemeMode};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};
use zbus::Connection;

#[derive(Clone)]
pub struct ThemeClient {
    current_state: Arc<Mutex<DaemonState>>,
    tx: broadcast::Sender<ThemeUpdate>,
    dbus_conn: Arc<tokio::sync::RwLock<Option<Connection>>>,
}

fn tokio_handle() -> tokio::runtime::Handle {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => handle,
        Err(_) => {
            static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> =
                std::sync::OnceLock::new();
            let rt = RUNTIME.get_or_init(|| {
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to create Tokio runtime for ThemeClient")
            });
            rt.handle().clone()
        }
    }
}

impl ThemeClient {
    pub async fn new() -> Self {
        let (tx, _) = broadcast::channel(64);
        let initial_state = read_state_snapshot().unwrap_or_default();
        let current_state = Arc::new(Mutex::new(initial_state));
        let dbus_conn = Arc::new(tokio::sync::RwLock::new(None));

        let client = Self {
            current_state,
            tx,
            dbus_conn,
        };

        client.connect_and_subscribe().await;
        client
    }

    pub fn spawn_task<F>(future: F) -> tokio::task::JoinHandle<F::Output>
    where
        F: std::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        tokio_handle().spawn(future)
    }

    pub fn current_state(&self) -> DaemonState {
        self.current_state.lock().unwrap().clone()
    }

    /// Current state wrapped as a `full()` update, for lagged catch-up and
    /// reconnect paths where no delta was observed.
    pub fn current_update(&self) -> ThemeUpdate {
        let state = self.current_state.lock().unwrap().clone();
        ThemeUpdate {
            state,
            change_kind: ChangeKind::full(),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ThemeUpdate> {
        self.tx.subscribe()
    }

    async fn connect_and_subscribe(&self) {
        let client_clone = self.clone();
        tokio_handle().spawn(async move {
            let mut backoff = Duration::from_millis(200);
            let max_backoff = Duration::from_secs(5);

            loop {
                match Connection::session().await {
                    Ok(conn) => {
                        let proxy_res = ThemeDbusProxy::builder(&conn)
                            .destination("org.shilpo.Theme")
                            .and_then(|b| b.path("/org/shilpo/Theme"))
                            .map(|b| b.build());

                        match proxy_res {
                            Ok(proxy_fut) => match proxy_fut.await {
                                Ok(proxy) => {
                                    // 1. Subscribe to StateChanged signal FIRST
                                    match proxy.receive_state_changed().await {
                                        Ok(mut signal_stream) => {
                                            info!("Subscribed to D-Bus org.shilpo.Theme StateChanged signal");
                                            *client_clone.dbus_conn.write().await = Some(conn.clone());
                                            backoff = Duration::from_millis(200);

                                            // 2. Query GetState to close signal gap
                                            if let Ok(raw_state) = proxy.get_state().await
                                                && let Ok(latest_state) = serde_json::from_str(&raw_state)
                                            {
                                                client_clone.update_state_if_newer(latest_state);
                                            }
                                            // 3. Process signals
                                            while let Some(signal) = signal_stream.next().await {
                                                if let Ok(signal) = signal.args()
                                                    && let Ok(update) =
                                                        serde_json::from_str(&signal.state)
                                                {
                                                    client_clone.handle_signal_update(update);
                                                }
                                            }
                                            warn!("D-Bus StateChanged signal stream ended; reconnecting...");
                                        }
                                        Err(err) => {
                                            debug!(error = %err, "Failed to subscribe to StateChanged signal");
                                        }
                                    }
                                }
                                Err(err) => {
                                    debug!(error = %err, "Failed to build ThemeDbusProxy");
                                }
                            },
                            Err(err) => {
                                debug!(error = %err, "Failed to construct ThemeDbusProxy builder");
                            }
                        }
                    }
                    Err(err) => {
                        debug!(error = %err, "Session D-Bus connection failed");
                    }
                }

                *client_clone.dbus_conn.write().await = None;
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(max_backoff);
            }
        });
    }

    fn force_update_state(&self, new_state: DaemonState) {
        let mut cur = self.current_state.lock().unwrap();
        *cur = new_state.clone();
        let _ = self.tx.send(ThemeUpdate {
            state: new_state,
            change_kind: ChangeKind::full(),
        });
    }

    fn update_state_if_newer(&self, new_state: DaemonState) {
        self.handle_signal_update(ThemeUpdate {
            state: new_state,
            change_kind: ChangeKind::full(),
        });
    }

    fn handle_signal_update(&self, update: ThemeUpdate) {
        let mut cur = self.current_state.lock().unwrap();
        if update.state.revision >= cur.revision {
            *cur = update.state.clone();
            let _ = self.tx.send(update);
        }
    }

    fn apply_response(&self, raw_state: String) -> Result<()> {
        let state = serde_json::from_str(&raw_state).context("Invalid DaemonState response")?;
        self.force_update_state(state);
        Ok(())
    }

    async fn get_or_connect(&self) -> Result<Connection> {
        if let Some(conn) = self.dbus_conn.read().await.clone() {
            return Ok(conn);
        }
        Connection::session()
            .await
            .context("Failed to connect to session D-Bus")
    }

    async fn proxy(&self) -> Result<ThemeDbusProxy<'static>> {
        let conn = self.get_or_connect().await?;
        ThemeDbusProxy::builder(&conn)
            .destination("org.shilpo.Theme")?
            .path("/org/shilpo/Theme")?
            .build()
            .await
            .context("Failed to build ThemeDbusProxy")
    }

    pub async fn sync_state(&self) -> Result<()> {
        let proxy = self.proxy().await?;
        let raw = proxy
            .get_state()
            .await
            .map_err(|e| anyhow!("get_state failed: {e}"))?;
        let state: DaemonState =
            serde_json::from_str(&raw).context("Invalid DaemonState from get_state")?;
        self.update_state_if_newer(state);
        Ok(())
    }

    pub async fn set_mode(&self, mode: ThemeMode) -> Result<()> {
        let proxy = self.proxy().await?;
        let mode_str = serde_json::to_string(&mode)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        match proxy.set_mode(&mode_str).await {
            Ok(raw) => self.apply_response(raw),
            Err(err) => {
                let _ = self.sync_state().await;
                Err(anyhow!("D-Bus SetMode failed: {err}"))
            }
        }
    }

    pub async fn toggle_mode(&self) -> Result<()> {
        let proxy = self.proxy().await?;
        match proxy.toggle_mode().await {
            Ok(raw) => self.apply_response(raw),
            Err(err) => {
                let _ = self.sync_state().await;
                Err(anyhow!("D-Bus ToggleMode failed: {err}"))
            }
        }
    }

    pub async fn set_color_source(&self, source: ColorSource) -> Result<()> {
        let proxy = self.proxy().await?;
        let source_str = serde_json::to_string(&source)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        match proxy.set_color_source(&source_str).await {
            Ok(raw) => self.apply_response(raw),
            Err(err) => {
                let _ = self.sync_state().await;
                Err(anyhow!("D-Bus SetColorSource failed: {err}"))
            }
        }
    }

    pub async fn set_scheme_variant(&self, variant: shilpo_theme::SchemeVariant) -> Result<()> {
        let proxy = self.proxy().await?;
        match proxy.set_scheme_variant(variant.as_str()).await {
            Ok(raw) => self.apply_response(raw),
            Err(err) => {
                let _ = self.sync_state().await;
                Err(anyhow!("D-Bus SetSchemeVariant failed: {err}"))
            }
        }
    }

    pub async fn set_custom_seed(&self, argb: u32) -> Result<()> {
        let proxy = self.proxy().await?;
        self.apply_response(
            proxy
                .set_custom_seed(argb)
                .await
                .map_err(|error| anyhow!("D-Bus SetCustomSeed failed: {error}"))?,
        )
    }

    pub async fn set_wallpaper(&self, path: &str) -> Result<()> {
        let proxy = self.proxy().await?;
        self.apply_response(
            proxy
                .set_wallpaper(path)
                .await
                .map_err(|error| anyhow!("D-Bus SetWallpaper failed: {error}"))?,
        )
    }

    pub async fn set_wallpaper_directory(&self, dir: &str) -> Result<()> {
        let proxy = self.proxy().await?;
        self.apply_response(
            proxy
                .set_wallpaper_directory(dir)
                .await
                .map_err(|error| anyhow!("D-Bus SetWallpaperDirectory failed: {error}"))?,
        )
    }

    pub async fn set_random_wallpaper(&self) -> Result<()> {
        let proxy = self.proxy().await?;
        self.apply_response(
            proxy
                .set_random_wallpaper()
                .await
                .map_err(|error| anyhow!("D-Bus SetRandomWallpaper failed: {error}"))?,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::ThemeClient;

    #[test]
    fn can_construct_from_a_non_tokio_executor() {
        let client = futures_lite::future::block_on(ThemeClient::new());
        assert!(client.current_state().revision >= 1);
    }
}
