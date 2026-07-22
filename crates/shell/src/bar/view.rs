use crate::bar::service_worker::{self, ConfigUpdate, WorkerCommand, WorkerUpdate};
use crate::runtime::ShellRuntime;
use gpui::{
    App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Task, Window,
    div, prelude::*, px,
};
use shilpo_config::{BarPosition, BarWidget, ShellConfig};
use shilpo_services::{
    AudioInfo, AudioService, BatteryInfo, BatteryService, NetworkInfo, NetworkService,
    NiriCompositorService, NiriWorkspaceInfo, Notification, NotificationService,
};
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
    pub notification_service: Option<NotificationService>,
    service_commands: service_worker::CommandSender,
    workspaces: Vec<NiriWorkspaceInfo>,
    battery: BatteryInfo,
    audio: AudioInfo,
    network: NetworkInfo,
    app_id: String,
    active_title: String,
    media_track: String,
    datetime_str: String,
    _observer_task: Task<()>,
    _service_task: Task<()>,
}

impl BarView {
    pub fn enqueue_request(&self, request: shilpo_services::IpcRequest) -> Result<(), String> {
        let command = match request {
            shilpo_services::IpcRequest::FocusWorkspace(id) => WorkerCommand::FocusWorkspace(id),
            shilpo_services::IpcRequest::ReloadConfig => WorkerCommand::ReloadConfig,
            _ => return Err("request is not a bar worker command".into()),
        };
        service_worker::try_send_command(&self.service_commands, command)
            .map_err(|error| format!("{error:?}"))
    }

    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let config_path = std::env::var("HOME")
            .map(|home| std::path::PathBuf::from(home).join(".config/shilpo/config.toml"))
            .unwrap_or_else(|_| std::path::PathBuf::from(".config/shilpo/config.toml"));
        let config = ShellConfig::default();

        let niri = NiriCompositorService::new().ok();
        let battery_service = BatteryService::new().ok();
        let audio_service = AudioService::new().ok();
        let network_service = NetworkService::new().ok();
        let battery = BatteryInfo::default();
        let audio = AudioInfo::default();
        let network = NetworkInfo::default();
        let notification_service = match NotificationService::new() {
            Ok(service) => Some(service),
            Err(error) => {
                tracing::warn!(error = %error, "notification service unavailable; toasts disabled");
                None
            }
        };
        let (notif_tx, notif_rx) = std::sync::mpsc::channel();
        if let Some(service) = &notification_service {
            service.set_new_notification_sender(notif_tx);
        }
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

        let workspaces = Vec::new();
        let fallback_ws = if workspaces.is_empty() {
            vec![
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
            ]
        } else {
            workspaces
        };

        let home = std::env::var("HOME")
            .ok()
            .or_else(|| std::env::var("USER").ok().map(|u| format!("/home/{}", u)))
            .unwrap_or_else(|| ".".to_string());
        let config_dir = std::path::PathBuf::from(home).join(".config/shilpo");
        if let Err(error) = std::fs::create_dir_all(&config_dir) {
            tracing::warn!(error = %error, path = ?config_dir, "config watcher directory unavailable");
        }

        let (updates_tx, updates_rx, service_commands, commands_rx) = service_worker::channels();
        let service_task = service_worker::spawn(
            cx.background_executor().clone(),
            updates_tx,
            commands_rx,
            config_path.clone(),
            niri,
            battery_service,
            audio_service,
            network_service,
        );

        use notify::Watcher;
        let watcher_commands = service_commands.clone();
        let watcher = match notify::RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Some(_event) = res.ok().filter(|e| e.kind.is_modify()) {
                    let _ = service_worker::try_send_command(
                        &watcher_commands,
                        WorkerCommand::ReloadConfig,
                    );
                }
            },
            notify::Config::default(),
        ) {
            Ok(mut watcher) => match watcher.watch(&config_dir, notify::RecursiveMode::Recursive) {
                Ok(()) => Some(watcher),
                Err(error) => {
                    tracing::warn!(error = %error, path = ?config_dir, "config watcher watch failed");
                    None
                }
            },
            Err(error) => {
                tracing::warn!(error = %error, "config watcher creation failed");
                None
            }
        };

        // Real-time state observer and IPC task
        let observer_task = cx.spawn(async move |this, cx| {
            let _watcher = watcher;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;

                let update_res = this.update(cx, |this, cx| {
                    // Pull and spawn newly arrived system notifications
                    while let Ok(notif) = notif_rx.try_recv() {
                        open_notification_toast(cx, notif);
                    }

                    let mut changed = false;
                    while let Ok(update) = updates_rx.try_recv() {
                        match update {
                            WorkerUpdate::Workspaces(value) if this.workspaces != value => {
                                this.workspaces = value;
                                changed = true;
                            }
                            WorkerUpdate::ActiveTitle(value) if this.active_title != value => {
                                this.active_title = value;
                                changed = true;
                            }
                            WorkerUpdate::AppId(value) if this.app_id != value => {
                                this.app_id = value;
                                changed = true;
                            }
                            WorkerUpdate::Battery(value) if this.battery != value => {
                                this.battery = value;
                                changed = true;
                            }
                            WorkerUpdate::Audio(value) if this.audio != value => {
                                this.audio = value;
                                changed = true;
                            }
                            WorkerUpdate::Network(value) if this.network != value => {
                                this.network = value;
                                changed = true;
                            }
                            WorkerUpdate::Config(ConfigUpdate::Loaded(config)) => {
                                this.config = config;
                                apply_config_theme(&this.config, None, cx);
                                changed = true;
                            }
                            WorkerUpdate::Config(ConfigUpdate::Failed(error)) => {
                                tracing::error!(error = %error, "config reload failed");
                            }
                            _ => {}
                        }
                    }

                    let now = chrono::Local::now();
                    let updated_dt = now.format("%H:%M · %a, %d/%m").to_string();

                    if this.datetime_str != updated_dt {
                        this.datetime_str = updated_dt;
                        changed = true;
                    }

                    if changed {
                        cx.notify();
                    }
                });

                if update_res.is_err() {
                    break;
                }
            }
        });

        Self {
            config,
            notification_service,
            service_commands,
            workspaces: fallback_ws,
            battery,
            audio,
            network,
            app_id: "shilpo.shell".into(),
            active_title: "Shilpo Shell".into(),
            media_track: "KK - Police ke hathiyar".into(),
            datetime_str: "17:53 · Tue, 21/07".into(),
            _observer_task: observer_task,
            _service_task: service_task,
        }
    }

    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
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
                this.w(px(self.config.bar.height as f32)).h_full()
            })
            .when(!side, |this| {
                this.w_full().h(px(self.config.bar.height as f32))
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
