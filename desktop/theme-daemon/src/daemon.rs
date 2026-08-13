use crate::adapters::{DesktopAdapter, select_desktop_adapter};
use crate::dbus::{ActorMessage, EffectStatus, ThemeDbusService};
use crate::executors::{AdapterExecutor, PersistenceExecutor, ProjectionStatus};
use crate::portal::PortalObserver;
use anyhow::Result;
use image::DynamicImage;
use image::imageops::FilterType;
use mcu_material_color::{Hct, QuantizerCelebi, Score};
use serde::{Deserialize, Serialize};
use shilpo_ui::theme::{
    ColorSource, SchemeVariant, ThemeCommand, ThemeMode, ThemeState, generate_m3_palettes,
    materialize_seed_with_variant, reduce, resolve_variant,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;
use tracing::{debug, info};
use zbus::Connection;
use zbus::names::BusName;

use crate::wallpaper_cache::WallpaperAnalysisCache;

pub type WallpaperExtractorFn =
    Arc<dyn Fn(&Path) -> anyhow::Result<(u32, SchemeVariant)> + Send + Sync>;
pub type WallpaperBackendFn = Arc<dyn Fn(&Path) -> Result<(), String> + Send + Sync>;

#[derive(Clone, Default)]
pub struct ThemeDaemonOptions {
    pub provider: Option<String>,
    pub gtk_theme_light: Option<String>,
    pub gtk_theme_dark: Option<String>,
    pub custom_adapter_cmd: Option<Vec<String>>,
    pub wallpaper_dir: Option<PathBuf>,
    pub scheme_variant: Option<SchemeVariant>,
    pub config_path: Option<PathBuf>,
    pub state_path: Option<PathBuf>,
    pub wallpaper_extractor: Option<WallpaperExtractorFn>,
    pub wallpaper_backend: Option<WallpaperBackendFn>,
    pub headless: bool,
}

impl std::fmt::Debug for ThemeDaemonOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThemeDaemonOptions")
            .field("provider", &self.provider)
            .field("gtk_theme_light", &self.gtk_theme_light)
            .field("gtk_theme_dark", &self.gtk_theme_dark)
            .field("custom_adapter_cmd", &self.custom_adapter_cmd)
            .field("wallpaper_dir", &self.wallpaper_dir)
            .field("scheme_variant", &self.scheme_variant)
            .field("config_path", &self.config_path)
            .field("state_path", &self.state_path)
            .field("wallpaper_extractor", &self.wallpaper_extractor.is_some())
            .field("wallpaper_backend", &self.wallpaper_backend.is_some())
            .field("headless", &self.headless)
            .finish()
    }
}

impl PartialEq for ThemeDaemonOptions {
    fn eq(&self, other: &Self) -> bool {
        self.provider == other.provider
            && self.gtk_theme_light == other.gtk_theme_light
            && self.gtk_theme_dark == other.gtk_theme_dark
            && self.custom_adapter_cmd == other.custom_adapter_cmd
            && self.wallpaper_dir == other.wallpaper_dir
            && self.scheme_variant == other.scheme_variant
            && self.config_path == other.config_path
            && self.state_path == other.state_path
            && self.headless == other.headless
    }
}

impl Eq for ThemeDaemonOptions {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonState {
    #[serde(flatten)]
    pub theme: ThemeState,
    pub wallpaper_path: Option<PathBuf>,
    pub wallpaper_seed: Option<u32>,
    /// The variant the wallpaper's image statistics resolved for the current
    /// wallpaper seed, so switching sources and re-materializing keeps the same
    /// image-aware resolution instead of falling back to seed chroma.
    #[serde(default)]
    pub wallpaper_detected_variant: SchemeVariant,
    pub wallpaper_dir: PathBuf,
}

impl DaemonState {
    pub fn new(timestamp: &str) -> Self {
        Self {
            theme: ThemeState::new(timestamp),
            wallpaper_path: None,
            wallpaper_seed: None,
            wallpaper_detected_variant: SchemeVariant::Auto,
            wallpaper_dir: PathBuf::from("~/Pictures/Wallpapers"),
        }
    }
}

impl Default for DaemonState {
    fn default() -> Self {
        Self::new(shilpo_ui::theme::state::DEFAULT_TIMESTAMP)
    }
}

impl std::ops::Deref for DaemonState {
    type Target = ThemeState;
    fn deref(&self) -> &Self::Target {
        &self.theme
    }
}

impl std::ops::DerefMut for DaemonState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.theme
    }
}

/// A command handled by the theme daemon.
///
/// Pure core transitions are carried as [`DaemonCommand::Theme`] and forwarded
/// unchanged to `shilpo_ui::theme::reduce`; wallpaper and portal concerns stay at
/// this (daemon) layer, never leaking into `core/theme`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonCommand {
    Theme(ThemeCommand),
    SetWallpaperDirectory(PathBuf),
    SetWallpaper {
        path: PathBuf,
        seed: u32,
        detected_variant: SchemeVariant,
    },
    PortalAppearanceChanged(Option<ThemeMode>),
}

pub struct WallpaperTaskResult {
    pub op_id: u64,
    pub path: PathBuf,
    pub seed: u32,
    pub detected_variant: SchemeVariant,
    pub error: Option<String>,
    pub reply: tokio::sync::oneshot::Sender<Result<DaemonState, String>>,
}

pub struct ThemeDaemon {
    state: DaemonState,
    adapter: Arc<dyn DesktopAdapter>,
    persistence_executor: PersistenceExecutor,
    adapter_executor: AdapterExecutor,
    config_path: PathBuf,
    actor_rx: mpsc::UnboundedReceiver<ActorMessage>,
    portal_rx: mpsc::UnboundedReceiver<Option<ThemeMode>>,
    wallpaper_result_tx: mpsc::UnboundedSender<WallpaperTaskResult>,
    wallpaper_result_rx: mpsc::UnboundedReceiver<WallpaperTaskResult>,
    current_wallpaper_op: Arc<AtomicU64>,
    wallpaper_cache: Arc<Mutex<WallpaperAnalysisCache>>,
    wallpaper_extractor: Option<WallpaperExtractorFn>,
    wallpaper_backend: Option<WallpaperBackendFn>,
    _conn: Option<Connection>,
    effects: Arc<Mutex<EffectStatus>>,
}

impl ThemeDaemon {
    pub async fn new() -> Result<Self> {
        Self::with_options(ThemeDaemonOptions::default()).await
    }

    pub async fn with_options(options: ThemeDaemonOptions) -> Result<Self> {
        let (actor_tx, actor_rx) = mpsc::unbounded_channel();
        let (portal_tx, portal_rx) = mpsc::unbounded_channel();
        let (wp_tx, wp_rx) = mpsc::unbounded_channel();

        let state_path = options
            .state_path
            .clone()
            .unwrap_or_else(crate::persistence::state_file_path);
        let config_path = options.config_path.clone().unwrap_or_else(|| {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
                .unwrap_or_else(|| PathBuf::from(".config"))
                .join("shilpo")
                .join("config.toml")
        });

        let adapter: Arc<dyn DesktopAdapter> = Arc::from(select_desktop_adapter(
            options.provider.as_deref(),
            options.gtk_theme_light,
            options.gtk_theme_dark,
            options.custom_adapter_cmd,
        ));

        let initial_state = initial_state(
            crate::persistence::read_state_snapshot_from(&state_path),
            options.wallpaper_dir.as_deref(),
            options.scheme_variant,
        );

        let persistence_executor = PersistenceExecutor::new(Some(state_path));
        let adapter_executor = AdapterExecutor::new(adapter.clone());
        let effects = Arc::new(Mutex::new(EffectStatus {
            durable_revision: persistence_executor.durable_revision(),
            projection_status: adapter_executor.status(),
        }));

        // Startup reconciliation: reapply persisted resolved mode through desktop adapter
        adapter_executor.project(
            initial_state.theme.revision,
            initial_state.theme.resolved_mode,
        );

        let conn = if options.headless {
            None
        } else {
            let conn = Connection::session().await?;
            use zbus::fdo::{DBusProxy, RequestNameFlags, RequestNameReply};
            let dbus = DBusProxy::new(&conn).await?;
            let reply = dbus
                .request_name(
                    "org.shilpo.Theme".try_into()?,
                    RequestNameFlags::DoNotQueue.into(),
                )
                .await?;
            if reply != RequestNameReply::PrimaryOwner {
                eprintln!("org.shilpo.Theme is already owned by another process");
                std::process::exit(1);
            }

            let service = ThemeDbusService::new(actor_tx, effects.clone());
            conn.object_server()
                .at("/org/shilpo/Theme", service)
                .await?;

            info!("shilpo-themed registered D-Bus name org.shilpo.Theme at /org/shilpo/Theme");

            PortalObserver::start(portal_tx).await;
            Some(conn)
        };

        let wallpaper_cache = Arc::new(Mutex::new(WallpaperAnalysisCache::default()));

        let daemon = Self {
            state: initial_state,
            adapter,
            persistence_executor,
            adapter_executor,
            config_path,
            actor_rx,
            portal_rx,
            wallpaper_result_tx: wp_tx,
            wallpaper_result_rx: wp_rx,
            current_wallpaper_op: Arc::new(AtomicU64::new(0)),
            wallpaper_cache,
            wallpaper_extractor: options.wallpaper_extractor,
            wallpaper_backend: options.wallpaper_backend,
            _conn: conn,
            effects,
        };

        let _ = daemon
            .persistence_executor
            .persist(daemon.state.clone())
            .await;
        let startup_update = ThemeUpdate {
            state: daemon.state.clone(),
            change_kind: ChangeKind::full(),
        };
        if let Some(conn) = &daemon._conn
            && let Ok(raw_update) = serde_json::to_string(&startup_update)
        {
            let _ = conn
                .emit_signal(
                    Option::<BusName>::None,
                    "/org/shilpo/Theme",
                    "org.shilpo.Theme",
                    "StateChanged",
                    &raw_update,
                )
                .await;
        }

        Ok(daemon)
    }

    pub fn committed_revision(&self) -> u64 {
        self.state.theme.revision
    }

    pub fn durable_revision(&self) -> u64 {
        self.persistence_executor.durable_revision()
    }

    pub fn projection_status(&self) -> ProjectionStatus {
        self.adapter_executor.status()
    }

    pub async fn shutdown(&self, deadline: std::time::Duration) -> bool {
        self.persistence_executor
            .shutdown_with_deadline(deadline)
            .await
    }

    pub async fn run(mut self) {
        info!("shilpo-themed daemon event loop started");

        loop {
            tokio::select! {
                Some(msg) = self.actor_rx.recv() => {
                    self.handle_actor_message(msg).await;
                }
                Some(portal_mode) = self.portal_rx.recv() => {
                    if let Err(error) = self
                        .process_command(DaemonCommand::PortalAppearanceChanged(portal_mode))
                        .await
                    {
                        tracing::warn!(%error, "Failed to persist portal appearance change");
                    }
                }
                Some(res) = self.wallpaper_result_rx.recv() => {
                    self.handle_wallpaper_completion(res).await;
                }
                else => break,
            }
        }
    }

    async fn handle_actor_message(&mut self, msg: ActorMessage) {
        self.refresh_effect_status();
        match msg {
            ActorMessage::GetState(reply) => {
                self.sync_wallpaper_dir_from_config();
                let _ = reply.send(Ok(self.state.clone()));
            }
            ActorMessage::GetDiagnostics(reply) => {
                self.sync_wallpaper_dir_from_config();
                let diag = serde_json::json!({
                    "committed_revision": self.state.theme.revision,
                    "durable_revision": self.persistence_executor.durable_revision(),
                    "persistence_error": self.persistence_executor.last_error(),
                    "projection_status": self.adapter_executor.status(),
                    "revision": self.state.theme.revision,
                    "selected_mode": self.state.theme.selected_mode,
                    "resolved_mode": self.state.theme.resolved_mode,
                    "color_source": self.state.theme.color_source,
                    "source_argb": format!("#{:08X}", self.state.theme.source_argb),
                    "adapter": self.adapter.name(),
                    "wallpaper_dir": self.state.wallpaper_dir,
                });
                let _ = reply.send(diag.to_string());
            }
            ActorMessage::SetMode(mode, reply) => {
                let result = self
                    .process_command(DaemonCommand::Theme(ThemeCommand::SetMode(mode)))
                    .await;
                self.respond_after_durable(reply, result);
            }
            ActorMessage::ToggleMode(reply) => {
                let result = self
                    .process_command(DaemonCommand::Theme(ThemeCommand::ToggleMode))
                    .await;
                self.respond_after_durable(reply, result);
            }
            ActorMessage::SetColorSource(source, reply) => {
                let result = self
                    .process_command(DaemonCommand::Theme(ThemeCommand::SetColorSource(source)))
                    .await;
                self.respond_after_durable(reply, result);
            }
            ActorMessage::SetSchemeVariant(variant, reply) => {
                let res = self
                    .process_command(DaemonCommand::Theme(ThemeCommand::SetSchemeVariant(
                        variant,
                    )))
                    .await;
                let config_path = self.config_path.clone();
                tokio::task::spawn_blocking(move || {
                    let content = fs::read_to_string(&config_path).unwrap_or_default();
                    if let Ok(mut table) = content.parse::<toml::Table>() {
                        let theme = table
                            .entry("theme")
                            .or_insert_with(|| toml::Value::Table(Default::default()));
                        if let toml::Value::Table(theme_table) = theme {
                            theme_table.insert(
                                "scheme_variant".to_string(),
                                toml::Value::String(variant.as_str().to_string()),
                            );
                            if let Ok(new_content) = toml::to_string(&table) {
                                let _ = fs::write(&config_path, new_content);
                            }
                        }
                    }
                });
                self.respond_after_durable(reply, res);
            }
            ActorMessage::SetCustomSeed(seed, reply) => {
                let result = self
                    .process_command(DaemonCommand::Theme(ThemeCommand::SetCustomSeed(seed)))
                    .await;
                self.respond_after_durable(reply, result);
            }
            ActorMessage::SetWallpaper(path_str, reply) => {
                let path = expand_tilde(Path::new(&path_str));
                self.spawn_wallpaper_task(path, reply);
            }
            ActorMessage::SetWallpaperDirectory(dir_str, reply) => {
                let dir = expand_tilde(Path::new(&dir_str));
                let result = if dir.is_dir() {
                    let result = self
                        .process_command(DaemonCommand::SetWallpaperDirectory(dir.clone()))
                        .await;
                    match result {
                        Ok(state) => self
                            .persist_wallpaper_directory_config(&dir)
                            .map(|_| Ok(state))
                            .unwrap_or_else(Err),
                        Err(error) => Err(error),
                    }
                } else {
                    Err(format!(
                        "Wallpaper directory does not exist: {}",
                        dir.display()
                    ))
                };
                self.respond_after_durable(reply, result);
            }
            ActorMessage::SetRandomWallpaper(reply) => match self.pick_random_wallpaper() {
                Ok(path) => self.spawn_wallpaper_task(path, reply),
                Err(error) => {
                    let _ = reply.send(Err(error));
                }
            },
        }
    }

    fn respond_after_durable(
        &self,
        reply: tokio::sync::oneshot::Sender<Result<DaemonState, String>>,
        result: Result<DaemonState, String>,
    ) {
        let executor = self.persistence_executor.clone();
        let effects = self.effects.clone();
        let adapter_executor = self.adapter_executor.clone();
        match result {
            Err(error) => {
                let _ = reply.send(Err(error));
            }
            Ok(state) => {
                let revision = state.theme.revision;
                tokio::spawn(async move {
                    let result = executor.wait_until_durable(revision).await.map(|durable| {
                        let mut status = effects.lock().unwrap();
                        status.durable_revision = durable;
                        status.projection_status = adapter_executor.status();
                        state
                    });
                    let _ = reply.send(result);
                });
            }
        }
    }

    fn persist_wallpaper_directory_config(&self, dir: &Path) -> Result<(), String> {
        let content = fs::read_to_string(&self.config_path).unwrap_or_default();
        let mut table: toml::Table = content.parse().unwrap_or_default();
        let desktop = table
            .entry("desktop")
            .or_insert_with(|| toml::Value::Table(Default::default()));
        if let toml::Value::Table(desktop_table) = desktop {
            desktop_table.insert(
                "wallpaper_dir".to_string(),
                toml::Value::String(dir.to_string_lossy().to_string()),
            );
        }
        let new_content = toml::to_string(&table)
            .map_err(|error| format!("Failed to serialize config: {error}"))?;
        fs::write(&self.config_path, new_content)
            .map_err(|error| format!("Failed to persist wallpaper directory: {error}"))
    }

    fn spawn_wallpaper_task(
        &mut self,
        path: PathBuf,
        reply: tokio::sync::oneshot::Sender<Result<DaemonState, String>>,
    ) {
        let active_variant = self.state.theme.scheme_variant;
        let (canonical_path, key) =
            match crate::wallpaper_cache::create_wallpaper_cache_key(&path, active_variant) {
                Ok(pair) => pair,
                Err(error) => {
                    let _ = reply.send(Err(error));
                    return;
                }
            };

        let op_id = self.current_wallpaper_op.fetch_add(1, Ordering::SeqCst) + 1;
        let tx = self.wallpaper_result_tx.clone();
        let cache = self.wallpaper_cache.clone();
        let extractor = self.wallpaper_extractor.clone();
        let backend = self.wallpaper_backend.clone();

        tokio::spawn(async move {
            info!(op_id, path = %canonical_path.display(), "Starting background wallpaper processing");

            let cached_analysis = crate::wallpaper_cache::with_cache(&cache, |c| c.get(&key));

            let (seed, detected_variant) = if let Some(analysis) = cached_analysis {
                let _span = tracing::info_span!(
                    target: "shilpo_profile",
                    "wallpaper_analysis",
                    operation = "wallpaper_analysis",
                    cache = "hit",
                    outcome = "success",
                    scheme_variant = ?active_variant,
                );
                let _enter = _span.enter();
                debug!(op_id, path = %canonical_path.display(), "Wallpaper analysis cache hit");
                (analysis.seed, analysis.detected_variant)
            } else {
                let span = tracing::info_span!(
                    target: "shilpo_profile",
                    "wallpaper_analysis",
                    operation = "wallpaper_analysis",
                    cache = "miss",
                    outcome = tracing::field::Empty,
                    scheme_variant = ?active_variant,
                );
                let _enter = span.enter();
                debug!(op_id, path = %canonical_path.display(), "Wallpaper analysis cache miss");

                let path_for_ext = canonical_path.clone();
                let extraction = tokio::task::spawn_blocking(move || {
                    if let Some(ext) = extractor {
                        ext(&path_for_ext)
                    } else {
                        extract_wallpaper_seed_and_variant(&path_for_ext)
                    }
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));

                match extraction {
                    Ok((seed, variant)) => {
                        span.record("outcome", "success");
                        let analysis = crate::wallpaper_cache::WallpaperAnalysis {
                            seed,
                            detected_variant: variant,
                        };
                        crate::wallpaper_cache::with_cache(&cache, |c| c.insert(key, analysis));
                        (seed, variant)
                    }
                    Err(error) => {
                        span.record("outcome", "failed");
                        let _ = tx.send(WallpaperTaskResult {
                            op_id,
                            path: canonical_path,
                            seed: 0,
                            detected_variant: SchemeVariant::Auto,
                            error: Some(error),
                            reply,
                        });
                        return;
                    }
                }
            };

            let path_for_backend = canonical_path.clone();
            let backend_result = tokio::task::spawn_blocking(move || {
                if let Some(b) = backend {
                    b(&path_for_backend)
                } else {
                    let status = std::process::Command::new("awww")
                        .arg("img")
                        .arg(&path_for_backend)
                        .status();
                    match status {
                        Ok(status) if status.success() => Ok(()),
                        Ok(status) => Err(format!("awww exited with status {status}")),
                        Err(error) => Err(format!("Failed to execute awww: {error}")),
                    }
                }
            })
            .await;

            let error = match backend_result {
                Ok(Ok(())) => None,
                Ok(Err(err_msg)) => Some(err_msg),
                Err(task_err) => Some(format!("Wallpaper backend task failed: {task_err}")),
            };

            debug!(
                op_id,
                success = error.is_none(),
                "Wallpaper backend invocation completed"
            );
            let _ = tx.send(WallpaperTaskResult {
                op_id,
                path: canonical_path,
                seed,
                detected_variant,
                error,
                reply,
            });
        });
    }

    async fn handle_wallpaper_completion(&mut self, res: WallpaperTaskResult) {
        let current_op = self.current_wallpaper_op.load(Ordering::SeqCst);
        if res.op_id < current_op {
            debug!(
                op_id = res.op_id,
                current_op, "Discarding superseded wallpaper task completion"
            );
            let _ = res.reply.send(Err("Wallpaper request superseded".into()));
            return;
        }

        if let Some(error) = res.error {
            let _ = res.reply.send(Err(error));
            return;
        }

        let result = self
            .process_command(DaemonCommand::SetWallpaper {
                path: res.path,
                seed: res.seed,
                detected_variant: res.detected_variant,
            })
            .await;
        self.respond_after_durable(res.reply, result);
    }

    async fn process_command(&mut self, command: DaemonCommand) -> Result<DaemonState, String> {
        let now = chrono::Utc::now().to_rfc3339();
        let previous_revision = self.state.theme.revision;
        let mut next_state = self.state.clone();
        let outcome = apply_command(&mut next_state, command, &now)?;

        if next_state.theme.revision > previous_revision {
            self.state = next_state;

            let update = ThemeUpdate {
                state: self.state.clone(),
                change_kind: outcome.change_kind,
            };
            let raw_update = serde_json::to_string(&update).map_err(|error| error.to_string())?;
            if let Some(conn) = &self._conn {
                let _ = conn
                    .emit_signal(
                        Option::<BusName>::None,
                        "/org/shilpo/Theme",
                        "org.shilpo.Theme",
                        "StateChanged",
                        &raw_update,
                    )
                    .await;
            }

            self.persistence_executor.enqueue(self.state.clone())?;
            self.refresh_effect_status();
        }

        if let Some(mode) = outcome.dispatch_adapter_mode {
            self.adapter_executor
                .project(self.state.theme.revision, mode);
            self.refresh_effect_status();
        }

        Ok(self.state.clone())
    }

    fn refresh_effect_status(&self) {
        let mut status = self.effects.lock().unwrap();
        status.durable_revision = self.persistence_executor.durable_revision();
        status.projection_status = self.adapter_executor.status();
    }

    fn sync_wallpaper_dir_from_config(&mut self) {
        if let Ok(content) = fs::read_to_string(&self.config_path)
            && let Ok(table) = content.parse::<toml::Table>()
            && let Some(desktop) = table.get("desktop").and_then(|v| v.as_table())
            && let Some(dir_str) = desktop.get("wallpaper_dir").and_then(|v| v.as_str())
        {
            let config_dir = expand_tilde(Path::new(dir_str));
            if config_dir != self.state.wallpaper_dir {
                debug!(
                    old = %self.state.wallpaper_dir.display(),
                    new = %config_dir.display(),
                    "Syncing daemon wallpaper_dir from config.toml"
                );
                self.state.wallpaper_dir = config_dir;
            }
        }
    }

    fn pick_random_wallpaper(&mut self) -> Result<PathBuf, String> {
        self.sync_wallpaper_dir_from_config();
        let mut wallpapers = Vec::new();
        let entries = std::fs::read_dir(&self.state.wallpaper_dir).map_err(|error| {
            format!(
                "Cannot read wallpaper directory {}: {error}",
                self.state.wallpaper_dir.display()
            )
        })?;
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file()
                && p.extension().and_then(|e| e.to_str()).is_some_and(|ext| {
                    matches!(ext.to_lowercase().as_str(), "png" | "jpg" | "jpeg" | "webp")
                })
            {
                wallpapers.push(p);
            }
        }
        if wallpapers.is_empty() {
            return Err(format!(
                "No supported wallpapers found in {} (expected png, jpg, jpeg, or webp)",
                self.state.wallpaper_dir.display()
            ));
        }

        let idx = (chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default() as usize)
            % wallpapers.len();
        Ok(wallpapers[idx].clone())
    }
}

/// Which aspects of the theme changed, so consumers can react to specific kinds
/// of change (e.g. animate a mode toggle without re-deriving the palette).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeKind {
    pub mode: bool,
    pub source: bool,
    pub variant: bool,
    pub palette: bool,
    pub wallpaper: bool,
}

impl ChangeKind {
    /// Marks every aspect as changed — used for full-state syncs where no
    /// transition produced the state (startup, `get_state` replies).
    pub fn full() -> Self {
        Self {
            mode: true,
            source: true,
            variant: true,
            palette: true,
            wallpaper: true,
        }
    }
}

/// A theme state delivered to consumers, plus what changed relative to the
/// previous state so they can react selectively instead of re-deriving
/// everything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeUpdate {
    pub state: DaemonState,
    pub change_kind: ChangeKind,
}

/// Side effects a daemon command requests, expressed purely so callers can
/// perform the actual I/O (adapter invocation, D-Bus signal, persistence).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct ApplyOutcome {
    /// Mode the desktop adapter should be switched to, if any.
    dispatch_adapter_mode: Option<ThemeMode>,
    /// Which aspects of the state actually changed, for consumers that react
    /// to specific kinds of change.
    change_kind: ChangeKind,
}

/// Pure application of a [`DaemonCommand`] to a [`DaemonState`] snapshot.
///
/// This is the daemon-side seam: it decides state transitions and which mode (if
/// any) the desktop adapter must be told about, without touching D-Bus, files, or
/// subprocesses, so every command's logic is testable without system mocks. Core
/// transitions are delegated to `shilpo_ui::theme::reduce`; wallpaper and portal
/// concerns are handled here, at the daemon layer (ADR-0002).
fn apply_command(
    state: &mut DaemonState,
    command: DaemonCommand,
    now: &str,
) -> Result<ApplyOutcome, String> {
    let before = ChangeSnapshot::from_state(state);
    let mut outcome = ApplyOutcome::default();

    match command {
        DaemonCommand::Theme(command) => match command {
            ThemeCommand::SetMode(mode) => {
                let previous_resolved = state.theme.resolved_mode;
                reduce(&mut state.theme, ThemeCommand::SetMode(mode), now);
                if mode != ThemeMode::System {
                    if state.theme.resolved_mode != previous_resolved {
                        outcome.dispatch_adapter_mode = Some(state.theme.resolved_mode);
                    } else if state.theme.selected_mode == mode {
                        outcome.dispatch_adapter_mode = Some(mode);
                    }
                }
            }
            ThemeCommand::ToggleMode => {
                reduce(&mut state.theme, ThemeCommand::ToggleMode, now);
                outcome.dispatch_adapter_mode = Some(state.theme.resolved_mode);
            }
            ThemeCommand::SetColorSource(source) => {
                let available = match source {
                    ColorSource::Custom => state.theme.custom_seed.is_some(),
                    ColorSource::Wallpaper => state.wallpaper_seed.is_some(),
                };
                if !available {
                    return Err(format!("No seed is available for color source {source:?}"));
                }
                reduce(&mut state.theme, ThemeCommand::SetColorSource(source), now);
                if source == ColorSource::Wallpaper
                    && let Some(seed) = state.wallpaper_seed
                {
                    materialize_seed_with_variant(
                        &mut state.theme,
                        seed,
                        state.wallpaper_detected_variant,
                        now,
                    );
                }
            }
            ThemeCommand::SetSchemeVariant(variant) => {
                reduce(
                    &mut state.theme,
                    ThemeCommand::SetSchemeVariant(variant),
                    now,
                );
            }
            ThemeCommand::SetCustomSeed(seed) => {
                reduce(&mut state.theme, ThemeCommand::SetCustomSeed(seed), now);
            }
            ThemeCommand::SetSeed(seed) => {
                reduce(&mut state.theme, ThemeCommand::SetSeed(seed), now);
            }
        },
        DaemonCommand::SetWallpaperDirectory(dir) => {
            if state.wallpaper_dir != dir {
                state.wallpaper_dir = dir;
                bump_revision(state, now);
            }
        }
        DaemonCommand::SetWallpaper {
            path,
            seed,
            detected_variant,
        } => {
            let mut changed = false;
            if state.wallpaper_path.as_ref() != Some(&path) {
                state.wallpaper_path = Some(path);
                changed = true;
            }
            if state.wallpaper_seed != Some(seed) {
                state.wallpaper_seed = Some(seed);
                changed = true;
            }
            if state.wallpaper_detected_variant != detected_variant {
                state.wallpaper_detected_variant = detected_variant;
                changed = true;
            }
            let seed_applied =
                materialize_seed_with_variant(&mut state.theme, seed, detected_variant, now);
            if changed && !seed_applied {
                bump_revision(state, now);
            }
        }
        DaemonCommand::PortalAppearanceChanged(portal_mode) => {
            if let Some(pm) = portal_mode {
                debug_assert!(pm != ThemeMode::System);
                if state.theme.selected_mode == ThemeMode::System && state.theme.resolved_mode != pm
                {
                    state.theme.resolved_mode = pm;
                    bump_revision(state, now);
                    outcome.dispatch_adapter_mode = Some(pm);
                }
            }
        }
    }

    outcome.change_kind = before.compute_change(state);

    Ok(outcome)
}

/// Snapshot of the aspects `ChangeKind` tracks, captured before a command runs
/// so the outcome can diff what actually changed.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ChangeSnapshot {
    selected_mode: ThemeMode,
    resolved_mode: ThemeMode,
    color_source: ColorSource,
    scheme_variant: shilpo_ui::theme::SchemeVariant,
    source_argb: u32,
    wallpaper_path: Option<PathBuf>,
    wallpaper_seed: Option<u32>,
    wallpaper_detected_variant: shilpo_ui::theme::SchemeVariant,
    wallpaper_dir: PathBuf,
}

impl ChangeSnapshot {
    fn from_state(state: &DaemonState) -> Self {
        Self {
            selected_mode: state.theme.selected_mode,
            resolved_mode: state.theme.resolved_mode,
            color_source: state.theme.color_source,
            scheme_variant: state.theme.scheme_variant,
            source_argb: state.theme.source_argb,
            wallpaper_path: state.wallpaper_path.clone(),
            wallpaper_seed: state.wallpaper_seed,
            wallpaper_detected_variant: state.wallpaper_detected_variant,
            wallpaper_dir: state.wallpaper_dir.clone(),
        }
    }

    fn compute_change(&self, state: &DaemonState) -> ChangeKind {
        ChangeKind {
            mode: self.selected_mode != state.theme.selected_mode
                || self.resolved_mode != state.theme.resolved_mode,
            source: self.color_source != state.theme.color_source,
            variant: self.scheme_variant != state.theme.scheme_variant,
            palette: self.source_argb != state.theme.source_argb
                || self.scheme_variant != state.theme.scheme_variant,
            wallpaper: self.wallpaper_path != state.wallpaper_path
                || self.wallpaper_seed != state.wallpaper_seed
                || self.wallpaper_detected_variant != state.wallpaper_detected_variant
                || self.wallpaper_dir != state.wallpaper_dir,
        }
    }
}

fn bump_revision(state: &mut DaemonState, now: &str) {
    state.theme.revision += 1;
    state.theme.updated_at = now.to_string();
}

fn expand_tilde(path: &Path) -> PathBuf {
    if let Ok(path_str) = path.to_path_buf().into_os_string().into_string() {
        if let (Some(rest), Ok(home)) = (path_str.strip_prefix("~/"), std::env::var("HOME")) {
            return PathBuf::from(home).join(rest);
        }
        PathBuf::from(path_str)
    } else {
        path.to_path_buf()
    }
}

fn initial_state(
    persisted: Option<DaemonState>,
    configured_wp_dir: Option<&Path>,
    configured_variant: Option<shilpo_ui::theme::SchemeVariant>,
) -> DaemonState {
    let mut state = persisted.unwrap_or_default();
    if state.theme.updated_at == shilpo_ui::theme::state::DEFAULT_TIMESTAMP {
        let now = chrono::Utc::now().to_rfc3339();
        state.theme.updated_at = now.clone();
        state.theme.palette_generated_at = now;
    }
    if let Some(configured_wp_dir) = configured_wp_dir {
        state.wallpaper_dir = expand_tilde(configured_wp_dir);
    } else {
        state.wallpaper_dir = expand_tilde(&state.wallpaper_dir);
    }
    if let Some(variant) = configured_variant {
        state.theme.scheme_variant = variant;
        state.theme.resolved_variant = resolve_variant(state.theme.source_argb, variant);
        let (light, dark) = generate_m3_palettes(state.theme.source_argb, variant);
        state.theme.light = light;
        state.theme.dark = dark;
    }
    state
}

fn extract_wallpaper_seed_and_variant(path: &Path) -> anyhow::Result<(u32, SchemeVariant)> {
    let bytes = std::fs::read(path)?;
    let img = image::load_from_memory(&bytes)?;

    let seed = extract_source_argb_from_image(&img)?;
    let variant = auto_detect_variant(&img);
    Ok((seed, variant))
}

fn extract_source_argb_from_image(img: &DynamicImage) -> anyhow::Result<u32> {
    let resized = img.resize(112, 112, FilterType::Triangle);
    let rgba = resized.to_rgba8();

    let pixels: Vec<u32> = rgba
        .pixels()
        .filter(|p| p.0[3] >= 128)
        .map(|p| 0xff00_0000 | ((p.0[0] as u32) << 16) | ((p.0[1] as u32) << 8) | (p.0[2] as u32))
        .collect();

    if pixels.is_empty() {
        anyhow::bail!("Wallpaper contains no usable pixels");
    }

    let mut color_to_count = QuantizerCelebi::quantize(&pixels, 128);
    color_to_count.retain(|&argb, _| Hct::from_int(argb).chroma() >= 5.0);

    let scored = Score::score(&color_to_count);
    scored
        .first()
        .copied()
        .ok_or_else(|| anyhow::anyhow!("Wallpaper contains no colorful pixels"))
}

fn auto_detect_variant(img: &DynamicImage) -> SchemeVariant {
    let resized = img.resize(256, 256, FilterType::Triangle);
    let rgba = resized.to_rgba8();

    let mut total_sat = 0.0f32;
    let mut hues = Vec::with_capacity(rgba.pixels().len());
    let mut rg_diffs = Vec::with_capacity(rgba.pixels().len());
    let mut yb_diffs = Vec::with_capacity(rgba.pixels().len());

    for p in rgba.pixels() {
        let r = p.0[0] as f32;
        let g = p.0[1] as f32;
        let b = p.0[2] as f32;

        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;

        let sat = if max > 0.0 { delta / max } else { 0.0 };
        total_sat += sat;

        if delta > 0.0 {
            let hue = if (max - r).abs() < f32::EPSILON {
                (g - b) / delta + (if g < b { 6.0 } else { 0.0 })
            } else if (max - g).abs() < f32::EPSILON {
                (b - r) / delta + 2.0
            } else {
                (r - g) / delta + 4.0
            };
            hues.push(hue * 60.0);
        }

        let rg = (r - g).abs();
        let yb = (0.5 * (r + g) - b).abs();
        rg_diffs.push(rg);
        yb_diffs.push(yb);
    }

    let count = rgba.pixels().len() as f32;
    let mean_sat = (total_sat / count) * 255.0;

    let mean_rg = rg_diffs.iter().sum::<f32>() / count;
    let mean_yb = yb_diffs.iter().sum::<f32>() / count;
    let std_rg = (rg_diffs.iter().map(|v| (v - mean_rg).powi(2)).sum::<f32>() / count).sqrt();
    let std_yb = (yb_diffs.iter().map(|v| (v - mean_yb).powi(2)).sum::<f32>() / count).sqrt();
    let colorfulness =
        (std_rg.powi(2) + std_yb.powi(2)).sqrt() + 0.3 * (mean_rg.powi(2) + mean_yb.powi(2)).sqrt();

    let mean_hue = if !hues.is_empty() {
        hues.iter().sum::<f32>() / hues.len() as f32
    } else {
        0.0
    };
    let hue_spread = if !hues.is_empty() {
        (hues.iter().map(|h| (h - mean_hue).powi(2)).sum::<f32>() / hues.len() as f32).sqrt()
    } else {
        0.0
    };

    if mean_sat < 20.0 {
        return SchemeVariant::Monochrome;
    }
    if colorfulness < 30.0 {
        if mean_sat < 55.0 {
            return SchemeVariant::Neutral;
        }
        if hue_spread < 22.0 {
            return SchemeVariant::Content;
        }
        return SchemeVariant::TonalSpot;
    }
    if colorfulness > 90.0 {
        if hue_spread > 55.0 && mean_sat > 150.0 {
            return SchemeVariant::Rainbow;
        }
        if mean_sat > 160.0 {
            return SchemeVariant::Fidelity;
        }
        if hue_spread > 45.0 {
            return SchemeVariant::Expressive;
        }
    }

    SchemeVariant::TonalSpot
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const TEST_NOW: &str = "2026-08-07T09:00:00Z";
    const CUSTOM_SEED: u32 = 0xffaabbcc;
    const WALLPAPER_SEED: u32 = 0xffdd11dd;

    fn apply(state: &mut DaemonState, command: DaemonCommand) -> Result<ApplyOutcome, String> {
        apply_command(state, command, TEST_NOW)
    }

    #[test]
    fn configured_wallpaper_directory_overrides_persisted_snapshot() {
        let persisted = DaemonState {
            wallpaper_dir: "/old/wallpapers".into(),
            ..DaemonState::default()
        };

        let state = initial_state(
            Some(persisted),
            Some(Path::new("/configured/wallpapers")),
            None,
        );

        assert_eq!(state.wallpaper_dir, Path::new("/configured/wallpapers"));
    }

    #[test]
    fn fresh_start_stamps_real_timestamp() {
        let state = initial_state(None, None, None);
        assert_ne!(
            state.theme.updated_at,
            shilpo_ui::theme::state::DEFAULT_TIMESTAMP
        );
        assert_eq!(state.theme.updated_at, state.theme.palette_generated_at);
    }

    #[test]
    fn portal_appearance_updates_resolved_mode_in_system_mode() {
        let mut state = DaemonState::default();
        assert_eq!(state.theme.selected_mode, ThemeMode::System);
        assert_eq!(state.theme.resolved_mode, ThemeMode::Light);
        let revision = state.theme.revision;

        let outcome = apply(
            &mut state,
            DaemonCommand::PortalAppearanceChanged(Some(ThemeMode::Dark)),
        )
        .unwrap();

        assert_eq!(state.theme.selected_mode, ThemeMode::System);
        assert_eq!(state.theme.resolved_mode, ThemeMode::Dark);
        assert_eq!(state.theme.revision, revision + 1);
        assert_eq!(state.theme.updated_at, TEST_NOW);
        assert_eq!(outcome.dispatch_adapter_mode, Some(ThemeMode::Dark));
    }

    #[test]
    fn portal_echo_equal_to_resolution_is_a_noop() {
        let mut state = DaemonState::default();
        let revision = state.theme.revision;

        let outcome = apply(
            &mut state,
            DaemonCommand::PortalAppearanceChanged(Some(ThemeMode::Light)),
        )
        .unwrap();

        assert_eq!(state.theme.resolved_mode, ThemeMode::Light);
        assert_eq!(state.theme.revision, revision);
        assert_eq!(outcome.dispatch_adapter_mode, None);
    }

    #[test]
    fn portal_no_preference_is_a_noop() {
        let mut state = DaemonState::default();
        let revision = state.theme.revision;

        let outcome = apply(&mut state, DaemonCommand::PortalAppearanceChanged(None)).unwrap();

        assert_eq!(state.theme.resolved_mode, ThemeMode::Light);
        assert_eq!(state.theme.revision, revision);
        assert_eq!(outcome.dispatch_adapter_mode, None);
    }

    #[test]
    fn portal_change_is_ignored_when_mode_is_fixed() {
        let mut state = DaemonState::default();
        apply(
            &mut state,
            DaemonCommand::Theme(ThemeCommand::SetMode(ThemeMode::Dark)),
        )
        .unwrap();
        assert_eq!(state.theme.selected_mode, ThemeMode::Dark);
        let revision = state.theme.revision;

        let outcome = apply(
            &mut state,
            DaemonCommand::PortalAppearanceChanged(Some(ThemeMode::Light)),
        )
        .unwrap();

        assert_eq!(state.theme.selected_mode, ThemeMode::Dark);
        assert_eq!(state.theme.resolved_mode, ThemeMode::Dark);
        assert_eq!(state.theme.revision, revision);
        assert_eq!(outcome.dispatch_adapter_mode, None);
    }

    #[test]
    fn fixed_mode_dispatches_adapter() {
        let mut state = DaemonState::default();
        let outcome = apply(
            &mut state,
            DaemonCommand::Theme(ThemeCommand::SetMode(ThemeMode::Dark)),
        )
        .unwrap();

        assert_eq!(state.theme.selected_mode, ThemeMode::Dark);
        assert_eq!(state.theme.resolved_mode, ThemeMode::Dark);
        assert_eq!(outcome.dispatch_adapter_mode, Some(ThemeMode::Dark));
        assert!(outcome.change_kind.mode);
        assert!(!outcome.change_kind.source);
        assert!(!outcome.change_kind.palette);
    }

    #[test]
    fn system_mode_does_not_dispatch_adapter() {
        let mut state = DaemonState::default();
        let revision = state.theme.revision;

        let outcome = apply(
            &mut state,
            DaemonCommand::Theme(ThemeCommand::SetMode(ThemeMode::System)),
        )
        .unwrap();

        assert_eq!(state.theme.selected_mode, ThemeMode::System);
        assert_eq!(state.theme.revision, revision);
        assert_eq!(outcome.dispatch_adapter_mode, None);
    }

    #[test]
    fn pinning_portal_resolved_mode_dispatches_adapter() {
        let mut state = DaemonState {
            theme: ThemeState {
                selected_mode: ThemeMode::System,
                resolved_mode: ThemeMode::Light,
                ..Default::default()
            },
            ..Default::default()
        };

        let outcome = apply(
            &mut state,
            DaemonCommand::Theme(ThemeCommand::SetMode(ThemeMode::Light)),
        )
        .unwrap();

        assert_eq!(state.theme.selected_mode, ThemeMode::Light);
        assert_eq!(outcome.dispatch_adapter_mode, Some(ThemeMode::Light));
    }

    #[test]
    fn toggle_mode_dispatches_resolved_mode() {
        let mut state = DaemonState {
            theme: ThemeState {
                resolved_mode: ThemeMode::Light,
                ..Default::default()
            },
            ..Default::default()
        };

        let outcome = apply(&mut state, DaemonCommand::Theme(ThemeCommand::ToggleMode)).unwrap();

        assert_eq!(state.theme.resolved_mode, ThemeMode::Dark);
        assert_eq!(outcome.dispatch_adapter_mode, Some(ThemeMode::Dark));
    }

    #[test]
    fn switching_to_wallpaper_source_applies_wallpaper_seed() {
        let mut state = DaemonState {
            theme: ThemeState {
                color_source: ColorSource::Custom,
                custom_seed: Some(CUSTOM_SEED),
                source_argb: CUSTOM_SEED,
                ..Default::default()
            },
            wallpaper_seed: Some(WALLPAPER_SEED),
            wallpaper_detected_variant: SchemeVariant::Expressive,
            ..Default::default()
        };
        let revision = state.theme.revision;

        let outcome = apply(
            &mut state,
            DaemonCommand::Theme(ThemeCommand::SetColorSource(ColorSource::Wallpaper)),
        )
        .unwrap();

        assert_eq!(state.theme.color_source, ColorSource::Wallpaper);
        assert_eq!(state.theme.source_argb, WALLPAPER_SEED);
        assert_eq!(state.theme.resolved_variant, SchemeVariant::Expressive);
        assert_eq!(state.theme.palette_generated_at, TEST_NOW);
        assert_eq!(state.theme.revision, revision + 2);
        assert!(outcome.change_kind.source);
        assert!(outcome.change_kind.palette);
        assert!(!outcome.change_kind.mode);
    }

    #[test]
    fn switching_away_and_back_to_wallpaper_keeps_image_aware_variant() {
        let mut state = DaemonState {
            theme: ThemeState {
                color_source: ColorSource::Wallpaper,
                custom_seed: Some(CUSTOM_SEED),
                source_argb: WALLPAPER_SEED,
                ..Default::default()
            },
            wallpaper_seed: Some(WALLPAPER_SEED),
            wallpaper_detected_variant: SchemeVariant::Expressive,
            ..Default::default()
        };

        apply(
            &mut state,
            DaemonCommand::Theme(ThemeCommand::SetColorSource(ColorSource::Custom)),
        )
        .unwrap();
        assert_eq!(state.theme.source_argb, CUSTOM_SEED);
        assert_eq!(
            state.theme.resolved_variant,
            resolve_variant(CUSTOM_SEED, SchemeVariant::Auto)
        );

        apply(
            &mut state,
            DaemonCommand::Theme(ThemeCommand::SetColorSource(ColorSource::Wallpaper)),
        )
        .unwrap();

        assert_eq!(state.theme.color_source, ColorSource::Wallpaper);
        assert_eq!(state.theme.source_argb, WALLPAPER_SEED);
        assert_eq!(state.theme.resolved_variant, SchemeVariant::Expressive);
        assert_eq!(state.theme.scheme_variant, SchemeVariant::Auto);
    }

    #[test]
    fn switching_to_custom_source_without_seed_fails() {
        let mut state = DaemonState::default();
        let result = apply(
            &mut state,
            DaemonCommand::Theme(ThemeCommand::SetColorSource(ColorSource::Custom)),
        );
        assert!(result.is_err());
    }

    #[test]
    fn switching_to_wallpaper_source_without_seed_fails() {
        let mut state = DaemonState::default();
        let result = apply(
            &mut state,
            DaemonCommand::Theme(ThemeCommand::SetColorSource(ColorSource::Wallpaper)),
        );
        assert!(result.is_err());
    }

    #[test]
    fn custom_seed_followed_by_custom_source_regenerates() {
        let mut state = DaemonState::default();
        apply(
            &mut state,
            DaemonCommand::Theme(ThemeCommand::SetCustomSeed(CUSTOM_SEED)),
        )
        .unwrap();
        apply(
            &mut state,
            DaemonCommand::Theme(ThemeCommand::SetColorSource(ColorSource::Custom)),
        )
        .unwrap();

        assert_eq!(state.theme.custom_seed, Some(CUSTOM_SEED));
        assert_eq!(state.theme.color_source, ColorSource::Custom);
        assert_eq!(state.theme.source_argb, CUSTOM_SEED);
    }

    #[test]
    fn wallpaper_directory_updates_state_and_bumps_revision() {
        let mut state = DaemonState::default();
        let revision = state.theme.revision;
        let new_dir = PathBuf::from("/new/wallpapers");

        apply(
            &mut state,
            DaemonCommand::SetWallpaperDirectory(new_dir.clone()),
        )
        .unwrap();

        assert_eq!(state.wallpaper_dir, new_dir);
        assert_eq!(state.theme.revision, revision + 1);
        assert_eq!(state.theme.updated_at, TEST_NOW);
    }

    #[test]
    fn wallpaper_directory_same_path_is_a_noop() {
        let mut state = DaemonState {
            wallpaper_dir: PathBuf::from("/same/dir"),
            ..Default::default()
        };
        let revision = state.theme.revision;

        apply(
            &mut state,
            DaemonCommand::SetWallpaperDirectory(PathBuf::from("/same/dir")),
        )
        .unwrap();

        assert_eq!(state.theme.revision, revision);
    }

    #[test]
    fn wallpaper_sets_path_and_seed_and_bumps_once() {
        let mut state = DaemonState {
            theme: ThemeState {
                color_source: ColorSource::Custom,
                ..Default::default()
            },
            ..Default::default()
        };
        let revision = state.theme.revision;
        let path = PathBuf::from("/w/pic.png");

        let outcome = apply(
            &mut state,
            DaemonCommand::SetWallpaper {
                path: path.clone(),
                seed: WALLPAPER_SEED,
                detected_variant: SchemeVariant::TonalSpot,
            },
        )
        .unwrap();

        assert_eq!(state.wallpaper_path, Some(path));
        assert_eq!(state.wallpaper_seed, Some(WALLPAPER_SEED));
        assert_eq!(state.theme.source_argb, 0xff006c4c);
        assert_eq!(state.theme.revision, revision + 1);
        assert!(outcome.change_kind.wallpaper);
        assert!(!outcome.change_kind.mode);
        assert!(!outcome.change_kind.palette);
    }

    #[test]
    fn wallpaper_seed_is_applied_when_source_is_wallpaper() {
        let mut state = DaemonState::default();
        let revision = state.theme.revision;
        let path = PathBuf::from("/w/pic.png");

        let outcome = apply(
            &mut state,
            DaemonCommand::SetWallpaper {
                path: path.clone(),
                seed: WALLPAPER_SEED,
                detected_variant: SchemeVariant::TonalSpot,
            },
        )
        .unwrap();

        assert_eq!(state.wallpaper_path, Some(path));
        assert_eq!(state.theme.source_argb, WALLPAPER_SEED);
        assert_eq!(state.theme.revision, revision + 1);
        assert_eq!(state.theme.palette_generated_at, TEST_NOW);
        assert!(outcome.change_kind.wallpaper);
        assert!(outcome.change_kind.palette);
    }

    #[test]
    fn switching_scheme_variant_flags_variant_and_palette() {
        let mut state = DaemonState::default();
        let revision = state.theme.revision;

        let outcome = apply(
            &mut state,
            DaemonCommand::Theme(ThemeCommand::SetSchemeVariant(
                shilpo_ui::theme::SchemeVariant::Expressive,
            )),
        )
        .unwrap();

        assert_eq!(
            state.theme.scheme_variant,
            shilpo_ui::theme::SchemeVariant::Expressive
        );
        assert_eq!(state.theme.revision, revision + 1);
        assert!(outcome.change_kind.variant);
        assert!(outcome.change_kind.palette);
        assert!(!outcome.change_kind.mode);
    }

    #[test]
    fn test_auto_detect_variant_decision_branches() {
        use image::{Rgba, RgbaImage};

        fn solid(color: [u8; 3]) -> DynamicImage {
            let mut img = RgbaImage::new(256, 256);
            for pixel in img.pixels_mut() {
                *pixel = Rgba([color[0], color[1], color[2], 255]);
            }
            DynamicImage::ImageRgba8(img)
        }

        fn vertical_bands(colors: &[[u8; 3]]) -> DynamicImage {
            let mut img = RgbaImage::new(256, 256);
            let band = 256 / colors.len();
            for (y, pixel) in img.pixels_mut().enumerate() {
                let row = y / 256;
                let color = colors[(row / band).min(colors.len() - 1)];
                *pixel = Rgba([color[0], color[1], color[2], 255]);
            }
            DynamicImage::ImageRgba8(img)
        }

        // mean_sat < 20 -> Monochrome (gray)
        assert_eq!(
            auto_detect_variant(&solid([128, 128, 128])),
            SchemeVariant::Monochrome
        );
        // low colorfulness + mean_sat < 55 -> Neutral
        assert_eq!(
            auto_detect_variant(&solid([80, 90, 100])),
            SchemeVariant::Neutral
        );
        // low colorfulness + hue_spread < 22 -> Content
        assert_eq!(
            auto_detect_variant(&solid([50, 70, 80])),
            SchemeVariant::Content
        );
        // low colorfulness + wide hue spread -> TonalSpot
        assert_eq!(
            auto_detect_variant(&vertical_bands(&[[50, 70, 80], [80, 70, 50]])),
            SchemeVariant::TonalSpot
        );
        // high colorfulness + wide hue spread + high saturation -> Rainbow
        assert_eq!(
            auto_detect_variant(&vertical_bands(&[[255, 0, 0], [0, 0, 255]])),
            SchemeVariant::Rainbow
        );
        // high colorfulness + high saturation, narrow hue spread -> Fidelity
        assert_eq!(
            auto_detect_variant(&vertical_bands(&[[255, 0, 0], [100, 0, 0]])),
            SchemeVariant::Fidelity
        );
        // high colorfulness + wide hue spread with moderate saturation -> Expressive
        assert_eq!(
            auto_detect_variant(&vertical_bands(&[
                [255, 50, 50],
                [50, 50, 50],
                [50, 255, 50]
            ])),
            SchemeVariant::Expressive
        );
    }

    #[test]
    fn test_wallpaper_materialization_flow_with_synthetic_image() {
        use image::{Rgba, RgbaImage};

        // Synthetic wallpaper (solid blue) drives the full daemon materialization
        // path: file -> seed + detected variant -> SetWallpaper -> state whose
        // palettes match generating with the detected variant explicitly.
        let path = std::env::temp_dir().join(format!(
            "shilpo-wallpaper-materialization-{}.png",
            std::process::id()
        ));
        let mut img = RgbaImage::new(64, 64);
        for pixel in img.pixels_mut() {
            *pixel = Rgba([0, 0, 255, 255]);
        }
        img.save(&path).unwrap();

        let (seed, detected_variant) = extract_wallpaper_seed_and_variant(&path).unwrap();
        assert_eq!(seed, 0xff0000ff);

        let mut state = DaemonState::default();
        let outcome = apply(
            &mut state,
            DaemonCommand::SetWallpaper {
                path: path.clone(),
                seed,
                detected_variant,
            },
        )
        .unwrap();

        assert_eq!(state.theme.scheme_variant, SchemeVariant::Auto);
        assert_eq!(state.theme.resolved_variant, detected_variant);
        assert!(outcome.change_kind.wallpaper);
        assert!(outcome.change_kind.palette);

        let (expected_light, expected_dark) = generate_m3_palettes(seed, detected_variant);
        assert_eq!(state.theme.light, expected_light);
        assert_eq!(state.theme.dark, expected_dark);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn configured_variant_syncs_resolved_variant_on_startup() {
        let state = initial_state(None, None, Some(SchemeVariant::Expressive));

        assert_eq!(state.theme.scheme_variant, SchemeVariant::Expressive);
        assert_eq!(state.theme.resolved_variant, SchemeVariant::Expressive);

        let (expected_light, expected_dark) =
            generate_m3_palettes(state.theme.source_argb, SchemeVariant::Expressive);
        assert_eq!(state.theme.light, expected_light);
        assert_eq!(state.theme.dark, expected_dark);
    }

    #[test]
    fn test_wallpaper_materialization_with_detected_variant_preserves_auto() {
        let mut state = DaemonState::default();
        let path = PathBuf::from("/w/pic.png");
        assert_eq!(state.theme.scheme_variant, SchemeVariant::Auto);

        let _outcome = apply(
            &mut state,
            DaemonCommand::SetWallpaper {
                path: path.clone(),
                seed: WALLPAPER_SEED,
                detected_variant: SchemeVariant::Expressive,
            },
        )
        .unwrap();

        assert_eq!(state.theme.scheme_variant, SchemeVariant::Auto);
        assert_eq!(state.theme.resolved_variant, SchemeVariant::Expressive);
    }

    #[test]
    fn test_wallpaper_materialization_honors_explicit_pin() {
        let mut state = DaemonState::default();
        state.theme.scheme_variant = SchemeVariant::TonalSpot;
        let path = PathBuf::from("/w/pic.png");

        let _outcome = apply(
            &mut state,
            DaemonCommand::SetWallpaper {
                path: path.clone(),
                seed: WALLPAPER_SEED,
                detected_variant: SchemeVariant::Expressive,
            },
        )
        .unwrap();

        assert_eq!(state.theme.scheme_variant, SchemeVariant::TonalSpot);
        assert_eq!(state.theme.resolved_variant, SchemeVariant::TonalSpot);
    }

    #[tokio::test]
    async fn test_9_analysis_cold_miss_invokes_decoder_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let file = tempfile::NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();

        let decode_count = Arc::new(AtomicUsize::new(0));
        let decode_count_clone = decode_count.clone();

        let options = ThemeDaemonOptions {
            wallpaper_extractor: Some(Arc::new(move |_| {
                decode_count_clone.fetch_add(1, Ordering::SeqCst);
                Ok((0xff112233, SchemeVariant::Expressive))
            })),
            wallpaper_backend: Some(Arc::new(|_| Ok(()))),
            headless: true,
            ..Default::default()
        };

        let mut daemon = ThemeDaemon::with_options(options).await.unwrap();

        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        daemon.spawn_wallpaper_task(path.clone(), reply_tx);

        let res = daemon.wallpaper_result_rx.recv().await.unwrap();
        daemon.handle_wallpaper_completion(res).await;

        let state = reply_rx.await.unwrap().unwrap();
        assert_eq!(state.wallpaper_seed, Some(0xff112233));
        assert_eq!(decode_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_10_analysis_second_request_hits_cache() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let file = tempfile::NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();

        let decode_count = Arc::new(AtomicUsize::new(0));
        let decode_count_clone = decode_count.clone();

        let options = ThemeDaemonOptions {
            wallpaper_extractor: Some(Arc::new(move |_| {
                decode_count_clone.fetch_add(1, Ordering::SeqCst);
                Ok((0xff112233, SchemeVariant::Expressive))
            })),
            wallpaper_backend: Some(Arc::new(|_| Ok(()))),
            headless: true,
            ..Default::default()
        };

        let mut daemon = ThemeDaemon::with_options(options).await.unwrap();

        // 1st request (miss)
        let (tx1, rx1) = tokio::sync::oneshot::channel();
        daemon.spawn_wallpaper_task(path.clone(), tx1);
        let res1 = daemon.wallpaper_result_rx.recv().await.unwrap();
        daemon.handle_wallpaper_completion(res1).await;
        let _ = rx1.await.unwrap().unwrap();

        assert_eq!(decode_count.load(Ordering::SeqCst), 1);

        // 2nd request (hit)
        let (tx2, rx2) = tokio::sync::oneshot::channel();
        daemon.spawn_wallpaper_task(path.clone(), tx2);
        let res2 = daemon.wallpaper_result_rx.recv().await.unwrap();
        daemon.handle_wallpaper_completion(res2).await;
        let _ = rx2.await.unwrap().unwrap();

        assert_eq!(
            decode_count.load(Ordering::SeqCst),
            1,
            "Decoder must not be called on cache hit"
        );
    }

    #[tokio::test]
    async fn test_11_analysis_mtime_invalidation() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::SystemTime;

        let file = tempfile::NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();

        let decode_count = Arc::new(AtomicUsize::new(0));
        let decode_count_clone = decode_count.clone();

        let options = ThemeDaemonOptions {
            wallpaper_extractor: Some(Arc::new(move |_| {
                let count = decode_count_clone.fetch_add(1, Ordering::SeqCst);
                if count == 0 {
                    Ok((0xff111111, SchemeVariant::Expressive))
                } else {
                    Ok((0xff222222, SchemeVariant::Fidelity))
                }
            })),
            wallpaper_backend: Some(Arc::new(|_| Ok(()))),
            headless: true,
            ..Default::default()
        };

        let mut daemon = ThemeDaemon::with_options(options).await.unwrap();

        // Request 1
        let (tx1, rx1) = tokio::sync::oneshot::channel();
        daemon.spawn_wallpaper_task(path.clone(), tx1);
        let res1 = daemon.wallpaper_result_rx.recv().await.unwrap();
        daemon.handle_wallpaper_completion(res1).await;
        let state1 = rx1.await.unwrap().unwrap();
        assert_eq!(state1.wallpaper_seed, Some(0xff111111));
        assert_eq!(decode_count.load(Ordering::SeqCst), 1);

        // Modify file mtime deterministically without sleep
        let f = std::fs::File::open(&path).unwrap();
        let new_mtime = SystemTime::now() + std::time::Duration::from_secs(100);
        let _ = f.set_modified(new_mtime);

        // Request 2 (mtime changed -> miss)
        let (tx2, rx2) = tokio::sync::oneshot::channel();
        daemon.spawn_wallpaper_task(path.clone(), tx2);
        let res2 = daemon.wallpaper_result_rx.recv().await.unwrap();
        daemon.handle_wallpaper_completion(res2).await;
        let state2 = rx2.await.unwrap().unwrap();
        assert_eq!(state2.wallpaper_seed, Some(0xff222222));
        assert_eq!(decode_count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_12_analysis_variant_change_invalidation() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let file = tempfile::NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();

        let decode_count = Arc::new(AtomicUsize::new(0));
        let decode_count_clone = decode_count.clone();

        let options = ThemeDaemonOptions {
            wallpaper_extractor: Some(Arc::new(move |_| {
                decode_count_clone.fetch_add(1, Ordering::SeqCst);
                Ok((0xff112233, SchemeVariant::Expressive))
            })),
            wallpaper_backend: Some(Arc::new(|_| Ok(()))),
            headless: true,
            ..Default::default()
        };

        let mut daemon = ThemeDaemon::with_options(options).await.unwrap();

        // Request 1 under default variant (Auto)
        let (tx1, rx1) = tokio::sync::oneshot::channel();
        daemon.spawn_wallpaper_task(path.clone(), tx1);
        let res1 = daemon.wallpaper_result_rx.recv().await.unwrap();
        daemon.handle_wallpaper_completion(res1).await;
        let _ = rx1.await.unwrap().unwrap();
        assert_eq!(decode_count.load(Ordering::SeqCst), 1);

        // Change scheme variant on daemon
        let _ = daemon
            .process_command(DaemonCommand::Theme(ThemeCommand::SetSchemeVariant(
                SchemeVariant::Fidelity,
            )))
            .await;

        // Request 2 under new variant -> miss
        let (tx2, rx2) = tokio::sync::oneshot::channel();
        daemon.spawn_wallpaper_task(path.clone(), tx2);
        let res2 = daemon.wallpaper_result_rx.recv().await.unwrap();
        daemon.handle_wallpaper_completion(res2).await;
        let _ = rx2.await.unwrap().unwrap();
        assert_eq!(
            decode_count.load(Ordering::SeqCst),
            2,
            "Variant change must invalidate cache"
        );
    }

    #[tokio::test]
    async fn test_13_analysis_failure_not_cached() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let file = tempfile::NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();

        let decode_count = Arc::new(AtomicUsize::new(0));
        let decode_count_clone = decode_count.clone();

        let options = ThemeDaemonOptions {
            wallpaper_extractor: Some(Arc::new(move |_| {
                let count = decode_count_clone.fetch_add(1, Ordering::SeqCst);
                if count == 0 {
                    Err(anyhow::anyhow!("Analysis failed"))
                } else {
                    Ok((0xff999999, SchemeVariant::TonalSpot))
                }
            })),
            wallpaper_backend: Some(Arc::new(|_| Ok(()))),
            headless: true,
            ..Default::default()
        };

        let mut daemon = ThemeDaemon::with_options(options).await.unwrap();

        // Request 1 fails
        let (tx1, rx1) = tokio::sync::oneshot::channel();
        daemon.spawn_wallpaper_task(path.clone(), tx1);
        let res1 = daemon.wallpaper_result_rx.recv().await.unwrap();
        daemon.handle_wallpaper_completion(res1).await;
        let err1 = rx1.await.unwrap();
        assert!(err1.is_err());
        assert_eq!(decode_count.load(Ordering::SeqCst), 1);

        // Request 2 retries decoder and succeeds
        let (tx2, rx2) = tokio::sync::oneshot::channel();
        daemon.spawn_wallpaper_task(path.clone(), tx2);
        let res2 = daemon.wallpaper_result_rx.recv().await.unwrap();
        daemon.handle_wallpaper_completion(res2).await;
        let state2 = rx2.await.unwrap().unwrap();
        assert_eq!(state2.wallpaper_seed, Some(0xff999999));
        assert_eq!(decode_count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_14_backend_failure_retains_cached_analysis() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

        let file = tempfile::NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();

        let decode_count = Arc::new(AtomicUsize::new(0));
        let decode_count_clone = decode_count.clone();
        let backend_fail = Arc::new(AtomicBool::new(true));
        let backend_fail_clone = backend_fail.clone();

        let options = ThemeDaemonOptions {
            wallpaper_extractor: Some(Arc::new(move |_| {
                decode_count_clone.fetch_add(1, Ordering::SeqCst);
                Ok((0xff334455, SchemeVariant::TonalSpot))
            })),
            wallpaper_backend: Some(Arc::new(move |_| {
                if backend_fail_clone.load(Ordering::SeqCst) {
                    Err("awww failed".into())
                } else {
                    Ok(())
                }
            })),
            headless: true,
            ..Default::default()
        };

        let mut daemon = ThemeDaemon::with_options(options).await.unwrap();

        // Request 1: analysis succeeds, backend fails
        let (tx1, rx1) = tokio::sync::oneshot::channel();
        daemon.spawn_wallpaper_task(path.clone(), tx1);
        let res1 = daemon.wallpaper_result_rx.recv().await.unwrap();
        daemon.handle_wallpaper_completion(res1).await;
        let err1 = rx1.await.unwrap();
        assert!(err1.is_err());
        assert_eq!(decode_count.load(Ordering::SeqCst), 1);

        // Fix backend failure
        backend_fail.store(false, Ordering::SeqCst);

        // Request 2: analysis hits cache, backend succeeds
        let (tx2, rx2) = tokio::sync::oneshot::channel();
        daemon.spawn_wallpaper_task(path.clone(), tx2);
        let res2 = daemon.wallpaper_result_rx.recv().await.unwrap();
        daemon.handle_wallpaper_completion(res2).await;
        let state2 = rx2.await.unwrap().unwrap();
        assert_eq!(state2.wallpaper_seed, Some(0xff334455));
        assert_eq!(
            decode_count.load(Ordering::SeqCst),
            1,
            "Cached analysis retained despite backend error"
        );
    }

    #[tokio::test]
    async fn test_15_cache_hit_produces_identical_state() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();

        let options = ThemeDaemonOptions {
            wallpaper_extractor: Some(Arc::new(|_| Ok((0xff778899, SchemeVariant::Expressive)))),
            wallpaper_backend: Some(Arc::new(|_| Ok(()))),
            headless: true,
            ..Default::default()
        };

        let mut daemon = ThemeDaemon::with_options(options).await.unwrap();

        let (tx1, rx1) = tokio::sync::oneshot::channel();
        daemon.spawn_wallpaper_task(path.clone(), tx1);
        let res1 = daemon.wallpaper_result_rx.recv().await.unwrap();
        daemon.handle_wallpaper_completion(res1).await;
        let state1 = rx1.await.unwrap().unwrap();

        let (tx2, rx2) = tokio::sync::oneshot::channel();
        daemon.spawn_wallpaper_task(path.clone(), tx2);
        let res2 = daemon.wallpaper_result_rx.recv().await.unwrap();
        daemon.handle_wallpaper_completion(res2).await;
        let state2 = rx2.await.unwrap().unwrap();

        assert_eq!(state1.wallpaper_seed, state2.wallpaper_seed);
        assert_eq!(
            state1.wallpaper_detected_variant,
            state2.wallpaper_detected_variant
        );
        assert_eq!(state1.theme.light, state2.theme.light);
        assert_eq!(state1.theme.dark, state2.theme.dark);
    }

    #[tokio::test]
    async fn test_16_superseded_request_populates_cache_without_state_mutation() {
        let file1 = tempfile::NamedTempFile::new().unwrap();
        let file2 = tempfile::NamedTempFile::new().unwrap();
        let path1 = file1.path().to_path_buf();
        let path2 = file2.path().to_path_buf();

        let options = ThemeDaemonOptions {
            wallpaper_extractor: Some(Arc::new(|p| {
                if p.to_string_lossy().contains("tmp") {
                    Ok((0xff111111, SchemeVariant::Expressive))
                } else {
                    Ok((0xff222222, SchemeVariant::TonalSpot))
                }
            })),
            wallpaper_backend: Some(Arc::new(|_| Ok(()))),
            headless: true,
            ..Default::default()
        };

        let mut daemon = ThemeDaemon::with_options(options).await.unwrap();

        let (tx1, rx1) = tokio::sync::oneshot::channel();
        let (tx2, rx2) = tokio::sync::oneshot::channel();

        // Spawn op 1, then op 2 (superseding op 1)
        daemon.spawn_wallpaper_task(path1.clone(), tx1);
        daemon.spawn_wallpaper_task(path2.clone(), tx2);

        // Receive completions
        let r1 = daemon.wallpaper_result_rx.recv().await.unwrap();
        let r2 = daemon.wallpaper_result_rx.recv().await.unwrap();

        let (res1, res2) = if r1.op_id < r2.op_id {
            (r1, r2)
        } else {
            (r2, r1)
        };

        // Op 1 completed task, but op_id (1) < current_op (2)
        daemon.handle_wallpaper_completion(res1).await;
        let err1 = rx1.await.unwrap();
        assert!(err1.is_err());
        assert_eq!(err1.unwrap_err(), "Wallpaper request superseded");

        // Op 2 completes
        daemon.handle_wallpaper_completion(res2).await;
        let state2 = rx2.await.unwrap().unwrap();
        assert_eq!(state2.wallpaper_path, Some(path2.clone()));

        // Op 1 path should be cached despite being superseded!
        let (canonical1, key1) = crate::wallpaper_cache::create_wallpaper_cache_key(
            &path1,
            daemon.state.theme.scheme_variant,
        )
        .unwrap();
        let cached = crate::wallpaper_cache::with_cache(&daemon.wallpaper_cache, |c| c.get(&key1));
        assert!(
            cached.is_some(),
            "Superseded op analysis must be present in cache"
        );
        assert_eq!(canonical1, key1.canonical_path);
    }

    #[tokio::test]
    #[ignore]
    async fn wallpaper_cache_cold_vs_hit_benchmark() {
        use image::{Rgba, RgbaImage};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Instant;

        let temp_dir = tempfile::tempdir().unwrap();
        let img_path = temp_dir.path().join("bench_wallpaper.png");

        let mut img = RgbaImage::new(1920, 1080);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            let r = (x % 256) as u8;
            let g = (y % 256) as u8;
            let b = ((x + y) % 256) as u8;
            *pixel = Rgba([r, g, b, 255]);
        }
        img.save(&img_path).unwrap();

        let decode_count = Arc::new(AtomicUsize::new(0));
        let decode_count_clone = decode_count.clone();

        let options = ThemeDaemonOptions {
            wallpaper_extractor: Some(Arc::new(move |p| {
                decode_count_clone.fetch_add(1, Ordering::SeqCst);
                extract_wallpaper_seed_and_variant(p)
            })),
            wallpaper_backend: Some(Arc::new(|_| Ok(()))),
            headless: true,
            ..Default::default()
        };

        let mut daemon = ThemeDaemon::with_options(options).await.unwrap();

        // 1. Cold analysis
        let cold_start = Instant::now();
        let (tx_cold, rx_cold) = tokio::sync::oneshot::channel();
        daemon.spawn_wallpaper_task(img_path.clone(), tx_cold);
        let res_cold = daemon.wallpaper_result_rx.recv().await.unwrap();
        daemon.handle_wallpaper_completion(res_cold).await;
        let _ = rx_cold.await.unwrap().unwrap();
        let cold_duration = cold_start.elapsed();

        assert_eq!(
            decode_count.load(Ordering::SeqCst),
            1,
            "Decoder should run exactly once during cold analysis"
        );

        // 2. 100 Cache hit lookups
        let hit_start = Instant::now();
        let hit_count = 100;
        for _ in 0..hit_count {
            let (tx_hit, rx_hit) = tokio::sync::oneshot::channel();
            daemon.spawn_wallpaper_task(img_path.clone(), tx_hit);
            let res_hit = daemon.wallpaper_result_rx.recv().await.unwrap();
            daemon.handle_wallpaper_completion(res_hit).await;
            let _ = rx_hit.await.unwrap().unwrap();
        }
        let hit_duration = hit_start.elapsed();
        let mean_hit_duration = hit_duration / hit_count;
        let ratio = cold_duration.as_secs_f64() / mean_hit_duration.as_secs_f64();

        assert_eq!(
            decode_count.load(Ordering::SeqCst),
            1,
            "Decoder count must remain 1 after 100 hits"
        );

        println!("\n=== Wallpaper Cache Cold vs Hit Benchmark Results ===");
        println!("Cold duration:      {:?}", cold_duration);
        println!("Total hit duration: {:?}", hit_duration);
        println!("Mean hit duration:  {:?}", mean_hit_duration);
        println!("Speedup ratio:      {:.2}x", ratio);
        println!(
            "Decoder invocations: {}",
            decode_count.load(Ordering::SeqCst)
        );
        println!("====================================================\n");
    }
}
