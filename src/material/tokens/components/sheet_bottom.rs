use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::foundation::ElevationRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub docked_container_color: ColorRole,
    pub docked_container_shape: ShapeRole,
    pub docked_drag_handle_color: ColorRole,
    pub docked_drag_handle_height: Dp,
    pub docked_drag_handle_width: Dp,
    pub docked_minimized_container_shape: ShapeRole,
    pub docked_modal_container_elevation: ElevationRole,
    pub docked_standard_container_elevation: ElevationRole,
    pub focus_indicator_color: ColorRole,
}

/// Returns authored token values for this component.
pub const fn sheet_bottom() -> Tokens {
    Tokens {
        docked_container_color: crate::material::tokens::color_role::ColorRole::SurfaceContainerLow,
        docked_container_shape: crate::material::tokens::shape::ShapeRole::CornerExtraLargeTop,
        docked_drag_handle_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        docked_drag_handle_height: crate::material::tokens::units::Dp(f32::from_bits(0x40800000)),
        docked_drag_handle_width: crate::material::tokens::units::Dp(f32::from_bits(0x42000000)),
        docked_minimized_container_shape: crate::material::tokens::shape::ShapeRole::CornerNone,
        docked_modal_container_elevation: crate::material::tokens::foundation::ElevationRole::Level1,
        docked_standard_container_elevation: crate::material::tokens::foundation::ElevationRole::Level1,
        focus_indicator_color: crate::material::tokens::color_role::ColorRole::Secondary,
    }
}
