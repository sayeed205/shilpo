use crate::{Icon, IconName, Sizable, Size};
use gpui::{
    Animation, AnimationExt as _, App, Hsla, IntoElement, ParentElement, RenderOnce, Styled as _,
    Transformation, Window, div, ease_in_out, percentage, prelude::FluentBuilder as _,
};
use instant::Duration;

/// A cycling loading spinner.
#[derive(IntoElement)]
pub struct Spinner {
    size: Size,
    icon: Icon,
    speed: Duration,
    easing: Box<dyn Fn(f32) -> f32>,
    color: Option<Hsla>,
}

impl Spinner {
    /// Create a new loading spinner.
    pub fn new() -> Self {
        Self {
            size: Size::Medium,
            speed: Duration::from_secs_f64(0.8),
            easing: Box::new(ease_in_out),
            icon: Icon::new(IconName::Loader),
            color: None,
        }
    }

    /// Set specified icon for the spinner.
    ///
    /// Default is [`IconName::Loader`].
    ///
    /// Please ensure the icon used is suitable for a loading spinner.
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = icon.into();
        self
    }

    /// Set the icon color.
    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    /// Set the easing function.
    pub fn ease(mut self, easing: impl Fn(f32) -> f32 + 'static) -> Self {
        self.easing = Box::new(easing);
        self
    }
}

impl Sizable for Spinner {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl RenderOnce for Spinner {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .child(
                self.icon
                    .with_size(self.size)
                    .when_some(self.color, |this, color| this.text_color(color))
                    .with_animation(
                        "circle",
                        Animation::new(self.speed).repeat().with_easing(self.easing),
                        |this, delta| this.transform(Transformation::rotate(percentage(delta))),
                    ),
            )
            .into_element()
    }
}

#[cfg(test)]
impl Spinner {
    pub(crate) fn get_size(&self) -> Size {
        self.size
    }

    pub(crate) fn get_icon_path(&self) -> &gpui::SharedString {
        self.icon.path_ref()
    }

    pub(crate) fn get_speed(&self) -> Duration {
        self.speed
    }

    pub(crate) fn get_color(&self) -> Option<Hsla> {
        self.color
    }

    pub(crate) fn run_easing(&self, val: f32) -> f32 {
        (self.easing)(val)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IconNamed;

    #[test]
    fn test_spinner_builder() {
        let sp = Spinner::new()
            .color(gpui::blue())
            .with_size(Size::Large)
            .icon(IconName::Check);

        assert_eq!(sp.get_size(), Size::Large);
        assert_eq!(sp.get_color(), Some(gpui::blue()));
        assert_eq!(sp.get_icon_path().as_ref(), IconName::Check.path().as_ref());
        assert_eq!(sp.get_speed(), Duration::from_secs_f64(0.8));
    }

    #[test]
    fn test_spinner_custom_ease() {
        let sp = Spinner::new().ease(|x| x * 2.0);
        assert_eq!(sp.run_easing(3.0), 6.0);
    }
}
