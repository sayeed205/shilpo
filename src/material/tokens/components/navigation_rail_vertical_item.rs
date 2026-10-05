use crate::material::tokens::typography::TypeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub active_indicator_height: Dp,
    pub active_indicator_width: Dp,
    pub icon_label_space: Dp,
    pub leading_space: Dp,
    pub trailing_space: Dp,
    pub label_text_font: TypeRole,
}

/// Returns authored token values for this component.
pub const fn navigation_rail_vertical_item() -> Tokens {
    Tokens {
        active_indicator_height: crate::material::tokens::units::Dp(f32::from_bits(0x42000000)),
        active_indicator_width: crate::material::tokens::units::Dp(f32::from_bits(0x42600000)),
        icon_label_space: crate::material::tokens::units::Dp(f32::from_bits(0x40800000)),
        leading_space: crate::material::tokens::units::Dp(f32::from_bits(0x41800000)),
        trailing_space: crate::material::tokens::units::Dp(f32::from_bits(0x41800000)),
        label_text_font: crate::material::tokens::typography::TypeRole::LabelMedium,
    }
}
