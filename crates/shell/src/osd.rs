use gpui::{
    App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div, px,
    relative,
};
use shilpo_ui::{ActiveTheme, Icon, IconName, StyledExt, h_flex, v_flex};

use crate::runtime::ShellRuntime;

/// Kind of On-Screen Display popup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OsdKind {
    Volume { level: u32, muted: bool },
    Brightness { level: u32 },
    Notification(shilpo_services::Notification),
}

/// On-Screen Display (OSD) Overlay View.
pub struct OsdView {
    pub kind: OsdKind,
}

impl OsdView {
    pub fn new(kind: OsdKind, window: &mut Window, cx: &mut Context<Self>) -> Self {
        window.on_window_should_close(cx, |_, cx| {
            ShellRuntime::forget_osd(cx);
            true
        });
        Self { kind }
    }

    pub fn view(
        kind: OsdKind,
        window: &mut Window,
        cx: &mut App,
    ) -> (Entity<shilpo_ui::Root>, Entity<Self>) {
        let view = cx.new(|cx| Self::new(kind, window, cx));
        let root = cx.new(|cx| {
            shilpo_ui::Root::new(view.clone(), window, cx)
                .bordered(false)
                .bg(cx.theme().transparent)
        });
        (root, view)
    }
}

impl Render for OsdView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let OsdKind::Notification(ref notif) = self.kind {
            let (bg, fg) = match notif.urgency {
                shilpo_services::NotificationUrgency::Critical => (
                    cx.theme().error_container.opacity(0.95),
                    cx.theme().on_error_container,
                ),
                shilpo_services::NotificationUrgency::Normal => (
                    cx.theme().surface_container_high.opacity(0.92),
                    cx.theme().on_surface,
                ),
                shilpo_services::NotificationUrgency::Low => (
                    cx.theme().surface_container.opacity(0.85),
                    cx.theme().on_surface_variant,
                ),
            };

            return h_flex()
                .w_full()
                .h_full()
                .px_4()
                .py_3()
                .gap_3()
                .rounded_2xl()
                .bg(bg)
                .text_color(fg)
                .border_1()
                .border_color(cx.theme().outline_variant.opacity(0.35))
                .shadow_2xl()
                .items_center()
                .child(Icon::new(IconName::Bell).size(px(20.)))
                .child(
                    v_flex()
                        .flex_1()
                        .gap_0p5()
                        .child(div().text_xs().font_bold().child(notif.summary.clone()))
                        .child(div().text_xs().child(notif.body.clone())),
                );
        }

        let (icon, level, muted) = match self.kind {
            OsdKind::Volume { level, muted } => (IconName::Bell, level, muted),
            OsdKind::Brightness { level } => (IconName::Sun, level, false),
            OsdKind::Notification(_) => unreachable!(),
        };

        let fill_pct = (level as f32 / 100.0).clamp(0.0, 1.0);
        let fill_color = if muted {
            cx.theme().outline_variant
        } else {
            cx.theme().primary
        };

        h_flex()
            .w_full()
            .h_full()
            .px_5()
            .py_2()
            .gap_3p5()
            .rounded_full()
            .bg(cx.theme().surface_container_high.opacity(0.88))
            .border_1()
            .border_color(cx.theme().outline_variant.opacity(0.35))
            .shadow_2xl()
            .items_center()
            .child(
                div()
                    .w(px(28.))
                    .h(px(28.))
                    .rounded_full()
                    .bg(cx.theme().primary_container.opacity(0.9))
                    .text_color(cx.theme().on_primary_container)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Icon::new(icon).size(px(16.))),
            )
            .child(
                v_flex().flex_1().justify_center().child(
                    div()
                        .h(px(8.))
                        .w_full()
                        .rounded_full()
                        .bg(cx.theme().surface_container.opacity(0.6))
                        .overflow_hidden()
                        .child(
                            div()
                                .h_full()
                                .w(relative(fill_pct))
                                .bg(fill_color)
                                .rounded_full(),
                        ),
                ),
            )
            .child(
                div()
                    .w(px(32.))
                    .text_xs()
                    .font_bold()
                    .text_color(cx.theme().on_surface)
                    .child(format!("{}%", level)),
            )
    }
}
