use std::{cell::Cell, rc::Rc, time::Duration};

use gpui::{
    Animation, AnimationExt as _, App, ClickEvent, ElementId, IntoElement, ParentElement,
    RenderOnce, StyleRefinement, Styled, Window, div, px, radians,
};

use crate::{
    Icon, IconName, StyledExt as _, button::IconButton, motion::SpringSpec,
    navigation_rail::motion::spring_progress,
};

const ICON_MORPH_DURATION: Duration = Duration::from_millis(400);
const ICON_TURN_RADIANS: f32 = std::f32::consts::PI / 10.;

#[derive(Clone)]
struct MenuIconMotionState {
    collapsed: bool,
    from: f32,
    current: Rc<Cell<f32>>,
    active_generation: Rc<Cell<u64>>,
    generation: u64,
    active: bool,
}

impl MenuIconMotionState {
    fn new(collapsed: bool) -> Self {
        let progress = if collapsed { 0. } else { 1. };
        Self {
            collapsed,
            from: progress,
            current: Rc::new(Cell::new(progress)),
            active_generation: Rc::new(Cell::new(0)),
            generation: 0,
            active: false,
        }
    }

    fn retarget(&mut self, collapsed: bool) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.collapsed = collapsed;
        self.from = self.current.get();
        self.active = true;
        self.active_generation.set(self.generation);
        self.generation
    }
}

/// M3 Expressive menu control for expanding or collapsing a navigation rail.
///
/// The button cross-morphs the Material `menu` and `menu_open` assets while
/// preserving [`IconButton`]'s focus, ripple, and pointer interaction behavior.
#[derive(IntoElement)]
pub struct NavigationRailMenuButton {
    id: ElementId,
    collapsed: bool,
    on_click: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
    style: StyleRefinement,
}

impl NavigationRailMenuButton {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            collapsed: true,
            on_click: None,
            style: StyleRefinement::default(),
        }
    }

    /// Sets whether the associated navigation rail is collapsed.
    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    /// Sets the click handler for toggling the associated navigation rail.
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl Styled for NavigationRailMenuButton {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for NavigationRailMenuButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let motion_key = format!("navigation-rail-menu-motion:{}", self.id);
        let menu_animation_name = format!("navigation-rail-menu-icon:{}", self.id);
        let open_animation_name = format!("navigation-rail-menu-open-icon:{}", self.id);
        let motion = window.use_keyed_state(motion_key, cx, |_, _| {
            MenuIconMotionState::new(self.collapsed)
        });
        let snapshot = motion.read(cx).clone();

        if snapshot.collapsed != self.collapsed {
            let generation = motion.update(cx, |state, _| state.retarget(self.collapsed));
            let motion = motion.clone();
            let collapsed = self.collapsed;
            cx.spawn(async move |cx| {
                cx.background_executor().timer(ICON_MORPH_DURATION).await;
                _ = motion.update(cx, |state, cx| {
                    if state.generation == generation {
                        state.active = false;
                        state.current.set(if collapsed { 0. } else { 1. });
                        cx.notify();
                    }
                });
            })
            .detach();
        }

        let state = motion.read(cx);
        let target = if self.collapsed { 0. } else { 1. };
        let generation = state.generation;
        let from = state.from;
        let current = state.current.clone();
        let active_generation = state.active_generation.clone();

        let icons = if state.active {
            let menu_current = current.clone();
            let menu_generation = active_generation.clone();
            let menu = centered_icon_layer(Icon::new(IconName::Menu).size(px(24.)).with_animation(
                ElementId::NamedInteger(menu_animation_name.into(), generation),
                Animation::new(ICON_MORPH_DURATION),
                move |icon, delta| {
                    let spring = spring_progress(
                        delta,
                        ICON_MORPH_DURATION,
                        SpringSpec::EXPRESSIVE_FAST_SPATIAL,
                    );
                    let progress = from + (target - from) * spring;
                    if menu_generation.get() == generation {
                        menu_current.set(progress);
                    }
                    icon.opacity(1. - progress.clamp(0., 1.))
                        .rotate(radians(-ICON_TURN_RADIANS * progress))
                },
            ))
            .into_any_element();

            let open =
                centered_icon_layer(Icon::new(IconName::MenuOpen).size(px(24.)).with_animation(
                    ElementId::NamedInteger(open_animation_name.into(), generation),
                    Animation::new(ICON_MORPH_DURATION),
                    move |icon, delta| {
                        let spring = spring_progress(
                            delta,
                            ICON_MORPH_DURATION,
                            SpringSpec::EXPRESSIVE_FAST_SPATIAL,
                        );
                        let progress = from + (target - from) * spring;
                        icon.opacity(progress.clamp(0., 1.))
                            .rotate(radians(ICON_TURN_RADIANS * (1. - progress)))
                    },
                ))
                .into_any_element();

            vec![menu, open]
        } else {
            vec![
                centered_icon_layer(
                    Icon::new(IconName::Menu)
                        .size(px(24.))
                        .opacity(if self.collapsed { 1. } else { 0. }),
                )
                .into_any_element(),
                centered_icon_layer(
                    Icon::new(IconName::MenuOpen)
                        .size(px(24.))
                        .opacity(if self.collapsed { 0. } else { 1. }),
                )
                .into_any_element(),
            ]
        };

        let mut button =
            IconButton::new(self.id.clone()).icon(Icon::new(IconName::Menu).opacity(0.));
        if let Some(on_click) = self.on_click {
            button = button.on_click(move |event, window, cx| on_click(event, window, cx));
        }

        div()
            .relative()
            .size_12()
            .refine_style(&self.style)
            .child(button)
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .children(icons),
            )
    }
}

fn centered_icon_layer(icon: impl IntoElement) -> impl IntoElement {
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .child(icon)
}
