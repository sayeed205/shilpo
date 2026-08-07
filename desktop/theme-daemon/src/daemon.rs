use crate::adapters::{DesktopAdapter, select_desktop_adapter};
use crate::dbus::{ActorMessage, ThemeDbusService};
use crate::persistence::{read_state_snapshot, write_state_snapshot};
use crate::portal::PortalObserver;
use anyhow::Result;
use image::imageops::FilterType;
use mcu_material_color::{Hct, QuantizerCelebi, Score};
use serde::{Deserialize, Serialize};
use shilpo_config::ShellConfig;
use shilpo_theme::{ColorSource, ThemeCommand, ThemeMode, ThemeState, generate_m3_palettes, reduce};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;
use tracing::{debug, info};
use zbus::Connection;
use zbus::connection::Builder;
use zbus::names::BusName;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonState {
    #[serde(flatten)]
    pub theme: ThemeState,
    pub wallpaper_path: Option<PathBuf>,
    pub wallpaper_seed: Option<u32>,
    pub wallpaper_dir: PathBuf,
}

impl DaemonState {
    pub fn new(timestamp: &str) -> Self {
        Self {
            theme: ThemeState::new(timestamp),
            wallpaper_path: None,
            wallpaper_seed: None,
            wallpaper_dir: PathBuf::from("~/Pictures/Wallpapers"),
        }
    }
}

impl Default for DaemonState {
    fn default() -> Self {
        Self::new(shilpo_theme::state::DEFAULT_TIMESTAMP)
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
/// unchanged to `shilpo_theme::reduce`; wallpaper and portal concerns stay at
/// this (daemon) layer, never leaking into `core/theme`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonCommand {
    Theme(ThemeCommand),
    SetWallpaperDirectory(PathBuf),
    SetWallpaper { path: PathBuf, seed: u32 },
    PortalAppearanceChanged(Option<ThemeMode>),
}

pub struct WallpaperTaskResult {
    pub op_id: u64,
    pub path: PathBuf,
    pub seed: u32,
    pub error: Option<String>,
    pub reply: tokio::sync::oneshot::Sender<Result<DaemonState, String>>,
}

pub struct ThemeDaemon {
    state: DaemonState,
    adapter: Arc<dyn DesktopAdapter>,
    config_path: PathBuf,
    actor_rx: mpsc::UnboundedReceiver<ActorMessage>,
    portal_rx: mpsc::UnboundedReceiver<Option<ThemeMode>>,
    wallpaper_result_tx: mpsc::UnboundedSender<WallpaperTaskResult>,
    wallpaper_result_rx: mpsc::UnboundedReceiver<WallpaperTaskResult>,
    current_wallpaper_op: Arc<AtomicU64>,
    _conn: Connection,
}

impl ThemeDaemon {
    pub async fn new() -> Result<Self> {
        let (actor_tx, actor_rx) = mpsc::unbounded_channel();
        let (portal_tx, portal_rx) = mpsc::unbounded_channel();
        let (wp_tx, wp_rx) = mpsc::unbounded_channel();

        let config_path = shilpo_config::default_config_path();

        let (
            config_provider,
            gtk_light,
            gtk_dark,
            custom_argv,
            configured_wp_dir,
            configured_variant,
        ) = match ShellConfig::load_or_create(&config_path) {
            Ok(cfg) => (
                cfg.theme.provider,
                cfg.theme.gtk_theme_light,
                cfg.theme.gtk_theme_dark,
                cfg.theme.custom_adapter_cmd,
                Some(cfg.desktop.wallpaper_dir),
                cfg.theme
                    .scheme_variant
                    .as_deref()
                    .map(shilpo_theme::SchemeVariant::from_str),
            ),
            Err(_) => (None, None, None, None, None, None),
        };

        let adapter: Arc<dyn DesktopAdapter> = Arc::from(select_desktop_adapter(
            config_provider.as_deref(),
            gtk_light,
            gtk_dark,
            custom_argv,
        ));

        let initial_state = initial_state(
            read_state_snapshot(),
            configured_wp_dir.as_deref(),
            configured_variant,
        );

        let service = ThemeDbusService::new(actor_tx);
        let conn = Builder::session()?
            .name("org.shilpo.Theme")?
            .serve_at("/org/shilpo/Theme", service)?
            .build()
            .await?;

        info!("shilpo-themed registered D-Bus name org.shilpo.Theme at /org/shilpo/Theme");

        PortalObserver::start(portal_tx).await;

        let daemon = Self {
            state: initial_state,
            adapter,
            config_path,
            actor_rx,
            portal_rx,
            wallpaper_result_tx: wp_tx,
            wallpaper_result_rx: wp_rx,
            current_wallpaper_op: Arc::new(AtomicU64::new(0)),
            _conn: conn.clone(),
        };

        let _ = write_state_snapshot(&daemon.state);
        let startup_update = ThemeUpdate {
            state: daemon.state.clone(),
            change_kind: ChangeKind::full(),
        };
        if let Ok(raw_update) = serde_json::to_string(&startup_update) {
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
        match msg {
            ActorMessage::GetState(reply) => {
                self.sync_wallpaper_dir_from_config();
                let _ = reply.send(Ok(self.state.clone()));
            }
            ActorMessage::GetDiagnostics(reply) => {
                self.sync_wallpaper_dir_from_config();
                let diag = serde_json::json!({
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
                let _ = reply.send(
                    self.process_command(DaemonCommand::Theme(ThemeCommand::SetMode(mode)))
                        .await,
                );
            }
            ActorMessage::ToggleMode(reply) => {
                let _ = reply.send(
                    self.process_command(DaemonCommand::Theme(ThemeCommand::ToggleMode))
                        .await,
                );
            }
            ActorMessage::SetColorSource(source, reply) => {
                let _ = reply.send(
                    self.process_command(DaemonCommand::Theme(ThemeCommand::SetColorSource(source)))
                        .await,
                );
            }
            ActorMessage::SetSchemeVariant(variant, reply) => {
                let res = self
                    .process_command(DaemonCommand::Theme(ThemeCommand::SetSchemeVariant(variant)))
                    .await;
                let config_path = self.config_path.clone();
                tokio::task::spawn_blocking(move || {
                    if let Ok(mut config) = ShellConfig::load_or_create(&config_path) {
                        config.theme.scheme_variant = Some(variant.as_str().to_string());
                        let _ = config.save(&config_path);
                    }
                });
                let _ = reply.send(res);
            }
            ActorMessage::SetCustomSeed(seed, reply) => {
                let _ = reply.send(
                    self.process_command(DaemonCommand::Theme(ThemeCommand::SetCustomSeed(seed)))
                        .await,
                );
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
                let _ = reply.send(result);
            }
            ActorMessage::SetRandomWallpaper(reply) => match self.pick_random_wallpaper() {
                Ok(path) => self.spawn_wallpaper_task(path, reply),
                Err(error) => {
                    let _ = reply.send(Err(error));
                }
            },
        }
    }

    fn persist_wallpaper_directory_config(&self, dir: &Path) -> Result<(), String> {
        let mut config = ShellConfig::load_or_create(&self.config_path)
            .map_err(|error| format!("Failed to load shell config: {error}"))?;
        config.desktop.wallpaper_dir = dir.to_path_buf();
        config
            .save(&self.config_path)
            .map_err(|error| format!("Failed to persist wallpaper directory: {error}"))
    }

    fn spawn_wallpaper_task(
        &mut self,
        path: PathBuf,
        reply: tokio::sync::oneshot::Sender<Result<DaemonState, String>>,
    ) {
        if !path.exists() {
            let _ = reply.send(Err(format!(
                "Wallpaper path does not exist: {}",
                path.display()
            )));
            return;
        }
        let op_id = self.current_wallpaper_op.fetch_add(1, Ordering::SeqCst) + 1;
        let tx = self.wallpaper_result_tx.clone();

        tokio::spawn(async move {
            info!(op_id, path = %path.display(), "Starting background wallpaper processing");

            let seed = tokio::task::spawn_blocking({
                let path = path.clone();
                move || extract_seed_from_file(&path)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result.map_err(|error| error.to_string()));

            let seed = match seed {
                Ok(seed) => seed,
                Err(error) => {
                    let _ = tx.send(WallpaperTaskResult {
                        op_id,
                        path,
                        seed: 0,
                        error: Some(error),
                        reply,
                    });
                    return;
                }
            };

            let awww_result = tokio::task::spawn_blocking({
                let path = path.clone();
                move || {
                    std::process::Command::new("awww")
                        .arg("img")
                        .arg(&path)
                        .status()
                }
            })
            .await;

            let error = match awww_result {
                Ok(Ok(status)) if status.success() => None,
                Ok(Ok(status)) => Some(format!("awww exited with status {status}")),
                Ok(Err(error)) => Some(format!("Failed to execute awww: {error}")),
                Err(error) => Some(format!("Wallpaper backend task failed: {error}")),
            };

            debug!(
                op_id,
                success = error.is_none(),
                "Wallpaper awww invocation completed"
            );
            let _ = tx.send(WallpaperTaskResult {
                op_id,
                path,
                seed,
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
            })
            .await;
        let _ = res.reply.send(result);
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
            let raw_update =
                serde_json::to_string(&update).map_err(|error| error.to_string())?;
            let _ = self
                ._conn
                .emit_signal(
                    Option::<BusName>::None,
                    "/org/shilpo/Theme",
                    "org.shilpo.Theme",
                    "StateChanged",
                    &raw_update,
                )
                .await;

            let state_to_save = self.state.clone();
            tokio::task::spawn_blocking(move || {
                let _ = write_state_snapshot(&state_to_save);
            });
        }

        if let Some(mode) = outcome.dispatch_adapter_mode {
            let adapter = Arc::clone(&self.adapter);
            tokio::task::spawn_blocking(move || {
                if let Err(error) = adapter.set_mode(mode) {
                    tracing::warn!(%error, provider = adapter.name(), "Desktop adapter set_mode failed");
                }
            });
        }

        Ok(self.state.clone())
    }

    fn sync_wallpaper_dir_from_config(&mut self) {
        if let Ok(cfg) = ShellConfig::load_or_create(&self.config_path) {
            let config_dir = expand_tilde(&cfg.desktop.wallpaper_dir);
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
/// transitions are delegated to `shilpo_theme::reduce`; wallpaper and portal
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
                    reduce(&mut state.theme, ThemeCommand::SetSeed(seed), now);
                }
            }
            ThemeCommand::SetSchemeVariant(variant) => {
                reduce(&mut state.theme, ThemeCommand::SetSchemeVariant(variant), now);
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
        DaemonCommand::SetWallpaper { path, seed } => {
            let mut changed = false;
            if state.wallpaper_path.as_ref() != Some(&path) {
                state.wallpaper_path = Some(path);
                changed = true;
            }
            if state.wallpaper_seed != Some(seed) {
                state.wallpaper_seed = Some(seed);
                changed = true;
            }
            let seed_applied = reduce(&mut state.theme, ThemeCommand::SetSeed(seed), now);
            if changed && !seed_applied {
                bump_revision(state, now);
            }
        }
        DaemonCommand::PortalAppearanceChanged(portal_mode) => {
            if let Some(pm) = portal_mode {
                debug_assert!(pm != ThemeMode::System);
                if state.theme.selected_mode == ThemeMode::System
                    && state.theme.resolved_mode != pm
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
    scheme_variant: shilpo_theme::SchemeVariant,
    source_argb: u32,
    wallpaper_path: Option<PathBuf>,
    wallpaper_seed: Option<u32>,
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
    configured_variant: Option<shilpo_theme::SchemeVariant>,
) -> DaemonState {
    let mut state = persisted.unwrap_or_default();
    if state.theme.updated_at == shilpo_theme::state::DEFAULT_TIMESTAMP {
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
        let (light, dark) = generate_m3_palettes(state.theme.source_argb, variant);
        state.theme.light = light;
        state.theme.dark = dark;
    }
    state
}

fn extract_seed_from_file(path: &Path) -> anyhow::Result<u32> {
    let bytes = std::fs::read(path)?;
    let img = image::load_from_memory(&bytes)?;

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
            shilpo_theme::state::DEFAULT_TIMESTAMP
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

        let outcome = apply(
            &mut state,
            DaemonCommand::Theme(ThemeCommand::ToggleMode),
        )
        .unwrap();

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
        assert_eq!(state.theme.palette_generated_at, TEST_NOW);
        assert_eq!(state.theme.revision, revision + 2);
        assert!(outcome.change_kind.source);
        assert!(outcome.change_kind.palette);
        assert!(!outcome.change_kind.mode);
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
                shilpo_theme::SchemeVariant::Expressive,
            )),
        )
        .unwrap();

        assert_eq!(state.theme.scheme_variant, shilpo_theme::SchemeVariant::Expressive);
        assert_eq!(state.theme.revision, revision + 1);
        assert!(outcome.change_kind.variant);
        assert!(outcome.change_kind.palette);
        assert!(!outcome.change_kind.mode);
    }
}
