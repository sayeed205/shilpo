use gpui::{Pixels, px};

use super::button_scale_tokens;
use super::button_shared_tokens::COMMON_MIN_WIDTH;
use crate::Size;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ButtonDimensions {
    pub height: Pixels,
    pub horizontal_padding: Pixels,
    pub vertical_padding: Pixels,
    pub min_width: Pixels,
    pub icon: Pixels,
    pub gap: Pixels,
    pub outline: Pixels,
}

use super::ButtonVariant;

pub(crate) fn resolve(
    size: Size,
    variant: ButtonVariant,
    compact: bool,
    icon_only: bool,
) -> ButtonDimensions {
    if variant == ButtonVariant::Plain {
        let metrics = button_scale_tokens::size_metrics(size);
        let row = button_scale_tokens::button_m3e_row(metrics.metric_bucket);
        return ButtonDimensions {
            height: match size {
                Size::Size(value) => value,
                _ => Pixels::ZERO,
            },
            horizontal_padding: Pixels::ZERO,
            vertical_padding: Pixels::ZERO,
            min_width: Pixels::ZERO,
            icon: row.icon,
            gap: row.gap,
            outline: row.outline,
        };
    }
    if icon_only {
        let container = match size {
            Size::XSmall => px(32.),
            Size::Small => px(40.),
            Size::Medium => px(48.),
            Size::Large => px(56.),
            Size::Size(value) => value,
        };
        let metrics = button_scale_tokens::size_metrics(size);
        let row = button_scale_tokens::button_m3e_row(metrics.metric_bucket);
        return ButtonDimensions {
            height: container,
            horizontal_padding: Pixels::ZERO,
            vertical_padding: Pixels::ZERO,
            min_width: container,
            icon: button_scale_tokens::icon_button_m3e_bucket_for_height(container).icon,
            gap: Pixels::ZERO,
            outline: row.outline,
        };
    }
    let metrics = button_scale_tokens::size_metrics(size);
    let row = button_scale_tokens::button_m3e_row(metrics.metric_bucket);
    let is_text = variant == ButtonVariant::Text;
    let horizontal_padding = if is_text {
        match metrics.metric_bucket {
            button_scale_tokens::ButtonScale::XSmall => px(12.),
            button_scale_tokens::ButtonScale::Small => px(16.),
            button_scale_tokens::ButtonScale::Medium => px(24.),
            button_scale_tokens::ButtonScale::Large => px(48.),
            button_scale_tokens::ButtonScale::XLarge => px(64.),
        }
    } else {
        row.horizontal
    };
    ButtonDimensions {
        height: match size {
            Size::Size(value) => value,
            _ => row.height,
        },
        horizontal_padding: if compact {
            horizontal_padding * 0.5
        } else {
            horizontal_padding
        },
        vertical_padding: row.vertical,
        min_width: COMMON_MIN_WIDTH,
        icon: row.icon,
        gap: row.gap,
        outline: row.outline,
    }
}

pub(crate) fn height(size: Size) -> Pixels {
    resolve(size, ButtonVariant::Filled, false, false).height
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m3_dimensions_match_androidx() {
        let expected = [
            (Size::XSmall, 32., 12., 6.),
            (Size::Small, 40., 16., 10.),
            (Size::Medium, 56., 24., 16.),
            (Size::Large, 96., 48., 32.),
        ];
        for (size, height, horizontal, vertical) in expected {
            let actual = resolve(size, ButtonVariant::Filled, false, false);
            assert_eq!(actual.height, px(height));
            assert_eq!(actual.horizontal_padding, px(horizontal));
            assert_eq!(actual.vertical_padding, px(vertical));
            assert_eq!(actual.min_width, px(58.));
        }
    }

    #[test]
    fn text_buttons_keep_height_and_min_width_but_narrow_padding() {
        for (size, padding) in [
            (Size::XSmall, 12.),
            (Size::Small, 16.),
            (Size::Medium, 24.),
            (Size::Large, 48.),
            (Size::Size(px(136.)), 64.),
        ] {
            assert_eq!(
                resolve(size, ButtonVariant::Text, false, false).horizontal_padding,
                px(padding)
            );
        }
    }

    #[test]
    fn custom_heights_use_metric_bucket_rows() {
        for (height, padding, icon, gap) in [
            (33., 12., 20., 4.),
            (41., 16., 20., 8.),
            (57., 24., 24., 8.),
            (97., 48., 32., 12.),
            (116., 48., 32., 12.),
        ] {
            let dimensions = resolve(Size::Size(px(height)), ButtonVariant::Filled, false, false);
            assert_eq!(dimensions.height, px(height));
            assert_eq!(dimensions.horizontal_padding, px(padding));
            assert_eq!(dimensions.icon, px(icon));
            assert_eq!(dimensions.gap, px(gap));
        }
    }

    #[test]
    fn icon_only_buttons_use_icon_button_geometry() {
        let text_icon = resolve(Size::Medium, ButtonVariant::Text, false, true);
        let icon = resolve(Size::Medium, ButtonVariant::Filled, false, true);
        assert_eq!(text_icon, icon);
        assert_eq!(icon.height, px(48.));
        assert_eq!(icon.min_width, px(48.));
        assert_eq!(icon.horizontal_padding, Pixels::ZERO);
    }
}
