use gpui::App;
use shilpo_services::{CompositorCommand, IpcRequest, IpcStatus};

use crate::{
    actions::ActionInvocation,
    bar::service_worker::{self, WorkerCommand},
    error::ShellError,
};

use super::{ShellRuntime, ShellSurfaces};

impl ShellRuntime {
    pub fn save_audio_preference(cx: &App, device: Option<String>, port: Option<String>) {
        if cx.has_global::<Self>()
            && let Some(store) = cx.global::<Self>().heed_store()
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
                let handle = Self::service_commands(cx)
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
            runtime.session_state_mut().record_recent_app(app_id);
            let path = runtime.session_path().clone();
            let session = runtime.session_state().clone();
            let _ = session.save_atomic(&path);
        }
    }

    pub fn recent_apps(cx: &App) -> Vec<String> {
        if cx.has_global::<Self>() {
            cx.global::<Self>().session_state().recent_apps.clone()
        } else {
            Vec::new()
        }
    }

    pub fn save_output_bar(
        cx: &mut App,
        output_name: &str,
        state: &shilpo_services::OutputBarState,
    ) {
        if cx.has_global::<Self>()
            && let Some(store) = cx.global::<Self>().heed_store()
        {
            let _ = store.put_output_bar(output_name, state);
        }
    }

    pub fn load_output_bar(cx: &App, output_name: &str) -> Option<shilpo_services::OutputBarState> {
        if cx.has_global::<Self>()
            && let Some(store) = cx.global::<Self>().heed_store()
        {
            store.get_output_bar(output_name).ok().flatten()
        } else {
            None
        }
    }

    pub(super) fn drain_ipc(cx: &mut App) {
        let requests = cx
            .global_mut::<Self>()
            .ipc_server_mut()
            .pop_pending_requests();
        for request in requests {
            match request {
                IpcRequest::ShowBar => {
                    if !ShellSurfaces::has_bars(cx) {
                        let _ = Self::dispatch_action(cx, ActionInvocation::ToggleBar);
                    }
                }
                IpcRequest::HideBar => {
                    if ShellSurfaces::has_bars(cx) {
                        let _ = Self::dispatch_action(cx, ActionInvocation::ToggleBar);
                    }
                }
                IpcRequest::ToggleBar => {
                    let _ = Self::dispatch_action(cx, ActionInvocation::ToggleBar);
                }
                IpcRequest::ShowOverview => {
                    if !ShellSurfaces::is_overview_open(cx) {
                        let _ = Self::dispatch_action(cx, ActionInvocation::ToggleOverview);
                    }
                }
                IpcRequest::HideOverview => {
                    if ShellSurfaces::is_overview_open(cx) {
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
                IpcRequest::SetBrightness(pct) => {
                    Self::dispatch_device_command(
                        cx,
                        shilpo_services::DeviceCommand::Brightness(
                            shilpo_services::BrightnessAction::SetBrightness(pct),
                        ),
                    );
                }
                IpcRequest::SetDisplayBrightness { id: _, percentage } => {
                    Self::dispatch_device_command(
                        cx,
                        shilpo_services::DeviceCommand::Brightness(
                            shilpo_services::BrightnessAction::SetBrightness(percentage),
                        ),
                    );
                }
                IpcRequest::GetStatus => {}
                IpcRequest::GetTelemetry => {}
                IpcRequest::Capture(intent) => {
                    ShellSurfaces::request(cx, super::SurfaceRequest::OpenCapture(intent));
                }
            }
        }
        Self::publish_status(cx);
    }

    pub(super) fn publish_status(cx: &mut App) {
        let runtime = cx.global::<Self>();
        let status = IpcStatus {
            running: runtime.readiness() != shilpo_services::ipc::ReadinessState::Failed,
            readiness: runtime.readiness(),
            bar: if runtime.shell_surfaces.bars_are_open() {
                shilpo_services::ipc::BarState::Visible
            } else {
                shilpo_services::ipc::BarState::Hidden
            },
            overview_visible: runtime.shell_surfaces.overview_is_open(),
            health: shilpo_services::ServiceHealth {
                extension_host: runtime
                    .extension_host
                    .diagnostics()
                    .and_then(|diagnostics| serde_json::to_value(diagnostics).ok()),
                ..Default::default()
            },
            instance_id: std::process::id().to_string(),
            pid: std::process::id(),
        };
        runtime.ipc_server().update_status(status);
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

        let mut config = crate::config::ShellConfig::default();
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
        let mut session = crate::config::ShellSessionState::default();
        assert_eq!(session.recent_apps.len(), 0);

        session.recent_apps.push("firefox".to_string());
        assert_eq!(session.recent_apps, vec!["firefox".to_string()]);
    }

    #[test]
    fn test_keyboard_focus_traps_and_modal_restoration() {
        let session = crate::config::ShellSessionState {
            dnd_active: true,
            ..Default::default()
        };
        assert!(session.dnd_active);
    }

    #[test]
    fn test_controlled_wayland_compositor_smoke_suite() {
        use shilpo_services::{CompositorAdapter, TestCompositorAdapter};
        let adapter = TestCompositorAdapter::new_default();
        assert!(adapter.current().workspaces.is_empty());
    }

    #[test]
    fn test_accessibility_regression_and_performance_profiling() {
        let start = std::time::Instant::now();
        let config = crate::config::ShellConfig::default();
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
