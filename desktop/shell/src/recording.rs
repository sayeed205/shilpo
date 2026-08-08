use crate::runtime::ShellRuntime;
use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyDownEvent, ParentElement, Render, Role, StatefulInteractiveElement, Styled,
    Subscription, Window, div, prelude::FluentBuilder, px,
};
use shilpo_capture::{AudioSource, RecordingSource};
use shilpo_ui::{ActiveTheme, FocusTrapElement, StyledExt, h_flex, v_flex};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordingSourceOption {
    pub label: String,
    pub description: String,
}

impl RecordingSourceOption {
    pub fn new(label: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            description: description.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordingSourcePickerEvent {
    Selected(usize),
    Cancelled,
}

pub struct RecordingSourcePicker {
    options: Vec<RecordingSourceOption>,
    dismiss_window: bool,
    focus_handle: FocusHandle,
}

impl RecordingSourcePicker {
    pub fn new(options: Vec<RecordingSourceOption>, cx: &mut Context<Self>) -> Self {
        Self {
            options,
            dismiss_window: false,
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn dismiss_window(mut self) -> Self {
        self.dismiss_window = true;
        self
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(RecordingSourcePickerEvent::Cancelled);
        if self.dismiss_window {
            window.remove_window();
        }
    }
}

impl EventEmitter<RecordingSourcePickerEvent> for RecordingSourcePicker {}

impl Focusable for RecordingSourcePicker {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for RecordingSourcePicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let options = self.options.clone();
        let dismiss_window = self.dismiss_window;
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(cx.theme().scrim.opacity(0.42))
            .id("recording-source-picker-backdrop")
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| this.cancel(window, cx)),
            )
            .child(
                v_flex()
                    .id("recording-source-picker")
                    .role(Role::Dialog)
                    .aria_label("Choose what to record")
                    .track_focus(&self.focus_handle)
                    .focus_trap("recording-source-picker-focus", &self.focus_handle)
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                        if event.keystroke.key.eq_ignore_ascii_case("escape") {
                            this.cancel(window, cx);
                        }
                    }))
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .w(px(560.))
                    .max_h(px(680.))
                    .p_6()
                    .gap_4()
                    .rounded_3xl()
                    .bg(cx.theme().surface_container_high)
                    .border_1()
                    .border_color(cx.theme().outline_variant.opacity(0.5))
                    .shadow_2xl()
                    .child(
                        div()
                            .text_xl()
                            .font_semibold()
                            .child("Choose what to record"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().on_surface_variant)
                            .child("Select a screen or an individual window."),
                    )
                    .child(
                        v_flex()
                            .id("recording-source-picker-list")
                            .gap_2()
                            .overflow_y_scroll()
                            .children(options.into_iter().enumerate().map(|(index, option)| {
                                h_flex()
                                    .id(("recording-source-option", index))
                                    .w_full()
                                    .p_3()
                                    .gap_3()
                                    .rounded_xl()
                                    .cursor_pointer()
                                    .bg(cx.theme().surface_container)
                                    .hover(|style| style.bg(cx.theme().surface_container_highest))
                                    .on_click(cx.listener(move |_, _, window, cx| {
                                        cx.emit(RecordingSourcePickerEvent::Selected(index));
                                        if dismiss_window {
                                            window.remove_window();
                                        }
                                    }))
                                    .child(
                                        v_flex()
                                            .gap_1()
                                            .child(div().font_medium().child(option.label))
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(cx.theme().on_surface_variant)
                                                    .child(option.description),
                                            ),
                                    )
                            }))
                            .when(self.options.is_empty(), |list| {
                                list.child(
                                    div()
                                        .p_4()
                                        .text_color(cx.theme().on_surface_variant)
                                        .child("No recordable screens or windows are available."),
                                )
                            }),
                    ),
            )
    }
}

#[derive(Clone)]
struct SourceChoice {
    label: String,
    description: String,
    source: RecordingSource,
}

pub struct RecordingChooserView {
    picker: Entity<RecordingSourcePicker>,
    _picker_subscription: Subscription,
}

impl RecordingChooserView {
    pub fn view(
        sources: Vec<RecordingSource>,
        audio: AudioSource,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<shilpo_ui::Root> {
        let chooser = cx.new(|cx| Self::new(sources, audio, window, cx));
        cx.new(|cx| {
            shilpo_ui::Root::new(chooser, window, cx)
                .bordered(false)
                .bg(gpui::transparent_black())
        })
    }

    fn new(
        sources_list: Vec<RecordingSource>,
        audio: AudioSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let choices = source_choices(&sources_list);
        let sources: Arc<Vec<RecordingSource>> =
            Arc::new(choices.iter().map(|choice| choice.source.clone()).collect());
        let options = choices
            .into_iter()
            .map(|choice| RecordingSourceOption::new(choice.label, choice.description))
            .collect();
        let picker = cx.new(|cx| RecordingSourcePicker::new(options, cx).dismiss_window());
        let picker_subscription = cx.subscribe(&picker, move |_, _, event, cx| match event {
            RecordingSourcePickerEvent::Selected(index) => {
                if let Some(source) = sources.get(*index) {
                    ShellRuntime::start_selected_recording(cx, source.clone(), audio);
                }
                ShellRuntime::forget_recording_chooser(cx);
            }
            RecordingSourcePickerEvent::Cancelled => {
                ShellRuntime::forget_recording_chooser(cx);
            }
        });

        let focus_handle = picker.read(cx).focus_handle(cx);
        window.focus(&focus_handle, cx);
        window.on_window_should_close(cx, |_, cx| {
            ShellRuntime::forget_recording_chooser(cx);
            true
        });

        Self {
            picker,
            _picker_subscription: picker_subscription,
        }
    }
}

fn source_choices(sources: &[RecordingSource]) -> Vec<SourceChoice> {
    let mut choices = Vec::new();
    for source in sources {
        match source {
            RecordingSource::Output(name) => {
                choices.push(SourceChoice {
                    label: name.clone(),
                    description: "Display Output".to_string(),
                    source: source.clone(),
                });
            }
            RecordingSource::Window(id) => {
                choices.push(SourceChoice {
                    label: format!("Window #{id}"),
                    description: "Application Window".to_string(),
                    source: source.clone(),
                });
            }
            RecordingSource::Region(r) => {
                choices.push(SourceChoice {
                    label: format!("Region {}x{}", r.width, r.height),
                    description: "Custom Region".to_string(),
                    source: source.clone(),
                });
            }
        }
    }
    choices
}

impl Render for RecordingChooserView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.picker.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_choices_from_sources() {
        let sources = vec![RecordingSource::Output("DP-1".into())];
        let choices = source_choices(&sources);
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].source, RecordingSource::Output("DP-1".into()));
    }
}
