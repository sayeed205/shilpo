use gpui::App;

use crate::{
    Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
};

#[inline]
pub(crate) fn clear_button(_cx: &App) -> Button {
    Button::new("clean")
        .icon(Icon::new(IconName::Cancel))
        .plain()
        .xsmall()
        .tab_stop(false)
}
