use crate::actions::ActionInvocation;
use crate::bar::service_worker::{self, ConfigUpdate, WorkerCommand, WorkerUpdate};
use crate::runtime::ShellRuntime;
use gpui::{
    App, AppContext, Context, Entity, IntoElement, ParentElement, Path, PathBuilder, Pixels, Point,
    Render, Styled, Window, div, prelude::*, px,
};
use shilpo_config::{BarPosition, BarWidget, ShellConfig};
use shilpo_services::{AudioInfo, BatteryInfo, NetworkInfo, Notification};
use shilpo_ui::{ActiveTheme, h_flex, v_flex};
use std::time::Duration;

use super::geometry::HUG_CORNER_RADIUS;

fn build_hug_corner(
    start: Point<Pixels>,
    edge_end: Point<Pixels>,
    arc_end: Point<Pixels>,
    control_a: Point<Pixels>,
    control_b: Point<Pixels>,
) -> Option<Path<Pixels>> {
    let mut builder = PathBuilder::fill();
    builder.move_to(start);
    builder.line_to(edge_end);
    builder.cubic_bezier_to(arc_end, control_a, control_b);
    builder.close();
    builder.build().ok()
}

pub fn parse_hex_color(hex: &str) -> Option<u32> {
    let clean = hex.trim_start_matches('#');
    if clean.len() == 6 {
        let rgb = u32::from_str_radix(clean, 16).ok()?;
        Some(0xff000000 | rgb)
    } else if clean.len() == 8 {
        u32::from_str_radix(clean, 16).ok()
    } else {
        None
    }
}

pub fn apply_config_theme(_config: &ShellConfig, _window: Option<&mut Window>, cx: &mut App) {
    if let Some(state) = shilpo_theme::read_state_snapshot() {
        shilpo_ui::Theme::global_mut(cx).apply_state(&state);
    }
}

/// Status Bar GPUI View (Multi-Capsule Segmented Bar with IPC integration).
pub struct BarView {
    pub config: ShellConfig,
    service_commands: service_worker::CommandSender,
    battery: BatteryInfo,
    audio: AudioInfo,
    network: NetworkInfo,
    #[allow(dead_code)]
    app_id: String,
    #[allow(dead_code)]
    active_title: String,
    #[allow(dead_code)]
    media_track: String,
    datetime_str: String,
    extension_instance_prefix: Option<String>,
    last_error: Option<String>,
    last_service_update: std::time::Instant,
}

impl BarView {
    pub fn is_stale(&self) -> bool {
        self.last_service_update.elapsed() > std::time::Duration::from_secs(30)
    }

    pub fn enqueue_request(
        &self,
        request: shilpo_services::IpcRequest,
        cx: &mut App,
    ) -> Result<(), String> {
        match request {
            shilpo_services::IpcRequest::Compositor(
                shilpo_services::CompositorCommand::FocusWorkspace(id),
            ) => ShellRuntime::dispatch_action(cx, ActionInvocation::FocusWorkspace(id))
                .map_err(|error| error.to_string()),
            shilpo_services::IpcRequest::ReloadConfig => service_worker::try_send_command(
                &self.service_commands,
                WorkerCommand::ReloadConfig,
            )
            .map_err(|e| format!("Failed to send worker command: {}", e)),
            _ => Err("Unsupported bar command".into()),
        }
    }

    pub fn new_with_config(
        window: &mut Window,
        cx: &mut Context<Self>,
        config: ShellConfig,
    ) -> Self {
        let battery = BatteryInfo::default();
        let audio = AudioInfo::default();
        let network = NetworkInfo::default();

        window.on_window_should_close(cx, |_, cx| {
            ShellRuntime::forget_bar(cx);
            true
        });

        // Dynamic theme synchronization with OS appearance and config
        apply_config_theme(&config, Some(window), cx);

        let service_commands = ShellRuntime::service_commands(cx).unwrap_or_else(|| {
            let (_, _, tx, _) = service_worker::channels();
            tx
        });

        Self {
            config,
            service_commands,
            battery,
            audio,
            network,
            app_id: "shilpo.shell".into(),
            active_title: "Shilpo Shell".into(),
            media_track: "KK - Police ke hathiyar".into(),
            datetime_str: "17:53 · Tue, 21/07".into(),
            extension_instance_prefix: None,
            last_error: None,
            last_service_update: std::time::Instant::now(),
        }
    }

    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new_with_config(window, cx, ShellConfig::default())
    }

    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    pub fn view_with_config(
        window: &mut Window,
        cx: &mut App,
        config: ShellConfig,
    ) -> Entity<Self> {
        cx.new(|cx| Self::new_with_config(window, cx, config))
    }

    pub fn view_with_config_on_display(
        window: &mut Window,
        cx: &mut App,
        config: ShellConfig,
        display_id: gpui::DisplayId,
    ) -> Entity<Self> {
        cx.new(|cx| {
            let mut view = Self::new_with_config(window, cx, config);
            view.extension_instance_prefix = Some(format!("bar:{display_id:?}"));
            view
        })
    }

    pub fn apply_worker_update(&mut self, update: &WorkerUpdate, cx: &mut Context<Self>) {
        self.last_service_update = std::time::Instant::now();
        let mut changed = false;
        match update {
            WorkerUpdate::Battery(value) if &self.battery != value => {
                self.battery = value.clone();
                changed = true;
            }
            WorkerUpdate::Audio(value) if &self.audio != value => {
                let show_osd = self.audio.available
                    && value.available
                    && (self.audio.volume != value.volume || self.audio.is_muted != value.is_muted);
                self.audio = value.clone();
                if show_osd {
                    ShellRuntime::show_osd(
                        cx,
                        crate::osd::OsdKind::Volume {
                            level: value.volume as u32,
                            muted: value.is_muted,
                        },
                    );
                }
                changed = true;
            }
            WorkerUpdate::Network(value) if &self.network != value => {
                self.network = value.clone();
                changed = true;
            }
            WorkerUpdate::Config(ConfigUpdate::Loaded(config)) => {
                self.config = (**config).clone();
                self.last_error = None;
                apply_config_theme(&self.config, None, cx);
                changed = true;
            }
            WorkerUpdate::Config(ConfigUpdate::Failed(error)) => {
                tracing::error!(error = %error, "config reload failed");
                self.last_error = Some(error.clone());
                open_notification_toast(cx, Notification::new("Configuration Warning", error));
                changed = true;
            }
            _ => {}
        }

        let now = chrono::Local::now();
        let updated_dt = now.format("%H:%M · %a, %d/%m").to_string();

        if self.datetime_str != updated_dt {
            self.datetime_str = updated_dt;
            changed = true;
        }

        if changed {
            cx.notify();
        }
    }
}

impl BarView {
    fn build_section(
        &self,
        section_name: &str,
        widget_names: &[BarWidget],
        side: bool,
        is_floating: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> gpui::AnyElement {
        use shilpo_config::{BarWidget, BuiltinBarWidget};
        let mut elements: Vec<gpui::AnyElement> = Vec::new();

        for (index, name) in widget_names.iter().enumerate() {
            match name {
                BarWidget::Builtin(BuiltinBarWidget::Workspaces) => {
                    let snapshot = ShellRuntime::compositor_snapshot(cx);
                    elements.push(
                        super::widgets::WorkspacesWidget::new(
                            "workspaces",
                            snapshot.workspaces.clone(),
                            snapshot.connection.clone(),
                        )
                        .into_any_element(),
                    );
                }
                BarWidget::Extension(ext_ref) => {
                    if let Some(tree) = ShellRuntime::extension_view(cx, ext_ref) {
                        let instance_id = self
                            .extension_instance_prefix
                            .as_ref()
                            .map(|prefix| format!("{prefix}:{section_name}:{index}"));
                        elements.push(super::ext_view_adapter::render_ext_view_tree(
                            ext_ref,
                            instance_id.as_deref(),
                            &tree,
                            window,
                            cx,
                        ));
                    }
                }
                _ => {}
            }
        }

        if elements.is_empty() {
            return div().into_any_element();
        }

        let flex = if side { v_flex() } else { h_flex() };
        let section = flex
            .gap(px(self.config.bar.widget_spacing as f32))
            .items_center()
            .children(elements);

        if is_floating {
            div()
                .p_1()
                .px_2()
                .rounded_full()
                .bg(cx.theme().surface_container_high.opacity(0.92))
                .border_1()
                .border_color(cx.theme().outline_variant.opacity(0.3))
                .shadow_md()
                .child(section)
                .into_any_element()
        } else {
            section.into_any_element()
        }
    }

    fn render_hug_corners(
        &self,
        position: BarPosition,
        radius: gpui::Pixels,
        bg_color: gpui::Hsla,
    ) -> impl IntoElement {
        use gpui::{canvas, point};

        let bar_height = px(self.config.bar.height as f32);

        canvas(
            move |_, _, _| {},
            move |bounds, _, window, _| {
                let r = radius.as_f32();
                let k = r * 0.552_284_8;

                match position {
                    BarPosition::Top => {
                        let y0 = bounds.origin.y.as_f32() + bar_height.as_f32();

                        let x0 = bounds.origin.x.as_f32();
                        if let Some(path) = build_hug_corner(
                            point(px(x0), px(y0)),
                            point(px(x0), px(y0 + r)),
                            point(px(x0 + r), px(y0)),
                            point(px(x0), px(y0 + r - k)),
                            point(px(x0 + r - k), px(y0)),
                        ) {
                            window.paint_path(path, bg_color);
                        }

                        let x1 = bounds.origin.x.as_f32() + bounds.size.width.as_f32();
                        if let Some(path) = build_hug_corner(
                            point(px(x1), px(y0)),
                            point(px(x1), px(y0 + r)),
                            point(px(x1 - r), px(y0)),
                            point(px(x1), px(y0 + r - k)),
                            point(px(x1 - r + k), px(y0)),
                        ) {
                            window.paint_path(path, bg_color);
                        }
                    }
                    BarPosition::Bottom => {
                        let y0 = bounds.origin.y.as_f32() + bounds.size.height.as_f32()
                            - bar_height.as_f32();

                        let x0 = bounds.origin.x.as_f32();
                        if let Some(path) = build_hug_corner(
                            point(px(x0), px(y0)),
                            point(px(x0), px(y0 - r)),
                            point(px(x0 + r), px(y0)),
                            point(px(x0), px(y0 - r + k)),
                            point(px(x0 + r - k), px(y0)),
                        ) {
                            window.paint_path(path, bg_color);
                        }

                        let x1 = bounds.origin.x.as_f32() + bounds.size.width.as_f32();
                        if let Some(path) = build_hug_corner(
                            point(px(x1), px(y0)),
                            point(px(x1), px(y0 - r)),
                            point(px(x1 - r), px(y0)),
                            point(px(x1), px(y0 - r + k)),
                            point(px(x1 - r + k), px(y0)),
                        ) {
                            window.paint_path(path, bg_color);
                        }
                    }
                    BarPosition::Left => {
                        let x0 = bounds.origin.x.as_f32() + bar_height.as_f32();
                        let y0 = bounds.origin.y.as_f32();
                        let y1 = bounds.origin.y.as_f32() + bounds.size.height.as_f32();

                        if let Some(path) = build_hug_corner(
                            point(px(x0), px(y0)),
                            point(px(x0 + r), px(y0)),
                            point(px(x0), px(y0 + r)),
                            point(px(x0 + r - k), px(y0)),
                            point(px(x0), px(y0 + r - k)),
                        ) {
                            window.paint_path(path, bg_color);
                        }

                        if let Some(path) = build_hug_corner(
                            point(px(x0), px(y1)),
                            point(px(x0 + r), px(y1)),
                            point(px(x0), px(y1 - r)),
                            point(px(x0 + r - k), px(y1)),
                            point(px(x0), px(y1 - r + k)),
                        ) {
                            window.paint_path(path, bg_color);
                        }
                    }
                    BarPosition::Right => {
                        let x0 = bounds.origin.x.as_f32() + bounds.size.width.as_f32()
                            - bar_height.as_f32();
                        let y0 = bounds.origin.y.as_f32();
                        let y1 = bounds.origin.y.as_f32() + bounds.size.height.as_f32();

                        if let Some(path) = build_hug_corner(
                            point(px(x0), px(y0)),
                            point(px(x0 - r), px(y0)),
                            point(px(x0), px(y0 + r)),
                            point(px(x0 - r + k), px(y0)),
                            point(px(x0), px(y0 + r - k)),
                        ) {
                            window.paint_path(path, bg_color);
                        }

                        if let Some(path) = build_hug_corner(
                            point(px(x0), px(y1)),
                            point(px(x0 - r), px(y1)),
                            point(px(x0), px(y1 - r)),
                            point(px(x0 - r + k), px(y1)),
                            point(px(x0), px(y1 - r + k)),
                        ) {
                            window.paint_path(path, bg_color);
                        }
                    }
                }
            },
        )
        .size_full()
        .absolute()
        .inset_0()
    }
}

impl Render for BarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use shilpo_config::BarStyle;

        let style = self.config.bar.style;
        let side = matches!(
            self.config.bar.position,
            BarPosition::Left | BarPosition::Right
        );
        let opacity = self.config.bar.opacity.clamp(0.0, 1.0);
        let compositor_stale = !ShellRuntime::compositor_snapshot(cx).connection.is_ready();
        let bg_color = cx
            .theme()
            .surface_container_high
            .opacity(if compositor_stale {
                opacity * 0.65
            } else {
                opacity
            });
        let widgets = self.config.bar.widgets.clone();
        let is_floating = style == BarStyle::Float;
        let start = self.build_section("start", &widgets.start, side, is_floating, window, cx);
        let center = self.build_section("center", &widgets.center, side, is_floating, window, cx);
        let end = self.build_section("end", &widgets.end, side, is_floating, window, cx);

        let bar_container = div()
            .when(side, |this| {
                this.w(px(self.config.bar.height as f32))
                    .h_full()
                    .flex_col()
            })
            .when(!side, |this| {
                this.w_full()
                    .h(px(self.config.bar.height as f32))
                    .flex_row()
            })
            .flex()
            .items_center()
            .justify_between()
            .bg(bg_color)
            .child(start)
            .child(center)
            .child(end);

        match style {
            BarStyle::Hug => {
                let hug_corners = self.render_hug_corners(
                    self.config.bar.position,
                    px(HUG_CORNER_RADIUS),
                    bg_color,
                );
                let main_bar = bar_container;
                let aligned_bar = match self.config.bar.position {
                    BarPosition::Top => div().absolute().top_0().w_full().child(main_bar),
                    BarPosition::Bottom => div().absolute().bottom_0().w_full().child(main_bar),
                    BarPosition::Left => div().absolute().left_0().h_full().child(main_bar),
                    BarPosition::Right => div().absolute().right_0().h_full().child(main_bar),
                };

                div()
                    .relative()
                    .size_full()
                    .child(aligned_bar)
                    .child(hug_corners)
                    .into_any_element()
            }
            BarStyle::Float => bar_container
                .px(px(self.config.bar.margin.horizontal as f32))
                .py(px(self.config.bar.margin.vertical as f32))
                .rounded_2xl()
                .shadow_md()
                .into_any_element(),
            BarStyle::Rect => bar_container.rounded_none().shadow_sm().into_any_element(),
        }
    }
}

fn notification_timeout(notification: &Notification) -> Option<Duration> {
    (notification.expire_timeout_ms > 0)
        .then(|| Duration::from_millis(notification.expire_timeout_ms as u64))
}

pub fn open_notification_toast(cx: &mut App, notification: Notification) {
    use crate::notification::NotificationToastView;
    use gpui::{
        Bounds, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
        layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
        point, px, size,
    };

    let timeout = notification_timeout(&notification);
    let notification_id = notification.id;

    let (display_bounds, display_id) = if let Some(display) = cx.primary_display() {
        (display.bounds(), Some(display.id()))
    } else {
        (
            Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.))),
            None,
        )
    };
    let window_size = size(px(320.), px(80.));
    let origin = point(
        display_bounds.origin.x + (display_bounds.size.width - px(340.)),
        display_bounds.origin.y + px(54.),
    );
    let options = WindowOptions {
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin,
            size: window_size,
        })),
        display_id,
        app_id: Some("shilpo-notification".to_string()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "notification".to_string(),
            layer: Layer::Overlay,
            anchor: Anchor::TOP | Anchor::RIGHT,
            keyboard_interactivity: KeyboardInteractivity::None,
            ..Default::default()
        }),
        ..Default::default()
    };

    let generation = ShellRuntime::reserve_notification_generation(cx);
    if let Ok(handle) = cx.open_window(options, move |window, cx| {
        NotificationToastView::view(notification.clone(), generation, window, cx)
    }) {
        ShellRuntime::register_notification(cx, generation, notification_id, handle);
        if let Some(timeout) = timeout {
            cx.spawn(async move |cx| {
                cx.background_executor().timer(timeout).await;
                cx.update(|cx| ShellRuntime::expire_notification(cx, generation));
            })
            .detach();
        }
    }
}

#[cfg(test)]
mod notification_tests {
    use super::*;

    #[test]
    fn zero_timeout_never_schedules_expiry() {
        let mut notification = Notification::new("Critical", "Keep visible");
        notification.expire_timeout_ms = 0;

        assert_eq!(notification_timeout(&notification), None);
    }
}

#[cfg(test)]
mod hug_corner_tests {
    use super::*;
    use gpui::point;

    #[test]
    fn fills_the_complete_quarter_circle_bounds() {
        let radius = HUG_CORNER_RADIUS;
        let control_offset = radius * 0.552_284_8;
        let path = build_hug_corner(
            point(px(0.0), px(0.0)),
            point(px(0.0), px(radius)),
            point(px(radius), px(0.0)),
            point(px(0.0), px(radius - control_offset)),
            point(px(radius - control_offset), px(0.0)),
        )
        .expect("valid Hug corner path");

        assert_eq!(path.bounds.origin, point(px(0.0), px(0.0)));
        assert_eq!(path.bounds.size.width, px(radius));
        assert_eq!(path.bounds.size.height, px(radius));
    }
}
