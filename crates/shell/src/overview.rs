use crate::runtime::ShellRuntime;
use gpui::{
    App, AppContext, Context, ElementId, Entity, InteractiveElement, IntoElement, ParentElement,
    Render, SharedString, Styled, Window, div, px,
};
use shilpo_services::{WindowInfo, WorkspaceInfo};
use shilpo_ui::{ActiveTheme, StyledExt};

/// Interactive Niri horizontal workspace column overview surface.
pub struct WorkspaceOverview {
    workspaces: Vec<WorkspaceInfo>,
    windows: Vec<WindowInfo>,
    active_workspace_id: Option<u64>,
    selected_window_id: Option<u64>,
}

impl WorkspaceOverview {
    pub fn new(
        workspaces: Vec<WorkspaceInfo>,
        windows: Vec<WindowInfo>,
        active_workspace_id: Option<u64>,
    ) -> Self {
        let selected_window_id = windows
            .iter()
            .find(|w| w.workspace_id == active_workspace_id && w.is_focused)
            .map(|w| w.id);

        Self {
            workspaces,
            windows,
            active_workspace_id,
            selected_window_id,
        }
    }

    /// Creates an offline/empty WorkspaceOverview for testing.
    pub fn new_offline() -> Self {
        Self::new(
            vec![
                WorkspaceInfo {
                    id: 1,
                    name: Some("1".into()),
                    idx: 1,
                    is_active: true,
                    is_focused: true,
                    is_urgent: false,
                },
                WorkspaceInfo {
                    id: 2,
                    name: Some("2".into()),
                    idx: 2,
                    is_active: false,
                    is_focused: false,
                    is_urgent: false,
                },
            ],
            vec![WindowInfo {
                id: 101,
                title: Some("Terminal".into()),
                app_id: Some("foot".into()),
                workspace_id: Some(1),
                is_focused: true,
            }],
            Some(1),
        )
    }

    pub fn selected_window_id(&self) -> Option<u64> {
        self.selected_window_id
    }

    pub fn view(window: &mut Window, cx: &mut App) -> Entity<shilpo_ui::Root> {
        let (workspaces, windows, active_ws) =
            if let Ok(niri) = shilpo_services::NiriCompositorService::new() {
                use shilpo_services::CompositorAdapter;
                let ws = CompositorAdapter::workspaces(&niri);
                let win = CompositorAdapter::windows(&niri);
                let active = ws.iter().find(|w| w.is_active).map(|w| w.id);
                (ws, win, active)
            } else {
                (Vec::new(), Vec::new(), None)
            };

        window.on_window_should_close(cx, |_, cx| {
            ShellRuntime::forget_overview(cx);
            true
        });

        let overview = cx.new(|_| Self::new(workspaces, windows, active_ws));
        cx.new(|cx| shilpo_ui::Root::new(overview, window, cx))
    }

    pub fn select_next_window(&mut self) {
        if self.windows.is_empty() {
            return;
        }
        let current_index = self
            .selected_window_id
            .and_then(|id| self.windows.iter().position(|w| w.id == id))
            .unwrap_or(0);
        let next_index = (current_index + 1) % self.windows.len();
        self.selected_window_id = Some(self.windows[next_index].id);
    }
}

impl Render for WorkspaceOverview {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let bg_color = theme.surface.opacity(0.85);
        let border_color = theme.outline;
        let card_bg = theme.surface_container_high;
        let active_card_bg = theme.primary_container;

        div()
            .id("workspace_overview")
            .size_full()
            .bg(bg_color)
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .p_8()
            .child(
                div()
                    .text_xl()
                    .font_bold()
                    .text_color(theme.on_surface)
                    .mb_6()
                    .child("Workspace Overview"),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_6()
                    .children(self.workspaces.iter().map(|ws| {
                        let ws_id = ws.id;
                        let is_active = self.active_workspace_id == Some(ws_id);
                        let ws_windows: Vec<_> = self
                            .windows
                            .iter()
                            .filter(|w| w.workspace_id == Some(ws_id))
                            .collect();

                        let ws_title = ws.name.clone().unwrap_or_else(|| ws.idx.to_string());

                        div()
                            .id(ElementId::Name(SharedString::from(format!(
                                "ws_card_{}",
                                ws_id
                            ))))
                            .w(px(240.0))
                            .h(px(320.0))
                            .rounded_xl()
                            .border_2()
                            .border_color(if is_active {
                                theme.primary
                            } else {
                                border_color
                            })
                            .bg(if is_active { active_card_bg } else { card_bg })
                            .p_4()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_lg()
                                    .font_semibold()
                                    .text_color(if is_active {
                                        theme.on_primary_container
                                    } else {
                                        theme.on_surface
                                    })
                                    .mb_3()
                                    .child(format!("Workspace {}", ws_title)),
                            )
                            .child(div().flex().flex_col().gap_2().children(
                                ws_windows.into_iter().map(|win| {
                                    let win_id = win.id;
                                    let is_selected = self.selected_window_id == Some(win_id);
                                    let win_title = win.title.as_deref().unwrap_or("Window");
                                    let win_app = win.app_id.as_deref().unwrap_or("app");

                                    div()
                                        .id(ElementId::Name(SharedString::from(format!(
                                            "win_tile_{}",
                                            win_id
                                        ))))
                                        .p_2()
                                        .rounded_md()
                                        .bg(if is_selected {
                                            theme.secondary_container
                                        } else {
                                            theme.surface_container_low
                                        })
                                        .text_sm()
                                        .text_color(theme.on_surface)
                                        .child(format!("{} ({})", win_title, win_app))
                                }),
                            ))
                    })),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_overview_navigation() {
        let mut overview = WorkspaceOverview::new_offline();
        assert_eq!(overview.selected_window_id(), Some(101));
        overview.select_next_window();
        assert_eq!(overview.selected_window_id(), Some(101));
    }
}
