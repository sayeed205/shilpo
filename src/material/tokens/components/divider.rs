use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub color: ColorRole,
    pub thickness: Dp,
}

/// Returns authored token values for this component.
pub const fn divider() -> Tokens {
    Tokens {
        color: crate::material::tokens::color_role::ColorRole::OutlineVariant,
        thickness: crate::material::tokens::units::Dp(f32::from_bits(0x3F800000)),
    }
}
