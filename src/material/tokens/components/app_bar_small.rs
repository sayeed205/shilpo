use crate::material::tokens::typography::TypeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_height: Dp,
    pub subtitle_font: TypeRole,
    pub title_font: TypeRole,
}

/// Returns authored token values for this component.
pub const fn app_bar_small() -> Tokens {
    Tokens {
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42800000)),
        subtitle_font: crate::material::tokens::typography::TypeRole::LabelMedium,
        title_font: crate::material::tokens::typography::TypeRole::TitleLarge,
    }
}
