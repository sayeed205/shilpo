use gpui::{Hsla, Pixels, px};

use crate::ActiveTheme;

pub(crate) const HEIGHT: Pixels = px(40.);
pub(crate) const ICON_CONSTRAINT: Pixels = px(18.);
pub(crate) const SEAM: Pixels = px(-1.);

pub(crate) struct SegmentedButtonTokens {
    pub height: Pixels,
    pub icon_constraint: Pixels,
    pub border: Hsla,
    pub seam: Pixels,
    pub radius: Pixels,
    pub inner_radius: Pixels,
    pub selected_container: Hsla,
    pub selected_content: Hsla,
    pub content: Hsla,
}

pub(crate) fn tokens(cx: &gpui::App) -> SegmentedButtonTokens {
    SegmentedButtonTokens {
        height: HEIGHT,
        icon_constraint: ICON_CONSTRAINT,
        border: cx.theme().outline,
        seam: SEAM,
        radius: px(20.),
        inner_radius: px(8.),
        selected_container: cx.theme().secondary_container,
        selected_content: cx.theme().on_secondary_container,
        content: cx.theme().on_surface,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outlined_segment_geometry_is_static() {
        assert_eq!(HEIGHT.as_f32(), 40.);
        assert_eq!(ICON_CONSTRAINT.as_f32(), 18.);
        assert_eq!(SEAM.as_f32(), -1.);
    }
}
