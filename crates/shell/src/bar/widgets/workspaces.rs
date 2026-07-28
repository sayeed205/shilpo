use crate::actions::ActionInvocation;
use crate::runtime::ShellRuntime;
use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, Role,
    StatefulInteractiveElement, StyleRefinement, Styled, Window, div, px,
};
use shilpo_services::{CompositorConnection, WorkspaceInfo};
use shilpo_ui::{ActiveTheme, ContextMenuExt, Icon, IconName, PopupMenuItem, StyledExt, h_flex};

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
        let is_reconnecting = matches!(self.connection, CompositorConnection::Reconnecting { .. });
        let is_connecting = matches!(self.connection, CompositorConnection::Connecting);

        let mut items: Vec<gpui::AnyElement> = Vec::new();

        if is_connecting {
            items.push(
                div()
                    .id("ws_connecting")
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
        } else if self.workspaces.is_empty() {
            items.push(
                div()
                    .id("ws_empty")
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
        } else {
            let pills = self.workspaces.into_iter().map(|ws| {
                let is_active = ws.is_active || ws.is_focused;
                let opacity_factor = if is_reconnecting { 0.5 } else { 1.0 };
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
                if !is_reconnecting {
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

        if let CompositorConnection::Reconnecting { attempt, .. } = self.connection {
            items.push(
                div()
                    .id("ws_reconnect_indicator")
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_1_5()
                    .h(px(20.))
                    .rounded_full()
                    .bg(cx.theme().error_container)
                    .text_color(cx.theme().on_error_container)
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
