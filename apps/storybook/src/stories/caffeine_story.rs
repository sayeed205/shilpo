use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, IntoElement, ParentElement, Render,
    Styled, Window,
};
use shilpo_ui::{CaffeineWidget, button::Button, dock::PanelControl, h_flex, v_flex};

use crate::section;

pub struct CaffeineStory {
    focus_handle: FocusHandle,
    caffeine_active: bool,
}

impl CaffeineStory {
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            caffeine_active: false,
        }
    }

    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }
}

impl super::Story for CaffeineStory {
    fn title() -> &'static str {
        "Caffeine (Inhibit Sleep) Widget"
    }

    fn description() -> &'static str {
        "Desktop sleep inhibitor widget toggling system idle suspension."
    }

    fn new_view(window: &mut Window, cx: &mut App) -> Entity<impl Render> {
        Self::view(window, cx)
    }

    fn zoomable() -> Option<PanelControl> {
        None
    }
}

impl Focusable for CaffeineStory {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for CaffeineStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();

        v_flex()
            .gap_6()
            .child(
                section("Interactive Caffeine Pill Preview")
                    .max_w_xl()
                    .child(h_flex().gap_4().items_center().child(
                        CaffeineWidget::new("caffeine-interactive", self.caffeine_active).on_click(
                            {
                                let entity = entity.clone();
                                move |_, _, cx| {
                                    entity.update(cx, |this, cx| {
                                        this.caffeine_active = !this.caffeine_active;
                                        cx.notify();
                                    });
                                }
                            },
                        ),
                    )),
            )
            .child(
                section("Controls & Preset States").max_w_xl().child(
                    h_flex().gap_3().child(
                        Button::new("toggle-caffeine-story")
                            .label(if self.caffeine_active {
                                "Caffeine: Active (Inhibiting Sleep)"
                            } else {
                                "Caffeine: Inactive (Normal Sleep)"
                            })
                            .on_click({
                                let entity = entity.clone();
                                move |_, _, cx| {
                                    entity.update(cx, |this, cx| {
                                        this.caffeine_active = !this.caffeine_active;
                                        cx.notify();
                                    });
                                }
                            }),
                    ),
                ),
            )
    }
}
