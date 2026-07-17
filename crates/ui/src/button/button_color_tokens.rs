use gpui::Hsla;

use crate::ActiveTheme;

use super::ButtonVariant;

pub(crate) struct ButtonColorTokens {
    pub container: Hsla,
    pub content: Hsla,
    pub border: Hsla,
}

pub(crate) fn colors(variant: ButtonVariant, cx: &gpui::App) -> ButtonColorTokens {
    match variant {
        ButtonVariant::Filled => ButtonColorTokens {
            container: cx.theme().primary,
            content: cx.theme().on_primary,
            border: cx.theme().primary,
        },
        ButtonVariant::Elevated => ButtonColorTokens {
            container: cx.theme().surface_container_low,
            content: cx.theme().primary,
            border: cx.theme().surface_container_low,
        },
        ButtonVariant::FilledTonal => ButtonColorTokens {
            container: cx.theme().secondary_container,
            content: cx.theme().on_secondary_container,
            border: cx.theme().secondary_container,
        },
        ButtonVariant::Outlined => ButtonColorTokens {
            container: cx.theme().transparent,
            content: cx.theme().on_surface_variant,
            border: cx.theme().outline_variant,
        },
        ButtonVariant::Text => ButtonColorTokens {
            container: cx.theme().transparent,
            content: cx.theme().on_surface_variant,
            border: cx.theme().transparent,
        },
    }
}
