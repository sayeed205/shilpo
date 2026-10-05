// Shapes retain logical-corner and relative-size descriptors. No device-pixel
// resolution or corner-radius normalization is performed here.

use super::units::Dp;

/// Named Material 3 shape roles.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ShapeRole {
    CornerExtraExtraLarge,
    CornerExtraLarge,
    CornerExtraLargeIncreased,
    CornerExtraLargeTop,
    CornerExtraSmall,
    CornerExtraSmallTop,
    CornerFull,
    CornerLarge,
    CornerLargeEnd,
    CornerLargeIncreased,
    CornerLargeStart,
    CornerLargeTop,
    CornerMedium,
    CornerNone,
    CornerSmall,
}

impl ShapeRole {
    pub const ALL: [Self; 15] = [
        Self::CornerExtraExtraLarge,
        Self::CornerExtraLarge,
        Self::CornerExtraLargeIncreased,
        Self::CornerExtraLargeTop,
        Self::CornerExtraSmall,
        Self::CornerExtraSmallTop,
        Self::CornerFull,
        Self::CornerLarge,
        Self::CornerLargeEnd,
        Self::CornerLargeIncreased,
        Self::CornerLargeStart,
        Self::CornerLargeTop,
        Self::CornerMedium,
        Self::CornerNone,
        Self::CornerSmall,
    ];
}

/// A corner radius remains either a dp value or an unresolved size fraction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CornerSize {
    Dp(Dp),
    /// A fraction of the target's limiting dimension, not a resolved radius.
    Relative(f32),
}

/// Four corners expressed using logical start/end semantics.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogicalCorners {
    pub top_start: CornerSize,
    pub top_end: CornerSize,
    pub bottom_end: CornerSize,
    pub bottom_start: CornerSize,
}

impl LogicalCorners {
    pub const fn all(size: CornerSize) -> Self {
        Self {
            top_start: size,
            top_end: size,
            bottom_end: size,
            bottom_start: size,
        }
    }

    pub const fn to_physical(self, direction: LayoutDirection) -> PhysicalCorners {
        match direction {
            LayoutDirection::Ltr => PhysicalCorners {
                top_left: self.top_start,
                top_right: self.top_end,
                bottom_right: self.bottom_end,
                bottom_left: self.bottom_start,
            },
            LayoutDirection::Rtl => PhysicalCorners {
                top_left: self.top_end,
                top_right: self.top_start,
                bottom_right: self.bottom_start,
                bottom_left: self.bottom_end,
            },
        }
    }
}

/// Physical corners after an explicit logical-to-physical direction mapping.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicalCorners {
    pub top_left: CornerSize,
    pub top_right: CornerSize,
    pub bottom_right: CornerSize,
    pub bottom_left: CornerSize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LayoutDirection {
    Ltr,
    Rtl,
}

/// A full relative circle descriptor or four logical corner descriptors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Corners(LogicalCorners),
    Full,
}

/// Named corner-size values used independently of a complete shape.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CornerValueRole {
    CornerValueExtraExtraLarge,
    CornerValueExtraLarge,
    CornerValueExtraLargeIncreased,
    CornerValueExtraSmall,
    CornerValueLarge,
    CornerValueLargeIncreased,
    CornerValueMedium,
    CornerValueNone,
    CornerValueSmall,
}

impl CornerValueRole {
    pub const ALL: [Self; 9] = [
        Self::CornerValueExtraExtraLarge,
        Self::CornerValueExtraLarge,
        Self::CornerValueExtraLargeIncreased,
        Self::CornerValueExtraSmall,
        Self::CornerValueLarge,
        Self::CornerValueLargeIncreased,
        Self::CornerValueMedium,
        Self::CornerValueNone,
        Self::CornerValueSmall,
    ];
}

/// Resolve a named shape role to logical corner descriptors.
pub const fn shape(role: ShapeRole) -> Shape {
    use CornerSize::Dp as Radius;
    use ShapeRole::*;

    let zero = Radius(Dp(0.0));
    match role {
        CornerFull => Shape::Full,
        CornerExtraExtraLarge => rounded(48.0),
        CornerExtraLarge => rounded(28.0),
        CornerExtraLargeIncreased => rounded(32.0),
        CornerExtraLargeTop => Shape::Corners(LogicalCorners {
            top_start: Radius(Dp(28.0)),
            top_end: Radius(Dp(28.0)),
            bottom_end: zero,
            bottom_start: zero,
        }),
        CornerExtraSmall => rounded(4.0),
        CornerExtraSmallTop => Shape::Corners(LogicalCorners {
            top_start: Radius(Dp(4.0)),
            top_end: Radius(Dp(4.0)),
            bottom_end: zero,
            bottom_start: zero,
        }),
        CornerLarge => rounded(16.0),
        CornerLargeEnd => Shape::Corners(LogicalCorners {
            top_start: zero,
            top_end: Radius(Dp(16.0)),
            bottom_end: Radius(Dp(16.0)),
            bottom_start: zero,
        }),
        CornerLargeIncreased => rounded(20.0),
        CornerLargeStart => Shape::Corners(LogicalCorners {
            top_start: Radius(Dp(16.0)),
            top_end: zero,
            bottom_end: zero,
            bottom_start: Radius(Dp(16.0)),
        }),
        CornerLargeTop => Shape::Corners(LogicalCorners {
            top_start: Radius(Dp(16.0)),
            top_end: Radius(Dp(16.0)),
            bottom_end: zero,
            bottom_start: zero,
        }),
        CornerMedium => rounded(12.0),
        CornerNone => rounded(0.0),
        CornerSmall => rounded(8.0),
    }
}

const fn rounded(value: f32) -> Shape {
    Shape::Corners(LogicalCorners::all(CornerSize::Dp(Dp(value))))
}

/// Resolve a standalone corner-size value.
pub const fn corner_value(role: CornerValueRole) -> CornerSize {
    use CornerValueRole::*;

    match role {
        CornerValueExtraExtraLarge => CornerSize::Dp(Dp(48.0)),
        CornerValueExtraLarge => CornerSize::Dp(Dp(28.0)),
        CornerValueExtraLargeIncreased => CornerSize::Dp(Dp(32.0)),
        CornerValueExtraSmall => CornerSize::Dp(Dp(4.0)),
        CornerValueLarge => CornerSize::Dp(Dp(16.0)),
        CornerValueLargeIncreased => CornerSize::Dp(Dp(20.0)),
        CornerValueMedium => CornerSize::Dp(Dp(12.0)),
        CornerValueNone => CornerSize::Dp(Dp(0.0)),
        CornerValueSmall => CornerSize::Dp(Dp(8.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        corner_value, shape, CornerSize, CornerValueRole, LayoutDirection, LogicalCorners,
        PhysicalCorners, Shape, ShapeRole,
    };
    use crate::material::tokens::units::Dp;

    fn dp(value: f32) -> CornerSize {
        CornerSize::Dp(Dp(value))
    }

    #[test]
    fn includes_all_shape_roles_and_corner_values() {
        assert_eq!(ShapeRole::ALL.len(), 15);
        let expected = [
            (ShapeRole::CornerExtraExtraLarge, 48.0),
            (ShapeRole::CornerExtraLarge, 28.0),
            (ShapeRole::CornerExtraLargeIncreased, 32.0),
            (ShapeRole::CornerExtraSmall, 4.0),
            (ShapeRole::CornerLarge, 16.0),
            (ShapeRole::CornerLargeIncreased, 20.0),
            (ShapeRole::CornerMedium, 12.0),
            (ShapeRole::CornerNone, 0.0),
            (ShapeRole::CornerSmall, 8.0),
        ];
        for (role, value) in expected {
            assert_eq!(shape(role), Shape::Corners(LogicalCorners::all(dp(value))));
        }
        assert_eq!(shape(ShapeRole::CornerFull), Shape::Full);
        let zero = dp(0.0);
        let asymmetric = [
            (
                ShapeRole::CornerExtraLargeTop,
                LogicalCorners {
                    top_start: dp(28.0),
                    top_end: dp(28.0),
                    bottom_end: zero,
                    bottom_start: zero,
                },
            ),
            (
                ShapeRole::CornerExtraSmallTop,
                LogicalCorners {
                    top_start: dp(4.0),
                    top_end: dp(4.0),
                    bottom_end: zero,
                    bottom_start: zero,
                },
            ),
            (
                ShapeRole::CornerLargeEnd,
                LogicalCorners {
                    top_start: zero,
                    top_end: dp(16.0),
                    bottom_end: dp(16.0),
                    bottom_start: zero,
                },
            ),
            (
                ShapeRole::CornerLargeStart,
                LogicalCorners {
                    top_start: dp(16.0),
                    top_end: zero,
                    bottom_end: zero,
                    bottom_start: dp(16.0),
                },
            ),
            (
                ShapeRole::CornerLargeTop,
                LogicalCorners {
                    top_start: dp(16.0),
                    top_end: dp(16.0),
                    bottom_end: zero,
                    bottom_start: zero,
                },
            ),
        ];
        for (role, corners) in asymmetric {
            assert_eq!(shape(role), Shape::Corners(corners), "{role:?}");
        }
        assert_eq!(CornerValueRole::ALL.len(), 9);
        let expected_corner_values = [
            (CornerValueRole::CornerValueExtraExtraLarge, 48.0),
            (CornerValueRole::CornerValueExtraLarge, 28.0),
            (CornerValueRole::CornerValueExtraLargeIncreased, 32.0),
            (CornerValueRole::CornerValueExtraSmall, 4.0),
            (CornerValueRole::CornerValueLarge, 16.0),
            (CornerValueRole::CornerValueLargeIncreased, 20.0),
            (CornerValueRole::CornerValueMedium, 12.0),
            (CornerValueRole::CornerValueNone, 0.0),
            (CornerValueRole::CornerValueSmall, 8.0),
        ];
        for (role, value) in expected_corner_values {
            assert_eq!(corner_value(role), dp(value));
        }
    }

    #[test]
    fn asymmetric_logical_corners_map_explicitly_for_ltr_and_rtl() {
        let logical = LogicalCorners {
            top_start: dp(1.0),
            top_end: dp(2.0),
            bottom_end: dp(3.0),
            bottom_start: dp(4.0),
        };
        assert_eq!(
            logical.to_physical(LayoutDirection::Ltr),
            PhysicalCorners {
                top_left: dp(1.0),
                top_right: dp(2.0),
                bottom_right: dp(3.0),
                bottom_left: dp(4.0),
            }
        );
        assert_eq!(
            logical.to_physical(LayoutDirection::Rtl),
            PhysicalCorners {
                top_left: dp(2.0),
                top_right: dp(1.0),
                bottom_right: dp(4.0),
                bottom_left: dp(3.0),
            }
        );
    }

    #[test]
    fn full_shape_and_relative_corner_sizes_are_not_resolved() {
        let half = CornerSize::Relative(0.5);
        assert_eq!(half, CornerSize::Relative(0.5_f32));
        assert_eq!(
            LogicalCorners::all(half).to_physical(LayoutDirection::Rtl),
            PhysicalCorners {
                top_left: half,
                top_right: half,
                bottom_right: half,
                bottom_left: half,
            }
        );
        assert_eq!(shape(ShapeRole::CornerFull), Shape::Full);
    }
}
