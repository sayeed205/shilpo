use crate::actions::ActionInvocation;
use crate::runtime::ShellRuntime;
use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, Role,
    StatefulInteractiveElement, StyleRefinement, Styled, Window, div, px,
};
use shilpo_ui::{ActiveTheme, ContextMenuExt, Icon, IconName, PopupMenuItem, StyledExt, h_flex};

/// Active Window Title widget for Shilpo status bar.
#[derive(IntoElement)]
pub struct ActiveWindowWidget {
    id: ElementId,
    title: String,
    style: StyleRefinement,
}

impl ActiveWindowWidget {
    pub fn new(id: impl Into<ElementId>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for ActiveWindowWidget {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for ActiveWindowWidget {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let aria = format!("Active Window: {}", self.title);
        div()
            .id(self.id)
            .role(Role::Button)
            .aria_label(aria)
            .px_3()
            .py_1()
            .rounded_full()
            .bg(cx.theme().surface_container_high)
            .text_color(cx.theme().on_surface)
            .text_sm()
            .font_medium()
            .max_w(px(220.))
            .overflow_hidden()
            .context_menu(|menu, _, _| {
                menu.item(
                    PopupMenuItem::new("Close Active Overlay")
                        .icon(IconName::SquareTerminal)
                        .accelerator("Super+Q")
                        .on_click(|_, window, cx| {
                            let _ = ShellRuntime::dispatch_action(cx, ActionInvocation::Quit);
                            window.remove_window();
                        }),
                )
            })
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(Icon::new(IconName::SquareTerminal).size(px(16.)))
                    .child(self.title),
            )
    }
}
