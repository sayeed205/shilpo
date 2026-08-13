#![allow(dead_code)]

use gpui::{Pixels, px};

use super::ButtonVariant;
use crate::Size;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ButtonScale {
    XSmall,
    Small,
    Medium,
    Large,
    XLarge,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ButtonM3eRow {
    pub height: Pixels,
    pub horizontal: Pixels,
    pub vertical: Pixels,
    pub icon: Pixels,
    pub gap: Pixels,
    pub outline: Pixels,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct IconButtonM3eRow {
    pub container: Pixels,
    pub icon: Pixels,
    pub square: Pixels,
    pub pressed: Pixels,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ThemeRole {
    Transparent,
    Primary,
    OnPrimary,
    PrimaryContainer,
    OnPrimaryContainer,
    SecondaryContainer,
    OnSecondaryContainer,
    SurfaceContainerLow,
    OnSurface,
    OnSurfaceVariant,
    Outline,
    OutlineVariant,
    SurfaceVariant,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ButtonSemanticState {
    pub container: ThemeRole,
    pub content: ThemeRole,
    pub border: ThemeRole,
    pub container_opacity: f32,
    pub content_opacity: f32,
    pub border_opacity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ButtonSemanticTable {
    pub base: ButtonSemanticState,
    pub selected: ButtonSemanticState,
    pub disabled: ButtonSemanticState,
    pub state_layer: ThemeRole,
    pub elevation_rest: u8,
    pub elevation_hover: u8,
    pub elevation_focus: u8,
    pub elevation_pressed: u8,
    pub elevation_disabled: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SizeMetrics {
    pub exact_height: Pixels,
    pub metric_bucket: ButtonScale,
    pub shape_bucket: ButtonScale,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ButtonShapeRow {
    pub square: Pixels,
    pub pressed: Pixels,
}

pub(crate) fn metric_bucket(height: Pixels) -> ButtonScale {
    if height < px(40.) {
        ButtonScale::XSmall
    } else if height < px(56.) {
        ButtonScale::Small
    } else if height < px(96.) {
        ButtonScale::Medium
    } else if height < px(136.) {
        ButtonScale::Large
    } else {
        ButtonScale::XLarge
    }
}

pub(crate) fn shape_bucket(height: Pixels) -> ButtonScale {
    if height <= px(36.) {
        ButtonScale::XSmall
    } else if height <= px(48.) {
        ButtonScale::Small
    } else if height <= px(76.) {
        ButtonScale::Medium
    } else if height <= px(116.) {
        ButtonScale::Large
    } else {
        ButtonScale::XLarge
    }
}

pub(crate) fn canonical_height(scale: ButtonScale) -> Pixels {
    match scale {
        ButtonScale::XSmall => px(32.),
        ButtonScale::Small => px(40.),
        ButtonScale::Medium => px(56.),
        ButtonScale::Large => px(96.),
        ButtonScale::XLarge => px(136.),
    }
}

pub(crate) fn button_m3e_row(scale: ButtonScale) -> ButtonM3eRow {
    match scale {
        ButtonScale::XSmall => ButtonM3eRow {
            height: px(32.),
            horizontal: px(12.),
            vertical: px(6.),
            icon: px(20.),
            gap: px(4.),
            outline: px(1.),
        },
        ButtonScale::Small => ButtonM3eRow {
            height: px(40.),
            horizontal: px(16.),
            vertical: px(10.),
            icon: px(20.),
            gap: px(8.),
            outline: px(1.),
        },
        ButtonScale::Medium => ButtonM3eRow {
            height: px(56.),
            horizontal: px(24.),
            vertical: px(16.),
            icon: px(24.),
            gap: px(8.),
            outline: px(1.),
        },
        ButtonScale::Large => ButtonM3eRow {
            height: px(96.),
            horizontal: px(48.),
            vertical: px(32.),
            icon: px(32.),
            gap: px(12.),
            outline: px(2.),
        },
        ButtonScale::XLarge => ButtonM3eRow {
            height: px(136.),
            horizontal: px(64.),
            vertical: px(48.),
            icon: px(40.),
            gap: px(16.),
            outline: px(3.),
        },
    }
}

pub(crate) fn icon_button_m3e_row(scale: ButtonScale) -> IconButtonM3eRow {
    match scale {
        ButtonScale::XSmall => IconButtonM3eRow {
            container: px(32.),
            icon: px(20.),
            square: px(12.),
            pressed: px(8.),
        },
        ButtonScale::Small => IconButtonM3eRow {
            container: px(40.),
            icon: px(24.),
            square: px(12.),
            pressed: px(8.),
        },
        ButtonScale::Medium => IconButtonM3eRow {
            container: px(56.),
            icon: px(24.),
            square: px(16.),
            pressed: px(12.),
        },
        ButtonScale::Large => IconButtonM3eRow {
            container: px(96.),
            icon: px(32.),
            square: px(28.),
            pressed: px(16.),
        },
        ButtonScale::XLarge => IconButtonM3eRow {
            container: px(136.),
            icon: px(40.),
            square: px(28.),
            pressed: px(16.),
        },
    }
}

pub(crate) fn button_m3e_for_height(height: Pixels) -> ButtonM3eRow {
    let scale = metric_bucket(height);
    let mut row = button_m3e_row(scale);
    row.height = height;
    row
}

pub(crate) fn icon_button_m3e_bucket_for_height(height: Pixels) -> IconButtonM3eRow {
    icon_button_m3e_row(metric_bucket(height))
}

pub(crate) fn size_metrics(size: Size) -> SizeMetrics {
    let exact_height = match size {
        Size::XSmall => px(32.),
        Size::Small => px(40.),
        Size::Medium => px(56.),
        Size::Large => px(96.),
        Size::Size(height) => height,
    };
    SizeMetrics {
        exact_height,
        metric_bucket: metric_bucket(exact_height),
        shape_bucket: shape_bucket(exact_height),
    }
}

pub(crate) fn button_shape_row(scale: ButtonScale) -> ButtonShapeRow {
    match scale {
        ButtonScale::XSmall | ButtonScale::Small => ButtonShapeRow {
            square: px(12.),
            pressed: px(8.),
        },
        ButtonScale::Medium => ButtonShapeRow {
            square: px(16.),
            pressed: px(12.),
        },
        ButtonScale::Large | ButtonScale::XLarge => ButtonShapeRow {
            square: px(28.),
            pressed: px(16.),
        },
    }
}

const fn state(container: ThemeRole, content: ThemeRole, border: ThemeRole) -> ButtonSemanticState {
    ButtonSemanticState {
        container,
        content,
        border,
        container_opacity: 1.,
        content_opacity: 1.,
        border_opacity: 1.,
    }
}

pub(crate) fn button_semantic_table(variant: ButtonVariant) -> ButtonSemanticTable {
    let (base, disabled, state_layer, elevations) = match variant {
        ButtonVariant::Filled => (
            state(
                ThemeRole::Primary,
                ThemeRole::OnPrimary,
                ThemeRole::Transparent,
            ),
            ButtonSemanticState {
                container: ThemeRole::OnSurface,
                content: ThemeRole::OnSurfaceVariant,
                border: ThemeRole::Transparent,
                container_opacity: 0.10,
                content_opacity: 0.38,
                border_opacity: 0.0,
            },
            ThemeRole::OnPrimary,
            (0, 1, 0, 0, 0),
        ),
        ButtonVariant::Elevated => (
            state(
                ThemeRole::SurfaceContainerLow,
                ThemeRole::Primary,
                ThemeRole::Transparent,
            ),
            ButtonSemanticState {
                container: ThemeRole::OnSurface,
                content: ThemeRole::OnSurfaceVariant,
                border: ThemeRole::Transparent,
                container_opacity: 0.10,
                content_opacity: 0.38,
                border_opacity: 0.0,
            },
            ThemeRole::Primary,
            (1, 2, 1, 1, 0),
        ),
        ButtonVariant::FilledTonal => (
            state(
                ThemeRole::SecondaryContainer,
                ThemeRole::OnSecondaryContainer,
                ThemeRole::Transparent,
            ),
            ButtonSemanticState {
                container: ThemeRole::OnSurface,
                content: ThemeRole::OnSurface,
                border: ThemeRole::Transparent,
                container_opacity: 0.12,
                content_opacity: 0.38,
                border_opacity: 0.0,
            },
            ThemeRole::OnSecondaryContainer,
            (0, 1, 0, 0, 0),
        ),
        ButtonVariant::Outlined => (
            state(
                ThemeRole::Transparent,
                ThemeRole::OnSurfaceVariant,
                ThemeRole::OutlineVariant,
            ),
            ButtonSemanticState {
                container: ThemeRole::Transparent,
                content: ThemeRole::OnSurfaceVariant,
                border: ThemeRole::OutlineVariant,
                container_opacity: 1.0,
                content_opacity: 0.38,
                border_opacity: 0.10,
            },
            ThemeRole::OnSurfaceVariant,
            (0, 0, 0, 0, 0),
        ),
        ButtonVariant::Text => (
            state(
                ThemeRole::Transparent,
                ThemeRole::Primary,
                ThemeRole::Transparent,
            ),
            ButtonSemanticState {
                container: ThemeRole::Transparent,
                content: ThemeRole::OnSurfaceVariant,
                border: ThemeRole::Transparent,
                container_opacity: 1.0,
                content_opacity: 0.38,
                border_opacity: 0.0,
            },
            ThemeRole::Primary,
            (0, 0, 0, 0, 0),
        ),
        ButtonVariant::Plain => (
            state(
                ThemeRole::Transparent,
                ThemeRole::OnSurfaceVariant,
                ThemeRole::Transparent,
            ),
            ButtonSemanticState {
                container: ThemeRole::Transparent,
                content: ThemeRole::OnSurfaceVariant,
                border: ThemeRole::Transparent,
                container_opacity: 1.0,
                content_opacity: 0.38,
                border_opacity: 0.0,
            },
            ThemeRole::Transparent,
            (0, 0, 0, 0, 0),
        ),
    };
    ButtonSemanticTable {
        base,
        selected: base,
        disabled,
        state_layer,
        elevation_rest: elevations.0,
        elevation_hover: elevations.1,
        elevation_focus: elevations.2,
        elevation_pressed: elevations.3,
        elevation_disabled: elevations.4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_boundaries_are_half_open() {
        assert_eq!(metric_bucket(px(0.)), ButtonScale::XSmall);
        assert_eq!(metric_bucket(px(39.99)), ButtonScale::XSmall);
        assert_eq!(metric_bucket(px(40.)), ButtonScale::Small);
        assert_eq!(metric_bucket(px(40.01)), ButtonScale::Small);
        assert_eq!(metric_bucket(px(55.99)), ButtonScale::Small);
        assert_eq!(metric_bucket(px(56.)), ButtonScale::Medium);
        assert_eq!(metric_bucket(px(56.01)), ButtonScale::Medium);
        assert_eq!(metric_bucket(px(95.99)), ButtonScale::Medium);
        assert_eq!(metric_bucket(px(96.)), ButtonScale::Large);
        assert_eq!(metric_bucket(px(96.01)), ButtonScale::Large);
        assert_eq!(metric_bucket(px(135.99)), ButtonScale::Large);
        assert_eq!(metric_bucket(px(136.)), ButtonScale::XLarge);
        assert_eq!(metric_bucket(px(136.01)), ButtonScale::XLarge);
    }

    #[test]
    fn shape_boundaries_are_closed() {
        assert_eq!(shape_bucket(px(35.99)), ButtonScale::XSmall);
        assert_eq!(shape_bucket(px(36.)), ButtonScale::XSmall);
        assert_eq!(shape_bucket(px(36.01)), ButtonScale::Small);
        assert_eq!(shape_bucket(px(47.99)), ButtonScale::Small);
        assert_eq!(shape_bucket(px(48.)), ButtonScale::Small);
        assert_eq!(shape_bucket(px(48.01)), ButtonScale::Medium);
        assert_eq!(shape_bucket(px(75.99)), ButtonScale::Medium);
        assert_eq!(shape_bucket(px(76.)), ButtonScale::Medium);
        assert_eq!(shape_bucket(px(76.01)), ButtonScale::Large);
        assert_eq!(shape_bucket(px(115.99)), ButtonScale::Large);
        assert_eq!(shape_bucket(px(116.)), ButtonScale::Large);
        assert_eq!(shape_bucket(px(116.01)), ButtonScale::XLarge);
    }

    #[test]
    fn canonical_rows_match_m3e_contract() {
        assert_eq!(
            [
                canonical_height(ButtonScale::XSmall),
                canonical_height(ButtonScale::Small),
                canonical_height(ButtonScale::Medium),
                canonical_height(ButtonScale::Large),
                canonical_height(ButtonScale::XLarge),
            ],
            [px(32.), px(40.), px(56.), px(96.), px(136.)]
        );
        let button_rows = [
            (
                ButtonScale::XSmall,
                (px(12.), px(6.), px(20.), px(4.), px(1.)),
            ),
            (
                ButtonScale::Small,
                (px(16.), px(10.), px(20.), px(8.), px(1.)),
            ),
            (
                ButtonScale::Medium,
                (px(24.), px(16.), px(24.), px(8.), px(1.)),
            ),
            (
                ButtonScale::Large,
                (px(48.), px(32.), px(32.), px(12.), px(2.)),
            ),
            (
                ButtonScale::XLarge,
                (px(64.), px(48.), px(40.), px(16.), px(3.)),
            ),
        ];
        for (scale, expected) in button_rows {
            let row = button_m3e_row(scale);
            assert_eq!(
                (row.horizontal, row.vertical, row.icon, row.gap, row.outline),
                expected
            );
        }

        let icon_rows = [
            (ButtonScale::XSmall, (px(32.), px(20.), px(12.), px(8.))),
            (ButtonScale::Small, (px(40.), px(24.), px(12.), px(8.))),
            (ButtonScale::Medium, (px(56.), px(24.), px(16.), px(12.))),
            (ButtonScale::Large, (px(96.), px(32.), px(28.), px(16.))),
            (ButtonScale::XLarge, (px(136.), px(40.), px(28.), px(16.))),
        ];
        for (scale, expected) in icon_rows {
            let row = icon_button_m3e_row(scale);
            assert_eq!((row.container, row.icon, row.square, row.pressed), expected);
        }
    }

    #[test]
    fn custom_metric_height_only_changes_button_height() {
        let row = button_m3e_for_height(px(50.));
        assert_eq!(row.height, px(50.));
        assert_eq!(row.horizontal, px(16.));
        assert_eq!(
            icon_button_m3e_bucket_for_height(px(50.)).container,
            px(40.)
        );
    }

    #[test]
    fn size_metrics_map_semantic_and_custom_sizes() {
        let semantic = [
            (Size::XSmall, px(32.)),
            (Size::Small, px(40.)),
            (Size::Medium, px(56.)),
            (Size::Large, px(96.)),
        ];
        for (size, height) in semantic {
            assert_eq!(size_metrics(size).exact_height, height);
        }
        let custom = size_metrics(Size::Size(px(48.)));
        assert_eq!(custom.exact_height, px(48.));
        assert_eq!(custom.metric_bucket, ButtonScale::Small);
        assert_eq!(custom.shape_bucket, ButtonScale::Small);
    }

    #[test]
    fn button_shape_rows_match_contract() {
        for scale in [ButtonScale::XSmall, ButtonScale::Small] {
            assert_eq!(
                button_shape_row(scale),
                ButtonShapeRow {
                    square: px(12.),
                    pressed: px(8.)
                }
            );
        }
        assert_eq!(
            button_shape_row(ButtonScale::Medium),
            ButtonShapeRow {
                square: px(16.),
                pressed: px(12.)
            }
        );
        for scale in [ButtonScale::Large, ButtonScale::XLarge] {
            assert_eq!(
                button_shape_row(scale),
                ButtonShapeRow {
                    square: px(28.),
                    pressed: px(16.)
                }
            );
        }
    }

    #[test]
    fn semantic_tables_keep_variant_local_disabled_and_elevation_values() {
        let variants = [
            (
                ButtonVariant::Filled,
                (
                    ThemeRole::Primary,
                    ThemeRole::OnPrimary,
                    ThemeRole::Transparent,
                ),
                (
                    ThemeRole::OnSurface,
                    ThemeRole::OnSurfaceVariant,
                    ThemeRole::Transparent,
                    0.10,
                    0.38,
                    0.0,
                ),
                ThemeRole::OnPrimary,
                (0, 1, 0, 0, 0),
            ),
            (
                ButtonVariant::Elevated,
                (
                    ThemeRole::SurfaceContainerLow,
                    ThemeRole::Primary,
                    ThemeRole::Transparent,
                ),
                (
                    ThemeRole::OnSurface,
                    ThemeRole::OnSurfaceVariant,
                    ThemeRole::Transparent,
                    0.10,
                    0.38,
                    0.0,
                ),
                ThemeRole::Primary,
                (1, 2, 1, 1, 0),
            ),
            (
                ButtonVariant::FilledTonal,
                (
                    ThemeRole::SecondaryContainer,
                    ThemeRole::OnSecondaryContainer,
                    ThemeRole::Transparent,
                ),
                (
                    ThemeRole::OnSurface,
                    ThemeRole::OnSurface,
                    ThemeRole::Transparent,
                    0.12,
                    0.38,
                    0.0,
                ),
                ThemeRole::OnSecondaryContainer,
                (0, 1, 0, 0, 0),
            ),
            (
                ButtonVariant::Outlined,
                (
                    ThemeRole::Transparent,
                    ThemeRole::OnSurfaceVariant,
                    ThemeRole::OutlineVariant,
                ),
                (
                    ThemeRole::Transparent,
                    ThemeRole::OnSurfaceVariant,
                    ThemeRole::OutlineVariant,
                    1.0,
                    0.38,
                    0.10,
                ),
                ThemeRole::OnSurfaceVariant,
                (0, 0, 0, 0, 0),
            ),
            (
                ButtonVariant::Text,
                (
                    ThemeRole::Transparent,
                    ThemeRole::Primary,
                    ThemeRole::Transparent,
                ),
                (
                    ThemeRole::Transparent,
                    ThemeRole::OnSurfaceVariant,
                    ThemeRole::Transparent,
                    1.0,
                    0.38,
                    0.0,
                ),
                ThemeRole::Primary,
                (0, 0, 0, 0, 0),
            ),
        ];
        for (variant, base, disabled, layer, elevation) in variants {
            let table = button_semantic_table(variant);
            assert_eq!(
                (table.base.container, table.base.content, table.base.border),
                base
            );
            assert_eq!(table.selected, table.base);
            assert_eq!(
                (
                    table.disabled.container,
                    table.disabled.content,
                    table.disabled.border,
                    table.disabled.container_opacity,
                    table.disabled.content_opacity,
                    table.disabled.border_opacity
                ),
                disabled
            );
            assert_eq!(table.state_layer, layer);
            assert_eq!(
                (
                    table.elevation_rest,
                    table.elevation_hover,
                    table.elevation_focus,
                    table.elevation_pressed,
                    table.elevation_disabled
                ),
                elevation
            );
            assert_eq!(table.disabled.content_opacity, 0.38);
        }
        assert_eq!(
            button_semantic_table(ButtonVariant::Filled)
                .disabled
                .container_opacity,
            0.10
        );
        assert_eq!(
            button_semantic_table(ButtonVariant::FilledTonal)
                .disabled
                .container_opacity,
            0.12
        );
        assert_eq!(
            button_semantic_table(ButtonVariant::Filled)
                .disabled
                .content,
            ThemeRole::OnSurfaceVariant
        );
        assert_eq!(
            button_semantic_table(ButtonVariant::FilledTonal)
                .disabled
                .content,
            ThemeRole::OnSurface
        );
        assert_eq!(
            button_semantic_table(ButtonVariant::Outlined)
                .disabled
                .border,
            ThemeRole::OutlineVariant
        );
        assert_eq!(
            button_semantic_table(ButtonVariant::Filled).disabled.border,
            ThemeRole::Transparent
        );
    }
}
