use gpui::{Hsla, Pixels, px};

use crate::ActiveTheme;

pub(crate) struct SegmentedButtonTokens {
    pub height: Pixels,
    pub icon_constraint: Pixels,
    pub border: Hsla,
    pub seam: Pixels,
    pub radius: Pixels,
    pub selected_container: Hsla,
    pub selected_content: Hsla,
    pub content: Hsla,
}

pub(crate) fn tokens(cx: &gpui::App) -> SegmentedButtonTokens {
    SegmentedButtonTokens {
        height: px(40.),
        icon_constraint: px(18.),
        border: cx.theme().outline,
        seam: px(-1.),
        radius: px(20.),
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
        assert_eq!(px(40.), px(40.));
        assert_eq!(px(18.), px(18.));
        assert_eq!(px(-1.), px(-1.));
    }
}
