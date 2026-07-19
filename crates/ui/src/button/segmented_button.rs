use std::rc::Rc;

use crate::{ActiveTheme, Icon, Sizable, Size, StyledExt};
use gpui::{
    div, prelude::FluentBuilder as _, AnyElement, App, ClickEvent, ElementId, InteractiveElement,
    IntoElement, ParentElement, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    StyleRefinement, Styled, Window,
};

use super::{button_shared_tokens, segmented_button_tokens, shared};

pub struct SegmentedButtonItem {
    id: ElementId,
    label: SharedString,
    icon: Option<Icon>,
    selected: bool,
    checked: bool,
    disabled: bool,
    on_click: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
}

impl SegmentedButtonItem {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            selected: false,
            checked: false,
            disabled: false,
            on_click: None,
        }
    }

    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

#[derive(IntoElement)]
pub struct SingleChoiceSegmentedButton {
    id: ElementId,
    items: Vec<SegmentedButtonItem>,
    style: StyleRefinement,
    on_selection_change: Option<Rc<dyn Fn(usize, &ClickEvent, &mut Window, &mut App)>>,
}

impl SingleChoiceSegmentedButton {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            items: Vec::new(),
            style: StyleRefinement::default(),
            on_selection_change: None,
        }
    }

    pub fn item(mut self, item: SegmentedButtonItem) -> Self {
        self.items.push(item);
        self
    }

    pub fn items(mut self, items: impl IntoIterator<Item = SegmentedButtonItem>) -> Self {
        self.items.extend(items);
        self
    }

    pub fn on_selection_change(
        mut self,
        handler: impl Fn(usize, &ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_selection_change = Some(Rc::new(handler));
        self
    }
}

#[derive(IntoElement)]
pub struct MultiChoiceSegmentedButton {
    id: ElementId,
    items: Vec<SegmentedButtonItem>,
    style: StyleRefinement,
    on_selection_change: Option<Rc<dyn Fn(usize, &ClickEvent, &mut Window, &mut App)>>,
}

impl MultiChoiceSegmentedButton {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            items: Vec::new(),
            style: StyleRefinement::default(),
            on_selection_change: None,
        }
    }

    pub fn item(mut self, item: SegmentedButtonItem) -> Self {
        self.items.push(item);
        self
    }

    pub fn items(mut self, items: impl IntoIterator<Item = SegmentedButtonItem>) -> Self {
        self.items.extend(items);
        self
    }

    pub fn on_selection_change(
        mut self,
        handler: impl Fn(usize, &ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_selection_change = Some(Rc::new(handler));
        self
    }
}

impl Styled for SingleChoiceSegmentedButton {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Styled for MultiChoiceSegmentedButton {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

fn render_item(
    item: SegmentedButtonItem,
    index: usize,
    count: usize,
    multi: bool,
    on_selection_change: Option<Rc<dyn Fn(usize, &ClickEvent, &mut Window, &mut App)>>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let tokens = segmented_button_tokens::tokens(cx);
    let state = button_shared_tokens::STATE_OPACITIES;
    let selected = if multi { item.checked } else { item.selected };
    let disabled = item.disabled;
    let focus = window
        .use_keyed_state(item.id.clone(), cx, |_, cx| cx.focus_handle())
        .read(cx)
        .clone();
    let focused = focus.is_focused(window);
    let content = if selected {
        tokens.selected_content
    } else {
        tokens.content
    };
    let background = if selected {
        tokens.selected_container
    } else {
        cx.theme().transparent
    };
    let hover = shared::interaction::state_layer(background, content, state.hover);
    let pressed = shared::interaction::state_layer(background, content, state.pressed);
    let focus_layer = shared::interaction::state_layer(background, content, state.focus);
    let left = index == 0;
    let right = index + 1 == count;
    let item_handler = item.on_click;

    div()
        .id(item.id)
        .debug_selector(move || format!("segment-{index}"))
        .role(Role::Button)
        .when(multi, |this| {
            this.aria_toggled(if selected {
                gpui::Toggled::True
            } else {
                gpui::Toggled::False
            })
        })
        .when(!multi, |this| this.aria_selected(selected))
        .when(!disabled, |this| this.track_focus(&focus))
        .flex_1()
        .flex()
        .flex_row()
        .h(tokens.height)
        .min_w(button_shared_tokens::COMMON_MIN_WIDTH)
        .items_center()
        .justify_center()
        .gap_2()
        .px_3()
        .border_1()
        .border_color(tokens.border)
        .bg(background)
        .text_color(if disabled {
            content.opacity(button_shared_tokens::DISABLED_CONTENT_OPACITY)
        } else {
            content
        })
        .when(left, |this| {
            this.rounded_tl(tokens.radius).rounded_bl(tokens.radius)
        })
        .when(right, |this| {
            this.rounded_tr(tokens.radius).rounded_br(tokens.radius)
        })
        .when(!left, |this| this.ml(tokens.seam))
        .when(disabled, |this| {
            this.cursor(shared::interaction::cursor(true, false, None))
        })
        .when(disabled, |this| {
            this.bg(cx
                .theme()
                .on_surface
                .opacity(button_shared_tokens::DISABLED_CONTAINER_OPACITY))
        })
        .when(!disabled, |this| {
            this.cursor(shared::interaction::cursor(false, false, None))
                .hover(|this| this.bg(hover))
                .active(|this| this.bg(pressed))
        })
        .when(focused && !disabled, |this| this.bg(focus_layer))
        .when_some(
            item_handler.clone().filter(|_| !disabled),
            |this, handler| {
                let group_handler = on_selection_change.clone();
                this.on_click(move |event, window, cx| {
                    if let Some(group_handler) = group_handler.as_ref() {
                        group_handler(index, event, window, cx);
                    }
                    handler(event, window, cx)
                })
            },
        )
        .when_some(
            on_selection_change.filter(|_| item_handler.is_none() && !disabled),
            |this, handler| {
                this.on_click(move |event, window, cx| handler(index, event, window, cx))
            },
        )
        .when_some(item.icon, |this, icon| {
            this.child(
                div()
                    .w(tokens.icon_constraint)
                    .h(tokens.icon_constraint)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon.with_size(Size::Size(tokens.icon_constraint))),
            )
        })
        .child(item.label)
        .into_any_element()
}

impl RenderOnce for SingleChoiceSegmentedButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let count = self.items.len();
        let on_selection_change = self.on_selection_change;
        div()
            .id(self.id)
            .h_flex()
            .w_full()
            .refine_style(&self.style)
            .children(self.items.into_iter().enumerate().map(|(index, item)| {
                render_item(
                    item,
                    index,
                    count,
                    false,
                    on_selection_change.clone(),
                    window,
                    cx,
                )
            }))
    }
}

impl RenderOnce for MultiChoiceSegmentedButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let count = self.items.len();
        let on_selection_change = self.on_selection_change;
        div()
            .id(self.id)
            .h_flex()
            .w_full()
            .refine_style(&self.style)
            .children(self.items.into_iter().enumerate().map(|(index, item)| {
                render_item(
                    item,
                    index,
                    count,
                    true,
                    on_selection_change.clone(),
                    window,
                    cx,
                )
            }))
    }
}

pub type SegmentedButton = SingleChoiceSegmentedButton;

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{
        div, px, AppContext, Context, Entity, IntoElement, Render,
        TestAppContext, VisualTestContext, Window,
    };

    struct SelectionState {
        selected: usize,
    }

    struct SelectionRoot {
        state: Entity<SelectionState>,
        disable_day: bool,
    }

    impl Render for SelectionRoot {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let selected = self.state.read(cx).selected;
            let state = self.state.clone();
            div().size_full().child(
                SingleChoiceSegmentedButton::new("date-segments")
                    .item(
                        SegmentedButtonItem::new("day", "Day")
                            .selected(selected == 0)
                            .disabled(self.disable_day),
                    )
                    .item(SegmentedButtonItem::new("week", "Week").selected(selected == 1))
                    .item(SegmentedButtonItem::new("month", "Month").selected(selected == 2))
                    .on_selection_change(move |index, _, _, cx| {
                        state.update(cx, |state, _| state.selected = index);
                    }),
            )
        }
    }

    fn selection_root<'a>(
        cx: &'a mut TestAppContext,
        disable_day: bool,
    ) -> (Entity<SelectionState>, &'a mut VisualTestContext) {
        cx.update(crate::init);
        let state = cx.new(|_| SelectionState { selected: 0 });
        let state_for_root = state.clone();
        let (_, visual) = cx.add_window_view(move |_, _| SelectionRoot {
            state: state_for_root,
            disable_day,
        });
        (state, visual)
    }

    fn draw(visual: &mut VisualTestContext) {
        visual.run_until_parked();
        visual.update(|window, cx| {
            _ = window.draw(cx);
        });
    }

    #[test]
    fn selection_mapping_is_controlled_per_item() {
        let single = SegmentedButtonItem::new("single", "One").selected(true);
        let multi = SegmentedButtonItem::new("multi", "One").checked(true);
        assert!(single.selected);
        assert!(multi.checked);
    }

    #[test]
    fn rows_store_equal_width_segments() {
        let row = SingleChoiceSegmentedButton::new("row")
            .item(SegmentedButtonItem::new("a", "A"))
            .item(SegmentedButtonItem::new("b", "B"));
        assert_eq!(row.items.len(), 2);
    }

    #[gpui::test]
    fn rendered_segment_selection_and_connected_geometry(cx: &mut TestAppContext) {
        let (state, visual) = selection_root(cx, false);
        draw(visual);
        let day = visual.debug_bounds("segment-0").expect("day bounds");
        let week = visual.debug_bounds("segment-1").expect("week bounds");
        let month = visual.debug_bounds("segment-2").expect("month bounds");
        assert!((day.size.width.as_f32() - week.size.width.as_f32()).abs() < 1.);
        assert!((week.size.width.as_f32() - month.size.width.as_f32()).abs() < 1.);
        assert!(week.origin.x <= day.right());
        assert!(month.origin.x <= week.right());
        assert_eq!(day.size.height, px(40.));

        visual.simulate_click(month.center(), Default::default());
        assert_eq!(state.read_with(visual, |state, _| state.selected), 2);
        assert!(visual.debug_bounds("segment-2").is_some());
    }

    #[gpui::test]
    fn rendered_disabled_segment_does_not_change_controlled_selection(cx: &mut TestAppContext) {
        let (state, visual) = selection_root(cx, true);
        draw(visual);
        let day = visual.debug_bounds("segment-0").unwrap();
        visual.simulate_click(day.center(), Default::default());
        assert_eq!(state.read_with(visual, |state, _| state.selected), 0);
    }
}
