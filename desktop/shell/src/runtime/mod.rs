pub mod action_dispatcher;
pub mod extension_host;
pub mod ipc;
pub mod service_hub;
pub mod session;
pub mod surface_manager;
pub mod theme_manager;

pub use action_dispatcher::ActionDispatcher;
pub use extension_host::ExtensionHost;
pub use service_hub::ServiceHub;
pub use session::SessionContext;
pub use surface_manager::SurfaceManager;

use surface_manager::WindowClosedOutcome;

use std::{path::PathBuf, sync::Arc};

use gpui::layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions};
use gpui::{
    App, AppContext, Global, Subscription, WindowBackgroundAppearance, WindowKind, WindowOptions,
};
use shilpo_capture::{CaptureIntent, capture_frame, frame_to_rgba};
use shilpo_services::{CompositorSnapshot, ShellIpcServer};

use crate::capture::CaptureOverlayView;
use crate::extensions::{ContributionSurface, ExtensionCoordinator};

/// The shell runtime orchestrator: composes the deep service modules, watches
/// the compositor stream, and routes lifecycle events between them.
///
/// Every component is private; the shell reaches them through the narrow
/// `pub(super)` accessor surface below.
pub struct ShellRuntime {
    ipc_server: ShellIpcServer,
    active_config: shilpo_config::ShellConfig,
    surface_manager: SurfaceManager,
    action_dispatcher: ActionDispatcher,
    extension_host: ExtensionHost,
    service_hub: Option<ServiceHub>,
    session_state: shilpo_config::ShellSessionState,
    session_path: PathBuf,
    heed_store: Option<Arc<shilpo_config::HeedSessionStore>>,
    _start_time: std::time::Instant,
    _window_closed: Option<Subscription>,
    _ipc_task: gpui::Task<()>,
}

impl Global for ShellRuntime {}

impl ShellRuntime {
    pub(crate) fn ipc_server(&self) -> &ShellIpcServer {
        &self.ipc_server
    }

    pub(crate) fn ipc_server_mut(&mut self) -> &mut ShellIpcServer {
        &mut self.ipc_server
    }

    pub(crate) fn readiness(&self) -> shilpo_services::ipc::ReadinessState {
        self.surface_manager.readiness()
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

    pub(crate) fn surface_manager(&self) -> &SurfaceManager {
        &self.surface_manager
    }

    pub(crate) fn surface_manager_mut(&mut self) -> &mut SurfaceManager {
        &mut self.surface_manager
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

    pub fn install(cx: &mut App, ipc_server: ShellIpcServer) {
        let initial_wallpaper_path = theme_manager::init(cx);
        let session = session::SessionContext::init();
        let hub = ServiceHub::start(cx.background_executor().clone(), &session);
        let extensions = ExtensionCoordinator::init(cx.background_executor().clone());

        let compositor = hub.compositor();
        let latest_snapshot = surface_manager::attach_compositor_stream(&ipc_server, &compositor);
        let surface_manager =
            SurfaceManager::new(initial_wallpaper_path.clone(), latest_snapshot.clone());
        let action_dispatcher = ActionDispatcher::new();
        let extension_host = ExtensionHost::new(extensions);

        cx.set_global(Self {
            ipc_server,
            active_config: session.active_config,
            surface_manager,
            action_dispatcher,
            extension_host,
            service_hub: Some(hub),
            session_state: session.session_state,
            session_path: session.session_path,
            heed_store: session.heed_store,
            _start_time: std::time::Instant::now(),
            _window_closed: None,
            _ipc_task: gpui::Task::ready(()),
        });

        surface_manager::spawn_compositor_stream_loop(cx, &compositor);
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
                .surface_manager
                .handle_window_closed(window_id);
            Self::publish_status(cx);
            match outcome {
                WindowClosedOutcome::Nothing => {}
                WindowClosedOutcome::ControlCenter => {
                    Self::dispatch_surface_lifecycle(
                        cx,
                        ContributionSurface::ControlCenter,
                        false,
                        340.,
                        540.,
                    );
                }
                WindowClosedOutcome::ExtensionPanel(contribution) => {
                    Self::dispatch_extension_event(
                        cx,
                        shilpo_ext::ExtensionEvent::ContributionUnmounted {
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
                cx.update(Self::sync_displays);
                cx.update(ServiceHub::drain);
                cx.update(Self::drain_extensions);
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
            let changed = runtime.surface_manager.latest_snapshot().outputs != snapshot.outputs;
            runtime
                .surface_manager
                .set_latest_snapshot(snapshot.clone());
            changed
        };
        cx.global_mut::<Self>()
            .action_dispatcher
            .update_enabled_for_snapshot(&snapshot);
        cx.global_mut::<Self>().surface_manager.update_readiness();
        Self::publish_status(cx);
        SurfaceManager::refresh_bars(cx);
        if outputs_changed {
            SurfaceManager::reconcile_bars(cx);
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
                let windows = cx
                    .global_mut::<Self>()
                    .surface_manager
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
                if let Some(handle) = windows.control_center {
                    let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
                }
                if let Some((_, _, handle)) = windows.notification {
                    let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
                }
                cx.global_mut::<Self>().service_hub = None;
                Self::publish_status(cx);
                cx.quit();
            });
        })
        .detach();
    }

    pub fn open_capture_overlay(cx: &mut App, intent: CaptureIntent) {
        if !cx.has_global::<Self>() {
            return;
        }
        let config = cx.global::<Self>().active_config.capture.clone();
        let frame = match capture_frame(None).and_then(|frame| frame_to_rgba(&frame)) {
            Ok(frame) => frame,
            Err(error) => {
                tracing::warn!(%error, "failed to prepare screenshot overlay");
                return;
            }
        };
        let options = WindowOptions {
            window_background: WindowBackgroundAppearance::Transparent,
            kind: WindowKind::LayerShell(LayerShellOptions {
                namespace: "capture-overlay".to_string(),
                layer: Layer::Overlay,
                anchor: Anchor::all(),
                keyboard_interactivity: KeyboardInteractivity::Exclusive,
                ..Default::default()
            }),
            ..Default::default()
        };
        if let Err(error) = cx.open_window(options, move |window, cx| {
            CaptureOverlayView::view(frame, intent, config, window, cx)
        }) {
            tracing::warn!(%error, "failed to open capture overlay");
        }
    }
}
