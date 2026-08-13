use shilpo_ext_api::{CanonicalId, ExtensionEvent, ExtensionId, HostOperation, ViewTree};
use shilpo_ext_runtime::{AuthorizedHostOperation, AuthorizedHostOperationKind};
use std::{collections::HashMap, path::PathBuf};

use crate::{
    actions::{ActionId, ActionInvocation},
    extensions::{
        ContributionDescriptor, ContributionInstance, ContributionSurface, ExtensionCommand,
        ExtensionGeneration,
    },
};
use gpui::App;

use super::{ShellRuntime, shell_surfaces::ShellSurfaces};
use crate::shell::keybindings::GlobalShortcutBackend;

/// Owns the wasm extension runtime (coordinator), its in-flight task registry,
/// and the location service used by extension effects.
///
/// Coordinator and task state are private; the shell interacts with extensions
/// exclusively through the method surface below.
pub struct ExtensionHost {
    extensions: Option<crate::extensions::ExtensionCoordinator>,
    extension_tasks: HashMap<(ExtensionGeneration, ExtensionId, String), gpui::Task<()>>,
    extension_location_service: shilpo_services::LocationService,
    #[cfg(test)]
    test_inputs: std::sync::Arc<std::sync::Mutex<Vec<ExtensionCommand>>>,
}

impl ExtensionHost {
    pub fn new(extensions: Option<crate::extensions::ExtensionCoordinator>) -> Self {
        Self {
            extensions,
            extension_tasks: HashMap::new(),
            extension_location_service: shilpo_services::LocationService::new(),
            #[cfg(test)]
            test_inputs: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    #[cfg(test)]
    pub(crate) fn test_inputs(&self) -> std::sync::Arc<std::sync::Mutex<Vec<ExtensionCommand>>> {
        self.test_inputs.clone()
    }

    pub(crate) fn is_loaded(&self) -> bool {
        self.extensions.is_some()
    }

    pub(crate) fn generation(&self) -> Option<ExtensionGeneration> {
        self.extensions.as_ref().map(|ext| ext.generation())
    }

    pub(crate) fn host_generation(&self) -> crate::extensions::HostGeneration {
        self.extensions
            .as_ref()
            .map(|ext| ext.host_generation())
            .unwrap_or_default()
    }

    pub(crate) fn diagnostics(&self) -> Option<crate::extensions::ExtensionHostDiagnostics> {
        self.extensions.as_ref().map(|ext| ext.host_diagnostics())
    }

    pub(crate) fn descriptors_for(
        &self,
        surface: ContributionSurface,
    ) -> Vec<ContributionDescriptor> {
        self.extensions
            .as_ref()
            .map_or_else(Vec::new, |extensions| extensions.descriptors_for(surface))
    }

    pub(crate) fn view(&self, id: &CanonicalId) -> Option<ViewTree> {
        self.extensions
            .as_ref()
            .and_then(|extensions| extensions.view(id))
    }

    pub(crate) fn asset_path(&self, id: &CanonicalId, relative: &str) -> Result<PathBuf, String> {
        self.extensions
            .as_ref()
            .ok_or_else(|| "extension runtime is unavailable".to_owned())?
            .asset_path(id, relative)
    }

    pub(crate) fn send_input(
        &self,
        contribution: &CanonicalId,
        instance_id: Option<&str>,
        event_id: impl Into<String>,
        value: Option<serde_json::Value>,
    ) {
        let event_id = event_id.into();
        #[cfg(test)]
        self.test_inputs
            .lock()
            .expect("test input recorder is not poisoned")
            .push(ExtensionCommand::Input {
                expected: self.generation().unwrap_or_default(),
                contribution: contribution.clone(),
                instance_id: instance_id.map(ToString::to_string),
                event_id: event_id.clone(),
                value: value.clone(),
            });

        if let Some(ext) = &self.extensions
            && let Err(error) = ext.send_command(ExtensionCommand::Input {
                expected: ext.generation(),
                contribution: contribution.clone(),
                instance_id: instance_id.map(ToString::to_string),
                event_id,
                value,
            })
        {
            tracing::warn!(%error, "extension input was not queued");
        }
    }

    pub(crate) fn send_event(&self, event: ExtensionEvent) {
        if let Some(ext) = &self.extensions {
            let cmd = match event {
                ExtensionEvent::PowerChanged {
                    percentage,
                    charging,
                } => ExtensionCommand::Replaceable(crate::extensions::ReplaceableEvent::Power {
                    percentage,
                    charging,
                }),
                ExtensionEvent::NetworkChanged { connected } => {
                    ExtensionCommand::Replaceable(crate::extensions::ReplaceableEvent::Network {
                        connected,
                    })
                }
                ExtensionEvent::MediaChanged {
                    title,
                    artist,
                    playing,
                } => ExtensionCommand::Replaceable(crate::extensions::ReplaceableEvent::Media {
                    title,
                    artist,
                    playing,
                }),
                _ => ExtensionCommand::Lifecycle {
                    expected: ext.generation(),
                    event,
                },
            };
            if let Err(error) = ext.send_command(cmd) {
                tracing::warn!(%error, "extension event was not queued");
            }
        }
    }

    pub(crate) fn send_event_to_extension(
        &self,
        extension_id: &ExtensionId,
        event: ExtensionEvent,
    ) {
        #[cfg(test)]
        self.test_inputs
            .lock()
            .expect("test extension input lock is not poisoned")
            .push(ExtensionCommand::Response {
                expected: self
                    .extensions
                    .as_ref()
                    .map_or(ExtensionGeneration(0), |ext| ext.generation()),
                extension_id: extension_id.clone(),
                event: event.clone(),
            });
        let Some(ext) = &self.extensions else {
            return;
        };
        if let Err(error) = ext.send_command(ExtensionCommand::Response {
            expected: ext.generation(),
            extension_id: extension_id.clone(),
            event,
        }) {
            tracing::warn!(%error, "targeted extension event was not queued");
        }
    }

    pub(crate) fn send_lifecycle_for(
        &self,
        surface: ContributionSurface,
        mounted: bool,
        width: f32,
        height: f32,
    ) {
        if let Some(ext) = &self.extensions {
            let expected_gen = ext.generation();
            for descriptor in self.descriptors_for(surface) {
                let event = if mounted {
                    ExtensionEvent::ContributionMounted {
                        contribution_id: descriptor.id.contribution_id.to_string(),
                        instance_id: None,
                        width,
                        height,
                    }
                } else {
                    ExtensionEvent::ContributionUnmounted {
                        contribution_id: descriptor.id.contribution_id.to_string(),
                        instance_id: None,
                    }
                };
                if let Err(error) = ext.send_command(ExtensionCommand::Lifecycle {
                    expected: expected_gen,
                    event,
                }) {
                    tracing::warn!(%error, "extension lifecycle event was not queued");
                }
            }
        }
    }

    pub(crate) fn send_instance_reconciliation(&mut self, desired: Vec<ContributionInstance>) {
        if let Some(ext) = &self.extensions
            && let Err(error) = ext.send_command(ExtensionCommand::ReconcileInstances {
                expected: ext.generation(),
                desired,
            })
        {
            tracing::warn!(%error, "extension instance reconciliation was not queued");
        }
    }

    pub fn mount_contribution(&self, contribution: &CanonicalId, width: f32, height: f32) {
        if let Some(ext) = &self.extensions
            && let Err(error) = ext.send_command(ExtensionCommand::Lifecycle {
                expected: ext.generation(),
                event: ExtensionEvent::ContributionMounted {
                    contribution_id: contribution.contribution_id.to_string(),
                    instance_id: None,
                    width,
                    height,
                },
            })
        {
            tracing::warn!(%error, "extension mount was not queued");
        }
    }

    pub fn unmount_contribution(&self, contribution: &CanonicalId) {
        if let Some(ext) = &self.extensions
            && let Err(error) = ext.send_command(ExtensionCommand::Lifecycle {
                expected: ext.generation(),
                event: ExtensionEvent::ContributionUnmounted {
                    contribution_id: contribution.contribution_id.to_string(),
                    instance_id: None,
                },
            })
        {
            tracing::warn!(%error, "extension unmount was not queued");
        }
    }

    pub(crate) fn send_response(
        &self,
        expected: ExtensionGeneration,
        extension_id: ExtensionId,
        event: ExtensionEvent,
    ) {
        if let Some(ext) = &self.extensions
            && let Err(error) = ext.send_command(ExtensionCommand::Response {
                expected,
                extension_id,
                event,
            })
        {
            tracing::warn!(%error, "extension response was not queued");
        }
    }

    pub(crate) fn drain_updates(&mut self) -> Vec<crate::extensions::ExtensionUpdate> {
        self.extensions
            .as_ref()
            .map(|ext| ext.drain_updates())
            .unwrap_or_default()
    }

    pub(crate) fn shutdown_task(
        &self,
        executor: gpui::BackgroundExecutor,
        timeout: std::time::Duration,
    ) -> Option<gpui::Task<bool>> {
        self.extensions
            .as_ref()
            .map(|ext| ext.shutdown(executor, timeout))
    }

    fn task_count(&self) -> usize {
        self.extension_tasks.len()
    }

    fn has_task(&self, key: &(ExtensionGeneration, ExtensionId, String)) -> bool {
        self.extension_tasks.contains_key(key)
    }

    fn insert_task(
        &mut self,
        key: (ExtensionGeneration, ExtensionId, String),
        task: gpui::Task<()>,
    ) {
        self.extension_tasks.insert(key, task);
    }

    fn remove_task(&mut self, key: &(ExtensionGeneration, ExtensionId, String)) {
        self.extension_tasks.remove(key);
    }

    fn retain_tasks_for_generation(&mut self, active_gen: ExtensionGeneration) {
        self.extension_tasks
            .retain(|(task_gen, _, _), _| *task_gen >= active_gen);
    }

    fn location_service(&self) -> shilpo_services::LocationService {
        self.extension_location_service.clone()
    }

    /// Drains and applies the pending extension updates.
    pub(crate) fn drain(cx: &mut App) {
        if !cx.has_global::<ShellRuntime>() {
            return;
        }
        let updates = cx
            .global_mut::<ShellRuntime>()
            .extension_host_mut()
            .drain_updates();
        for update in updates {
            Self::apply_update(cx, update);
        }
    }

    pub(crate) fn apply_update(cx: &mut App, update: crate::extensions::ExtensionUpdate) {
        let current_gen = cx.global::<ShellRuntime>().extension_host().generation();
        let current_host_gen = cx
            .global::<ShellRuntime>()
            .extension_host()
            .host_generation();
        if update.host_generation != current_host_gen {
            return;
        }
        if current_gen.is_some_and(|target_gen| update.generation < target_gen) {
            return;
        }

        if update.snapshot.is_some() {
            Self::sync_extension_actions(cx);
            ShellSurfaces::reconcile_bar_extension_instances(cx);
        }

        for (extension_id, effect) in update.effects {
            Self::execute_effect(cx, &extension_id, update.generation, effect);
        }

        if let Some(snapshot) = &update.snapshot {
            let active_gen = snapshot.generation;
            cx.global_mut::<ShellRuntime>()
                .extension_host_mut()
                .retain_tasks_for_generation(active_gen);
        }

        if update.snapshot.is_some() || !update.invalidated_views.is_empty() {
            cx.refresh_windows();
        }
    }

    /// Reconciles the action registry and keybindings with the contributions
    /// from loaded extensions.
    pub(crate) fn sync_extension_actions(cx: &mut App) {
        let desired_actions = cx
            .global::<ShellRuntime>()
            .extension_host()
            .descriptors_for(ContributionSurface::Action);
        cx.global_mut::<ShellRuntime>()
            .action_dispatcher_mut()
            .sync_extension_actions(desired_actions);

        let extension_shortcuts = cx
            .global::<ShellRuntime>()
            .extension_host()
            .descriptors_for(ContributionSurface::Shortcut);

        let user_bindings = ShellRuntime::active_config(cx).keybindings;

        let report = cx
            .global_mut::<ShellRuntime>()
            .action_dispatcher_mut()
            .reconcile_keybindings(&user_bindings, &extension_shortcuts);

        let backend = crate::shell::keybindings::NiriShortcutBackend::new();
        let compositor = std::env::var("SHILPO_COMPOSITOR").ok();
        match backend.sync_for_compositor(compositor.as_deref(), &report.resolved) {
            Ok(projection) => {
                for diagnostic in projection.diagnostics {
                    tracing::warn!(%diagnostic, "shortcut projection diagnostic");
                }
                tracing::info!(include = %projection.include_directive, "Niri shortcut include required");
            }
            Err(error) => tracing::error!(
                ?error,
                "shortcut projection failed; last-good projection retained"
            ),
        }
    }

    pub(crate) fn execute_effect(
        cx: &mut App,
        extension_id: &ExtensionId,
        generation: ExtensionGeneration,
        effect: AuthorizedHostOperation,
    ) {
        match effect.into_kind() {
            AuthorizedHostOperationKind::NonHttp(HostOperation::InvokeAction {
                action_id,
                payload_json,
            }) => {
                let invocation = action_id
                    .parse::<ActionId>()
                    .map_err(|err| err.to_string())
                    .and_then(|id| {
                        ActionInvocation::from_id_and_payload(
                            id,
                            payload_json.and_then(|s| serde_json::from_str(&s).ok()),
                        )
                    });
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
            AuthorizedHostOperationKind::NonHttp(HostOperation::ShowNotification {
                title,
                body,
                icon,
            }) => {
                let mut notification = shilpo_services::Notification::new(title, body);
                notification.app_name = extension_id.to_string();
                notification.app_icon = icon;
                super::ShellSurfaces::request(
                    cx,
                    super::SurfaceRequest::ShowNotification(notification),
                );
            }
            AuthorizedHostOperationKind::NonHttp(HostOperation::SetThemeSource { color }) => {
                let argb = crate::bar::view::parse_hex_color(&color).unwrap_or(0xFF006C4C);
                shilpo_theme_daemon::ThemeClient::spawn_task(async move {
                    let client = shilpo_theme_daemon::ThemeClient::new().await;
                    let _ = client.set_custom_seed(argb).await;
                    let _ = client
                        .set_color_source(shilpo_ui::theme::ColorSource::Custom)
                        .await;
                });
            }
            AuthorizedHostOperationKind::NonHttp(HostOperation::SetWallpaper { path, .. }) => {
                shilpo_theme_daemon::ThemeClient::spawn_task(async move {
                    let client = shilpo_theme_daemon::ThemeClient::new().await;
                    let _ = client.set_wallpaper(&path).await;
                });
            }
            AuthorizedHostOperationKind::NonHttp(HostOperation::ClipboardWrite { text }) => {
                let result = cx
                    .global::<ShellRuntime>()
                    .service_hub
                    .as_ref()
                    .map(|hub| hub.copy_text(&text));
                if let Some(Err(error)) = result {
                    tracing::warn!(
                        extension = %extension_id,
                        error = %error,
                        "extension clipboard effect failed"
                    );
                }
            }
            AuthorizedHostOperationKind::HttpRequest(request) => {
                let request_id = request.request_id().to_string();
                let key = (generation, extension_id.clone(), request_id.clone());
                let accepted = {
                    let host = cx.global_mut::<ShellRuntime>().extension_host_mut();
                    request_id.len() <= 128
                        && !request_id.is_empty()
                        && host.task_count() < 8
                        && !host.has_task(&key)
                };
                if !accepted {
                    let ext_id = extension_id.clone();
                    cx.global_mut::<ShellRuntime>()
                        .extension_host_mut()
                        .send_response(
                        generation,
                        ext_id,
                        ExtensionEvent::HttpResponse {
                            request_id,
                            status: None,
                            body: String::new(),
                            error: Some(
                                "request ID is invalid, duplicated, or the HTTP limit was reached"
                                    .into(),
                            ),
                        },
                    );
                    return;
                }
                let ext_id = extension_id.clone();
                let task_key = key.clone();
                let expected = generation;
                let task = cx.spawn(async move |cx| {
                    let response = crate::extension_http::fetch(request).await;
                    cx.update(|cx: &mut gpui::App| {
                        if cx.has_global::<ShellRuntime>() {
                            let host = cx.global_mut::<ShellRuntime>().extension_host_mut();
                            host.remove_task(&task_key);
                            host.send_response(expected, ext_id, response);
                        }
                    });
                });
                cx.global_mut::<ShellRuntime>()
                    .extension_host_mut()
                    .insert_task(key, task);
            }
            AuthorizedHostOperationKind::NonHttp(HostOperation::LocationRead) => {
                let location_service = cx
                    .global::<ShellRuntime>()
                    .extension_host()
                    .location_service();
                let ext_id = extension_id.clone();
                let task_id = uuid::Uuid::new_v4().to_string();
                let key = (generation, extension_id.clone(), task_id);
                let task_key = key.clone();
                let expected = generation;
                let task = cx.spawn(async move |cx| {
                    let result = location_service.read_location_async().await;
                    let event = match result {
                        Ok(info) => ExtensionEvent::LocationResponse {
                            latitude: Some(info.latitude),
                            longitude: Some(info.longitude),
                            accuracy_meters: Some(info.accuracy_meters),
                            error: None,
                        },
                        Err(error) => ExtensionEvent::LocationResponse {
                            latitude: None,
                            longitude: None,
                            accuracy_meters: None,
                            error: Some(error),
                        },
                    };
                    cx.update(|cx: &mut gpui::App| {
                        if cx.has_global::<ShellRuntime>() {
                            let host = cx.global_mut::<ShellRuntime>().extension_host_mut();
                            host.remove_task(&task_key);
                            host.send_response(expected, ext_id, event);
                        }
                    });
                });
                cx.global_mut::<ShellRuntime>()
                    .extension_host_mut()
                    .insert_task(key, task);
            }
            AuthorizedHostOperationKind::NonHttp(op) => {
                tracing::debug!(
                    extension = %extension_id,
                    operation = ?op,
                    "unhandled host operation"
                );
            }
        }
    }
}

impl ShellRuntime {
    pub fn extension_descriptors(
        cx: &App,
        surface: ContributionSurface,
    ) -> Vec<ContributionDescriptor> {
        cx.global::<Self>()
            .extension_host()
            .descriptors_for(surface)
    }

    pub fn extension_surface_views(
        cx: &App,
        surface: ContributionSurface,
    ) -> Vec<(CanonicalId, ViewTree)> {
        let descriptors = Self::extension_descriptors(cx, surface);
        descriptors
            .into_iter()
            .filter_map(|descriptor| {
                let tree = Self::extension_view(cx, &descriptor.id)?;
                Some((descriptor.id, tree))
            })
            .collect()
    }

    pub fn descriptors_for(cx: &App, surface: ContributionSurface) -> Vec<ContributionDescriptor> {
        if cx.has_global::<Self>() {
            cx.global::<Self>()
                .extension_host()
                .descriptors_for(surface)
        } else {
            Vec::new()
        }
    }

    pub fn extension_view(cx: &App, id: &CanonicalId) -> Option<ViewTree> {
        cx.global::<Self>().extension_host().view(id)
    }

    pub fn extension_asset_path(
        cx: &App,
        id: &CanonicalId,
        relative: &str,
    ) -> Result<PathBuf, String> {
        cx.global::<Self>()
            .extension_host()
            .asset_path(id, relative)
    }

    pub fn dispatch_extension_input(
        cx: &mut App,
        contribution: &CanonicalId,
        instance_id: Option<&str>,
        event_id: impl Into<String>,
        value: Option<serde_json::Value>,
    ) {
        cx.global_mut::<Self>().extension_host_mut().send_input(
            contribution,
            instance_id,
            event_id,
            value,
        );
    }

    pub fn dispatch_extension_event(cx: &mut App, event: ExtensionEvent) {
        cx.global_mut::<Self>()
            .extension_host_mut()
            .send_event(event);
    }

    pub fn dispatch_extension_menu_event(
        cx: &mut App,
        extension_id: &ExtensionId,
        event: ExtensionEvent,
    ) {
        cx.global_mut::<Self>()
            .extension_host_mut()
            .send_event_to_extension(extension_id, event);
    }

    pub(crate) fn dispatch_surface_lifecycle(
        cx: &mut App,
        surface: ContributionSurface,
        mounted: bool,
        width: f32,
        height: f32,
    ) {
        cx.global_mut::<Self>()
            .extension_host_mut()
            .send_lifecycle_for(surface, mounted, width, height);
    }

    pub(super) fn drain_extensions(cx: &mut App) {
        ExtensionHost::drain(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension_host_initialization() {
        let host = ExtensionHost::new(None);
        assert!(!host.is_loaded());
        assert!(host.generation().is_none());
        assert!(host.descriptors_for(ContributionSurface::Action).is_empty());
        assert_eq!(host.task_count(), 0);
    }

    #[test]
    fn extension_task_registry_tracks_generations() {
        let mut host = ExtensionHost::new(None);
        let key = (
            ExtensionGeneration(7),
            ExtensionId::new("org.shilpo.test").unwrap(),
            "req-1".into(),
        );
        assert!(!host.has_task(&key));

        host.insert_task(key.clone(), gpui::Task::ready(()));
        assert!(host.has_task(&key));
        assert_eq!(host.task_count(), 1);

        host.remove_task(&key);
        assert_eq!(host.task_count(), 0);
    }

    #[test]
    fn extension_settings_merge_global_values_with_instance_overrides() {
        let mut config = crate::config::ShellConfig::default();
        config.extensions.settings.insert(
            "org.shilpo.weather".into(),
            serde_json::json!({
                "location": "Kolkata",
                "show_condition": false
            }),
        );
        let id = ExtensionId::new("org.shilpo.weather").unwrap();
        assert_eq!(
            crate::runtime::shell_surfaces::extension_settings(
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

    struct ExtensionHostTestHarness {
        host: ExtensionHost,
    }

    impl ExtensionHostTestHarness {
        fn new_offline() -> Self {
            Self {
                host: ExtensionHost::new(None),
            }
        }
    }

    #[test]
    fn test_harness_extension_host_isolated_state_transitions() {
        let harness = ExtensionHostTestHarness::new_offline();
        assert!(!harness.host.is_loaded());
        assert!(harness.host.generation().is_none());
        assert!(
            harness
                .host
                .descriptors_for(ContributionSurface::Action)
                .is_empty()
        );
    }
}
