use std::rc::Rc;

use gpui::{
    AnyElement, App, Axis, Div, ElementId, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, Role, SharedString, StatefulInteractiveElement, StyleRefinement, Styled, Window,
    div, prelude::FluentBuilder, px, relative,
};

use crate::{
    ActiveTheme, AxisExt, FocusableExt as _, Sizable, Size, StyledExt, h_flex, text::Text,
    tooltip::ComponentTooltip, v_flex,
};

/// A Radio element.
///
/// This is not included the Radio group implementation, you can manage the group by yourself.
#[derive(IntoElement)]
pub struct Radio {
    base: Div,
    style: StyleRefinement,
    id: ElementId,
    label: Option<Text>,
    children: Vec<AnyElement>,
    checked: bool,
    disabled: bool,
    tab_stop: bool,
    tab_index: isize,
    size: Size,
    on_click: Option<Rc<dyn Fn(&bool, &mut Window, &mut App) + 'static>>,
    tooltip: ComponentTooltip,
    position_in_set: Option<usize>,
    size_of_set: Option<usize>,
}

impl Radio {
    /// Create a new Radio element with the given id.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            base: div(),
            style: StyleRefinement::default(),
            label: None,
            children: Vec::new(),
            checked: false,
            disabled: false,
            tab_index: 0,
            tab_stop: true,
            size: Size::default(),
            on_click: None,
            tooltip: ComponentTooltip::default(),
            position_in_set: None,
            size_of_set: None,
        }
    }

    /// Set tooltip text for the radio.
    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip.text = Some((tooltip.into(), None));
        self
    }

    /// Set the label of the Radio element.
    pub fn label(mut self, label: impl Into<Text>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set the checked state of the Radio element, default is `false`.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// Set the disabled state of the Radio element, default is `false`.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Set the tab index for the Radio element, default is `0`.
    pub fn tab_index(mut self, tab_index: isize) -> Self {
        self.tab_index = tab_index;
        self
    }

    /// Set the tab stop for the Radio element, default is `true`.
    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }

    /// Add on_click handler when the Radio is clicked.
    ///
    /// The `&bool` parameter is the **new checked state**.
    pub fn on_click(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
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

impl Sizable for Radio {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl Styled for Radio {
    fn style(&mut self) -> &mut gpui::StyleRefinement {
        &mut self.style
    }
}

impl InteractiveElement for Radio {
    fn interactivity(&mut self) -> &mut gpui::Interactivity {
        self.base.interactivity()
    }
}

impl StatefulInteractiveElement for Radio {}

impl ParentElement for Radio {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Radio {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let checked = self.checked;
        let focus_handle = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let is_focused = focus_handle.is_focused(window);
        let disabled = self.disabled;

        let (border_color, circle_color) = if disabled {
            let color = cx.theme().on_surface.opacity(0.38);
            (color, color)
        } else if checked {
            (cx.theme().primary, cx.theme().primary)
        } else {
            (cx.theme().on_surface_variant, cx.theme().transparent)
        };

        let outer_size = match self.size {
            Size::XSmall => gpui::px(14.),
            Size::Small => gpui::px(16.),
            Size::Medium => gpui::px(20.),
            Size::Large => gpui::px(24.),
            _ => gpui::px(20.),
        };
        let inner_size = outer_size * 0.5;

        let group_name = SharedString::from(format!("radio-group-{}", self.id));

        self.base
            .id(self.id.clone())
            .role(Role::RadioButton)
            .aria_selected(self.checked)
            .when_some(
                self.label.as_ref().map(|l| l.get_text(cx)),
                |this, label| this.aria_label(label),
            )
            .when_some(self.position_in_set, |this, pos| {
                this.aria_position_in_set(pos)
            })
            .when_some(self.size_of_set, |this, size| this.aria_size_of_set(size))
            .when(!self.disabled, |this| {
                this.cursor_pointer().track_focus(
                    &focus_handle
                        .tab_stop(self.tab_stop)
                        .tab_index(self.tab_index),
                )
            })
            .group(group_name.clone())
            .h_flex()
            .gap_x_2()
            .text_color(cx.theme().on_surface)
            .items_start()
            .line_height(relative(1.))
            .rounded(cx.theme().radius * 0.5)
            .focus_ring(is_focused, px(2.), window, cx)
            .map(|this| match self.size {
                Size::XSmall => this.text_xs(),
                Size::Small => this.text_sm(),
                Size::Medium => this.text_base(),
                Size::Large => this.text_lg(),
                _ => this,
            })
            .refine_style(&self.style)
            .child(
                div()
                    .size(outer_size)
                    .flex_shrink_0()
                    .relative()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .absolute()
                            .size(outer_size + px(16.))
                            .rounded_full()
                            .when(!disabled, |this| {
                                let layer_color = if checked {
                                    cx.theme().primary
                                } else {
                                    cx.theme().on_surface
                                };
                                this.group_hover(group_name.clone(), |style| {
                                    style.bg(layer_color.opacity(0.08))
                                })
                            }),
                    )
                    .child(
                        div()
                            .size(outer_size)
                            .rounded_full()
                            .border_2()
                            .border_color(border_color)
                            .bg(cx.theme().transparent)
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(checked, |this| {
                                this.child(div().size(inner_size).rounded_full().bg(circle_color))
                            }),
                    ),
            )
            .when(!self.children.is_empty() || self.label.is_some(), |this| {
                this.child(
                    v_flex()
                        .w_full()
                        .line_height(relative(1.2))
                        .gap_1()
                        .when_some(self.label, |this, label| {
                            this.child(
                                div()
                                    .size_full()
                                    .line_height(relative(1.))
                                    .when(self.disabled, |this| {
                                        this.text_color(cx.theme().on_surface_variant)
                                    })
                                    .child(label),
                            )
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

/// A Radio group element.
#[derive(IntoElement)]
pub struct RadioGroup {
    id: ElementId,
    style: StyleRefinement,
    radios: Vec<Radio>,
    layout: Axis,
    selected_index: Option<usize>,
    disabled: bool,
    on_click: Option<Rc<dyn Fn(&usize, &mut Window, &mut App) + 'static>>,
}

impl RadioGroup {
    fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default().flex_1(),
            on_click: None,
            layout: Axis::Vertical,
            selected_index: None,
            disabled: false,
            radios: vec![],
        }
    }

    /// Create a new Radio group with default Vertical layout.
    pub fn vertical(id: impl Into<ElementId>) -> Self {
        Self::new(id)
    }

    /// Create a new Radio group with Horizontal layout.
    pub fn horizontal(id: impl Into<ElementId>) -> Self {
        Self::new(id).layout(Axis::Horizontal)
    }

    /// Set the layout of the Radio group. Default is `Axis::Vertical`.
    pub fn layout(mut self, layout: Axis) -> Self {
        self.layout = layout;
        self
    }

    // Add on_click handler when selected index changes.
    //
    // The `&usize` parameter is the selected index.
    pub fn on_click(mut self, handler: impl Fn(&usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }

    /// Set the selected index.
    pub fn selected_index(mut self, index: Option<usize>) -> Self {
        self.selected_index = index;
        self
    }

    /// Set the disabled state.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Add a child Radio element.
    pub fn child(mut self, child: impl Into<Radio>) -> Self {
        self.radios.push(child.into());
        self
    }

    /// Add multiple child Radio elements.
    pub fn children(mut self, children: impl IntoIterator<Item = impl Into<Radio>>) -> Self {
        self.radios.extend(children.into_iter().map(Into::into));
        self
    }
}

impl Styled for RadioGroup {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl From<&'static str> for Radio {
    fn from(label: &'static str) -> Self {
        Self::new(label).label(label)
    }
}

impl From<SharedString> for Radio {
    fn from(label: SharedString) -> Self {
        Self::new(label.clone()).label(label)
    }
}

impl From<String> for Radio {
    fn from(label: String) -> Self {
        Self::new(SharedString::from(label.clone())).label(SharedString::from(label))
    }
}

impl RenderOnce for RadioGroup {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let on_click = self.on_click;
        let disabled = self.disabled;
        let selected_ix = self.selected_index;

        let base = if self.layout.is_vertical() {
            v_flex()
        } else {
            h_flex().w_full().flex_wrap()
        };

        let total = self.radios.len();
        let mut container = div().id(self.id).role(Role::RadioGroup);
        *container.style() = self.style;

        container.child(
            base.gap_3()
                .children(self.radios.into_iter().enumerate().map(|(ix, mut radio)| {
                    let checked = selected_ix == Some(ix);

                    radio.id = ix.into();
                    radio.position_in_set = Some(ix + 1);
                    radio.size_of_set = Some(total);
                    radio.disabled(disabled).checked(checked).when_some(
                        on_click.clone(),
                        |this, on_click| {
                            this.on_click(move |_, window, cx| {
                                on_click(&ix, window, cx);
                            })
                        },
                    )
                })),
        )
    }
}

#[cfg(test)]
impl Radio {
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

    #[allow(dead_code)]
    pub(crate) fn current_position_in_set(&self) -> Option<usize> {
        self.position_in_set
    }

    #[allow(dead_code)]
    pub(crate) fn current_size_of_set(&self) -> Option<usize> {
        self.size_of_set
    }
}

#[cfg(test)]
impl RadioGroup {
    pub(crate) fn get_layout(&self) -> Axis {
        self.layout
    }

    pub(crate) fn get_selected_index(&self) -> Option<usize> {
        self.selected_index
    }

    pub(crate) fn is_disabled(&self) -> bool {
        self.disabled
    }

    pub(crate) fn radios_count(&self) -> usize {
        self.radios.len()
    }

    pub(crate) fn get_radio(&self, ix: usize) -> &Radio {
        &self.radios[ix]
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[test]
    fn test_radio_builder() {
        let radio = Radio::new("test-r")
            .checked(true)
            .disabled(true)
            .tab_stop(false)
            .tab_index(2)
            .with_size(Size::Large);

        assert!(radio.is_checked());
        assert!(radio.is_disabled());
        assert!(!radio.is_tab_stop());
        assert_eq!(radio.current_tab_index(), 2);
        assert_eq!(radio.current_size(), Size::Large);
    }

    #[test]
    fn test_radio_conversions() {
        // From<&'static str>
        let r1: Radio = "label-1".into();
        assert_eq!(r1.id.to_string(), "label-1");

        // From<SharedString>
        let s = SharedString::from("label-2");
        let r2: Radio = s.into();
        assert_eq!(r2.id.to_string(), "label-2");

        // From<String>
        let r3: Radio = String::from("label-3").into();
        assert_eq!(r3.id.to_string(), "label-3");
    }

    #[test]
    fn test_radio_group_builder() {
        let group = RadioGroup::vertical("test-rg")
            .selected_index(Some(1))
            .disabled(true)
            .child("Option A")
            .children(vec!["Option B", "Option C"]);

        assert_eq!(group.get_layout(), Axis::Vertical);
        assert_eq!(group.get_selected_index(), Some(1));
        assert!(group.is_disabled());
        assert_eq!(group.radios_count(), 3);
        assert_eq!(group.get_radio(0).id.to_string(), "Option A");
        assert_eq!(group.get_radio(1).id.to_string(), "Option B");

        let h_group = RadioGroup::horizontal("test-h");
        assert_eq!(h_group.get_layout(), Axis::Horizontal);
    }

    #[gpui::test]
    fn test_radio_handle_click(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        let clicked = Arc::new(Mutex::new(false));
        let clicked_val = Arc::new(Mutex::new(false));

        let clicked_clone = clicked.clone();
        let clicked_val_clone = clicked_val.clone();
        let radio = Radio::new("test-r").on_click(move |val, _, _| {
            *clicked_clone.lock().unwrap() = true;
            *clicked_val_clone.lock().unwrap() = *val;
        });

        cx.update(|window, cx| {
            Radio::handle_click(&radio.on_click, false, window, cx);
            assert!(*clicked.lock().unwrap());
            assert!(*clicked_val.lock().unwrap()); // !false -> true
        });
    }
}
