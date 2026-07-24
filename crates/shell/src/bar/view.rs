use crate::bar::service_worker::{self, ConfigUpdate, WorkerCommand, WorkerUpdate};
use crate::runtime::ShellRuntime;
use gpui::{
    App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div,
    prelude::*, px,
};
use shilpo_config::{BarPosition, BarWidget, ShellConfig};
use shilpo_services::{AudioInfo, BatteryInfo, NetworkInfo, NiriWorkspaceInfo, Notification};
use shilpo_ui::{ActiveTheme, h_flex, v_flex};
use std::time::Duration;

use super::widgets::{
    ClockBatteryCapsule, PerfMediaCapsule, StatusTogglesCapsule, WindowInfoCapsule,
    WorkspacesWidget,
};

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
                self.config = config.clone();
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

#[derive(Default)]
struct RenderedWidgets {
    win: bool,
    ws: bool,
    clock_bat: bool,
    perf_media: bool,
    toggles: bool,
}

impl BarView {
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
                BarWidget::Launcher | BarWidget::ActiveWindow if !rendered.win => {
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
                BarWidget::Workspaces if !rendered.ws => {
                    elements.push(
                        WorkspacesWidget::new("mod-ws", self.workspaces.clone()).into_any_element(),
                    );
                    rendered.ws = true;
                }
                BarWidget::Clock | BarWidget::Battery if !rendered.clock_bat => {
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
                BarWidget::Media | BarWidget::Sysinfo if !rendered.perf_media => {
                    elements.push(
                        PerfMediaCapsule::new("mod-perf", 40, 66, 3, self.media_track.clone())
                            .into_any_element(),
                    );
                    rendered.perf_media = true;
                }
                BarWidget::Network | BarWidget::Audio | BarWidget::Settings
                    if !rendered.toggles =>
                {
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
}

impl Render for BarView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_floating = self.config.bar.style == shilpo_config::BarStyle::FloatingCapsule;
        let side = matches!(
            self.config.bar.position,
            BarPosition::Left | BarPosition::Right
        );
        let bg_color = cx.theme().surface_container_high.opacity(0.92);
        let mut rendered = RenderedWidgets::default();

        div()
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
            .when(side, |this| this.py(px(self.config.bar.padding as f32)))
            .when(!side, |this| this.px(px(self.config.bar.padding as f32)))
            .when(is_floating, |this| {
                this.px(px(self.config.bar.margin.horizontal as f32))
                    .py(px(self.config.bar.margin.vertical as f32))
            })
            .when(!is_floating, |this| {
                this.bg(bg_color)
                    .when(
                        matches!(self.config.bar.position, BarPosition::Top),
                        |this| this.border_b_1(),
                    )
                    .when(
                        matches!(self.config.bar.position, BarPosition::Bottom),
                        |this| this.border_t_1(),
                    )
                    .when(
                        matches!(self.config.bar.position, BarPosition::Left),
                        |this| this.border_r_1(),
                    )
                    .when(
                        matches!(self.config.bar.position, BarPosition::Right),
                        |this| this.border_l_1(),
                    )
                    .border_color(cx.theme().outline_variant.opacity(0.3))
                    .shadow_sm()
            })
            .child(self.build_section(
                &self.config.bar.widgets.start,
                side,
                is_floating,
                &mut rendered,
                cx,
            ))
            .child(self.build_section(
                &self.config.bar.widgets.center,
                side,
                is_floating,
                &mut rendered,
                cx,
            ))
            .child(self.build_section(
                &self.config.bar.widgets.end,
                side,
                is_floating,
                &mut rendered,
                cx,
            ))
    }
}

pub fn open_notification_toast(cx: &mut App, notification: Notification) {
    use crate::notification::NotificationToastView;
    use gpui::{
        Bounds, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
        layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
        point, px, size,
    };

    ShellRuntime::push_notification_history(cx, notification.clone());
    if ShellRuntime::is_dnd_active(cx) {
        tracing::info!("DND active: suppressing notification toast popup");
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

    if let Ok(handle) = cx.open_window(options, move |window, cx| {
        NotificationToastView::view(notification.clone(), window, cx)
    }) {
        let generation = ShellRuntime::register_notification(cx, handle);
        cx.spawn(async move |cx| {
            cx.background_executor().timer(Duration::from_secs(5)).await;
            cx.update(|cx| ShellRuntime::expire_notification(cx, generation));
        })
        .detach();
    }
}
