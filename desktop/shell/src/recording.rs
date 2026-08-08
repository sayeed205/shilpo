use crate::runtime::ShellRuntime;
use gpui::{
    App, AppContext, Context, Entity, Focusable, IntoElement, ParentElement, Render, Styled,
    Subscription, Window, div,
};
use shilpo_capture::{AudioSource, RecordingSource};
use shilpo_ui::recording::{
    RecordingSourceOption, RecordingSourcePicker, RecordingSourcePickerEvent,
};
use std::sync::Arc;

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
