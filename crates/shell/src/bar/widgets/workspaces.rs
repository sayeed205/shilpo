use crate::actions::ActionInvocation;
use crate::runtime::ShellRuntime;
use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, Role,
    StatefulInteractiveElement, StyleRefinement, Styled, Window, div, px,
};
use shilpo_services::{CompositorConnection, WorkspaceInfo};
use shilpo_ui::{
    ActiveTheme, ContextMenuExt, Icon, IconName, PopupMenuItem, StyledExt, h_flex, tooltip::Tooltip,
};

fn workspace_actions_enabled(connection: &CompositorConnection) -> bool {
    matches!(connection, CompositorConnection::Ready)
}

fn workspace_status_label(connection: &CompositorConnection) -> Option<&'static str> {
    match connection {
        CompositorConnection::Connecting => Some("Connecting..."),
        CompositorConnection::Reconnecting { .. } => Some("Reconnecting"),
        CompositorConnection::Stopped => Some("Compositor Unavailable"),
        CompositorConnection::Ready => None,
    }
}

/// Workspaces widget for status bar consuming compositor snapshots.
#[derive(IntoElement)]
pub struct WorkspacesWidget {
    id: ElementId,
    workspaces: Vec<WorkspaceInfo>,
    connection: CompositorConnection,
    style: StyleRefinement,
}

impl WorkspacesWidget {
    pub fn new(
        id: impl Into<ElementId>,
        workspaces: Vec<WorkspaceInfo>,
        connection: CompositorConnection,
    ) -> Self {
        Self {
            id: id.into(),
            workspaces,
            connection,
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for WorkspacesWidget {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for WorkspacesWidget {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let is_ready = workspace_actions_enabled(&self.connection);
        let is_connecting = matches!(self.connection, CompositorConnection::Connecting);
        let is_stopped = workspace_status_label(&self.connection) == Some("Compositor Unavailable");

        let mut items: Vec<gpui::AnyElement> = Vec::new();

        if is_connecting {
            items.push(
                div()
                    .id("ws_connecting")
                    .role(Role::Status)
                    .aria_label("Compositor connecting")
                    .h(px(20.))
                    .px_2()
                    .rounded_full()
                    .bg(cx.theme().surface_container_highest.opacity(0.5))
                    .text_color(cx.theme().on_surface_variant.opacity(0.6))
                    .text_xs()
                    .font_bold()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child("Connecting...")
                    .into_any_element(),
            );
        } else if is_stopped {
            let last_err = match &self.connection {
                CompositorConnection::Stopped => "Compositor is unavailable".to_string(),
                _ => String::new(),
            };
            items.push(
                div()
                    .id("ws_stopped")
                    .role(Role::Status)
                    .aria_label("Compositor unavailable")
                    .tooltip(move |window, cx| Tooltip::new(last_err.clone()).build(window, cx))
                    .h(px(20.))
                    .px_2()
                    .rounded_full()
                    .bg(cx.theme().error_container)
                    .text_color(cx.theme().on_error_container)
                    .text_xs()
                    .font_bold()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child("Compositor Unavailable")
                    .into_any_element(),
            );
        } else if is_ready && self.workspaces.is_empty() {
            items.push(
                div()
                    .id("ws_empty")
                    .role(Role::Status)
                    .aria_label("No workspaces available")
                    .h(px(20.))
                    .min_w(px(24.))
                    .px_1_5()
                    .rounded_full()
                    .bg(cx.theme().surface_container_highest.opacity(0.5))
                    .text_color(cx.theme().on_surface_variant.opacity(0.5))
                    .text_xs()
                    .font_bold()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child("1")
                    .into_any_element(),
            );
        } else if !self.workspaces.is_empty() {
            let pills = self.workspaces.into_iter().map(|ws| {
                let is_active = ws.is_active || ws.is_focused;
                let opacity_factor = if is_ready { 1.0 } else { 0.5 };
                let (bg, fg, w) = if is_active {
                    (
                        cx.theme().primary.opacity(opacity_factor),
                        cx.theme().on_primary.opacity(opacity_factor),
                        px(24.),
                    )
                } else {
                    (
                        cx.theme().surface_container_highest.opacity(opacity_factor),
                        cx.theme().on_surface_variant.opacity(opacity_factor),
                        px(18.),
                    )
                };

                let label = ws.name.unwrap_or_else(|| ws.idx.to_string());
                let ws_id = ws.id;

                let pill = div()
                    .id(("ws", ws.id))
                    .role(Role::Button)
                    .aria_label(format!("Workspace {}", label))
                    .h(px(20.))
                    .min_w(w)
                    .px_1_5()
                    .rounded_full()
                    .bg(bg)
                    .text_color(fg)
                    .text_xs()
                    .font_bold()
                    .flex()
                    .items_center()
                    .justify_center();
                if is_ready {
                    pill.cursor_pointer()
                        .on_click(move |_, _, cx| {
                            let _ = ShellRuntime::dispatch_action(
                                cx,
                                ActionInvocation::FocusWorkspace(ws_id),
                            );
                        })
                        .context_menu(move |menu, _window, _cx| {
                            menu.item(
                                PopupMenuItem::new("Focus Workspace")
                                    .icon(IconName::Check)
                                    .accelerator(format!("Super+{}", ws_id))
                                    .on_click(move |_, _, cx| {
                                        let _ = ShellRuntime::dispatch_action(
                                            cx,
                                            ActionInvocation::FocusWorkspace(ws_id),
                                        );
                                    }),
                            )
                        })
                        .child(label)
                        .into_any_element()
                } else {
                    pill.child(label).into_any_element()
                }
            });

            items.extend(pills);
        }

        if let CompositorConnection::Reconnecting {
            attempt,
            ref last_error,
        } = self.connection
        {
            let error_msg = last_error
                .clone()
                .unwrap_or_else(|| "Attempting to reconnect to compositor...".into());
            items.push(
                div()
                    .id("ws_reconnect_indicator")
                    .role(Role::Status)
                    .aria_label(format!("Compositor reconnecting attempt {}", attempt))
                    .tooltip(move |window, cx| Tooltip::new(error_msg.clone()).build(window, cx))
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_1_5()
                    .h(px(20.))
                    .rounded_full()
                    .bg(cx.theme().tertiary_container)
                    .text_color(cx.theme().on_tertiary_container)
                    .text_xs()
                    .child(Icon::new(IconName::Info).size(px(12.)))
                    .child(format!("Reconnecting ({})", attempt))
                    .into_any_element(),
            );
        }

        h_flex()
            .id(self.id)
            .h(px(32.))
            .px_2()
            .items_center()
            .gap_1_5()
            .rounded_full()
            .bg(cx.theme().surface_container_high.opacity(0.92))
            .border_1()
            .border_color(cx.theme().outline_variant.opacity(0.3))
            .shadow_sm()
            .children(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_actions_only_enable_when_ready() {
        assert!(workspace_actions_enabled(&CompositorConnection::Ready));
        assert!(!workspace_actions_enabled(
            &CompositorConnection::Connecting
        ));
        assert!(!workspace_actions_enabled(
            &CompositorConnection::Reconnecting {
                attempt: 1,
                last_error: None,
            }
        ));
        assert!(!workspace_actions_enabled(&CompositorConnection::Stopped));
    }

    #[test]
    fn workspace_status_labels_cover_connection_states() {
        assert_eq!(
            workspace_status_label(&CompositorConnection::Connecting),
            Some("Connecting...")
        );
        assert_eq!(
            workspace_status_label(&CompositorConnection::Reconnecting {
                attempt: 2,
                last_error: Some("socket closed".into()),
            }),
            Some("Reconnecting")
        );
        assert_eq!(
            workspace_status_label(&CompositorConnection::Stopped),
            Some("Compositor Unavailable")
        );
        assert_eq!(workspace_status_label(&CompositorConnection::Ready), None);
    }
}
