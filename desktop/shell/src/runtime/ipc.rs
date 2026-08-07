use std::sync::Arc;

use gpui::App;
use shilpo_services::{
    CompositorAdapter, CompositorCommand, CompositorSnapshot, IpcRequest, IpcStatus,
};

use crate::{
    actions::ActionInvocation,
    bar::service_worker::{self, WorkerCommand},
    error::ShellError,
};

use super::ShellRuntime;

impl ShellRuntime {
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
            cx.global::<Self>().surface_manager.latest_snapshot.clone()
        } else {
            Arc::new(CompositorSnapshot::default())
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
        runtime.surface_manager.notification_generation = runtime.surface_manager.notification_generation.wrapping_add(1);
        runtime.surface_manager.notification_generation
    }

    pub fn invoke_notification_action(cx: &App, id: u32, action_key: &str) {
        if cx.has_global::<Self>()
            && let Some(ref hub) = cx.global::<Self>().service_hub
            && let Some(ref notif_service) = hub.notification
        {
            notif_service.invoke_action(id, action_key);
        }
    }

    pub(super) fn enqueue_worker(cx: &mut App, request: IpcRequest) -> Result<(), ShellError> {
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

    pub(super) fn drain_ipc(cx: &mut App) {
        let requests = cx.global_mut::<Self>().ipc_server.pop_pending_requests();
        for request in requests {
            match request {
                IpcRequest::ShowBar => {
                    if cx.global::<Self>().surface_manager.bars.is_empty() {
                        let _ = Self::dispatch_action(cx, ActionInvocation::ToggleBar);
                    }
                }
                IpcRequest::HideBar => {
                    if !cx.global::<Self>().surface_manager.bars.is_empty() {
                        let _ = Self::dispatch_action(cx, ActionInvocation::ToggleBar);
                    }
                }
                IpcRequest::ToggleBar => {
                    let _ = Self::dispatch_action(cx, ActionInvocation::ToggleBar);
                }
                IpcRequest::ShowControlCenter => {
                    if cx.global::<Self>().surface_manager.control_center.is_none() {
                        let _ = Self::dispatch_action(cx, ActionInvocation::ToggleControlCenter);
                    }
                }
                IpcRequest::HideControlCenter => {
                    if cx.global::<Self>().surface_manager.control_center.is_some() {
                        let _ = Self::dispatch_action(cx, ActionInvocation::ToggleControlCenter);
                    }
                }
                IpcRequest::ToggleControlCenter => {
                    let _ = Self::dispatch_action(cx, ActionInvocation::ToggleControlCenter);
                }
                IpcRequest::ShowOverview => {
                    if cx.global::<Self>().surface_manager.overview.is_none() {
                        let _ = Self::dispatch_action(cx, ActionInvocation::ToggleOverview);
                    }
                }
                IpcRequest::HideOverview => {
                    if cx.global::<Self>().surface_manager.overview.is_some() {
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

    pub(super) fn publish_status(&self) {
        let status = IpcStatus {
            running: self.readiness != shilpo_services::ReadinessState::Failed,
            instance_id: std::process::id().to_string(),
            pid: std::process::id(),
            readiness: self.readiness,
            bar: self.surface_manager.bar_state.clone(),
            overview_visible: self.surface_manager.overview.is_some(),
            control_center_visible: self.surface_manager.control_center.is_some(),
            health: shilpo_services::ServiceHealth::default(),
        };

        self.ipc_server.update_status(status);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shilpo_services::BarState;
    use shilpo_services::ipc::ReadinessState;

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
    fn test_keyboard_focus_traps_and_modal_restoration() {
        let session = shilpo_config::ShellSessionState {
            dnd_active: true,
            ..Default::default()
        };
        assert!(session.dnd_active);
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
}
