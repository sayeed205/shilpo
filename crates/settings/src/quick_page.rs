use gpui::{
    App, ImageSource, IntoElement, ObjectFit, ParentElement, SharedString, Styled, StyledImage,
    Window, div, img, prelude::FluentBuilder as _, px,
};
use shilpo_theme::{SchemeVariant, ThemeClient, ThemeMode};
use shilpo_ui::scroll::ScrollableElement;
use shilpo_ui::{
    ActiveTheme, Icon, IconName, Selectable, Sizable, StyledExt,
    button::{Button, ButtonGroup, ButtonGroupMode, ButtonVariants},
    h_flex, v_flex,
};

/// Material 3 scheme variant labels.
const SCHEME_VARIANTS: &[&str] = &[
    "Auto",
    "Tonal Spot",
    "Content",
    "Expressive",
    "Fidelity",
    "Fruit Salad",
    "Monochrome",
    "Neutral",
    "Rainbow",
];

/// Deep module for the Quick Settings page.
pub struct QuickPage;

impl QuickPage {
    /// Render the Quick Settings page containing Wallpaper & Color controls.
    pub fn render(
        theme_client: &ThemeClient,
        _window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let state = theme_client.current_state();

        let current_mode = cx.theme().selected_mode();
        let wallpaper_path = state.wallpaper_path.clone();

        v_flex()
            .flex_1()
            .w_full()
            .gap_4()
            .overflow_y_scrollbar()
            // ── Section: Wallpaper & Colors ──
            .child(Self::render_wallpaper_section(
                theme_client,
                wallpaper_path.as_deref(),
                current_mode,
                cx,
            ))
    }

    /// The main Wallpaper & Colors section:
    /// wallpaper preview on the left, controls on the right.
    fn render_wallpaper_section(
        theme_client: &ThemeClient,
        wallpaper_path: Option<&std::path::Path>,
        current_mode: ThemeMode,
        cx: &App,
    ) -> impl IntoElement {
        let client_random = theme_client.clone();
        let client_light = theme_client.clone();
        let client_dark = theme_client.clone();
        let current_variant = cx.theme().scheme_variant;

        // ── Title Row ──
        let title_row = h_flex()
            .gap_2()
            .items_center()
            .child(
                Icon::new(IconName::Palette)
                    .size(px(22.))
                    .text_color(cx.theme().primary),
            )
            .child(
                div()
                    .text_lg()
                    .font_bold()
                    .text_color(cx.theme().on_surface)
                    .child("Wallpaper & Colors"),
            );

        // ── Main Content: Preview (left) + Controls (right) ──
        let main_content = h_flex()
            .gap_4()
            .w_full()
            .items_start()
            // Left: Wallpaper Preview
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(320.))
                    .h(px(200.))
                    .rounded_xl()
                    .overflow_hidden()
                    .bg(cx.theme().surface_container_high)
                    .relative()
                    .when_some(
                        wallpaper_path.map(|p| ImageSource::from(p.to_path_buf())),
                        |container, source| {
                            container.child(
                                img(source)
                                    .absolute()
                                    .inset_0()
                                    .size_full()
                                    .rounded_xl()
                                    .object_fit(ObjectFit::Cover),
                            )
                        },
                    )
                    .when(wallpaper_path.is_none(), |container| {
                        container.child(
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    Icon::new(IconName::Palette)
                                        .size(px(48.))
                                        .text_color(cx.theme().on_surface_variant),
                                ),
                        )
                    }),
            )
            // Right: Controls stack
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_3()
                    // Random Wallpaper button
                    .child(
                        Button::new("random-wallpaper")
                            .icon(IconName::Refresh)
                            .label("Random Wallpaper")
                            .outline()
                            .small()
                            .on_click(move |_, _, _| {
                                let client = client_random.clone();
                                ThemeClient::spawn_task(async move {
                                    let _ = client.set_random_wallpaper().await;
                                });
                            }),
                    )
                    // Light / Dark mode ButtonGroup with M3 Expressive morphing
                    .child(
                        ButtonGroup::new("mode-toggle")
                            .mode(ButtonGroupMode::Connected)
                            .filled_tonal()
                            .child(
                                Button::new("mode-light")
                                    .icon(IconName::Sunny)
                                    .label("Light")
                                    .selected(current_mode == ThemeMode::Light)
                                    .on_click({
                                        let client = client_light.clone();
                                        move |_, _, _| {
                                            let client = client.clone();
                                            ThemeClient::spawn_task(async move {
                                                let _ = client.set_mode(ThemeMode::Light).await;
                                            });
                                        }
                                    }),
                            )
                            .child(
                                Button::new("mode-dark")
                                    .icon(IconName::MoonStars)
                                    .label("Dark")
                                    .selected(
                                        current_mode == ThemeMode::Dark
                                            || current_mode == ThemeMode::System,
                                    )
                                    .on_click({
                                        let client = client_dark.clone();
                                        move |_, _, _| {
                                            let client = client.clone();
                                            ThemeClient::spawn_task(async move {
                                                let _ = client.set_mode(ThemeMode::Dark).await;
                                            });
                                        }
                                    }),
                            ),
                    ),
            );

        // ── Scheme Variant ButtonGroup ──
        let scheme_group = ButtonGroup::new("scheme-variants")
            .mode(ButtonGroupMode::Connected)
            .filled_tonal()
            .small()
            .flex_wrap()
            .children(SCHEME_VARIANTS.iter().enumerate().map(|(ix, label)| {
                let variant = SchemeVariant::from_str(label);
                let selected = current_variant == variant;
                let client = theme_client.clone();
                Button::new(SharedString::from(format!("scheme-{ix}")))
                    .label(*label)
                    .selected(selected)
                    .small()
                    .on_click(move |_, _, _| {
                        let client = client.clone();
                        ThemeClient::spawn_task(async move {
                            let _ = client.set_scheme_variant(variant).await;
                        });
                    })
            }));

        // ── Assemble the full section ──
        v_flex()
            .w_full()
            .gap_3()
            .child(title_row)
            .child(main_content)
            .child(scheme_group)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quick_page_render_construction() {
        let client = futures_lite::future::block_on(ThemeClient::new());
        assert!(client.current_state().revision >= 1);
    }
}
