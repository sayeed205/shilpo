use gpui::{App, Styled};

use crate::{
    ActiveTheme as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
};

#[inline]
pub(crate) fn clear_button(cx: &App) -> Button {
    Button::new("clean")
        .icon(Icon::new(IconName::CircleX))
        .text()
        .xsmall()
        .tab_stop(false)
        .text_color(cx.theme().on_surface_variant)
}
