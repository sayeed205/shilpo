use crate::material::tokens::typography::TypeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub active_indicator_height: Dp,
    pub full_width_leading_space: Dp,
    pub full_width_trailing_space: Dp,
    pub icon_label_space: Dp,
    pub leading_space: Dp,
    pub label_text_font: TypeRole,
}

/// Returns authored token values for this component.
pub const fn navigation_rail_horizontal_item() -> Tokens {
    Tokens {
        active_indicator_height: crate::material::tokens::units::Dp(f32::from_bits(0x42600000)),
        full_width_leading_space: crate::material::tokens::units::Dp(f32::from_bits(0x41800000)),
        full_width_trailing_space: crate::material::tokens::units::Dp(f32::from_bits(0x41800000)),
        icon_label_space: crate::material::tokens::units::Dp(f32::from_bits(0x41000000)),
        leading_space: crate::material::tokens::units::Dp(f32::from_bits(0x41800000)),
        label_text_font: crate::material::tokens::typography::TypeRole::LabelLarge,
    }
}
