pub mod action_dispatcher;
pub mod extension_host;
pub mod ipc;
pub mod service_hub;
pub mod session;
pub mod shell_surfaces;
pub mod theme_manager;
pub(crate) mod wallpaper_preview;

pub use action_dispatcher::ActionDispatcher;
pub use extension_host::ExtensionHost;
pub use service_hub::ServiceHub;
pub use session::SessionContext;
pub use shell_surfaces::{ShellSurfaces, SurfaceRequest, SurfaceSnapshot};
pub(crate) use wallpaper_preview::{WallpaperPreviewResource, WallpaperPreviewSnapshot};

use shell_surfaces::WindowClosedOutcome;

use std::{path::PathBuf, sync::Arc};

use crate::extensions::ExtensionCoordinator;
use gpui::{App, AppContext, Entity, Global, Subscription};
use shilpo_services::{CompositorSnapshot, ShellIpcServer};

/// The shell runtime orchestrator: composes the deep service modules, watches
/// the compositor stream, and routes lifecycle events between them.
///
/// Every component is private; the shell reaches them through the narrow
/// `pub(super)` accessor surface below.
pub struct ShellRuntime {
    ipc_server: ShellIpcServer,
    _shell_bus: Option<zbus::Connection>,
    active_config: shilpo_config::ShellConfig,
    shell_surfaces: ShellSurfaces,
    action_dispatcher: ActionDispatcher,
    extension_host: ExtensionHost,
    wallpaper_preview: Entity<WallpaperPreviewResource>,
    service_hub: Option<ServiceHub>,
    session_state: shilpo_config::ShellSessionState,
    session_path: PathBuf,
    heed_store: Option<Arc<shilpo_config::HeedSessionStore>>,
    _start_time: std::time::Instant,
    _window_closed: Option<Subscription>,
    _wallpaper_preview_changed: Option<Subscription>,
    _ipc_task: gpui::Task<()>,
}

impl Global for ShellRuntime {}

impl ShellRuntime {
    #[cfg(test)]
    pub(crate) fn install_for_test(cx: &mut App) {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "shilpo-shell-surface-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let ipc = ShellIpcServer::new_at(&root, &root.join("shilpo-shell/ipc.sock")).unwrap();
        let wallpaper_preview = cx.new(WallpaperPreviewResource::new);
        cx.set_global(Self {
            ipc_server: ipc,
            _shell_bus: None,
            active_config: shilpo_config::ShellConfig::default(),
            shell_surfaces: ShellSurfaces::new(Arc::new(CompositorSnapshot::default())),
            action_dispatcher: ActionDispatcher::new(),
            extension_host: ExtensionHost::new(None),
            wallpaper_preview,
            service_hub: None,
            session_state: shilpo_config::ShellSessionState::default(),
            session_path: root.join("session.json"),
            heed_store: None,
            _start_time: std::time::Instant::now(),
            _window_closed: None,
            _wallpaper_preview_changed: None,
            _ipc_task: gpui::Task::ready(()),
        });
    }

    pub(crate) fn ipc_server(&self) -> &ShellIpcServer {
        &self.ipc_server
    }

    pub(crate) fn ipc_server_mut(&mut self) -> &mut ShellIpcServer {
        &mut self.ipc_server
    }

    pub(crate) fn readiness(&self) -> shilpo_services::ipc::ReadinessState {
        self.shell_surfaces.readiness()
    }

    pub(crate) fn session_state(&self) -> &shilpo_config::ShellSessionState {
        &self.session_state
    }

    pub(crate) fn session_state_mut(&mut self) -> &mut shilpo_config::ShellSessionState {
        &mut self.session_state
    }

    pub(crate) fn session_path(&self) -> &PathBuf {
        &self.session_path
    }

    pub(crate) fn heed_store(&self) -> Option<&Arc<shilpo_config::HeedSessionStore>> {
        self.heed_store.as_ref()
    }

    pub(crate) fn shell_surfaces(&self) -> &ShellSurfaces {
        &self.shell_surfaces
    }

    pub(crate) fn shell_surfaces_mut(&mut self) -> &mut ShellSurfaces {
        &mut self.shell_surfaces
    }

    pub(crate) fn action_dispatcher(&self) -> &ActionDispatcher {
        &self.action_dispatcher
    }

    pub(crate) fn action_dispatcher_mut(&mut self) -> &mut ActionDispatcher {
        &mut self.action_dispatcher
    }

    pub(crate) fn extension_host(&self) -> &ExtensionHost {
        &self.extension_host
    }

    pub(crate) fn extension_host_mut(&mut self) -> &mut ExtensionHost {
        &mut self.extension_host
    }

    pub(crate) fn wallpaper_preview(cx: &App) -> Entity<WallpaperPreviewResource> {
        cx.global::<Self>().wallpaper_preview.clone()
    }

    pub(crate) fn wallpaper_preview_snapshot(cx: &App) -> WallpaperPreviewSnapshot {
        let entity = Self::wallpaper_preview(cx);
        entity.read(cx).snapshot()
    }

    pub(crate) fn set_wallpaper_path(cx: &mut App, path: Option<PathBuf>) {
        let entity = Self::wallpaper_preview(cx);
        entity.update(cx, |wp, cx| {
            wp.set_wallpaper_path(path, cx);
        });
    }

    pub(crate) fn service_hub(&self) -> Option<&ServiceHub> {
        self.service_hub.as_ref()
    }

    pub(crate) fn service_hub_mut(&mut self) -> Option<&mut ServiceHub> {
        self.service_hub.as_mut()
    }

    pub fn active_config(cx: &App) -> shilpo_config::ShellConfig {
        if cx.has_global::<Self>() {
            cx.global::<Self>().active_config.clone()
        } else {
            shilpo_config::ShellConfig::default()
        }
    }

    pub(crate) fn set_active_config(cx: &mut App, config: &shilpo_config::ShellConfig) {
        if cx.has_global::<Self>() {
            cx.global_mut::<Self>().active_config = config.clone();
        }
    }

    pub fn install(cx: &mut App, ipc_server: ShellIpcServer, shell_bus: zbus::Connection) {
        let initial_wallpaper_path = theme_manager::init(cx);
        let session = session::SessionContext::init();
        let hub = ServiceHub::start(cx.background_executor().clone(), &session);
        let extensions = ExtensionCoordinator::init(cx.background_executor().clone());

        let compositor = hub.compositor();
        let latest_snapshot = shell_surfaces::attach_compositor_stream(&ipc_server, &compositor);
        let shell_surfaces = ShellSurfaces::new(latest_snapshot.clone());
        let action_dispatcher = ActionDispatcher::new();
        let extension_host = ExtensionHost::new(extensions);
        let wallpaper_preview = cx.new(WallpaperPreviewResource::new);
        wallpaper_preview.update(cx, |wp, cx| {
            wp.set_wallpaper_path(initial_wallpaper_path.clone(), cx);
        });

        cx.set_global(Self {
            ipc_server,
            _shell_bus: Some(shell_bus),
            active_config: session.active_config,
            shell_surfaces,
            action_dispatcher,
            extension_host,
            wallpaper_preview: wallpaper_preview.clone(),
            service_hub: Some(hub),
            session_state: session.session_state,
            session_path: session.session_path,
            heed_store: session.heed_store,
            _start_time: std::time::Instant::now(),
            _window_closed: None,
            _wallpaper_preview_changed: None,
            _ipc_task: gpui::Task::ready(()),
        });

        let wallpaper_preview_changed = cx.observe(&wallpaper_preview, |_, cx| {
            crate::bar::cards::adapter::CardCoordinator::refresh_owner(
                cx,
                &crate::bar::cards::workspace_card::workspace_owner_id(),
            );
        });
        cx.global_mut::<Self>()._wallpaper_preview_changed = Some(wallpaper_preview_changed);

        shell_surfaces::spawn_compositor_stream_loop(cx, &compositor);
        theme_manager::sync_wallpaper(cx, initial_wallpaper_path);
        Self::on_compositor_snapshot_changed(cx, latest_snapshot);
        Self::spawn_window_closed_watch(cx);
        ExtensionHost::sync_extension_actions(cx);
        Self::spawn_drain_loop(cx);
        Self::publish_status(cx);
    }

    fn spawn_window_closed_watch(cx: &mut App) {
        let subscription = cx.on_window_closed(|cx, window_id| {
            if !cx.has_global::<ShellRuntime>() {
                return;
            }
            let outcome = cx
                .global_mut::<ShellRuntime>()
                .shell_surfaces
                .handle_window_closed(window_id);
            crate::bar::cards::adapter::CardCoordinator::forget_window(cx, window_id);
            Self::publish_status(cx);
            match outcome {
                WindowClosedOutcome::Nothing => {}
                WindowClosedOutcome::Capture => ShellSurfaces::restore_prior_focus(cx),
                WindowClosedOutcome::ExtensionPanel(contribution) => {
                    Self::dispatch_extension_event(
                        cx,
                        shilpo_ext_api::ExtensionEvent::ContributionUnmounted {
                            contribution_id: contribution.contribution_id.to_string(),
                            instance_id: None,
                        },
                    );
                }
            }
        });
        cx.global_mut::<Self>()._window_closed = Some(subscription);
    }

    fn spawn_drain_loop(cx: &mut App) {
        let task = cx.spawn(async move |cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
                cx.update(|cx| ShellSurfaces::request(cx, SurfaceRequest::SyncDisplays));
                cx.update(ServiceHub::drain);
                cx.update(Self::drain_extensions);
                cx.update(Self::publish_status);
                cx.update(Self::drain_ipc);
            }
        });
        cx.global_mut::<Self>()._ipc_task = task;
    }

    pub fn on_compositor_snapshot_changed(cx: &mut App, snapshot: Arc<CompositorSnapshot>) {
        if !cx.has_global::<Self>() {
            return;
        }
        let outputs_changed = {
            let runtime = cx.global_mut::<Self>();
            let changed = runtime.shell_surfaces.latest_snapshot().outputs != snapshot.outputs;
            runtime.shell_surfaces.set_latest_snapshot(snapshot.clone());
            changed
        };
        cx.global_mut::<Self>()
            .action_dispatcher
            .update_enabled_for_snapshot(&snapshot);
        cx.global_mut::<Self>().shell_surfaces.update_readiness();
        Self::publish_status(cx);
        ShellSurfaces::refresh_bars(cx);
        crate::bar::cards::adapter::CardCoordinator::refresh_owner(
            cx,
            &crate::bar::cards::workspace_card::workspace_owner_id(),
        );
        if outputs_changed {
            ShellSurfaces::reconcile_bars(cx);
        }
    }

    pub fn shutdown(cx: &mut App) {
        if !cx.has_global::<Self>() {
            return;
        }
        let shutdown_task = cx.global::<Self>().extension_host.shutdown_task(
            cx.background_executor().clone(),
            std::time::Duration::from_millis(300),
        );

        cx.spawn(async move |cx| {
            if let Some(task) = shutdown_task {
                let _ = task.await;
            }
            cx.update(|cx| {
                // Dismiss all card channels on shutdown.
                crate::bar::cards::adapter::CardCoordinator::dispatch(
                    cx,
                    crate::bar::cards::model::CardRequest::Shutdown,
                );
                crate::bar::cards::adapter::CardCoordinator::destroy_all_bands(cx);
                let windows = cx
                    .global_mut::<Self>()
                    .shell_surfaces
                    .take_windows_for_shutdown();
                for (_, (handle, _)) in windows.bars {
                    let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
                }
                for (_, (handle, _)) in windows.extension_surfaces {
                    let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
                }
                if let Some((handle, _)) = windows.extension_panel {
                    let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
                }
                if let Some((_, _, handle)) = windows.notification {
                    let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
                }
                if let Some((_, handle)) = windows.capture {
                    let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
                }
                cx.global_mut::<Self>().service_hub = None;
                Self::publish_status(cx);
                cx.quit();
            });
        })
        .detach();
    }
}
