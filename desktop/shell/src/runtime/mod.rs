pub mod action_dispatcher;
pub mod extension_host;
pub mod ipc;
pub mod service_hub;
pub mod surface_manager;

pub use service_hub::ServiceHub;

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use gpui::{App, AppContext, DisplayId, Entity, Global, Subscription, WindowHandle};
use shilpo_services::{BarState, CompositorConnection, CompositorSnapshot, ShellIpcServer};

use crate::{
    actions::{ActionId, ActionRegistry},
    bar::{BarView, geometry::BarGeometry},
    extensions::{ContributionSurface, ExtensionCoordinator},
};

use extension_host::ExtensionSurfaceSpec;

pub struct ShellRuntime {
    pub(super) ipc_server: ShellIpcServer,
    pub(super) active_config: shilpo_config::ShellConfig,
    pub(super) bars: HashMap<DisplayId, (WindowHandle<BarView>, crate::bar::BarSpec)>,
    pub(super) last_bar_specs: Vec<(BarGeometry, bool)>,
    pub(super) bar_state: BarState,
    pub(super) readiness: shilpo_services::ipc::ReadinessState,
    pub(super) control_center: Option<WindowHandle<shilpo_ui::Root>>,
    pub(super) overview: Option<WindowHandle<shilpo_ui::Root>>,
    pub(super) overview_entity: Option<Entity<crate::overview::WorkspaceOverview>>,
    pub(super) overview_instance: u64,
    pub(super) next_overview_instance: u64,
    pub(super) overview_opened_workspace_id: Option<u64>,
    pub(super) current_wallpaper_path: Option<PathBuf>,
    pub(super) latest_snapshot: Arc<CompositorSnapshot>,
    pub(super) notification: Option<(
        u64,
        u32,
        WindowHandle<crate::notification::NotificationToastView>,
    )>,
    pub(super) notification_generation: u64,
    pub(super) prior_window_id: Option<u64>,
    pub(super) osd: Option<(
        u64,
        WindowHandle<shilpo_ui::Root>,
        Entity<crate::osd::OsdView>,
    )>,
    pub(super) _osd_generation: u64,
    pub(super) extensions: Option<ExtensionCoordinator>,
    pub(super) extension_surfaces:
        HashMap<String, (WindowHandle<shilpo_ui::Root>, ExtensionSurfaceSpec)>,
    pub(super) extension_panel: Option<(WindowHandle<shilpo_ui::Root>, shilpo_ext::CanonicalId)>,
    pub(super) extension_output_ids: std::collections::HashSet<DisplayId>,
    pub(super) extension_tasks: std::collections::HashMap<
        (
            crate::extensions::ExtensionGeneration,
            shilpo_ext::ExtensionId,
            String,
        ),
        gpui::Task<()>,
    >,
    pub(super) extension_location_service: shilpo_services::LocationService,
    pub(super) actions: ActionRegistry,
    pub(super) keybindings: crate::actions::KeybindingManager,
    pub(super) session_state: shilpo_config::ShellSessionState,
    pub(super) session_path: PathBuf,
    pub heed_store: Option<Arc<shilpo_config::HeedSessionStore>>,
    pub(super) _start_time: std::time::Instant,
    pub(super) service_hub: Option<ServiceHub>,
    pub(super) _window_closed: Option<Subscription>,
    pub(super) _ipc_task: gpui::Task<()>,
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

    pub fn service_commands(cx: &App) -> Option<crate::bar::service_worker::CommandSender> {
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
        let theme_client = futures_lite::future::block_on(shilpo_theme_daemon::ThemeClient::new());
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
        service_hub::apply_notification_dnd(hub.notification.as_ref(), session_state.dnd_active);
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
            _start_time: std::time::Instant::now(),
            service_hub: Some(hub),
            _window_closed: None,
            _ipc_task: gpui::Task::ready(()),
        });
        let wallpaper_probe =
            cx.background_spawn(async { surface_manager::query_awww_wallpaper_path() });
        let theme_wallpaper_path = initial_wallpaper_path;
        shilpo_theme_daemon::ThemeClient::spawn_task(async move {
            let client = shilpo_theme_daemon::ThemeClient::new().await;
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
}
