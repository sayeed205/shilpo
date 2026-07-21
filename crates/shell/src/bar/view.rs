use gpui::{
    App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Task, Window,
    div, px,
};
use shilpo_config::ShellConfig;
use shilpo_services::{
    AudioInfo, AudioService, BatteryInfo, BatteryService, NetworkInfo, NetworkService,
    NiriCompositorService, NiriWorkspaceInfo,
};
use shilpo_ui::h_flex;
use std::time::Duration;

use super::widgets::{
    ClockBatteryCapsule, PerfMediaCapsule, StatusTogglesCapsule, WindowInfoCapsule,
    WorkspacesWidget,
};

/// Status Bar GPUI View (Multi-Capsule Segmented Bar).
pub struct BarView {
    pub config: ShellConfig,
    pub niri_service: NiriCompositorService,
    pub battery_service: BatteryService,
    pub audio_service: AudioService,
    pub network_service: NetworkService,
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
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let config = ShellConfig::default();

        let niri_service =
            NiriCompositorService::new().unwrap_or_else(|_| NiriCompositorService::new().unwrap());

        let battery_service = BatteryService::new().unwrap();
        let battery = battery_service.battery_info();

        let audio_service = AudioService::new().unwrap();
        let audio = audio_service.audio_info();

        let network_service = NetworkService::new().unwrap();
        let network = network_service.network_info();

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

        // Real-time state observer task
        let observer_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;

                let update_res = this.update(cx, |this, cx| {
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

impl Render for BarView {
    fn render(&mut self, _: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w_full()
            .h(px(48.))
            .flex()
            .items_center()
            .justify_between()
            .px_3()
            // Far Left Module
            .child(WindowInfoCapsule::new(
                "mod-win",
                self.app_id.clone(),
                self.active_title.clone(),
            ))
            // Center Modules (Perf & Workspaces)
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(PerfMediaCapsule::new(
                        "mod-perf",
                        40,
                        66,
                        3,
                        self.media_track.clone(),
                    ))
                    .child(WorkspacesWidget::new("mod-ws", self.workspaces.clone())),
            )
            // Far Right Modules (Clock/Power & Status Toggles)
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(ClockBatteryCapsule::new(
                        "mod-clock",
                        self.datetime_str.clone(),
                        self.battery.clone(),
                    ))
                    .child(StatusTogglesCapsule::new(
                        "mod-toggles",
                        self.audio.clone(),
                        self.network.clone(),
                    )),
            )
    }
}
