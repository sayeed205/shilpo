use gpui::{px, Hsla, Pixels};

use crate::{ActiveTheme, Size};

pub(crate) const SEAM: Pixels = px(-1.);

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

pub(crate) fn tokens(size: Size, cx: &gpui::App) -> SegmentedButtonTokens {
    let metrics = super::button_scale_tokens::size_metrics(size);
    let scale_row = super::button_scale_tokens::button_m3e_row(metrics.metric_bucket);
    SegmentedButtonTokens {
        height: scale_row.height,
        icon_constraint: scale_row.icon,
        border: cx.theme().outline,
        seam: SEAM,
        radius: scale_row.height * 0.5,
        selected_container: cx.theme().secondary_container,
        selected_content: cx.theme().on_secondary_container,
        content: cx.theme().on_surface,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn outlined_segment_geometry_is_dynamic(cx: &mut gpui::TestAppContext) {
        cx.update(crate::init);
        cx.update(|cx| {
            let expected = [
                (Size::XSmall, 32., 20.),
                (Size::Small, 40., 20.),
                (Size::Medium, 56., 24.),
                (Size::Large, 96., 32.),
            ];
            for (size, height, icon) in expected {
                let tk = tokens(size, cx);
                assert_eq!(tk.height, px(height));
                assert_eq!(tk.icon_constraint, px(icon));
                assert_eq!(tk.seam, px(-1.));
                assert_eq!(tk.radius, px(height * 0.5));
            }
        });
    }
}
