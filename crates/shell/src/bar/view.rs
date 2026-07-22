use crate::runtime::ShellRuntime;
use gpui::{
    App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Task, Window,
    div, prelude::*, px,
};
use shilpo_config::{BarPosition, BarWidget, ShellConfig};
use shilpo_services::{
    AudioInfo, AudioService, BatteryInfo, BatteryService, IpcRequest, NetworkInfo, NetworkService,
    NiriCompositorService, NiriWorkspaceInfo, Notification, NotificationService, ShellIpcServer,
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
    pub niri_service: NiriCompositorService,
    pub battery_service: BatteryService,
    pub audio_service: AudioService,
    pub network_service: NetworkService,
    pub notification_service: NotificationService,
    pub ipc_server: ShellIpcServer,
    workspaces: Vec<NiriWorkspaceInfo>,
    battery: BatteryInfo,
    audio: AudioInfo,
    network: NetworkInfo,
    app_id: String,
    active_title: String,
    media_track: String,
    datetime_str: String,
    _observer_task: Task<()>,
}

impl BarView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let config_path = std::env::var("HOME")
            .map(|home| std::path::PathBuf::from(home).join(".config/shilpo/config.toml"))
            .unwrap_or_else(|_| std::path::PathBuf::from(".config/shilpo/config.toml"));
        let config = ShellConfig::load_or_create(&config_path).unwrap_or_else(|error| {
            eprintln!("[shilpo-shell] config error: {error}");
            ShellConfig::default()
        });

        let niri_service =
            NiriCompositorService::new().unwrap_or_else(|_| NiriCompositorService::new().unwrap());

        let battery_service = BatteryService::new().unwrap();
        let battery = battery_service.battery_info();

        let audio_service = AudioService::new().unwrap();
        let audio = audio_service.audio_info();

        let network_service = NetworkService::new().unwrap();
        let network = network_service.network_info();

        let notification_service = NotificationService::new().unwrap();
        let (notif_tx, notif_rx) = std::sync::mpsc::channel();
        notification_service.set_new_notification_sender(notif_tx);

        let ipc_server = ShellIpcServer::new().unwrap_or_else(|_| {
            eprintln!("[shilpo-shell] Warning: IPC socket binding fallback");
            ShellIpcServer::new().unwrap()
        });

        // Dynamic theme synchronization with OS appearance and config
        apply_config_theme(&config, Some(window), cx);
        cx.observe_window_appearance(window, |this, window, cx| {
            apply_config_theme(&this.config, Some(window), cx);
            window.refresh();
        })
        .detach();

        let workspaces = niri_service.workspaces();
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

        let (config_tx, config_rx) = std::sync::mpsc::channel();
        let home = std::env::var("HOME")
            .ok()
            .or_else(|| std::env::var("USER").ok().map(|u| format!("/home/{}", u)))
            .unwrap_or_else(|| ".".to_string());
        let config_dir = std::path::PathBuf::from(home).join(".config/shilpo");
        let _ = std::fs::create_dir_all(&config_dir);

        use notify::Watcher;
        let mut watcher = notify::RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Some(_event) = res.ok().filter(|e| e.kind.is_modify()) {
                    let _ = config_tx.send(());
                }
            },
            notify::Config::default(),
        )
        .unwrap();
        let _ = watcher.watch(&config_dir, notify::RecursiveMode::Recursive);

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

                    // Process configuration changes automatically
                    while config_rx.try_recv().is_ok() {
                        match ShellConfig::load_or_create(&config_path) {
                            Ok(config) => {
                                this.config = config;
                                apply_config_theme(&this.config, None, cx);
                                cx.notify();
                            }
                            Err(error) => eprintln!("[shilpo-shell] config reload error: {error}"),
                        }
                    }

                    // Process pending IPC commands
                    let requests = this.ipc_server.pop_pending_requests();
                    for req in requests {
                        match req {
                            IpcRequest::FocusWorkspace(id) => {
                                let _ = this.niri_service.focus_workspace(id);
                            }
                            IpcRequest::ReloadConfig => {
                                match ShellConfig::load_or_create(&config_path) {
                                    Ok(config) => {
                                        this.config = config;
                                        apply_config_theme(&this.config, None, cx);
                                        cx.notify();
                                    }
                                    Err(error) => {
                                        eprintln!("[shilpo-shell] config reload error: {error}")
                                    }
                                }
                            }
                            IpcRequest::ToggleLauncher => {
                                ShellRuntime::toggle_launcher(cx);
                            }
                            IpcRequest::ToggleControlCenter => {
                                ShellRuntime::toggle_control_center(cx);
                            }
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
                                cx.notify();
                            }
                            _ => {}
                        }
                    }

                    let updated_ws = this.niri_service.workspaces();
                    let updated_title = this
                        .niri_service
                        .active_window_title()
                        .unwrap_or_else(|| "Desktop".into());
                    let updated_app_id =
                        this.niri_service.app_id().unwrap_or_else(|| "niri".into());
                    let updated_bat = this.battery_service.battery_info();
                    let updated_audio = this.audio_service.audio_info();
                    let updated_net = this.network_service.network_info();

                    let now = chrono::Local::now();
                    let updated_dt = now.format("%H:%M · %a, %d/%m").to_string();

                    let mut changed = false;
                    if !updated_ws.is_empty() && this.workspaces != updated_ws {
                        this.workspaces = updated_ws;
                        changed = true;
                    }
                    if this.active_title != updated_title {
                        this.active_title = updated_title;
                        changed = true;
                    }
                    if this.app_id != updated_app_id {
                        this.app_id = updated_app_id;
                        changed = true;
                    }
                    if this.battery != updated_bat {
                        this.battery = updated_bat;
                        changed = true;
                    }
                    if this.audio != updated_audio {
                        this.audio = updated_audio;
                        changed = true;
                    }
                    if this.network != updated_net {
                        this.network = updated_net;
                        changed = true;
                    }
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
            niri_service,
            battery_service,
            audio_service,
            network_service,
            notification_service,
            ipc_server,
            workspaces: fallback_ws,
            battery,
            audio,
            network,
            app_id: "shilpo.shell".into(),
            active_title: "Shilpo Shell".into(),
            media_track: "KK - Police ke hathiyar".into(),
            datetime_str: "17:53 · Tue, 21/07".into(),
            _observer_task: observer_task,
        }
    }

    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }
}

impl BarView {
    fn build_section(&self, widget_names: &[BarWidget], side: bool) -> impl IntoElement {
        let mut elements: Vec<gpui::AnyElement> = Vec::new();
        let mut rendered_win = false;
        let mut rendered_ws = false;
        let mut rendered_clock_bat = false;
        let mut rendered_perf_media = false;
        let mut rendered_toggles = false;

        for name in widget_names {
            match name {
                BarWidget::Launcher | BarWidget::ActiveWindow if !rendered_win => {
                    elements.push(
                        WindowInfoCapsule::new(
                            "mod-win",
                            self.app_id.clone(),
                            self.active_title.clone(),
                        )
                        .on_click(|_, _, cx| ShellRuntime::open_or_focus_launcher(cx))
                        .into_any_element(),
                    );
                    rendered_win = true;
                }
                BarWidget::Workspaces if !rendered_ws => {
                    elements.push(
                        WorkspacesWidget::new("mod-ws", self.workspaces.clone()).into_any_element(),
                    );
                    rendered_ws = true;
                }
                BarWidget::Clock | BarWidget::Battery if !rendered_clock_bat => {
                    elements.push(
                        ClockBatteryCapsule::new(
                            "mod-clock",
                            self.datetime_str.clone(),
                            self.battery.clone(),
                        )
                        .into_any_element(),
                    );
                    rendered_clock_bat = true;
                }
                BarWidget::Media | BarWidget::Sysinfo if !rendered_perf_media => {
                    elements.push(
                        PerfMediaCapsule::new("mod-perf", 40, 66, 3, self.media_track.clone())
                            .into_any_element(),
                    );
                    rendered_perf_media = true;
                }
                BarWidget::Network | BarWidget::Audio | BarWidget::Settings
                    if !rendered_toggles =>
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
                    rendered_toggles = true;
                }
                _ => {}
            }
        }

        let flex = if side { v_flex() } else { h_flex() };
        flex.gap(px(self.config.bar.widget_spacing as f32))
            .items_center()
            .children(elements)
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
                this.mx(px(self.config.bar.margin.horizontal as f32))
                    .my(px(self.config.bar.margin.vertical as f32))
                    .rounded_full()
                    .border_1()
                    .border_color(cx.theme().outline_variant.opacity(0.3))
                    .bg(bg_color)
                    .shadow_md()
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
            .child(self.build_section(&self.config.bar.widgets.start, side))
            .child(self.build_section(&self.config.bar.widgets.center, side))
            .child(self.build_section(&self.config.bar.widgets.end, side))
    }
}

pub fn open_notification_toast(cx: &mut App, notification: Notification) {
    use crate::notification::NotificationToastView;
    use gpui::{
        Bounds, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
        layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
        point, px, size,
    };

    let window_size = size(px(320.), px(80.));
    let options = WindowOptions {
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(1920. - 340.), px(54.)),
            size: window_size,
        })),
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
        cx.spawn(async move |cx| {
            cx.background_executor().timer(Duration::from_secs(5)).await;
            cx.update(|cx| {
                handle
                    .update(cx, |_, window, _| window.remove_window())
                    .ok();
            });
        })
        .detach();
    }
}
