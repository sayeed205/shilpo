use gpui::{
    App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
    Styled, Window, div, img, px,
};
use shilpo_services::Notification;
use shilpo_ui::{ActiveTheme, Icon, IconName, StyledExt, h_flex, v_flex};

use crate::runtime::ShellRuntime;

/// OSD Toast Notification View.
pub struct NotificationToastView {
    pub notification: Notification,
}

impl NotificationToastView {
    pub fn new(notification: Notification, window: &mut Window, cx: &mut Context<Self>) -> Self {
        window.on_window_should_close(cx, |_, cx| {
            ShellRuntime::forget_notification(cx);
            true
        });
        Self { notification }
    }

    pub fn view(notification: Notification, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(notification, window, cx))
    }
}

impl Render for NotificationToastView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app_icon = if let Some(icon_name) = &self.notification.app_icon {
            if let Some(path) = shilpo_services::applications::icons::lookup_icon(icon_name) {
                div()
                    .w(px(36.))
                    .h(px(36.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .overflow_hidden()
                    .child(img(path).w(px(32.)).h(px(32.)))
            } else {
                div()
                    .w(px(36.))
                    .h(px(36.))
                    .rounded_xl()
                    .bg(cx.theme().primary_container)
                    .text_color(cx.theme().on_primary_container)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Icon::new(IconName::Bell).size(px(18.)))
            }
        } else {
            div()
                .w(px(36.))
                .h(px(36.))
                .rounded_xl()
                .bg(cx.theme().primary_container)
                .text_color(cx.theme().on_primary_container)
                .flex()
                .items_center()
                .justify_center()
                .child(Icon::new(IconName::Bell).size(px(18.)))
        };

        h_flex()
            .w_full()
            .h_full()
            .p_4()
            .gap_4()
            .rounded_2xl()
            .bg(cx.theme().surface_container_highest)
            .border_1()
            .border_color(cx.theme().outline_variant.opacity(0.4))
            .shadow_lg()
            .items_center()
            .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                ShellRuntime::close_active_notification(cx);
                window.remove_window();
            })
            .child(app_icon)
            .child(
                v_flex()
                    .gap_0p5()
                    .child(
                        div()
                            .text_sm()
                            .font_bold()
                            .text_color(cx.theme().on_surface)
                            .child(self.notification.summary.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().on_surface_variant)
                            .child(self.notification.body.clone()),
                    ),
            )
    }
}
