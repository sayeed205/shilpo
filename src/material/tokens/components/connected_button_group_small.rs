use crate::material::tokens::shape::CornerValueRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub between_space: Dp,
    pub container_height: Dp,
    pub container_shape: ShapeRole,
    pub inner_corner_corner_size: CornerValueRole,
    pub pressed_inner_corner_corner_size: CornerValueRole,
    pub selected_inner_corner_corner_size_percent: f32,
}

/// Returns authored token values for this component.
pub const fn connected_button_group_small() -> Tokens {
    Tokens {
        between_space: crate::material::tokens::units::Dp(f32::from_bits(0x40000000)),
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42200000)),
        container_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        inner_corner_corner_size: crate::material::tokens::shape::CornerValueRole::CornerValueSmall,
        pressed_inner_corner_corner_size:
            crate::material::tokens::shape::CornerValueRole::CornerValueExtraSmall,
        selected_inner_corner_corner_size_percent: f32::from_bits(0x42480000),
    }
}
