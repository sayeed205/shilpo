use gpui::{Hsla, Pixels, px};

use super::{IconButtonShape, IconButtonSize, IconButtonVariant, IconButtonWidth};
use crate::ActiveTheme;

pub(crate) fn resolve_width(size: IconButtonSize, width: IconButtonWidth) -> Pixels {
    let container = dimensions(size).container;
    match width {
        IconButtonWidth::Default => container,
        IconButtonWidth::Narrow => match size {
            IconButtonSize::XXSmall => px(14.),
            IconButtonSize::XSmall => px(24.),
            IconButtonSize::Small => px(30.),
            IconButtonSize::Medium => px(36.),
            IconButtonSize::Large => px(42.),
            IconButtonSize::XLarge => px(54.),
        },
        IconButtonWidth::Wide => match size {
            IconButtonSize::XXSmall => px(24.),
            IconButtonSize::XSmall => px(42.),
            IconButtonSize::Small => px(54.),
            IconButtonSize::Medium => px(64.),
            IconButtonSize::Large => px(76.),
            IconButtonSize::XLarge => px(96.),
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IconButtonDimensions {
    pub container: Pixels,
    pub icon: Pixels,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IconButtonCorner {
    Full,
    Square(Pixels),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IconButtonShapes {
    pub shape: IconButtonCorner,
    pub pressed_shape: IconButtonCorner,
}

pub(crate) fn dimensions(size: IconButtonSize) -> IconButtonDimensions {
    match size {
        IconButtonSize::XXSmall => IconButtonDimensions {
            container: px(18.),
            icon: px(12.),
        },
        IconButtonSize::XSmall => IconButtonDimensions {
            container: px(32.),
            icon: px(16.),
        },
        IconButtonSize::Small => IconButtonDimensions {
            container: px(40.),
            icon: px(20.),
        },
        IconButtonSize::Medium => IconButtonDimensions {
            container: px(48.),
            icon: px(24.),
        },
        IconButtonSize::Large => IconButtonDimensions {
            container: px(56.),
            icon: px(28.),
        },
        IconButtonSize::XLarge => IconButtonDimensions {
            container: px(72.),
            icon: px(32.),
        },
    }
}

pub fn icon_button_dimensions(size: IconButtonSize) -> IconButtonDimensions {
    dimensions(size)
}

pub(crate) fn shapes(size: IconButtonSize, shape: IconButtonShape) -> IconButtonShapes {
    let container = dimensions(size).container;
    match shape {
        IconButtonShape::Round => IconButtonShapes {
            shape: IconButtonCorner::Full,
            pressed_shape: IconButtonCorner::Full,
        },
        IconButtonShape::Square => IconButtonShapes {
            shape: IconButtonCorner::Square(match size {
                IconButtonSize::XXSmall => px(6.),
                IconButtonSize::XSmall => px(12.),
                IconButtonSize::Small => px(12.),
                IconButtonSize::Medium => px(16.),
                IconButtonSize::Large | IconButtonSize::XLarge => px(28.),
            }),
            pressed_shape: IconButtonCorner::Square(container * 0.25),
        },
    }
}

pub fn icon_button_shapes(size: IconButtonSize, shape: IconButtonShape) -> IconButtonShapes {
    shapes(size, shape)
}

pub(crate) struct IconButtonColors {
    pub container: Hsla,
    pub content: Hsla,
    pub border: Hsla,
}

pub(crate) fn colors(
    variant: IconButtonVariant,
    checked: bool,
    cx: &gpui::App,
) -> IconButtonColors {
    match (variant, checked) {
        (IconButtonVariant::Standard, false) => IconButtonColors {
            container: cx.theme().transparent,
            content: cx.theme().on_surface_variant,
            border: cx.theme().transparent,
        },
        (IconButtonVariant::Standard, true) => IconButtonColors {
            container: cx.theme().primary_container,
            content: cx.theme().on_primary_container,
            border: cx.theme().transparent,
        },
        (IconButtonVariant::Filled, false) | (IconButtonVariant::Filled, true) => {
            IconButtonColors {
                container: cx.theme().primary,
                content: cx.theme().on_primary,
                border: cx.theme().primary,
            }
        }
        (IconButtonVariant::FilledTonal, false) => IconButtonColors {
            container: cx.theme().secondary_container,
            content: cx.theme().on_secondary_container,
            border: cx.theme().secondary_container,
        },
        (IconButtonVariant::FilledTonal, true) => IconButtonColors {
            container: cx.theme().secondary,
            content: cx.theme().on_secondary,
            border: cx.theme().secondary,
        },
        (IconButtonVariant::Outlined, false) => IconButtonColors {
            container: cx.theme().transparent,
            content: cx.theme().on_surface_variant,
            border: cx.theme().outline_variant,
        },
        (IconButtonVariant::Outlined, true) => IconButtonColors {
            container: cx.theme().primary_container,
            content: cx.theme().on_primary_container,
            border: cx.theme().primary,
        },
    }
}
