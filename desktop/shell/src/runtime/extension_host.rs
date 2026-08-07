use std::{collections::HashMap, path::PathBuf};
use shilpo_ext_types::{CanonicalId, ExtensionId};

use crate::{
    actions::{ActionId, ActionInvocation},
    extensions::{ContributionDescriptor, ContributionSurface, ExtensionGeneration},
};
use gpui::{
    App, AppContext, Bounds, DisplayId, Pixels, WindowBackgroundAppearance, WindowBounds,
    WindowKind, WindowOptions,
    layer_shell::{KeyboardInteractivity, Layer, LayerShellOptions},
    point, px, size,
};

use super::{ShellRuntime, surface_manager::overlay_options};

#[derive(Clone, Debug, PartialEq)]
pub struct ExtensionSurfaceSpec {
    pub contribution: CanonicalId,
    pub display_id: DisplayId,
    pub bounds: Bounds<Pixels>,
}

pub(super) fn extension_settings(
    config: &shilpo_config::ShellConfig,
    extension_id: &ExtensionId,
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

impl ShellRuntime {
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
    ) -> Vec<(CanonicalId, shilpo_ext::ViewTree)> {
        let descriptors = Self::extension_descriptors(cx, surface);
        descriptors
            .into_iter()
            .filter_map(|descriptor| {
                let tree = Self::extension_view(cx, &descriptor.id)?;
                Some((descriptor.id, tree))
            })
            .collect()
    }

    pub fn extension_view(cx: &App, id: &CanonicalId) -> Option<shilpo_ext::ViewTree> {
        cx.global::<Self>()
            .extensions
            .as_ref()
            .and_then(|extensions| extensions.view(id))
    }

    pub fn extension_asset_path(
        cx: &App,
        id: &CanonicalId,
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
        contribution: &CanonicalId,
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

    pub fn open_extension_panel(cx: &mut App, contribution: CanonicalId) {
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

    pub(super) fn drain_extensions(cx: &mut App) {
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

    pub(super) fn apply_extension_update(cx: &mut App, update: crate::extensions::ExtensionUpdate) {
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

    pub(super) fn dispatch_extension_event(cx: &mut App, event: shilpo_ext::ExtensionEvent) {
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

    pub(super) fn sync_extension_actions(cx: &mut App) {
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

    pub(super) fn execute_extension_effect(
        cx: &mut App,
        extension_id: &ExtensionId,
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
                shilpo_theme_daemon::ThemeClient::spawn_task(async move {
                    let client = shilpo_theme_daemon::ThemeClient::new().await;
                    let _ = client.set_custom_seed(argb).await;
                    let _ = client
                        .set_color_source(shilpo_theme::ColorSource::Custom)
                        .await;
                });
            }
            shilpo_ext::AuthorizedHostEffectKind::NonHttp(
                shilpo_ext::HostEffect::SetWallpaper { path, .. },
            ) => {
                shilpo_theme_daemon::ThemeClient::spawn_task(async move {
                    let client = shilpo_theme_daemon::ThemeClient::new().await;
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

    pub(super) fn reconcile_extension_surfaces(
        cx: &mut App,
        outputs: &[crate::bar::OutputDescriptor],
    ) {
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

    pub(super) fn reconcile_bar_extension_instances(cx: &mut App) {
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let id = ExtensionId::new("org.shilpo.weather").unwrap();
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
}
