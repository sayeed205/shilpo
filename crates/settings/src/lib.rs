use gpui::{
    App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render, Role,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};
use shilpo_ui::{ActiveTheme, Icon, IconName, StyledExt, h_flex, v_flex};

/// Settings App Navigation Category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsCategory {
    #[default]
    System,
    Display,
    Sound,
    Network,
    Bluetooth,
    Appearance,
    Shortcuts,
    About,
}

impl SettingsCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Display => "Display",
            Self::Sound => "Sound",
            Self::Network => "Network",
            Self::Bluetooth => "Bluetooth",
            Self::Appearance => "Appearance",
            Self::Shortcuts => "Shortcuts",
            Self::About => "About",
        }
    }

    pub fn icon(&self) -> IconName {
        match self {
            Self::System => IconName::Star,
            Self::Display => IconName::Sun,
            Self::Sound => IconName::Bell,
            Self::Network => IconName::Network,
            Self::Bluetooth => IconName::SquareTerminal,
            Self::Appearance => IconName::Palette,
            Self::Shortcuts => IconName::Copy,
            Self::About => IconName::Check,
        }
    }

    pub const ALL: &'static [Self] = &[
        Self::System,
        Self::Display,
        Self::Sound,
        Self::Network,
        Self::Bluetooth,
        Self::Appearance,
        Self::Shortcuts,
        Self::About,
    ];
}

/// Standalone Settings Application View.
pub struct SettingsView {
    pub active_category: SettingsCategory,
    pub active_scale: f32,
    pub selected_font: String,
    pub active_theme_mode: String,
    pub active_corner_radius_scale: f32,
    pub high_contrast: bool,
    pub reduced_motion: bool,
    pub clock_format: String,
    pub temperature_unit: String,
    pub active_locale: String,
}

impl SettingsView {
    pub fn new() -> Self {
        Self {
            active_category: SettingsCategory::default(),
            active_scale: 1.0,
            selected_font: "sans-serif".to_string(),
            active_theme_mode: "Dark".to_string(),
            active_corner_radius_scale: 1.0,
            high_contrast: false,
            reduced_motion: false,
            clock_format: "%H:%M".to_string(),
            temperature_unit: "Celsius".to_string(),
            active_locale: "en-US".to_string(),
        }
    }

    pub fn view(window: &mut Window, cx: &mut App) -> Entity<shilpo_ui::Root> {
        let view = cx.new(|_| Self::new());
        cx.new(|cx| {
            shilpo_ui::Root::new(view, window, cx)
                .bordered(true)
                .bg(cx.theme().surface)
        })
    }
}

impl Default for SettingsView {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.active_category;

        h_flex()
            .size_full()
            .bg(cx.theme().surface)
            .text_color(cx.theme().on_surface)
            // Left Navigation Sidebar
            .child(
                v_flex()
                    .w_64()
                    .h_full()
                    .p_4()
                    .gap_2()
                    .border_r_1()
                    .border_color(cx.theme().outline_variant)
                    .bg(cx.theme().surface_container)
                    .children(SettingsCategory::ALL.iter().map(|&cat| {
                        let is_active = active == cat;
                        let (bg, fg) = if is_active {
                            (cx.theme().primary_container, cx.theme().on_primary_container)
                        } else {
                            (cx.theme().surface_container, cx.theme().on_surface_variant)
                        };

                        h_flex()
                            .id(("settings-cat", cat as usize))
                            .role(Role::Button)
                            .px_3()
                            .py_2()
                            .rounded_xl()
                            .bg(bg)
                            .text_color(fg)
                            .gap_3()
                            .items_center()
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.active_category = cat;
                                cx.notify();
                            }))
                            .child(Icon::new(cat.icon()).size(px(16.)))
                            .child(div().text_xs().font_medium().child(cat.label()))
                    })),
            )
            // Main Content Area
            .child(
                v_flex()
                    .flex_1()
                    .h_full()
                    .p_6()
                    .gap_4()
                    .child(
                        div()
                            .text_lg()
                            .font_bold()
                            .text_color(cx.theme().on_surface)
                            .child(active.label()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().on_surface_variant)
                            .child(format!(
                                "Dedicated OS Control Panel for {}. Configure system parameters, appearance, and preferences.",
                                active.label()
                            )),
                    )
                    .when(active == SettingsCategory::Display, |this| {
                        let active_scale = self.active_scale;
                        this.child(
                            v_flex()
                                .gap_2()
                                .child(div().text_xs().font_bold().child("Display Scaling"))
                                .child(h_flex().gap_2().children(
                                    [1.0f32, 1.25, 1.5, 2.0].into_iter().enumerate().map(|(i, scale)| {
                                        let is_active = (active_scale - scale).abs() < 0.01;
                                        let (bg, fg) = if is_active {
                                            (cx.theme().primary, cx.theme().on_primary)
                                        } else {
                                            (cx.theme().surface_container, cx.theme().on_surface)
                                        };
                                        div()
                                            .id(("display-scale-pill", i))
                                            .role(Role::Button)
                                            .cursor_pointer()
                                            .px_3()
                                            .py_1p5()
                                            .rounded_xl()
                                            .bg(bg)
                                            .text_color(fg)
                                            .text_xs()
                                            .font_bold()
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.active_scale = scale;
                                                cx.notify();
                                            }))
                                            .child(format!("{}%", (scale * 100.0) as u32))
                                    }),
                                )),
                        )
                    })
                    .when(active == SettingsCategory::Appearance, |this| {
                        let fonts = shilpo_ui::FontFamilyCache::global(cx).list_font_families(cx);
                        let sample_fonts = if fonts.is_empty() {
                            vec!["sans-serif".into(), "Inter".into(), "Roboto".into(), "Fira Code".into()]
                        } else {
                            fonts.into_iter().take(5).collect()
                        };
                        let selected_font = self.selected_font.clone();
                        let active_theme_mode = self.active_theme_mode.clone();
                        let active_radius = self.active_corner_radius_scale;

                        this.child(
                            v_flex()
                                .gap_4()
                                // Theme Mode / Auto Schedule
                                .child(
                                    v_flex()
                                        .gap_2()
                                        .child(div().text_xs().font_bold().child("System Theme & Auto-Schedule Policy"))
                                        .child(h_flex().gap_2().children(
                                            ["Dark", "Light", "Auto (Sunset-to-Sunrise)"].into_iter().enumerate().map(|(i, mode)| {
                                                let is_active = active_theme_mode == mode;
                                                let (bg, fg) = if is_active {
                                                    (cx.theme().primary, cx.theme().on_primary)
                                                } else {
                                                    (cx.theme().surface_container, cx.theme().on_surface)
                                                };
                                                let mode_str = mode.to_string();
                                                div()
                                                    .id(("theme-mode-pill", i))
                                                    .role(Role::Button)
                                                    .cursor_pointer()
                                                    .px_3()
                                                    .py_1p5()
                                                    .rounded_xl()
                                                    .bg(bg)
                                                    .text_color(fg)
                                                    .text_xs()
                                                    .font_semibold()
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.active_theme_mode = mode_str.clone();
                                                        cx.notify();
                                                    }))
                                                    .child(mode)
                                            }),
                                        )),
                                )
                                // Corner Radius Scaling
                                .child(
                                    v_flex()
                                        .gap_2()
                                        .child(div().text_xs().font_bold().child("Corner Radius Scale Factor"))
                                        .child(h_flex().gap_2().children(
                                            [(0.5f32, "Compact (0.5x)"), (1.0, "Standard (1.0x)"), (1.5, "Extra Rounded (1.5x)")].into_iter().enumerate().map(|(i, (scale, label))| {
                                                let is_active = (active_radius - scale).abs() < 0.01;
                                                let (bg, fg) = if is_active {
                                                    (cx.theme().primary, cx.theme().on_primary)
                                                } else {
                                                    (cx.theme().surface_container, cx.theme().on_surface)
                                                };
                                                div()
                                                    .id(("corner-radius-pill", i))
                                                    .role(Role::Button)
                                                    .cursor_pointer()
                                                    .px_3()
                                                    .py_1p5()
                                                    .rounded_xl()
                                                    .bg(bg)
                                                    .text_color(fg)
                                                    .text_xs()
                                                    .font_semibold()
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.active_corner_radius_scale = scale;
                                                        cx.notify();
                                                    }))
                                                    .child(label)
                                            }),
                                        )),
                                )
                                // Fonts
                                .child(
                                    v_flex()
                                        .gap_2()
                                        .child(div().text_xs().font_bold().child("UI & Typography Fonts (Heading, Body, Monospace)"))
                                        .child(h_flex().gap_2().flex_wrap().children(
                                            sample_fonts.into_iter().enumerate().map(|(i, font)| {
                                                let font_str = font.to_string();
                                                let is_active = selected_font == font_str;
                                                let (bg, fg) = if is_active {
                                                    (cx.theme().primary, cx.theme().on_primary)
                                                } else {
                                                    (cx.theme().surface_container, cx.theme().on_surface)
                                                };
                                                let font_clone = font_str.clone();
                                                div()
                                                    .id(("appearance-font-pill", i))
                                                    .role(Role::Button)
                                                    .cursor_pointer()
                                                    .px_3()
                                                    .py_1p5()
                                                    .rounded_xl()
                                                    .bg(bg)
                                                    .text_color(fg)
                                                    .text_xs()
                                                    .font_semibold()
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.selected_font = font_clone.clone();
                                                        cx.notify();
                                                    }))
                                                    .child(font_str)
                                            }),
                                        )),
                                )
                                // Accessibility & Motion Overrides
                                .child(
                                    v_flex()
                                        .gap_2()
                                        .child(div().text_xs().font_bold().child("Accessibility & Motion Preferences"))
                                        .child(h_flex().gap_2().children([
                                            {
                                                let is_active = self.high_contrast;
                                                let (bg, fg) = if is_active {
                                                    (cx.theme().primary, cx.theme().on_primary)
                                                } else {
                                                    (cx.theme().surface_container, cx.theme().on_surface)
                                                };
                                                div()
                                                    .id("high-contrast-pill")
                                                    .role(Role::Button)
                                                    .cursor_pointer()
                                                    .px_3()
                                                    .py_1p5()
                                                    .rounded_xl()
                                                    .bg(bg)
                                                    .text_color(fg)
                                                    .text_xs()
                                                    .font_semibold()
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.high_contrast = !this.high_contrast;
                                                        cx.notify();
                                                    }))
                                                    .child("High Contrast")
                                            },
                                            {
                                                let is_active = self.reduced_motion;
                                                let (bg, fg) = if is_active {
                                                    (cx.theme().primary, cx.theme().on_primary)
                                                } else {
                                                    (cx.theme().surface_container, cx.theme().on_surface)
                                                };
                                                div()
                                                    .id("reduced-motion-pill")
                                                    .role(Role::Button)
                                                    .cursor_pointer()
                                                    .px_3()
                                                    .py_1p5()
                                                    .rounded_xl()
                                                    .bg(bg)
                                                    .text_color(fg)
                                                    .text_xs()
                                                    .font_semibold()
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.reduced_motion = !this.reduced_motion;
                                                        cx.notify();
                                                    }))
                                                    .child("Reduced Motion")
                                            },
                                        ])),
                                )
                                // Clock & Units Preferences
                                .child(
                                    v_flex()
                                        .gap_2()
                                        .child(div().text_xs().font_bold().child("Clock Format & Temperature Units"))
                                        .child(h_flex().gap_2().children([
                                            {
                                                let fmt_24 = self.clock_format == "%H:%M";
                                                let (bg, fg) = if fmt_24 {
                                                    (cx.theme().primary, cx.theme().on_primary)
                                                } else {
                                                    (cx.theme().surface_container, cx.theme().on_surface)
                                                };
                                                div()
                                                    .id("clock-24h-pill")
                                                    .role(Role::Button)
                                                    .cursor_pointer()
                                                    .px_3()
                                                    .py_1p5()
                                                    .rounded_xl()
                                                    .bg(bg)
                                                    .text_color(fg)
                                                    .text_xs()
                                                    .font_semibold()
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.clock_format = "%H:%M".to_string();
                                                        cx.notify();
                                                    }))
                                                    .child("24-Hour (14:30)")
                                            },
                                            {
                                                let fmt_12 = self.clock_format == "%I:%M %p";
                                                let (bg, fg) = if fmt_12 {
                                                    (cx.theme().primary, cx.theme().on_primary)
                                                } else {
                                                    (cx.theme().surface_container, cx.theme().on_surface)
                                                };
                                                div()
                                                    .id("clock-12h-pill")
                                                    .role(Role::Button)
                                                    .cursor_pointer()
                                                    .px_3()
                                                    .py_1p5()
                                                    .rounded_xl()
                                                    .bg(bg)
                                                    .text_color(fg)
                                                    .text_xs()
                                                    .font_semibold()
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.clock_format = "%I:%M %p".to_string();
                                                        cx.notify();
                                                    }))
                                                    .child("12-Hour (02:30 PM)")
                                            },
                                        ])),
                                )
                                // System Locale Selection
                                .child(
                                    v_flex()
                                        .gap_2()
                                        .child(div().text_xs().font_bold().child("System Locale & Translation Dictionary"))
                                        .child(h_flex().gap_2().children(
                                            [("en-US", "English (US)"), ("bn-IN", "Bangla (bn-IN)"), ("ar-SA", "Arabic (RTL)")].into_iter().enumerate().map(|(i, (loc_code, loc_label))| {
                                                let is_active = self.active_locale == loc_code;
                                                let (bg, fg) = if is_active {
                                                    (cx.theme().primary, cx.theme().on_primary)
                                                } else {
                                                    (cx.theme().surface_container, cx.theme().on_surface)
                                                };
                                                let loc_str = loc_code.to_string();
                                                div()
                                                    .id(("locale-pill", i))
                                                    .role(Role::Button)
                                                    .cursor_pointer()
                                                    .px_3()
                                                    .py_1p5()
                                                    .rounded_xl()
                                                    .bg(bg)
                                                    .text_color(fg)
                                                    .text_xs()
                                                    .font_semibold()
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.active_locale = loc_str.clone();
                                                        cx.notify();
                                                    }))
                                                    .child(loc_label)
                                            }),
                                        )),
                                ),
                        )
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_categories() {
        assert_eq!(SettingsCategory::ALL.len(), 8);
        assert_eq!(SettingsCategory::System.label(), "System");
    }
}
