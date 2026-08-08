use crate::runtime::ShellRuntime;
use gpui::{
    App, AppContext, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement,
    Styled, Window, div, px,
};
use shilpo_ext::{
    ContainerDirection, SemanticColorToken, ViewNode, ViewStyle, ViewTree,
};
use shilpo_ext_types::CanonicalId;
use shilpo_ui::{
    ActiveTheme, Icon, Sizable, Size,
    input::{Input, InputEvent, InputState},
    progress::LoadingIndicator,
    slider::{Slider, SliderEvent, SliderState, SliderValue},
};

pub fn render_ext_view_tree(
    contribution: &CanonicalId,
    instance_id: Option<&str>,
    tree: &ViewTree,
    window: &mut Window,
    cx: &mut App,
) -> gpui::AnyElement {
    render_view_node(contribution, instance_id, &tree.root, window, cx)
}

fn render_view_node(
    contribution: &CanonicalId,
    instance_id: Option<&str>,
    node: &ViewNode,
    window: &mut Window,
    cx: &mut App,
) -> gpui::AnyElement {
    match node {
        ViewNode::Container(c) => {
            let mut container = div().flex();
            container = match c.direction {
                ContainerDirection::Row => container.flex_row().items_center(),
                ContainerDirection::Column => container.flex_col(),
                ContainerDirection::Stack => container.relative(),
            };

            if let Some(gap) = c.gap {
                container = container.gap(px(gap));
            }

            if let Some(style) = &c.style {
                container = apply_view_style(container, style, cx);
            }

            for child in &c.children {
                let child = render_view_node(contribution, instance_id, child, window, cx);
                container = if c.direction == ContainerDirection::Stack {
                    container.child(div().absolute().inset_0().child(child))
                } else {
                    container.child(child)
                };
            }

            container.into_any_element()
        }
        ViewNode::Text(t) => {
            let mut el = div().child(t.content.clone());
            if let Some(style) = &t.style {
                el = apply_view_style(el, style, cx);
            }
            if let Some(sz) = t.font_size {
                el = el.text_size(px(sz));
            }
            if t.bold == Some(true) {
                el = el.font_weight(gpui::FontWeight::BOLD);
            }
            el.into_any_element()
        }
        ViewNode::Icon(icon) => {
            let mut glyph = Icon::default().path(format!("icons/{}.svg", icon.name));
            if let Some(size) = icon.size {
                glyph = glyph.size(px(size));
            }
            let mut el = div().child(glyph);
            if let Some(style) = &icon.style {
                el = apply_view_style(el, style, cx);
            }
            el.into_any_element()
        }
        ViewNode::Image(img) => {
            let asset = ShellRuntime::extension_asset_path(cx, contribution, &img.asset_path).ok();
            let mut el = div();
            if let Some(asset) = asset {
                el = el.child(gpui::img(asset));
            }
            if let Some(style) = &img.style {
                el = apply_view_style(el, style, cx);
            }
            if let Some(w) = img.width {
                el = el.w(px(w));
            }
            if let Some(h) = img.height {
                el = el.h(px(h));
            }
            el.into_any_element()
        }
        ViewNode::Button(btn) => {
            let contribution = contribution.clone();
            let instance_id = instance_id.map(ToOwned::to_owned);
            let event_id = btn.event_id.clone();
            let mut base = div()
                .px_3()
                .py_1()
                .rounded_md()
                .bg(cx.theme().primary)
                .text_color(cx.theme().on_primary)
                .child(btn.label.clone());
            if let Some(style) = &btn.style {
                base = apply_view_style(base, style, cx);
            }
            base.id(format!("ext:{contribution}:button:{event_id}"))
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                    ShellRuntime::dispatch_extension_input(
                        cx,
                        &contribution,
                        instance_id.as_deref(),
                        event_id.clone(),
                        None,
                    );
                })
                .into_any_element()
        }
        ViewNode::IconButton(ibtn) => {
            let contribution = contribution.clone();
            let instance_id = instance_id.map(ToOwned::to_owned);
            let event_id = ibtn.event_id.clone();
            let mut base = div()
                .p_1()
                .rounded_full()
                .bg(cx.theme().surface_container)
                .child(Icon::default().path(format!("icons/{}.svg", ibtn.icon_name)));
            if let Some(style) = &ibtn.style {
                base = apply_view_style(base, style, cx);
            }
            base.id(format!("ext:{contribution}:icon-button:{event_id}"))
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                    ShellRuntime::dispatch_extension_input(
                        cx,
                        &contribution,
                        instance_id.as_deref(),
                        event_id.clone(),
                        None,
                    );
                })
                .into_any_element()
        }
        ViewNode::Toggle(t) => {
            let contribution = contribution.clone();
            let instance_id = instance_id.map(ToOwned::to_owned);
            let event_id = t.event_id.clone();
            let value = !t.value;
            let bg = if t.value {
                cx.theme().primary
            } else {
                cx.theme().surface_container
            };
            let mut base = div().w(px(36.0)).h(px(20.0)).rounded_full().bg(bg);
            if let Some(style) = &t.style {
                base = apply_view_style(base, style, cx);
            }
            base.id(format!("ext:{contribution}:toggle:{event_id}"))
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                    ShellRuntime::dispatch_extension_input(
                        cx,
                        &contribution,
                        instance_id.as_deref(),
                        event_id.clone(),
                        Some(value.into()),
                    );
                })
                .into_any_element()
        }
        ViewNode::Slider(s) => {
            let contribution = contribution.clone();
            let instance_id = instance_id.map(ToOwned::to_owned);
            let event_id = s.event_id.clone();
            let state = cx.new(|_| {
                SliderState::new()
                    .min(s.min)
                    .max(s.max)
                    .step(((s.max - s.min) / 100.0).max(f32::EPSILON))
                    .default_value(s.value)
            });
            cx.subscribe(&state, move |_, event: &SliderEvent, cx| {
                if let SliderEvent::Change(SliderValue::Single(value))
                | SliderEvent::Release(SliderValue::Single(value)) = event
                {
                    ShellRuntime::dispatch_extension_input(
                        cx,
                        &contribution,
                        instance_id.as_deref(),
                        event_id.clone(),
                        Some((*value).into()),
                    );
                }
            })
            .detach();
            let mut base = div().w_full().child(Slider::new(&state));
            if let Some(style) = &s.style {
                base = apply_view_style(base, style, cx);
            }
            base.into_any_element()
        }
        ViewNode::TextInput(input) => {
            let contribution = contribution.clone();
            let instance_id = instance_id.map(ToOwned::to_owned);
            let event_id = input.event_id.clone();
            let placeholder = input.placeholder.clone().unwrap_or_default();
            let value = input.value.clone();
            let state = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .default_value(value)
            });
            cx.subscribe(&state, move |state, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = state.read(cx).value().to_string();
                    ShellRuntime::dispatch_extension_input(
                        cx,
                        &contribution,
                        instance_id.as_deref(),
                        event_id.clone(),
                        Some(value.into()),
                    );
                }
            })
            .detach();
            let mut base = div().w_full().child(Input::new(&state));
            if let Some(style) = &input.style {
                base = apply_view_style(base, style, cx);
            }
            base.into_any_element()
        }
        ViewNode::List(l) => {
            let mut el = div().flex().flex_col();
            if let Some(style) = &l.style {
                el = apply_view_style(el, style, cx);
            }
            for item in &l.items {
                el = el.child(render_view_node(
                    contribution,
                    instance_id,
                    item,
                    window,
                    cx,
                ));
            }
            el.into_any_element()
        }
        ViewNode::Spacer(sp) => {
            let sz = sp.size.unwrap_or(8.0);
            div().w(px(sz)).h(px(sz)).into_any_element()
        }
        ViewNode::Divider => div()
            .h(px(1.0))
            .w_full()
            .bg(cx.theme().outline)
            .into_any_element(),
        ViewNode::Badge(b) => {
            let mut el = div()
                .px_2()
                .py_0p5()
                .rounded_full()
                .bg(cx.theme().secondary)
                .text_color(cx.theme().on_primary)
                .text_xs()
                .child(b.label.clone());
            if let Some(style) = &b.style {
                el = apply_view_style(el, style, cx);
            }
            el.into_any_element()
        }
        ViewNode::Progress(p) => {
            let progress_pct = p.value.clamp(0.0, 1.0);
            let mut el = div()
                .h(px(4.0))
                .w_full()
                .rounded_full()
                .bg(cx.theme().surface_container)
                .child(
                    div()
                        .h_full()
                        .w(gpui::DefiniteLength::Fraction(progress_pct))
                        .bg(cx.theme().primary)
                        .rounded_full(),
                );
            if let Some(style) = &p.style {
                el = apply_view_style(el, style, cx);
            }
            el.into_any_element()
        }
        ViewNode::LoadingIndicator(indicator) => {
            let id = format!(
                "ext:{contribution}:{}:loading-indicator",
                instance_id.unwrap_or("shared")
            );
            let size = indicator
                .size
                .map_or(Size::XSmall, |size| Size::Size(px(size)));
            let color = indicator
                .color
                .map(|token| resolve_color_token(token, cx))
                .unwrap_or(cx.theme().primary);
            let mut el = div()
                .flex()
                .items_center()
                .justify_center()
                .child(LoadingIndicator::new(id).with_size(size).color(color));
            if let Some(style) = &indicator.style {
                el = apply_view_style(el, style, cx);
            }
            el.into_any_element()
        }
    }
}

fn apply_view_style(mut div: gpui::Div, style: &ViewStyle, cx: &App) -> gpui::Div {
    if let Some(p) = style.padding {
        div = div.p(px(p));
    }
    if let Some(m) = style.margin {
        div = div.m(px(m));
    }
    if let Some(w) = style.width {
        div = div.w(px(w));
    }
    if let Some(h) = style.height {
        div = div.h(px(h));
    }
    if let Some(r) = style.corner_radius {
        div = div.rounded(px(r));
    }
    if let Some(o) = style.opacity {
        div = div.opacity(o);
    }
    if let Some(fg) = style.flex_grow {
        div = div.flex_grow(fg);
    }
    if let Some(c) = style.color {
        div = div.text_color(resolve_color_token(c, cx));
    }
    if let Some(bg) = style.background {
        div = div.bg(resolve_color_token(bg, cx));
    }
    div
}

fn resolve_color_token(token: SemanticColorToken, cx: &App) -> gpui::Hsla {
    match token {
        SemanticColorToken::Primary => cx.theme().primary,
        SemanticColorToken::OnPrimary => cx.theme().on_primary,
        SemanticColorToken::Secondary => cx.theme().secondary,
        SemanticColorToken::Surface => cx.theme().surface,
        SemanticColorToken::SurfaceContainer => cx.theme().surface_container,
        SemanticColorToken::OnSurface => cx.theme().on_surface,
        SemanticColorToken::OnSurfaceVariant => cx.theme().on_surface_variant,
        SemanticColorToken::Outline => cx.theme().outline,
        SemanticColorToken::Error => cx.theme().error,
    }
}
