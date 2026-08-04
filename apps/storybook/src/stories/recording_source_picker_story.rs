use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, IntoElement, ParentElement, Render,
    Styled, Subscription, Window, div, px,
};
use shilpo_ui::{
    StyledExt,
    recording::{RecordingSourceOption, RecordingSourcePicker, RecordingSourcePickerEvent},
    v_flex,
};

pub struct RecordingSourcePickerStory {
    picker: Entity<RecordingSourcePicker>,
    last_action: String,
    focus_handle: FocusHandle,
    _subscription: Subscription,
}

impl RecordingSourcePickerStory {
    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let picker = cx.new(|cx| {
            RecordingSourcePicker::new(
                vec![
                    RecordingSourceOption::new("eDP-1", "Screen · Built-in display · 1920×1080"),
                    RecordingSourceOption::new("Design review", "Window · dev.zed.Zed"),
                    RecordingSourceOption::new("Documentation", "Window · firefox"),
                ],
                cx,
            )
        });
        let subscription = cx.subscribe(&picker, |this, _, event, cx| {
            this.last_action = match event {
                RecordingSourcePickerEvent::Selected(index) => {
                    format!("Selected source {}", index + 1)
                }
                RecordingSourcePickerEvent::Cancelled => "Cancelled".into(),
            };
            cx.notify();
        });
        let focus_handle = cx.focus_handle();
        window.focus(&picker.read(cx).focus_handle(cx), cx);
        Self {
            picker,
            last_action: "Choose a source or press Escape".into(),
            focus_handle,
            _subscription: subscription,
        }
    }
}

impl super::Story for RecordingSourcePickerStory {
    fn title() -> &'static str {
        "Recording Source Picker"
    }

    fn description() -> &'static str {
        "M3 screen/window chooser with mouse selection, backdrop cancellation, and keyboard focus trapping."
    }

    fn new_view(window: &mut Window, cx: &mut App) -> Entity<impl Render> {
        Self::view(window, cx)
    }
}

impl Focusable for RecordingSourcePickerStory {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for RecordingSourcePickerStory {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .font_medium()
                    .child(self.last_action.clone()),
            )
            .child(div().h(px(560.)).w_full().child(self.picker.clone()))
    }
}
