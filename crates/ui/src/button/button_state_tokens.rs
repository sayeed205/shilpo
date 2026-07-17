use crate::theme::Colorize as _;
use gpui::{Background, Hsla};

#[derive(Clone, Copy)]
pub(crate) struct ButtonStateTokens {
    pub hover: f32,
    pub focus: f32,
    pub pressed: f32,
    pub dragged: f32,
}

pub(crate) fn tokens() -> ButtonStateTokens {
    ButtonStateTokens {
        hover: 0.08,
        focus: 0.10,
        pressed: 0.10,
        dragged: 0.16,
    }
}

pub(crate) fn state_layer(container: Hsla, content: Hsla, opacity: f32) -> Background {
    if container.a == 0. {
        content.opacity(opacity).into()
    } else {
        container.mix_oklab(content, 1. - opacity).into()
    }
}
