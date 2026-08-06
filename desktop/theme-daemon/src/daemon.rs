use crate::adapters::{DesktopAdapter, select_desktop_adapter};
use crate::dbus::{ActorMessage, ThemeDbusService};
use crate::persistence::{read_state_snapshot, write_state_snapshot};
use crate::portal::PortalObserver;
use anyhow::Result;
use image::imageops::FilterType;
use mcu_material_color::{Hct, QuantizerCelebi, Score};
use shilpo_config::ShellConfig;
use shilpo_theme::{SideEffect, ThemeCommand, ThemeMode, ThemeState, generate_m3_palettes, reduce};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;
use tracing::{debug, info};
use zbus::Connection;
use zbus::connection::Builder;
use zbus::names::BusName;

pub struct WallpaperTaskResult {
    pub op_id: u64,
    pub path: PathBuf,
    pub seed: u32,
    pub error: Option<String>,
    pub reply: tokio::sync::oneshot::Sender<Result<ThemeState, String>>,
}

pub struct ThemeDaemon {
    state: ThemeState,
    adapter: Arc<dyn DesktopAdapter>,
    config_path: PathBuf,
    wallpaper_dir: PathBuf,
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

        let wallpaper_dir = initial_state.wallpaper_dir.clone();
        let daemon = Self {
            state: initial_state,
            adapter,
            config_path,
            wallpaper_dir,
            actor_rx,
            portal_rx,
            wallpaper_result_tx: wp_tx,
            wallpaper_result_rx: wp_rx,
            current_wallpaper_op: Arc::new(AtomicU64::new(0)),
            _conn: conn.clone(),
        };

        let _ = write_state_snapshot(&daemon.state);
        if let Ok(raw_state) = serde_json::to_string(&daemon.state) {
            let _ = conn
                .emit_signal(
                    Option::<BusName>::None,
                    "/org/shilpo/Theme",
                    "org.shilpo.Theme",
                    "StateChanged",
                    &raw_state,
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
                        .process_command(ThemeCommand::PortalAppearanceChanged(portal_mode))
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
                    "revision": self.state.revision,
                    "selected_mode": self.state.selected_mode,
                    "resolved_mode": self.state.resolved_mode,
                    "color_source": self.state.color_source,
                    "source_argb": format!("#{:08X}", self.state.source_argb),
                    "adapter": self.adapter.name(),
                    "wallpaper_dir": self.wallpaper_dir,
                });
                let _ = reply.send(diag.to_string());
            }
            ActorMessage::SetMode(mode, reply) => {
                let _ = reply.send(self.process_command(ThemeCommand::SetMode(mode)).await);
            }
            ActorMessage::ToggleMode(reply) => {
                let _ = reply.send(self.process_command(ThemeCommand::ToggleMode).await);
            }
            ActorMessage::SetColorSource(source, reply) => {
                let _ = reply.send(
                    self.process_command(ThemeCommand::SetColorSource(source))
                        .await,
                );
            }
            ActorMessage::SetSchemeVariant(variant, reply) => {
                let res = self
                    .process_command(ThemeCommand::SetSchemeVariant(variant))
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
                    self.process_command(ThemeCommand::SetCustomSeed(seed))
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
                        .process_command(ThemeCommand::SetWallpaperDirectory(dir.clone()))
                        .await;
                    match result {
                        Ok(state) => {
                            self.wallpaper_dir = state.wallpaper_dir.clone();
                            self.persist_wallpaper_directory_config(&dir)
                                .map(|_| Ok(state))
                                .unwrap_or_else(Err)
                        }
                        Err(error) => Err(error),
                    }
                } else {
                    Err(format!(
                        "Wallpaper directory does not exist: {}",
                        dir.display()
                    ))
                };
                if let Ok(state) = &result {
                    self.wallpaper_dir = state.wallpaper_dir.clone();
                }
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
        reply: tokio::sync::oneshot::Sender<Result<ThemeState, String>>,
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
            .process_command(ThemeCommand::SetWallpaper {
                path: res.path,
                seed: res.seed,
            })
            .await;
        let _ = res.reply.send(result);
    }

    async fn process_command(&mut self, command: ThemeCommand) -> Result<ThemeState, String> {
        if let ThemeCommand::SetColorSource(source) = &command {
            let available = match source {
                shilpo_theme::ColorSource::Custom => self.state.custom_seed.is_some(),
                shilpo_theme::ColorSource::Wallpaper => self.state.wallpaper_seed.is_some(),
            };
            if !available {
                return Err(format!("No seed is available for color source {source:?}"));
            }
        }

        let mut next_state = self.state.clone();
        let prev_rev = next_state.revision;
        let now = chrono::Utc::now().to_rfc3339();
        let effects = reduce(&mut next_state, command, &now);

        if next_state.revision > prev_rev {
            self.state = next_state;

            let raw_state =
                serde_json::to_string(&self.state).map_err(|error| error.to_string())?;
            let _ = self
                ._conn
                .emit_signal(
                    Option::<BusName>::None,
                    "/org/shilpo/Theme",
                    "org.shilpo.Theme",
                    "StateChanged",
                    &raw_state,
                )
                .await;

            let state_to_save = self.state.clone();
            tokio::task::spawn_blocking(move || {
                let _ = write_state_snapshot(&state_to_save);
            });
        }

        for effect in effects {
            match effect {
                SideEffect::DispatchDesktopAdapter(mode) => {
                    let adapter = Arc::clone(&self.adapter);
                    tokio::task::spawn_blocking(move || {
                        if let Err(error) = adapter.set_mode(mode) {
                            tracing::warn!(%error, provider = adapter.name(), "Desktop adapter set_mode failed");
                        }
                    });
                }
            }
        }

        Ok(self.state.clone())
    }

    fn sync_wallpaper_dir_from_config(&mut self) {
        if let Ok(cfg) = ShellConfig::load_or_create(&self.config_path) {
            let config_dir = expand_tilde(&cfg.desktop.wallpaper_dir);
            if config_dir != self.wallpaper_dir {
                debug!(
                    old = %self.wallpaper_dir.display(),
                    new = %config_dir.display(),
                    "Syncing daemon wallpaper_dir from config.toml"
                );
                self.wallpaper_dir = config_dir.clone();
                self.state.wallpaper_dir = config_dir;
            }
        }
    }

    fn pick_random_wallpaper(&mut self) -> Result<PathBuf, String> {
        self.sync_wallpaper_dir_from_config();
        let mut wallpapers = Vec::new();
        let entries = std::fs::read_dir(&self.wallpaper_dir).map_err(|error| {
            format!(
                "Cannot read wallpaper directory {}: {error}",
                self.wallpaper_dir.display()
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
                self.wallpaper_dir.display()
            ));
        }

        let idx = (chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default() as usize)
            % wallpapers.len();
        Ok(wallpapers[idx].clone())
    }
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
    persisted: Option<ThemeState>,
    configured_wp_dir: Option<&Path>,
    configured_variant: Option<shilpo_theme::SchemeVariant>,
) -> ThemeState {
    let mut state = persisted.unwrap_or_default();
    if let Some(configured_wp_dir) = configured_wp_dir {
        state.wallpaper_dir = expand_tilde(configured_wp_dir);
    } else {
        state.wallpaper_dir = expand_tilde(&state.wallpaper_dir);
    }
    if let Some(variant) = configured_variant {
        state.scheme_variant = variant;
        let (light, dark) = generate_m3_palettes(state.source_argb, variant);
        state.light = light;
        state.dark = dark;
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
    use super::initial_state;
    use shilpo_theme::ThemeState;
    use std::path::Path;

    #[test]
    fn configured_wallpaper_directory_overrides_persisted_snapshot() {
        let persisted = ThemeState {
            wallpaper_dir: "/old/wallpapers".into(),
            ..ThemeState::default()
        };

        let state = initial_state(
            Some(persisted),
            Some(Path::new("/configured/wallpapers")),
            None,
        );

        assert_eq!(state.wallpaper_dir, Path::new("/configured/wallpapers"));
    }
}
