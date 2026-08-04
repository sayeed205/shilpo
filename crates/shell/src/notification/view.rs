use std::collections::{HashSet, VecDeque};
use std::time::Duration;

use gpui::{
    Animation, AnimationExt, App, AppContext, Context, ElementId, Entity, InteractiveElement,
    IntoElement, ParentElement, Render, Role, StatefulInteractiveElement, Styled, Window, div, img,
    prelude::FluentBuilder, px,
};
use shilpo_services::Notification;
use shilpo_ui::{
    ActiveTheme, Colorize, Icon, IconName, StyledExt, animation::cubic_bezier, h_flex, v_flex,
};

use crate::runtime::ShellRuntime;

#[derive(Clone)]
pub struct ToastEntry {
    pub notification: Notification,
    pub generation: u64,
    pub timeout: Option<Duration>,
}

/// OSD Toast Notification View.
pub struct NotificationToastView {
    pub stack: VecDeque<ToastEntry>,
    pub bar_position: shilpo_config::BarPosition,
    pub expanded: bool,
    pub unfolded_apps: HashSet<String>,
    pub hovered: bool,
    pub entering: bool,
    pub closing: bool,
    pub autohide_task: Option<gpui::Task<()>>,
}

impl NotificationToastView {
    pub fn new(
        notification: Notification,
        generation: u64,
        timeout: Option<Duration>,
        bar_position: shilpo_config::BarPosition,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        window.on_window_should_close(cx, move |_, cx| {
            ShellRuntime::forget_notification(cx, generation);
            true
        });
        let mut stack = VecDeque::new();
        stack.push_front(ToastEntry {
            notification,
            generation,
            timeout,
        });

        let mut this = Self {
            stack,
            bar_position,
            expanded: false,
            unfolded_apps: HashSet::new(),
            hovered: false,
            entering: true,
            closing: false,
            autohide_task: None,
        };
        this.reset_autohide_timer(window, cx);

        cx.spawn_in(window, async move |view, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(250))
                .await;
            _ = view.update_in(cx, |this, _, cx| {
                this.entering = false;
                cx.notify();
            });
        })
        .detach();

        this
    }

    pub fn view(
        notification: Notification,
        generation: u64,
        timeout: Option<Duration>,
        bar_position: shilpo_config::BarPosition,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| Self::new(notification, generation, timeout, bar_position, window, cx))
    }

    pub fn push(
        &mut self,
        notification: Notification,
        generation: u64,
        timeout: Option<Duration>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stack.push_front(ToastEntry {
            notification,
            generation,
            timeout,
        });
        self.reset_autohide_timer(window, cx);
        cx.notify();
    }

    pub fn toggle_unfold_app(
        &mut self,
        app_name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.unfolded_apps.contains(app_name) {
            self.unfolded_apps.remove(app_name);
        } else {
            self.unfolded_apps.insert(app_name.to_string());
        }
        self.reset_autohide_timer(window, cx);
        cx.notify();
    }

    pub fn dismiss_gen(&mut self, generation: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }

        if self.stack.len() > 1 {
            if let Some(pos) = self.stack.iter().position(|e| e.generation == generation) {
                let popped = self.stack.remove(pos);
                if let Some(entry) = popped {
                    ShellRuntime::forget_notification(cx, entry.generation);
                }
            }
            if self.stack.len() <= 1 {
                self.unfolded_apps.clear();
            }
            self.reset_autohide_timer(window, cx);
            cx.notify();
            return;
        }

        self.dismiss(window, cx);
    }

    pub fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }

        if self.stack.len() > 1
            && let Some(popped) = self.stack.pop_front()
        {
            ShellRuntime::forget_notification(cx, popped.generation);
            if self.stack.len() <= 1 {
                self.unfolded_apps.clear();
            }
            self.reset_autohide_timer(window, cx);
            cx.notify();
            return;
        }

        self.closing = true;
        self.autohide_task = None;
        cx.notify();

        let top_gen = self.stack.front().map(|e| e.generation);
        cx.spawn_in(window, async move |_, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(220))
                .await;
            _ = cx.update(|_window, cx| {
                if let Some(target_gen) = top_gen {
                    ShellRuntime::expire_notification(cx, target_gen);
                } else {
                    ShellRuntime::close_active_notification(cx);
                }
            });
        })
        .detach();
    }

    pub fn reset_autohide_timer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.hovered || self.expanded || !self.unfolded_apps.is_empty() || self.closing {
            self.autohide_task = None;
            return;
        }

        let Some(top) = self.stack.front().cloned() else {
            self.autohide_task = None;
            return;
        };

        let Some(timeout) = top.timeout else {
            self.autohide_task = None;
            return;
        };

        let generation = top.generation;
        self.autohide_task = Some(cx.spawn_in(window, async move |view, cx| {
            cx.background_executor().timer(timeout).await;
            _ = view
                .update_in(cx, |this, window, cx| {
                    this.dismiss(window, cx);
                })
                .or_else(|_| {
                    cx.update(|_window, cx| ShellRuntime::expire_notification(cx, generation))
                });
        }));
    }
}

fn render_icon_badge(
    notification: &Notification,
    cx: &Context<NotificationToastView>,
    size_px: f32,
    img_size_px: f32,
) -> gpui::AnyElement {
    let resolved_path = notification
        .desktop_entry
        .as_deref()
        .filter(|s| !s.is_empty())
        .and_then(shilpo_services::applications::icons::lookup_icon)
        .or_else(|| {
            notification
                .app_icon
                .as_deref()
                .filter(|s| {
                    !s.is_empty()
                        && *s != "dialog-information"
                        && *s != "dialog-warning"
                        && *s != "dialog-error"
                        && *s != "notification"
                        && *s != "bell"
                })
                .and_then(shilpo_services::applications::icons::lookup_icon)
        })
        .or_else(|| {
            if !notification.app_name.is_empty() {
                shilpo_services::applications::icons::lookup_icon(&notification.app_name)
            } else {
                None
            }
        })
        .or_else(|| {
            notification
                .app_icon
                .as_deref()
                .filter(|s| !s.is_empty())
                .and_then(shilpo_services::applications::icons::lookup_icon)
        });

    if let Some(path) = resolved_path {
        div()
            .w(px(size_px))
            .h(px(size_px))
            .flex_shrink_0()
            .rounded_2xl()
            .bg(cx.theme().surface_container_highest.opacity(0.5))
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .child(img(path).w(px(img_size_px)).h(px(img_size_px)))
            .into_any_element()
    } else {
        div()
            .w(px(size_px))
            .h(px(size_px))
            .flex_shrink_0()
            .rounded_2xl()
            .bg(cx.theme().primary_container)
            .text_color(cx.theme().on_primary_container)
            .flex()
            .items_center()
            .justify_center()
            .child(Icon::new(IconName::Notifications).size(px(img_size_px)))
            .into_any_element()
    }
}

impl Render for NotificationToastView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.stack.is_empty() {
            return div().id("toast-empty-root");
        }

        let is_bottom = self.bar_position == shilpo_config::BarPosition::Bottom;
        let slide_right = matches!(
            self.bar_position,
            shilpo_config::BarPosition::Top
                | shilpo_config::BarPosition::Bottom
                | shilpo_config::BarPosition::Right
        );
        let closing = self.closing;
        let entering = self.entering;

        let (anim_id, easing, duration) = if closing {
            (
                ElementId::NamedInteger("toast-motion-anim".into(), 2),
                cubic_bezier(0.4, 0.0, 1.0, 1.0),
                Duration::from_millis(220),
            )
        } else if entering {
            (
                ElementId::NamedInteger("toast-motion-anim".into(), 1),
                cubic_bezier(0.05, 0.7, 0.1, 1.0),
                Duration::from_millis(250),
            )
        } else {
            (
                ElementId::NamedInteger("toast-motion-anim".into(), 0),
                cubic_bezier(0.0, 0.0, 1.0, 1.0),
                Duration::from_millis(1),
            )
        };

        let root_flex = if slide_right {
            h_flex()
                .w_full()
                .h_full()
                .when(is_bottom, |this| this.items_end())
                .when(!is_bottom, |this| this.items_start())
                .justify_end()
        } else {
            h_flex()
                .w_full()
                .h_full()
                .when(is_bottom, |this| this.items_end())
                .when(!is_bottom, |this| this.items_start())
                .justify_start()
        };

        // Group notifications strictly by application name
        let mut groups: Vec<(String, Vec<ToastEntry>)> = Vec::new();
        for entry in &self.stack {
            let app_name = if entry.notification.app_name.is_empty() {
                "Notification".to_string()
            } else {
                entry.notification.app_name.clone()
            };
            if let Some(group) = groups.iter_mut().find(|(name, _)| name == &app_name) {
                group.1.push(entry.clone());
            } else {
                groups.push((app_name, vec![entry.clone()]));
            }
        }

        let group_cards = groups.into_iter().enumerate().map(|(g_idx, (app_name, app_entries))| {
            let is_unfolded = self.unfolded_apps.contains(&app_name);
            let count = app_entries.len();
            let app_name_clone1 = app_name.clone();
            let app_name_clone2 = app_name.clone();

            if is_unfolded && count > 1 {
                // Unfolded list for this specific app
                v_flex()
                    .id(ElementId::NamedInteger("toast-app-unfolded".into(), g_idx as u64))
                    .gap_2()
                    .w(px(368.))
                    .children(app_entries.into_iter().enumerate().map(|(idx, entry)| {
                        let notif = &entry.notification;
                        let target_gen = entry.generation;
                        let (card_bg, card_fg) = match notif.urgency {
                            shilpo_services::NotificationUrgency::Critical => (
                                cx.theme().error_container,
                                cx.theme().on_error_container,
                            ),
                            shilpo_services::NotificationUrgency::Normal => (
                                cx.theme().surface_container_high,
                                cx.theme().on_surface,
                            ),
                            shilpo_services::NotificationUrgency::Low => (
                                cx.theme().surface_container,
                                cx.theme().on_surface_variant,
                            ),
                        };
                        let name = if notif.app_name.is_empty() {
                            "Notification".to_string()
                        } else {
                            notif.app_name.clone()
                        };
                        let card_icon = render_icon_badge(notif, cx, 36.0, 26.0);
                        let app_name_collapse = app_name_clone1.clone();
                        let notif_has_image = notif.image_path.is_some();
                        let notif_has_actions = !notif.actions.is_empty();
                        let notif_has_expandable = notif_has_image || notif_has_actions;

                        h_flex()
                            .w(px(368.))
                            .p_3p5()
                            .gap_3()
                            .rounded_3xl()
                            .overflow_hidden()
                            .bg(card_bg)
                            .text_color(card_fg)
                            .border_1()
                            .border_color(cx.theme().outline_variant.opacity(0.3))
                            .shadow_md()
                            .items_start()
                            .child(card_icon)
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_1()
                                    .child(
                                        h_flex()
                                            .w_full()
                                            .items_center()
                                            .justify_between()
                                            .child(
                                                h_flex()
                                                    .gap_1p5()
                                                    .items_center()
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .font_semibold()
                                                            .text_color(cx.theme().primary)
                                                            .child(name),
                                                    )
                                                    .when(idx == 0, |this| {
                                                        this.child(
                                                            div()
                                                                .id(ElementId::NamedInteger("toast-app-collapse-btn".into(), g_idx as u64))
                                                                .role(Role::Button)
                                                                .px_2()
                                                                .py_0p5()
                                                                .rounded_full()
                                                                .bg(cx.theme().primary_container)
                                                                .text_color(cx.theme().on_primary_container)
                                                                .text_xs()
                                                                .font_semibold()
                                                                .cursor_pointer()
                                                                .on_click(cx.listener(move |this, _, window, cx| {
                                                                    this.toggle_unfold_app(&app_name_collapse, window, cx);
                                                                }))
                                                                .child("Collapse"),
                                                        )
                                                    }),
                                            )
                                            .child(
                                                h_flex()
                                                    .flex_shrink_0()
                                                    .gap_1()
                                                    .items_center()
                                                    .when(notif_has_expandable, |this| {
                                                        let unf_toggle_btn = div()
                                                            .id(ElementId::NamedInteger("toast-unf-expand-toggle".into(), (g_idx * 100 + idx) as u64))
                                                            .role(Role::Button)
                                                            .p_1()
                                                            .flex_shrink_0()
                                                            .rounded_full()
                                                            .bg(cx.theme().surface_container.opacity(0.6))
                                                            .text_color(cx.theme().on_surface_variant)
                                                            .cursor_pointer()
                                                            .hover(|s| {
                                                                s.bg(cx.theme().surface_container_highest)
                                                                    .text_color(cx.theme().on_surface)
                                                            })
                                                            .on_click(cx.listener(|this, _, window, cx| {
                                                                this.expanded = !this.expanded;
                                                                this.reset_autohide_timer(window, cx);
                                                                cx.notify();
                                                            }))
                                                            .child(Icon::new(if self.expanded { IconName::KeyboardArrowUp } else { IconName::KeyboardArrowDown }).size(px(16.)));
                                                        this.child(unf_toggle_btn)
                                                    })
                                                    .child(
                                                        div()
                                                            .id(ElementId::NamedInteger("toast-unfolded-close".into(), (g_idx * 100 + idx) as u64))
                                                            .role(Role::Button)
                                                            .p_1()
                                                            .rounded_full()
                                                            .bg(cx.theme().surface_container.opacity(0.6))
                                                            .text_color(cx.theme().on_surface_variant)
                                                            .cursor_pointer()
                                                            .hover(|s| s.bg(cx.theme().surface_container_highest))
                                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                                this.dismiss_gen(target_gen, window, cx);
                                                            }))
                                                            .child(Icon::new(IconName::Close).size(px(14.))),
                                                    ),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .w_full()
                                            .text_sm()
                                            .font_semibold()
                                            .text_color(cx.theme().on_surface)
                                            .child(notif.summary.clone()),
                                    )
                                    .when(!notif.body.is_empty(), |this| {
                                        this.child(
                                            div()
                                                .w_full()
                                                .text_xs()
                                                .text_color(cx.theme().on_surface_variant)
                                                .child(notif.body.clone()),
                                        )
                                    })
                                    .when(notif_has_expandable && self.expanded, |this| {
                                        let unf_actions = notif.actions.clone();
                                        let unf_notif_id = notif.id;
                                        this.child(
                                            v_flex()
                                                .w_full()
                                                .gap_2()
                                                .pt_2()
                                                .when_some(notif.image_path.clone(), |this, img_path| {
                                                    this.child(
                                                        div()
                                                            .w_full()
                                                            .max_h(px(160.))
                                                            .rounded_2xl()
                                                            .overflow_hidden()
                                                            .child(img(img_path).w_full()),
                                                    )
                                                })
                                                .when(!unf_actions.is_empty(), |this| {
                                                    this.child(
                                                        h_flex()
                                                            .flex_wrap()
                                                            .gap_2()
                                                            .items_center()
                                                            .children(unf_actions.into_iter().enumerate().map(|(i, (key, label))| {
                                                                let key_clone = key.clone();
                                                                div()
                                                                    .id(ElementId::NamedInteger("toast-unf-action-btn".into(), (g_idx * 1000 + idx * 10 + i) as u64))
                                                                    .role(Role::Button)
                                                                    .px_3()
                                                                    .py_1()
                                                                    .rounded_full()
                                                                    .bg(cx.theme().primary_container)
                                                                    .text_color(cx.theme().on_primary_container)
                                                                    .text_xs()
                                                                    .font_semibold()
                                                                    .cursor_pointer()
                                                                    .hover(|s| s.bg(cx.theme().primary_container.darken(0.1)))
                                                                    .on_click(cx.listener(move |this, _, window, cx| {
                                                                        ShellRuntime::invoke_notification_action(cx, unf_notif_id, &key_clone);
                                                                        this.dismiss_gen(target_gen, window, cx);
                                                                    }))
                                                                    .child(label)
                                                            }))
                                                    )
                                                })
                                        )
                                    }),
                            )
                    }))
                    .into_any_element()
            } else {
                // Render Deck Stack for this app
                let top_entry = &app_entries[0];
                let notification = &top_entry.notification;
                let top_gen = top_entry.generation;

                let app_icon = render_icon_badge(notification, cx, 36.0, 26.0);
                let (bg, fg) = match notification.urgency {
                    shilpo_services::NotificationUrgency::Critical => (
                        cx.theme().error_container,
                        cx.theme().on_error_container,
                    ),
                    shilpo_services::NotificationUrgency::Normal => (
                        cx.theme().surface_container_high,
                        cx.theme().on_surface,
                    ),
                    shilpo_services::NotificationUrgency::Low => (
                        cx.theme().surface_container,
                        cx.theme().on_surface_variant,
                    ),
                };

                let name = if notification.app_name.is_empty() {
                    "Notification".to_string()
                } else {
                    notification.app_name.clone()
                };

                let toggle_icon = if self.expanded {
                    IconName::KeyboardArrowUp
                } else {
                    IconName::KeyboardArrowDown
                };

                let toggle_btn = div()
                    .id(ElementId::NamedInteger("toast-expand-toggle-btn".into(), g_idx as u64))
                    .role(Role::Button)
                    .aria_label("Toggle notification actions")
                    .p_1()
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(cx.theme().surface_container.opacity(0.6))
                    .text_color(cx.theme().on_surface_variant)
                    .cursor_pointer()
                    .hover(|s| {
                        s.bg(cx.theme().surface_container_highest)
                            .text_color(cx.theme().on_surface)
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.expanded = !this.expanded;
                        this.reset_autohide_timer(window, cx);
                        cx.notify();
                    }))
                    .child(Icon::new(toggle_icon).size(px(16.)));

                let peek_card_2 = if count >= 3 {
                    let app_peek_name = app_name_clone1.clone();
                    Some(
                        div()
                            .id(ElementId::NamedInteger("toast-peek-card-2".into(), g_idx as u64))
                            .absolute()
                            .w(px(328.))
                            .left(px(16.))
                            .bottom(px(-13.))
                            .h(px(24.))
                            .rounded_2xl()
                            .bg(cx.theme().surface_container_lowest)
                            .border_1()
                            .border_color(cx.theme().outline_variant.opacity(0.3))
                            .shadow_sm()
                            .opacity(0.7)
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.toggle_unfold_app(&app_peek_name, window, cx);
                            })),
                    )
                } else {
                    None
                };

                let peek_card_1 = if count >= 2 {
                    let app_peek_name = app_name_clone1.clone();
                    Some(
                        div()
                            .id(ElementId::NamedInteger("toast-peek-card-1".into(), g_idx as u64))
                            .absolute()
                            .w(px(344.))
                            .left(px(8.))
                            .bottom(px(-7.))
                            .h(px(24.))
                            .rounded_2xl()
                            .bg(cx.theme().surface_container)
                            .border_1()
                            .border_color(cx.theme().outline_variant.opacity(0.4))
                            .shadow_md()
                            .opacity(0.9)
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.toggle_unfold_app(&app_peek_name, window, cx);
                            })),
                    )
                } else {
                    None
                };

                let summary = notification.summary.clone();
                let body = notification.body.clone();
                let app_unfold_name = app_name_clone2.clone();
                let has_image = notification.image_path.is_some();
                let has_actions = !notification.actions.is_empty();
                let has_expandable = has_image || has_actions;

                let bottom_margin = if count >= 3 {
                    px(16.)
                } else if count >= 2 {
                    px(10.)
                } else {
                    px(0.)
                };

                v_flex().w(px(368.)).mb(bottom_margin).child(
                    div()
                        .relative()
                        .w(px(368.))
                        .children(peek_card_2)
                        .children(peek_card_1)
                        .child(
                            h_flex()
                                .w(px(368.))
                                .relative()
                                .p_3p5()
                                .gap_3()
                                .rounded_3xl()
                                .overflow_hidden()
                                .bg(bg)
                                .text_color(fg)
                                .border_1()
                                .border_color(cx.theme().outline_variant.opacity(0.3))
                                .shadow_md()
                                .items_start()
                                .child(app_icon)
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .min_w_0()
                                        .gap_1()
                                        .child(
                                            h_flex()
                                                .w_full()
                                                .items_center()
                                                .justify_between()
                                                .child(
                                                    h_flex()
                                                        .gap_1p5()
                                                        .items_center()
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .font_semibold()
                                                                .text_color(cx.theme().primary)
                                                                .child(name),
                                                        )
                                                        .when(count > 1, |this| {
                                                            this.child(
                                                                div()
                                                                    .id(ElementId::NamedInteger("toast-cycle-badge".into(), g_idx as u64))
                                                                    .role(Role::Button)
                                                                    .px_2()
                                                                    .py_0p5()
                                                                    .rounded_full()
                                                                    .bg(cx.theme().primary_container)
                                                                    .text_color(
                                                                        cx.theme().on_primary_container,
                                                                    )
                                                                    .text_xs()
                                                                    .font_semibold()
                                                                    .cursor_pointer()
                                                                    .hover(|s| {
                                                                        s.bg(cx.theme()
                                                                            .primary_container
                                                                            .darken(0.1))
                                                                    })
                                                                    .on_click(cx.listener(
                                                                        move |this, _, window, cx| {
                                                                            this.toggle_unfold_app(
                                                                                &app_unfold_name,
                                                                                window,
                                                                                cx,
                                                                            );
                                                                        },
                                                                    ))
                                                                    .child(format!("+{} more", count - 1)),
                                                            )
                                                        }),
                                                )
                                                .child(
                                                    h_flex()
                                                        .flex_shrink_0()
                                                        .gap_1()
                                                        .items_center()
                                                        .when(has_expandable, |this| this.child(toggle_btn))
                                                        .child(
                                                            div()
                                                                .id(ElementId::NamedInteger("toast-top-close-btn".into(), g_idx as u64))
                                                                .role(Role::Button)
                                                                .p_1()
                                                                .rounded_full()
                                                                .bg(cx.theme().surface_container.opacity(0.6))
                                                                .text_color(cx.theme().on_surface_variant)
                                                                .cursor_pointer()
                                                                .hover(|s| s.bg(cx.theme().surface_container_highest))
                                                                .on_click(cx.listener(move |this, _, window, cx| {
                                                                    this.dismiss_gen(top_gen, window, cx);
                                                                }))
                                                                .child(Icon::new(IconName::Close).size(px(14.))),
                                                        ),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .w_full()
                                                .text_sm()
                                                .font_semibold()
                                                .text_color(cx.theme().on_surface)
                                                .child(summary),
                                        )
                                        .when(!body.is_empty(), |this| {
                                            this.child(
                                                div()
                                                    .w_full()
                                                    .text_xs()
                                                    .text_color(cx.theme().on_surface_variant)
                                                    .child(body),
                                            )
                                        })
                                        .when(has_expandable && self.expanded, |this| {
                                            let actions = notification.actions.clone();
                                            let notif_id = notification.id;
                                            this.child(
                                                v_flex()
                                                    .w_full()
                                                    .gap_2()
                                                    .pt_2()
                                                    .when_some(notification.image_path.clone(), |this, img_path| {
                                                        this.child(
                                                            div()
                                                                .w_full()
                                                                .max_h(px(160.))
                                                                .rounded_2xl()
                                                                .overflow_hidden()
                                                                .child(img(img_path).w_full()),
                                                        )
                                                    })
                                                    .when(has_actions, |this| {
                                                        this.child(
                                                            h_flex()
                                                                .flex_wrap()
                                                                .gap_2()
                                                                .items_center()
                                                                .children(actions.into_iter().enumerate().map(
                                                                    |(i, (key, label))| {
                                                                        let key_clone = key.clone();
                                                                        div()
                                                                            .id(ElementId::NamedInteger("toast-action-btn".into(), (g_idx * 100 + i) as u64))
                                                                            .role(Role::Button)
                                                                            .px_3()
                                                                            .py_1()
                                                                            .rounded_full()
                                                                            .bg(cx.theme().primary_container)
                                                                            .text_color(
                                                                                cx.theme().on_primary_container,
                                                                            )
                                                                            .text_xs()
                                                                            .font_semibold()
                                                                            .cursor_pointer()
                                                                            .hover(|s| {
                                                                                s.bg(cx.theme()
                                                                                    .primary_container
                                                                                    .darken(0.1))
                                                                            })
                                                                            .on_click(cx.listener(
                                                                                move |this, _, window, cx| {
                                                                                    ShellRuntime::invoke_notification_action(
                                                                                        cx,
                                                                                        notif_id,
                                                                                        &key_clone,
                                                                                    );
                                                                                    this.dismiss_gen(top_gen, window, cx);
                                                                                },
                                                                            ))
                                                                            .child(label)
                                                                    },
                                                                )),
                                                        )
                                                    }),
                                            )
                                        }),
                                ),
                        ),
                )
                    .into_any_element()
            }
        });

        let main_card_container = v_flex()
            .id("toast-scroll-container")
            .w(px(368.))
            .max_h(px(400.))
            .overflow_y_scroll()
            .pr_0()
            .gap_3p5()
            .children(group_cards);

        div()
            .id("toast-notification-root")
            .w_full()
            .h_full()
            .on_hover(cx.listener(|this, hovered: &bool, window, cx| {
                if this.hovered != *hovered {
                    this.hovered = *hovered;
                    this.reset_autohide_timer(window, cx);
                    cx.notify();
                }
            }))
            .child(root_flex.child(main_card_container.with_animation(
                anim_id,
                Animation::new(duration).with_easing(easing),
                move |this, delta| {
                    if closing {
                        let opacity = 1.0 - delta;
                        let x_offset = if slide_right {
                            px(120.) * delta
                        } else {
                            px(-120.) * delta
                        };
                        this.shadow_none().opacity(opacity).left(x_offset)
                    } else if entering {
                        let opacity = delta;
                        let x_offset = if slide_right {
                            px(120.) * (1.0 - delta)
                        } else {
                            px(-120.) * (1.0 - delta)
                        };
                        this.shadow_none().opacity(opacity).left(x_offset)
                    } else {
                        this
                    }
                },
            )))
    }
}
