use crate::actions::ActionInvocation;
use crate::runtime::ShellRuntime;
use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, Role,
    StatefulInteractiveElement, StyleRefinement, Styled, Window, div, px,
};
use shilpo_services::NiriWorkspaceInfo;
use shilpo_ui::{ActiveTheme, ContextMenuExt, IconName, PopupMenuItem, StyledExt, h_flex};

/// Workspaces widget for Niri compositor status bar.
#[derive(IntoElement)]
pub struct WorkspacesWidget {
    id: ElementId,
    workspaces: Vec<NiriWorkspaceInfo>,
    style: StyleRefinement,
}

impl WorkspacesWidget {
    pub fn new(id: impl Into<ElementId>, workspaces: Vec<NiriWorkspaceInfo>) -> Self {
        Self {
            id: id.into(),
            workspaces,
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
        let items: Vec<gpui::AnyElement> = if self.workspaces.is_empty() {
            vec![
                div()
                    .id("ws_fallback")
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
            ]
        } else {
            self.workspaces
                .into_iter()
                .map(|ws| {
                    let is_active = ws.is_active || ws.is_focused;
                    let (bg, fg, w) = if is_active {
                        (cx.theme().primary, cx.theme().on_primary, px(24.))
                    } else {
                        (
                            cx.theme().surface_container_highest,
                            cx.theme().on_surface_variant,
                            px(18.),
                        )
                    };

                    let label = ws.name.unwrap_or_else(|| ws.idx.to_string());
                    let ws_id = ws.id;

                    div()
                        .id(("ws", ws.id))
                        .role(Role::Button)
                        .aria_label(format!("Workspace {}", label))
                        .cursor_pointer()
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
                                    .on_click(move |_, _, cx| {
                                        let _ = ShellRuntime::dispatch_action(
                                            cx,
                                            ActionInvocation::FocusWorkspace(ws_id),
                                        );
                                    }),
                            )
                        })
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
                        .justify_center()
                        .child(label)
                        .into_any_element()
                })
                .collect()
        };

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
