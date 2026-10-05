use amane::{
    Button, Center, Color, Column, Cursor, Parent, Rectangle, Row, Service, SpaceBetween, Text,
    Weight, Window, children,
};

use crate::components::button;
use crate::fonts;
use crate::theme::{self, Theme};

const NAME: &str = "showcase";
const WIDTH: f32 = 840.0;
const HEIGHT: f32 = 620.0;
const MARGIN: f32 = 24.0;
const HEADER_HEIGHT: f32 = 48.0;
const CONTENT_GAP: f32 = 20.0;

struct Activations(usize);

impl Service for Activations {
    fn new() -> Self {
        Self(0)
    }

    // the action count changes only when the enabled sample is clicked
    fn listen() {}
}

struct IconActivations(usize);

impl Service for IconActivations {
    fn new() -> Self {
        Self(0)
    }

    fn listen() {}
}

/// The button playground, opened on demand through `amane ipc call showcase`.
pub fn view() -> Window {
    let theme = theme::current();
    let activations = Activations::read().0;
    let icon_activations = IconActivations::read().0;

    let body = Column::new(children![
        introduction(&theme),
        samples(&theme, activations, icon_activations),
        specifications(&theme),
        Text::new("Press a button to see its ripple; the gear demo updates its count without opening Settings.")
            .size(12.0)
            .font(fonts::BODY)
            .color(theme.secondary_text),
    ])
    .gap(CONTENT_GAP);

    let frame = Rectangle::new()
        .width(Parent)
        .height(Parent)
        .fill(theme.background)
        .padding(MARGIN)
        .child(Column::new(children![header(&theme), body]).gap(CONTENT_GAP));

    Window::new()
        .title("Showcase")
        .size(WIDTH, HEIGHT)
        .child(frame)
}

fn header(theme: &Theme) -> Row {
    let mark = Rectangle::new()
        .width(44.0)
        .height(44.0)
        .radius(15.0)
        .fill(theme.selected_surface)
        .align_child(Center, Center)
        .child(
            Text::new("󰄄")
                .size(21.0)
                .font(fonts::NERD)
                .tight()
                .color(theme.accent),
        );

    let title = Column::new(children![
        Text::new("Showcase")
            .size(20.0)
            .font(fonts::BODY)
            .weight(Weight::Bold)
            .color(theme.text),
        Text::new("A live study of expressive interaction")
            .size(12.0)
            .font(fonts::BODY)
            .color(theme.secondary_text),
    ])
    .gap(2.0);

    let close = Rectangle::new()
        .width(40.0)
        .height(40.0)
        .radius(12.0)
        .fill(Color::TRANSPARENT)
        .cursor(Cursor::Pointer)
        .on_click(|button| {
            if button == Button::Left {
                amane::close_window(NAME);
            }
        })
        .align_child(Center, Center)
        .child(
            Text::new("󰅖")
                .size(18.0)
                .font(fonts::NERD)
                .tight()
                .color(theme.secondary_text),
        );

    Row::new(children![
        Row::new(children![mark, title]).gap(12.0).align(Center),
        close
    ])
    .width(Parent)
    .height(HEADER_HEIGHT)
    .justify(SpaceBetween)
    .align(Center)
}

fn introduction(theme: &Theme) -> Column {
    Column::new(children![
        Text::new("Filled buttons")
            .size(22.0)
            .font(fonts::BODY)
            .weight(Weight::Bold)
            .color(theme.text),
        Text::new("A filled action and compact icon counterpart, both with an expressive press.")
            .size(13.0)
            .font(fonts::BODY)
            .color(theme.secondary_text),
    ])
    .gap(4.0)
}

fn samples(theme: &Theme, activations: usize, icon_activations: usize) -> Rectangle {
    let enabled_label = if activations == 1 {
        String::from("Action ran once")
    } else {
        format!("Action ran {activations} times")
    };

    let enabled = Column::new(children![
        Text::new("FILLED · ENABLED")
            .size(10.0)
            .font(fonts::BODY)
            .weight(Weight::Bold)
            .color(theme.muted_text),
        button::filled("showcase.filled.enabled", theme, "Run action", true, || {
            Activations::write().0 += 1;
        }),
        Text::new(enabled_label)
            .size(12.0)
            .font(fonts::BODY)
            .color(theme.secondary_text),
    ])
    .gap(12.0);

    let disabled = Column::new(children![
        Text::new("FILLED · DISABLED")
            .size(10.0)
            .font(fonts::BODY)
            .weight(Weight::Bold)
            .color(theme.muted_text),
        button::filled(
            "showcase.filled.disabled",
            theme,
            "Unavailable",
            false,
            || {}
        ),
        Text::new("No input or activation")
            .size(12.0)
            .font(fonts::BODY)
            .color(theme.secondary_text),
    ])
    .gap(12.0);

    let icon_enabled_label = if icon_activations == 1 {
        String::from("Icon action ran once")
    } else {
        format!("Icon action ran {icon_activations} times")
    };

    let icon_enabled = Column::new(children![
        Text::new("ICON · ENABLED")
            .size(10.0)
            .font(fonts::BODY)
            .weight(Weight::Bold)
            .color(theme.muted_text),
        button::filled_icon(
            "showcase.icon.enabled",
            theme,
            button::settings_fill(),
            true,
            || IconActivations::write().0 += 1,
        ),
        Text::new(icon_enabled_label)
            .size(12.0)
            .font(fonts::BODY)
            .color(theme.secondary_text),
    ])
    .gap(12.0);

    let icon_disabled = Column::new(children![
        Text::new("ICON · DISABLED")
            .size(10.0)
            .font(fonts::BODY)
            .weight(Weight::Bold)
            .color(theme.muted_text),
        button::filled_icon(
            "showcase.icon.disabled",
            theme,
            button::settings_fill(),
            false,
            || {},
        ),
        Text::new("No input or activation")
            .size(12.0)
            .font(fonts::BODY)
            .color(theme.secondary_text),
    ])
    .gap(12.0);

    let separator = || {
        Rectangle::new()
            .width(1.0)
            .height(110.0)
            .fill(theme::with_opacity(theme.border, 0.65))
    };

    let examples = Row::new(children![
        enabled,
        separator(),
        disabled,
        separator(),
        icon_enabled,
        separator(),
        icon_disabled,
    ])
        .width(Parent)
        .gap(20.0)
        .align(Center);

    let heading = Row::new(children![
        Text::new("FILLED + ICON SPECIMENS")
            .size(11.0)
            .font(fonts::BODY)
            .weight(Weight::Bold)
            .color(theme.secondary_text),
        Text::new("Press · hold · release")
            .size(11.0)
            .font(fonts::BODY)
            .color(theme.muted_text),
    ])
    .width(Parent)
    .justify(SpaceBetween)
    .align(Center);

    Rectangle::new()
        .width(Parent)
        .height(238.0)
        .radius(22.0)
        .fill(theme.surface)
        .border(1.0, theme::with_opacity(theme.border, 0.48))
        .padding(22.0)
        .child(Column::new(children![heading, examples]).gap(30.0))
}

fn specifications(theme: &Theme) -> Row {
    Row::new(children![
        token(theme, "40 px", "button height"),
        token(theme, "20 → 8 px", "corner radius"),
        token(theme, "1600 · 1.0", "stiffness · damping"),
        token(theme, "10% · 38%", "disabled fill · label"),
    ])
    .width(Parent)
    .gap(12.0)
}

fn token(theme: &Theme, value: &str, caption: &str) -> Rectangle {
    Rectangle::new()
        .width(Parent)
        .height(68.0)
        .radius(16.0)
        .fill(theme.surface)
        .padding(12.0)
        .child(
            Column::new(children![
                Text::new(value)
                    .size(14.0)
                    .font(fonts::BODY)
                    .weight(Weight::SemiBold)
                    .color(theme.text),
                Text::new(caption)
                    .size(10.0)
                    .font(fonts::BODY)
                    .color(theme.secondary_text),
            ])
            .gap(3.0),
        )
}

pub fn ipc(_arguments: &[String]) -> String {
    amane::open_window(NAME, view);

    String::from("ok")
}
