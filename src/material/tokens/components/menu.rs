use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::foundation::ElevationRole;
use crate::material::tokens::shape::ShapeRole;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_color: ColorRole,
    pub container_elevation: ElevationRole,
    pub container_shape: ShapeRole,
    pub focus_indicator_color: ColorRole,
    pub list_item_selected_container_color: ColorRole,
    pub list_item_selected_label_text_color: ColorRole,
    pub list_item_selected_leading_trailing_icon_color: ColorRole,
    pub menu_list_item_leading_icon_color: ColorRole,
}

/// Returns authored token values for this component.
pub const fn menu() -> Tokens {
    Tokens {
        container_color: crate::material::tokens::color_role::ColorRole::SurfaceContainer,
        container_elevation: crate::material::tokens::foundation::ElevationRole::Level2,
        container_shape: crate::material::tokens::shape::ShapeRole::CornerExtraSmall,
        focus_indicator_color: crate::material::tokens::color_role::ColorRole::Secondary,
        list_item_selected_container_color:
            crate::material::tokens::color_role::ColorRole::SecondaryContainer,
        list_item_selected_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        list_item_selected_leading_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        menu_list_item_leading_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
    }
}
