use gpui::{App, IntoElement, RenderOnce, Window};

use crate::{Icon, Sizable, Size, progress::ProgressCircle};

/// Button icon which can be an Icon or Progress use for `icon` method of Button.
#[doc(hidden)]
#[derive(IntoElement)]
pub struct ButtonIcon {
    id: Option<gpui::ElementId>,
    icon: ButtonIconVariant,
    loading_icon: Option<Icon>,
    loading: bool,
    size: Size,
}

impl<T> From<T> for ButtonIcon
where
    T: Into<ButtonIconVariant>,
{
    fn from(icon: T) -> Self {
        ButtonIcon::new(icon)
    }
}

impl ButtonIcon {
    /// Creates a new ButtonIcon with the given icon.
    pub fn new(icon: impl Into<ButtonIconVariant>) -> Self {
        Self {
            id: None,
            icon: icon.into(),
            loading_icon: None,
            loading: false,
            size: Size::Medium,
        }
    }

    pub(crate) fn id(mut self, id: gpui::ElementId) -> Self {
        self.id = Some(id);
        self
    }

    pub(crate) fn loading_icon(mut self, icon: Option<Icon>) -> Self {
        self.loading_icon = icon;
        self
    }

    pub(crate) fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }
}

impl Sizable for ButtonIcon {
    fn with_size(mut self, size: impl Into<crate::Size>) -> Self {
        self.size = size.into();
        self
    }
}

/// Button icon which can be an Icon, Progress, or ProgressCircle use for `icon` method of Button.
#[doc(hidden)]
#[derive(IntoElement)]
pub enum ButtonIconVariant {
    Icon(Box<Icon>),
    Progress(Box<ProgressCircle>),
}

impl<T> From<T> for ButtonIconVariant
where
    T: Into<Icon>,
{
    fn from(icon: T) -> Self {
        Self::Icon(Box::new(icon.into()))
    }
}

impl From<ProgressCircle> for ButtonIconVariant {
    fn from(progress: ProgressCircle) -> Self {
        Self::Progress(Box::new(progress))
    }
}

impl ButtonIconVariant {
    /// Returns true if the ButtonIconKind is a Progress or ProgressCircle.
    #[inline]
    pub(crate) fn is_progress(&self) -> bool {
        matches!(self, Self::Progress(_))
    }
}

impl Sizable for ButtonIconVariant {
    fn with_size(self, size: impl Into<crate::Size>) -> Self {
        match self {
            Self::Icon(icon) => Self::Icon(Box::new(icon.with_size(size))),
            Self::Progress(progress) => Self::Progress(Box::new(progress.with_size(size))),
        }
    }
}

impl RenderOnce for ButtonIconVariant {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        match self {
            Self::Icon(icon) => icon.into_any_element(),
            Self::Progress(progress) => progress.into_any_element(),
        }
    }
}

impl RenderOnce for ButtonIcon {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        if self.loading {
            if self.icon.is_progress() {
                self.icon.with_size(self.size).into_any_element()
            } else {
                let spinner_id = match &self.id {
                    Some(id) => gpui::ElementId::Name(format!("{}-loading", id).into()),
                    None => gpui::ElementId::Name("button-loading".into()),
                };
                ProgressCircle::new(spinner_id)
                    .loading(true)
                    .with_size(self.size)
                    .into_any_element()
            }
        } else {
            self.icon.with_size(self.size).into_any_element()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IconName;

    #[gpui::test]
    fn test_button_icon_builder(_cx: &mut gpui::TestAppContext) {
        let custom_icon = Icon::new(IconName::ProgressActivity);
        let icon = ButtonIcon::new(IconName::Add)
            .loading(true)
            .loading_icon(Some(custom_icon))
            .large();

        assert!(icon.loading);
        assert!(icon.loading_icon.is_some());
        assert_eq!(icon.size, Size::Large);
    }

    #[gpui::test]
    fn test_button_icon_variant_types(_cx: &mut gpui::TestAppContext) {
        // Test Icon variant
        let icon_variant = ButtonIconVariant::Icon(Box::new(Icon::new(IconName::Add)));
        assert!(!icon_variant.is_progress());

        // Test Progress variant
        let progress_variant =
            ButtonIconVariant::Progress(Box::new(ProgressCircle::new("test-progress")));
        assert!(progress_variant.is_progress());
    }
}
