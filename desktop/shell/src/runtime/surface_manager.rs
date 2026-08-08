use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};

use gpui::{
    App, AppContext, Bounds, DisplayId, Entity, Pixels, Point, Size, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowKind, WindowOptions,
    layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
    point, px, size,
};
use shilpo_ext_types::CanonicalId;
use shilpo_services::{
    BarState, CompositorAdapter, CompositorOutput, CompositorSnapshot, ShellIpcServer,
};
use shilpo_theme_daemon::DaemonState;
use uuid::Uuid;

use crate::{
    ControlCenterView,
    actions::ActionInvocation,
    bar::{BarSpec, BarView, OutputDescriptor, ReconciliationOp, geometry::BarGeometry},
    error::ShellError,
    extensions::{ContributionInstance, ContributionSurface},
    overview::{OverviewCloseReason, WorkspaceOverview},
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

fn overlay_options(
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

pub(super) fn attach_compositor_stream(
    ipc_server: &ShellIpcServer,
    compositor: &Arc<dyn CompositorAdapter>,
) -> Arc<CompositorSnapshot> {
    ipc_server.attach_broker(compositor.command_broker());
    compositor.current()
}

pub(super) fn spawn_compositor_stream_loop(cx: &mut App, compositor: &Arc<dyn CompositorAdapter>) {
    let mut rx = compositor.subscribe();
    cx.spawn(async move |cx| {
        while rx.changed().await.is_ok() {
            let snapshot = rx.borrow().clone();
            cx.update(|cx: &mut gpui::App| {
                ShellRuntime::on_compositor_snapshot_changed(cx, snapshot);
            });
        }
    })
    .detach();
}

fn discovered_wallpaper_needs_theme_sync(
    theme_wallpaper_path: Option<&Path>,
    discovered_wallpaper_path: &Path,
) -> bool {
    theme_wallpaper_path != Some(discovered_wallpaper_path)
}

fn should_restore_overview_prior_focus(
    reason: OverviewCloseReason,
    opened_workspace_id: Option<u64>,
    current_workspace_id: Option<u64>,
) -> bool {
    if reason != OverviewCloseReason::Cancel {
        return false;
    }

    match (opened_workspace_id, current_workspace_id) {
        (Some(opened), Some(current)) => opened == current,
        _ => true,
    }
}

/// Describes a desktop surface contributed by an extension.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtensionSurfaceSpec {
    pub contribution: CanonicalId,
    pub display_id: DisplayId,
    pub bounds: Bounds<Pixels>,
}

/// Merges the global extension settings with a per-instance override map.
pub(crate) fn extension_settings(
    config: &shilpo_config::ShellConfig,
    extension_id: &shilpo_ext_types::ExtensionId,
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

/// The set of shell windows that must be torn down on shutdown.
pub(crate) struct ShutdownWindows {
    pub(crate) bars: HashMap<DisplayId, (WindowHandle<BarView>, BarSpec)>,
    pub(crate) extension_surfaces:
        HashMap<String, (WindowHandle<shilpo_ui::Root>, ExtensionSurfaceSpec)>,
    pub(crate) extension_panel: Option<(WindowHandle<shilpo_ui::Root>, CanonicalId)>,
    pub(crate) control_center: Option<WindowHandle<shilpo_ui::Root>>,
    pub(crate) notification: Option<(
        u64,
        u32,
        WindowHandle<crate::notification::NotificationToastView>,
    )>,
}

/// What closed when a shell window was destroyed.
pub(crate) enum WindowClosedOutcome {
    Nothing,
    ControlCenter,
    ExtensionPanel(CanonicalId),
}

/// Owns every shell window: the per-display bars, the control center, the
/// workspace overview, notification toasts, the OSD, and the extension surfaces.
///
/// Window handles and surface state are private; the shell interacts with the
/// manager exclusively through the method surface below.
pub struct SurfaceManager {
    bars: HashMap<DisplayId, (WindowHandle<BarView>, BarSpec)>,
    last_bar_specs: Vec<(BarGeometry, bool)>,
    bar_state: BarState,
    control_center: Option<WindowHandle<shilpo_ui::Root>>,
    overview: Option<WindowHandle<shilpo_ui::Root>>,
    overview_entity: Option<Entity<WorkspaceOverview>>,
    overview_instance: u64,
    next_overview_instance: u64,
    overview_opened_workspace_id: Option<u64>,
    current_wallpaper_path: Option<PathBuf>,
    latest_snapshot: Arc<CompositorSnapshot>,
    notification: Option<(
        u64,
        u32,
        WindowHandle<crate::notification::NotificationToastView>,
    )>,
    notification_generation: u64,
    prior_window_id: Option<u64>,
    osd: Option<(
        u64,
        WindowHandle<shilpo_ui::Root>,
        Entity<crate::osd::OsdView>,
    )>,
    _osd_generation: u64,
    extension_surfaces: HashMap<String, (WindowHandle<shilpo_ui::Root>, ExtensionSurfaceSpec)>,
    extension_panel: Option<(WindowHandle<shilpo_ui::Root>, CanonicalId)>,
    extension_output_ids: HashSet<DisplayId>,
    readiness: shilpo_services::ipc::ReadinessState,
}

impl SurfaceManager {
    pub fn new(
        initial_wallpaper_path: Option<PathBuf>,
        latest_snapshot: Arc<CompositorSnapshot>,
    ) -> Self {
        let mut manager = Self {
            bars: HashMap::new(),
            last_bar_specs: Vec::new(),
            bar_state: BarState::Starting,
            control_center: None,
            overview: None,
            overview_entity: None,
            overview_instance: 0,
            next_overview_instance: 0,
            overview_opened_workspace_id: None,
            current_wallpaper_path: initial_wallpaper_path,
            latest_snapshot,
            notification: None,
            notification_generation: 0,
            prior_window_id: None,
            osd: None,
            _osd_generation: 0,
            extension_surfaces: HashMap::new(),
            extension_panel: None,
            extension_output_ids: HashSet::new(),
            readiness: shilpo_services::ipc::ReadinessState::Starting,
        };
        manager.update_readiness();
        manager
    }

    pub(crate) fn latest_snapshot(&self) -> Arc<CompositorSnapshot> {
        self.latest_snapshot.clone()
    }

    pub(crate) fn set_latest_snapshot(&mut self, snapshot: Arc<CompositorSnapshot>) {
        self.latest_snapshot = snapshot;
    }

    #[allow(dead_code)]
    pub(crate) fn bar_state(&self) -> BarState {
        self.bar_state.clone()
    }

    pub(crate) fn set_bar_state(&mut self, state: BarState) {
        self.bar_state = state;
        self.update_readiness();
    }

    pub(crate) fn readiness(&self) -> shilpo_services::ipc::ReadinessState {
        self.readiness
    }

    pub(crate) fn update_readiness(&mut self) {
        self.readiness = readiness_for(&self.latest_snapshot.connection, &self.bar_state);
    }

    pub(crate) fn store_extension_surface(
        &mut self,
        instance_id: String,
        handle: WindowHandle<shilpo_ui::Root>,
        spec: ExtensionSurfaceSpec,
    ) {
        self.extension_surfaces.insert(instance_id, (handle, spec));
    }

    pub(crate) fn remove_extension_surface(
        &mut self,
        id: &str,
    ) -> Option<(WindowHandle<shilpo_ui::Root>, ExtensionSurfaceSpec)> {
        self.extension_surfaces.remove(id)
    }

    pub(crate) fn stale_extension_surface_ids(
        &self,
        desired: &HashMap<String, ExtensionSurfaceSpec>,
    ) -> Vec<String> {
        self.extension_surfaces
            .iter()
            .filter(|(id, (_, current))| desired.get(*id) != Some(current))
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub(crate) fn has_extension_surface(&self, instance_id: &str) -> bool {
        self.extension_surfaces.contains_key(instance_id)
    }

    pub(crate) fn bar_handles(&self) -> Vec<WindowHandle<BarView>> {
        self.bars.values().map(|(handle, _)| *handle).collect()
    }

    pub(crate) fn overview_entity(&self) -> Option<Entity<WorkspaceOverview>> {
        self.overview_entity.clone()
    }

    pub(crate) fn current_wallpaper_path(&self) -> Option<PathBuf> {
        self.current_wallpaper_path.clone()
    }

    pub(crate) fn is_overview_open(&self) -> bool {
        self.overview.is_some()
    }

    pub(crate) fn is_control_center_open(&self) -> bool {
        self.control_center.is_some()
    }

    pub(crate) fn has_bars(&self) -> bool {
        !self.bars.is_empty()
    }

    pub(crate) fn begin_overview_instance(&mut self) -> u64 {
        self.next_overview_instance = self.next_overview_instance.wrapping_add(1);
        self.overview_instance = self.next_overview_instance;
        self.overview_instance
    }

    pub(crate) fn reserve_notification_generation(&mut self) -> u64 {
        self.notification_generation = self.notification_generation.wrapping_add(1);
        self.notification_generation
    }

    pub(crate) fn register_notification(
        &mut self,
        generation: u64,
        notification_id: u32,
        handle: WindowHandle<crate::notification::NotificationToastView>,
    ) {
        self.notification = Some((generation, notification_id, handle));
        tracing::warn!("[NOTIFTRACE] register_notification gen={generation} id={notification_id}");
    }

    pub(crate) fn active_notification_handle(
        &self,
    ) -> Option<WindowHandle<crate::notification::NotificationToastView>> {
        self.notification.as_ref().map(|(_, _, handle)| *handle)
    }

    /// Removes every surface belonging to a window that was just closed and
    /// reports which overlay was destroyed so the orchestrator can emit events.
    pub(crate) fn handle_window_closed(
        &mut self,
        window_id: gpui::WindowId,
    ) -> WindowClosedOutcome {
        let mut outcome = WindowClosedOutcome::Nothing;
        self.bars
            .retain(|_, (handle, _)| handle.window_id() != window_id);
        self.extension_surfaces
            .retain(|_, (handle, _)| handle.window_id() != window_id);
        if self.bars.is_empty() {
            self.bar_state = BarState::Hidden;
        }
        if self
            .control_center
            .as_ref()
            .is_some_and(|handle| handle.window_id() == window_id)
        {
            self.control_center = None;
            outcome = WindowClosedOutcome::ControlCenter;
        }
        let panel = self.extension_panel.take();
        if let Some((handle, id)) = panel {
            if handle.window_id() == window_id {
                outcome = WindowClosedOutcome::ExtensionPanel(id);
            } else {
                self.extension_panel = Some((handle, id));
            }
        }
        if self
            .notification
            .as_ref()
            .is_some_and(|(_, _, handle)| handle.window_id() == window_id)
        {
            self.notification = None;
        }
        outcome
    }

    /// Collects every open window so the orchestrator can tear them down during shutdown.
    pub(crate) fn take_windows_for_shutdown(&mut self) -> ShutdownWindows {
        self.bar_state = BarState::Hidden;
        ShutdownWindows {
            bars: std::mem::take(&mut self.bars),
            extension_surfaces: std::mem::take(&mut self.extension_surfaces),
            extension_panel: self.extension_panel.take(),
            control_center: self.control_center.take(),
            notification: self.notification.take(),
        }
    }

    pub(crate) fn output_name_for_bounds(
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

    pub(crate) fn output_name_for_display(
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

    pub(crate) fn sync_displays(cx: &mut App) {
        if !cx.has_global::<ShellRuntime>() {
            return;
        }

        let snapshot = ShellRuntime::compositor_snapshot(cx);

        let current_outputs = cx
            .displays()
            .into_iter()
            .enumerate()
            .map(|(index, display)| {
                let display_id = display.id();
                let output_name = Self::output_name_for_display(&*display, &snapshot.outputs);
                let is_primary = index == 0;

                OutputDescriptor {
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
            .collect::<HashSet<_>>();
        let changed = {
            let surface_manager = cx.global_mut::<ShellRuntime>().surface_manager_mut();
            if output_ids != surface_manager.extension_output_ids {
                surface_manager.extension_output_ids = output_ids;
                true
            } else {
                false
            }
        };
        if changed {
            ShellRuntime::dispatch_extension_event(cx, shilpo_ext::ExtensionEvent::OutputsChanged);
        }

        Self::reconcile_extension_surfaces(cx, &current_outputs);
        Self::reconcile_bars(cx);
    }

    pub(crate) fn open_bar_with_spec(cx: &mut App, spec: BarSpec) -> bool {
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
                let surface_manager = cx.global_mut::<ShellRuntime>().surface_manager_mut();
                surface_manager.bars.insert(display_id, (handle, spec));
                surface_manager.bar_state = BarState::Visible;
                ShellRuntime::publish_status(cx);
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

    pub(crate) fn open_bar(
        cx: &mut App,
        geometry: &BarGeometry,
        with_display_geometry: bool,
    ) -> bool {
        let config = ShellRuntime::active_config(cx).bar;
        let spec = BarSpec::new(geometry.clone(), config, with_display_geometry);
        Self::open_bar_with_spec(cx, spec)
    }

    pub(crate) fn reconcile_bars(cx: &mut App) {
        let snapshot = ShellRuntime::compositor_snapshot(cx);
        let outputs = cx
            .displays()
            .into_iter()
            .enumerate()
            .map(|(index, display)| {
                let display_id = display.id();
                let output_name = Self::output_name_for_display(&*display, &snapshot.outputs);
                let is_primary = index == 0;

                OutputDescriptor {
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
                .global::<ShellRuntime>()
                .surface_manager()
                .bars
                .iter()
                .map(|(id, (_, spec))| (*id, spec.clone()))
                .collect();
            crate::bar::reconciliation::reconcile_output_bars(
                &outputs,
                &ShellRuntime::active_config(cx),
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
                    if let Some((handle, _)) = cx
                        .global_mut::<ShellRuntime>()
                        .surface_manager_mut()
                        .bars
                        .remove(&display_id)
                    {
                        let _ = handle.update(cx, |_, window, _| window.remove_window());
                    }
                }
                ReconciliationOp::Retain(_) => {
                    mounted = true;
                }
            }
        }

        if !mounted
            && !cx
                .global::<ShellRuntime>()
                .surface_manager()
                .bars
                .is_empty()
        {
            mounted = true;
        }

        if !mounted {
            cx.global_mut::<ShellRuntime>()
                .surface_manager_mut()
                .set_bar_state(BarState::OpenFailed);
            ShellRuntime::publish_status(cx);
        }
    }

    pub(crate) fn mark_bar_open_failed(cx: &mut App) {
        cx.global_mut::<ShellRuntime>()
            .surface_manager_mut()
            .set_bar_state(BarState::OpenFailed);
        ShellRuntime::publish_status(cx);
    }

    pub(crate) fn toggle_bar(cx: &mut App) {
        if !cx.global::<ShellRuntime>().surface_manager().has_bars() {
            Self::reconcile_bars(cx);
        } else {
            let old_bars =
                std::mem::take(&mut cx.global_mut::<ShellRuntime>().surface_manager_mut().bars);
            for (_, (handle, _)) in old_bars {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            }
            let surface_manager = cx.global_mut::<ShellRuntime>().surface_manager_mut();
            surface_manager.bar_state = BarState::Hidden;
            surface_manager.last_bar_specs.clear();
            ShellRuntime::publish_status(cx);
        }
    }

    pub(super) fn capture_prior_focus(cx: &mut App) {
        let focused_window_id = ShellRuntime::compositor_snapshot(cx).focused_window_id;
        let surface_manager = cx.global_mut::<ShellRuntime>().surface_manager_mut();
        if surface_manager.control_center.is_none() && surface_manager.overview.is_none() {
            surface_manager.prior_window_id = focused_window_id;
        }
    }

    pub(super) fn restore_prior_focus(cx: &mut App) {
        let prior_window_id = {
            let surface_manager = cx.global_mut::<ShellRuntime>().surface_manager_mut();
            if surface_manager.control_center.is_some() || surface_manager.overview.is_some() {
                None
            } else {
                surface_manager.prior_window_id.take()
            }
        };
        if let Some(window_id) = prior_window_id {
            let _ = Self::overview_focus_window(cx, window_id);
        }
    }

    pub(crate) fn toggle_overview(cx: &mut App) {
        if cx
            .global::<ShellRuntime>()
            .surface_manager()
            .overview
            .is_some()
        {
            Self::close_overview(cx);
        } else {
            Self::open_or_focus_overview(cx);
        }
    }

    pub(crate) fn close_overview(cx: &mut App) {
        let overview = cx
            .global::<ShellRuntime>()
            .surface_manager()
            .overview_entity
            .clone();
        if let Some(overview) = overview {
            overview.update(cx, |view, cx| {
                view.begin_close(OverviewCloseReason::Cancel, cx);
            });
        }
    }

    pub(crate) fn finish_overview_close(
        cx: &mut App,
        instance_id: u64,
        reason: OverviewCloseReason,
    ) {
        let (opened_workspace_id, handle) = {
            let surface_manager = cx.global_mut::<ShellRuntime>().surface_manager_mut();
            if surface_manager.overview_instance != instance_id {
                return;
            }
            let opened_workspace_id = surface_manager.overview_opened_workspace_id.take();
            let handle = surface_manager.overview.take();
            surface_manager.overview_entity = None;
            (opened_workspace_id, handle)
        };

        if let Some(handle) = handle {
            ShellRuntime::dispatch_surface_lifecycle(
                cx,
                ContributionSurface::Launcher,
                false,
                1280.,
                720.,
            );
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }

        if should_restore_overview_prior_focus(
            reason,
            opened_workspace_id,
            ShellRuntime::compositor_snapshot(cx).focused_workspace_id,
        ) {
            Self::restore_prior_focus(cx);
        }

        ShellRuntime::publish_status(cx);
    }

    pub(crate) fn forget_overview(cx: &mut App, instance_id: u64) {
        let had_handle = {
            let surface_manager = cx.global_mut::<ShellRuntime>().surface_manager_mut();
            if surface_manager.overview_instance != instance_id {
                return;
            }
            let had_handle = surface_manager.overview.take().is_some();
            surface_manager.overview_entity = None;
            surface_manager.overview_opened_workspace_id = None;
            had_handle
        };

        if had_handle {
            ShellRuntime::dispatch_surface_lifecycle(
                cx,
                ContributionSurface::Launcher,
                false,
                1280.,
                720.,
            );
            Self::restore_prior_focus(cx);
        }

        ShellRuntime::publish_status(cx);
    }

    pub(crate) fn overview_focus_workspace(cx: &mut App, ws_id: u64) -> Result<(), ShellError> {
        ShellRuntime::dispatch_action(cx, ActionInvocation::FocusWorkspace(ws_id))
    }

    pub(crate) fn overview_move_window(
        cx: &mut App,
        window_id: u64,
        workspace_id: u64,
    ) -> Result<(), ShellError> {
        ShellRuntime::dispatch_action(
            cx,
            ActionInvocation::MoveWindowToWorkspace {
                window_id,
                workspace_id,
            },
        )
    }

    pub(crate) fn overview_focus_window(cx: &mut App, window_id: u64) -> Result<(), ShellError> {
        ShellRuntime::dispatch_action(cx, ActionInvocation::FocusWindow(window_id))
    }

    pub(crate) fn overview_reduced_motion(cx: &App) -> bool {
        ShellRuntime::active_config(cx).theme.reduced_motion
    }

    pub(crate) fn refresh_bars(cx: &mut App) {
        let handles = cx.global::<ShellRuntime>().surface_manager().bar_handles();
        for handle in handles {
            let _ = handle.update(cx, |_, _, cx| cx.notify());
        }
    }

    pub(crate) fn overview_wallpaper_path(cx: &App) -> Option<PathBuf> {
        if cx.has_global::<ShellRuntime>()
            && let Some(path) = cx
                .global::<ShellRuntime>()
                .surface_manager()
                .current_wallpaper_path()
        {
            return Some(path);
        }
        query_awww_wallpaper_path()
    }

    pub(crate) fn overview_applications(cx: &App) -> Vec<shilpo_services::Application> {
        ShellRuntime::app_scanner(cx)
            .map(|scanner| scanner.applications())
            .unwrap_or_default()
    }

    pub(crate) fn normalize_app_key(value: &str) -> String {
        crate::app_icons::normalize_app_key(value)
    }

    pub(crate) fn app_icon_index(cx: &App) -> HashMap<String, PathBuf> {
        let apps = Self::overview_applications(cx);
        crate::app_icons::build_app_icon_index(apps)
    }

    pub(crate) fn open_or_focus_overview(cx: &mut App) {
        Self::open_or_focus_overview_on_display(cx, None);
    }

    pub(crate) fn open_or_focus_overview_on_display(
        cx: &mut App,
        target_display_id: Option<DisplayId>,
    ) {
        if let Some(handle) = cx.global::<ShellRuntime>().surface_manager().overview {
            let _ = handle.update(cx, |_, window, _| window.activate_window());
            return;
        }

        Self::capture_prior_focus(cx);
        if let Some(discovered_wallpaper_path) = query_awww_wallpaper_path() {
            let theme_wallpaper_path = cx
                .global::<ShellRuntime>()
                .surface_manager()
                .current_wallpaper_path
                .as_deref();
            if discovered_wallpaper_needs_theme_sync(
                theme_wallpaper_path,
                &discovered_wallpaper_path,
            ) {
                cx.global_mut::<ShellRuntime>()
                    .surface_manager_mut()
                    .current_wallpaper_path = Some(discovered_wallpaper_path.clone());

                shilpo_theme_daemon::ThemeClient::spawn_task(async move {
                    let client = shilpo_theme_daemon::ThemeClient::new().await;
                    let _ = client
                        .set_wallpaper(discovered_wallpaper_path.to_str().unwrap_or_default())
                        .await;
                });

                Self::refresh_bars(cx);
            }
        }

        let _instance_id = cx
            .global_mut::<ShellRuntime>()
            .surface_manager_mut()
            .begin_overview_instance();
        let opened_workspace_id = ShellRuntime::compositor_snapshot(cx).focused_workspace_id;
        cx.global_mut::<ShellRuntime>()
            .surface_manager_mut()
            .overview_opened_workspace_id = opened_workspace_id;

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
            WorkspaceOverview::view(window, cx)
        }) {
            Ok(handle) => {
                let surface_manager = cx.global_mut::<ShellRuntime>().surface_manager_mut();
                surface_manager.overview = Some(handle);
                ShellRuntime::dispatch_surface_lifecycle(
                    cx,
                    ContributionSurface::Launcher,
                    true,
                    1280.,
                    720.,
                );
                ShellRuntime::publish_status(cx);
            }
            Err(error) => tracing::warn!(error = %error, "failed to open overview overlay"),
        }
    }

    pub(crate) fn open_or_focus_control_center(cx: &mut App) {
        if let Some(handle) = cx.global::<ShellRuntime>().surface_manager().control_center {
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
                let surface_manager = cx.global_mut::<ShellRuntime>().surface_manager_mut();
                surface_manager.control_center = Some(handle);
                ShellRuntime::dispatch_surface_lifecycle(
                    cx,
                    ContributionSurface::ControlCenter,
                    true,
                    340.,
                    540.,
                );
                ShellRuntime::publish_status(cx);
            }
            Err(error) => tracing::warn!(error = %error, "failed to open control center overlay"),
        }
    }

    pub(crate) fn toggle_control_center(cx: &mut App) {
        if cx
            .global::<ShellRuntime>()
            .surface_manager()
            .control_center
            .is_some()
        {
            Self::close_control_center(cx);
        } else {
            Self::open_or_focus_control_center(cx);
        }
    }

    pub(crate) fn close_control_center(cx: &mut App) {
        if !Self::remove_control_center_surface(cx) {
            return;
        }
        Self::restore_prior_focus(cx);
        ShellRuntime::publish_status(cx);
    }

    fn remove_control_center_surface(cx: &mut App) -> bool {
        let handle = cx
            .global_mut::<ShellRuntime>()
            .surface_manager_mut()
            .control_center
            .take();
        let Some(handle) = handle else { return false };
        ShellRuntime::dispatch_surface_lifecycle(
            cx,
            ContributionSurface::ControlCenter,
            false,
            340.,
            540.,
        );
        let _ = handle.update(cx, |_, window, _| window.remove_window());
        true
    }

    pub(crate) fn forget_control_center(cx: &mut App) {
        let had_handle = cx
            .global_mut::<ShellRuntime>()
            .surface_manager_mut()
            .control_center
            .take()
            .is_some();
        if had_handle {
            ShellRuntime::dispatch_surface_lifecycle(
                cx,
                ContributionSurface::ControlCenter,
                false,
                340.,
                540.,
            );
            Self::restore_prior_focus(cx);
        }
        ShellRuntime::publish_status(cx);
    }

    pub(crate) fn register_overview_entity(cx: &mut App, entity: Entity<WorkspaceOverview>) {
        if cx.has_global::<ShellRuntime>() {
            cx.global_mut::<ShellRuntime>()
                .surface_manager_mut()
                .overview_entity = Some(entity);
        }
    }

    pub(crate) fn close_active_notification(cx: &mut App) {
        let entry = cx
            .global_mut::<ShellRuntime>()
            .surface_manager_mut()
            .notification
            .take();
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

    pub(crate) fn expire_notification(cx: &mut App, generation: u64) {
        let entry = cx
            .global_mut::<ShellRuntime>()
            .surface_manager_mut()
            .notification
            .take();
        let Some((current_generation, notification_id, handle)) = entry else {
            tracing::warn!(
                "[NOTIFTRACE] expire_notification(gen={generation}) no active entry; window NOT removed"
            );
            return;
        };
        if current_generation != generation {
            cx.global_mut::<ShellRuntime>()
                .surface_manager_mut()
                .notification = Some((current_generation, notification_id, handle));
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

    pub(crate) fn forget_notification(cx: &mut App, generation: u64) {
        let is_current = cx
            .global::<ShellRuntime>()
            .surface_manager()
            .notification
            .as_ref()
            .is_some_and(|(current_generation, _, _)| *current_generation == generation);
        if is_current
            && let Some((_, notification_id, _)) = cx
                .global_mut::<ShellRuntime>()
                .surface_manager_mut()
                .notification
                .take()
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
        if cx.has_global::<ShellRuntime>()
            && let Some(hub) = cx.global::<ShellRuntime>().service_hub()
        {
            hub.dismiss_notification(notification_id);
        }
    }

    fn expire_notification_id(cx: &App, notification_id: u32) {
        if cx.has_global::<ShellRuntime>()
            && let Some(hub) = cx.global::<ShellRuntime>().service_hub()
        {
            hub.expire_notification(notification_id);
        }
    }

    pub(crate) fn forget_osd(cx: &mut App) {
        if cx.has_global::<ShellRuntime>() {
            cx.global_mut::<ShellRuntime>().surface_manager_mut().osd = None;
        }
    }

    fn schedule_osd_dismiss(cx: &mut App, generation: u64) {
        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(2))
                .await;
            cx.update(|cx: &mut gpui::App| {
                if cx.has_global::<ShellRuntime>() {
                    let surface_manager = cx.global_mut::<ShellRuntime>().surface_manager_mut();
                    if let Some((current_gen, handle, _)) = &surface_manager.osd
                        && *current_gen == generation
                    {
                        let window_handle = *handle;
                        surface_manager.osd = None;
                        let _ = window_handle.update(cx, |_, window, _| window.remove_window());
                    }
                }
            });
        })
        .detach();
    }

    pub(crate) fn show_osd(cx: &mut App, kind: crate::osd::OsdKind) {
        let existing = cx
            .global_mut::<ShellRuntime>()
            .surface_manager_mut()
            .osd
            .take();
        if let Some((generation, window_handle, view_handle)) = existing {
            view_handle.update(cx, |view, cx| {
                view.kind = kind;
                cx.notify();
            });
            let next_gen = generation + 1;
            cx.global_mut::<ShellRuntime>().surface_manager_mut().osd =
                Some((next_gen, window_handle, view_handle));
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
            cx.global_mut::<ShellRuntime>().surface_manager_mut().osd =
                Some((1, window_handle, view_handle));
            Self::schedule_osd_dismiss(cx, 1);
        }
    }

    pub(crate) fn forget_bar(cx: &mut App) {
        let surface_manager = cx.global_mut::<ShellRuntime>().surface_manager_mut();
        surface_manager.bars.clear();
        surface_manager.bar_state = BarState::Hidden;
        ShellRuntime::publish_status(cx);
    }

    pub(crate) fn open_extension_panel(cx: &mut App, contribution: CanonicalId) {
        let existing = cx
            .global_mut::<ShellRuntime>()
            .surface_manager_mut()
            .extension_panel
            .take();
        if let Some((handle, current)) = existing {
            if current == contribution
                && handle
                    .update(cx, |_, window, _| window.activate_window())
                    .is_ok()
            {
                cx.global_mut::<ShellRuntime>()
                    .surface_manager_mut()
                    .extension_panel = Some((handle, current));
                return;
            }
            cx.global_mut::<ShellRuntime>()
                .extension_host_mut()
                .unmount_contribution(&current);
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
                cx.global_mut::<ShellRuntime>()
                    .extension_host_mut()
                    .mount_contribution(&contribution, 420., 600.);
                cx.global_mut::<ShellRuntime>()
                    .surface_manager_mut()
                    .extension_panel = Some((handle, contribution));
            }
            Err(error) => tracing::warn!(error = %error, "failed to open extension side panel"),
        }
    }

    /// Reconciles the extension desktop surfaces (bar widgets + config surfaces)
    /// against the desired instance layout.
    pub(crate) fn reconcile_extension_surfaces(cx: &mut App, outputs: &[OutputDescriptor]) {
        let config = ShellRuntime::active_config(cx);
        let mut instances = Vec::new();

        let bars = cx.global::<ShellRuntime>().surface_manager().bars.clone();
        for (display_id, (_, spec)) in &bars {
            for (section, widgets) in [
                ("start", &spec.config.widgets.start),
                ("center", &spec.config.widgets.center),
                ("end", &spec.config.widgets.end),
            ] {
                for (index, widget) in widgets.iter().enumerate() {
                    if let shilpo_config::BarWidget::Extension(contribution) = widget {
                        instances.push(ContributionInstance {
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
            instances.push(ContributionInstance {
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

        cx.global_mut::<ShellRuntime>()
            .extension_host_mut()
            .send_instance_reconciliation(instances);

        let stale = cx
            .global::<ShellRuntime>()
            .surface_manager()
            .stale_extension_surface_ids(&desired_windows);
        for id in stale {
            if let Some((handle, _)) = cx
                .global_mut::<ShellRuntime>()
                .surface_manager_mut()
                .remove_extension_surface(&id)
            {
                let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
            }
        }

        for (instance_id, spec) in desired_windows {
            if cx
                .global::<ShellRuntime>()
                .surface_manager()
                .has_extension_surface(&instance_id)
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
                    cx.global_mut::<ShellRuntime>()
                        .surface_manager_mut()
                        .store_extension_surface(instance_id, handle, spec);
                }
                Err(error) => tracing::warn!(
                    error = %error,
                    "failed to open extension desktop surface"
                ),
            }
        }
    }

    /// Reconciles the extension instances contributed through bar widgets.
    pub(crate) fn reconcile_bar_extension_instances(cx: &mut App) {
        let config = ShellRuntime::active_config(cx);
        let bars = cx.global::<ShellRuntime>().surface_manager().bars.clone();
        let mut instances = Vec::new();
        for (display_id, (_, spec)) in &bars {
            for (section, widgets) in [
                ("start", &spec.config.widgets.start),
                ("center", &spec.config.widgets.center),
                ("end", &spec.config.widgets.end),
            ] {
                for (index, widget) in widgets.iter().enumerate() {
                    if let shilpo_config::BarWidget::Extension(contribution) = widget {
                        instances.push(ContributionInstance {
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
        cx.global_mut::<ShellRuntime>()
            .extension_host_mut()
            .send_instance_reconciliation(instances);
    }

    /// Applies a theme daemon state change to the wallpaper path and refreshes
    /// every surface that renders the wallpaper.
    pub(crate) fn apply_theme_state(cx: &mut App, state: &DaemonState) {
        let (overview_entity, wallpaper_path, cc_handle, ov_handle) = {
            let runtime = cx.global_mut::<ShellRuntime>();
            if let Some(path) = state.wallpaper_path.clone().filter(|path| path.is_file()) {
                runtime.surface_manager_mut().current_wallpaper_path = Some(path);
            }
            (
                runtime.surface_manager().overview_entity(),
                runtime.surface_manager().current_wallpaper_path(),
                runtime.surface_manager().control_center,
                runtime.surface_manager().overview,
            )
        };

        if let Some(overview) = overview_entity {
            overview.update(cx, |view, cx| {
                view.update_wallpaper_path(wallpaper_path, cx);
            });
        }
        Self::refresh_bars(cx);
        if let Some(cc) = cc_handle {
            let _ = cc.update(cx, |_, _, cx| cx.notify());
        }
        if let Some(ov) = ov_handle {
            let _ = ov.update(cx, |_, _, cx| cx.notify());
        }
        cx.refresh_windows();
    }
}

impl ShellRuntime {
    pub fn sync_displays(cx: &mut App) {
        SurfaceManager::sync_displays(cx);
    }

    pub fn open_bar_with_spec(cx: &mut App, spec: BarSpec) -> bool {
        SurfaceManager::open_bar_with_spec(cx, spec)
    }

    pub fn open_bar(cx: &mut App, geometry: &BarGeometry, with_display_geometry: bool) -> bool {
        SurfaceManager::open_bar(cx, geometry, with_display_geometry)
    }

    pub fn reconcile_bars(cx: &mut App) {
        SurfaceManager::reconcile_bars(cx);
    }

    pub fn mark_bar_open_failed(cx: &mut App) {
        SurfaceManager::mark_bar_open_failed(cx);
    }

    pub fn toggle_bar(cx: &mut App) {
        SurfaceManager::toggle_bar(cx);
    }

    pub fn toggle_overview(cx: &mut App) {
        SurfaceManager::toggle_overview(cx);
    }

    pub fn close_overview(cx: &mut App) {
        SurfaceManager::close_overview(cx);
    }

    pub fn finish_overview_close(cx: &mut App, instance_id: u64, reason: OverviewCloseReason) {
        SurfaceManager::finish_overview_close(cx, instance_id, reason);
    }

    pub fn forget_overview(cx: &mut App, instance_id: u64) {
        SurfaceManager::forget_overview(cx, instance_id);
    }

    pub fn overview_focus_workspace(cx: &mut App, ws_id: u64) -> Result<(), ShellError> {
        SurfaceManager::overview_focus_workspace(cx, ws_id)
    }

    pub fn overview_move_window(
        cx: &mut App,
        window_id: u64,
        workspace_id: u64,
    ) -> Result<(), ShellError> {
        SurfaceManager::overview_move_window(cx, window_id, workspace_id)
    }

    pub fn overview_focus_window(cx: &mut App, window_id: u64) -> Result<(), ShellError> {
        SurfaceManager::overview_focus_window(cx, window_id)
    }

    pub fn overview_reduced_motion(cx: &App) -> bool {
        SurfaceManager::overview_reduced_motion(cx)
    }

    pub fn is_overview_open(cx: &App) -> bool {
        if cx.has_global::<Self>() {
            cx.global::<Self>().surface_manager().is_overview_open()
        } else {
            false
        }
    }

    pub fn is_control_center_open(cx: &App) -> bool {
        if cx.has_global::<Self>() {
            cx.global::<Self>()
                .surface_manager()
                .is_control_center_open()
        } else {
            false
        }
    }

    pub fn has_bars(cx: &App) -> bool {
        if cx.has_global::<Self>() {
            cx.global::<Self>().surface_manager().has_bars()
        } else {
            false
        }
    }

    pub fn overview_wallpaper_path(cx: &App) -> Option<PathBuf> {
        SurfaceManager::overview_wallpaper_path(cx)
    }

    pub fn overview_applications(cx: &App) -> Vec<shilpo_services::Application> {
        SurfaceManager::overview_applications(cx)
    }

    pub fn normalize_app_key(value: &str) -> String {
        SurfaceManager::normalize_app_key(value)
    }

    pub fn app_icon_index(cx: &App) -> HashMap<String, PathBuf> {
        SurfaceManager::app_icon_index(cx)
    }

    pub fn begin_overview_instance(cx: &mut App) -> u64 {
        cx.global_mut::<Self>()
            .surface_manager_mut()
            .begin_overview_instance()
    }

    pub fn open_or_focus_overview(cx: &mut App) {
        SurfaceManager::open_or_focus_overview(cx);
    }

    pub fn open_or_focus_overview_on_display(cx: &mut App, target_display_id: Option<DisplayId>) {
        SurfaceManager::open_or_focus_overview_on_display(cx, target_display_id);
    }

    pub fn open_or_focus_control_center(cx: &mut App) {
        SurfaceManager::open_or_focus_control_center(cx);
    }

    pub fn toggle_control_center(cx: &mut App) {
        SurfaceManager::toggle_control_center(cx);
    }

    pub fn close_control_center(cx: &mut App) {
        SurfaceManager::close_control_center(cx);
    }

    pub fn forget_control_center(cx: &mut App) {
        SurfaceManager::forget_control_center(cx);
    }

    pub fn register_overview_entity(cx: &mut App, entity: Entity<WorkspaceOverview>) {
        SurfaceManager::register_overview_entity(cx, entity);
    }

    pub fn active_notification_handle(
        cx: &App,
    ) -> Option<WindowHandle<crate::notification::NotificationToastView>> {
        if cx.has_global::<Self>() {
            cx.global::<Self>()
                .surface_manager()
                .active_notification_handle()
        } else {
            None
        }
    }

    pub fn register_notification(
        cx: &mut App,
        generation: u64,
        notification_id: u32,
        handle: WindowHandle<crate::notification::NotificationToastView>,
    ) {
        cx.global_mut::<Self>()
            .surface_manager_mut()
            .register_notification(generation, notification_id, handle);
    }

    pub fn close_active_notification(cx: &mut App) {
        SurfaceManager::close_active_notification(cx);
    }

    pub fn expire_notification(cx: &mut App, generation: u64) {
        SurfaceManager::expire_notification(cx, generation);
    }

    pub fn forget_notification(cx: &mut App, generation: u64) {
        SurfaceManager::forget_notification(cx, generation);
    }

    pub fn forget_osd(cx: &mut App) {
        SurfaceManager::forget_osd(cx);
    }

    pub fn show_osd(cx: &mut App, kind: crate::osd::OsdKind) {
        SurfaceManager::show_osd(cx, kind);
    }

    pub fn forget_bar(cx: &mut App) {
        SurfaceManager::forget_bar(cx);
    }

    pub fn open_extension_panel(cx: &mut App, contribution: CanonicalId) {
        SurfaceManager::open_extension_panel(cx, contribution);
    }

    pub fn compositor_snapshot(cx: &App) -> Arc<CompositorSnapshot> {
        if cx.has_global::<Self>() {
            cx.global::<Self>().surface_manager().latest_snapshot()
        } else {
            Arc::new(CompositorSnapshot::default())
        }
    }

    pub fn workspace_overview(cx: &App) -> Vec<shilpo_services::WorkspaceInfo> {
        Self::compositor_snapshot(cx).workspaces.clone()
    }

    pub fn reserve_notification_generation(cx: &mut App) -> u64 {
        cx.global_mut::<Self>()
            .surface_manager_mut()
            .reserve_notification_generation()
    }
}

fn readiness_for(
    connection: &shilpo_services::CompositorConnection,
    bar_state: &BarState,
) -> shilpo_services::ipc::ReadinessState {
    match connection {
        shilpo_services::CompositorConnection::Connecting => {
            shilpo_services::ipc::ReadinessState::Starting
        }
        shilpo_services::CompositorConnection::Ready => {
            if matches!(bar_state, BarState::Visible | BarState::Hidden) {
                shilpo_services::ipc::ReadinessState::Ready
            } else {
                shilpo_services::ipc::ReadinessState::Degraded
            }
        }
        shilpo_services::CompositorConnection::Reconnecting { .. } => {
            shilpo_services::ipc::ReadinessState::Degraded
        }
        shilpo_services::CompositorConnection::Stopped => {
            shilpo_services::ipc::ReadinessState::Failed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_surface_manager_initialization() {
        let snapshot = Arc::new(CompositorSnapshot::default());
        let manager = SurfaceManager::new(Some(PathBuf::from("/wallpaper.png")), snapshot);
        assert_eq!(manager.bar_state(), BarState::Starting);
        assert!(!manager.has_bars());
        assert!(!manager.is_overview_open());
        assert!(!manager.is_control_center_open());
        assert_eq!(
            manager.current_wallpaper_path(),
            Some(PathBuf::from("/wallpaper.png"))
        );
    }

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
            OverviewCloseReason::Cancel,
            Some(2),
            Some(1),
        ));
        assert!(should_restore_overview_prior_focus(
            OverviewCloseReason::Cancel,
            Some(2),
            Some(2),
        ));
        assert!(!should_restore_overview_prior_focus(
            OverviewCloseReason::Selection,
            Some(2),
            Some(2),
        ));
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
            SurfaceManager::output_name_for_bounds(bounds, &outputs),
            Some("HDMI-A-1".into())
        );
    }

    #[test]
    fn overview_instances_advance_monotonically() {
        let snapshot = Arc::new(CompositorSnapshot::default());
        let mut manager = SurfaceManager::new(None, snapshot);
        let first = manager.begin_overview_instance();
        let second = manager.begin_overview_instance();
        assert!(second > first);
    }

    #[test]
    fn notification_generation_reserves_incrementally() {
        let snapshot = Arc::new(CompositorSnapshot::default());
        let mut manager = SurfaceManager::new(None, snapshot);
        let first = manager.reserve_notification_generation();
        let second = manager.reserve_notification_generation();
        assert!(second > first);
    }

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
        let id = shilpo_ext_types::ExtensionId::new("org.shilpo.weather").unwrap();
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

    struct SurfaceManagerTestHarness {
        manager: SurfaceManager,
    }

    impl SurfaceManagerTestHarness {
        fn new_offline() -> Self {
            let snapshot = Arc::new(CompositorSnapshot::default());
            Self {
                manager: SurfaceManager::new(None, snapshot),
            }
        }
    }

    #[test]
    fn test_harness_surface_manager_readiness_and_extension_surfaces() {
        let mut harness = SurfaceManagerTestHarness::new_offline();
        assert_eq!(
            harness.manager.readiness(),
            shilpo_services::ipc::ReadinessState::Starting
        );

        harness.manager.set_bar_state(BarState::Visible);
        harness.manager.update_readiness();
        assert_eq!(
            harness.manager.readiness(),
            shilpo_services::ipc::ReadinessState::Starting
        );

        let ready_snapshot = CompositorSnapshot {
            connection: shilpo_services::CompositorConnection::Ready,
            ..Default::default()
        };
        harness
            .manager
            .set_latest_snapshot(Arc::new(ready_snapshot));
        harness.manager.update_readiness();
        assert_eq!(
            harness.manager.readiness(),
            shilpo_services::ipc::ReadinessState::Ready
        );

        assert!(!harness.manager.has_extension_surface("inst-1"));
        let desired = HashMap::new();
        assert!(
            harness
                .manager
                .stale_extension_surface_ids(&desired)
                .is_empty()
        );
    }
}
