use std::{rc::Rc, time::Duration};

use crate::{
    ActiveTheme, Disableable, FocusableExt, IconName, Selectable, Sizable, Size, StyledExt as _,
    icon::IconNamed, text::Text, tooltip::ComponentTooltip, v_flex,
};
use gpui::{
    Animation, AnimationExt, AnyElement, App, Div, ElementId, InteractiveElement, IntoElement,
    ParentElement, RenderOnce, Role, SharedString, StatefulInteractiveElement, StyleRefinement,
    Styled, Toggled, Window, div, prelude::FluentBuilder as _, px, relative, rems, svg,
};

/// A Checkbox element.
#[derive(IntoElement)]
pub struct Checkbox {
    id: ElementId,
    base: Div,
    style: StyleRefinement,
    label: Option<Text>,
    children: Vec<AnyElement>,
    checked: bool,
    disabled: bool,
    size: Size,
    tab_stop: bool,
    tab_index: isize,
    on_click: Option<Rc<dyn Fn(&bool, &mut Window, &mut App) + 'static>>,
    tooltip: ComponentTooltip,
}

impl Checkbox {
    /// Create a new Checkbox with the given id.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            base: div(),
            style: StyleRefinement::default(),
            label: None,
            children: Vec::new(),
            checked: false,
            disabled: false,
            size: Size::default(),
            on_click: None,
            tab_stop: true,
            tab_index: 0,
            tooltip: ComponentTooltip::default(),
        }
    }

    /// Set tooltip text for the checkbox.
    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip.text = Some((tooltip.into(), None));
        self
    }

    /// Set the label for the checkbox.
    pub fn label(mut self, label: impl Into<Text>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set the checked state for the checkbox.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// Set the click handler for the checkbox.
    ///
    /// The `&bool` parameter indicates the new checked state after the click.
    pub fn on_click(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }

    /// Set the tab stop for the checkbox, default is true.
    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }

    /// Set the tab index for the checkbox, default is 0.
    pub fn tab_index(mut self, tab_index: isize) -> Self {
        self.tab_index = tab_index;
        self
    }

    fn handle_click(
        on_click: &Option<Rc<dyn Fn(&bool, &mut Window, &mut App) + 'static>>,
        checked: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        let new_checked = !checked;
        if let Some(f) = on_click {
            (f)(&new_checked, window, cx);
        }
    }
}

impl InteractiveElement for Checkbox {
    fn interactivity(&mut self) -> &mut gpui::Interactivity {
        self.base.interactivity()
    }
}
impl StatefulInteractiveElement for Checkbox {}

impl Styled for Checkbox {
    fn style(&mut self) -> &mut gpui::StyleRefinement {
        &mut self.style
    }
}

impl Disableable for Checkbox {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Selectable for Checkbox {
    fn selected(self, selected: bool) -> Self {
        self.checked(selected)
    }

    fn is_selected(&self) -> bool {
        self.checked
    }
}

impl ParentElement for Checkbox {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Sizable for Checkbox {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

pub(crate) fn checkbox_check_icon(
    id: ElementId,
    size: Size,
    checked: bool,
    disabled: bool,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let toggle_state = window.use_keyed_state(id, cx, |_, _| checked);
    let color = if disabled {
        cx.theme().on_primary.opacity(0.5)
    } else {
        cx.theme().on_primary
    };

    svg()
        .absolute()
        .top_px()
        .left_px()
        .map(|this| match size {
            Size::XSmall => this.size_2(),
            Size::Small => this.size_2p5(),
            Size::Medium => this.size_3(),
            Size::Large => this.size_3p5(),
            _ => this.size_3(),
        })
        .text_color(color)
        .map(|this| match checked {
            true => this.path(IconName::Check.path()),
            _ => this,
        })
        .map(|this| {
            if !disabled && checked != *toggle_state.read(cx) {
                let duration = Duration::from_secs_f64(0.25);
                cx.spawn({
                    let toggle_state = toggle_state.clone();
                    async move |cx| {
                        cx.background_executor().timer(duration).await;
                        _ = toggle_state.update(cx, |this, _| *this = checked);
                    }
                })
                .detach();

                this.with_animation(
                    ElementId::NamedInteger("toggle".into(), checked as u64),
                    Animation::new(Duration::from_secs_f64(0.25)),
                    move |this, delta| {
                        this.opacity(if checked { 1.0 * delta } else { 1.0 - delta })
                    },
                )
                .into_any_element()
            } else {
                this.into_any_element()
            }
        })
}

impl RenderOnce for Checkbox {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let checked = self.checked;

        let focus_handle = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let is_focused = focus_handle.is_focused(window);

        let border_color = if checked {
            cx.theme().primary
        } else {
            cx.theme().surface_container_highest
        };
        let color = if self.disabled {
            border_color.opacity(0.5)
        } else {
            border_color
        };
        let radius = cx.theme().radius.min(px(4.));

        self.base
            .id(self.id.clone())
            .role(Role::CheckBox)
            .aria_toggled(if checked {
                Toggled::True
            } else {
                Toggled::False
            })
            .when_some(
                self.label.as_ref().map(|l| l.get_text(cx)),
                |this, label| this.aria_label(label),
            )
            .when(!self.disabled, |this| {
                this.track_focus(
                    &focus_handle
                        .tab_stop(self.tab_stop)
                        .tab_index(self.tab_index),
                )
            })
            .h_flex()
            .gap_2()
            .items_start()
            .line_height(relative(1.))
            .text_color(cx.theme().on_surface)
            .map(|this| match self.size {
                Size::XSmall => this.text_xs(),
                Size::Small => this.text_sm(),
                Size::Medium => this.text_base(),
                Size::Large => this.text_lg(),
                _ => this,
            })
            .when(self.disabled, |this| {
                this.text_color(cx.theme().on_surface_variant)
            })
            .rounded(cx.theme().radius * 0.5)
            .focus_ring(is_focused, px(2.), window, cx)
            .refine_style(&self.style)
            .child(
                div()
                    .relative()
                    .map(|this| match self.size {
                        Size::XSmall => this.size_3(),
                        Size::Small => this.size_3p5(),
                        Size::Medium => this.size_4(),
                        Size::Large => this.size(rems(1.125)),
                        _ => this.size_4(),
                    })
                    .flex_shrink_0()
                    .border_1()
                    .border_color(color)
                    .rounded(radius)
                    .when(cx.theme().shadow && !self.disabled, |this| this.shadow_xs())
                    .map(|this| match checked {
                        false => this.bg(cx.theme().surface_container_highest),
                        true if self.disabled => this.bg(color),
                        true => this.bg(cx.theme().primary),
                    })
                    .child(checkbox_check_icon(
                        self.id,
                        self.size,
                        checked,
                        self.disabled,
                        window,
                        cx,
                    )),
            )
            .when(self.label.is_some() || !self.children.is_empty(), |this| {
                this.child(
                    v_flex()
                        .flex_1()
                        .overflow_hidden()
                        .line_height(relative(1.2))
                        .gap_1()
                        .map(|this| {
                            if let Some(label) = self.label {
                                this.child(
                                    div()
                                        .size_full()
                                        .text_color(cx.theme().on_surface)
                                        .when(self.disabled, |this| {
                                            this.text_color(cx.theme().on_surface_variant)
                                        })
                                        .line_height(relative(1.))
                                        .child(label),
                                )
                            } else {
                                this
                            }
                        })
                        .children(self.children),
                )
            })
            .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                // Avoid focus on mouse down.
                window.prevent_default();
            })
            .when(!self.disabled, |this| {
                this.on_click({
                    let on_click = self.on_click.clone();
                    move |_, window, cx| {
                        window.prevent_default();
                        Self::handle_click(&on_click, checked, window, cx);
                    }
                })
            })
            .map(|this| self.tooltip.apply(this))
    }
}

#[cfg(test)]
impl Checkbox {
    pub(crate) fn is_checked(&self) -> bool {
        self.checked
    }

    pub(crate) fn is_disabled(&self) -> bool {
        self.disabled
    }

    pub(crate) fn current_size(&self) -> Size {
        self.size
    }

    pub(crate) fn is_tab_stop(&self) -> bool {
        self.tab_stop
    }

    pub(crate) fn current_tab_index(&self) -> isize {
        self.tab_index
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_checkbox_builder() {
        let cb = Checkbox::new("test-cb")
            .checked(true)
            .disabled(true)
            .tab_stop(false)
            .tab_index(5)
            .with_size(Size::Large);

        assert!(cb.is_checked());
        assert!(cb.is_disabled());
        assert!(!cb.is_tab_stop());
        assert_eq!(cb.current_tab_index(), 5);
        assert_eq!(cb.current_size(), Size::Large);
    }

    #[test]
    fn test_checkbox_selectable_trait() {
        let cb = Checkbox::new("test-cb").selected(true);
        assert!(cb.is_selected());
        assert!(cb.is_checked());

        let cb = cb.selected(false);
        assert!(!cb.is_selected());
        assert!(!cb.is_checked());
    }

    #[test]
    fn test_checkbox_sizable_trait() {
        let cb = Checkbox::new("test-cb").with_size(Size::Small);
        assert_eq!(cb.current_size(), Size::Small);
    }

    #[gpui::test]
    fn test_checkbox_handle_click(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        let clicked = Arc::new(Mutex::new(false));
        let clicked_val = Arc::new(Mutex::new(false));

        let clicked_clone = clicked.clone();
        let clicked_val_clone = clicked_val.clone();
        let cb = Checkbox::new("test-cb").on_click(move |val, _, _| {
            *clicked_clone.lock().unwrap() = true;
            *clicked_val_clone.lock().unwrap() = *val;
        });

        cx.update(|window, cx| {
            Checkbox::handle_click(&cb.on_click, false, window, cx);
            assert!(*clicked.lock().unwrap());
            assert!(*clicked_val.lock().unwrap()); // !false -> true
        });

        // Test clicking when checked
        *clicked.lock().unwrap() = false;
        cx.update(|window, cx| {
            Checkbox::handle_click(&cb.on_click, true, window, cx);
            assert!(*clicked.lock().unwrap());
            assert!(!*clicked_val.lock().unwrap()); // !true -> false
        });
    }
}
