use std::{collections::HashMap, path::{Path, PathBuf}};

use gpui::{
    App, Bounds, DisplayId, Entity, Pixels, Point, Size, WindowBackgroundAppearance, WindowBounds,
    WindowHandle, WindowKind, WindowOptions,
    layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
    point, px, size,
};
use shilpo_services::{BarState, CompositorOutput};
use uuid::Uuid;

use crate::{
    ControlCenterView,
    bar::{BarView, ReconciliationOp, geometry::BarGeometry},
    error::ShellError,
    extensions::ContributionSurface,
};

use super::ShellRuntime;

pub(super) fn bar_window_options(
    geometry: &BarGeometry,
    with_display_geometry: bool,
) -> WindowOptions {
    let bounds = if with_display_geometry {
        geometry.bounds
    } else {
        Bounds::new(point(px(0.), px(0.)), geometry.bounds.size)
    };
    WindowOptions {
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        display_id: with_display_geometry.then_some(geometry.display_id),
        app_id: Some("shilpo-bar".to_string()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "bar".to_string(),
            layer: Layer::Top,
            anchor: geometry.anchor,
            exclusive_zone: Some(geometry.exclusive_zone),
            exclusive_edge: Some(geometry.exclusive_edge),
            margin: geometry.margin,
            keyboard_interactivity: KeyboardInteractivity::None,
        }),
        ..Default::default()
    }
}

pub(super) fn overlay_options(
    app_id: &str,
    namespace: &str,
    window_size: Size<Pixels>,
    origin: Point<Pixels>,
    display_id: Option<DisplayId>,
) -> WindowOptions {
    WindowOptions {
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin,
            size: window_size,
        })),
        display_id,
        app_id: Some(app_id.to_string()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: namespace.to_string(),
            layer: Layer::Top,
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn parse_awww_wallpaper_path(output: &str) -> Option<PathBuf> {
    const MARKER: &str = "currently displaying: image: ";
    output.lines().find_map(|line| {
        let path = line.split_once(MARKER)?.1.trim();
        (!path.is_empty()).then(|| PathBuf::from(path))
    })
}

pub(crate) fn query_awww_wallpaper_path() -> Option<PathBuf> {
    let output = std::process::Command::new("awww")
        .arg("query")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = parse_awww_wallpaper_path(&String::from_utf8_lossy(&output.stdout))?;
    path.is_file().then_some(path)
}

fn discovered_wallpaper_needs_theme_sync(
    theme_wallpaper_path: Option<&Path>,
    discovered_wallpaper_path: &Path,
) -> bool {
    theme_wallpaper_path != Some(discovered_wallpaper_path)
}

fn should_restore_overview_prior_focus(
    reason: crate::overview::OverviewCloseReason,
    opened_workspace_id: Option<u64>,
    current_workspace_id: Option<u64>,
) -> bool {
    if reason != crate::overview::OverviewCloseReason::Cancel {
        return false;
    }

    match (opened_workspace_id, current_workspace_id) {
        (Some(opened), Some(current)) => opened == current,
        _ => true,
    }
}

impl ShellRuntime {
    pub(super) fn output_name_for_bounds(
        bounds: Bounds<Pixels>,
        compositor_outputs: &[CompositorOutput],
    ) -> Option<String> {
        let origin_x = bounds.origin.x.as_f32();
        let origin_y = bounds.origin.y.as_f32();
        let width = bounds.size.width.as_f32();
        let height = bounds.size.height.as_f32();
        let close = |left: f32, right: f32| (left - right).abs() <= 2.0;
        let geometry_match = |output: &CompositorOutput, size_only: bool| {
            let scale = output.scale.max(1.0) as f32;
            let position_matches = close(output.logical_position.0 as f32, origin_x)
                && close(output.logical_position.1 as f32, origin_y);
            let size_matches = (close(output.logical_size.0 as f32, width)
                && close(output.logical_size.1 as f32, height))
                || (close(output.logical_size.0 as f32 * scale, width)
                    && close(output.logical_size.1 as f32 * scale, height));
            size_matches && (size_only || position_matches)
        };

        compositor_outputs
            .iter()
            .find(|output| geometry_match(output, false))
            .or_else(|| {
                let size_matches = compositor_outputs
                    .iter()
                    .filter(|output| geometry_match(output, true))
                    .collect::<Vec<_>>();
                (size_matches.len() == 1).then(|| size_matches[0])
            })
            .map(|output| output.name.clone())
    }

    pub(super) fn output_name_for_display(
        display: &dyn gpui::PlatformDisplay,
        compositor_outputs: &[CompositorOutput],
    ) -> Option<String> {
        let display_uuid = display.uuid().ok();
        display_uuid
            .and_then(|display_uuid| {
                compositor_outputs
                    .iter()
                    .find(|output| {
                        Uuid::new_v5(&Uuid::NAMESPACE_DNS, output.name.as_bytes()) == display_uuid
                    })
                    .map(|output| output.name.clone())
            })
            .or_else(|| Self::output_name_for_bounds(display.bounds(), compositor_outputs))
            .or_else(|| display_uuid.map(|uuid| uuid.to_string()))
    }

    pub fn sync_displays(cx: &mut App) {
        if !cx.has_global::<Self>() {
            return;
        }

        let snapshot = Self::compositor_snapshot(cx);

        let current_outputs = cx
            .displays()
            .into_iter()
            .enumerate()
            .map(|(index, display)| {
                let display_id = display.id();
                let output_name = Self::output_name_for_display(&*display, &snapshot.outputs);
                let is_primary = index == 0;

                crate::bar::OutputDescriptor {
                    display_id,
                    bounds: display.bounds(),
                    name: output_name,
                    is_primary,
                    scale: None,
                }
            })
            .collect::<Vec<_>>();

        let output_ids = current_outputs
            .iter()
            .map(|o| o.display_id)
            .collect::<std::collections::HashSet<_>>();
        if output_ids != cx.global::<Self>().extension_output_ids {
            cx.global_mut::<Self>().extension_output_ids = output_ids;
            Self::dispatch_extension_event(
                cx,
                shilpo_ext::ExtensionEvent::OutputsChanged,
            );
        }

        Self::reconcile_extension_surfaces(cx, &current_outputs);
    }

    pub fn open_bar_with_spec(cx: &mut App, spec: crate::bar::BarSpec) -> bool {
        let geometry = spec.geometry.clone();
        let display_id = spec.geometry.display_id;
        let with_display_geometry = spec.with_display_geometry;
        let options = bar_window_options(&geometry, with_display_geometry);
        let spec_for_view = spec.clone();
        let view_result = cx.open_window(options, move |window, cx| {
            BarView::view_with_spec(spec_for_view, window, cx)
        });

        match view_result {
            Ok(handle) => {
                let runtime = cx.global_mut::<Self>();
                runtime.bars.insert(display_id, (handle, spec));
                runtime.bar_state = BarState::Visible;
                runtime.publish_status();
                true
            }
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "failed to open shell bar window on display {:?}",
                    display_id
                );
                false
            }
        }
    }

    pub fn open_bar(cx: &mut App, geometry: &BarGeometry, with_display_geometry: bool) -> bool {
        let config = Self::active_config(cx).bar;
        let spec = crate::bar::BarSpec::new(geometry.clone(), config, with_display_geometry);
        Self::open_bar_with_spec(cx, spec)
    }

    pub fn reconcile_bars(cx: &mut App) {
        let snapshot = Self::compositor_snapshot(cx);
        let outputs = cx
            .displays()
            .into_iter()
            .enumerate()
            .map(|(index, display)| {
                let display_id = display.id();
                let output_name = Self::output_name_for_display(&*display, &snapshot.outputs);
                let is_primary = index == 0;

                crate::bar::OutputDescriptor {
                    display_id,
                    bounds: display.bounds(),
                    name: output_name,
                    is_primary,
                    scale: None,
                }
            })
            .collect::<Vec<_>>();

        let ops = {
            let current_bars = cx
                .global::<Self>()
                .bars
                .iter()
                .map(|(id, (_, spec))| (*id, spec.clone()))
                .collect();
            crate::bar::reconciliation::reconcile_output_bars(
                &outputs,
                &Self::active_config(cx),
                &current_bars,
            )
        };

        let mut mounted = false;
        for op in ops {
            match op {
                ReconciliationOp::Create(spec) | ReconciliationOp::Recreate(spec) => {
                    mounted |= Self::open_bar_with_spec(cx, spec);
                }
                ReconciliationOp::Remove(display_id) => {
                    if let Some((handle, _)) = cx.global_mut::<Self>().bars.remove(&display_id) {
                        let _ = handle.update(cx, |_, window, _| window.remove_window());
                    }
                }
                ReconciliationOp::Retain(_) => {
                    mounted = true;
                }
            }
        }

        if !mounted && !cx.global::<Self>().bars.is_empty() {
            mounted = true;
        }

        let runtime = cx.global_mut::<Self>();
        if !mounted {
            runtime.bar_state = BarState::OpenFailed;
            runtime.publish_status();
        }
    }

    pub fn mark_bar_open_failed(cx: &mut App) {
        let runtime = cx.global_mut::<Self>();
        runtime.bar_state = BarState::OpenFailed;
        runtime.publish_status();
    }

    pub fn toggle_bar(cx: &mut App) {
        if cx.global::<Self>().bars.is_empty() {
            Self::reconcile_bars(cx);
        } else {
            let old_bars = std::mem::take(&mut cx.global_mut::<Self>().bars);
            for (_, (handle, _)) in old_bars {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            }
            let runtime = cx.global_mut::<Self>();
            runtime.bar_state = BarState::Hidden;
            runtime.last_bar_specs.clear();
            runtime.publish_status();
        }
    }

    pub(super) fn capture_prior_focus(cx: &mut App) {
        let focused_window_id = Self::compositor_snapshot(cx).focused_window_id;
        let runtime = cx.global_mut::<Self>();
        if runtime.control_center.is_none() && runtime.overview.is_none() {
            runtime.prior_window_id = focused_window_id;
        }
    }

    pub(super) fn restore_prior_focus(cx: &mut App) {
        let runtime = cx.global_mut::<Self>();
        if runtime.control_center.is_some() || runtime.overview.is_some() {
            return;
        }
        if let Some(window_id) = runtime.prior_window_id.take() {
            let _ = Self::overview_focus_window(cx, window_id);
        }
    }

    pub fn toggle_overview(cx: &mut App) {
        if cx.global::<Self>().overview.is_some() {
            Self::close_overview(cx);
        } else {
            Self::open_or_focus_overview(cx);
        }
    }

    pub fn close_overview(cx: &mut App) {
        let overview = cx.global::<Self>().overview_entity.clone();
        if let Some(overview) = overview {
            overview.update(cx, |view, cx| {
                view.begin_close(crate::overview::OverviewCloseReason::Cancel, cx);
            });
        }
    }

    pub fn finish_overview_close(
        cx: &mut App,
        instance_id: u64,
        reason: crate::overview::OverviewCloseReason,
    ) {
        let runtime = cx.global_mut::<Self>();
        if runtime.overview_instance != instance_id {
            return;
        }

        let opened_workspace_id = runtime.overview_opened_workspace_id.take();
        let handle = runtime.overview.take();
        runtime.overview_entity = None;

        if let Some(handle) = handle {
            Self::dispatch_surface_lifecycle(cx, ContributionSurface::Launcher, false, 1280., 720.);
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }

        if should_restore_overview_prior_focus(
            reason,
            opened_workspace_id,
            Self::compositor_snapshot(cx).focused_workspace_id,
        ) {
            Self::restore_prior_focus(cx);
        }

        cx.global::<Self>().publish_status();
    }

    pub fn forget_overview(cx: &mut App, instance_id: u64) {
        let runtime = cx.global_mut::<Self>();
        if runtime.overview_instance != instance_id {
            return;
        }

        let had_handle = runtime.overview.take().is_some();
        runtime.overview_entity = None;
        runtime.overview_opened_workspace_id = None;

        if had_handle {
            Self::dispatch_surface_lifecycle(cx, ContributionSurface::Launcher, false, 1280., 720.);
            Self::restore_prior_focus(cx);
        }

        cx.global::<Self>().publish_status();
    }

    pub fn overview_focus_workspace(cx: &mut App, ws_id: u64) -> Result<(), ShellError> {
        Self::dispatch_action(cx, crate::actions::ActionInvocation::FocusWorkspace(ws_id))
    }

    pub fn overview_move_window(
        cx: &mut App,
        window_id: u64,
        workspace_id: u64,
    ) -> Result<(), ShellError> {
        Self::dispatch_action(
            cx,
            crate::actions::ActionInvocation::MoveWindowToWorkspace {
                window_id,
                workspace_id,
            },
        )
    }

    pub fn overview_focus_window(cx: &mut App, window_id: u64) -> Result<(), ShellError> {
        Self::dispatch_action(
            cx,
            crate::actions::ActionInvocation::FocusWindow(window_id),
        )
    }

    pub fn overview_reduced_motion(cx: &App) -> bool {
        Self::active_config(cx).theme.reduced_motion
    }

    pub fn is_overview_open(cx: &App) -> bool {
        cx.has_global::<Self>() && cx.global::<Self>().overview.is_some()
    }

    pub(super) fn refresh_bars(cx: &mut App) {
        let handles: Vec<_> = cx
            .global::<Self>()
            .bars
            .values()
            .map(|(handle, _)| *handle)
            .collect();
        for handle in handles {
            let _ = handle.update(cx, |_, _, cx| cx.notify());
        }
    }

    pub fn overview_wallpaper_path(cx: &App) -> Option<PathBuf> {
        if cx.has_global::<Self>() {
            let runtime = cx.global::<Self>();
            if let Some(path) = runtime.current_wallpaper_path.clone() {
                return Some(path);
            }
        }
        query_awww_wallpaper_path()
    }

    pub fn overview_applications(cx: &App) -> Vec<shilpo_services::Application> {
        Self::app_scanner(cx)
            .map(|scanner| scanner.applications())
            .unwrap_or_default()
    }

    pub fn normalize_app_key(value: &str) -> String {
        crate::app_icons::normalize_app_key(value)
    }

    pub fn app_icon_index(cx: &App) -> HashMap<String, PathBuf> {
        let apps = Self::overview_applications(cx);
        crate::app_icons::build_app_icon_index(apps)
    }

    pub fn begin_overview_instance(cx: &mut App) -> u64 {
        let runtime = cx.global_mut::<Self>();
        runtime.next_overview_instance = runtime.next_overview_instance.wrapping_add(1);
        runtime.overview_instance = runtime.next_overview_instance;
        runtime.overview_instance
    }

    pub fn open_or_focus_overview(cx: &mut App) {
        Self::open_or_focus_overview_on_display(cx, None);
    }

    pub fn open_or_focus_overview_on_display(cx: &mut App, target_display_id: Option<DisplayId>) {
        if let Some(handle) = cx.global::<Self>().overview {
            let _ = handle.update(cx, |_, window, _| window.activate_window());
            return;
        }

        Self::capture_prior_focus(cx);
        if let Some(discovered_wallpaper_path) = query_awww_wallpaper_path() {
            let theme_wallpaper_path = cx.global::<Self>().current_wallpaper_path.as_deref();
            if discovered_wallpaper_needs_theme_sync(
                theme_wallpaper_path,
                &discovered_wallpaper_path,
            ) {
                cx.global_mut::<Self>().current_wallpaper_path =
                    Some(discovered_wallpaper_path.clone());

                shilpo_theme_daemon::ThemeClient::spawn_task(async move {
                    let client = shilpo_theme_daemon::ThemeClient::new().await;
                    let _ = client.set_wallpaper(discovered_wallpaper_path.to_str().unwrap_or_default()).await;
                });

                Self::refresh_bars(cx);
            }
        }

        let _instance_id = Self::begin_overview_instance(cx);
        let opened_workspace_id = Self::compositor_snapshot(cx).focused_workspace_id;
        cx.global_mut::<Self>().overview_opened_workspace_id = opened_workspace_id;

        let window_size = size(px(1280.), px(720.));
        let (origin, display_id) = if let Some(display) = cx
            .displays()
            .into_iter()
            .find(|d| target_display_id == Some(d.id()))
            .or_else(|| cx.primary_display())
        {
            let bounds = display.bounds();
            (
                point(
                    bounds.origin.x + (bounds.size.width - window_size.width) / 2.0,
                    bounds.origin.y + (bounds.size.height - window_size.height) / 2.0,
                ),
                Some(display.id()),
            )
        } else {
            (point(px(320.), px(180.)), None)
        };

        let options = overlay_options(
            "shilpo-overview",
            "overview",
            window_size,
            origin,
            display_id,
        );

        match cx.open_window(options, move |window, cx| {
            crate::overview::WorkspaceOverview::view(window, cx)
        }) {
            Ok(handle) => {
                let runtime = cx.global_mut::<Self>();
                runtime.overview = Some(handle);
                Self::dispatch_surface_lifecycle(cx, ContributionSurface::Launcher, true, 1280., 720.);
                cx.global::<Self>().publish_status();
            }
            Err(error) => tracing::warn!(error = %error, "failed to open overview overlay"),
        }
    }

    pub fn open_or_focus_control_center(cx: &mut App) {
        if let Some(handle) = cx.global::<Self>().control_center {
            let _ = handle.update(cx, |_, window, _| window.activate_window());
            return;
        }

        Self::capture_prior_focus(cx);

        let (display_bounds, display_id) = if let Some(display) = cx.primary_display() {
            (display.bounds(), Some(display.id()))
        } else {
            (
                Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.))),
                None,
            )
        };

        let cc_size = size(px(340.), px(540.));
        let origin = point(
            display_bounds.origin.x + display_bounds.size.width - cc_size.width - px(16.),
            display_bounds.origin.y + px(48.),
        );

        let options = overlay_options(
            "shilpo-control-center",
            "control-center",
            cc_size,
            origin,
            display_id,
        );

        match cx.open_window(options, move |window, cx| {
            ControlCenterView::view(window, cx)
        }) {
            Ok(handle) => {
                cx.global_mut::<Self>().control_center = Some(handle);
                Self::dispatch_surface_lifecycle(
                    cx,
                    ContributionSurface::ControlCenter,
                    true,
                    340.,
                    540.,
                );
                cx.global::<Self>().publish_status();
            }
            Err(error) => tracing::warn!(error = %error, "failed to open control center overlay"),
        }
    }

    pub fn toggle_control_center(cx: &mut App) {
        if cx.global::<Self>().control_center.is_some() {
            Self::close_control_center(cx);
        } else {
            Self::open_or_focus_control_center(cx);
        }
    }

    pub fn close_control_center(cx: &mut App) {
        if !Self::remove_control_center_surface(cx) {
            return;
        }
        Self::restore_prior_focus(cx);
        cx.global::<Self>().publish_status();
    }

    #[allow(dead_code)]
    fn close_control_center_for_replacement(cx: &mut App) {
        let _ = Self::remove_control_center_surface(cx);
    }

    fn remove_control_center_surface(cx: &mut App) -> bool {
        let handle = cx.global_mut::<Self>().control_center.take();
        let Some(handle) = handle else { return false };
        Self::dispatch_surface_lifecycle(cx, ContributionSurface::ControlCenter, false, 340., 540.);
        let _ = handle.update(cx, |_, window, _| window.remove_window());
        true
    }

    pub fn forget_control_center(cx: &mut App) {
        let had_handle = cx.global_mut::<Self>().control_center.take().is_some();
        if had_handle {
            Self::dispatch_surface_lifecycle(
                cx,
                ContributionSurface::ControlCenter,
                false,
                340.,
                540.,
            );
            Self::restore_prior_focus(cx);
        }
        cx.global::<Self>().publish_status();
    }

    pub fn register_overview_entity(
        cx: &mut App,
        entity: Entity<crate::overview::WorkspaceOverview>,
    ) {
        if cx.has_global::<Self>() {
            cx.global_mut::<Self>().overview_entity = Some(entity);
        }
    }

    pub fn active_notification_handle(
        cx: &App,
    ) -> Option<WindowHandle<crate::notification::NotificationToastView>> {
        if cx.has_global::<Self>() {
            cx.global::<Self>()
                .notification
                .as_ref()
                .map(|(_, _, handle)| *handle)
        } else {
            None
        }
    }

    pub(crate) fn register_notification(
        cx: &mut App,
        generation: u64,
        notification_id: u32,
        handle: WindowHandle<crate::notification::NotificationToastView>,
    ) {
        let runtime = cx.global_mut::<Self>();
        runtime.notification = Some((generation, notification_id, handle));
        tracing::warn!("[NOTIFTRACE] register_notification gen={generation} id={notification_id}");
    }

    pub fn close_active_notification(cx: &mut App) {
        let entry = cx.global_mut::<Self>().notification.take();
        if let Some((_, notification_id, handle)) = entry {
            tracing::warn!(
                "[NOTIFTRACE] close_active_notification id={notification_id} removing window"
            );
            Self::dismiss_notification(cx, notification_id);
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        } else {
            tracing::warn!("[NOTIFTRACE] close_active_notification no active entry");
        }
    }

    pub fn expire_notification(cx: &mut App, generation: u64) {
        let entry = cx.global_mut::<Self>().notification.take();
        let Some((current_generation, notification_id, handle)) = entry else {
            tracing::warn!(
                "[NOTIFTRACE] expire_notification(gen={generation}) no active entry; window NOT removed"
            );
            return;
        };
        if current_generation != generation {
            cx.global_mut::<Self>().notification =
                Some((current_generation, notification_id, handle));
            tracing::warn!(
                "[NOTIFTRACE] expire_notification stale gen={generation} current={current_generation}; window NOT removed"
            );
            return;
        }
        tracing::warn!(
            "[NOTIFTRACE] expire_notification(gen={generation}) id={notification_id} removing window"
        );
        Self::expire_notification_id(cx, notification_id);
        let _ = handle.update(cx, |_, window, _| window.remove_window());
    }

    pub fn forget_notification(cx: &mut App, generation: u64) {
        let is_current = cx
            .global::<Self>()
            .notification
            .as_ref()
            .is_some_and(|(current_generation, _, _)| *current_generation == generation);
        if is_current
            && let Some((_, notification_id, _)) = cx.global_mut::<Self>().notification.take()
        {
            tracing::warn!(
                "[NOTIFTRACE] forget_notification gen={generation} id={notification_id} entry dropped"
            );
            Self::dismiss_notification(cx, notification_id);
        } else {
            tracing::warn!(
                "[NOTIFTRACE] forget_notification gen={generation} mismatch/none; no-op"
            );
        }
    }

    fn dismiss_notification(cx: &App, notification_id: u32) {
        if cx.has_global::<Self>()
            && let Some(notification) = cx
                .global::<Self>()
                .service_hub
                .as_ref()
                .and_then(|hub| hub.notification.as_ref())
        {
            notification.dismiss(notification_id);
        }
    }

    fn expire_notification_id(cx: &App, notification_id: u32) {
        if cx.has_global::<Self>()
            && let Some(notification) = cx
                .global::<Self>()
                .service_hub
                .as_ref()
                .and_then(|hub| hub.notification.as_ref())
        {
            notification.expire(notification_id);
        }
    }

    pub fn forget_osd(cx: &mut App) {
        if cx.has_global::<Self>() {
            cx.global_mut::<Self>().osd = None;
        }
    }

    fn schedule_osd_dismiss(cx: &mut App, generation: u64) {
        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(2))
                .await;
            cx.update(|cx: &mut gpui::App| {
                if cx.has_global::<Self>() {
                    let runtime = cx.global_mut::<Self>();
                    if let Some((current_gen, handle, _)) = &runtime.osd
                        && *current_gen == generation
                    {
                        let window_handle = *handle;
                        runtime.osd = None;
                        let _ = window_handle.update(cx, |_, window, _| window.remove_window());
                    }
                }
            });
        })
        .detach();
    }

    pub fn show_osd(cx: &mut App, kind: crate::osd::OsdKind) {
        let existing = cx.global_mut::<Self>().osd.take();
        if let Some((generation, window_handle, view_handle)) = existing {
            view_handle.update(cx, |view, cx| {
                view.kind = kind;
                cx.notify();
            });
            let next_gen = generation + 1;
            cx.global_mut::<Self>().osd = Some((next_gen, window_handle, view_handle));
            Self::schedule_osd_dismiss(cx, next_gen);
            return;
        }

        let (display_bounds, display_id) = if let Some(display) = cx.primary_display() {
            (display.bounds(), Some(display.id()))
        } else {
            (
                Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.))),
                None,
            )
        };
        let osd_size = size(px(260.), px(48.));
        let origin = point(
            display_bounds.origin.x + (display_bounds.size.width - osd_size.width) / 2.0,
            display_bounds.origin.y + display_bounds.size.height - px(140.),
        );
        let options = WindowOptions {
            titlebar: None,
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(origin, osd_size))),
            display_id,
            app_id: Some("shilpo-osd".into()),
            window_background: WindowBackgroundAppearance::Transparent,
            kind: WindowKind::LayerShell(LayerShellOptions {
                namespace: "osd".into(),
                layer: Layer::Overlay,
                anchor: Anchor::BOTTOM,
                margin: Some((px(0.), px(0.), px(84.), px(0.))),
                keyboard_interactivity: KeyboardInteractivity::None,
                ..Default::default()
            }),
            focus: false,
            show: true,
            ..Default::default()
        };

        let spawned_view: std::sync::Arc<std::sync::Mutex<Option<Entity<crate::osd::OsdView>>>> =
            std::sync::Arc::new(std::sync::Mutex::new(None));
        let view_cell = spawned_view.clone();
        let window_result = cx.open_window(options, move |window, cx| {
            let (root, view) = crate::osd::OsdView::view(kind, window, cx);
            *view_cell.lock().unwrap() = Some(view);
            root
        });

        if let Ok(window_handle) = window_result
            && let Some(view_handle) = spawned_view.lock().unwrap().take()
        {
            cx.global_mut::<Self>().osd = Some((1, window_handle, view_handle));
            Self::schedule_osd_dismiss(cx, 1);
        }
    }

    pub fn forget_bar(cx: &mut App) {
        let runtime = cx.global_mut::<Self>();
        runtime.bars.clear();
        runtime.bar_state = BarState::Hidden;
        runtime.publish_status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_awww_wallpaper_query() {
        let output =
            ": eDP-1: 1920x1080, scale: 1, currently displaying: image: /pictures/wallpaper.png\n";
        assert_eq!(
            parse_awww_wallpaper_path(output),
            Some(PathBuf::from("/pictures/wallpaper.png"))
        );
        assert_eq!(parse_awww_wallpaper_path("no image"), None);
    }

    #[test]
    fn discovered_wallpaper_syncs_when_theme_path_is_missing_or_stale() {
        let discovered = Path::new("/pictures/red-wallpaper.png");

        assert!(discovered_wallpaper_needs_theme_sync(None, discovered));
        assert!(discovered_wallpaper_needs_theme_sync(
            Some(Path::new("/pictures/old-wallpaper.png")),
            discovered
        ));
        assert!(!discovered_wallpaper_needs_theme_sync(
            Some(discovered),
            discovered
        ));
    }

    #[test]
    fn closing_overview_after_workspace_change_does_not_restore_origin_focus() {
        assert!(!should_restore_overview_prior_focus(
            crate::overview::OverviewCloseReason::Cancel,
            Some(2),
            Some(1),
        ));
        assert!(should_restore_overview_prior_focus(
            crate::overview::OverviewCloseReason::Cancel,
            Some(2),
            Some(2),
        ));
        assert!(!should_restore_overview_prior_focus(
            crate::overview::OverviewCloseReason::Selection,
            Some(2),
            Some(2),
        ));
    }

    #[test]
    fn test_performance_frame_budget_compliance() {
        use std::time::Instant;
        let config = shilpo_config::ShellConfig::default();
        let display_bounds = gpui::Bounds {
            origin: gpui::Point::default(),
            size: gpui::Size {
                width: gpui::px(1920.0),
                height: gpui::px(1080.0),
            },
        };
        let display_id = gpui::DisplayId::from(1u64);

        let start = Instant::now();
        for _ in 0..1000 {
            let _geom = crate::bar::geometry::BarGeometry::calculate_with_scale(
                display_id,
                display_bounds,
                &config.bar,
                Some(1.0),
            );
        }
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 16,
            "1000 geometry calculations took {:?}, exceeding 16.6ms frame budget",
            elapsed
        );
    }

    #[test]
    fn test_gpui_surface_rendering_specs() {
        let config = shilpo_config::ShellConfig::default();
        let display_bounds = Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.)));
        let display_id = gpui::DisplayId::from(1u64);

        let bar_geom = crate::bar::geometry::BarGeometry::calculate_with_scale(
            display_id,
            display_bounds,
            &config.bar,
            Some(1.0),
        );
        assert!(bar_geom.bounds.size.height >= px(config.bar.height as f32));
    }

    #[test]
    fn test_multi_output_dpi_resolution_scaling_fixtures() {
        let config = shilpo_config::ShellConfig::default();
        let display_bounds = Bounds::new(point(px(0.), px(0.)), size(px(3840.), px(2160.)));
        let display_id = gpui::DisplayId::from(2u64);

        let bar_geom = crate::bar::geometry::BarGeometry::calculate_with_scale(
            display_id,
            display_bounds,
            &config.bar,
            Some(2.0),
        );
        assert_eq!(bar_geom.display_id, display_id);
        assert_eq!(bar_geom.bounds.size.width, px(3840.));
    }

    #[test]
    fn output_name_matching_uses_geometry_when_uuid_mapping_is_unavailable() {
        let outputs = vec![
            CompositorOutput {
                name: "eDP-1".into(),
                make: None,
                model: None,
                logical_position: (0, 0),
                logical_size: (1920, 1080),
                scale: 1.0,
            },
            CompositorOutput {
                name: "HDMI-A-1".into(),
                make: None,
                model: None,
                logical_position: (1920, 0),
                logical_size: (1920, 1080),
                scale: 1.0,
            },
        ];
        let bounds = Bounds::new(point(px(1920.), px(0.)), size(px(1920.), px(1080.)));
        assert_eq!(
            ShellRuntime::output_name_for_bounds(bounds, &outputs),
            Some("HDMI-A-1".into())
        );
    }

    #[test]
    fn test_workspace_overview_surface() {
        let mut overview = crate::overview::WorkspaceOverview::new_offline();
        assert_eq!(overview.selected_window_id(), Some(101));
        overview.select_next_window();
        assert_eq!(overview.selected_window_id(), Some(101));
    }
}
