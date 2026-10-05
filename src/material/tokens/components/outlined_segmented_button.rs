use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::typography::TypeRole;
use crate::material::tokens::units::{Dp, Opacity};

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_height: Dp,
    pub disabled_icon_color: ColorRole,
    pub disabled_icon_opacity: Opacity,
    pub disabled_label_text_color: ColorRole,
    pub disabled_label_text_opacity: Opacity,
    pub disabled_outline_color: ColorRole,
    pub disabled_outline_opacity: Opacity,
    pub label_text_font: TypeRole,
    pub outline_color: ColorRole,
    pub outline_width: Dp,
    pub selected_container_color: ColorRole,
    pub selected_focus_icon_color: ColorRole,
    pub selected_focus_label_text_color: ColorRole,
    pub selected_hover_icon_color: ColorRole,
    pub selected_hover_label_text_color: ColorRole,
    pub selected_label_text_color: ColorRole,
    pub selected_pressed_icon_color: ColorRole,
    pub selected_pressed_label_text_color: ColorRole,
    pub selected_icon_color: ColorRole,
    pub shape: ShapeRole,
    pub unselected_focus_icon_color: ColorRole,
    pub unselected_focus_label_text_color: ColorRole,
    pub unselected_hover_icon_color: ColorRole,
    pub unselected_hover_label_text_color: ColorRole,
    pub unselected_label_text_color: ColorRole,
    pub unselected_pressed_icon_color: ColorRole,
    pub unselected_pressed_label_text_color: ColorRole,
    pub unselected_icon_color: ColorRole,
    pub icon_size: Dp,
}

/// Returns authored token values for this component.
pub const fn outlined_segmented_button() -> Tokens {
    Tokens {
        container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42200000)),
        disabled_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        disabled_icon_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3EC28F5C)),
        disabled_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        disabled_label_text_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3EC28F5C)),
        disabled_outline_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        disabled_outline_opacity: crate::material::tokens::units::Opacity(f32::from_bits(0x3DF5C28F)),
        label_text_font: crate::material::tokens::typography::TypeRole::LabelLarge,
        outline_color: crate::material::tokens::color_role::ColorRole::Outline,
        outline_width: crate::material::tokens::units::Dp(f32::from_bits(0x3F800000)),
        selected_container_color: crate::material::tokens::color_role::ColorRole::SecondaryContainer,
        selected_focus_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_focus_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_hover_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_hover_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_label_text_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_pressed_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_pressed_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        selected_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        unselected_focus_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        unselected_focus_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        unselected_hover_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        unselected_hover_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        unselected_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        unselected_pressed_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        unselected_pressed_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        unselected_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41900000)),
    }
}
