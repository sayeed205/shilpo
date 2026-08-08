use crate::runtime::ShellRuntime;
use gpui::{
    App, AppContext, Context, Entity, Focusable, IntoElement, ParentElement, Render, Styled,
    Subscription, Window, div,
};
use shilpo_capture::{RecordingAudio, RecordingSource};
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

/// Shell adapter that binds compositor sources and recording actions to the
/// reusable source-picker presentation.
pub struct RecordingChooserView {
    picker: Entity<RecordingSourcePicker>,
    _picker_subscription: Subscription,
}

impl RecordingChooserView {
    pub fn view(
        catalog: shilpo_capture::RecordingSourceCatalog,
        audio: RecordingAudio,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<shilpo_ui::Root> {
        let chooser = cx.new(|cx| Self::new(catalog, audio, window, cx));
        cx.new(|cx| {
            shilpo_ui::Root::new(chooser, window, cx)
                .bordered(false)
                .bg(gpui::transparent_black())
        })
    }

    fn new(
        catalog: shilpo_capture::RecordingSourceCatalog,
        audio: RecordingAudio,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let choices = source_choices(&catalog);
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

fn source_choices(catalog: &shilpo_capture::RecordingSourceCatalog) -> Vec<SourceChoice> {
    let mut choices = Vec::new();
    for output in &catalog.outputs {
        choices.push(SourceChoice {
            label: output.name.clone(),
            description: match (&output.make, &output.model) {
                (Some(make), Some(model)) => format!(
                    "Screen · {make} {model} · {}×{}",
                    output.logical_size.0, output.logical_size.1
                ),
                _ => format!(
                    "Screen · {}×{}",
                    output.logical_size.0, output.logical_size.1
                ),
            },
            source: output.source(),
        });
    }

    for captured_window in &catalog.windows {
        choices.push(SourceChoice {
            label: captured_window.title.clone(),
            description: format!("Window · {}", captured_window.app_id),
            source: captured_window.source(),
        });
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
    use shilpo_capture::{RecordableOutput, RecordableWindow, RecordingSourceCatalog};

    #[test]
    fn source_choices_preserve_exact_capture_metadata() {
        let catalog = RecordingSourceCatalog {
            outputs: vec![RecordableOutput {
                name: "DP-1".into(),
                make: Some("Example".into()),
                model: Some("Panel".into()),
                logical_size: (1920, 1080),
            }],
            windows: vec![RecordableWindow {
                identifier: "window-7".into(),
                title: "Project".into(),
                app_id: "dev.editor".into(),
            }],
        };

        let choices = source_choices(&catalog);
        assert_eq!(choices.len(), 2);
        assert_eq!(choices[0].source, RecordingSource::Output("DP-1".into()));
        assert_eq!(
            choices[1].source,
            RecordingSource::Window {
                identifier: "window-7".into(),
                app_id: "dev.editor".into(),
                title: "Project".into(),
            }
        );
    }

    #[test]
    fn empty_catalog_has_no_choices() {
        assert!(source_choices(&RecordingSourceCatalog::default()).is_empty());
    }
}
