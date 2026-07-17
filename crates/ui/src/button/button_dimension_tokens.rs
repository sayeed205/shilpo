use gpui::{Pixels, px};

use crate::Size;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ButtonDimensions {
    pub height: Pixels,
    pub horizontal_padding: Pixels,
    pub vertical_padding: Pixels,
    pub min_width: Pixels,
}

#[derive(Clone, Copy)]
enum DimensionSize {
    XSmall,
    Small,
    Medium,
    Large,
    XLarge,
}

fn dimension_size(size: Size) -> DimensionSize {
    match size {
        Size::XSmall => DimensionSize::XSmall,
        Size::Small => DimensionSize::Small,
        Size::Medium => DimensionSize::Medium,
        Size::Large => DimensionSize::Large,
        Size::Size(value) if value <= px(32.) => DimensionSize::XSmall,
        Size::Size(value) if value <= px(40.) => DimensionSize::Small,
        Size::Size(value) if value <= px(56.) => DimensionSize::Medium,
        Size::Size(value) if value <= px(96.) => DimensionSize::Large,
        Size::Size(_) => DimensionSize::XLarge,
    }
}

pub(crate) fn resolve(size: Size, is_text: bool, compact: bool) -> ButtonDimensions {
    let dimensions = match dimension_size(size) {
        DimensionSize::XSmall => (px(32.), px(12.), px(6.)),
        DimensionSize::Small => (px(40.), px(24.), px(8.)),
        DimensionSize::Medium => (px(56.), px(24.), px(16.)),
        DimensionSize::Large => (px(96.), px(48.), px(32.)),
        DimensionSize::XLarge => (px(136.), px(64.), px(48.)),
    };
    let horizontal_padding = if is_text {
        px(12.)
    } else {
        dimensions.1
    };
    ButtonDimensions {
        height: match size {
            Size::Size(value) => value,
            _ => dimensions.0,
        },
        horizontal_padding: if compact {
            horizontal_padding * 0.5
        } else {
            horizontal_padding
        },
        vertical_padding: dimensions.2,
        min_width: px(58.),
    }
}

pub(crate) fn height(size: Size) -> Pixels {
    resolve(size, false, false).height
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m3_dimensions_match_androidx() {
        let expected = [
            (Size::XSmall, 32., 12., 6.),
            (Size::Small, 40., 24., 8.),
            (Size::Medium, 56., 24., 16.),
            (Size::Large, 96., 48., 32.),
        ];
        for (size, height, horizontal, vertical) in expected {
            let actual = resolve(size, false, false);
            assert_eq!(actual.height, px(height));
            assert_eq!(actual.horizontal_padding, px(horizontal));
            assert_eq!(actual.vertical_padding, px(vertical));
            assert_eq!(actual.min_width, px(58.));
        }
    }

    #[test]
    fn text_buttons_keep_height_and_min_width_but_narrow_padding() {
        let normal = resolve(Size::Small, false, false);
        let text = resolve(Size::Small, true, false);
        assert_eq!(text.height, normal.height);
        assert_eq!(text.min_width, normal.min_width);
        assert_eq!(text.horizontal_padding, px(12.));
        assert!(text.horizontal_padding < normal.horizontal_padding);
    }
}
