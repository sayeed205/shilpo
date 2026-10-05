use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::foundation::ElevationRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::typography::TypeRole;
use crate::material::tokens::units::{Dp, Opacity};

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub menu_container_color: ColorRole,
    pub menu_container_elevation: ElevationRole,
    pub menu_container_shape: ShapeRole,
    pub text_field_caret_color: ColorRole,
    pub text_field_container_color: ColorRole,
    pub text_field_container_shape: ShapeRole,
    pub field_disabled_input_text_color: ColorRole,
    pub field_disabled_input_text_opacity: Opacity,
    pub field_disabled_label_text_color: ColorRole,
    pub field_disabled_label_text_opacity: Opacity,
    pub text_field_disabled_leading_icon_color: ColorRole,
    pub text_field_disabled_leading_icon_opacity: Opacity,
    pub text_field_disabled_outline_color: ColorRole,
    pub text_field_disabled_outline_opacity: Opacity,
    pub text_field_disabled_outline_width: Dp,
    pub field_disabled_supporting_text_color: ColorRole,
    pub field_disabled_supporting_text_opacity: Opacity,
    pub text_field_disabled_trailing_icon_color: ColorRole,
    pub text_field_disabled_trailing_icon_opacity: Opacity,
    pub text_field_error_focus_caret_color: ColorRole,
    pub field_error_focus_input_text_color: ColorRole,
    pub field_error_focus_label_text_color: ColorRole,
    pub text_field_error_focus_leading_icon_color: ColorRole,
    pub text_field_error_focus_outline_color: ColorRole,
    pub field_error_focus_supporting_text_color: ColorRole,
    pub text_field_error_focus_trailing_icon_color: ColorRole,
    pub field_error_hover_input_text_color: ColorRole,
    pub field_error_hover_label_text_color: ColorRole,
    pub text_field_error_hover_leading_icon_color: ColorRole,
    pub text_field_error_hover_outline_color: ColorRole,
    pub field_error_hover_supporting_text_color: ColorRole,
    pub text_field_error_hover_trailing_icon_color: ColorRole,
    pub field_error_input_text_color: ColorRole,
    pub field_error_label_text_color: ColorRole,
    pub text_field_error_leading_icon_color: ColorRole,
    pub text_field_error_outline_color: ColorRole,
    pub field_error_supporting_text_color: ColorRole,
    pub text_field_error_trailing_icon_color: ColorRole,
    pub field_focus_input_text_color: ColorRole,
    pub field_focus_label_text_color: ColorRole,
    pub text_field_focus_leading_icon_color: ColorRole,
    pub text_field_focus_outline_color: ColorRole,
    pub text_field_focus_outline_width: Dp,
    pub field_focus_supporting_text_color: ColorRole,
    pub text_field_focus_trailing_icon_color: ColorRole,
    pub field_hover_input_text_color: ColorRole,
    pub field_hover_label_text_color: ColorRole,
    pub text_field_hover_leading_icon_color: ColorRole,
    pub text_field_hover_outline_color: ColorRole,
    pub text_field_hover_outline_width: Dp,
    pub field_hover_supporting_text_color: ColorRole,
    pub text_field_hover_trailing_icon_color: ColorRole,
    pub field_input_text_color: ColorRole,
    pub field_input_text_font: TypeRole,
    pub field_label_text_color: ColorRole,
    pub field_label_text_font: TypeRole,
    pub text_field_leading_icon_color: ColorRole,
    pub text_field_leading_icon_size: Dp,
    pub text_field_outline_color: ColorRole,
    pub text_field_outline_width: Dp,
    pub field_supporting_text_color: ColorRole,
    pub field_supporting_text_font: TypeRole,
    pub text_field_trailing_icon_color: ColorRole,
    pub text_field_trailing_icon_size: Dp,
}

/// Returns authored token values for this component.
pub const fn outlined_autocomplete() -> Tokens {
    Tokens {
        menu_container_color: crate::material::tokens::color_role::ColorRole::SurfaceContainer,
        menu_container_elevation: crate::material::tokens::foundation::ElevationRole::Level2,
        menu_container_shape: crate::material::tokens::shape::ShapeRole::CornerExtraSmall,
        text_field_caret_color: crate::material::tokens::color_role::ColorRole::Primary,
        text_field_container_color:
            crate::material::tokens::color_role::ColorRole::SurfaceContainerHighest,
        text_field_container_shape: crate::material::tokens::shape::ShapeRole::CornerExtraSmall,
        field_disabled_input_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        field_disabled_input_text_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3EC28F5C,
        )),
        field_disabled_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        field_disabled_label_text_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3EC28F5C,
        )),
        text_field_disabled_leading_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        text_field_disabled_leading_icon_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3EC28F5C,
        )),
        text_field_disabled_outline_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        text_field_disabled_outline_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3DF5C28F,
        )),
        text_field_disabled_outline_width: crate::material::tokens::units::Dp(f32::from_bits(0x3F800000)),
        field_disabled_supporting_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        field_disabled_supporting_text_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3EC28F5C,
        )),
        text_field_disabled_trailing_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        text_field_disabled_trailing_icon_opacity: crate::material::tokens::units::Opacity(
            f32::from_bits(0x3EC28F5C),
        ),
        text_field_error_focus_caret_color: crate::material::tokens::color_role::ColorRole::Error,
        field_error_focus_input_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        field_error_focus_label_text_color: crate::material::tokens::color_role::ColorRole::Error,
        text_field_error_focus_leading_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        text_field_error_focus_outline_color: crate::material::tokens::color_role::ColorRole::Error,
        field_error_focus_supporting_text_color: crate::material::tokens::color_role::ColorRole::Error,
        text_field_error_focus_trailing_icon_color: crate::material::tokens::color_role::ColorRole::Error,
        field_error_hover_input_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        field_error_hover_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnErrorContainer,
        text_field_error_hover_leading_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        text_field_error_hover_outline_color:
            crate::material::tokens::color_role::ColorRole::OnErrorContainer,
        field_error_hover_supporting_text_color: crate::material::tokens::color_role::ColorRole::Error,
        text_field_error_hover_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnErrorContainer,
        field_error_input_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        field_error_label_text_color: crate::material::tokens::color_role::ColorRole::Error,
        text_field_error_leading_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        text_field_error_outline_color: crate::material::tokens::color_role::ColorRole::Error,
        field_error_supporting_text_color: crate::material::tokens::color_role::ColorRole::Error,
        text_field_error_trailing_icon_color: crate::material::tokens::color_role::ColorRole::Error,
        field_focus_input_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        field_focus_label_text_color: crate::material::tokens::color_role::ColorRole::Primary,
        text_field_focus_leading_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        text_field_focus_outline_color: crate::material::tokens::color_role::ColorRole::Primary,
        text_field_focus_outline_width: crate::material::tokens::units::Dp(f32::from_bits(0x40000000)),
        field_focus_supporting_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        text_field_focus_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        field_hover_input_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        field_hover_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        text_field_hover_leading_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        text_field_hover_outline_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        text_field_hover_outline_width: crate::material::tokens::units::Dp(f32::from_bits(0x3F800000)),
        field_hover_supporting_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        text_field_hover_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        field_input_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        field_input_text_font: crate::material::tokens::typography::TypeRole::BodyLarge,
        field_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        field_label_text_font: crate::material::tokens::typography::TypeRole::BodyLarge,
        text_field_leading_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        text_field_leading_icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
        text_field_outline_color: crate::material::tokens::color_role::ColorRole::Outline,
        text_field_outline_width: crate::material::tokens::units::Dp(f32::from_bits(0x3F800000)),
        field_supporting_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        field_supporting_text_font: crate::material::tokens::typography::TypeRole::BodySmall,
        text_field_trailing_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        text_field_trailing_icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
    }
}
