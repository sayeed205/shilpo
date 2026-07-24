use gpui::{
    App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, Window, div, px,
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
}

impl SettingsView {
    pub fn new() -> Self {
        Self {
            active_category: SettingsCategory::default(),
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
            .w_full()
            .h_full()
            .bg(cx.theme().surface)
            .text_color(cx.theme().on_surface)
            // Left Navigation Sidebar
            .child(
                v_flex()
                    .w(px(220.))
                    .h_full()
                    .p_4()
                    .gap_2()
                    .bg(cx.theme().surface_container_low)
                    .border_r_1()
                    .border_color(cx.theme().outline_variant.opacity(0.3))
                    .child(
                        div()
                            .pb_3()
                            .text_sm()
                            .font_bold()
                            .text_color(cx.theme().primary)
                            .child("Shilpo Settings"),
                    )
                    .children(SettingsCategory::ALL.iter().map(|&cat| {
                        let is_active = cat == active;
                        let bg = if is_active {
                            cx.theme().primary_container
                        } else {
                            cx.theme().transparent
                        };
                        let fg = if is_active {
                            cx.theme().on_primary_container
                        } else {
                            cx.theme().on_surface
                        };

                        h_flex()
                            .id(("settings-cat", cat as usize))
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
                    ),
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
