use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::shape::ShapeRole;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub active_indicator_color: ColorRole,
    pub active_shape: ShapeRole,
    pub stop_color: ColorRole,
    pub stop_shape: ShapeRole,
    pub track_color: ColorRole,
    pub track_shape: ShapeRole,
}

/// Returns authored token values for this component.
pub const fn progress_indicator() -> Tokens {
    Tokens {
        active_indicator_color: crate::material::tokens::color_role::ColorRole::Primary,
        active_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        stop_color: crate::material::tokens::color_role::ColorRole::Primary,
        stop_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        track_color: crate::material::tokens::color_role::ColorRole::SecondaryContainer,
        track_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
    }
}
