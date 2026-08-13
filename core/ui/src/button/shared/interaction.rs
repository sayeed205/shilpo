use gpui::{Background, CursorStyle, Hsla};

use crate::theme::Colorize as _;

pub(crate) fn state_layer(container: Hsla, role: Hsla, opacity: f32) -> Background {
    if container.a == 0. {
        role.opacity(opacity).into()
    } else {
        // Mix role color into existing container instead of replacing its alpha.
        container.mix_oklab(role, 1. - opacity).into()
    }
}

pub(crate) fn cursor(disabled: bool, loading: bool, explicit: Option<CursorStyle>) -> CursorStyle {
    if disabled || loading {
        CursorStyle::OperationNotAllowed
    } else {
        explicit.unwrap_or(CursorStyle::PointingHand)
    }
}

#[cfg(test)]
mod tests {
    use gpui::hsla;

    use super::*;

    #[test]
    fn disabled_cursor_wins_over_explicit_cursor() {
        assert_eq!(
            cursor(true, false, Some(CursorStyle::Arrow)),
            CursorStyle::OperationNotAllowed
        );
        assert_eq!(
            cursor(false, false, Some(CursorStyle::Arrow)),
            CursorStyle::Arrow
        );
    }

    #[test]
    fn state_layer_distinguishes_transparent_and_semitransparent_bases() {
        let role = hsla(0.6, 0.8, 0.5, 1.);
        let transparent = state_layer(Hsla::transparent_black(), role, 0.1);
        let semitransparent = state_layer(hsla(0., 0., 0.2, 0.5), role, 0.1);
        let alternate_role = state_layer(Hsla::transparent_black(), hsla(0.1, 0.8, 0.5, 1.), 0.1);
        assert_ne!(transparent, semitransparent);
        assert_ne!(transparent, alternate_role);
    }
}
