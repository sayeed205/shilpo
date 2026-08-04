use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, IntoElement, ParentElement, Render,
    Styled, Window,
};
use shilpo_ui::{SysInfoWidget, button::Button, dock::PanelControl, h_flex, v_flex};
use std::time::Duration;

use crate::section;

pub struct SysInfoStory {
    focus_handle: FocusHandle,
    cpu_percent: u8,
    frame_index: usize,
    _anim_task: Option<gpui::Task<()>>,
}

impl SysInfoStory {
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let _anim_task = Some(cx.spawn(async move |this, cx| {
            loop {
                let (cpu, frame) = this
                    .update(cx, |this, _| (this.cpu_percent, this.frame_index))
                    .unwrap_or((0, 0));

                let speed = ((cpu as f32) / 5.0).clamp(1.0, 20.0);
                let interval_ms = (500.0 / speed) as u64;

                cx.background_executor()
                    .timer(Duration::from_millis(interval_ms))
                    .await;

                let res = this.update(cx, |this, cx| {
                    this.frame_index = (frame + 1) % 5;
                    cx.notify();
                });
                if res.is_err() {
                    break;
                }
            }
        }));

        Self {
            focus_handle: cx.focus_handle(),
            cpu_percent: 15,
            frame_index: 0,
            _anim_task,
        }
    }

    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }
}

impl super::Story for SysInfoStory {
    fn title() -> &'static str {
        "SysInfo (RunCat Animation)"
    }

    fn description() -> &'static str {
        "CPU load monitor widget featuring RunCat-inspired running cat animation."
    }

    fn new_view(window: &mut Window, cx: &mut App) -> Entity<impl Render> {
        Self::view(window, cx)
    }

    fn zoomable() -> Option<PanelControl> {
        None
    }
}

impl Focusable for SysInfoStory {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SysInfoStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        let cpu = self.cpu_percent;
        let speed = ((cpu as f32) / 5.0).clamp(1.0, 20.0);
        let interval_ms = (500.0 / speed) as u64;

        v_flex()
            .gap_6()
            .child(section("Status Bar Widget Preview").max_w_xl().child(
                h_flex().gap_4().items_center().child(SysInfoWidget::new(
                    "sysinfo-preview",
                    self.frame_index,
                    self.cpu_percent,
                    35,
                )),
            ))
            .child(
                section("Interactive CPU Load Controller").max_w_xl().child(
                    v_flex()
                        .gap_4()
                        .child(
                            h_flex()
                                .gap_3()
                                .items_center()
                                .child(format!("CPU Load: {}%", self.cpu_percent))
                                .child(format!(
                                    "(Speed: {:.1}x, Interval: {}ms)",
                                    speed, interval_ms
                                )),
                        )
                        .child(
                            h_flex()
                                .gap_2()
                                .child(Button::new("preset-0").label("0% (Idle)").on_click({
                                    let entity = entity.clone();
                                    move |_, _, cx| {
                                        entity.update(cx, |this, cx| {
                                            this.cpu_percent = 0;
                                            cx.notify();
                                        });
                                    }
                                }))
                                .child(Button::new("preset-20").label("20% (Light)").on_click({
                                    let entity = entity.clone();
                                    move |_, _, cx| {
                                        entity.update(cx, |this, cx| {
                                            this.cpu_percent = 20;
                                            cx.notify();
                                        });
                                    }
                                }))
                                .child(Button::new("preset-50").label("50% (Medium)").on_click({
                                    let entity = entity.clone();
                                    move |_, _, cx| {
                                        entity.update(cx, |this, cx| {
                                            this.cpu_percent = 50;
                                            cx.notify();
                                        });
                                    }
                                }))
                                .child(Button::new("preset-80").label("80% (Heavy)").on_click({
                                    let entity = entity.clone();
                                    move |_, _, cx| {
                                        entity.update(cx, |this, cx| {
                                            this.cpu_percent = 80;
                                            cx.notify();
                                        });
                                    }
                                }))
                                .child(Button::new("preset-100").label("100% (Max)").on_click({
                                    let entity = entity.clone();
                                    move |_, _, cx| {
                                        entity.update(cx, |this, cx| {
                                            this.cpu_percent = 100;
                                            cx.notify();
                                        });
                                    }
                                })),
                        ),
                ),
            )
    }
}
