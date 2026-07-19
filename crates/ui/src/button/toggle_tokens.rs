use gpui::{Hsla, Pixels, px};

use crate::ActiveTheme;

use super::{ToggleButtonColors, ToggleVariant};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ToggleTokens {
    pub height: Pixels,
    pub horizontal_padding: Pixels,
    pub vertical_padding: Pixels,
    pub icon: Pixels,
    pub gap: Pixels,
}

pub(crate) fn tokens(size: crate::Size) -> ToggleTokens {
    let dimensions = super::button_dimension_tokens::resolve(size, false, false, false);
    let (icon, gap) = match size {
        crate::Size::XSmall => (px(16.), px(4.)),
        crate::Size::Small => (px(20.), px(8.)),
        crate::Size::Medium => (px(24.), px(8.)),
        crate::Size::Large => (px(32.), px(12.)),
        crate::Size::Size(value) if value <= px(32.) => (px(16.), px(4.)),
        crate::Size::Size(value) if value <= px(40.) => (px(20.), px(8.)),
        crate::Size::Size(value) if value <= px(56.) => (px(24.), px(8.)),
        crate::Size::Size(_) => (px(32.), px(12.)),
    };
    ToggleTokens {
        height: dimensions.height,
        // Toggle remains on its pre-Phase-1C geometry contract.
        horizontal_padding: match size {
            crate::Size::XSmall => px(16.),
            crate::Size::Small => px(24.),
            crate::Size::Medium => px(24.),
            crate::Size::Large => px(48.),
            crate::Size::Size(value) if value <= px(32.) => px(16.),
            crate::Size::Size(value) if value <= px(56.) => px(24.),
            crate::Size::Size(_) => px(48.),
        },
        vertical_padding: match size {
            crate::Size::XSmall => px(4.),
            crate::Size::Small => px(8.),
            crate::Size::Medium => px(16.),
            crate::Size::Large => px(32.),
            crate::Size::Size(value) if value <= px(32.) => px(4.),
            crate::Size::Size(value) if value <= px(56.) => px(8.),
            crate::Size::Size(_) => px(32.),
        },
        icon,
        gap,
    }
}

pub(crate) fn colors(variant: ToggleVariant, checked: bool, cx: &gpui::App) -> ToggleButtonColors {
    match (variant, checked) {
        (ToggleVariant::Filled, true) => colors_of(
            cx.theme().primary,
            cx.theme().on_primary,
            cx.theme().transparent,
        ),
        (ToggleVariant::Filled, false) => colors_of(
            cx.theme().surface_container,
            cx.theme().primary,
            cx.theme().transparent,
        ),
        (ToggleVariant::Elevated, false) => colors_of(
            cx.theme().surface_container_low,
            cx.theme().primary,
            cx.theme().transparent,
        ),
        (ToggleVariant::Elevated, true) => colors_of(
            cx.theme().primary,
            cx.theme().on_primary,
            cx.theme().transparent,
        ),
        (ToggleVariant::Tonal, false) => colors_of(
            cx.theme().secondary_container,
            cx.theme().on_secondary_container,
            cx.theme().transparent,
        ),
        (ToggleVariant::Tonal, true) => colors_of(
            cx.theme().secondary,
            cx.theme().on_secondary,
            cx.theme().transparent,
        ),
        (ToggleVariant::Outlined, false) => colors_of(
            cx.theme().transparent,
            cx.theme().on_surface_variant,
            cx.theme().outline,
        ),
        (ToggleVariant::Outlined, true) => colors_of(
            cx.theme().inverse_surface,
            cx.theme().inverse_on_surface,
            cx.theme().outline,
        ),
        _ => legacy_colors(variant, checked, cx),
    }
}

fn colors_of(container: Hsla, content: Hsla, border: Hsla) -> ToggleButtonColors {
    ToggleButtonColors {
        container,
        content,
        border,
    }
}

fn legacy_colors(variant: ToggleVariant, checked: bool, cx: &gpui::App) -> ToggleButtonColors {
    match (variant, checked) {
        (ToggleVariant::Ghost, false) => colors_of(
            cx.theme().transparent,
            cx.theme().on_surface,
            cx.theme().transparent,
        ),
        (ToggleVariant::Ghost, true) => colors_of(
            cx.theme().secondary_container,
            cx.theme().on_secondary_container,
            cx.theme().transparent,
        ),
        (ToggleVariant::Outline, false) => colors_of(
            cx.theme().surface,
            cx.theme().on_surface,
            cx.theme().outline_variant,
        ),
        (ToggleVariant::Outline, true) => colors_of(
            cx.theme().secondary_container,
            cx.theme().on_secondary_container,
            cx.theme().outline_variant,
        ),
        _ => colors_of(
            cx.theme().transparent,
            cx.theme().on_surface_variant,
            cx.theme().transparent,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_toggle_metrics_are_material_values() {
        let tokens = tokens(crate::Size::Small);
        assert_eq!(tokens.height, px(40.));
        assert_eq!(tokens.horizontal_padding, px(24.));
        assert_eq!(tokens.vertical_padding, px(8.));
        assert_eq!(tokens.icon, px(20.));
        assert_eq!(tokens.gap, px(8.));
    }

    #[test]
    fn toggle_metrics_follow_androidx_size_buckets() {
        let expected = [
            (crate::Size::XSmall, 32., 16., 4.),
            (crate::Size::Small, 40., 20., 8.),
            (crate::Size::Medium, 56., 24., 8.),
            (crate::Size::Large, 96., 32., 12.),
        ];
        for (size, height, icon, gap) in expected {
            let tokens = tokens(size);
            assert_eq!(tokens.height, px(height));
            assert_eq!(tokens.icon, px(icon));
            assert_eq!(tokens.gap, px(gap));
        }
    }
}
