use gpui::{
    App, Bounds, Focusable, Global, Pixels, Point, Subscription, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowKind, WindowOptions,
    layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
    point, px, size,
};

use crate::{control_center::ControlCenterView, launcher::LauncherView};

pub struct ShellRuntime {
    launcher: Option<WindowHandle<LauncherView>>,
    control_center: Option<WindowHandle<ControlCenterView>>,
    _window_closed: Option<Subscription>,
}

impl Global for ShellRuntime {}

impl ShellRuntime {
    pub fn install(cx: &mut App) {
        cx.set_global(Self {
            launcher: None,
            control_center: None,
            _window_closed: None,
        });
        let subscription = cx.on_window_closed(|cx, window_id| {
            let runtime = cx.global_mut::<Self>();
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
        });
        cx.global_mut::<Self>()._window_closed = Some(subscription);
    }

    pub fn open_or_focus_launcher(cx: &mut App) {
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
            return;
        }
        let options = overlay_options(
            "shilpo-launcher",
            "launcher",
            size(px(640.), px(480.)),
            point(px(0.), px(0.)),
        );
        match cx.open_window(options, LauncherView::view) {
            Ok(handle) => cx.global_mut::<Self>().launcher = Some(handle),
            Err(error) => {
                tracing::error!(error = %error, overlay = "launcher", "failed to open overlay window")
            }
        }
    }

    pub fn toggle_launcher(cx: &mut App) {
        let handle = cx.global_mut::<Self>().launcher.take();
        if let Some(handle) = handle
            && handle
                .update(cx, |_, window, _| window.remove_window())
                .is_ok()
        {
            return;
        }
        Self::open_or_focus_launcher(cx);
    }

    pub fn open_or_focus_control_center(cx: &mut App) {
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
            return;
        }
        let options = overlay_options(
            "shilpo-control-center",
            "control-center",
            size(px(340.), px(380.)),
            point(px(1920. - 360.), px(54.)),
        );
        match cx.open_window(options, ControlCenterView::view) {
            Ok(handle) => cx.global_mut::<Self>().control_center = Some(handle),
            Err(error) => {
                tracing::error!(error = %error, overlay = "control-center", "failed to open overlay window")
            }
        }
    }

    pub fn toggle_control_center(cx: &mut App) {
        let handle = cx.global_mut::<Self>().control_center.take();
        if let Some(handle) = handle
            && handle
                .update(cx, |_, window, _| window.remove_window())
                .is_ok()
        {
            return;
        }
        Self::open_or_focus_control_center(cx);
    }
}

fn overlay_options(
    app_id: &str,
    namespace: &str,
    window_size: gpui::Size<Pixels>,
    origin: Point<Pixels>,
) -> WindowOptions {
    WindowOptions {
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin,
            size: window_size,
        })),
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
