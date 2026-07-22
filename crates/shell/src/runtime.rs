use gpui::{
    App, AppContext, Bounds, DisplayId, Focusable, Global, Pixels, Point, Subscription,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions,
    layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
    point, px, size,
};
use shilpo_services::{BarState, IpcRequest, IpcStatus, ShellIpcServer};

use crate::{
    bar::{BarView, geometry::BarGeometry},
    control_center::ControlCenterView,
    launcher::LauncherView,
};

pub struct ShellRuntime {
    ipc_server: ShellIpcServer,
    bar: Option<WindowHandle<BarView>>,
    bar_spec: Option<(BarGeometry, bool)>,
    bar_state: BarState,
    launcher: Option<WindowHandle<LauncherView>>,
    control_center: Option<WindowHandle<ControlCenterView>>,
    notification: Option<(
        u64,
        WindowHandle<crate::notification::NotificationToastView>,
    )>,
    notification_generation: u64,
    _window_closed: Option<Subscription>,
    _ipc_task: gpui::Task<()>,
}

impl Global for ShellRuntime {}

impl ShellRuntime {
    pub fn install(cx: &mut App, ipc_server: ShellIpcServer) {
        cx.set_global(Self {
            ipc_server,
            bar: None,
            bar_spec: None,
            bar_state: BarState::Starting,
            launcher: None,
            control_center: None,
            notification: None,
            notification_generation: 0,
            _window_closed: None,
            _ipc_task: cx.spawn(async |_| {}),
        });

        let subscription = cx.on_window_closed(|cx, window_id| {
            let runtime = cx.global_mut::<Self>();
            if runtime
                .bar
                .as_ref()
                .is_some_and(|handle| handle.window_id() == window_id)
            {
                runtime.bar = None;
                runtime.bar_state = BarState::Hidden;
            }
            if runtime
                .launcher
                .as_ref()
                .is_some_and(|handle| handle.window_id() == window_id)
            {
                runtime.launcher = None;
            }
            if runtime
                .control_center
                .as_ref()
                .is_some_and(|handle| handle.window_id() == window_id)
            {
                runtime.control_center = None;
            }
            if runtime
                .notification
                .as_ref()
                .is_some_and(|(_, handle)| handle.window_id() == window_id)
            {
                runtime.notification = None;
            }
            runtime.publish_status();
        });
        cx.global_mut::<Self>()._window_closed = Some(subscription);

        let task = cx.spawn(async move |cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
                cx.update(Self::drain_ipc);
            }
        });
        cx.global_mut::<Self>()._ipc_task = task;
        cx.global::<Self>().publish_status();
    }

    fn publish_status(&self) {
        self.ipc_server.update_status(IpcStatus {
            running: true,
            bar: self.bar_state.clone(),
            launcher_visible: self.launcher.is_some(),
            control_center_visible: self.control_center.is_some(),
        });
    }

    pub fn open_bar(cx: &mut App, geometry: &BarGeometry, with_display_geometry: bool) -> bool {
        let options = bar_window_options(geometry, with_display_geometry);
        let result = cx.open_window(options, BarView::view);
        let runtime = cx.global_mut::<Self>();
        match result {
            Ok(handle) => {
                runtime.bar = Some(handle);
                runtime.bar_spec = Some((geometry.clone(), with_display_geometry));
                runtime.bar_state = BarState::Visible;
                runtime.publish_status();
                true
            }
            Err(error) => {
                tracing::error!(error = %error, "failed to open bar window");
                runtime.bar_state = BarState::OpenFailed;
                runtime.publish_status();
                false
            }
        }
    }

    pub fn mark_bar_open_failed(cx: &mut App) {
        let runtime = cx.global_mut::<Self>();
        runtime.bar_state = BarState::OpenFailed;
        runtime.publish_status();
    }

    pub fn toggle_bar(cx: &mut App) {
        let (handle, spec) = {
            let runtime = cx.global_mut::<Self>();
            (runtime.bar.take(), runtime.bar_spec.clone())
        };
        if let Some(handle) = handle {
            let removed = cx
                .update_window(*handle, |_, window, _| window.remove_window())
                .is_ok();
            let runtime = cx.global_mut::<Self>();
            if removed {
                runtime.bar_state = BarState::Hidden;
            } else {
                runtime.bar_state = BarState::OpenFailed;
            }
            runtime.publish_status();
            return;
        }
        if let Some((geometry, with_display_geometry)) = spec {
            Self::open_bar(cx, &geometry, with_display_geometry);
        } else {
            Self::mark_bar_open_failed(cx);
            tracing::warn!("cannot toggle bar: no valid reopen geometry");
        }
    }

    pub fn open_or_focus_launcher(cx: &mut App) {
        Self::close_control_center(cx);
        let handle = cx.global_mut::<Self>().launcher.take();
        if let Some(handle) = handle
            && handle
                .update(cx, |view, window, cx| {
                    window.activate_window();
                    view.focus_handle(cx).focus(window, cx);
                })
                .is_ok()
        {
            cx.global_mut::<Self>().launcher = Some(handle);
            cx.global::<Self>().publish_status();
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
        let launcher_size = size(px(640.), px(480.));
        let origin = point(
            display_bounds.origin.x + (display_bounds.size.width - launcher_size.width) / 2.0,
            display_bounds.origin.y + (display_bounds.size.height - launcher_size.height) / 2.0,
        );
        let options = overlay_options(
            "shilpo-launcher",
            "launcher",
            launcher_size,
            origin,
            display_id,
        );
        match cx.open_window(options, LauncherView::view) {
            Ok(handle) => cx.global_mut::<Self>().launcher = Some(handle),
            Err(error) => {
                tracing::error!(error = %error, overlay = "launcher", "failed to open overlay window")
            }
        }
        cx.global::<Self>().publish_status();
    }

    pub fn toggle_launcher(cx: &mut App) {
        if cx.global::<Self>().launcher.is_some() {
            Self::close_launcher(cx);
        } else {
            Self::open_or_focus_launcher(cx);
        }
    }

    pub fn close_launcher(cx: &mut App) {
        let handle = cx.global_mut::<Self>().launcher.take();
        let Some(handle) = handle else { return };
        // Registry entry is invalidated above. A close racing with this call can
        // leave handle stale; update_window failure is expected in that case.
        let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
        cx.global::<Self>().publish_status();
    }

    pub fn forget_launcher(cx: &mut App) {
        cx.global_mut::<Self>().launcher = None;
        cx.global::<Self>().publish_status();
    }

    pub fn open_or_focus_control_center(cx: &mut App) {
        Self::close_launcher(cx);
        let handle = cx.global_mut::<Self>().control_center.take();
        if let Some(handle) = handle
            && handle
                .update(cx, |view, window, cx| {
                    window.activate_window();
                    view.focus_handle(cx).focus(window, cx);
                })
                .is_ok()
        {
            cx.global_mut::<Self>().control_center = Some(handle);
            cx.global::<Self>().publish_status();
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
        let cc_size = size(px(340.), px(380.));
        let origin = point(
            display_bounds.origin.x + (display_bounds.size.width - px(360.)),
            display_bounds.origin.y + px(54.),
        );
        let options = overlay_options(
            "shilpo-control-center",
            "control-center",
            cc_size,
            origin,
            display_id,
        );
        match cx.open_window(options, ControlCenterView::view) {
            Ok(handle) => cx.global_mut::<Self>().control_center = Some(handle),
            Err(error) => {
                tracing::error!(error = %error, overlay = "control-center", "failed to open overlay window")
            }
        }
        cx.global::<Self>().publish_status();
    }

    pub fn toggle_control_center(cx: &mut App) {
        if cx.global::<Self>().control_center.is_some() {
            Self::close_control_center(cx);
        } else {
            Self::open_or_focus_control_center(cx);
        }
    }

    pub fn close_control_center(cx: &mut App) {
        let handle = cx.global_mut::<Self>().control_center.take();
        let Some(handle) = handle else { return };
        // Registry entry is invalidated above. A close racing with this call can
        // leave handle stale; update_window failure is expected in that case.
        let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
        cx.global::<Self>().publish_status();
    }

    pub fn forget_control_center(cx: &mut App) {
        cx.global_mut::<Self>().control_center = None;
        cx.global::<Self>().publish_status();
    }

    pub fn register_notification(
        cx: &mut App,
        handle: WindowHandle<crate::notification::NotificationToastView>,
    ) -> u64 {
        let runtime = cx.global_mut::<Self>();
        runtime.notification_generation = runtime.notification_generation.wrapping_add(1);
        let generation = runtime.notification_generation;
        runtime.notification = Some((generation, handle));
        generation
    }

    pub fn expire_notification(cx: &mut App, generation: u64) {
        let entry = cx.global_mut::<Self>().notification.take();
        let Some((current_generation, handle)) = entry else {
            return;
        };
        if current_generation != generation {
            cx.global_mut::<Self>().notification = Some((current_generation, handle));
            return;
        }
        // Generation check above makes delayed expiry harmless after replacement.
        // Entry is taken before close so stale expiry cannot retain registry state.
        let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
    }

    pub fn forget_notification(cx: &mut App) {
        cx.global_mut::<Self>().notification = None;
    }

    pub fn forget_bar(cx: &mut App) {
        let runtime = cx.global_mut::<Self>();
        runtime.bar = None;
        runtime.bar_state = BarState::Hidden;
        runtime.publish_status();
    }

    fn enqueue_worker(cx: &mut App, request: IpcRequest) {
        let handle = cx.global_mut::<Self>().bar.take();
        let Some(handle) = handle else {
            tracing::warn!(?request, "IPC worker request dropped: bar unavailable");
            return;
        };
        match handle.update(cx, |bar, _, _| bar.enqueue_request(request)) {
            Ok(Ok(())) => cx.global_mut::<Self>().bar = Some(handle),
            Ok(Err(error)) => {
                tracing::warn!(error = %error, "IPC worker request rejected");
                cx.global_mut::<Self>().bar = Some(handle);
            }
            Err(error) => {
                tracing::warn!(error = %error, "IPC worker request dropped: bar handle unavailable")
            }
        }
    }

    fn drain_ipc(cx: &mut App) {
        let requests = cx.global_mut::<Self>().ipc_server.pop_pending_requests();
        for request in requests {
            match request {
                IpcRequest::ToggleBar => Self::toggle_bar(cx),
                IpcRequest::ToggleLauncher => Self::toggle_launcher(cx),
                IpcRequest::ToggleControlCenter => Self::toggle_control_center(cx),
                IpcRequest::SetTheme {
                    source_argb,
                    is_dark,
                } => {
                    let mode = if is_dark {
                        shilpo_ui::ThemeMode::Dark
                    } else {
                        shilpo_ui::ThemeMode::Light
                    };
                    shilpo_ui::Theme::global_mut(cx).set_source_argb(source_argb);
                    shilpo_ui::Theme::global_mut(cx).set_mode(mode);
                }
                IpcRequest::Quit => {
                    Self::shutdown(cx);
                    return;
                }
                request @ (IpcRequest::FocusWorkspace(_) | IpcRequest::ReloadConfig) => {
                    Self::enqueue_worker(cx, request);
                }
                IpcRequest::GetStatus => {}
            }
        }
        cx.global::<Self>().publish_status();
    }

    pub fn shutdown(cx: &mut App) {
        if !cx.has_global::<Self>() {
            return;
        }
        let (bar, launcher, control_center, notification) = {
            let runtime = cx.global_mut::<Self>();
            (
                runtime.bar.take(),
                runtime.launcher.take(),
                runtime.control_center.take(),
                runtime.notification.take(),
            )
        };
        if let Some(handle) = bar {
            let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
        }
        if let Some(handle) = launcher {
            let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
        }
        if let Some(handle) = control_center {
            let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
        }
        if let Some((_, handle)) = notification {
            let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
        }
        let runtime = cx.global_mut::<Self>();
        runtime.bar_state = BarState::Hidden;
        runtime.publish_status();
        cx.quit();
    }
}

fn bar_window_options(geometry: &BarGeometry, with_display_geometry: bool) -> WindowOptions {
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
    window_size: gpui::Size<Pixels>,
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
            layer: Layer::Overlay,
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            ..Default::default()
        }),
        ..Default::default()
    }
}
