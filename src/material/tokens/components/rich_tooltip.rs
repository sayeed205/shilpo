use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::foundation::ElevationRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::typography::TypeRole;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub action_focus_label_text_color: ColorRole,
    pub action_hover_label_text_color: ColorRole,
    pub action_label_text_color: ColorRole,
    pub action_label_text_font: TypeRole,
    pub action_pressed_label_text_color: ColorRole,
    pub container_color: ColorRole,
    pub container_elevation: ElevationRole,
    pub container_shape: ShapeRole,
    pub subhead_color: ColorRole,
    pub subhead_font: TypeRole,
    pub supporting_text_color: ColorRole,
    pub supporting_text_font: TypeRole,
}

/// Returns authored token values for this component.
pub const fn rich_tooltip() -> Tokens {
    Tokens {
        action_focus_label_text_color: crate::material::tokens::color_role::ColorRole::Primary,
        action_hover_label_text_color: crate::material::tokens::color_role::ColorRole::Primary,
        action_label_text_color: crate::material::tokens::color_role::ColorRole::Primary,
        action_label_text_font: crate::material::tokens::typography::TypeRole::LabelLarge,
        action_pressed_label_text_color: crate::material::tokens::color_role::ColorRole::Primary,
        container_color: crate::material::tokens::color_role::ColorRole::SurfaceContainer,
        container_elevation: crate::material::tokens::foundation::ElevationRole::Level2,
        container_shape: crate::material::tokens::shape::ShapeRole::CornerMedium,
        subhead_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        subhead_font: crate::material::tokens::typography::TypeRole::TitleSmall,
        supporting_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        supporting_text_font: crate::material::tokens::typography::TypeRole::BodyMedium,
    }
}
