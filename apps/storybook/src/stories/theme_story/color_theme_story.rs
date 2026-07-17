use gpui::*;
use gpui_component::{ActiveTheme as _, ThemeColor, h_flex, input::{Input, InputEvent, InputState}, v_flex};

const ROLES: &[(&str, fn(&ThemeColor) -> Hsla)] = &[
    ("surface", |t| t.surface),
    ("on_surface", |t| t.on_surface),
    ("surface_variant", |t| t.surface_variant),
    ("on_surface_variant", |t| t.on_surface_variant),
    ("outline", |t| t.outline),
    ("outline_variant", |t| t.outline_variant),
    ("primary", |t| t.primary),
    ("on_primary", |t| t.on_primary),
    ("primary_container", |t| t.primary_container),
    ("on_primary_container", |t| t.on_primary_container),
    ("secondary", |t| t.secondary),
    ("on_secondary", |t| t.on_secondary),
    ("secondary_container", |t| t.secondary_container),
    ("on_secondary_container", |t| t.on_secondary_container),
    ("tertiary", |t| t.tertiary),
    ("on_tertiary", |t| t.on_tertiary),
    ("tertiary_container", |t| t.tertiary_container),
    ("on_tertiary_container", |t| t.on_tertiary_container),
    ("error", |t| t.error),
    ("on_error", |t| t.on_error),
    ("error_container", |t| t.error_container),
    ("on_error_container", |t| t.on_error_container),
];

pub struct ThemeColorsStory {
    filter_input: Entity<InputState>,
}

impl crate::stories::Story for ThemeColorsStory {
    fn title() -> &'static str { "Material Theme Roles" }
    fn description() -> &'static str { "Material 3 roles generated from current source color and mode." }
    fn new_view(window: &mut Window, cx: &mut App) -> Entity<impl Render> { Self::view(window, cx) }
}

impl ThemeColorsStory {
    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder("Filter roles..."));
            cx.subscribe(&input, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) { cx.notify(); }
            }).detach();
            Self { filter_input: input }
        })
    }
}

impl Render for ThemeColorsStory {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.filter_input.read(cx).value().to_lowercase();
        let theme = cx.theme();
        v_flex()
            .size_full()
            .gap_4()
            .child(h_flex().gap_3().child(Input::new(&self.filter_input)).child(
                div().text_sm().child(format!("source #{:08x} · {:?}", theme.source_argb, theme.mode)),
            ))
            .child(div().flex().flex_wrap().gap_4().children(
                ROLES.iter().filter(|(name, _)| query.is_empty() || name.contains(&query)).map(|(name, role)| {
                    let color = role(&theme.colors);
                    v_flex()
                        .w(px(190.))
                        .gap_1()
                        .child(div().h(px(64.)).w_full().rounded(theme.radius).bg(color))
                        .child(div().text_sm().child(*name))
                        .child(div().text_xs().text_color(theme.on_surface_variant).child(hsla_to_hex(color)))
                }),
            ))
    }
}

fn hsla_to_hex(color: Hsla) -> String {
    let rgb = color.to_rgb();
    format!("#{:02x}{:02x}{:02x}", (rgb.r * 255.) as u8, (rgb.g * 255.) as u8, (rgb.b * 255.) as u8)
}
