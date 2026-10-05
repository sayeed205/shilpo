use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::typography::TypeRole;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_color: ColorRole,
    pub container_shape: ShapeRole,
    pub supporting_text_color: ColorRole,
    pub supporting_text_font: TypeRole,
}

/// Returns authored token values for this component.
pub const fn plain_tooltip() -> Tokens {
    Tokens {
        container_color: crate::material::tokens::color_role::ColorRole::InverseSurface,
        container_shape: crate::material::tokens::shape::ShapeRole::CornerExtraSmall,
        supporting_text_color: crate::material::tokens::color_role::ColorRole::InverseOnSurface,
        supporting_text_font: crate::material::tokens::typography::TypeRole::BodySmall,
    }
}
