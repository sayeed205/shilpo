use gpui::InteractiveElement;
use gpui::ParentElement;
use gpui::{App, Axis, ElementId, IntoElement, Window};
use gpui::{
    RenderOnce, StatefulInteractiveElement as _, StyleRefinement, Styled, div,
    prelude::FluentBuilder as _,
};
use std::{cell::Cell, rc::Rc};

use crate::{
    Disableable, Sizable, Size, StyledExt,
    button::{Button, ButtonGroupMode, ButtonVariant, ButtonVariants, button_group_tokens},
};

/// A ButtonGroup element, to wrap multiple buttons in a group.
#[derive(IntoElement)]
pub struct ButtonGroup {
    id: ElementId,
    style: StyleRefinement,
    children: Vec<Button>,
    pub(super) multiple: bool,
    pub(super) disabled: bool,
    pub(super) layout: Axis,
    mode: ButtonGroupMode,

    // The button props
    pub(super) compact: bool,
    pub(super) outline: bool,
    pub(super) variant: Option<ButtonVariant>,
    pub(super) size: Option<Size>,

    on_click: Option<Box<dyn Fn(&Vec<usize>, &mut Window, &mut App) + 'static>>,
}

impl Disableable for ButtonGroup {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl ButtonGroup {
    /// Creates a new ButtonGroup.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            children: Vec::new(),
            variant: None,
            size: None,
            compact: false,
            outline: false,
            multiple: false,
            disabled: false,
            layout: Axis::Horizontal,
            mode: ButtonGroupMode::default(),
            on_click: None,
        }
    }

    /// Adds a button as a child to the ButtonGroup.
    pub fn child(mut self, child: Button) -> Self {
        self.children.push(child.disabled(self.disabled));
        self
    }

    /// Adds multiple buttons as children to the ButtonGroup.
    pub fn children(mut self, children: impl IntoIterator<Item = Button>) -> Self {
        self.children.extend(
            children
                .into_iter()
                .map(|child| child.disabled(self.disabled)),
        );
        self
    }

    /// With the multiple selection mode, default is false (single selection).
    pub fn multiple(mut self, multiple: bool) -> Self {
        self.multiple = multiple;
        self
    }

    /// Set the layout of the button group. Default is `Axis::Horizontal`.
    pub fn layout(mut self, layout: Axis) -> Self {
        self.layout = layout;
        self
    }

    pub fn mode(mut self, mode: ButtonGroupMode) -> Self {
        self.mode = mode;
        self
    }

    /// With the compact mode for the ButtonGroup.
    ///
    /// See also: [`Button::compact()`]
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    /// With the outline mode for the ButtonGroup.
    ///
    /// See also: [`Button::outline()`]
    pub fn outline(mut self) -> Self {
        self.outline = true;
        self
    }

    /// Sets the on_click handler for the ButtonGroup.
    ///
    /// The handler first argument is a vector of the selected button indices.
    ///
    /// The `&Vec<usize>` is the indices of the clicked (selected in `multiple` mode) buttons.
    /// For example: `[0, 2, 3]` is means the first, third and fourth buttons are clicked.
    ///
    /// ```ignore
    /// ButtonGroup::new("size-button")
    ///    .child(Button::new("large").label("Large").selected(self.size == Size::Large))
    ///    .child(Button::new("medium").label("Medium").selected(self.size == Size::Medium))
    ///    .child(Button::new("small").label("Small").selected(self.size == Size::Small))
    ///    .on_click(cx.listener(|view, clicks: &Vec<usize>, _, cx| {
    ///        if clicks.contains(&0) {
    ///            view.size = Size::Large;
    ///        } else if clicks.contains(&1) {
    ///            view.size = Size::Medium;
    ///        } else if clicks.contains(&2) {
    ///            view.size = Size::Small;
    ///        }
    ///        cx.notify();
    ///    }))
    /// ```
    pub fn on_click(
        mut self,
        handler: impl Fn(&Vec<usize>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl Sizable for ButtonGroup {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = Some(size.into());
        self
    }
}

impl Styled for ButtonGroup {
    fn style(&mut self) -> &mut gpui::StyleRefinement {
        &mut self.style
    }
}

impl ButtonVariants for ButtonGroup {
    fn with_variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = Some(variant);
        self
    }
}

impl RenderOnce for ButtonGroup {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let children_len = self.children.len();
        let mut selected_ixs: Vec<usize> = Vec::new();
        let state = Rc::new(Cell::new(None));

        for (ix, child) in self.children.iter().enumerate() {
            if child.selected {
                selected_ixs.push(ix);
            }
        }

        let vertical = self.layout == Axis::Vertical;
        let tokens = button_group_tokens::tokens(self.mode);

        div()
            .id(self.id)
            .flex()
            .gap(tokens.spacing)
            .when(vertical, |this| this.flex_col().justify_center())
            .when(!vertical, |this| this.items_center())
            .refine_style(&self.style)
            .children(
                self.children
                    .into_iter()
                    .enumerate()
                    .map(|(child_index, child)| {
                        let state = Rc::clone(&state);
                        let selected = child.selected;
                        let child = child
                            .corner_radii(button_group_tokens::corner_radii(
                                self.mode,
                                self.layout,
                                child_index,
                                children_len,
                                selected,
                            ))
                            .when_some(self.size, |this, size| this.with_size(size))
                            .when(self.size.is_none(), |this| {
                                this.with_size(Size::Size(tokens.height))
                            })
                            .when(selected && self.variant.is_none(), |this| {
                                this.filled_tonal()
                            })
                            .when_some(self.variant, |this, variant| {
                                if selected {
                                    this.with_variant(ButtonVariant::Filled)
                                } else {
                                    this.with_variant(variant)
                                }
                            })
                            .when(self.compact, |this| this.compact())
                            .when(self.outline && !selected, |this| this.outline())
                            .when(self.on_click.is_some(), |this| {
                                this.on_click(move |_, _, _| {
                                    state.set(Some(child_index));
                                })
                            });

                        child
                    }),
            )
            .when_some(
                self.on_click.filter(|_| !self.disabled),
                move |this, on_click| {
                    this.on_click(move |_, window, cx| {
                        let mut selected_ixs = selected_ixs.clone();
                        if let Some(ix) = state.get() {
                            if self.multiple {
                                if let Some(pos) = selected_ixs.iter().position(|&i| i == ix) {
                                    selected_ixs.remove(pos);
                                } else {
                                    selected_ixs.push(ix);
                                }
                            } else {
                                selected_ixs.clear();
                                selected_ixs.push(ix);
                            }
                        }

                        on_click(&selected_ixs, window, cx);
                    })
                },
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::button::button_geometry;
    use gpui::{
        AppContext, Axis, Context, Entity, IntoElement, Render, TestAppContext, VisualTestContext,
        Window, div, px,
    };

    #[gpui::test]
    fn test_button_group_builder(_cx: &mut gpui::TestAppContext) {
        let group = ButtonGroup::new("complex-group")
            .child(Button::new("btn1").label("One"))
            .child(Button::new("btn2").label("Two"))
            .child(Button::new("btn3").label("Three"))
            .filled()
            .large()
            .outline()
            .compact()
            .multiple(true)
            .layout(Axis::Vertical)
            .disabled(false)
            .on_click(|_, _, _| {});

        assert_eq!(group.children.len(), 3);
        assert_eq!(group.variant, Some(ButtonVariant::Filled));
        assert_eq!(group.size, Some(Size::Large));
        assert!(group.outline);
        assert!(group.compact);
        assert!(group.multiple);
        assert_eq!(group.layout, Axis::Vertical);
        assert!(!group.disabled);
        assert!(group.on_click.is_some());
    }

    struct GroupClickState {
        clicks: usize,
    }

    struct GroupClickRoot {
        state: Entity<GroupClickState>,
    }

    impl Render for GroupClickRoot {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let state = self.state.clone();
            div()
                .size_full()
                .debug_selector(|| "button-group-host".to_string())
                .w(px(200.))
                .h(px(40.))
                .child(
                    ButtonGroup::new("button-group")
                        .child(Button::new("one").label("One"))
                        .child(Button::new("two").label("Two"))
                        .on_click(move |_, _, cx| {
                            state.update(cx, |state, _| state.clicks += 1);
                        }),
                )
        }
    }

    #[gpui::test]
    fn rendered_enabled_group_callback_fires_once(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let state = cx.new(|_| GroupClickState { clicks: 0 });
        let state_for_root = state.clone();
        let (_, visual) = cx.add_window_view(move |_, _| GroupClickRoot {
            state: state_for_root,
        });
        let visual: &mut VisualTestContext = visual;
        visual.run_until_parked();
        visual.update(|window, cx| _ = window.draw(cx));
        let bounds = visual
            .debug_bounds("button-group-host")
            .expect("group bounds");
        visual.simulate_click(bounds.center(), Default::default());
        assert_eq!(state.read_with(visual, |state, _| state.clicks), 1);
    }

    struct GroupGeometryRoot {}
    impl Render for GroupGeometryRoot {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().child(
                ButtonGroup::new("button-group")
                    .mode(ButtonGroupMode::Connected)
                    .child(Button::new("one").label("One"))
                    .child(Button::new("two").label("Two")),
            )
        }
    }

    #[gpui::test]
    fn test_connected_button_group_corners(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let _capture = button_geometry::capture_render_geometry();
        let (_, visual) = cx.add_window_view(move |_, _| GroupGeometryRoot {});
        visual.run_until_parked();
        visual.update(|window, cx| _ = window.draw(cx));
        let captured = button_geometry::captured_render_geometry().unwrap();
        assert_eq!(captured.corners.top_left, px(8.));
        assert_eq!(captured.corners.bottom_left, px(8.));
        assert_eq!(captured.corners.top_right, px(20.));
        assert_eq!(captured.corners.bottom_right, px(20.));
    }
}
