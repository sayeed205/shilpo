use gpui::App;
use shilpo_ext_api::CanonicalId;
use shilpo_services::{CompositorCommand, CompositorSnapshot, Notification};

use crate::{
    actions::{ActionDescriptor, ActionId, ActionInvocation, ActionRegistry},
    error::ShellError,
    extensions::ContributionDescriptor,
};

use super::{ShellRuntime, ShellSurfaces, shell_surfaces::SurfaceRequest};

/// Owns the shell action registry and the keybinding table, plus the logic that
/// maps `ActionInvocation`s onto shell behavior and compositor commands.
///
/// Registry and keybinding state are private; the shell interacts with the
/// dispatcher exclusively through the method surface below.
pub struct ActionDispatcher {
    actions: ActionRegistry,
    keybindings: crate::actions::KeybindingManager,
}

impl ActionDispatcher {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            actions: ActionRegistry::default(),
            keybindings: crate::actions::KeybindingManager::with_defaults(),
        }
    }

    pub(crate) fn update_shortcut(&mut self, spec: &str, action: ActionId) -> Result<(), String> {
        let shortcut = crate::actions::Shortcut::parse(spec)
            .ok_or_else(|| format!("invalid shortcut specification: '{}'", spec))?;
        self.keybindings.register(shortcut, action)
    }

    pub(crate) fn update_shortcut_with_override(
        &mut self,
        spec: &str,
        action: ActionId,
    ) -> Result<Option<ActionId>, String> {
        let shortcut = crate::actions::Shortcut::parse(spec)
            .ok_or_else(|| format!("invalid shortcut specification: '{}'", spec))?;
        Ok(self.keybindings.register_with_override(shortcut, action))
    }

    pub(crate) fn reset_shortcuts_to_defaults(&mut self) {
        self.keybindings.reset_to_defaults();
    }

    pub(crate) fn register_extension_action(
        &mut self,
        id: CanonicalId,
        name: impl Into<String>,
        label: impl Into<String>,
    ) -> Result<ActionId, String> {
        self.actions.register_extension(id, name, label)
    }

    pub(crate) fn action_descriptors(&self) -> Vec<ActionDescriptor> {
        self.actions.all()
    }

    pub(crate) fn keybinding_descriptors(&self) -> Vec<(String, String)> {
        self.actions
            .all()
            .into_iter()
            .filter_map(|desc| {
                self.keybindings
                    .shortcut_for(&desc.id)
                    .map(|shortcut| (shortcut.to_spec(), desc.label))
            })
            .collect()
    }

    /// Reconciles the extension actions with the currently loaded extensions.
    pub(crate) fn sync_extension_actions(&mut self, desired: Vec<ContributionDescriptor>) {
        let existing = self
            .actions
            .all()
            .into_iter()
            .filter_map(|descriptor| descriptor.id.extension_id())
            .collect::<Vec<_>>();
        for id in existing {
            self.actions.unregister_extension(&id);
        }
        for descriptor in desired {
            if let Err(error) = self.actions.register_extension(
                descriptor.id,
                descriptor.extension_name.clone(),
                descriptor.name,
            ) {
                tracing::warn!(error = %error, "extension action registration failed");
            }
        }
    }

    /// Reflects compositor readiness and capabilities in the enabled flags of
    /// the compositor-backed actions.
    pub(crate) fn update_enabled_for_snapshot(&mut self, snapshot: &CompositorSnapshot) {
        let is_ready = snapshot.connection.is_ready();
        let set = |actions: &mut ActionRegistry, id: &ActionId, enabled: bool| {
            if let Some(desc) = actions.descriptor_mut(id) {
                desc.enabled = enabled;
            }
        };
        set(
            &mut self.actions,
            &ActionId::FocusWorkspace,
            is_ready && snapshot.capabilities.can_focus_workspace,
        );
        set(
            &mut self.actions,
            &ActionId::CreateWorkspace,
            is_ready && snapshot.capabilities.can_create_workspace,
        );
        set(
            &mut self.actions,
            &ActionId::MoveWindowToWorkspace,
            is_ready && snapshot.capabilities.can_move_window,
        );
        set(
            &mut self.actions,
            &ActionId::FocusWindow,
            is_ready && snapshot.capabilities.can_focus_window,
        );
        set(
            &mut self.actions,
            &ActionId::CloseWindow,
            is_ready && snapshot.capabilities.can_close_window,
        );
    }

    pub(crate) fn dispatch_action(
        cx: &mut App,
        action: ActionInvocation,
    ) -> Result<(), ShellError> {
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
                            Self::show_compositor_error_message(cx, &err.to_string());
                        });
                    }
                })
                .detach();
                Ok(())
            }
            Err(err) => {
                tracing::warn!(error = %err, "action invocation failed");
                Self::show_compositor_error_message(cx, &err.to_string());
                Err(err)
            }
        }
    }

    pub(crate) fn show_compositor_error_toast(
        cx: &mut App,
        error: &shilpo_services::CompositorCommandError,
    ) {
        Self::show_compositor_error_message(cx, &error.to_string());
    }

    fn show_compositor_error_message(cx: &mut App, concise: &str) {
        if cx.has_global::<ShellRuntime>()
            && let Some(hub) = cx.global::<ShellRuntime>().service_hub()
        {
            hub.push_notification(Notification::new("Compositor command failed", concise));
        }
    }

    pub(crate) fn dispatch_invocation(
        cx: &mut App,
        invocation: ActionInvocation,
    ) -> Result<crate::actions::ActionResult, ShellError> {
        let action_id = invocation.id();
        let (enabled, name) = {
            let dispatcher = cx.global::<ShellRuntime>().action_dispatcher();
            let descriptor = dispatcher
                .actions
                .descriptor(&action_id)
                .cloned()
                .ok_or_else(|| ShellError::ActionFailed("unknown action id".into()))?;
            if !invocation.matches_descriptor(&descriptor) {
                return Err(ShellError::ActionFailed("invocation mismatch".into()));
            }
            (descriptor.enabled, descriptor.name)
        };

        if !enabled {
            return Err(ShellError::ActionFailed(format!(
                "action '{}' is currently disabled",
                name
            )));
        }

        match invocation {
            ActionInvocation::ToggleBar => {
                ShellSurfaces::request(cx, SurfaceRequest::ToggleBars);
                Ok(crate::actions::ActionResult::Immediate)
            }
            ActionInvocation::ToggleOverview => {
                ShellSurfaces::request(cx, SurfaceRequest::ToggleOverview);
                Ok(crate::actions::ActionResult::Immediate)
            }
            ActionInvocation::ReloadConfig => {
                ShellRuntime::enqueue_worker(cx, shilpo_services::IpcRequest::ReloadConfig)?;
                Ok(crate::actions::ActionResult::Immediate)
            }
            ActionInvocation::Quit => {
                ShellRuntime::shutdown(cx);
                Ok(crate::actions::ActionResult::Immediate)
            }
            ActionInvocation::FocusWorkspace(id) => {
                let comp = ShellRuntime::compositor(cx)
                    .ok_or_else(|| ShellError::ActionFailed("compositor unavailable".into()))?;
                let ticket = comp
                    .command_broker()
                    .submit(CompositorCommand::FocusWorkspace(id))
                    .map_err(|error| ShellError::ActionFailed(error.to_string()))?;
                Ok(crate::actions::ActionResult::Compositor(ticket))
            }
            ActionInvocation::FocusWindow(id) => {
                let comp = ShellRuntime::compositor(cx)
                    .ok_or_else(|| ShellError::ActionFailed("compositor unavailable".into()))?;
                let ticket = comp
                    .command_broker()
                    .submit(CompositorCommand::FocusWindow(id))
                    .map_err(|error| ShellError::ActionFailed(error.to_string()))?;
                Ok(crate::actions::ActionResult::Compositor(ticket))
            }
            ActionInvocation::CloseWindow(id) => {
                let comp = ShellRuntime::compositor(cx)
                    .ok_or_else(|| ShellError::ActionFailed("compositor unavailable".into()))?;
                let ticket = comp
                    .command_broker()
                    .submit(CompositorCommand::CloseWindow(id))
                    .map_err(|error| ShellError::ActionFailed(error.to_string()))?;
                Ok(crate::actions::ActionResult::Compositor(ticket))
            }
            ActionInvocation::CreateWorkspace => {
                let comp = ShellRuntime::compositor(cx)
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
                let comp = ShellRuntime::compositor(cx)
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
                ShellRuntime::dispatch_device_command(
                    cx,
                    shilpo_services::DeviceCommand::Audio(shilpo_services::AudioAction::SetVolume(
                        (ShellRuntime::device_snapshot(cx).audio.volume + 5).min(100),
                    )),
                );
                let info = ShellRuntime::device_snapshot(cx).audio;
                let target_vol = (info.volume + 5).min(100);
                ShellSurfaces::request(
                    cx,
                    SurfaceRequest::ShowOsd(crate::osd::OsdKind::Volume {
                        level: target_vol as u32,
                        muted: info.is_muted,
                    }),
                );
                Ok(crate::actions::ActionResult::Immediate)
            }
            ActionInvocation::VolumeDown => {
                ShellRuntime::dispatch_device_command(
                    cx,
                    shilpo_services::DeviceCommand::Audio(shilpo_services::AudioAction::SetVolume(
                        ShellRuntime::device_snapshot(cx)
                            .audio
                            .volume
                            .saturating_sub(5),
                    )),
                );
                let info = ShellRuntime::device_snapshot(cx).audio;
                let target_vol = info.volume.saturating_sub(5);
                ShellSurfaces::request(
                    cx,
                    SurfaceRequest::ShowOsd(crate::osd::OsdKind::Volume {
                        level: target_vol as u32,
                        muted: info.is_muted,
                    }),
                );
                Ok(crate::actions::ActionResult::Immediate)
            }
            ActionInvocation::VolumeMute => {
                ShellRuntime::dispatch_device_command(
                    cx,
                    shilpo_services::DeviceCommand::Audio(shilpo_services::AudioAction::ToggleMute),
                );
                let info = ShellRuntime::device_snapshot(cx).audio;
                ShellSurfaces::request(
                    cx,
                    SurfaceRequest::ShowOsd(crate::osd::OsdKind::Volume {
                        level: info.volume as u32,
                        muted: !info.is_muted,
                    }),
                );
                Ok(crate::actions::ActionResult::Immediate)
            }
            ActionInvocation::BrightnessUp => {
                let info = ShellRuntime::device_snapshot(cx).brightness;
                let connector = ShellRuntime::compositor(cx)
                    .and_then(|c| c.current().focused_output.clone())
                    .unwrap_or_else(|| "eDP-1".to_string());
                ShellRuntime::dispatch_device_command(
                    cx,
                    shilpo_services::DeviceCommand::Brightness(
                        shilpo_services::BrightnessAction::StepUp,
                    ),
                );
                let target_display = info
                    .displays
                    .iter()
                    .find(|d| d.connector.as_deref() == Some(&connector))
                    .or_else(|| info.displays.iter().find(|d| d.is_primary))
                    .or_else(|| info.displays.first());

                let (target_pct, display_name, connector_opt) = match target_display {
                    Some(d) => (
                        (d.percentage as i16 + 5).clamp(0, 100) as u32,
                        Some(d.name.clone()),
                        d.connector.clone(),
                    ),
                    None => ((info.percentage + 5).min(100) as u32, None, Some(connector)),
                };

                ShellSurfaces::request(
                    cx,
                    SurfaceRequest::ShowOsd(crate::osd::OsdKind::Brightness {
                        level: target_pct,
                        display_name,
                        connector: connector_opt,
                    }),
                );
                Ok(crate::actions::ActionResult::Immediate)
            }
            ActionInvocation::BrightnessDown => {
                let info = ShellRuntime::device_snapshot(cx).brightness;
                let connector = ShellRuntime::compositor(cx)
                    .and_then(|c| c.current().focused_output.clone())
                    .unwrap_or_else(|| "eDP-1".to_string());
                ShellRuntime::dispatch_device_command(
                    cx,
                    shilpo_services::DeviceCommand::Brightness(
                        shilpo_services::BrightnessAction::StepDown,
                    ),
                );
                let target_display = info
                    .displays
                    .iter()
                    .find(|d| d.connector.as_deref() == Some(&connector))
                    .or_else(|| info.displays.iter().find(|d| d.is_primary))
                    .or_else(|| info.displays.first());

                let (target_pct, display_name, connector_opt) = match target_display {
                    Some(d) => (
                        d.percentage.saturating_sub(5) as u32,
                        Some(d.name.clone()),
                        d.connector.clone(),
                    ),
                    None => (
                        info.percentage.saturating_sub(5) as u32,
                        None,
                        Some(connector),
                    ),
                };

                ShellSurfaces::request(
                    cx,
                    SurfaceRequest::ShowOsd(crate::osd::OsdKind::Brightness {
                        level: target_pct,
                        display_name,
                        connector: connector_opt,
                    }),
                );
                Ok(crate::actions::ActionResult::Immediate)
            }
            ActionInvocation::TakeScreenshot => {
                ShellSurfaces::request(
                    cx,
                    SurfaceRequest::OpenCapture(shilpo_services::capture::CaptureIntent::Clipboard),
                );
                Ok(crate::actions::ActionResult::Immediate)
            }

            ActionInvocation::Extension { id, payload } => {
                if !cx.global::<ShellRuntime>().extension_host().is_loaded() {
                    return Err(ShellError::ActionFailed(format!(
                        "extension action 'ext:{id}' has no loaded runtime"
                    )));
                }
                ShellRuntime::dispatch_extension_input(cx, &id, None, "invoke", payload);
                Ok(crate::actions::ActionResult::Immediate)
            }
        }
    }
}

impl ShellRuntime {
    pub fn update_shortcut(cx: &mut App, spec: &str, action: ActionId) -> Result<(), String> {
        cx.global_mut::<Self>()
            .action_dispatcher_mut()
            .update_shortcut(spec, action)
    }

    pub fn update_shortcut_with_override(
        cx: &mut App,
        spec: &str,
        action: ActionId,
    ) -> Result<Option<ActionId>, String> {
        cx.global_mut::<Self>()
            .action_dispatcher_mut()
            .update_shortcut_with_override(spec, action)
    }

    pub fn reset_shortcuts_to_defaults(cx: &mut App) {
        if cx.has_global::<Self>() {
            cx.global_mut::<Self>()
                .action_dispatcher_mut()
                .reset_shortcuts_to_defaults();
        }
    }

    pub fn register_extension_action(
        cx: &mut App,
        id: CanonicalId,
        name: impl Into<String>,
        label: impl Into<String>,
    ) -> Result<ActionId, String> {
        cx.global_mut::<Self>()
            .action_dispatcher_mut()
            .register_extension_action(id, name, label)
    }

    pub fn action_descriptors(cx: &App) -> Vec<ActionDescriptor> {
        if cx.has_global::<Self>() {
            cx.global::<Self>().action_dispatcher().action_descriptors()
        } else {
            ActionRegistry::default().all()
        }
    }

    pub fn keybinding_descriptors(cx: &App) -> Vec<(String, String)> {
        cx.global::<Self>()
            .action_dispatcher()
            .keybinding_descriptors()
    }

    pub fn focus_workspace(cx: &mut App, ws_id: u64) -> Result<(), ShellError> {
        ActionDispatcher::dispatch_action(cx, ActionInvocation::FocusWorkspace(ws_id))
    }

    pub fn dispatch_action(cx: &mut App, action: ActionInvocation) -> Result<(), ShellError> {
        ActionDispatcher::dispatch_action(cx, action)
    }

    pub fn dispatch_invocation(
        cx: &mut App,
        invocation: ActionInvocation,
    ) -> Result<crate::actions::ActionResult, ShellError> {
        ActionDispatcher::dispatch_invocation(cx, invocation)
    }

    pub(super) fn show_compositor_error_toast(
        cx: &mut App,
        error: &shilpo_services::CompositorCommandError,
    ) {
        ActionDispatcher::show_compositor_error_toast(cx, error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_dispatcher_initialization_and_shortcuts() {
        let mut dispatcher = ActionDispatcher::new();
        assert!(!dispatcher.action_descriptors().is_empty());

        let res = dispatcher.update_shortcut("Ctrl+Shift+T", ActionId::ToggleBar);
        assert!(res.is_ok());

        dispatcher.reset_shortcuts_to_defaults();
        assert!(!dispatcher.keybinding_descriptors().is_empty());
    }

    #[test]
    fn test_action_dispatcher_extension_action_registration() {
        use shilpo_ext_api::{ContributionId, ExtensionId};
        let mut dispatcher = ActionDispatcher::new();
        let ext_id = ExtensionId::new("org.shilpo.test").unwrap();
        let contrib_id = ContributionId::new("test-action").unwrap();
        let cid = CanonicalId::new(ext_id, contrib_id);
        let res = dispatcher.register_extension_action(cid, "test-action", "Test Action Label");
        assert!(res.is_ok());
        let action_id = res.unwrap();
        assert!(
            dispatcher
                .action_descriptors()
                .into_iter()
                .any(|d| d.id == action_id)
        );
    }

    #[test]
    fn shortcut_override_reports_the_displaced_action() {
        let mut dispatcher = ActionDispatcher::new();
        dispatcher
            .update_shortcut("Ctrl+Shift+T", ActionId::ToggleBar)
            .unwrap();
        let displaced = dispatcher
            .update_shortcut_with_override("Ctrl+Shift+T", ActionId::ToggleOverview)
            .unwrap();
        assert_eq!(displaced, Some(ActionId::ToggleBar));
    }

    #[test]
    fn snapshot_enables_only_actions_the_compositor_supports() {
        let mut dispatcher = ActionDispatcher::new();
        let snapshot = CompositorSnapshot {
            connection: shilpo_services::CompositorConnection::Ready,
            capabilities: shilpo_services::CompositorCapabilities {
                can_focus_workspace: false,
                ..Default::default()
            },
            ..Default::default()
        };

        dispatcher.update_enabled_for_snapshot(&snapshot);

        let descriptors = dispatcher.action_descriptors();
        let focus_ws = descriptors
            .iter()
            .find(|d| d.id == ActionId::FocusWorkspace)
            .unwrap();
        assert!(!focus_ws.enabled);
        let create_ws = descriptors
            .iter()
            .find(|d| d.id == ActionId::CreateWorkspace)
            .unwrap();
        assert!(create_ws.enabled);
    }

    #[test]
    fn extension_actions_are_replaced_during_sync() {
        use shilpo_ext_api::{ContributionId, ExtensionId};
        let mut dispatcher = ActionDispatcher::new();
        let ext_id = ExtensionId::new("org.shilpo.test").unwrap();
        let cid = CanonicalId::new(ext_id.clone(), ContributionId::new("first").unwrap());
        dispatcher
            .register_extension_action(cid, "first", "First")
            .unwrap();

        let next = CanonicalId::new(ext_id, ContributionId::new("second").unwrap());
        dispatcher.sync_extension_actions(vec![ContributionDescriptor {
            id: next.clone(),
            extension_name: "org.shilpo.test".into(),
            name: "second".into(),
            surface: crate::extensions::ContributionSurface::Action,
            settings_schema: None,
            default_size: None,
            minimum_size: None,
        }]);

        let ids = dispatcher
            .action_descriptors()
            .into_iter()
            .filter_map(|d| d.id.extension_id())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec![next]);
    }

    struct ActionDispatcherTestHarness {
        dispatcher: ActionDispatcher,
    }

    impl ActionDispatcherTestHarness {
        fn new_offline() -> Self {
            Self {
                dispatcher: ActionDispatcher::new(),
            }
        }
    }

    #[test]
    fn test_harness_action_dispatcher_isolated_state_transitions() {
        let mut harness = ActionDispatcherTestHarness::new_offline();
        assert!(!harness.dispatcher.action_descriptors().is_empty());
        assert!(
            harness
                .dispatcher
                .update_shortcut("Ctrl+Shift+U", ActionId::ToggleBar)
                .is_ok()
        );
        harness.dispatcher.reset_shortcuts_to_defaults();
        assert!(!harness.dispatcher.keybinding_descriptors().is_empty());
    }
}
