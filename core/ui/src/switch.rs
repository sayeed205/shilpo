use crate::{
    ActiveTheme, Disableable, Icon, IconName, Side, Sizable, Size, StyledExt,
    animation::cubic_bezier, h_flex, text::Text, tooltip::ComponentTooltip,
};
use gpui::{
    Animation, AnimationExt as _, App, ElementId, Hsla, InteractiveElement, IntoElement,
    ParentElement as _, RenderOnce, SharedString, StyleRefinement, Styled, Window, div,
    prelude::FluentBuilder as _, px,
};
use std::{rc::Rc, time::Duration};

/// A Switch element that can be toggled on or off.
#[derive(IntoElement)]
pub struct Switch {
    id: ElementId,
    style: StyleRefinement,
    checked: bool,
    disabled: bool,
    label: Option<Text>,
    label_side: Side,
    on_click: Option<Rc<dyn Fn(&bool, &mut Window, &mut App)>>,
    size: Size,
    color: Option<Hsla>,
    tooltip: ComponentTooltip,
    checked_icon: Option<IconName>,
    unchecked_icon: Option<IconName>,
}

impl Switch {
    /// Create a new Switch element.
    pub fn new(id: impl Into<ElementId>) -> Self {
        let id: ElementId = id.into();
        Self {
            id: id.clone(),
            style: StyleRefinement::default(),
            checked: false,
            disabled: false,
            label: None,
            on_click: None,
            label_side: Side::Right,
            size: Size::Medium,
            color: None,
            tooltip: ComponentTooltip::default(),
            checked_icon: None,
            unchecked_icon: None,
        }
    }

    /// Convenience builder to toggle default icon behavior.
    /// When set to `true`, sets `checked_icon = Some(IconName::Check)` and `unchecked_icon = None`.
    pub fn show_icons(mut self, show: bool) -> Self {
        if show {
            self.checked_icon = Some(IconName::Check);
            self.unchecked_icon = None;
        } else {
            self.checked_icon = None;
            self.unchecked_icon = None;
        }
        self
    }

    /// Set an icon to display inside the thumb when checked.
    pub fn checked_icon(mut self, icon: impl Into<Option<IconName>>) -> Self {
        self.checked_icon = icon.into();
        self
    }

    /// Set an icon to display inside the thumb when unchecked.
    pub fn unchecked_icon(mut self, icon: impl Into<Option<IconName>>) -> Self {
        self.unchecked_icon = icon.into();
        self
    }

    /// Set the checked state of the switch.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// Set the label of the switch.
    pub fn label(mut self, label: impl Into<Text>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Add a click handler for the switch.
    pub fn on_click<F>(mut self, handler: F) -> Self
    where
        F: Fn(&bool, &mut Window, &mut App) + 'static,
    {
        self.on_click = Some(Rc::new(handler));
        self
    }

    /// Set the background color of the switch when checked.
    /// Defaults to `cx.theme().primary`.
    pub fn color(mut self, color: impl Into<Hsla>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Set tooltip text for the switch.
    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip.text = Some((tooltip.into(), None));
        self
    }
}

impl Styled for Switch {
    fn style(&mut self) -> &mut gpui::StyleRefinement {
        &mut self.style
    }
}

impl Sizable for Switch {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl Disableable for Switch {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for Switch {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let checked = self.checked;
        let on_click = self.on_click.clone();
        let toggle_state = window.use_keyed_state(self.id.clone(), cx, |_, _| checked);
        let prev_checked = toggle_state.read(cx);

        let (bg_width, bg_height, unchecked_thumb_size, checked_thumb_size, icon_size) =
            match self.size {
                Size::XSmall => (px(36.), px(20.), px(10.), px(14.), px(8.)),
                Size::Small => (px(44.), px(24.), px(12.), px(18.), px(10.)),
                Size::Medium | Size::Large => (px(52.), px(32.), px(16.), px(24.), px(12.)),
                Size::Size(height) => {
                    let h = height.as_f32();
                    let scale = h / 32.0;
                    (
                        px(52.0 * scale),
                        px(h),
                        px(16.0 * scale),
                        px(24.0 * scale),
                        px(12.0 * scale),
                    )
                }
            };

        let unchecked_thumb_size = if self.unchecked_icon.is_some() {
            checked_thumb_size
        } else {
            unchecked_thumb_size
        };

        let checked_bg = self.color.unwrap_or(cx.theme().primary);
        let checked_border = checked_bg;

        let (bg, border_color, toggle_bg) = if self.disabled {
            if checked {
                (
                    checked_bg.opacity(0.12),
                    checked_border.opacity(0.12),
                    cx.theme().on_primary.opacity(0.38),
                )
            } else {
                (
                    cx.theme().surface_container_highest.opacity(0.12),
                    cx.theme().outline.opacity(0.12),
                    cx.theme().outline.opacity(0.38),
                )
            }
        } else {
            if checked {
                (checked_bg, checked_border, cx.theme().on_primary)
            } else {
                (
                    cx.theme().surface_container_highest,
                    cx.theme().outline,
                    cx.theme().outline,
                )
            }
        };

        let usable_width = bg_width - px(4.);
        let usable_height = bg_height - px(4.);

        let x_unchecked = (usable_height - unchecked_thumb_size) / 2.;
        let y_unchecked = (usable_height - unchecked_thumb_size) / 2.;

        let x_checked =
            usable_width - checked_thumb_size - (usable_height - checked_thumb_size) / 2.;
        let y_checked = (usable_height - checked_thumb_size) / 2.;

        let duration = Duration::from_millis(200);

        let active_icon = if checked {
            self.checked_icon
        } else {
            self.unchecked_icon
        };
        let icon_color = if checked {
            cx.theme().primary
        } else {
            cx.theme().surface_container_highest
        };
        let icon_element = active_icon
            .map(|icon_name| Icon::new(icon_name).size(icon_size).text_color(icon_color));

        let thumb_element = div()
            .absolute()
            .rounded_full()
            .bg(toggle_bg)
            .shadow_md()
            .flex()
            .items_center()
            .justify_center()
            .when_some(icon_element, |this, icon| this.child(icon))
            .map(|this| {
                let static_size = if checked {
                    checked_thumb_size
                } else {
                    unchecked_thumb_size
                };
                let static_x = if checked { x_checked } else { x_unchecked };
                let static_y = if checked { y_checked } else { y_unchecked };
                let this = this.size(static_size).left(static_x).top(static_y);

                if !self.disabled && *prev_checked != checked {
                    cx.spawn({
                        let toggle_state = toggle_state.clone();
                        async move |cx| {
                            cx.background_executor().timer(duration).await;
                            _ = toggle_state.update(cx, |this, cx| {
                                *this = checked;
                                cx.notify();
                            });
                        }
                    })
                    .detach();

                    let animation =
                        Animation::new(duration).with_easing(cubic_bezier(0.2, 0.0, 0.0, 1.0));
                    this.with_animation(
                        ElementId::NamedInteger("move_thumb".into(), checked as u64),
                        animation,
                        move |this, delta| {
                            let size_from = if checked {
                                unchecked_thumb_size
                            } else {
                                checked_thumb_size
                            };
                            let size_to = if checked {
                                checked_thumb_size
                            } else {
                                unchecked_thumb_size
                            };
                            let x_from = if checked { x_unchecked } else { x_checked };
                            let x_to = if checked { x_checked } else { x_unchecked };
                            let y_from = if checked { y_unchecked } else { y_checked };
                            let y_to = if checked { y_checked } else { y_unchecked };

                            let current_size = size_from + (size_to - size_from) * delta;
                            let current_x = x_from + (x_to - x_from) * delta;
                            let current_y = y_from + (y_to - y_from) * delta;
                            this.size(current_size).left(current_x).top(current_y)
                        },
                    )
                    .into_any_element()
                } else {
                    this.into_any_element()
                }
            });

        div().refine_style(&self.style).child(
            h_flex()
                .id(self.id.clone())
                .gap_2()
                .items_center()
                .when(!self.disabled, |this| this.cursor_pointer())
                .when(self.label_side.is_left(), |this| this.flex_row_reverse())
                .child(
                    // Switch Track
                    div()
                        .relative()
                        .w(bg_width)
                        .h(bg_height)
                        .rounded_full()
                        .border_2()
                        .border_color(border_color)
                        .bg(bg)
                        .child(thumb_element),
                )
                .when_some(self.label, |this, label| {
                    let label_color = if self.disabled {
                        cx.theme().on_surface.opacity(0.38)
                    } else {
                        cx.theme().on_surface
                    };
                    this.child(div().text_color(label_color).child(label).map(
                        |this| match self.size {
                            Size::XSmall => this.text_xs(),
                            Size::Small => this.text_sm(),
                            _ => this.text_base(),
                        },
                    ))
                })
                .when_some(
                    on_click
                        .as_ref()
                        .map(|c| c.clone())
                        .filter(|_| !self.disabled),
                    |this, on_click| {
                        let toggle_state = toggle_state.clone();
                        this.on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                            cx.stop_propagation();
                            _ = toggle_state.update(cx, |this, _| *this = checked);
                            on_click(&!checked, window, cx);
                        })
                    },
                ),
        )
    }
}

#[cfg(test)]
impl Switch {
    pub(crate) fn is_checked(&self) -> bool {
        self.checked
    }

    pub(crate) fn is_disabled(&self) -> bool {
        self.disabled
    }

    pub(crate) fn current_size(&self) -> Size {
        self.size
    }

    pub(crate) fn get_label_side(&self) -> Side {
        self.label_side
    }

    pub(crate) fn get_checked_icon(&self) -> Option<IconName> {
        self.checked_icon
    }

    pub(crate) fn get_unchecked_icon(&self) -> Option<IconName> {
        self.unchecked_icon
    }

    pub(crate) fn get_color(&self) -> Option<Hsla> {
        self.color
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_switch_builder() {
        let switch = Switch::new("test-sw")
            .checked(true)
            .disabled(true)
            .color(gpui::red())
            .with_size(Size::Small);

        assert!(switch.is_checked());
        assert!(switch.is_disabled());
        assert_eq!(switch.current_size(), Size::Small);
        assert_eq!(switch.get_color(), Some(gpui::red()));
        assert_eq!(switch.get_label_side(), Side::Right);
    }

    #[test]
    fn test_switch_show_icons() {
        // show_icons(true) -> checked_icon: Check, unchecked_icon: None
        let switch = Switch::new("test-sw").show_icons(true);
        assert_eq!(switch.get_checked_icon(), Some(IconName::Check));
        assert_eq!(switch.get_unchecked_icon(), None);

        // show_icons(false) -> checked_icon: None, unchecked_icon: None
        let switch = switch.show_icons(false);
        assert_eq!(switch.get_checked_icon(), None);
        assert_eq!(switch.get_unchecked_icon(), None);
    }

    #[test]
    fn test_switch_checked_unchecked_icons() {
        let switch = Switch::new("test-sw")
            .checked_icon(IconName::Check)
            .unchecked_icon(IconName::Remove);

        assert_eq!(switch.get_checked_icon(), Some(IconName::Check));
        assert_eq!(switch.get_unchecked_icon(), Some(IconName::Remove));
    }

    #[gpui::test]
    fn test_switch_click_handler(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        let clicked = Arc::new(Mutex::new(false));
        let clicked_val = Arc::new(Mutex::new(false));

        let clicked_clone = clicked.clone();
        let clicked_val_clone = clicked_val.clone();
        let switch = Switch::new("test-sw").on_click(move |val, _, _| {
            *clicked_clone.lock().unwrap() = true;
            *clicked_val_clone.lock().unwrap() = *val;
        });

        // Simulating the action on click handler closure directly
        let on_click = switch.on_click.unwrap();
        cx.update(|window, cx| {
            on_click(&true, window, cx);
            assert!(*clicked.lock().unwrap());
            assert!(*clicked_val.lock().unwrap());
        });
    }
}
