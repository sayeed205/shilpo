use amane::{Canvas, Path, Rectangle, Shape};

use crate::theme::{self, Theme};

use super::{build_button, HEIGHT};

const WIDTH: f32 = HEIGHT;
const ICON_SIZE: f32 = 24.0;
const HOVER_ALPHA: f32 = 0.08;

/// Draw a small filled Material 3 Expressive icon button.
///
/// `name` is a stable, globally unique identity shared with the other button styles. The icon is a
/// filled Amane shape, so its tint follows the button's enabled state. The callback runs only after
/// a left-button release over the button.
pub fn filled(
    name: &'static str,
    theme: &Theme,
    icon: impl Shape + 'static,
    enabled: bool,
    on_activate: impl Fn() + 'static,
) -> Rectangle {
    let (fill, tint) = if enabled {
        (theme.accent, theme.on_accent)
    } else {
        (
            theme::with_opacity(theme.text, 0.10),
            theme::with_opacity(theme.text, 0.38),
        )
    };

    let icon = icon.fill(tint);
    let content = Canvas::new()
        .width(ICON_SIZE)
        .height(ICON_SIZE)
        .shapes(vec![Box::new(icon)]);

    build_button(
        name,
        theme,
        enabled,
        WIDTH,
        HEIGHT,
        fill,
        content,
        HOVER_ALPHA,
        false,
        on_activate,
    )
}

/// The supplied 24 px gear SVG as an Amane shape, retaining its original viewBox geometry.
///
/// Amane rasterizes SVG files but does not expose image tinting. Keeping this as vector path data lets
/// the icon button apply the current theme color without changing the checked-in SVG asset.
pub fn settings_fill() -> Path {
    // Generated from assets/settings_fill.svg: x / 40, (y + 960) / 40 maps its 960-square viewBox
    // to the original 24 px icon size. The SVG asset remains the source reference for these curves.
    Path::new()
        .move_to(10.825, 22.0)
        .quad_to(10.15, 22.0, 9.6625, 21.55)
        .quad_to(9.175, 21.1, 9.075, 20.45)
        .line_to(8.85, 18.8)
        .quad_to(8.525, 18.675, 8.2375, 18.5)
        .quad_to(7.95, 18.325, 7.675, 18.125)
        .line_to(6.125, 18.775)
        .quad_to(5.5, 19.05, 4.875, 18.825)
        .quad_to(4.25, 18.6, 3.9, 18.025)
        .line_to(2.725, 15.975)
        .quad_to(2.375, 15.4, 2.525, 14.75)
        .quad_to(2.675, 14.1, 3.2, 13.675)
        .line_to(4.525, 12.675)
        .quad_to(4.5, 12.5, 4.5, 12.3375)
        .line_to(4.5, 11.6625)
        .quad_to(4.5, 11.5, 4.525, 11.325)
        .line_to(3.2, 10.325)
        .quad_to(2.675, 9.9, 2.525, 9.25)
        .quad_to(2.375, 8.6, 2.725, 8.025)
        .line_to(3.9, 5.975)
        .quad_to(4.25, 5.4, 4.875, 5.175)
        .quad_to(5.5, 4.95, 6.125, 5.225)
        .line_to(7.675, 5.875)
        .quad_to(7.95, 5.675, 8.25, 5.5)
        .quad_to(8.55, 5.325, 8.85, 5.2)
        .line_to(9.075, 3.55)
        .quad_to(9.175, 2.9, 9.6625, 2.45)
        .quad_to(10.15, 2.0, 10.825, 2.0)
        .line_to(13.175, 2.0)
        .quad_to(13.85, 2.0, 14.3375, 2.45)
        .quad_to(14.825, 2.9, 14.925, 3.55)
        .line_to(15.15, 5.2)
        .quad_to(15.475, 5.325, 15.7625, 5.5)
        .quad_to(16.05, 5.675, 16.325, 5.875)
        .line_to(17.875, 5.225)
        .quad_to(18.5, 4.95, 19.125, 5.175)
        .quad_to(19.75, 5.4, 20.1, 5.975)
        .line_to(21.275, 8.025)
        .quad_to(21.625, 8.6, 21.475, 9.25)
        .quad_to(21.325, 9.9, 20.8, 10.325)
        .line_to(19.475, 11.325)
        .quad_to(19.5, 11.5, 19.5, 11.6625)
        .line_to(19.5, 12.3375)
        .quad_to(19.5, 12.5, 19.45, 12.675)
        .line_to(20.775, 13.675)
        .quad_to(21.3, 14.1, 21.45, 14.75)
        .quad_to(21.6, 15.4, 21.25, 15.975)
        .line_to(20.05, 18.025)
        .quad_to(19.7, 18.6, 19.075, 18.825)
        .quad_to(18.45, 19.05, 17.825, 18.775)
        .line_to(16.325, 18.125)
        .quad_to(16.05, 18.325, 15.75, 18.5)
        .quad_to(15.45, 18.675, 15.15, 18.8)
        .line_to(14.925, 20.45)
        .quad_to(14.825, 21.1, 14.3375, 21.55)
        .quad_to(13.85, 22.0, 13.175, 22.0)
        .line_to(10.825, 22.0)
        .close()
        .move_to(12.05, 15.5)
        .quad_to(13.5, 15.5, 14.525, 14.475)
        .quad_to(15.55, 13.45, 15.55, 12.0)
        .quad_to(15.55, 10.55, 14.525, 9.525)
        .quad_to(13.5, 8.5, 12.05, 8.5)
        .quad_to(10.575, 8.5, 9.5625, 9.525)
        .quad_to(8.55, 10.55, 8.55, 12.0)
        .quad_to(8.55, 13.45, 9.5625, 14.475)
        .quad_to(10.575, 15.5, 12.05, 15.5)
        .close()
}

#[cfg(test)]
mod tests {
    const SETTINGS_FILL: &str = include_str!("assets/settings_fill.svg");

    #[test]
    fn supplied_settings_icon_keeps_its_original_svg_asset_and_view_box() {
        assert!(SETTINGS_FILL.contains("viewBox=\"0 -960 960 960\""));
        assert!(SETTINGS_FILL.contains("width=\"24px\""));
        assert!(SETTINGS_FILL.contains("height=\"24px\""));
        assert!(SETTINGS_FILL.contains("fill=\"#e3e3e3\""));
        assert!(SETTINGS_FILL.contains("M433-80q-27 0-46.5-18T363-142"));
    }
}
