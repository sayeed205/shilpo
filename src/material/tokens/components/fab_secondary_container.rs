use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::foundation::ElevationRole;

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_color: ColorRole,
    pub container_elevation: ElevationRole,
    pub focused_container_elevation: ElevationRole,
    pub focused_icon_color: ColorRole,
    pub hovered_container_elevation: ElevationRole,
    pub hovered_icon_color: ColorRole,
    pub icon_color: ColorRole,
    pub pressed_container_elevation: ElevationRole,
    pub pressed_icon_color: ColorRole,
}

/// Returns authored token values for this component.
pub const fn fab_secondary_container() -> Tokens {
    Tokens {
        container_color: crate::material::tokens::color_role::ColorRole::SecondaryContainer,
        container_elevation: crate::material::tokens::foundation::ElevationRole::Level3,
        focused_container_elevation: crate::material::tokens::foundation::ElevationRole::Level3,
        focused_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        hovered_container_elevation: crate::material::tokens::foundation::ElevationRole::Level4,
        hovered_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        pressed_container_elevation: crate::material::tokens::foundation::ElevationRole::Level3,
        pressed_icon_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
    }
}
