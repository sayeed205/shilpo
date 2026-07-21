use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, StyleRefinement,
    Styled, Window, div, px,
};
use shilpo_services::NiriWorkspaceInfo;
use shilpo_ui::{ActiveTheme, StyledExt, h_flex};

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
        let items = self.workspaces.into_iter().map(|ws| {
            let is_active = ws.is_active || ws.is_focused;
            let (bg, fg, w) = if is_active {
                (cx.theme().primary, cx.theme().on_primary, px(28.))
            } else {
                (
                    cx.theme().surface_container_highest,
                    cx.theme().on_surface_variant,
                    px(12.),
                )
            };

            let label = ws.name.unwrap_or_else(|| ws.idx.to_string());

            div()
                .id(("ws", ws.id))
                .h(px(24.))
                .min_w(w)
                .px_2()
                .rounded_full()
                .bg(bg)
                .text_color(fg)
                .text_xs()
                .font_bold()
                .flex()
                .items_center()
                .justify_center()
                .child(label)
        });

        h_flex().id(self.id).gap_2().items_center().children(items)
    }
}
