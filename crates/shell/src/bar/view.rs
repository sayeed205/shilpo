use gpui::{
    App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div, px,
};
use shilpo_services::{NiriCompositorService, NiriWorkspaceInfo};
use shilpo_ui::{floating_toolbar::FloatingToolbar, h_flex};

use super::widgets::{ClockWidget, WorkspacesWidget};

/// Status Bar GPUI View.
pub struct BarView {
    pub niri_service: NiriCompositorService,
    workspaces: Vec<NiriWorkspaceInfo>,
    clock_time: String,
}

impl BarView {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        let niri_service =
            NiriCompositorService::new().unwrap_or_else(|_| NiriCompositorService::new().unwrap());

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

        Self {
            niri_service,
            workspaces: fallback_ws,
            clock_time: "16:45".into(),
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
            .justify_center()
            .px_4()
            .child(
                FloatingToolbar::horizontal("status-bar")
                    .vibrant(true)
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_6()
                            .px_3()
                            .child(WorkspacesWidget::new("ws-widget", self.workspaces.clone()))
                            .child(ClockWidget::new("clock-widget", self.clock_time.clone())),
                    ),
            )
    }
}
