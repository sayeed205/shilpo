use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::foundation::ElevationRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::typography::TypeRole;
use crate::material::tokens::units::Dp;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_color: ColorRole,
    pub container_elevation: ElevationRole,
    pub container_height: Dp,
    pub container_shape: ShapeRole,
    pub focus_container_elevation: ElevationRole,
    pub focus_icon_color: ColorRole,
    pub focus_label_text_color: ColorRole,
    pub hover_container_elevation: ElevationRole,
    pub hover_icon_color: ColorRole,
    pub hover_label_text_color: ColorRole,
    pub icon_color: ColorRole,
    pub icon_size: Dp,
    pub label_text_color: ColorRole,
    pub label_text_font: TypeRole,
    pub lowered_container_elevation: ElevationRole,
    pub lowered_focus_container_elevation: ElevationRole,
    pub lowered_hover_container_elevation: ElevationRole,
    pub lowered_pressed_container_elevation: ElevationRole,
    pub pressed_container_elevation: ElevationRole,
    pub pressed_icon_color: ColorRole,
    pub pressed_label_text_color: ColorRole,
}

/// Returns authored token values for this component.
pub const fn extended_fab_primary() -> Tokens {
    Tokens {
        container_color: crate::material::tokens::color_role::ColorRole::PrimaryContainer,
        container_elevation: crate::material::tokens::foundation::ElevationRole::Level3,
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42600000)),
        container_shape: crate::material::tokens::shape::ShapeRole::CornerLarge,
        focus_container_elevation: crate::material::tokens::foundation::ElevationRole::Level3,
        focus_icon_color: crate::material::tokens::color_role::ColorRole::OnPrimaryContainer,
        focus_label_text_color: crate::material::tokens::color_role::ColorRole::OnPrimaryContainer,
        hover_container_elevation: crate::material::tokens::foundation::ElevationRole::Level4,
        hover_icon_color: crate::material::tokens::color_role::ColorRole::OnPrimaryContainer,
        hover_label_text_color: crate::material::tokens::color_role::ColorRole::OnPrimaryContainer,
        icon_color: crate::material::tokens::color_role::ColorRole::OnPrimaryContainer,
        icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
        label_text_color: crate::material::tokens::color_role::ColorRole::OnPrimaryContainer,
        label_text_font: crate::material::tokens::typography::TypeRole::LabelLarge,
        lowered_container_elevation: crate::material::tokens::foundation::ElevationRole::Level1,
        lowered_focus_container_elevation: crate::material::tokens::foundation::ElevationRole::Level1,
        lowered_hover_container_elevation: crate::material::tokens::foundation::ElevationRole::Level2,
        lowered_pressed_container_elevation: crate::material::tokens::foundation::ElevationRole::Level1,
        pressed_container_elevation: crate::material::tokens::foundation::ElevationRole::Level3,
        pressed_icon_color: crate::material::tokens::color_role::ColorRole::OnPrimaryContainer,
        pressed_label_text_color: crate::material::tokens::color_role::ColorRole::OnPrimaryContainer,
    }
}
