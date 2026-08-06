use crate::actions::ActionInvocation;
use crate::bar::service_worker::{self, WorkerCommand, WorkerUpdate};
use crate::battery::BatteryIndicator;
use crate::runtime::ShellRuntime;
use gpui::{
    App, AppContext, Context, Entity, IntoElement, ParentElement, Path, PathBuilder, Pixels, Point,
    Render, Styled, Window, div, prelude::*, px,
};
use shilpo_config::{BarPosition, BarWidget, ShellConfig};
use shilpo_services::Notification;
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
    if let Some(state) = shilpo_theme_daemon::read_state_snapshot() {
        shilpo_ui::Theme::global_mut(cx).apply_state(&state);
    }
}

/// Status Bar GPUI View (Multi-Capsule Segmented Bar with IPC integration).
use super::state::{BarState, BarStateEffect};

pub struct BarView {
    pub state: BarState,
    service_commands: service_worker::CommandSender,
    _datetime_task: Option<gpui::Task<()>>,
    _sysinfo_task: Option<gpui::Task<()>>,
    extension_instance_prefix: Option<String>,
    output_name: Option<String>,
}

impl BarView {
    pub fn update_datetime(&mut self) -> bool {
        self.state.update_datetime()
    }

    pub fn is_stale(&self) -> bool {
        self.state.is_stale()
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

        let state = BarState::new(config);

        let _datetime_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let res = this.update(cx, |view, cx| {
                    if view.state.update_datetime() {
                        cx.notify();
                    }
                });
                if res.is_err() {
                    break;
                }
            }
        }));

        let _sysinfo_task = Some(cx.spawn(async move |this, cx| {
            use sysinfo::System;
            let mut sys = System::new();
            let mut last_cpu_sample = std::time::Instant::now() - Duration::from_secs(2);
            let mut cpu_pct: u8 = 0;
            let mut ram_pct: u8 = 0;

            loop {
                if last_cpu_sample.elapsed() >= Duration::from_secs(1) {
                    sys.refresh_cpu_usage();
                    sys.refresh_memory();
                    cpu_pct = sys.global_cpu_usage().round().clamp(0.0, 100.0) as u8;
                    let total_mem = sys.total_memory();
                    let used_mem = sys.used_memory();
                    ram_pct = if total_mem > 0 {
                        ((used_mem as f64 / total_mem as f64) * 100.0)
                            .round()
                            .clamp(0.0, 100.0) as u8
                    } else {
                        0
                    };
                    last_cpu_sample = std::time::Instant::now();
                }

                let speed = ((cpu_pct as f32) / 5.0).clamp(1.0, 20.0);
                let interval_ms = (500.0 / speed) as u64;

                cx.background_executor()
                    .timer(Duration::from_millis(interval_ms))
                    .await;

                let res = this.update(cx, |view, cx| {
                    view.state.cpu_percent = cpu_pct;
                    view.state.ram_percent = ram_pct;
                    view.state.cat_frame_index = (view.state.cat_frame_index + 1) % 5;
                    cx.notify();
                });
                if res.is_err() {
                    break;
                }
            }
        }));

        Self {
            state,
            service_commands,
            _datetime_task,
            _sysinfo_task,
            extension_instance_prefix: None,
            output_name: None,
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
        Self::view_with_config_on_display_with_output(window, cx, config, display_id, None)
    }

    pub fn view_with_config_on_display_with_output(
        window: &mut Window,
        cx: &mut App,
        config: ShellConfig,
        display_id: gpui::DisplayId,
        output_name: Option<String>,
    ) -> Entity<Self> {
        cx.new(|cx| {
            let mut view = Self::new_with_config(window, cx, config);
            view.extension_instance_prefix = Some(format!("bar:{display_id:?}"));
            view.output_name = output_name;
            view
        })
    }

    pub fn apply_worker_update(&mut self, update: &WorkerUpdate, cx: &mut Context<Self>) {
        let result = self.state.apply_worker_update(update);
        for effect in result.effects {
            match effect {
                BarStateEffect::ShowOsd(kind) => {
                    ShellRuntime::show_osd(cx, kind);
                }
                BarStateEffect::ShowNotificationToast(notification) => {
                    open_notification_toast(cx, notification);
                }
                BarStateEffect::ApplyConfigTheme(config) => {
                    apply_config_theme(&config, None, cx);
                }
            }
        }
        if result.changed {
            cx.notify();
        }
    }

    pub fn build(spec: crate::bar::BarSpec, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut view = Self::new_with_config(window, cx, ShellConfig::default());
        view.state.config.bar = spec.config;
        view.extension_instance_prefix = Some(format!("bar:{:?}", spec.display_id));
        view.output_name = spec.output_name;
        view
    }

    pub fn view_with_spec(
        spec: crate::bar::BarSpec,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| Self::build(spec, window, cx))
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
                    let is_vertical = matches!(
                        self.state.config.bar.position,
                        shilpo_config::BarPosition::Left | shilpo_config::BarPosition::Right
                    );
                    let pill_orientation = if is_vertical {
                        super::widgets::pill_strip::PillOrientation::Vertical
                    } else {
                        super::widgets::pill_strip::PillOrientation::Horizontal
                    };
                    elements.push(
                        super::widgets::WorkspacesWidget::new(
                            "workspaces",
                            snapshot.workspaces.clone(),
                            snapshot.connection.clone(),
                            pill_orientation,
                        )
                        .into_any_element(),
                    );
                }
                BarWidget::Builtin(BuiltinBarWidget::RunningApps) => {
                    let snapshot = ShellRuntime::compositor_snapshot(cx);
                    let app_icons = std::sync::Arc::new(crate::app_icons::build_app_icon_index(
                        ShellRuntime::overview_applications(cx),
                    ));
                    let reduced_motion = ShellRuntime::overview_reduced_motion(cx);
                    let is_vertical = matches!(
                        self.state.config.bar.position,
                        shilpo_config::BarPosition::Left | shilpo_config::BarPosition::Right
                    );
                    let pill_orientation = if is_vertical {
                        super::widgets::pill_strip::PillOrientation::Vertical
                    } else {
                        super::widgets::pill_strip::PillOrientation::Horizontal
                    };
                    elements.push(
                        super::widgets::RunningAppsWidget::new(
                            format!("running_apps_{section_name}_{index}"),
                            self.output_name.clone(),
                            (*snapshot).clone(),
                            app_icons,
                            pill_orientation,
                            reduced_motion,
                        )
                        .into_any_element(),
                    );
                }
                BarWidget::Builtin(BuiltinBarWidget::Clock) => {
                    let is_vertical = matches!(
                        self.state.config.bar.position,
                        shilpo_config::BarPosition::Left | shilpo_config::BarPosition::Right
                    );
                    elements.push(
                        super::widgets::ClockWidget::new(
                            format!("clock_{section_name}_{index}"),
                            self.state.time_str.clone(),
                            is_vertical,
                        )
                        .into_any_element(),
                    );
                }
                BarWidget::Builtin(BuiltinBarWidget::Date) => {
                    let is_vertical = matches!(
                        self.state.config.bar.position,
                        shilpo_config::BarPosition::Left | shilpo_config::BarPosition::Right
                    );
                    elements.push(
                        super::widgets::DateWidget::new(
                            format!("date_{section_name}_{index}"),
                            self.state.date_str.clone(),
                            is_vertical,
                        )
                        .into_any_element(),
                    );
                }
                BarWidget::Builtin(BuiltinBarWidget::Media) => {
                    if let Some(info) = &self.state.media_info
                        && !info.is_empty()
                    {
                        let is_vertical = matches!(
                            self.state.config.bar.position,
                            shilpo_config::BarPosition::Left | shilpo_config::BarPosition::Right
                        );
                        elements.push(
                            super::widgets::MediaWidget::new(
                                format!("media_{section_name}_{index}"),
                                info.clone(),
                                is_vertical,
                                self.service_commands.clone(),
                            )
                            .into_any_element(),
                        );
                    }
                }
                BarWidget::Builtin(BuiltinBarWidget::Battery) => {
                    if self.state.battery.is_present {
                        elements.push(
                            BatteryIndicator::new(
                                format!("battery_{section_name}_{index}"),
                                self.state.battery.clone(),
                            )
                            .into_any_element(),
                        );
                    }
                }
                BarWidget::Builtin(BuiltinBarWidget::Sysinfo) => {
                    elements.push(
                        super::widgets::SysInfoWidget::new(
                            format!("sysinfo_{section_name}_{index}"),
                            self.state.cat_frame_index,
                            self.state.cpu_percent,
                            self.state.ram_percent,
                        )
                        .into_any_element(),
                    );
                }
                BarWidget::Builtin(BuiltinBarWidget::Network) => {
                    elements.push(
                        super::widgets::NetworkWidget::new(
                            format!("network_{section_name}_{index}"),
                            self.state.network.clone(),
                        )
                        .into_any_element(),
                    );
                }
                BarWidget::Builtin(BuiltinBarWidget::Bluetooth) => {
                    elements.push(
                        super::widgets::BluetoothWidget::new(
                            format!("bluetooth_{section_name}_{index}"),
                            self.state.bluetooth.clone(),
                        )
                        .into_any_element(),
                    );
                }
                BarWidget::Builtin(BuiltinBarWidget::Caffeine) => {
                    let caffeine_svc = self.state.caffeine_service.clone();
                    let is_active = caffeine_svc.is_active();
                    elements.push(
                        crate::widgets::CaffeineWidget::new(
                            format!("caffeine_{section_name}_{index}"),
                            is_active,
                        )
                        .on_click(move |_, _, _cx| {
                            caffeine_svc.toggle();
                        })
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
            .gap(px(self.state.config.bar.widget_spacing as f32))
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

        let bar_height = px(self.state.config.bar.height as f32);

        canvas(
            move |_, _, _| {},
            move |bounds, _, window, _| {
                let r = radius.as_f32();
                let k = r * 0.552_284_8;

                match position {
                    BarPosition::Top => {
                        let y0 = bounds.origin.y.as_f32() + bar_height.as_f32() - 0.5;

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
                            - bar_height.as_f32()
                            + 0.5;

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
                        let x0 = bounds.origin.x.as_f32() + bar_height.as_f32() - 0.5;
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
                            - bar_height.as_f32()
                            + 0.5;
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

pub(crate) fn compute_bar_input_region(
    child_bounds: &[gpui::Bounds<gpui::Pixels>],
) -> Option<gpui::Bounds<gpui::Pixels>> {
    let mut region: Option<gpui::Bounds<gpui::Pixels>> = None;
    for bounds in child_bounds {
        region = match region {
            Some(acc) => Some(acc.union(bounds)),
            None => Some(*bounds),
        };
    }
    region
}

impl Render for BarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use shilpo_config::BarStyle;

        let style = self.state.config.bar.style;
        let side = matches!(
            self.state.config.bar.position,
            BarPosition::Left | BarPosition::Right
        );
        let opacity = if style == BarStyle::Hug {
            1.0
        } else {
            self.state.config.bar.opacity.clamp(0.0, 1.0)
        };
        let compositor_stale = !ShellRuntime::compositor_snapshot(cx).connection.is_ready();
        let bg_color = cx
            .theme()
            .surface_container_high
            .opacity(if compositor_stale {
                opacity * 0.65
            } else {
                opacity
            });
        let widgets = self.state.config.bar.widgets.clone();
        let is_floating = style == BarStyle::Float;
        let start = self.build_section("start", &widgets.start, side, is_floating, window, cx);
        let center = self.build_section("center", &widgets.center, side, is_floating, window, cx);
        let end = self.build_section("end", &widgets.end, side, is_floating, window, cx);

        let bar_container = div()
            .when(side, |this| {
                this.w(px(self.state.config.bar.height as f32))
                    .h_full()
                    .flex_col()
            })
            .when(!side, |this| {
                this.w_full()
                    .h(px(self.state.config.bar.height as f32))
                    .flex_row()
            })
            .flex()
            .items_center()
            .justify_between()
            .bg(bg_color)
            .child(start)
            .child(center)
            .child(end);

        let set_bar_input_region = move |child_bounds: Vec<gpui::Bounds<gpui::Pixels>>,
                                         window: &mut Window,
                                         _: &mut App| {
            if let Some(region) = compute_bar_input_region(&child_bounds) {
                window.set_input_region(Some(&[region]));
            } else {
                window.set_input_region(Some(&[]));
            }
        };

        match style {
            BarStyle::Hug => {
                let hug_corners = self.render_hug_corners(
                    self.state.config.bar.position,
                    px(HUG_CORNER_RADIUS),
                    bg_color,
                );
                let main_bar = bar_container;
                let aligned_bar = match self.state.config.bar.position {
                    BarPosition::Top => div().absolute().top_0().w_full().child(main_bar),
                    BarPosition::Bottom => div().absolute().bottom_0().w_full().child(main_bar),
                    BarPosition::Left => div().absolute().left_0().h_full().child(main_bar),
                    BarPosition::Right => div().absolute().right_0().h_full().child(main_bar),
                }
                .on_children_prepainted(set_bar_input_region);

                div()
                    .relative()
                    .size_full()
                    .child(aligned_bar)
                    .child(hug_corners)
                    .into_any_element()
            }
            BarStyle::Float => div()
                .relative()
                .size_full()
                .on_children_prepainted(set_bar_input_region)
                .child(
                    bar_container
                        .px(px(self.state.config.bar.margin.horizontal as f32))
                        .py(px(self.state.config.bar.margin.vertical as f32))
                        .rounded_2xl()
                        .shadow_md(),
                )
                .into_any_element(),
            BarStyle::Rect => div()
                .relative()
                .size_full()
                .on_children_prepainted(set_bar_input_region)
                .child(bar_container.rounded_none().shadow_sm())
                .into_any_element(),
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
    use shilpo_config::BarPosition;

    let timeout = notification_timeout(&notification);
    let notification_id = notification.id;

    let bar_config = ShellRuntime::active_config(cx).bar;
    let bar_position = bar_config.position;
    let bar_h = bar_config.height as f32;
    let is_float = bar_config.style == shilpo_config::BarStyle::Float;
    let float_margin_h = if is_float {
        bar_config.margin.horizontal as f32
    } else {
        0.
    };
    let float_margin_v = if is_float {
        bar_config.margin.vertical as f32
    } else {
        0.
    };

    let (display_bounds, display_id) = if let Some(display) = cx.primary_display() {
        (display.bounds(), Some(display.id()))
    } else {
        (
            Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.))),
            None,
        )
    };
    let gap = px(8.);
    let window_height = display_bounds.size.height - px(bar_h + float_margin_v) - gap - gap;
    let window_size = size(px(376.), window_height);

    // Layer shell margins are evaluated relative to the bar's exclusive zone by Niri.
    // Windowed origins include explicit bar height offsets.
    let (anchor, margin, origin) = match bar_position {
        BarPosition::Top => (
            Anchor::TOP | Anchor::RIGHT,
            Some((gap, gap, px(0.), px(0.))),
            point(
                display_bounds.origin.x + display_bounds.size.width - window_size.width - gap,
                display_bounds.origin.y + px(bar_h + float_margin_v) + gap,
            ),
        ),
        BarPosition::Bottom => (
            Anchor::BOTTOM | Anchor::RIGHT,
            Some((px(0.), gap, gap, px(0.))),
            point(
                display_bounds.origin.x + display_bounds.size.width - window_size.width - gap,
                display_bounds.origin.y + display_bounds.size.height
                    - window_size.height
                    - px(bar_h + float_margin_v)
                    - gap,
            ),
        ),
        BarPosition::Left => (
            Anchor::TOP | Anchor::LEFT,
            Some((gap, px(0.), px(0.), gap)),
            point(
                display_bounds.origin.x + px(bar_h + float_margin_h) + gap,
                display_bounds.origin.y + gap,
            ),
        ),
        BarPosition::Right => (
            Anchor::TOP | Anchor::RIGHT,
            Some((gap, gap, px(0.), px(0.))),
            point(
                display_bounds.origin.x + display_bounds.size.width
                    - window_size.width
                    - px(bar_h + float_margin_h)
                    - gap,
                display_bounds.origin.y + gap,
            ),
        ),
    };

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
            anchor,
            margin,
            keyboard_interactivity: KeyboardInteractivity::None,
            ..Default::default()
        }),
        ..Default::default()
    };

    if let Some(handle) = ShellRuntime::active_notification_handle(cx) {
        let generation = ShellRuntime::reserve_notification_generation(cx);
        if handle
            .update(cx, |view, window, cx| {
                view.push(notification.clone(), generation, timeout, window, cx);
            })
            .is_ok()
        {
            ShellRuntime::register_notification(cx, generation, notification_id, handle);
            return;
        }
    }

    let generation = ShellRuntime::reserve_notification_generation(cx);
    if let Ok(handle) = cx.open_window(options, move |window, cx| {
        NotificationToastView::view(
            notification.clone(),
            generation,
            timeout,
            bar_position,
            window,
            cx,
        )
    }) {
        ShellRuntime::register_notification(cx, generation, notification_id, handle);
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

#[cfg(test)]
mod bar_input_region_tests {
    use crate::bar::view::compute_bar_input_region;
    use gpui::{Bounds, point, px, size};

    #[test]
    fn calculates_union_of_child_bounds_for_input_region() {
        let child1 = Bounds::new(point(px(0.0), px(0.0)), size(px(100.0), px(48.0)));
        let child2 = Bounds::new(point(px(200.0), px(0.0)), size(px(100.0), px(48.0)));

        let region = compute_bar_input_region(&[child1, child2]).unwrap();
        assert_eq!(region.origin, point(px(0.0), px(0.0)));
        assert_eq!(region.size, size(px(300.0), px(48.0)));
    }

    #[test]
    fn returns_none_when_child_bounds_empty() {
        assert_eq!(compute_bar_input_region(&[]), None);
    }
}
