use crate::material::tokens::typography::TypeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_height: Dp,
    pub subtitle_font: TypeRole,
    pub title_font: TypeRole,
    pub large_container_height: Dp,
}

/// Returns authored token values for this component.
pub const fn app_bar_large_flexible() -> Tokens {
    Tokens {
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42F00000)),
        subtitle_font: crate::material::tokens::typography::TypeRole::TitleMedium,
        title_font: crate::material::tokens::typography::TypeRole::DisplaySmall,
        large_container_height: crate::material::tokens::units::Dp(f32::from_bits(0x43180000)),
    }
}
