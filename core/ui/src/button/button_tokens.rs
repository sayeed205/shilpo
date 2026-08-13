#[cfg(test)]
use std::cell::RefCell;

use gpui::Hsla;

use super::ButtonVariant;
use crate::{ActiveTheme, theme::Colorize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ButtonPaintState {
    Rest,
    Hover,
    Focus,
    Pressed,
    Disabled,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ResolvedButtonPaint {
    pub container: Hsla,
    pub content: Hsla,
    pub border: Hsla,
    pub elevation: u8,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RenderPaintCapture {
    pub variant: ButtonVariant,
    pub base: ResolvedButtonPaint,
    pub hover: ResolvedButtonPaint,
    pub focus: ResolvedButtonPaint,
    pub pressed: ResolvedButtonPaint,
    pub disabled: ResolvedButtonPaint,
}

#[cfg(test)]
thread_local! {
    static CAPTURED_RENDER_PAINT: RefCell<Option<RenderPaintCapture>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub(crate) struct RenderPaintCaptureGuard {
    previous: Option<RenderPaintCapture>,
}

#[cfg(test)]
impl Drop for RenderPaintCaptureGuard {
    fn drop(&mut self) {
        CAPTURED_RENDER_PAINT.with(|capture| *capture.borrow_mut() = self.previous);
    }
}

#[cfg(test)]
pub(crate) fn capture_render_paint() -> RenderPaintCaptureGuard {
    let previous = CAPTURED_RENDER_PAINT.with(|capture| capture.borrow_mut().take());
    RenderPaintCaptureGuard { previous }
}

#[cfg(test)]
pub(crate) fn captured_render_paint() -> Option<RenderPaintCapture> {
    CAPTURED_RENDER_PAINT.with(|capture| *capture.borrow())
}

#[cfg(test)]
pub(crate) fn record_render_paint(capture: RenderPaintCapture) {
    CAPTURED_RENDER_PAINT.with(|value| *value.borrow_mut() = Some(capture));
}

fn role(role: super::button_scale_tokens::ThemeRole, cx: &gpui::App) -> Hsla {
    use super::button_scale_tokens::ThemeRole::*;
    match role {
        Transparent => cx.theme().transparent,
        Primary => cx.theme().primary,
        OnPrimary => cx.theme().on_primary,
        PrimaryContainer => cx.theme().primary_container,
        OnPrimaryContainer => cx.theme().on_primary_container,
        SecondaryContainer => cx.theme().secondary_container,
        OnSecondaryContainer => cx.theme().on_secondary_container,
        SurfaceContainerLow => cx.theme().surface_container_low,
        OnSurface => cx.theme().on_surface,
        OnSurfaceVariant => cx.theme().on_surface_variant,
        Outline => cx.theme().outline,
        OutlineVariant => cx.theme().outline_variant,
        SurfaceVariant => cx.theme().surface_variant,
    }
}

pub(crate) fn resolved_paint(
    variant: ButtonVariant,
    state: ButtonPaintState,
    cx: &gpui::App,
) -> ResolvedButtonPaint {
    let table = super::button_scale_tokens::button_semantic_table(variant);
    let semantic = if state == ButtonPaintState::Disabled {
        table.disabled
    } else {
        table.base
    };
    let mut container = role(semantic.container, cx).opacity(semantic.container_opacity);
    let mut content = role(semantic.content, cx).opacity(semantic.content_opacity);
    let border = role(semantic.border, cx).opacity(semantic.border_opacity);
    let elevation = match state {
        ButtonPaintState::Rest => table.elevation_rest,
        ButtonPaintState::Hover => table.elevation_hover,
        ButtonPaintState::Focus => table.elevation_focus,
        ButtonPaintState::Pressed => table.elevation_pressed,
        ButtonPaintState::Disabled => table.elevation_disabled,
    };
    if variant == ButtonVariant::Plain {
        if matches!(
            state,
            ButtonPaintState::Hover | ButtonPaintState::Focus | ButtonPaintState::Pressed
        ) {
            content = role(super::button_scale_tokens::ThemeRole::OnSurface, cx);
        }
    } else if matches!(
        state,
        ButtonPaintState::Hover | ButtonPaintState::Focus | ButtonPaintState::Pressed
    ) {
        let layer = role(table.state_layer, cx);
        let opacity = match state {
            ButtonPaintState::Hover => 0.08,
            ButtonPaintState::Focus => 0.10,
            _ => 0.10,
        };
        container = if container.a == 0. {
            layer.opacity(opacity)
        } else {
            container.mix_oklab(layer, 1. - opacity)
        };
    }
    ResolvedButtonPaint {
        container,
        content,
        border,
        elevation,
    }
}
