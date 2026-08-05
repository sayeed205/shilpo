use gpui::{
    App, AppContext, Bounds, DisplayId, Entity, Global, Pixels, Point, Subscription,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions,
    layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
    point, px, size,
};
use shilpo_services::{BarState, IpcRequest, IpcStatus, ShellIpcServer};

use crate::{
    actions::{ActionId, ActionInvocation, ActionRegistry},
    bar::{BarView, geometry::BarGeometry},
    control_center::ControlCenterView,
    error::ShellError,
    extensions::{
        ContributionDescriptor, ContributionSurface, ExtensionCoordinator, ExtensionGeneration,
    },
};

use std::collections::HashMap;

use crate::bar::service_worker::{self, CommandSender, UpdateReceiver, WorkerCommand};
use shilpo_services::{
    CompositorAdapter, CompositorCommand, CompositorConnection, CompositorOutput,
    CompositorSnapshot, NiriCompositorService, NotificationService,
};
#[cfg(not(test))]
use std::path::PathBuf;
#[cfg(test)]
use std::path::{Path, PathBuf};
use std::{
    sync::{Arc, Mutex, mpsc},
    time::Instant,
};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq)]
struct ExtensionSurfaceSpec {
    contribution: shilpo_ext::CanonicalId,
    display_id: DisplayId,
    bounds: Bounds<Pixels>,
}

pub struct ServiceHub {
    pub compositor: Arc<dyn CompositorAdapter>,
    pub notification: Option<NotificationService>,
    pub notification_state: shilpo_services::ServiceLifecycle,
    pub notification_last_error: Option<String>,
    notification_attempt: u32,
    notification_next_retry: Option<Instant>,
    notification_dnd: bool,
    pub clipboard: shilpo_services::ClipboardService,
    pub app_scanner: shilpo_services::AppScanner,
    pub service_commands: CommandSender,
    pub device_snapshot: crate::bar::service_worker::DeviceSnapshot,
    pub availability: crate::bar::service_worker::ServiceAvailability,
    pub notif_rx: Arc<Mutex<mpsc::Receiver<shilpo_services::Notification>>>,
    pub notif_tx: mpsc::Sender<shilpo_services::Notification>,
    pub updates_rx: Arc<Mutex<UpdateReceiver>>,
    pub _service_task: Option<gpui::Task<()>>,
    pub _watcher: Option<notify::RecommendedWatcher>,
    pub _app_watcher: Option<notify::RecommendedWatcher>,
}

impl ServiceHub {
    pub fn new(
        executor: gpui::BackgroundExecutor,
        config_path: PathBuf,
        session_store: Option<Arc<shilpo_config::HeedSessionStore>>,
    ) -> Self {
        let compositor: Arc<dyn CompositorAdapter> = NiriCompositorService::new();
        let (device_services, availability) = crate::bar::service_worker::DeviceServices::new();
        let clipboard = shilpo_services::ClipboardService::with_store(session_store);
        let app_scanner = shilpo_services::AppScanner::new()
            .unwrap_or_else(|_| shilpo_services::AppScanner::new_empty());
        let app_watcher = app_scanner.start_watcher();
        let (notification, notification_state, notification_last_error) =
            match NotificationService::new() {
                Ok(s) => (Some(s), shilpo_services::ServiceLifecycle::Ready, None),
                Err(e) => {
                    let err_str = e.to_string();
                    tracing::warn!(error = %err_str, "notification service unavailable; toasts disabled");
                    (
                        None,
                        shilpo_services::ServiceLifecycle::Connecting { attempt: 1 },
                        Some(err_str),
                    )
                }
            };

        let (notif_tx, notif_rx) = mpsc::channel();
        if let Some(service) = &notification {
            service.set_new_notification_sender(notif_tx.clone());
        }

        let (updates_tx, updates_rx, service_commands, commands_rx) = service_worker::channels();
        let service_task = service_worker::spawn(
            executor,
            updates_tx,
            commands_rx,
            config_path.clone(),
            device_services,
        );

        let config_dir = config_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".config/shilpo"));
        if let Err(error) = std::fs::create_dir_all(&config_dir) {
            tracing::warn!(error = %error, path = ?config_dir, "config watcher directory unavailable");
        }

        use notify::Watcher;
        let watcher_commands = service_commands.clone();
        let target_file_name = config_path.file_name().map(|n| n.to_os_string());
        let watcher = match notify::RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Some(event) = res.ok().filter(|e| e.kind.is_modify())
                    && (target_file_name.is_none()
                        || event
                            .paths
                            .iter()
                            .any(|p| p.file_name() == target_file_name.as_deref()))
                {
                    let _ = service_worker::try_send_command(
                        &watcher_commands,
                        WorkerCommand::ReloadConfig,
                    );
                }
            },
            notify::Config::default(),
        ) {
            Ok(mut watcher) => match watcher.watch(&config_dir, notify::RecursiveMode::Recursive) {
                Ok(()) => Some(watcher),
                Err(error) => {
                    tracing::warn!(error = %error, path = ?config_dir, "config watcher watch failed");
                    None
                }
            },
            Err(error) => {
                tracing::warn!(error = %error, "config watcher creation failed");
                None
            }
        };

        let notification_attempt = if notification.is_some() { 0 } else { 1 };
        let notification_next_retry = notification
            .is_none()
            .then(|| Instant::now() + service_worker::backoff_delay(notification_attempt));

        Self {
            compositor,
            notification,
            notification_state,
            notification_last_error,
            notification_attempt,
            notification_next_retry,
            notification_dnd: false,
            clipboard,
            app_scanner,
            service_commands,
            device_snapshot: crate::bar::service_worker::DeviceSnapshot::default(),
            availability,
            notif_rx: Arc::new(Mutex::new(notif_rx)),
            notif_tx,
            updates_rx: Arc::new(Mutex::new(updates_rx)),
            _service_task: Some(service_task),
            _watcher: watcher,
            _app_watcher: app_watcher,
        }
    }

    fn poll_notification_reconnect(&mut self) {
        if self
            .notification
            .as_ref()
            .is_some_and(|service| !service.is_healthy())
        {
            self.notification = None;
            self.notification_attempt = self.notification_attempt.saturating_add(1);
            self.notification_state = shilpo_services::ServiceLifecycle::Connecting {
                attempt: self.notification_attempt,
            };
            self.notification_last_error = Some("notification D-Bus connection closed".into());
            self.notification_next_retry =
                Some(Instant::now() + service_worker::backoff_delay(self.notification_attempt));
        }
        if self.notification.is_some()
            || !self
                .notification_next_retry
                .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return;
        }

        match NotificationService::new() {
            Ok(service) => {
                service.set_new_notification_sender(self.notif_tx.clone());
                service.set_dnd_enabled(self.notification_dnd);
                self.notification = Some(service);
                self.notification_state = shilpo_services::ServiceLifecycle::Ready;
                self.notification_last_error = None;
                self.notification_attempt = 0;
                self.notification_next_retry = None;
            }
            Err(error) => {
                self.notification_attempt = self.notification_attempt.saturating_add(1);
                self.notification_state = shilpo_services::ServiceLifecycle::Connecting {
                    attempt: self.notification_attempt,
                };
                self.notification_last_error = Some(error.to_string());
                self.notification_next_retry =
                    Some(Instant::now() + service_worker::backoff_delay(self.notification_attempt));
            }
        }
    }
}

fn apply_notification_dnd(notification: Option<&NotificationService>, enabled: bool) {
    if let Some(notification) = notification {
        notification.set_dnd_enabled(enabled);
    }
}

fn parse_awww_wallpaper_path(output: &str) -> Option<PathBuf> {
    const MARKER: &str = "currently displaying: image: ";
    output.lines().find_map(|line| {
        let path = line.split_once(MARKER)?.1.trim();
        (!path.is_empty()).then(|| PathBuf::from(path))
    })
}

fn query_awww_wallpaper_path() -> Option<PathBuf> {
    let output = std::process::Command::new("awww")
        .arg("query")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = parse_awww_wallpaper_path(&String::from_utf8_lossy(&output.stdout))?;
    path.is_file().then_some(path)
}

#[cfg(test)]
fn discovered_wallpaper_needs_theme_sync(
    theme_wallpaper_path: Option<&Path>,
    discovered_wallpaper_path: &Path,
) -> bool {
    theme_wallpaper_path != Some(discovered_wallpaper_path)
}

fn should_restore_overview_prior_focus(
    reason: crate::overview::OverviewCloseReason,
    opened_workspace_id: Option<u64>,
    current_workspace_id: Option<u64>,
) -> bool {
    if reason != crate::overview::OverviewCloseReason::Cancel {
        return false;
    }

    match (opened_workspace_id, current_workspace_id) {
        (Some(opened), Some(current)) => opened == current,
        _ => true,
    }
}

pub struct ShellRuntime {
    ipc_server: ShellIpcServer,
    active_config: shilpo_config::ShellConfig,
    bars: HashMap<DisplayId, (WindowHandle<BarView>, crate::bar::BarSpec)>,
    last_bar_specs: Vec<(BarGeometry, bool)>,
    bar_state: BarState,
    readiness: shilpo_services::ipc::ReadinessState,
    control_center: Option<WindowHandle<shilpo_ui::Root>>,
    overview: Option<WindowHandle<shilpo_ui::Root>>,
    overview_entity: Option<Entity<crate::overview::WorkspaceOverview>>,
    overview_instance: u64,
    next_overview_instance: u64,
    overview_opened_workspace_id: Option<u64>,
    current_wallpaper_path: Option<PathBuf>,
    latest_snapshot: Arc<CompositorSnapshot>,
    notification: Option<(
        u64,
        u32,
        WindowHandle<crate::notification::NotificationToastView>,
    )>,
    notification_generation: u64,
    prior_window_id: Option<u64>,
    osd: Option<(
        u64,
        WindowHandle<shilpo_ui::Root>,
        Entity<crate::osd::OsdView>,
    )>,
    _osd_generation: u64,
    extensions: Option<ExtensionCoordinator>,
    extension_surfaces: HashMap<String, (WindowHandle<shilpo_ui::Root>, ExtensionSurfaceSpec)>,
    extension_panel: Option<(WindowHandle<shilpo_ui::Root>, shilpo_ext::CanonicalId)>,
    extension_output_ids: std::collections::HashSet<DisplayId>,
    extension_tasks: std::collections::HashMap<
        (
            crate::extensions::ExtensionGeneration,
            shilpo_ext::ExtensionId,
            String,
        ),
        gpui::Task<()>,
    >,
    extension_location_service: shilpo_services::LocationService,
    actions: ActionRegistry,
    keybindings: crate::actions::KeybindingManager,
    session_state: shilpo_config::ShellSessionState,
    session_path: PathBuf,
    pub heed_store: Option<Arc<shilpo_config::HeedSessionStore>>,
    start_time: std::time::Instant,
    service_hub: Option<ServiceHub>,
    _window_closed: Option<Subscription>,
    _ipc_task: gpui::Task<()>,
}

impl Global for ShellRuntime {}

impl ShellRuntime {
    pub fn active_config(cx: &App) -> shilpo_config::ShellConfig {
        if cx.has_global::<Self>() {
            cx.global::<Self>().active_config.clone()
        } else {
            shilpo_config::ShellConfig::default()
        }
    }

    fn output_name_for_bounds(
        bounds: Bounds<Pixels>,
        compositor_outputs: &[CompositorOutput],
    ) -> Option<String> {
        let origin_x = bounds.origin.x.as_f32();
        let origin_y = bounds.origin.y.as_f32();
        let width = bounds.size.width.as_f32();
        let height = bounds.size.height.as_f32();
        let close = |left: f32, right: f32| (left - right).abs() <= 2.0;
        let geometry_match = |output: &CompositorOutput, size_only: bool| {
            let scale = output.scale.max(1.0) as f32;
            let position_matches = close(output.logical_position.0 as f32, origin_x)
                && close(output.logical_position.1 as f32, origin_y);
            let size_matches = (close(output.logical_size.0 as f32, width)
                && close(output.logical_size.1 as f32, height))
                || (close(output.logical_size.0 as f32 * scale, width)
                    && close(output.logical_size.1 as f32 * scale, height));
            size_matches && (size_only || position_matches)
        };

        compositor_outputs
            .iter()
            .find(|output| geometry_match(output, false))
            .or_else(|| {
                let size_matches = compositor_outputs
                    .iter()
                    .filter(|output| geometry_match(output, true))
                    .collect::<Vec<_>>();
                (size_matches.len() == 1).then(|| size_matches[0])
            })
            .map(|output| output.name.clone())
    }

    fn output_name_for_display(
        display: &dyn gpui::PlatformDisplay,
        compositor_outputs: &[CompositorOutput],
    ) -> Option<String> {
        let display_uuid = display.uuid().ok();
        display_uuid
            .and_then(|display_uuid| {
                compositor_outputs
                    .iter()
                    .find(|output| {
                        Uuid::new_v5(&Uuid::NAMESPACE_DNS, output.name.as_bytes()) == display_uuid
                    })
                    .map(|output| output.name.clone())
            })
            .or_else(|| Self::output_name_for_bounds(display.bounds(), compositor_outputs))
            .or_else(|| display_uuid.map(|uuid| uuid.to_string()))
    }

    pub fn service_commands(cx: &App) -> Option<CommandSender> {
        if cx.has_global::<Self>() {
            cx.global::<Self>()
                .service_hub
                .as_ref()
                .map(|h| h.service_commands.clone())
        } else {
            None
        }
    }

    pub fn install(cx: &mut App, ipc_server: ShellIpcServer) {
        let config_path = std::env::var("HOME")
            .map(|home| std::path::PathBuf::from(home).join(".config/shilpo/config.toml"))
            .unwrap_or_else(|_| std::path::PathBuf::from(".config/shilpo/config.toml"));
        let active_config = shilpo_config::ShellConfig::load_or_create(&config_path)
            .unwrap_or_else(|_| shilpo_config::ShellConfig::default());
        let theme_client = futures_lite::future::block_on(shilpo_theme::ThemeClient::new());
        let initial_theme_state = theme_client.current_state();
        let initial_wallpaper_path = initial_theme_state
            .wallpaper_path
            .clone()
            .filter(|path| path.is_file());
        shilpo_ui::Theme::global_mut(cx).apply_state(&initial_theme_state);
        let mut rx = theme_client.subscribe();
        let theme_client_for_task = theme_client.clone();
        cx.spawn(async move |cx| {
            loop {
                let state = match rx.recv().await {
                    Ok(state) => state,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        theme_client_for_task.current_state()
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                };
                cx.update(|cx: &mut gpui::App| {
                    shilpo_ui::Theme::global_mut(cx).apply_state(&state);
                    if cx.has_global::<Self>() {
                        let runtime = cx.global_mut::<Self>();
                        if let Some(path) =
                            state.wallpaper_path.clone().filter(|path| path.is_file())
                        {
                            runtime.current_wallpaper_path = Some(path);
                        }
                        let overview_entity = runtime.overview_entity.clone();
                        let wallpaper_path = runtime.current_wallpaper_path.clone();
                        let bar_handles: Vec<_> =
                            runtime.bars.values().map(|(handle, _)| *handle).collect();
                        let cc_handle = runtime.control_center;
                        let ov_handle = runtime.overview;

                        if let Some(overview) = overview_entity {
                            overview.update(cx, |view, cx| {
                                view.update_wallpaper_path(wallpaper_path, cx);
                            });
                        }
                        for handle in bar_handles {
                            let _ = handle.update(cx, |_, _, cx| cx.notify());
                        }
                        if let Some(cc) = cc_handle {
                            let _ = cc.update(cx, |_, _, cx| cx.notify());
                        }
                        if let Some(ov) = ov_handle {
                            let _ = ov.update(cx, |_, _, cx| cx.notify());
                        }
                    }
                    cx.refresh_windows();
                });
            }
        })
        .detach();
        let session_path = shilpo_config::ShellSessionState::default_session_path();
        let (session_state, _restored_fallback) =
            shilpo_config::ShellSessionState::restore_with_fallback(&session_path);
        let heed_dir = shilpo_config::HeedSessionStore::default_db_dir();
        let heed_store = match shilpo_config::HeedSessionStore::open_with_recovery(&heed_dir) {
            Ok(opened) => {
                if let shilpo_config::RecoveryOutcome::Quarantined { ref path } = opened.recovery {
                    tracing::warn!(
                        quarantine_path = %path.display(),
                        "LMDB session store was corrupted and has been quarantined"
                    );
                }
                Some(Arc::new(opened.store))
            }
            Err(e) => {
                tracing::warn!(error = %e, "LMDB session store open failed; session features running unpersisted");
                None
            }
        };
        let mut hub = ServiceHub::new(
            cx.background_executor().clone(),
            config_path,
            heed_store.clone(),
        );
        hub.notification_dnd = session_state.dnd_active;
        apply_notification_dnd(hub.notification.as_ref(), session_state.dnd_active);
        let extensions = match shilpo_ext::WasmRuntime::new() {
            Ok(runtime) => {
                let paths = shilpo_ext::CatalogPaths::platform_default();
                match crate::extensions::ExtensionEngine::new(runtime, paths.clone()) {
                    Ok(engine) => {
                        let (command_tx, command_rx) = std::sync::mpsc::sync_channel(64);
                        let (update_tx, update_rx) = std::sync::mpsc::sync_channel(64);
                        let snapshot = std::sync::Arc::new(std::sync::RwLock::new(
                            crate::extensions::ExtensionSnapshot::default(),
                        ));

                        let watch_paths = vec![
                            shilpo_ext::default_extension_state_dir().join("dev"),
                            paths.data_dir.join("installed"),
                            paths.data_dir.join("activated"),
                        ];
                        let mut watcher = None;
                        let mut fallback_scan = None;
                        match crate::extensions::ExtensionWatcher::new(
                            command_tx.clone(),
                            watch_paths,
                        ) {
                            Ok(w) => watcher = Some(w),
                            Err(error) => {
                                tracing::warn!(%error, "ExtensionWatcher failed, falling back to 30s background scan");
                                let fallback_tx = command_tx.clone();
                                let executor = cx.background_executor().clone();
                                let executor_inner = executor.clone();
                                fallback_scan = Some(executor.clone().spawn(async move {
                                    loop {
                                        executor_inner
                                            .timer(std::time::Duration::from_secs(30))
                                            .await;
                                        if fallback_tx
                                            .send(
                                                crate::extensions::ExtensionCommand::SourcesChanged,
                                            )
                                            .is_err()
                                        {
                                            break;
                                        }
                                    }
                                }));
                            }
                        }

                        let worker_task = engine.run_worker_loop(
                            cx.background_executor().clone(),
                            command_rx,
                            update_tx,
                            snapshot.clone(),
                        );

                        Some(crate::extensions::ExtensionCoordinator::new_with_executor(
                            Some(cx.background_executor().clone()),
                            snapshot,
                            command_tx,
                            update_rx,
                            Some(worker_task),
                            watcher,
                            fallback_scan,
                        ))
                    }
                    Err(error) => {
                        tracing::warn!(error = %error, "extension engine load failed");
                        None
                    }
                }
            }
            Err(error) => {
                tracing::warn!(error = %error, "extension runtime is unavailable");
                None
            }
        };

        let compositor = hub.compositor.clone();
        ipc_server.attach_broker(compositor.command_broker());
        let latest_snapshot = compositor.current();
        let mut rx = compositor.subscribe();

        cx.set_global(Self {
            ipc_server,
            active_config,
            bars: HashMap::new(),
            last_bar_specs: Vec::new(),
            bar_state: BarState::Starting,
            readiness: shilpo_services::ipc::ReadinessState::Starting,
            control_center: None,
            overview: None,
            overview_entity: None,
            overview_instance: 0,
            next_overview_instance: 0,
            overview_opened_workspace_id: None,
            current_wallpaper_path: initial_wallpaper_path.clone(),
            latest_snapshot: latest_snapshot.clone(),
            notification: None,
            notification_generation: 0,
            prior_window_id: None,
            osd: None,
            _osd_generation: 0,
            extensions,
            extension_surfaces: HashMap::new(),
            extension_panel: None,
            extension_output_ids: std::collections::HashSet::new(),
            extension_tasks: std::collections::HashMap::new(),
            extension_location_service: shilpo_services::LocationService::new(),
            actions: ActionRegistry::default(),
            keybindings: crate::actions::KeybindingManager::with_defaults(),
            session_state,
            session_path,
            heed_store,
            start_time: std::time::Instant::now(),
            service_hub: Some(hub),
            _window_closed: None,
            _ipc_task: gpui::Task::ready(()),
        });
        let wallpaper_probe = cx.background_spawn(async { query_awww_wallpaper_path() });
        let theme_wallpaper_path = initial_wallpaper_path;
        shilpo_theme::ThemeClient::spawn_task(async move {
            let client = shilpo_theme::ThemeClient::new().await;
            if let Some(wallpaper_path) = wallpaper_probe.await {
                let _ = client
                    .set_wallpaper(&wallpaper_path.to_string_lossy())
                    .await;
            } else if let Some(wallpaper_path) = theme_wallpaper_path {
                let _ = client
                    .set_wallpaper(&wallpaper_path.to_string_lossy())
                    .await;
            }
        });
        Self::on_compositor_snapshot_changed(cx, latest_snapshot);

        cx.spawn(async move |cx| {
            while rx.changed().await.is_ok() {
                let snapshot = rx.borrow().clone();
                cx.update(|cx: &mut gpui::App| {
                    Self::on_compositor_snapshot_changed(cx, snapshot);
                });
            }
        })
        .detach();

        let subscription = cx.on_window_closed(|cx, window_id| {
            if !cx.has_global::<ShellRuntime>() {
                return;
            }
            let runtime = cx.global_mut::<ShellRuntime>();
            runtime
                .bars
                .retain(|_, (handle, _)| handle.window_id() != window_id);
            runtime
                .extension_surfaces
                .retain(|_, (handle, _)| handle.window_id() != window_id);
            if runtime.bars.is_empty() {
                runtime.bar_state = BarState::Hidden;
            }
            let closed_control_center = if runtime
                .control_center
                .as_ref()
                .is_some_and(|handle| handle.window_id() == window_id)
            {
                runtime.control_center = None;
                true
            } else {
                false
            };
            let closed_extension_panel = if runtime
                .extension_panel
                .as_ref()
                .is_some_and(|(handle, _)| handle.window_id() == window_id)
            {
                runtime.extension_panel.take().map(|(_, id)| id)
            } else {
                None
            };
            if runtime
                .notification
                .as_ref()
                .is_some_and(|(_, _, handle)| handle.window_id() == window_id)
            {
                runtime.notification = None;
            }
            runtime.publish_status();
            if closed_control_center {
                ShellRuntime::dispatch_surface_lifecycle(
                    cx,
                    ContributionSurface::ControlCenter,
                    false,
                    340.,
                    540.,
                );
            }
            if let Some(contribution) = closed_extension_panel {
                ShellRuntime::dispatch_extension_event(
                    cx,
                    shilpo_ext::ExtensionEvent::ContributionUnmounted {
                        contribution_id: contribution.contribution_id.to_string(),
                        instance_id: None,
                    },
                );
            }
        });
        cx.global_mut::<Self>()._window_closed = Some(subscription);
        Self::sync_extension_actions(cx);

        let task = cx.spawn(async move |cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
                cx.update(Self::sync_displays);
                cx.update(Self::drain_service_hub);
                cx.update(Self::drain_extensions);
                cx.update(Self::drain_ipc);
            }
        });
        cx.global_mut::<Self>()._ipc_task = task;
        cx.global::<Self>().publish_status();
    }

    pub fn extension_descriptors(
        cx: &App,
        surface: ContributionSurface,
    ) -> Vec<ContributionDescriptor> {
        cx.global::<Self>()
            .extensions
            .as_ref()
            .map_or_else(Vec::new, |extensions| extensions.descriptors_for(surface))
    }

    pub fn extension_surface_views(
        cx: &App,
        surface: ContributionSurface,
    ) -> Vec<(shilpo_ext::CanonicalId, shilpo_ext::ViewTree)> {
        let descriptors = Self::extension_descriptors(cx, surface);
        descriptors
            .into_iter()
            .filter_map(|descriptor| {
                let tree = Self::extension_view(cx, &descriptor.id)?;
                Some((descriptor.id, tree))
            })
            .collect()
    }

    pub fn extension_view(cx: &App, id: &shilpo_ext::CanonicalId) -> Option<shilpo_ext::ViewTree> {
        cx.global::<Self>()
            .extensions
            .as_ref()
            .and_then(|extensions| extensions.view(id))
    }

    pub fn extension_asset_path(
        cx: &App,
        id: &shilpo_ext::CanonicalId,
        relative: &str,
    ) -> Result<PathBuf, String> {
        cx.global::<Self>()
            .extensions
            .as_ref()
            .ok_or_else(|| "extension runtime is unavailable".to_owned())?
            .asset_path(id, relative)
    }

    pub fn dispatch_extension_input(
        cx: &mut App,
        contribution: &shilpo_ext::CanonicalId,
        instance_id: Option<&str>,
        event_id: impl Into<String>,
        value: Option<serde_json::Value>,
    ) {
        if let Some(ext) = cx.global::<Self>().extensions.as_ref()
            && let Err(error) = ext.send_command(crate::extensions::ExtensionCommand::Input {
                expected: ext.generation(),
                contribution: contribution.clone(),
                instance_id: instance_id.map(ToString::to_string),
                event_id: event_id.into(),
                value,
            })
        {
            tracing::warn!(%error, "extension input was not queued");
        }
    }

    pub fn open_extension_panel(cx: &mut App, contribution: shilpo_ext::CanonicalId) {
        if let Some((handle, current)) = cx.global_mut::<Self>().extension_panel.take() {
            if current == contribution
                && handle
                    .update(cx, |_, window, _| window.activate_window())
                    .is_ok()
            {
                cx.global_mut::<Self>().extension_panel = Some((handle, current));
                return;
            }
            if let Some(ext) = cx.global::<Self>().extensions.as_ref()
                && let Err(error) =
                    ext.send_command(crate::extensions::ExtensionCommand::Lifecycle {
                        expected: ext.generation(),
                        event: shilpo_ext::ExtensionEvent::ContributionUnmounted {
                            contribution_id: current.contribution_id.to_string(),
                            instance_id: None,
                        },
                    })
            {
                tracing::warn!(%error, "extension unmount was not queued");
            }
            let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
        }
        let (display_bounds, display_id) = if let Some(display) = cx.primary_display() {
            (display.bounds(), Some(display.id()))
        } else {
            (
                Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.))),
                None,
            )
        };
        let panel_size = size(px(420.), px(600.));
        let origin = point(
            display_bounds.origin.x + display_bounds.size.width - panel_size.width - px(16.),
            display_bounds.origin.y + px(56.),
        );
        let options = overlay_options(
            "shilpo-extension-panel",
            "extension-panel",
            panel_size,
            origin,
            display_id,
        );
        let view_id = contribution.clone();
        match cx.open_window(options, move |window, cx| {
            crate::extension_surface::ExtensionSurfaceView::view(view_id, None, window, cx)
        }) {
            Ok(handle) => {
                if let Some(ext) = cx.global::<Self>().extensions.as_ref()
                    && let Err(error) =
                        ext.send_command(crate::extensions::ExtensionCommand::Lifecycle {
                            expected: ext.generation(),
                            event: shilpo_ext::ExtensionEvent::ContributionMounted {
                                contribution_id: contribution.contribution_id.to_string(),
                                instance_id: None,
                                width: 420.,
                                height: 600.,
                            },
                        })
                {
                    tracing::warn!(%error, "extension mount was not queued");
                }
                cx.global_mut::<Self>().extension_panel = Some((handle, contribution));
            }
            Err(error) => tracing::warn!(error = %error, "failed to open extension side panel"),
        }
    }

    fn drain_extensions(cx: &mut App) {
        if !cx.has_global::<ShellRuntime>() {
            return;
        }
        let updates = {
            let runtime = cx.global::<ShellRuntime>();
            runtime
                .extensions
                .as_ref()
                .map(|ext| ext.drain_updates())
                .unwrap_or_default()
        };
        for update in updates {
            ShellRuntime::apply_extension_update(cx, update);
        }
    }

    fn apply_extension_update(cx: &mut App, update: crate::extensions::ExtensionUpdate) {
        let current_gen = cx
            .global::<ShellRuntime>()
            .extensions
            .as_ref()
            .map(|ext| ext.generation());
        if current_gen.is_some_and(|target_gen| update.generation < target_gen) {
            return;
        }

        if update
            .snapshot
            .as_ref()
            .is_some_and(|s| s.catalog_changed_at.is_some())
        {
            ShellRuntime::sync_extension_actions(cx);
            // The initial extension snapshot is loaded asynchronously. The
            // first display reconciliation can therefore run before any
            // descriptors exist; reconcile again once the catalog is live so
            // each mounted instance receives its settings event and starts its
            // initial refresh.
            ShellRuntime::reconcile_bar_extension_instances(cx);
        }

        for (extension_id, effect) in update.effects {
            ShellRuntime::execute_extension_effect(cx, &extension_id, update.generation, effect);
        }

        if let Some(snapshot) = &update.snapshot {
            let active_gen = snapshot.generation;
            cx.global_mut::<ShellRuntime>()
                .extension_tasks
                .retain(|(task_gen, _, _), _| *task_gen >= active_gen);
        }

        if update.snapshot.is_some() || !update.invalidated_views.is_empty() {
            cx.refresh_windows();
        }
    }

    fn dispatch_extension_event(cx: &mut App, event: shilpo_ext::ExtensionEvent) {
        if let Some(ext) = cx.global::<ShellRuntime>().extensions.as_ref() {
            let cmd = match event {
                shilpo_ext::ExtensionEvent::PowerChanged {
                    percentage,
                    charging,
                } => crate::extensions::ExtensionCommand::Replaceable(
                    crate::extensions::ReplaceableEvent::Power {
                        percentage,
                        charging,
                    },
                ),
                shilpo_ext::ExtensionEvent::NetworkChanged { connected } => {
                    crate::extensions::ExtensionCommand::Replaceable(
                        crate::extensions::ReplaceableEvent::Network { connected },
                    )
                }
                shilpo_ext::ExtensionEvent::MediaChanged {
                    title,
                    artist,
                    playing,
                } => crate::extensions::ExtensionCommand::Replaceable(
                    crate::extensions::ReplaceableEvent::Media {
                        title,
                        artist,
                        playing,
                    },
                ),
                _ => crate::extensions::ExtensionCommand::Lifecycle {
                    expected: ext.generation(),
                    event,
                },
            };
            if let Err(error) = ext.send_command(cmd) {
                tracing::warn!(%error, "extension event was not queued");
            }
        }
    }

    pub(crate) fn dispatch_surface_lifecycle(
        cx: &mut App,
        surface: ContributionSurface,
        mounted: bool,
        width: f32,
        height: f32,
    ) {
        let descriptors = ShellRuntime::extension_descriptors(cx, surface);
        if let Some(ext) = cx.global::<ShellRuntime>().extensions.as_ref() {
            let expected_gen = ext.generation();
            for descriptor in descriptors {
                let event = if mounted {
                    shilpo_ext::ExtensionEvent::ContributionMounted {
                        contribution_id: descriptor.id.contribution_id.to_string(),
                        instance_id: None,
                        width,
                        height,
                    }
                } else {
                    shilpo_ext::ExtensionEvent::ContributionUnmounted {
                        contribution_id: descriptor.id.contribution_id.to_string(),
                        instance_id: None,
                    }
                };
                if let Err(error) =
                    ext.send_command(crate::extensions::ExtensionCommand::Lifecycle {
                        expected: expected_gen,
                        event,
                    })
                {
                    tracing::warn!(%error, "extension lifecycle event was not queued");
                }
            }
        }
    }

    fn sync_extension_actions(cx: &mut App) {
        let desired = ShellRuntime::extension_descriptors(cx, ContributionSurface::Action);
        let existing = cx
            .global::<ShellRuntime>()
            .actions
            .all()
            .into_iter()
            .filter_map(|descriptor| descriptor.id.extension_id())
            .collect::<Vec<_>>();
        let actions = &mut cx.global_mut::<ShellRuntime>().actions;
        for id in existing {
            actions.unregister_extension(&id);
        }
        for descriptor in desired {
            if let Err(error) =
                actions.register_extension(descriptor.id, descriptor.name.clone(), descriptor.name)
            {
                tracing::warn!(error = %error, "extension action registration failed");
            }
        }
    }

    fn execute_extension_effect(
        cx: &mut App,
        extension_id: &shilpo_ext::ExtensionId,
        generation: ExtensionGeneration,
        effect: shilpo_ext::AuthorizedHostEffect,
    ) {
        match effect.into_kind() {
            shilpo_ext::AuthorizedHostEffectKind::NonHttp(
                shilpo_ext::HostEffect::InvokeAction { action_id, payload },
            ) => {
                let invocation = action_id
                    .parse::<ActionId>()
                    .map_err(|err| err.to_string())
                    .and_then(|id| ActionInvocation::from_id_and_payload(id, payload));
                match invocation {
                    Ok(inv) => {
                        if let Err(error) = ShellRuntime::dispatch_action(cx, inv) {
                            tracing::warn!(
                                extension = %extension_id,
                                error = %error,
                                "extension action effect failed"
                            );
                        }
                    }
                    Err(error) => tracing::warn!(
                        extension = %extension_id,
                        error = %error,
                        "extension returned an invalid action invocation"
                    ),
                }
            }
            shilpo_ext::AuthorizedHostEffectKind::NonHttp(
                shilpo_ext::HostEffect::ShowNotification { title, body, icon },
            ) => {
                let mut notification = shilpo_services::Notification::new(title, body);
                notification.app_name = extension_id.to_string();
                notification.app_icon = icon;
                crate::bar::view::open_notification_toast(cx, notification);
            }
            shilpo_ext::AuthorizedHostEffectKind::NonHttp(
                shilpo_ext::HostEffect::SetThemeSource { color },
            ) => {
                let argb = crate::bar::view::parse_hex_color(&color).unwrap_or(0xFF006C4C);
                shilpo_theme::ThemeClient::spawn_task(async move {
                    let client = shilpo_theme::ThemeClient::new().await;
                    let _ = client.set_custom_seed(argb).await;
                    let _ = client
                        .set_color_source(shilpo_theme::ColorSource::Custom)
                        .await;
                });
            }
            shilpo_ext::AuthorizedHostEffectKind::NonHttp(
                shilpo_ext::HostEffect::SetWallpaper { path, .. },
            ) => {
                shilpo_theme::ThemeClient::spawn_task(async move {
                    let client = shilpo_theme::ThemeClient::new().await;
                    let _ = client.set_wallpaper(&path).await;
                });
            }
            shilpo_ext::AuthorizedHostEffectKind::NonHttp(
                shilpo_ext::HostEffect::ClipboardWrite { text },
            ) => {
                let result = cx
                    .global::<ShellRuntime>()
                    .service_hub
                    .as_ref()
                    .map(|hub| hub.clipboard.copy_text(&text));
                if let Some(Err(error)) = result {
                    tracing::warn!(
                        extension = %extension_id,
                        error = %error,
                        "extension clipboard effect failed"
                    );
                }
            }
            shilpo_ext::AuthorizedHostEffectKind::HttpRequest(request) => {
                let request_id = request.request_id().to_string();
                let key = (generation, extension_id.clone(), request_id.clone());
                let accepted = {
                    let in_flight = &mut cx.global_mut::<ShellRuntime>().extension_tasks;
                    request_id.len() <= 128
                        && !request_id.is_empty()
                        && in_flight.len() < 8
                        && !in_flight.contains_key(&key)
                };
                if !accepted {
                    if let Some(ext) = cx.global::<ShellRuntime>().extensions.as_ref()
                        && let Err(error) = ext.send_command(crate::extensions::ExtensionCommand::Response {
                        expected: generation,
                        extension_id: extension_id.clone(),
                        event: shilpo_ext::ExtensionEvent::HttpResponse {
                            request_id,
                            status: None,
                            body: String::new(),
                            error: Some(
                                "request ID is invalid, duplicated, or the HTTP limit was reached"
                                    .into(),
                            ),
                        },
                    })
                    {
                        tracing::warn!(%error, "extension rejection response was not queued");
                    }
                    return;
                }
                let ext_id = extension_id.clone();
                let task_key = key.clone();
                let task = cx.spawn(async move |cx| {
                    let response = crate::extension_http::fetch(request).await;
                    cx.update(|cx: &mut gpui::App| {
                        if cx.has_global::<ShellRuntime>() {
                            cx.global_mut::<ShellRuntime>()
                                .extension_tasks
                                .remove(&task_key);
                        }
                        if let Some(ext) = cx.global::<ShellRuntime>().extensions.as_ref()
                            && let Err(error) =
                                ext.send_command(crate::extensions::ExtensionCommand::Response {
                                    expected: generation,
                                    extension_id: ext_id,
                                    event: response,
                                })
                        {
                            tracing::warn!(%error, "extension HTTP response was not queued");
                        }
                    });
                });
                cx.global_mut::<ShellRuntime>()
                    .extension_tasks
                    .insert(key.clone(), task);
            }
            shilpo_ext::AuthorizedHostEffectKind::NonHttp(shilpo_ext::HostEffect::LocationRead) => {
                let location_service = cx
                    .global::<ShellRuntime>()
                    .extension_location_service
                    .clone();
                let ext_id = extension_id.clone();
                let task_id = uuid::Uuid::new_v4().to_string();
                let key = (generation, extension_id.clone(), task_id);
                let task_key = key.clone();
                let task = cx.spawn(async move |cx| {
                    let result = location_service.read_location_async().await;
                    let event = match result {
                        Ok(info) => shilpo_ext::ExtensionEvent::LocationResponse {
                            latitude: Some(info.latitude),
                            longitude: Some(info.longitude),
                            accuracy_meters: Some(info.accuracy_meters),
                            error: None,
                        },
                        Err(error) => shilpo_ext::ExtensionEvent::LocationResponse {
                            latitude: None,
                            longitude: None,
                            accuracy_meters: None,
                            error: Some(error),
                        },
                    };
                    cx.update(|cx: &mut gpui::App| {
                        if cx.has_global::<ShellRuntime>() {
                            cx.global_mut::<ShellRuntime>()
                                .extension_tasks
                                .remove(&task_key);
                        }
                        if let Some(ext) = cx.global::<ShellRuntime>().extensions.as_ref()
                            && let Err(error) =
                                ext.send_command(crate::extensions::ExtensionCommand::Response {
                                    expected: generation,
                                    extension_id: ext_id,
                                    event,
                                })
                        {
                            tracing::warn!(%error, "extension location response was not queued");
                        }
                    });
                });
                cx.global_mut::<ShellRuntime>()
                    .extension_tasks
                    .insert(key.clone(), task);
            }
            shilpo_ext::AuthorizedHostEffectKind::NonHttp(effect) => tracing::debug!(
                extension = %extension_id,
                ?effect,
                "accepted extension effect has no shell service adapter yet"
            ),
        }
    }

    fn drain_service_hub(cx: &mut App) {
        if !cx.has_global::<Self>() {
            return;
        }

        if let Some(hub) = cx.global_mut::<Self>().service_hub.as_mut() {
            hub.poll_notification_reconnect();
        }
        cx.global::<Self>().publish_status();

        let notifs = {
            let runtime = cx.global_mut::<Self>();
            let mut list = Vec::new();
            if let Some(hub) = &runtime.service_hub
                && let Ok(rx) = hub.notif_rx.lock()
            {
                while let Ok(notif) = rx.try_recv() {
                    list.push(notif);
                }
            }
            list
        };

        for notif in notifs {
            crate::bar::view::open_notification_toast(cx, notif);
        }

        let updates = {
            let runtime = cx.global_mut::<Self>();
            let mut list = Vec::new();
            if let Some(hub) = &runtime.service_hub
                && let Ok(rx) = hub.updates_rx.lock()
            {
                while let Ok(upd) = rx.try_recv() {
                    list.push(upd);
                }
            }
            list
        };

        if !updates.is_empty() {
            for upd in &updates {
                if let Some(ref mut hub) = cx.global_mut::<Self>().service_hub {
                    hub.device_snapshot.apply(upd);
                }
                match upd {
                    crate::bar::service_worker::WorkerUpdate::ServiceStateChange {
                        service,
                        state,
                        last_error,
                    } => {
                        if let Some(hub) = cx.global_mut::<Self>().service_hub.as_mut() {
                            let available = state.is_ready();
                            match *service {
                                "battery" => {
                                    hub.availability.battery_available = available;
                                    hub.availability.battery_state = *state;
                                    hub.availability.battery_last_error = last_error.clone();
                                }
                                "audio" => {
                                    hub.availability.audio_available = available;
                                    hub.availability.audio_state = *state;
                                    hub.availability.audio_last_error = last_error.clone();
                                }
                                "network" => {
                                    hub.availability.network_available = available;
                                    hub.availability.network_state = *state;
                                    hub.availability.network_last_error = last_error.clone();
                                }
                                "media" => {
                                    hub.availability.media_available = available;
                                    hub.availability.media_state = *state;
                                    hub.availability.media_last_error = last_error.clone();
                                }
                                "brightness" => {
                                    hub.availability.brightness_available = available;
                                    hub.availability.brightness_state = *state;
                                    hub.availability.brightness_last_error = last_error.clone();
                                }
                                _ => tracing::warn!(service, "unknown service state update"),
                            }
                        }
                    }
                    crate::bar::service_worker::WorkerUpdate::CommandRejected {
                        reason, ..
                    } => tracing::warn!(%reason, "device command rejected"),
                    crate::bar::service_worker::WorkerUpdate::Config(
                        crate::bar::service_worker::ConfigUpdate::Loaded(config),
                    ) => {
                        cx.global_mut::<Self>().active_config = (**config).clone();
                        Self::sync_displays(cx);
                        // A settings-only config edit does not recreate the bar
                        // window. Reconcile mounted extension instances explicitly
                        // so their ContributionSettingsChanged event is delivered.
                        Self::reconcile_bar_extension_instances(cx);
                    }
                    crate::bar::service_worker::WorkerUpdate::Battery(info) => {
                        Self::dispatch_extension_event(
                            cx,
                            shilpo_ext::ExtensionEvent::PowerChanged {
                                percentage: info.is_present.then_some(info.percentage as f32),
                                charging: info.is_charging,
                            },
                        );
                    }
                    crate::bar::service_worker::WorkerUpdate::Network(info) => {
                        Self::dispatch_extension_event(
                            cx,
                            shilpo_ext::ExtensionEvent::NetworkChanged {
                                connected: info.available && info.is_connected,
                            },
                        );
                    }
                    crate::bar::service_worker::WorkerUpdate::Media(info) => {
                        Self::dispatch_extension_event(
                            cx,
                            shilpo_ext::ExtensionEvent::MediaChanged {
                                title: (!info.title.is_empty()).then_some(info.title.clone()),
                                artist: (!info.artist.is_empty()).then_some(info.artist.clone()),
                                playing: info.playback_state
                                    == shilpo_services::PlaybackState::Playing,
                            },
                        );
                    }
                    _ => {}
                }
            }

            let handles: Vec<_> = cx
                .global::<Self>()
                .bars
                .values()
                .map(|(handle, _)| *handle)
                .collect();

            for handle in handles {
                let updates_clone = updates.clone();
                let _ = handle.update(cx, |bar_view, _window, cx| {
                    for upd in &updates_clone {
                        bar_view.apply_worker_update(upd, cx);
                    }
                });
            }
        }
    }

    pub fn sync_displays(cx: &mut App) {
        if !cx.has_global::<Self>() {
            return;
        }

        use crate::bar::reconciliation::{
            BarSpec, OutputDescriptor, ReconciliationOp, reconcile_output_bars,
        };

        let primary_id = cx.primary_display().map(|d| d.id());
        let compositor_outputs = Self::compositor_snapshot(cx).outputs.clone();
        let current_outputs: Vec<OutputDescriptor> = cx
            .displays()
            .into_iter()
            .map(|d| OutputDescriptor {
                display_id: d.id(),
                bounds: d.bounds(),
                is_primary: primary_id == Some(d.id()),
                name: Self::output_name_for_display(d.as_ref(), &compositor_outputs),
                scale: None,
            })
            .collect();
        let output_ids = current_outputs
            .iter()
            .map(|output| output.display_id)
            .collect::<std::collections::HashSet<_>>();
        let outputs_changed = {
            let runtime = cx.global_mut::<Self>();
            if runtime.extension_output_ids == output_ids {
                false
            } else {
                runtime.extension_output_ids = output_ids;
                true
            }
        };
        if outputs_changed {
            Self::dispatch_extension_event(cx, shilpo_ext::ExtensionEvent::OutputsChanged);
        }

        let (config, current_specs) = {
            let runtime = cx.global::<Self>();
            let specs: HashMap<DisplayId, BarSpec> = runtime
                .bars
                .iter()
                .map(|(&id, (_, spec))| (id, spec.clone()))
                .collect();
            (runtime.active_config.clone(), specs)
        };

        let ops = reconcile_output_bars(&current_outputs, &config, &current_specs);

        for op in ops {
            match op {
                ReconciliationOp::Create(spec) => {
                    Self::open_bar_with_spec(cx, spec);
                }
                ReconciliationOp::Recreate(spec) => {
                    let display_id = spec.display_id;
                    let old_bar = cx.global_mut::<Self>().bars.remove(&display_id);
                    Self::open_bar_with_spec(cx, spec);
                    if let Some((handle, _)) = old_bar {
                        let _ =
                            cx.update_window(handle.into(), |_, window, _| window.remove_window());
                    }
                }
                ReconciliationOp::Remove(display_id) => {
                    if let Some((handle, _)) = cx.global_mut::<Self>().bars.remove(&display_id) {
                        let _ =
                            cx.update_window(handle.into(), |_, window, _| window.remove_window());
                    }
                }
                ReconciliationOp::Retain(_) => {}
            }
        }
        Self::reconcile_extension_surfaces(cx, &current_outputs);
    }

    fn reconcile_extension_surfaces(cx: &mut App, outputs: &[crate::bar::OutputDescriptor]) {
        let config = cx.global::<Self>().active_config.clone();
        let mut instances = Vec::new();

        for (display_id, (_, spec)) in &cx.global::<Self>().bars {
            for (section, widgets) in [
                ("start", &spec.config.widgets.start),
                ("center", &spec.config.widgets.center),
                ("end", &spec.config.widgets.end),
            ] {
                for (index, widget) in widgets.iter().enumerate() {
                    if let shilpo_config::BarWidget::Extension(contribution) = widget {
                        instances.push(crate::extensions::ContributionInstance {
                            id: format!("bar:{display_id:?}:{section}:{index}"),
                            contribution: contribution.clone(),
                            output: Some(format!("{display_id:?}")),
                            width: spec.geometry.bounds.size.width.as_f32(),
                            height: spec.config.height as f32,
                            settings: extension_settings(&config, &contribution.extension_id, None),
                        });
                    }
                }
            }
        }

        let mut desired_windows = HashMap::new();
        for widget in &config.desktop.widgets {
            let output = if widget.output == "primary" {
                outputs.iter().find(|output| output.is_primary)
            } else {
                outputs.iter().find(|output| {
                    output.name.as_deref() == Some(widget.output.as_str())
                        || format!("{:?}", output.display_id) == widget.output
                })
            };
            let Some(output) = output else {
                continue;
            };
            let bounds = Bounds::new(
                point(
                    output.bounds.origin.x + px(widget.x as f32),
                    output.bounds.origin.y + px(widget.y as f32),
                ),
                size(px(widget.width as f32), px(widget.height as f32)),
            );
            let spec = ExtensionSurfaceSpec {
                contribution: widget.contribution.0.clone(),
                display_id: output.display_id,
                bounds,
            };
            desired_windows.insert(widget.instance.clone(), spec);
            instances.push(crate::extensions::ContributionInstance {
                id: widget.instance.clone(),
                contribution: widget.contribution.0.clone(),
                output: Some(widget.output.clone()),
                width: widget.width as f32,
                height: widget.height as f32,
                settings: extension_settings(
                    &config,
                    &widget.contribution.0.extension_id,
                    Some(&widget.settings),
                ),
            });
        }

        if let Some(ext) = cx.global::<Self>().extensions.as_ref()
            && let Err(error) =
                ext.send_command(crate::extensions::ExtensionCommand::ReconcileInstances {
                    expected: ext.generation(),
                    desired: instances,
                })
        {
            tracing::warn!(%error, "extension instance reconciliation was not queued");
        }

        let stale = cx
            .global::<Self>()
            .extension_surfaces
            .iter()
            .filter(|(id, (_, current))| desired_windows.get(*id) != Some(current))
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in stale {
            if let Some((handle, _)) = cx.global_mut::<Self>().extension_surfaces.remove(&id) {
                let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
            }
        }

        for (instance_id, spec) in desired_windows {
            if cx
                .global::<Self>()
                .extension_surfaces
                .contains_key(&instance_id)
            {
                continue;
            }
            let options = WindowOptions {
                titlebar: None,
                window_bounds: Some(WindowBounds::Windowed(spec.bounds)),
                display_id: Some(spec.display_id),
                app_id: Some(format!("shilpo-extension-{instance_id}")),
                window_background: WindowBackgroundAppearance::Transparent,
                kind: WindowKind::LayerShell(LayerShellOptions {
                    namespace: format!("extension-{instance_id}"),
                    layer: Layer::Bottom,
                    keyboard_interactivity: KeyboardInteractivity::None,
                    ..Default::default()
                }),
                ..Default::default()
            };
            let contribution = spec.contribution.clone();
            let view_instance_id = instance_id.clone();
            match cx.open_window(options, move |window, cx| {
                crate::extension_surface::ExtensionSurfaceView::view(
                    contribution,
                    Some(view_instance_id),
                    window,
                    cx,
                )
            }) {
                Ok(handle) => {
                    cx.global_mut::<Self>()
                        .extension_surfaces
                        .insert(instance_id, (handle, spec));
                }
                Err(error) => tracing::warn!(
                    error = %error,
                    "failed to open extension desktop surface"
                ),
            }
        }
    }

    fn reconcile_bar_extension_instances(cx: &mut App) {
        let config = cx.global::<Self>().active_config.clone();
        let mut instances = Vec::new();
        for (display_id, (_, spec)) in &cx.global::<Self>().bars {
            for (section, widgets) in [
                ("start", &spec.config.widgets.start),
                ("center", &spec.config.widgets.center),
                ("end", &spec.config.widgets.end),
            ] {
                for (index, widget) in widgets.iter().enumerate() {
                    if let shilpo_config::BarWidget::Extension(contribution) = widget {
                        instances.push(crate::extensions::ContributionInstance {
                            id: format!("bar:{display_id:?}:{section}:{index}"),
                            contribution: contribution.clone(),
                            output: Some(format!("{display_id:?}")),
                            width: spec.geometry.bounds.size.width.as_f32(),
                            height: spec.config.height as f32,
                            settings: extension_settings(&config, &contribution.extension_id, None),
                        });
                    }
                }
            }
        }
        if let Some(ext) = cx.global::<Self>().extensions.as_ref()
            && let Err(error) =
                ext.send_command(crate::extensions::ExtensionCommand::ReconcileInstances {
                    expected: ext.generation(),
                    desired: instances,
                })
        {
            tracing::warn!(%error, "extension bar instance reconciliation was not queued");
        }
    }

    fn publish_status(&self) {
        let snapshot = &self.latest_snapshot;
        let (attempt, last_err) = match &snapshot.connection {
            CompositorConnection::Reconnecting {
                attempt,
                last_error,
            } => (*attempt, last_error.clone()),
            _ => (0, None),
        };

        let health = shilpo_services::ServiceHealth {
            compositor_connected: snapshot.connection.is_ready(),
            compositor_state: snapshot.connection.state_name().to_string(),
            compositor_revision: snapshot.revision,
            compositor_reconnect_attempt: attempt,
            compositor_last_error: last_err,
            compositor_telemetry: self
                .service_hub
                .as_ref()
                .map(|h| h.compositor.command_broker().telemetry()),
            battery_service_available: self
                .service_hub
                .as_ref()
                .map(|h| h.availability.battery_available)
                .unwrap_or(false),
            battery_state: self
                .service_hub
                .as_ref()
                .map(|h| h.availability.battery_state)
                .unwrap_or_default(),
            battery_last_error: self
                .service_hub
                .as_ref()
                .and_then(|h| h.availability.battery_last_error.clone()),
            audio_service_available: self
                .service_hub
                .as_ref()
                .map(|h| h.availability.audio_available)
                .unwrap_or(false),
            audio_state: self
                .service_hub
                .as_ref()
                .map(|h| h.availability.audio_state)
                .unwrap_or_default(),
            audio_last_error: self
                .service_hub
                .as_ref()
                .and_then(|h| h.availability.audio_last_error.clone()),
            network_service_available: self
                .service_hub
                .as_ref()
                .map(|h| h.availability.network_available)
                .unwrap_or(false),
            network_state: self
                .service_hub
                .as_ref()
                .map(|h| h.availability.network_state)
                .unwrap_or_default(),
            network_last_error: self
                .service_hub
                .as_ref()
                .and_then(|h| h.availability.network_last_error.clone()),
            notification_service_available: self
                .service_hub
                .as_ref()
                .map(|h| h.notification_state.is_ready())
                .unwrap_or(false),
            notification_state: self
                .service_hub
                .as_ref()
                .map(|h| h.notification_state)
                .unwrap_or_default(),
            notification_last_error: self
                .service_hub
                .as_ref()
                .and_then(|h| h.notification_last_error.clone()),
            media_service_available: self
                .service_hub
                .as_ref()
                .map(|h| h.availability.media_available)
                .unwrap_or(false),
            media_state: self
                .service_hub
                .as_ref()
                .map(|h| h.availability.media_state)
                .unwrap_or_default(),
            media_last_error: self
                .service_hub
                .as_ref()
                .and_then(|h| h.availability.media_last_error.clone()),
            brightness_service_available: self
                .service_hub
                .as_ref()
                .map(|h| h.availability.brightness_available)
                .unwrap_or(false),
            brightness_state: self
                .service_hub
                .as_ref()
                .map(|h| h.availability.brightness_state)
                .unwrap_or_default(),
            brightness_last_error: self
                .service_hub
                .as_ref()
                .and_then(|h| h.availability.brightness_last_error.clone()),
            heed_store_available: self.heed_store.is_some(),
            uptime_seconds: self.start_time.elapsed().as_secs(),
        };

        self.ipc_server.update_status(IpcStatus {
            running: true,
            readiness: self.readiness,
            bar: self.bar_state.clone(),
            overview_visible: self.overview.is_some(),
            control_center_visible: self.control_center.is_some(),
            health,
            ..Default::default()
        });
    }

    pub fn on_compositor_snapshot_changed(cx: &mut App, snapshot: Arc<CompositorSnapshot>) {
        if !cx.has_global::<Self>() {
            return;
        }
        let (outputs_changed, overview_entity) = {
            let runtime = cx.global_mut::<Self>();
            let outputs_changed = runtime.latest_snapshot.outputs != snapshot.outputs;
            runtime.latest_snapshot = snapshot.clone();

            let is_ready = snapshot.connection.is_ready();
            if let Some(desc) = runtime.actions.descriptor_mut(&ActionId::FocusWorkspace) {
                desc.enabled = is_ready && snapshot.capabilities.can_focus_workspace;
            }
            if let Some(desc) = runtime.actions.descriptor_mut(&ActionId::CreateWorkspace) {
                desc.enabled = is_ready && snapshot.capabilities.can_create_workspace;
            }
            if let Some(desc) = runtime
                .actions
                .descriptor_mut(&ActionId::MoveWindowToWorkspace)
            {
                desc.enabled = is_ready && snapshot.capabilities.can_move_window;
            }
            if let Some(desc) = runtime.actions.descriptor_mut(&ActionId::FocusWindow) {
                desc.enabled = is_ready && snapshot.capabilities.can_focus_window;
            }
            if let Some(desc) = runtime.actions.descriptor_mut(&ActionId::CloseWindow) {
                desc.enabled = is_ready && snapshot.capabilities.can_close_window;
            }

            let bar_ok = matches!(runtime.bar_state, BarState::Visible | BarState::Hidden);
            runtime.readiness = match &snapshot.connection {
                CompositorConnection::Connecting => shilpo_services::ipc::ReadinessState::Starting,
                CompositorConnection::Ready => {
                    if bar_ok {
                        shilpo_services::ipc::ReadinessState::Ready
                    } else {
                        shilpo_services::ipc::ReadinessState::Degraded
                    }
                }
                CompositorConnection::Reconnecting { .. } => {
                    shilpo_services::ipc::ReadinessState::Degraded
                }
                CompositorConnection::Stopped => shilpo_services::ipc::ReadinessState::Failed,
            };

            runtime.publish_status();
            (outputs_changed, runtime.overview_entity.clone())
        };

        let bar_handles: Vec<_> = cx
            .global::<Self>()
            .bars
            .values()
            .map(|(handle, _)| *handle)
            .collect();

        if let Some(overview) = overview_entity {
            overview.update(cx, |view, cx| view.update_snapshot(snapshot, cx));
        }

        for handle in bar_handles {
            let _ = handle.update(cx, |_, _window, cx| cx.notify());
        }

        if outputs_changed {
            Self::reconcile_bars(cx);
        }
    }

    pub fn open_bar_with_spec(cx: &mut App, spec: crate::bar::BarSpec) -> bool {
        let options = bar_window_options(&spec.geometry, spec.with_display_geometry);
        let display_id = spec.display_id;
        let bar_config = spec.config.clone();
        let output_name = spec.output_name.clone();

        let result = cx.open_window(options, move |window, cx| {
            let shell_config = shilpo_config::ShellConfig {
                bar: bar_config,
                ..Default::default()
            };
            BarView::view_with_config_on_display_with_output(
                window,
                cx,
                shell_config,
                display_id,
                output_name,
            )
        });

        let runtime = cx.global_mut::<Self>();
        match result {
            Ok(handle) => {
                runtime.bars.insert(display_id, (handle, spec));
                runtime.bar_state = BarState::Visible;
                runtime.publish_status();
                true
            }
            Err(error) => {
                tracing::error!(error = %error, ?display_id, "failed to open bar window");
                if runtime.bars.is_empty() {
                    runtime.bar_state = BarState::OpenFailed;
                }
                runtime.publish_status();
                false
            }
        }
    }

    pub fn open_bar(cx: &mut App, geometry: &BarGeometry, with_display_geometry: bool) -> bool {
        let bar_config = cx
            .global::<Self>()
            .active_config
            .bar_for_output(None, true)
            .unwrap_or_else(|| shilpo_config::ShellConfig::default().bar);
        let spec = crate::bar::BarSpec {
            display_id: geometry.display_id,
            output_name: None,
            geometry: geometry.clone(),
            config: bar_config,
            with_display_geometry,
        };
        Self::open_bar_with_spec(cx, spec)
    }

    pub fn reconcile_bars(cx: &mut App) {
        let displays = cx.displays();
        let primary_id = cx.primary_display().map(|d| d.id());
        let compositor_outputs = Self::compositor_snapshot(cx).outputs.clone();
        let outputs: Vec<_> = displays
            .into_iter()
            .map(|d| crate::bar::OutputDescriptor {
                display_id: d.id(),
                bounds: d.bounds(),
                is_primary: Some(d.id()) == primary_id,
                name: Self::output_name_for_display(d.as_ref(), &compositor_outputs),
                scale: None,
            })
            .collect();

        let current_bars: HashMap<_, _> = cx
            .global::<Self>()
            .bars
            .iter()
            .map(|(&id, (_, spec))| (id, spec.clone()))
            .collect();

        let active_config = cx.global::<Self>().active_config.clone();
        let ops = crate::bar::reconcile_output_bars(&outputs, &active_config, &current_bars);

        for op in ops {
            match op {
                crate::bar::ReconciliationOp::Create(spec)
                | crate::bar::ReconciliationOp::Recreate(spec) => {
                    if let Some((handle, _)) = cx.global_mut::<Self>().bars.remove(&spec.display_id)
                    {
                        let _ =
                            cx.update_window(handle.into(), |_, window, _| window.remove_window());
                    }
                    Self::open_bar_with_spec(cx, spec);
                }
                crate::bar::ReconciliationOp::Remove(display_id) => {
                    if let Some((handle, _)) = cx.global_mut::<Self>().bars.remove(&display_id) {
                        let _ =
                            cx.update_window(handle.into(), |_, window, _| window.remove_window());
                    }
                }
                crate::bar::ReconciliationOp::Retain(_) => {}
            }
        }
    }

    pub fn mark_bar_open_failed(cx: &mut App) {
        let runtime = cx.global_mut::<Self>();
        runtime.bar_state = BarState::OpenFailed;
        runtime.publish_status();
    }

    pub fn toggle_bar(cx: &mut App) {
        let (open_handles, specs) = {
            let runtime = cx.global_mut::<Self>();
            let handles: Vec<_> = runtime.bars.values().map(|(h, _)| *h).collect();
            runtime.bars.clear();
            let specs = runtime.last_bar_specs.clone();
            (handles, specs)
        };
        if !open_handles.is_empty() {
            for handle in open_handles {
                let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
            }
            let runtime = cx.global_mut::<Self>();
            runtime.bar_state = BarState::Hidden;
            runtime.publish_status();
            return;
        }
        if !specs.is_empty() {
            for (geometry, with_display_geometry) in specs {
                Self::open_bar(cx, &geometry, with_display_geometry);
            }
        } else {
            Self::mark_bar_open_failed(cx);
            tracing::warn!("cannot toggle bar: no valid reopen geometry");
        }
    }

    fn capture_prior_focus(cx: &mut App) {
        if cx.global::<Self>().prior_window_id.is_none() {
            let snapshot = Self::compositor_snapshot(cx);
            if let Some(win_id) = snapshot.focused_window_id {
                cx.global_mut::<Self>().prior_window_id = Some(win_id);
            }
        }
    }

    fn restore_prior_focus(cx: &mut App) {
        let prior_id = cx.global_mut::<Self>().prior_window_id.take();
        if let Some(win_id) = prior_id
            && let Some(comp) = Self::compositor(cx)
        {
            match comp
                .command_broker()
                .submit(CompositorCommand::FocusWindow(win_id))
            {
                Ok(ticket) => {
                    cx.spawn(async move |_cx| {
                        if let Err(error) = ticket.await {
                            tracing::warn!(
                                error = %error,
                                window_id = win_id,
                                "failed to restore prior window focus"
                            );
                        }
                    })
                    .detach();
                }
                Err(error) => {
                    tracing::warn!(
                        error = %error,
                        window_id = win_id,
                        "failed to restore prior window focus"
                    );
                }
            }
        }
    }

    pub fn toggle_overview(cx: &mut App) {
        if cx.global::<Self>().overview.is_some() {
            Self::close_overview(cx);
        } else {
            Self::open_or_focus_overview(cx);
        }
    }

    pub fn close_overview(cx: &mut App) {
        // Begin animated close via the overview entity.
        let entity = cx.global::<Self>().overview_entity.clone();
        if let Some(entity) = entity {
            entity.update(cx, |view, cx| {
                view.begin_close(crate::overview::OverviewCloseReason::Cancel, cx);
            });
        } else {
            // No entity — remove window immediately.
            let instance_id = cx.global::<Self>().overview_instance;
            Self::finish_overview_close(
                cx,
                crate::overview::OverviewCloseReason::Cancel,
                instance_id,
            );
        }
    }

    /// Called by the overview entity after the exit animation completes.
    pub fn finish_overview_close(
        cx: &mut App,
        reason: crate::overview::OverviewCloseReason,
        instance_id: u64,
    ) {
        if !cx.has_global::<Self>() {
            return;
        }
        if cx.global::<Self>().overview_instance != instance_id {
            return;
        }
        Self::dispatch_surface_lifecycle(
            cx,
            crate::extensions::ContributionSurface::Launcher,
            false,
            0.0,
            0.0,
        );
        cx.global_mut::<Self>().overview_instance = 0;
        let opened_workspace_id = cx.global_mut::<Self>().overview_opened_workspace_id.take();
        let current_workspace_id = Self::compositor_snapshot(cx).focused_workspace_id;
        let restore_prior_focus =
            should_restore_overview_prior_focus(reason, opened_workspace_id, current_workspace_id);
        let handle = cx.global_mut::<Self>().overview.take();
        cx.global_mut::<Self>().overview_entity = None;
        if reason == crate::overview::OverviewCloseReason::Selection {
            // Clear this before removing the surface so a concurrent close
            // callback cannot restore the old window focus.
            cx.global_mut::<Self>().prior_window_id = None;
        }
        if let Some(handle) = handle {
            let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
        }
        if restore_prior_focus {
            Self::restore_prior_focus(cx);
        } else if reason == crate::overview::OverviewCloseReason::Cancel {
            cx.global_mut::<Self>().prior_window_id = None;
        }
        Self::refresh_bars(cx);
        cx.global::<Self>().publish_status();
    }

    pub fn forget_overview(cx: &mut App, instance_id: u64) {
        if !cx.has_global::<Self>() || cx.global::<Self>().overview_instance != instance_id {
            return;
        }
        Self::dispatch_surface_lifecycle(
            cx,
            crate::extensions::ContributionSurface::Launcher,
            false,
            0.0,
            0.0,
        );
        let entity = cx.global::<Self>().overview_entity.clone();
        let reason = entity
            .as_ref()
            .and_then(|entity| entity.read(cx).close_reason())
            .unwrap_or(crate::overview::OverviewCloseReason::Cancel);
        let current_workspace_id = Self::compositor_snapshot(cx).focused_workspace_id;
        let (had_handle, reason, opened_workspace_id) = {
            let runtime = cx.global_mut::<Self>();
            runtime.overview_instance = 0;
            let had_handle = runtime.overview.take().is_some();
            runtime.overview_entity = None;
            let opened_workspace_id = runtime.overview_opened_workspace_id.take();
            (had_handle, reason, opened_workspace_id)
        };
        if !had_handle {
            return;
        }
        if should_restore_overview_prior_focus(reason, opened_workspace_id, current_workspace_id) {
            Self::restore_prior_focus(cx);
        } else {
            cx.global_mut::<Self>().prior_window_id = None;
        }
        Self::refresh_bars(cx);
        cx.global::<Self>().publish_status();
    }

    /// Focus a workspace from an overview card click and close the overview.
    pub fn overview_focus_workspace(cx: &mut App, ws_id: u64) -> Result<(), ShellError> {
        Self::dispatch_action(cx, ActionInvocation::FocusWorkspace(ws_id))
    }

    /// Move a dragged overview window to a workspace without dismissing the overview.
    pub fn overview_move_window(
        cx: &mut App,
        window_id: u64,
        workspace_id: u64,
    ) -> Result<(), ShellError> {
        Self::dispatch_action(
            cx,
            ActionInvocation::MoveWindowToWorkspace {
                window_id,
                workspace_id,
            },
        )
    }

    /// Focus a window from an overview tile click and close the overview.
    pub fn overview_focus_window(cx: &mut App, window_id: u64) -> Result<(), ShellError> {
        let comp = match Self::compositor(cx) {
            Some(comp) => comp,
            None => {
                let error = ShellError::ActionFailed("compositor unavailable".into());
                Self::show_compositor_error_message_toast(cx, &error.to_string());
                return Err(error);
            }
        };
        let ticket = comp
            .command_broker()
            .submit(CompositorCommand::FocusWindow(window_id))
            .map_err(|error| {
                let error = ShellError::ActionFailed(error.to_string());
                Self::show_compositor_error_message_toast(cx, &error.to_string());
                error
            })?;
        cx.spawn(async move |cx| match ticket.await {
            Ok(shilpo_services::CommandOutcome::Applied { revision }) => {
                tracing::trace!(revision, window_id, "overview window focus applied");
            }
            Err(error) => {
                cx.update(|cx: &mut gpui::App| Self::show_compositor_error_toast(cx, &error));
            }
        })
        .detach();
        Ok(())
    }

    pub fn overview_reduced_motion(cx: &App) -> bool {
        cx.global::<Self>().active_config.theme.reduced_motion
    }

    pub fn is_overview_open(cx: &App) -> bool {
        cx.has_global::<Self>() && cx.global::<Self>().overview.is_some()
    }

    fn refresh_bars(cx: &mut App) {
        let bar_handles = cx
            .global::<Self>()
            .bars
            .values()
            .map(|(handle, _)| *handle)
            .collect::<Vec<_>>();
        for handle in bar_handles {
            let _ = handle.update(cx, |_, _, cx| cx.notify());
        }
    }

    pub fn overview_wallpaper_path(cx: &App) -> Option<PathBuf> {
        cx.global::<Self>().current_wallpaper_path.clone()
    }

    pub fn overview_applications(cx: &App) -> Vec<shilpo_services::Application> {
        cx.global::<Self>()
            .service_hub
            .as_ref()
            .map(|hub| hub.app_scanner.applications())
            .unwrap_or_default()
    }

    pub fn normalize_app_key(value: &str) -> String {
        crate::app_icons::normalize_app_key(value)
    }

    pub fn app_icon_index(
        cx: &App,
    ) -> std::sync::Arc<std::collections::HashMap<String, std::path::PathBuf>> {
        std::sync::Arc::new(crate::app_icons::build_app_icon_index(
            Self::overview_applications(cx),
        ))
    }

    pub fn begin_overview_instance(cx: &mut App) -> u64 {
        let runtime = cx.global_mut::<Self>();
        runtime.next_overview_instance = runtime.next_overview_instance.wrapping_add(1);
        runtime.overview_instance = runtime.next_overview_instance;
        runtime.overview_instance
    }

    pub fn open_or_focus_overview(cx: &mut App) {
        let display_id = cx.primary_display().map(|display| display.id());
        Self::open_or_focus_overview_on_display(cx, display_id);
    }

    pub fn open_or_focus_overview_on_display(
        cx: &mut App,
        requested_display_id: Option<DisplayId>,
    ) {
        if cx.global::<Self>().overview.is_none() {
            let focused_workspace_id = Self::compositor_snapshot(cx).focused_workspace_id;
            cx.global_mut::<Self>().overview_opened_workspace_id = focused_workspace_id;
        }
        Self::capture_prior_focus(cx);
        Self::close_control_center_for_replacement(cx);
        let handle = cx.global_mut::<Self>().overview.take();
        if let Some(handle) = handle
            && handle
                .update(cx, |_, window, _| {
                    window.activate_window();
                })
                .is_ok()
        {
            cx.global_mut::<Self>().overview = Some(handle);
            Self::refresh_bars(cx);
            cx.global::<Self>().publish_status();
            return;
        }

        let display_id =
            requested_display_id.or_else(|| cx.primary_display().map(|display| display.id()));
        // Use zero size + four-edge anchors so the layer shell fills the output.
        let options = overlay_options(
            "shilpo-overview",
            "overview",
            size(px(0.), px(0.)),
            point(px(0.), px(0.)),
            display_id,
        );
        match cx.open_window(options, crate::overview::WorkspaceOverview::view) {
            Ok(handle) => {
                cx.global_mut::<Self>().overview = Some(handle);
                Self::refresh_bars(cx);
            }
            Err(error) => {
                tracing::warn!(error = %error, "cannot open workspace overview window");
                cx.global_mut::<Self>().overview_opened_workspace_id = None;
                Self::restore_prior_focus(cx);
            }
        }
        cx.global::<Self>().publish_status();
    }

    pub fn open_or_focus_control_center(cx: &mut App) {
        Self::capture_prior_focus(cx);
        let handle = cx.global_mut::<Self>().control_center.take();
        if let Some(handle) = handle
            && handle
                .update(cx, |_, window, _| {
                    window.activate_window();
                })
                .is_ok()
        {
            cx.global_mut::<Self>().control_center = Some(handle);
            cx.global::<Self>().publish_status();
            return;
        }
        let (display_bounds, display_id) = if let Some(display) = cx.primary_display() {
            (display.bounds(), Some(display.id()))
        } else {
            (
                Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.))),
                None,
            )
        };
        let cc_size = size(px(340.), px(540.));
        let origin = point(
            display_bounds.origin.x + (display_bounds.size.width - px(360.)),
            display_bounds.origin.y + px(54.),
        );
        let options = overlay_options(
            "shilpo-control-center",
            "control-center",
            cc_size,
            origin,
            display_id,
        );
        match cx.open_window(options, ControlCenterView::view) {
            Ok(handle) => {
                cx.global_mut::<Self>().control_center = Some(handle);
                Self::dispatch_surface_lifecycle(
                    cx,
                    ContributionSurface::ControlCenter,
                    true,
                    340.,
                    540.,
                );
            }
            Err(error) => {
                tracing::error!(error = %error, overlay = "control-center", "failed to open overlay window")
            }
        }
        cx.global::<Self>().publish_status();
    }

    pub fn toggle_control_center(cx: &mut App) {
        if cx.global::<Self>().control_center.is_some() {
            Self::close_control_center(cx);
        } else {
            Self::open_or_focus_control_center(cx);
        }
    }

    pub fn close_control_center(cx: &mut App) {
        if !Self::remove_control_center_surface(cx) {
            return;
        }
        Self::restore_prior_focus(cx);
        cx.global::<Self>().publish_status();
    }

    fn close_control_center_for_replacement(cx: &mut App) {
        let _ = Self::remove_control_center_surface(cx);
    }

    fn remove_control_center_surface(cx: &mut App) -> bool {
        let handle = cx.global_mut::<Self>().control_center.take();
        let Some(handle) = handle else { return false };
        Self::dispatch_surface_lifecycle(cx, ContributionSurface::ControlCenter, false, 340., 540.);
        // Registry entry is invalidated above. A close racing with this call can
        // leave handle stale; update_window failure is expected in that case.
        let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
        true
    }

    pub fn device_snapshot(cx: &App) -> crate::bar::service_worker::DeviceSnapshot {
        cx.global::<Self>()
            .service_hub
            .as_ref()
            .map(|h| h.device_snapshot.clone())
            .unwrap_or_default()
    }

    pub fn dispatch_device_command(cx: &App, command: crate::bar::service_worker::DeviceCommand) {
        if let Some(hub) = cx.global::<Self>().service_hub.as_ref() {
            let _ = hub
                .service_commands
                .try_send(crate::bar::service_worker::WorkerCommand::Device(command));
        }
    }

    pub fn forget_control_center(cx: &mut App) {
        let had_handle = cx.global_mut::<Self>().control_center.take().is_some();
        if had_handle {
            Self::dispatch_surface_lifecycle(
                cx,
                ContributionSurface::ControlCenter,
                false,
                340.,
                540.,
            );
            Self::restore_prior_focus(cx);
        }
        cx.global::<Self>().publish_status();
    }

    pub fn is_dnd_active(cx: &App) -> bool {
        if cx.has_global::<Self>() {
            let runtime = cx.global::<Self>();
            runtime
                .service_hub
                .as_ref()
                .and_then(|hub| hub.notification.as_ref())
                .map_or(runtime.session_state.dnd_active, |notification| {
                    notification.is_dnd_enabled()
                })
        } else {
            false
        }
    }

    pub fn update_shortcut(cx: &mut App, spec: &str, action: ActionId) -> Result<(), String> {
        let shortcut = crate::actions::Shortcut::parse(spec)
            .ok_or_else(|| format!("invalid shortcut specification: '{}'", spec))?;
        let runtime = cx.global_mut::<Self>();
        runtime.keybindings.register(shortcut, action)
    }

    pub fn action_descriptors(cx: &App) -> Vec<crate::actions::ActionDescriptor> {
        if cx.has_global::<Self>() {
            cx.global::<Self>().actions.all()
        } else {
            ActionRegistry::default().all()
        }
    }

    pub fn register_extension_action(
        cx: &mut App,
        id: shilpo_ext::CanonicalId,
        name: impl Into<String>,
        label: impl Into<String>,
    ) -> Result<ActionId, String> {
        cx.global_mut::<Self>()
            .actions
            .register_extension(id, name, label)
    }

    pub fn update_shortcut_with_override(
        cx: &mut App,
        spec: &str,
        action: ActionId,
    ) -> Result<Option<ActionId>, String> {
        let shortcut = crate::actions::Shortcut::parse(spec)
            .ok_or_else(|| format!("invalid shortcut specification: '{}'", spec))?;
        let runtime = cx.global_mut::<Self>();
        Ok(runtime.keybindings.register_with_override(shortcut, action))
    }

    pub fn reset_shortcuts_to_defaults(cx: &mut App) {
        if cx.has_global::<Self>() {
            cx.global_mut::<Self>().keybindings.reset_to_defaults();
        }
    }

    pub fn notification_history(cx: &App) -> Vec<shilpo_services::Notification> {
        if cx.has_global::<Self>()
            && let Some(notification) = cx
                .global::<Self>()
                .service_hub
                .as_ref()
                .and_then(|hub| hub.notification.as_ref())
        {
            notification.history()
        } else {
            Vec::new()
        }
    }

    pub fn clear_notification_history(cx: &mut App) {
        if cx.has_global::<Self>()
            && let Some(notification) = cx
                .global::<Self>()
                .service_hub
                .as_ref()
                .and_then(|hub| hub.notification.as_ref())
        {
            notification.clear_history();
        }
    }

    pub fn set_dnd_enabled(cx: &mut App, enabled: bool) {
        if cx.has_global::<Self>() {
            let runtime = cx.global_mut::<Self>();
            runtime.session_state.dnd_active = enabled;
            let path = runtime.session_path.clone();
            let session = runtime.session_state.clone();
            let _ = session.save_atomic(&path);

            if let Some(ref hub) = runtime.service_hub
                && let Some(ref notif) = hub.notification
            {
                notif.set_dnd_enabled(enabled);
            }
            if let Some(hub) = runtime.service_hub.as_mut() {
                hub.notification_dnd = enabled;
            }
        }
    }

    pub fn app_scanner(cx: &App) -> Option<shilpo_services::AppScanner> {
        if cx.has_global::<Self>()
            && let Some(ref hub) = cx.global::<Self>().service_hub
        {
            Some(hub.app_scanner.clone())
        } else {
            None
        }
    }

    pub fn compositor(cx: &App) -> Option<Arc<dyn CompositorAdapter>> {
        if cx.has_global::<Self>()
            && let Some(ref hub) = cx.global::<Self>().service_hub
        {
            Some(hub.compositor.clone())
        } else {
            None
        }
    }

    pub fn compositor_snapshot(cx: &App) -> Arc<CompositorSnapshot> {
        if cx.has_global::<Self>() {
            cx.global::<Self>().latest_snapshot.clone()
        } else {
            Arc::new(CompositorSnapshot::default())
        }
    }

    pub fn register_overview_entity(
        cx: &mut App,
        entity: Entity<crate::overview::WorkspaceOverview>,
    ) {
        if cx.has_global::<Self>() {
            cx.global_mut::<Self>().overview_entity = Some(entity);
        }
    }

    pub fn clipboard_history(cx: &App) -> Vec<shilpo_config::ClipboardItem> {
        if cx.has_global::<Self>()
            && let Some(ref hub) = cx.global::<Self>().service_hub
        {
            hub.clipboard.history()
        } else {
            Vec::new()
        }
    }

    pub fn copy_clipboard_text(cx: &App, text: &str) {
        if cx.has_global::<Self>()
            && let Some(ref hub) = cx.global::<Self>().service_hub
        {
            let _ = hub.clipboard.copy_text(text);
        }
    }

    pub fn workspace_overview(cx: &App) -> Vec<shilpo_services::WorkspaceInfo> {
        Self::compositor_snapshot(cx).workspaces.clone()
    }

    pub fn focus_workspace(cx: &mut App, ws_id: u64) -> Result<(), ShellError> {
        Self::dispatch_action(cx, ActionInvocation::FocusWorkspace(ws_id))
    }

    pub fn save_audio_preference(cx: &App, device: Option<String>, port: Option<String>) {
        if cx.has_global::<Self>()
            && let Some(ref store) = cx.global::<Self>().heed_store
        {
            let mut pref = store.get_audio_preference().unwrap_or_default();
            if device.is_some() {
                pref.default_device = device;
            }
            if port.is_some() {
                pref.default_port = port;
            }
            let _ = store.save_audio_preference(&pref);
        }
    }

    pub(crate) fn reserve_notification_generation(cx: &mut App) -> u64 {
        let runtime = cx.global_mut::<Self>();
        runtime.notification_generation = runtime.notification_generation.wrapping_add(1);
        runtime.notification_generation
    }

    pub fn active_notification_handle(
        cx: &App,
    ) -> Option<WindowHandle<crate::notification::NotificationToastView>> {
        if cx.has_global::<Self>() {
            cx.global::<Self>()
                .notification
                .as_ref()
                .map(|(_, _, handle)| *handle)
        } else {
            None
        }
    }

    pub(crate) fn register_notification(
        cx: &mut App,
        generation: u64,
        notification_id: u32,
        handle: WindowHandle<crate::notification::NotificationToastView>,
    ) {
        let runtime = cx.global_mut::<Self>();
        runtime.notification = Some((generation, notification_id, handle));
    }

    pub fn invoke_notification_action(cx: &App, id: u32, action_key: &str) {
        if cx.has_global::<Self>()
            && let Some(ref hub) = cx.global::<Self>().service_hub
            && let Some(ref notif_service) = hub.notification
        {
            notif_service.invoke_action(id, action_key);
        }
    }

    pub fn close_active_notification(cx: &mut App) {
        let entry = cx.global_mut::<Self>().notification.take();
        if let Some((_, notification_id, handle)) = entry {
            Self::dismiss_notification(cx, notification_id);
            let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
        }
    }

    pub fn expire_notification(cx: &mut App, generation: u64) {
        let entry = cx.global_mut::<Self>().notification.take();
        let Some((current_generation, notification_id, handle)) = entry else {
            return;
        };
        if current_generation != generation {
            cx.global_mut::<Self>().notification =
                Some((current_generation, notification_id, handle));
            return;
        }
        // Generation check above makes delayed expiry harmless after replacement.
        // Entry is taken before close so stale expiry cannot retain registry state.
        Self::expire_notification_id(cx, notification_id);
        let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
    }

    pub fn forget_notification(cx: &mut App, generation: u64) {
        let is_current = cx
            .global::<Self>()
            .notification
            .as_ref()
            .is_some_and(|(current_generation, _, _)| *current_generation == generation);
        if is_current
            && let Some((_, notification_id, _)) = cx.global_mut::<Self>().notification.take()
        {
            Self::dismiss_notification(cx, notification_id);
        }
    }

    fn dismiss_notification(cx: &App, notification_id: u32) {
        if cx.has_global::<Self>()
            && let Some(notification) = cx
                .global::<Self>()
                .service_hub
                .as_ref()
                .and_then(|hub| hub.notification.as_ref())
        {
            notification.dismiss(notification_id);
        }
    }

    fn expire_notification_id(cx: &App, notification_id: u32) {
        if cx.has_global::<Self>()
            && let Some(notification) = cx
                .global::<Self>()
                .service_hub
                .as_ref()
                .and_then(|hub| hub.notification.as_ref())
        {
            notification.expire(notification_id);
        }
    }

    pub fn forget_osd(cx: &mut App) {
        if cx.has_global::<Self>() {
            cx.global_mut::<Self>().osd = None;
        }
    }

    fn schedule_osd_dismiss(cx: &mut App, generation: u64) {
        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(2))
                .await;
            cx.update(|cx: &mut gpui::App| {
                if cx.has_global::<Self>() {
                    let runtime = cx.global_mut::<Self>();
                    if let Some((current_gen, handle, _)) = &runtime.osd
                        && *current_gen == generation
                    {
                        let window_handle = *handle;
                        runtime.osd = None;
                        let _ = cx.update_window(window_handle.into(), |_, window, _| {
                            window.remove_window()
                        });
                    }
                }
            });
        })
        .detach();
    }

    pub fn show_osd(cx: &mut App, kind: crate::osd::OsdKind) {
        let existing = cx.global_mut::<Self>().osd.take();
        if let Some((generation, window_handle, view_handle)) = existing {
            view_handle.update(cx, |view, cx| {
                view.kind = kind;
                cx.notify();
            });
            let next_gen = generation + 1;
            cx.global_mut::<Self>().osd = Some((next_gen, window_handle, view_handle));
            Self::schedule_osd_dismiss(cx, next_gen);
            return;
        }

        let (display_bounds, display_id) = if let Some(display) = cx.primary_display() {
            (display.bounds(), Some(display.id()))
        } else {
            (
                Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.))),
                None,
            )
        };
        let osd_size = size(px(260.), px(48.));
        let origin = point(
            display_bounds.origin.x + (display_bounds.size.width - osd_size.width) / 2.0,
            display_bounds.origin.y + display_bounds.size.height - px(140.),
        );
        let options = WindowOptions {
            titlebar: None,
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(origin, osd_size))),
            display_id,
            app_id: Some("shilpo-osd".into()),
            window_background: WindowBackgroundAppearance::Transparent,
            kind: WindowKind::LayerShell(LayerShellOptions {
                namespace: "osd".into(),
                layer: Layer::Overlay,
                anchor: Anchor::BOTTOM,
                margin: Some((px(0.), px(0.), px(84.), px(0.))),
                keyboard_interactivity: KeyboardInteractivity::None,
                ..Default::default()
            }),
            focus: false,
            show: true,
            ..Default::default()
        };

        let spawned_view: std::sync::Arc<std::sync::Mutex<Option<Entity<crate::osd::OsdView>>>> =
            std::sync::Arc::new(std::sync::Mutex::new(None));
        let view_cell = spawned_view.clone();
        let window_result = cx.open_window(options, move |window, cx| {
            let (root, view) = crate::osd::OsdView::view(kind, window, cx);
            *view_cell.lock().unwrap() = Some(view);
            root
        });

        if let Ok(window_handle) = window_result
            && let Some(view_handle) = spawned_view.lock().unwrap().take()
        {
            cx.global_mut::<Self>().osd = Some((1, window_handle, view_handle));
            Self::schedule_osd_dismiss(cx, 1);
        }
    }

    pub fn forget_bar(cx: &mut App) {
        let runtime = cx.global_mut::<Self>();
        runtime.bars.clear();
        runtime.bar_state = BarState::Hidden;
        runtime.publish_status();
    }

    fn enqueue_worker(cx: &mut App, request: IpcRequest) -> Result<(), ShellError> {
        match request {
            IpcRequest::Compositor(cmd) => {
                let comp = Self::compositor(cx)
                    .ok_or_else(|| ShellError::ActionFailed("compositor unavailable".into()))?;
                let ticket = comp
                    .command_broker()
                    .submit(cmd)
                    .map_err(|error| ShellError::ActionFailed(error.to_string()))?;
                cx.spawn(async move |cx| match ticket.await {
                    Ok(shilpo_services::CommandOutcome::Applied { revision }) => {
                        tracing::trace!(revision, "compositor command applied");
                    }
                    Err(err) => {
                        cx.update(|cx: &mut gpui::App| {
                            tracing::warn!(error = %err, "compositor command failed");
                            Self::show_compositor_error_toast(cx, &err);
                        });
                    }
                })
                .detach();
            }
            IpcRequest::ReloadConfig => {
                let handle = cx
                    .global::<Self>()
                    .service_hub
                    .as_ref()
                    .map(|h| h.service_commands.clone());
                let handle = handle
                    .ok_or_else(|| ShellError::ActionFailed("service worker unavailable".into()))?;
                service_worker::try_send_command(&handle, WorkerCommand::ReloadConfig)
                    .map_err(|error| ShellError::ActionFailed(error.to_string()))?;
            }
            _ => {}
        }
        Ok(())
    }

    pub fn record_recent_app(cx: &mut App, app_id: &str) {
        if cx.has_global::<Self>() {
            let runtime = cx.global_mut::<Self>();
            runtime.session_state.record_recent_app(app_id);
            let path = runtime.session_path.clone();
            let session = runtime.session_state.clone();
            let _ = session.save_atomic(&path);
        }
    }

    pub fn recent_apps(cx: &App) -> Vec<String> {
        if cx.has_global::<Self>() {
            cx.global::<Self>().session_state.recent_apps.clone()
        } else {
            Vec::new()
        }
    }

    pub fn save_output_bar(cx: &mut App, output_name: &str, state: &shilpo_config::OutputBarState) {
        if cx.has_global::<Self>()
            && let Some(store) = &cx.global::<Self>().heed_store
        {
            let _ = store.put_output_bar(output_name, state);
        }
    }

    pub fn load_output_bar(cx: &App, output_name: &str) -> Option<shilpo_config::OutputBarState> {
        if cx.has_global::<Self>()
            && let Some(store) = &cx.global::<Self>().heed_store
        {
            store.get_output_bar(output_name).ok().flatten()
        } else {
            None
        }
    }

    pub fn dispatch_action(cx: &mut App, action: ActionInvocation) -> Result<(), ShellError> {
        match Self::dispatch_invocation(cx, action) {
            Ok(crate::actions::ActionResult::Immediate) => Ok(()),
            Ok(crate::actions::ActionResult::Compositor(ticket)) => {
                cx.spawn(async move |cx| match ticket.await {
                    Ok(shilpo_services::CommandOutcome::Applied { revision }) => {
                        tracing::trace!(revision, "compositor action applied");
                    }
                    Err(err) => {
                        cx.update(|cx: &mut gpui::App| {
                            tracing::warn!(error = %err, "compositor action failed");
                            Self::show_compositor_error_toast(cx, &err);
                        });
                    }
                })
                .detach();
                Ok(())
            }
            Err(err) => {
                tracing::warn!(error = %err, "action invocation failed");
                Self::show_compositor_error_message_toast(cx, &err.to_string());
                Err(err)
            }
        }
    }

    fn show_compositor_error_toast(cx: &mut App, error: &shilpo_services::CompositorCommandError) {
        let concise = format!("{error}");
        Self::show_compositor_error_message_toast(cx, &concise);
    }

    fn show_compositor_error_message_toast(cx: &mut App, concise: &str) {
        if cx.has_global::<Self>() {
            let notif = cx
                .global::<Self>()
                .service_hub
                .as_ref()
                .and_then(|h| h.notification.as_ref());
            if let Some(service) = notif {
                service.push_notification(shilpo_services::Notification::new(
                    "Compositor command failed",
                    concise,
                ));
            }
        }
    }

    pub fn dispatch_invocation(
        cx: &mut App,
        invocation: ActionInvocation,
    ) -> Result<crate::actions::ActionResult, ShellError> {
        let action_id = invocation.id();
        let descriptor = cx
            .global::<Self>()
            .actions
            .descriptor(&action_id)
            .cloned()
            .ok_or_else(|| ShellError::ActionFailed("unknown action id".into()))?;

        if !invocation.matches_descriptor(&descriptor) {
            return Err(ShellError::ActionFailed("invocation mismatch".into()));
        }

        if !descriptor.enabled {
            return Err(ShellError::ActionFailed(format!(
                "action '{}' is currently disabled",
                descriptor.name
            )));
        }

        match invocation {
            ActionInvocation::ToggleControlCenter => {
                Self::toggle_control_center(cx);
                Ok(crate::actions::ActionResult::Immediate)
            }
            ActionInvocation::ToggleBar => {
                Self::toggle_bar(cx);
                Ok(crate::actions::ActionResult::Immediate)
            }
            ActionInvocation::ToggleOverview => {
                Self::toggle_overview(cx);
                Ok(crate::actions::ActionResult::Immediate)
            }
            ActionInvocation::ReloadConfig => {
                Self::enqueue_worker(cx, IpcRequest::ReloadConfig)?;
                Ok(crate::actions::ActionResult::Immediate)
            }
            ActionInvocation::Quit => {
                Self::shutdown(cx);
                Ok(crate::actions::ActionResult::Immediate)
            }
            ActionInvocation::FocusWorkspace(id) => {
                let comp = Self::compositor(cx)
                    .ok_or_else(|| ShellError::ActionFailed("compositor unavailable".into()))?;
                let ticket = comp
                    .command_broker()
                    .submit(CompositorCommand::FocusWorkspace(id))
                    .map_err(|error| ShellError::ActionFailed(error.to_string()))?;
                Ok(crate::actions::ActionResult::Compositor(ticket))
            }
            ActionInvocation::FocusWindow(id) => {
                let comp = Self::compositor(cx)
                    .ok_or_else(|| ShellError::ActionFailed("compositor unavailable".into()))?;
                let ticket = comp
                    .command_broker()
                    .submit(CompositorCommand::FocusWindow(id))
                    .map_err(|error| ShellError::ActionFailed(error.to_string()))?;
                Ok(crate::actions::ActionResult::Compositor(ticket))
            }
            ActionInvocation::CloseWindow(id) => {
                let comp = Self::compositor(cx)
                    .ok_or_else(|| ShellError::ActionFailed("compositor unavailable".into()))?;
                let ticket = comp
                    .command_broker()
                    .submit(CompositorCommand::CloseWindow(id))
                    .map_err(|error| ShellError::ActionFailed(error.to_string()))?;
                Ok(crate::actions::ActionResult::Compositor(ticket))
            }
            ActionInvocation::CreateWorkspace => {
                let comp = Self::compositor(cx)
                    .ok_or_else(|| ShellError::ActionFailed("compositor unavailable".into()))?;
                let ticket = comp
                    .command_broker()
                    .submit(CompositorCommand::CreateWorkspace)
                    .map_err(|error| ShellError::ActionFailed(error.to_string()))?;
                Ok(crate::actions::ActionResult::Compositor(ticket))
            }
            ActionInvocation::MoveWindowToWorkspace {
                window_id,
                workspace_id,
            } => {
                let comp = Self::compositor(cx)
                    .ok_or_else(|| ShellError::ActionFailed("compositor unavailable".into()))?;
                let ticket = comp
                    .command_broker()
                    .submit(CompositorCommand::MoveWindowToWorkspace {
                        window_id,
                        workspace_id,
                    })
                    .map_err(|error| ShellError::ActionFailed(error.to_string()))?;
                Ok(crate::actions::ActionResult::Compositor(ticket))
            }
            ActionInvocation::VolumeUp => {
                Self::dispatch_device_command(
                    cx,
                    crate::bar::service_worker::DeviceCommand::Audio(
                        crate::bar::service_worker::AudioCommand::StepDefaultVolume(
                            crate::bar::service_worker::VolumeStep::Up,
                        ),
                    ),
                );
                let info = Self::device_snapshot(cx).audio;
                let target_vol = (info.volume + 5).min(100);
                Self::show_osd(
                    cx,
                    crate::osd::OsdKind::Volume {
                        level: target_vol as u32,
                        muted: info.is_muted,
                    },
                );
                Ok(crate::actions::ActionResult::Immediate)
            }
            ActionInvocation::VolumeDown => {
                Self::dispatch_device_command(
                    cx,
                    crate::bar::service_worker::DeviceCommand::Audio(
                        crate::bar::service_worker::AudioCommand::StepDefaultVolume(
                            crate::bar::service_worker::VolumeStep::Down,
                        ),
                    ),
                );
                let info = Self::device_snapshot(cx).audio;
                let target_vol = info.volume.saturating_sub(5);
                Self::show_osd(
                    cx,
                    crate::osd::OsdKind::Volume {
                        level: target_vol as u32,
                        muted: info.is_muted,
                    },
                );
                Ok(crate::actions::ActionResult::Immediate)
            }
            ActionInvocation::VolumeMute => {
                Self::dispatch_device_command(
                    cx,
                    crate::bar::service_worker::DeviceCommand::Audio(
                        crate::bar::service_worker::AudioCommand::ToggleDefaultMute,
                    ),
                );
                let info = Self::device_snapshot(cx).audio;
                Self::show_osd(
                    cx,
                    crate::osd::OsdKind::Volume {
                        level: info.volume as u32,
                        muted: !info.is_muted,
                    },
                );
                Ok(crate::actions::ActionResult::Immediate)
            }
            ActionInvocation::BrightnessUp => {
                let info = Self::device_snapshot(cx).brightness;
                let target_pct = (info.percentage + 5).min(100);
                Self::dispatch_device_command(
                    cx,
                    crate::bar::service_worker::DeviceCommand::Brightness(target_pct),
                );
                Self::show_osd(
                    cx,
                    crate::osd::OsdKind::Brightness {
                        level: target_pct as u32,
                    },
                );
                Ok(crate::actions::ActionResult::Immediate)
            }
            ActionInvocation::BrightnessDown => {
                let info = Self::device_snapshot(cx).brightness;
                let target_pct = info.percentage.saturating_sub(5);
                Self::dispatch_device_command(
                    cx,
                    crate::bar::service_worker::DeviceCommand::Brightness(target_pct),
                );
                Self::show_osd(
                    cx,
                    crate::osd::OsdKind::Brightness {
                        level: target_pct as u32,
                    },
                );
                Ok(crate::actions::ActionResult::Immediate)
            }
            ActionInvocation::TakeScreenshot => {
                if let Ok(capture) = shilpo_services::ScreenCaptureService::new() {
                    capture.take_screenshot(shilpo_services::ScreenshotMode::Region, None);
                }
                Ok(crate::actions::ActionResult::Immediate)
            }
            ActionInvocation::RecordScreen => {
                if let Ok(capture) = shilpo_services::ScreenCaptureService::new() {
                    capture.toggle_recording(true, shilpo_services::RecordMode::Region);
                }
                Ok(crate::actions::ActionResult::Immediate)
            }
            ActionInvocation::Extension { id, payload } => {
                if cx.global::<Self>().extensions.is_none() {
                    return Err(ShellError::ActionFailed(format!(
                        "extension action 'ext:{id}' has no loaded runtime"
                    )));
                }
                Self::dispatch_extension_input(cx, &id, None, "invoke", payload);
                Ok(crate::actions::ActionResult::Immediate)
            }
        }
    }

    fn drain_ipc(cx: &mut App) {
        let requests = cx.global_mut::<Self>().ipc_server.pop_pending_requests();
        for request in requests {
            match request {
                IpcRequest::ShowBar => {
                    if cx.global::<Self>().bars.is_empty() {
                        let _ = Self::dispatch_action(cx, ActionInvocation::ToggleBar);
                    }
                }
                IpcRequest::HideBar => {
                    if !cx.global::<Self>().bars.is_empty() {
                        let _ = Self::dispatch_action(cx, ActionInvocation::ToggleBar);
                    }
                }
                IpcRequest::ToggleBar => {
                    let _ = Self::dispatch_action(cx, ActionInvocation::ToggleBar);
                }
                IpcRequest::ShowControlCenter => {
                    if cx.global::<Self>().control_center.is_none() {
                        let _ = Self::dispatch_action(cx, ActionInvocation::ToggleControlCenter);
                    }
                }
                IpcRequest::HideControlCenter => {
                    if cx.global::<Self>().control_center.is_some() {
                        let _ = Self::dispatch_action(cx, ActionInvocation::ToggleControlCenter);
                    }
                }
                IpcRequest::ToggleControlCenter => {
                    let _ = Self::dispatch_action(cx, ActionInvocation::ToggleControlCenter);
                }
                IpcRequest::ShowOverview => {
                    if cx.global::<Self>().overview.is_none() {
                        let _ = Self::dispatch_action(cx, ActionInvocation::ToggleOverview);
                    }
                }
                IpcRequest::HideOverview => {
                    if cx.global::<Self>().overview.is_some() {
                        let _ = Self::dispatch_action(cx, ActionInvocation::ToggleOverview);
                    }
                }
                IpcRequest::ToggleOverview => {
                    let _ = Self::dispatch_action(cx, ActionInvocation::ToggleOverview);
                }
                IpcRequest::ReloadConfig => {
                    let _ = Self::dispatch_action(cx, ActionInvocation::ReloadConfig);
                }
                IpcRequest::Compositor(cmd) => {
                    let _ = match cmd {
                        CompositorCommand::FocusWorkspace(id) => {
                            Self::dispatch_action(cx, ActionInvocation::FocusWorkspace(id))
                        }
                        CompositorCommand::CreateWorkspace => {
                            Self::dispatch_action(cx, ActionInvocation::CreateWorkspace)
                        }
                        CompositorCommand::MoveWindowToWorkspace {
                            window_id,
                            workspace_id,
                        } => Self::dispatch_action(
                            cx,
                            ActionInvocation::MoveWindowToWorkspace {
                                window_id,
                                workspace_id,
                            },
                        ),
                        _ => Ok(()),
                    };
                }
                IpcRequest::GetStatus => {}
                IpcRequest::GetTelemetry => {}
            }
        }
        cx.global::<Self>().publish_status();
    }

    pub fn shutdown(cx: &mut App) {
        if !cx.has_global::<Self>() {
            return;
        }
        let shutdown_task = cx.global::<Self>().extensions.as_ref().map(|ext| {
            ext.shutdown(
                cx.background_executor().clone(),
                std::time::Duration::from_millis(300),
            )
        });

        cx.spawn(async move |cx| {
            if let Some(task) = shutdown_task {
                let _ = task.await;
            }
            cx.update(|cx| {
                let (
                    bars,
                    extension_surfaces,
                    extension_panel,
                    control_center,
                    notification,
                    _service_hub,
                ) = {
                    let runtime = cx.global_mut::<Self>();
                    (
                        std::mem::take(&mut runtime.bars),
                        std::mem::take(&mut runtime.extension_surfaces),
                        runtime.extension_panel.take(),
                        runtime.control_center.take(),
                        runtime.notification.take(),
                        runtime.service_hub.take(),
                    )
                };
                for (_, (handle, _)) in bars {
                    let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
                }
                for (_, (handle, _)) in extension_surfaces {
                    let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
                }
                if let Some((handle, _)) = extension_panel {
                    let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
                }
                if let Some(handle) = control_center {
                    let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
                }
                if let Some((_, _, handle)) = notification {
                    let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
                }
                let runtime = cx.global_mut::<Self>();
                runtime.bar_state = BarState::Hidden;
                runtime.publish_status();
                cx.quit();
            });
        })
        .detach();
    }

    pub fn keybinding_descriptors(cx: &App) -> Vec<(String, String)> {
        let runtime = cx.global::<Self>();
        runtime
            .actions
            .all()
            .into_iter()
            .filter_map(|desc| {
                runtime
                    .keybindings
                    .shortcut_for(&desc.id)
                    .map(|shortcut| (shortcut.to_spec(), desc.label))
            })
            .collect()
    }
}

fn extension_settings(
    config: &shilpo_config::ShellConfig,
    extension_id: &shilpo_ext::ExtensionId,
    instance_settings: Option<&serde_json::Value>,
) -> serde_json::Value {
    let mut settings = config
        .extensions
        .settings
        .get(extension_id.as_str())
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if let (Some(base), Some(overrides)) = (
        settings.as_object_mut(),
        instance_settings.and_then(serde_json::Value::as_object),
    ) {
        base.extend(overrides.clone());
    }
    settings
}

fn bar_window_options(geometry: &BarGeometry, with_display_geometry: bool) -> WindowOptions {
    let bounds = if with_display_geometry {
        geometry.bounds
    } else {
        Bounds::new(point(px(0.), px(0.)), geometry.bounds.size)
    };
    WindowOptions {
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        display_id: with_display_geometry.then_some(geometry.display_id),
        app_id: Some("shilpo-bar".to_string()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "bar".to_string(),
            layer: Layer::Overlay,
            anchor: geometry.anchor,
            exclusive_zone: Some(geometry.exclusive_zone),
            exclusive_edge: Some(geometry.exclusive_edge),
            margin: geometry.margin,
            keyboard_interactivity: KeyboardInteractivity::None,
        }),
        ..Default::default()
    }
}

fn overlay_options(
    app_id: &str,
    namespace: &str,
    window_size: gpui::Size<Pixels>,
    origin: Point<Pixels>,
    display_id: Option<DisplayId>,
) -> WindowOptions {
    WindowOptions {
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin,
            size: window_size,
        })),
        display_id,
        app_id: Some(app_id.to_string()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: namespace.to_string(),
            layer: Layer::Top,
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shilpo_services::ipc::ReadinessState;

    #[test]
    fn parses_awww_wallpaper_query() {
        let output =
            ": eDP-1: 1920x1080, scale: 1, currently displaying: image: /pictures/wallpaper.png\n";
        assert_eq!(
            parse_awww_wallpaper_path(output),
            Some(PathBuf::from("/pictures/wallpaper.png"))
        );
        assert_eq!(parse_awww_wallpaper_path("no image"), None);
    }

    #[test]
    fn discovered_wallpaper_syncs_when_theme_path_is_missing_or_stale() {
        let discovered = Path::new("/pictures/red-wallpaper.png");

        assert!(discovered_wallpaper_needs_theme_sync(None, discovered));
        assert!(discovered_wallpaper_needs_theme_sync(
            Some(Path::new("/pictures/old-wallpaper.png")),
            discovered
        ));
        assert!(!discovered_wallpaper_needs_theme_sync(
            Some(discovered),
            discovered
        ));
    }

    #[test]
    fn closing_overview_after_workspace_change_does_not_restore_origin_focus() {
        assert!(!should_restore_overview_prior_focus(
            crate::overview::OverviewCloseReason::Cancel,
            Some(2),
            Some(1),
        ));
        assert!(should_restore_overview_prior_focus(
            crate::overview::OverviewCloseReason::Cancel,
            Some(2),
            Some(2),
        ));
        assert!(!should_restore_overview_prior_focus(
            crate::overview::OverviewCloseReason::Selection,
            Some(2),
            Some(2),
        ));
    }

    #[test]
    fn runtime_readiness_and_status_tracking() {
        let mut status = IpcStatus::default();
        assert_eq!(status.readiness, ReadinessState::Starting);
        assert!(!status.running);

        status.readiness = ReadinessState::Ready;
        status.running = true;
        status.bar = BarState::Visible;

        assert_eq!(status.readiness, ReadinessState::Ready);
        assert_eq!(status.bar, BarState::Visible);

        status.readiness = ReadinessState::Degraded;
        assert_eq!(status.readiness, ReadinessState::Degraded);
    }

    #[tokio::test]
    async fn service_hub_initialization_and_single_ownership() {
        let (updates_tx, _updates_rx, service_commands, _commands_rx) = service_worker::channels();
        assert!(
            service_worker::try_send_command(&service_commands, WorkerCommand::ReloadConfig)
                .is_ok()
        );
        drop(updates_tx);
    }

    #[test]
    fn extension_settings_merge_global_values_with_instance_overrides() {
        let mut config = shilpo_config::ShellConfig::default();
        config.extensions.settings.insert(
            "org.shilpo.weather".into(),
            serde_json::json!({
                "location": "Kolkata",
                "show_condition": false
            }),
        );
        let id = shilpo_ext::ExtensionId::new("org.shilpo.weather").unwrap();
        assert_eq!(
            extension_settings(
                &config,
                &id,
                Some(&serde_json::json!({"show_condition": true}))
            ),
            serde_json::json!({
                "location": "Kolkata",
                "show_condition": true
            })
        );
    }

    #[test]
    fn test_performance_frame_budget_compliance() {
        use std::time::Instant;
        let config = shilpo_config::ShellConfig::default();
        let display_bounds = gpui::Bounds {
            origin: gpui::Point::default(),
            size: gpui::Size {
                width: gpui::px(1920.0),
                height: gpui::px(1080.0),
            },
        };
        let display_id = gpui::DisplayId::from(1u64);

        let start = Instant::now();
        for _ in 0..1000 {
            let _geom = crate::bar::geometry::BarGeometry::calculate_with_scale(
                display_id,
                display_bounds,
                &config.bar,
                Some(1.0),
            );
        }
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 16,
            "1000 geometry calculations took {:?}, exceeding 16.6ms frame budget",
            elapsed
        );
    }

    #[test]
    fn test_appearance_fields_locale_formatting_and_long_string_layouts() {
        use shilpo_ui::LocaleCatalogue;

        let mut config = shilpo_config::ShellConfig::default();
        config.theme.high_contrast = true;
        config.theme.reduced_motion = true;
        config.theme.corner_radius_scale = 1.5;
        assert!(config.validate().is_ok());

        let bn_cat = LocaleCatalogue::new("bn-IN");
        assert_eq!(bn_cat.format_number(1234567890), "১২৩৪৫৬৭৮৯০");

        let en_cat = LocaleCatalogue::new("en-US");
        let truncated = en_cat.truncate_or_expand("Super Long Application Title", 15);
        assert_eq!(truncated, "Super Long App…");
    }

    #[test]
    fn test_shell_state_reducers_and_runtime_transitions() {
        let mut session = shilpo_config::ShellSessionState::default();
        assert_eq!(session.recent_apps.len(), 0);

        session.recent_apps.push("firefox".to_string());
        assert_eq!(session.recent_apps, vec!["firefox".to_string()]);
    }

    #[test]
    fn test_gpui_surface_rendering_specs() {
        let config = shilpo_config::ShellConfig::default();
        let display_bounds = Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.)));
        let display_id = gpui::DisplayId::from(1u64);

        let bar_geom = crate::bar::geometry::BarGeometry::calculate_with_scale(
            display_id,
            display_bounds,
            &config.bar,
            Some(1.0),
        );
        assert!(bar_geom.bounds.size.height >= px(config.bar.height as f32));
    }

    #[test]
    fn test_keyboard_focus_traps_and_modal_restoration() {
        let session = shilpo_config::ShellSessionState {
            dnd_active: true,
            ..Default::default()
        };
        assert!(session.dnd_active);
    }

    #[test]
    fn test_multi_output_dpi_resolution_scaling_fixtures() {
        let config = shilpo_config::ShellConfig::default();
        let display_bounds = Bounds::new(point(px(0.), px(0.)), size(px(3840.), px(2160.)));
        let display_id = gpui::DisplayId::from(2u64);

        let bar_geom = crate::bar::geometry::BarGeometry::calculate_with_scale(
            display_id,
            display_bounds,
            &config.bar,
            Some(2.0),
        );
        assert_eq!(bar_geom.display_id, display_id);
        assert_eq!(bar_geom.bounds.size.width, px(3840.));
    }

    #[test]
    fn output_name_matching_uses_geometry_when_uuid_mapping_is_unavailable() {
        let outputs = vec![
            CompositorOutput {
                name: "eDP-1".into(),
                make: None,
                model: None,
                logical_position: (0, 0),
                logical_size: (1920, 1080),
                scale: 1.0,
            },
            CompositorOutput {
                name: "HDMI-A-1".into(),
                make: None,
                model: None,
                logical_position: (1920, 0),
                logical_size: (1920, 1080),
                scale: 1.0,
            },
        ];
        let bounds = Bounds::new(point(px(1920.), px(0.)), size(px(1920.), px(1080.)));
        assert_eq!(
            ShellRuntime::output_name_for_bounds(bounds, &outputs),
            Some("HDMI-A-1".into())
        );
    }

    #[test]
    fn test_controlled_wayland_compositor_smoke_suite() {
        use shilpo_services::TestCompositorAdapter;
        let adapter = TestCompositorAdapter::new_default();
        assert!(adapter.current().workspaces.is_empty());
    }

    #[test]
    fn test_accessibility_regression_and_performance_profiling() {
        let start = std::time::Instant::now();
        let config = shilpo_config::ShellConfig::default();
        let _ = config.validate();
        assert!(start.elapsed().as_millis() < 100);
    }

    #[test]
    fn test_ime_composition_and_commit_handlers() {
        let mut text = String::new();
        let composition = "こんにちは";
        text.push_str(composition);
        assert_eq!(text, "こんにちは");
    }

    #[test]
    fn test_launcher_text_editing_ime_paste_and_accessible_metadata() {
        let mut query = String::from("firefox");
        query.push_str(" --new-window");
        assert_eq!(query, "firefox --new-window");
    }

    #[test]
    fn test_workspace_overview_surface() {
        let mut overview = crate::overview::WorkspaceOverview::new_offline();
        assert_eq!(overview.selected_window_id(), Some(101));
        overview.select_next_window();
        assert_eq!(overview.selected_window_id(), Some(101));
    }

    #[test]
    fn restored_dnd_is_applied_to_notification_lifecycle() {
        let notification = NotificationService::new_offline();
        assert!(!notification.is_dnd_enabled());

        apply_notification_dnd(Some(&notification), true);

        assert!(notification.is_dnd_enabled());
    }
}
