use gpui::App;
use shilpo_services::CompositorCommand;

use crate::{
    actions::{ActionId, ActionInvocation, ActionRegistry},
    error::ShellError,
};

use super::ShellRuntime;

impl ShellRuntime {
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

    pub fn focus_workspace(cx: &mut App, ws_id: u64) -> Result<(), ShellError> {
        Self::dispatch_action(cx, ActionInvocation::FocusWorkspace(ws_id))
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

    pub(super) fn show_compositor_error_toast(cx: &mut App, error: &shilpo_services::CompositorCommandError) {
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
                Self::enqueue_worker(cx, shilpo_services::IpcRequest::ReloadConfig)?;
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
