use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::units::Opacity;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_color: ColorRole,
    pub container_opacity: Opacity,
}

/// Returns authored token values for this component.
pub const fn scrim() -> Tokens {
    Tokens {
        container_color: crate::material::tokens::color_role::ColorRole::Scrim,
        container_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3EA3D70A)),
    }
}
