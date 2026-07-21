use gpui::{
    App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div, px,
};
use shilpo_config::ShellConfig;
use shilpo_services::{
    AudioInfo, AudioService, BatteryInfo, BatteryService, NetworkInfo, NetworkService,
    NiriCompositorService, NiriWorkspaceInfo,
};
use shilpo_ui::h_flex;

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
}

impl BarView {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
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
                NiriWorkspaceInfo {
                    id: 4,
                    name: Some("4".into()),
                    idx: 4,
                    is_active: false,
                    is_focused: false,
                },
            ]
        } else {
            workspaces
        };

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
            datetime_str: "17:44 · Tue, 21/07".into(),
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
