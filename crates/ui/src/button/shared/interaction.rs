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
}
