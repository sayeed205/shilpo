use crate::material::tokens::typography::TypeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_height: Dp,
    pub title_font: TypeRole,
}

/// Returns authored token values for this component.
pub const fn app_bar_large() -> Tokens {
    Tokens {
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x43180000)),
        title_font: crate::material::tokens::typography::TypeRole::HeadlineMedium,
    }
}
