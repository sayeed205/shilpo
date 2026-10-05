use crate::material::tokens::color_role::ColorRole;
use crate::material::tokens::foundation::ElevationRole;
use crate::material::tokens::shape::ShapeRole;
use crate::material::tokens::typography::TypeRole;
use crate::material::tokens::units::{Dp, Opacity};

/// Token values used by this component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub container_shape: ShapeRole,
    pub divider_bottom_space: Dp,
    pub divider_leading_space: Dp,
    pub divider_top_space: Dp,
    pub divider_trailing_space: Dp,
    pub focus_indicator_color: ColorRole,
    pub item_between_space: Dp,
    pub item_bottom_space: Dp,
    pub item_container_color: ColorRole,
    pub item_container_elevation: ElevationRole,
    pub item_container_expressive_shape: ShapeRole,
    pub item_container_shape: ShapeRole,
    pub item_disabled_container_expressive_shape: ShapeRole,
    pub item_disabled_label_text_color: ColorRole,
    pub item_disabled_label_text_opacity: Opacity,
    pub item_disabled_leading_icon_color: ColorRole,
    pub item_disabled_leading_icon_opacity: Opacity,
    pub item_disabled_overline_color: ColorRole,
    pub item_disabled_overline_opacity: Opacity,
    pub item_disabled_state_layer_opacity: Opacity,
    pub item_disabled_supporting_text_color: ColorRole,
    pub item_disabled_supporting_text_opacity: Opacity,
    pub item_disabled_trailing_icon_color: ColorRole,
    pub item_disabled_trailing_icon_opacity: Opacity,
    pub item_dragged_container_elevation: ElevationRole,
    pub item_dragged_container_expressive_shape: ShapeRole,
    pub item_dragged_label_text_color: ColorRole,
    pub item_dragged_leading_icon_icon_color: ColorRole,
    pub item_dragged_trailing_icon_icon_color: ColorRole,
    pub item_focus_label_text_color: ColorRole,
    pub item_focus_leading_icon_icon_color: ColorRole,
    pub item_focus_trailing_icon_icon_color: ColorRole,
    pub item_focused_container_expressive_shape: ShapeRole,
    pub item_hover_label_text_color: ColorRole,
    pub item_hover_leading_icon_icon_color: ColorRole,
    pub item_hover_trailing_icon_icon_color: ColorRole,
    pub item_hovered_container_expressive_shape: ShapeRole,
    pub item_label_text_color: ColorRole,
    pub item_label_text_font: TypeRole,
    pub item_large_leading_video_height: Dp,
    pub item_large_leading_video_width: Dp,
    pub item_leading_avatar_color: ColorRole,
    pub item_leading_avatar_label_color: ColorRole,
    pub item_leading_avatar_label_font: TypeRole,
    pub item_leading_avatar_shape: ShapeRole,
    pub item_leading_avatar_size: Dp,
    pub item_leading_icon_color: ColorRole,
    pub item_leading_icon_expressive_size: Dp,
    pub item_leading_icon_size: Dp,
    pub item_leading_image_expressive_shape: ShapeRole,
    pub item_leading_image_height: Dp,
    pub item_leading_image_shape: ShapeRole,
    pub item_leading_image_width: Dp,
    pub item_leading_space: Dp,
    pub item_leading_video_shape: ShapeRole,
    pub item_leading_video_width: Dp,
    pub item_one_line_container_height: Dp,
    pub item_overline_color: ColorRole,
    pub item_overline_font: TypeRole,
    pub item_pressed_container_expressive_shape: ShapeRole,
    pub item_pressed_label_text_color: ColorRole,
    pub item_pressed_leading_icon_icon_color: ColorRole,
    pub item_pressed_trailing_icon_icon_color: ColorRole,
    pub item_segmented_container_color: ColorRole,
    pub item_selected_container_color: ColorRole,
    pub item_selected_container_expressive_shape: ShapeRole,
    pub item_selected_container_shape: ShapeRole,
    pub item_selected_disabled_container_color: ColorRole,
    pub item_selected_disabled_container_expressive_shape: ShapeRole,
    pub item_selected_disabled_container_opacity: Opacity,
    pub item_selected_disabled_label_text_color: ColorRole,
    pub item_selected_disabled_label_text_opacity: Opacity,
    pub item_selected_disabled_leading_icon_color: ColorRole,
    pub item_selected_disabled_leading_icon_opacity: Opacity,
    pub item_selected_disabled_overline_color: ColorRole,
    pub item_selected_disabled_overline_opacity: Opacity,
    pub item_selected_disabled_state_layer_opacity: Opacity,
    pub item_selected_disabled_supporting_text_color: ColorRole,
    pub item_selected_disabled_supporting_text_opacity: Opacity,
    pub item_selected_disabled_trailing_icon_color: ColorRole,
    pub item_selected_disabled_trailing_icon_opacity: Opacity,
    pub item_selected_disabled_trailing_supporting_text_color: ColorRole,
    pub item_selected_disabled_trailing_supporting_text_opacity: Opacity,
    pub item_selected_dragged_container_expressive_shape: ShapeRole,
    pub item_selected_dragged_label_text_color: ColorRole,
    pub item_selected_dragged_leading_icon_color: ColorRole,
    pub item_selected_dragged_trailing_icon_color: ColorRole,
    pub item_selected_focus_label_text_color: ColorRole,
    pub item_selected_focus_leading_icon_color: ColorRole,
    pub item_selected_focus_trailing_icon_color: ColorRole,
    pub item_selected_focused_container_expressive_shape: ShapeRole,
    pub item_selected_hover_label_text_color: ColorRole,
    pub item_selected_hover_leading_icon_color: ColorRole,
    pub item_selected_hover_trailing_icon_color: ColorRole,
    pub item_selected_hovered_container_expressive_shape: ShapeRole,
    pub item_selected_label_text_color: ColorRole,
    pub item_selected_leading_icon_color: ColorRole,
    pub item_selected_overline_color: ColorRole,
    pub item_selected_pressed_container_expressive_shape: ShapeRole,
    pub item_selected_pressed_label_text_color: ColorRole,
    pub item_selected_pressed_leading_icon_color: ColorRole,
    pub item_selected_pressed_trailing_icon_color: ColorRole,
    pub item_selected_supporting_text_color: ColorRole,
    pub item_selected_trailing_icon_color: ColorRole,
    pub item_selected_trailing_supporting_text_color: ColorRole,
    pub item_small_leading_video_height: Dp,
    pub item_small_leading_video_width: Dp,
    pub item_supporting_text_color: ColorRole,
    pub item_supporting_text_font: TypeRole,
    pub item_three_line_container_height: Dp,
    pub item_top_space: Dp,
    pub item_trailing_icon_color: ColorRole,
    pub item_trailing_icon_expressive_size: Dp,
    pub item_trailing_icon_size: Dp,
    pub item_trailing_space: Dp,
    pub item_trailing_supporting_text_color: ColorRole,
    pub item_trailing_supporting_text_font: TypeRole,
    pub item_two_line_container_height: Dp,
    pub item_unselected_trailing_icon_color: ColorRole,
    pub segmented_gap: Dp,
}

/// Returns authored token values for this component.
pub const fn list() -> Tokens {
    Tokens {
        container_shape: crate::material::tokens::shape::ShapeRole::CornerLarge,
        divider_bottom_space: crate::material::tokens::units::Dp(f32::from_bits(0x00000000)),
        divider_leading_space: crate::material::tokens::units::Dp(f32::from_bits(0x41800000)),
        divider_top_space: crate::material::tokens::units::Dp(f32::from_bits(0x00000000)),
        divider_trailing_space: crate::material::tokens::units::Dp(f32::from_bits(0x41800000)),
        focus_indicator_color: crate::material::tokens::color_role::ColorRole::Secondary,
        item_between_space: crate::material::tokens::units::Dp(f32::from_bits(0x41400000)),
        item_bottom_space: crate::material::tokens::units::Dp(f32::from_bits(0x41200000)),
        item_container_color: crate::material::tokens::color_role::ColorRole::Surface,
        item_container_elevation: crate::material::tokens::foundation::ElevationRole::Level0,
        item_container_expressive_shape: crate::material::tokens::shape::ShapeRole::CornerExtraSmall,
        item_container_shape: crate::material::tokens::shape::ShapeRole::CornerNone,
        item_disabled_container_expressive_shape:
            crate::material::tokens::shape::ShapeRole::CornerExtraSmall,
        item_disabled_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        item_disabled_label_text_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3EC28F5C,
        )),
        item_disabled_leading_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        item_disabled_leading_icon_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3EC28F5C,
        )),
        item_disabled_overline_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        item_disabled_overline_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3EC28F5C,
        )),
        item_disabled_state_layer_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3DCCCCCD,
        )),
        item_disabled_supporting_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        item_disabled_supporting_text_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3EC28F5C,
        )),
        item_disabled_trailing_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        item_disabled_trailing_icon_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3EC28F5C,
        )),
        item_dragged_container_elevation: crate::material::tokens::foundation::ElevationRole::Level4,
        item_dragged_container_expressive_shape: crate::material::tokens::shape::ShapeRole::CornerLarge,
        item_dragged_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        item_dragged_leading_icon_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        item_dragged_trailing_icon_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        item_focus_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        item_focus_leading_icon_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        item_focus_trailing_icon_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        item_focused_container_expressive_shape: crate::material::tokens::shape::ShapeRole::CornerLarge,
        item_hover_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        item_hover_leading_icon_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        item_hover_trailing_icon_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        item_hovered_container_expressive_shape: crate::material::tokens::shape::ShapeRole::CornerMedium,
        item_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        item_label_text_font: crate::material::tokens::typography::TypeRole::BodyLarge,
        item_large_leading_video_height: crate::material::tokens::units::Dp(f32::from_bits(0x42800000)),
        item_large_leading_video_width: crate::material::tokens::units::Dp(f32::from_bits(0x42E40000)),
        item_leading_avatar_color: crate::material::tokens::color_role::ColorRole::PrimaryContainer,
        item_leading_avatar_label_color:
            crate::material::tokens::color_role::ColorRole::OnPrimaryContainer,
        item_leading_avatar_label_font: crate::material::tokens::typography::TypeRole::TitleMedium,
        item_leading_avatar_shape: crate::material::tokens::shape::ShapeRole::CornerFull,
        item_leading_avatar_size: crate::material::tokens::units::Dp(f32::from_bits(0x42200000)),
        item_leading_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        item_leading_icon_expressive_size: crate::material::tokens::units::Dp(f32::from_bits(0x41A00000)),
        item_leading_icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
        item_leading_image_expressive_shape: crate::material::tokens::shape::ShapeRole::CornerSmall,
        item_leading_image_height: crate::material::tokens::units::Dp(f32::from_bits(0x42600000)),
        item_leading_image_shape: crate::material::tokens::shape::ShapeRole::CornerNone,
        item_leading_image_width: crate::material::tokens::units::Dp(f32::from_bits(0x42600000)),
        item_leading_space: crate::material::tokens::units::Dp(f32::from_bits(0x41800000)),
        item_leading_video_shape: crate::material::tokens::shape::ShapeRole::CornerSmall,
        item_leading_video_width: crate::material::tokens::units::Dp(f32::from_bits(0x42C80000)),
        item_one_line_container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42600000)),
        item_overline_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        item_overline_font: crate::material::tokens::typography::TypeRole::LabelSmall,
        item_pressed_container_expressive_shape: crate::material::tokens::shape::ShapeRole::CornerLarge,
        item_pressed_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        item_pressed_leading_icon_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        item_pressed_trailing_icon_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        item_segmented_container_color: crate::material::tokens::color_role::ColorRole::Surface,
        item_selected_container_color: crate::material::tokens::color_role::ColorRole::SecondaryContainer,
        item_selected_container_expressive_shape: crate::material::tokens::shape::ShapeRole::CornerLarge,
        item_selected_container_shape: crate::material::tokens::shape::ShapeRole::CornerLarge,
        item_selected_disabled_container_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        item_selected_disabled_container_expressive_shape:
            crate::material::tokens::shape::ShapeRole::CornerLarge,
        item_selected_disabled_container_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3EC28F5C,
        )),
        item_selected_disabled_label_text_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        item_selected_disabled_label_text_opacity: crate::material::tokens::units::Opacity(
            f32::from_bits(0x3EC28F5C),
        ),
        item_selected_disabled_leading_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurface,
        item_selected_disabled_leading_icon_opacity: crate::material::tokens::units::Opacity(
            f32::from_bits(0x3EC28F5C),
        ),
        item_selected_disabled_overline_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        item_selected_disabled_overline_opacity: crate::material::tokens::units::Opacity(f32::from_bits(
            0x3EC28F5C,
        )),
        item_selected_disabled_state_layer_opacity: crate::material::tokens::units::Opacity(
            f32::from_bits(0x3DCCCCCD),
        ),
        item_selected_disabled_supporting_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurface,
        item_selected_disabled_supporting_text_opacity: crate::material::tokens::units::Opacity(
            f32::from_bits(0x3EC28F5C),
        ),
        item_selected_disabled_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurface,
        item_selected_disabled_trailing_icon_opacity: crate::material::tokens::units::Opacity(
            f32::from_bits(0x3EC28F5C),
        ),
        item_selected_disabled_trailing_supporting_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurface,
        item_selected_disabled_trailing_supporting_text_opacity: crate::material::tokens::units::Opacity(
            f32::from_bits(0x3EC28F5C),
        ),
        item_selected_dragged_container_expressive_shape:
            crate::material::tokens::shape::ShapeRole::CornerLarge,
        item_selected_dragged_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        item_selected_dragged_leading_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurface,
        item_selected_dragged_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurface,
        item_selected_focus_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        item_selected_focus_leading_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        item_selected_focus_trailing_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        item_selected_focused_container_expressive_shape:
            crate::material::tokens::shape::ShapeRole::CornerLarge,
        item_selected_hover_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        item_selected_hover_leading_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        item_selected_hover_trailing_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        item_selected_hovered_container_expressive_shape:
            crate::material::tokens::shape::ShapeRole::CornerLarge,
        item_selected_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        item_selected_leading_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        item_selected_overline_color: crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        item_selected_pressed_container_expressive_shape:
            crate::material::tokens::shape::ShapeRole::CornerLarge,
        item_selected_pressed_label_text_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        item_selected_pressed_leading_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurface,
        item_selected_pressed_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSurface,
        item_selected_supporting_text_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        item_selected_trailing_icon_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        item_selected_trailing_supporting_text_color:
            crate::material::tokens::color_role::ColorRole::OnSecondaryContainer,
        item_small_leading_video_height: crate::material::tokens::units::Dp(f32::from_bits(0x42600000)),
        item_small_leading_video_width: crate::material::tokens::units::Dp(f32::from_bits(0x42C80000)),
        item_supporting_text_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        item_supporting_text_font: crate::material::tokens::typography::TypeRole::BodyMedium,
        item_three_line_container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42B00000)),
        item_top_space: crate::material::tokens::units::Dp(f32::from_bits(0x41200000)),
        item_trailing_icon_color: crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        item_trailing_icon_expressive_size: crate::material::tokens::units::Dp(f32::from_bits(0x41A00000)),
        item_trailing_icon_size: crate::material::tokens::units::Dp(f32::from_bits(0x41C00000)),
        item_trailing_space: crate::material::tokens::units::Dp(f32::from_bits(0x41800000)),
        item_trailing_supporting_text_color:
            crate::material::tokens::color_role::ColorRole::OnSurfaceVariant,
        item_trailing_supporting_text_font: crate::material::tokens::typography::TypeRole::LabelSmall,
        item_two_line_container_height: crate::material::tokens::units::Dp(f32::from_bits(0x42900000)),
        item_unselected_trailing_icon_color: crate::material::tokens::color_role::ColorRole::OnSurface,
        segmented_gap: crate::material::tokens::units::Dp(f32::from_bits(0x40000000)),
    }
}
