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

use std::{path::PathBuf, sync::Arc};

use gpui::{App, AppContext, Global, Subscription};
use shilpo_services::{BarState, CompositorConnection, CompositorSnapshot, ShellIpcServer};

use crate::{
    actions::ActionId,
    extensions::{ContributionSurface, ExtensionCoordinator},
};

pub struct ShellRuntime {
    pub(super) ipc_server: ShellIpcServer,
    pub(super) active_config: shilpo_config::ShellConfig,
    pub(super) readiness: shilpo_services::ipc::ReadinessState,
    pub(super) surface_manager: SurfaceManager,
    pub(super) action_dispatcher: ActionDispatcher,
    pub(super) extension_host: ExtensionHost,
    pub(super) service_hub: Option<ServiceHub>,
    pub(super) session_state: shilpo_config::ShellSessionState,
    pub(super) session_path: PathBuf,
    pub heed_store: Option<Arc<shilpo_config::HeedSessionStore>>,
    pub(super) _start_time: std::time::Instant,
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
        let initial_wallpaper_path = theme_manager::init(cx);
        let session = session::SessionContext::init();
        let hub = ServiceHub::start(cx.background_executor().clone(), &session);
        let extensions = ExtensionCoordinator::init(cx.background_executor().clone());

        let compositor = hub.compositor.clone();
        let latest_snapshot = surface_manager::attach_compositor_stream(&ipc_server, &compositor);
        let surface_manager = SurfaceManager::new(initial_wallpaper_path.clone(), latest_snapshot.clone());
        let action_dispatcher = ActionDispatcher::new();
        let extension_host = ExtensionHost::new(extensions);

        cx.set_global(Self {
            ipc_server,
            active_config: session.active_config,
            readiness: shilpo_services::ipc::ReadinessState::Starting,
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
        Self::sync_extension_actions(cx);
        Self::spawn_drain_loop(cx);
        cx.global::<Self>().publish_status();
    }

    fn spawn_window_closed_watch(cx: &mut App) {
        let subscription = cx.on_window_closed(|cx, window_id| {
            if !cx.has_global::<ShellRuntime>() {
                return;
            }
            let runtime = cx.global_mut::<ShellRuntime>();
            runtime
                .surface_manager
                .bars
                .retain(|_, (handle, _)| handle.window_id() != window_id);
            runtime
                .surface_manager
                .extension_surfaces
                .retain(|_, (handle, _)| handle.window_id() != window_id);
            if runtime.surface_manager.bars.is_empty() {
                runtime.surface_manager.bar_state = BarState::Hidden;
            }
            let closed_control_center = if runtime
                .surface_manager
                .control_center
                .as_ref()
                .is_some_and(|handle| handle.window_id() == window_id)
            {
                runtime.surface_manager.control_center = None;
                true
            } else {
                false
            };
            let closed_extension_panel = if runtime
                .surface_manager
                .extension_panel
                .as_ref()
                .is_some_and(|(handle, _)| handle.window_id() == window_id)
            {
                runtime.surface_manager.extension_panel.take().map(|(_, id)| id)
            } else {
                None
            };
            if runtime
                .surface_manager
                .notification
                .as_ref()
                .is_some_and(|(_, _, handle)| handle.window_id() == window_id)
            {
                runtime.surface_manager.notification = None;
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
    }

    fn spawn_drain_loop(cx: &mut App) {
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
    }

    pub fn on_compositor_snapshot_changed(cx: &mut App, snapshot: Arc<CompositorSnapshot>) {
        if !cx.has_global::<Self>() {
            return;
        }
        let (outputs_changed, overview_entity) = {
            let runtime = cx.global_mut::<Self>();
            let outputs_changed = runtime.surface_manager.latest_snapshot.outputs != snapshot.outputs;
            runtime.surface_manager.latest_snapshot = snapshot.clone();

            let is_ready = snapshot.connection.is_ready();
            if let Some(desc) = runtime.action_dispatcher.actions.descriptor_mut(&ActionId::FocusWorkspace) {
                desc.enabled = is_ready && snapshot.capabilities.can_focus_workspace;
            }
            if let Some(desc) = runtime.action_dispatcher.actions.descriptor_mut(&ActionId::CreateWorkspace) {
                desc.enabled = is_ready && snapshot.capabilities.can_create_workspace;
            }
            if let Some(desc) = runtime
                .action_dispatcher
                .actions
                .descriptor_mut(&ActionId::MoveWindowToWorkspace)
            {
                desc.enabled = is_ready && snapshot.capabilities.can_move_window;
            }
            if let Some(desc) = runtime.action_dispatcher.actions.descriptor_mut(&ActionId::FocusWindow) {
                desc.enabled = is_ready && snapshot.capabilities.can_focus_window;
            }
            if let Some(desc) = runtime.action_dispatcher.actions.descriptor_mut(&ActionId::CloseWindow) {
                desc.enabled = is_ready && snapshot.capabilities.can_close_window;
            }

            let bar_ok = matches!(runtime.surface_manager.bar_state, BarState::Visible | BarState::Hidden);
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
            (outputs_changed, runtime.surface_manager.overview_entity.clone())
        };

        if let Some(overview) = overview_entity {
            overview.update(cx, |view, cx| view.update_snapshot(snapshot, cx));
        }

        Self::refresh_bars(cx);

        if outputs_changed {
            Self::reconcile_bars(cx);
        }
    }

    pub fn shutdown(cx: &mut App) {
        if !cx.has_global::<Self>() {
            return;
        }
        let shutdown_task = cx.global::<Self>().extension_host.extensions.as_ref().map(|ext| {
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
                        std::mem::take(&mut runtime.surface_manager.bars),
                        std::mem::take(&mut runtime.surface_manager.extension_surfaces),
                        runtime.surface_manager.extension_panel.take(),
                        runtime.surface_manager.control_center.take(),
                        runtime.surface_manager.notification.take(),
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
                runtime.surface_manager.bar_state = BarState::Hidden;
                runtime.publish_status();
                cx.quit();
            });
        })
        .detach();
    }
}
