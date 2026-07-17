use gpui::Hsla;

use super::{ButtonVariant, button_color_tokens::colors};

#[derive(Clone, Copy)]
pub(crate) enum ButtonElevation {
    None,
    Level1,
}

pub(crate) struct ButtonTokens {
    pub container: Hsla,
    pub content: Hsla,
    pub border: Hsla,
    pub elevation: ButtonElevation,
}

pub(crate) fn tokens(variant: ButtonVariant, cx: &gpui::App) -> ButtonTokens {
    let color = colors(variant, cx);
    ButtonTokens {
        container: color.container,
        content: color.content,
        border: color.border,
        elevation: match variant {
            ButtonVariant::Elevated => ButtonElevation::Level1,
            _ => ButtonElevation::None,
        },
    }
}
