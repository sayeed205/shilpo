use crate::material::tokens::foundation::ElevationRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub close_button_between_space: Dp,
    pub close_button_container_elevation: ElevationRole,
    pub close_button_container_height: Dp,
    pub close_button_container_shape: ShapeRole,
    pub close_button_container_width: Dp,
    pub close_button_icon_size: Dp,
    pub list_item_between_space: Dp,
    pub list_item_container_elevation: ElevationRole,
    pub list_item_container_height: Dp,
    pub list_item_container_shape: ShapeRole,
    pub list_item_icon_label_space: Dp,
    pub list_item_icon_size: Dp,
    pub list_item_leading_space: Dp,
    pub list_item_trailing_space: Dp,
}

/// Returns authored token values for this component.
pub const fn fab_menu_baseline() -> Tokens {
    Tokens {
        close_button_between_space: crate::material::tokens::units::Dp(f32::from_bits(0x41000000)),
        close_button_container_elevation: crate::material::tokens::foundation::ElevationRole::Level3,
        close_button_container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42600000)),
        close_button_container_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        close_button_container_width: crate::material::tokens::units::Dp(f32::from_bits(0x42600000)),
        close_button_icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41A00000)),
        list_item_between_space: crate::material::tokens::units::Dp(f32::from_bits(0x40800000)),
        list_item_container_elevation: crate::material::tokens::foundation::ElevationRole::Level3,
        list_item_container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42600000)),
        list_item_container_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        list_item_icon_label_space: crate::material::tokens::units::Dp(f32::from_bits(0x41000000)),
        list_item_icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
        list_item_leading_space: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
        list_item_trailing_space: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
    }
}
