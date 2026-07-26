use crate::bar::service_worker::{self, ConfigUpdate, WorkerCommand, WorkerUpdate};
use crate::runtime::ShellRuntime;
use gpui::{
    App, AppContext, Context, Entity, IntoElement, ParentElement, Path, PathBuilder, Pixels, Point,
    Render, Styled, Window, div, prelude::*, px,
};
use shilpo_config::{BarPosition, BarWidget, BuiltinBarWidget, ShellConfig};
use shilpo_services::{AudioInfo, BatteryInfo, NetworkInfo, NiriWorkspaceInfo, Notification};
use shilpo_ui::{ActiveTheme, h_flex, v_flex};
use std::time::Duration;

use super::geometry::HUG_CORNER_RADIUS;
use super::widgets::{
    ClockBatteryCapsule, PerfMediaCapsule, StatusTogglesCapsule, WindowInfoCapsule,
    WorkspacesWidget,
};

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

pub fn apply_config_theme(config: &ShellConfig, window: Option<&mut Window>, cx: &mut App) {
    if let Some(argb) = parse_hex_color(&config.theme.accent) {
        let theme = shilpo_ui::Theme::global_mut(cx);
        theme.set_source_argb(argb);
    }
    let ui_mode = match config.theme.mode {
        shilpo_config::ThemeMode::Dark => shilpo_ui::ThemeMode::Dark,
        shilpo_config::ThemeMode::Light => shilpo_ui::ThemeMode::Light,
        shilpo_config::ThemeMode::Auto => shilpo_ui::ThemeMode::System,
    };
    shilpo_ui::Theme::change(ui_mode, window, cx);
}

/// Status Bar GPUI View (Multi-Capsule Segmented Bar with IPC integration).
pub struct BarView {
    pub config: ShellConfig,
    service_commands: service_worker::CommandSender,
    workspaces: Vec<NiriWorkspaceInfo>,
    battery: BatteryInfo,
    audio: AudioInfo,
    network: NetworkInfo,
    app_id: String,
    active_title: String,
    #[allow(dead_code)]
    media_track: String,
    datetime_str: String,
    last_error: Option<String>,
    last_service_update: std::time::Instant,
}

impl BarView {
    pub fn is_stale(&self) -> bool {
        self.last_service_update.elapsed() > std::time::Duration::from_secs(30)
    }

    pub fn enqueue_request(&self, request: shilpo_services::IpcRequest) -> Result<(), String> {
        let command = match request {
            shilpo_services::IpcRequest::FocusWorkspace(id) => WorkerCommand::FocusWorkspace(id),
            shilpo_services::IpcRequest::ReloadConfig => WorkerCommand::ReloadConfig,
            _ => return Err("Unsupported bar command".into()),
        };
        service_worker::try_send_command(&self.service_commands, command)
            .map_err(|e| format!("Failed to send worker command: {}", e))
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
        cx.observe_window_appearance(window, |this, window, cx| {
            apply_config_theme(&this.config, Some(window), cx);
            window.refresh();
        })
        .detach();

        let fallback_ws = vec![
            NiriWorkspaceInfo {
                id: 1,
                name: Some("1".into()),
                idx: 1,
                is_active: true,
                is_focused: true,
            },
            NiriWorkspaceInfo {
                id: 2,
                name: Some("2".into()),
                idx: 2,
                is_active: false,
                is_focused: false,
            },
            NiriWorkspaceInfo {
                id: 3,
                name: Some("3".into()),
                idx: 3,
                is_active: false,
                is_focused: false,
            },
        ];

        let service_commands = ShellRuntime::service_commands(cx).unwrap_or_else(|| {
            let (_, _, tx, _) = service_worker::channels();
            tx
        });

        Self {
            config,
            service_commands,
            workspaces: fallback_ws,
            battery,
            audio,
            network,
            app_id: "shilpo.shell".into(),
            active_title: "Shilpo Shell".into(),
            media_track: "KK - Police ke hathiyar".into(),
            datetime_str: "17:53 · Tue, 21/07".into(),
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

    pub fn apply_worker_update(&mut self, update: &WorkerUpdate, cx: &mut Context<Self>) {
        self.last_service_update = std::time::Instant::now();
        let mut changed = false;
        match update {
            WorkerUpdate::Workspaces(value) if &self.workspaces != value => {
                self.workspaces = value.clone();
                changed = true;
            }
            WorkerUpdate::ActiveTitle(value) if &self.active_title != value => {
                self.active_title = value.clone();
                changed = true;
            }
            WorkerUpdate::AppId(value) if &self.app_id != value => {
                self.app_id = value.clone();
                changed = true;
            }
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

#[allow(dead_code)]
#[derive(Default)]
struct RenderedWidgets {
    win: bool,
    ws: bool,
    clock_bat: bool,
    perf_media: bool,
    toggles: bool,
}

impl BarView {
    #[allow(dead_code)]
    fn build_section(
        &self,
        widget_names: &[BarWidget],
        side: bool,
        is_floating: bool,
        rendered: &mut RenderedWidgets,
        cx: &App,
    ) -> impl IntoElement {
        let mut elements: Vec<gpui::AnyElement> = Vec::new();

        for name in widget_names {
            match name {
                BarWidget::Builtin(BuiltinBarWidget::Launcher | BuiltinBarWidget::ActiveWindow)
                    if !rendered.win =>
                {
                    elements.push(
                        WindowInfoCapsule::new(
                            "mod-win",
                            self.app_id.clone(),
                            self.active_title.clone(),
                        )
                        .on_click(|_, _, cx| ShellRuntime::open_or_focus_launcher(cx))
                        .into_any_element(),
                    );
                    rendered.win = true;
                }
                BarWidget::Builtin(BuiltinBarWidget::Workspaces) if !rendered.ws => {
                    elements.push(
                        WorkspacesWidget::new("mod-ws", self.workspaces.clone()).into_any_element(),
                    );
                    rendered.ws = true;
                }
                BarWidget::Builtin(BuiltinBarWidget::Clock | BuiltinBarWidget::Battery)
                    if !rendered.clock_bat =>
                {
                    elements.push(
                        ClockBatteryCapsule::new(
                            "mod-clock",
                            self.datetime_str.clone(),
                            self.battery.clone(),
                        )
                        .into_any_element(),
                    );
                    rendered.clock_bat = true;
                }
                BarWidget::Builtin(BuiltinBarWidget::Media | BuiltinBarWidget::Sysinfo)
                    if !rendered.perf_media =>
                {
                    elements.push(
                        PerfMediaCapsule::new("mod-perf", 40, 66, 3, self.media_track.clone())
                            .into_any_element(),
                    );
                    rendered.perf_media = true;
                }
                BarWidget::Builtin(
                    BuiltinBarWidget::Network
                    | BuiltinBarWidget::Audio
                    | BuiltinBarWidget::Settings,
                ) if !rendered.toggles => {
                    elements.push(
                        StatusTogglesCapsule::new(
                            "mod-toggles",
                            self.audio.clone(),
                            self.network.clone(),
                        )
                        .on_click(|_, _, cx| ShellRuntime::open_or_focus_control_center(cx))
                        .into_any_element(),
                    );
                    rendered.toggles = true;
                }
                BarWidget::Extension(_) => {
                    // Extension contribution widget rendering seam
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

                        // Left inverse corner (below bar at x=0, y=bar_height)
                        // Filled shape: (0, y0) -> (0, y0 + r) -> arc centered at (r, y0 + r) to (r, y0) -> (0, y0)
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

                        // Right inverse corner (below bar at x=width, y=bar_height)
                        // Filled shape: (width, y0) -> (width, y0 + r) -> arc centered at (width - r, y0 + r) to (width - r, y0) -> (width, y0)
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

                        // Left inverse corner (above bar at x=0, y=y0)
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

                        // Right inverse corner (above bar at x=width, y=y0)
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

                        // Top-right inverse corner (right of bar at x0, y0)
                        if let Some(path) = build_hug_corner(
                            point(px(x0), px(y0)),
                            point(px(x0 + r), px(y0)),
                            point(px(x0), px(y0 + r)),
                            point(px(x0 + r - k), px(y0)),
                            point(px(x0), px(y0 + r - k)),
                        ) {
                            window.paint_path(path, bg_color);
                        }

                        // Bottom-right inverse corner (right of bar at x0, y1)
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

                        // Top-left inverse corner (left of bar at x0, y0)
                        if let Some(path) = build_hug_corner(
                            point(px(x0), px(y0)),
                            point(px(x0 - r), px(y0)),
                            point(px(x0), px(y0 + r)),
                            point(px(x0 - r + k), px(y0)),
                            point(px(x0), px(y0 + r - k)),
                        ) {
                            window.paint_path(path, bg_color);
                        }

                        // Bottom-left inverse corner (left of bar at x0, y1)
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use shilpo_config::BarStyle;

        let style = self.config.bar.style;
        let side = matches!(
            self.config.bar.position,
            BarPosition::Left | BarPosition::Right
        );
        let opacity = self.config.bar.opacity.clamp(0.0, 1.0);
        let bg_color = cx.theme().surface_container_high.opacity(opacity);

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
            .bg(bg_color);

        let styled_bar = match style {
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
        };

        // All widgets removed for now per user instruction ("remove all the components from the bar for now and we will tackle widgets one by one", "dont add any widgets in the bar yet").
        styled_bar
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
