use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::typography::TypeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub color: ColorRole,
    pub large_color: ColorRole,
    pub large_label_text_color: ColorRole,
    pub large_label_text_font: TypeRole,
    pub large_shape: ShapeRole,
    pub large_size: Dp,
    pub shape: ShapeRole,
    pub size: Dp,
}

/// Returns authored token values for this component.
pub const fn badge() -> Tokens {
    Tokens {
        color: crate::material::tokens::color_role::ColorRole::Error,
        large_color: crate::material::tokens::color_role::ColorRole::Error,
        large_label_text_color: crate::material::tokens::color_role::ColorRole::OnError,
        large_label_text_font: crate::material::tokens::typography::TypeRole::LabelSmall,
        large_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        large_size: crate::material::tokens::units::Dp(f32::from_bits(0x41800000)),
        shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        size: crate::material::tokens::units::Dp(f32::from_bits(0x40C00000)),
    }
}
