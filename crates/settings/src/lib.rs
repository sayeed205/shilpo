use gpui::{
    App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
    Styled, Window, div,
};
use shilpo_ui::{
    ActiveTheme, IconName, NavigationRail, NavigationRailHeader, NavigationRailItem,
    NavigationRailMenuButton, Selectable, StyledExt, h_flex, v_flex,
};

/// Settings App Navigation Category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsCategory {
    #[default]
    Quick,
    Network,
    Bluetooth,
    Bar,
    Desktop,
    Interface,
    Storage,
}

impl SettingsCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Quick => "Quick",
            Self::Network => "Network",
            Self::Bluetooth => "Bluetooth",
            Self::Bar => "Bar",
            Self::Desktop => "Desktop",
            Self::Interface => "Interface",
            Self::Storage => "Storage",
        }
    }

    pub fn icon(&self, active: bool) -> IconName {
        match self {
            Self::Quick => IconName::InstantMix,
            Self::Network => IconName::AndroidWifi3Bar,
            Self::Bluetooth => IconName::Bluetooth,
            Self::Bar => {
                if active {
                    IconName::ToolbarFill
                } else {
                    IconName::Toolbar
                }
            }
            Self::Desktop => {
                if active {
                    IconName::ComputerFill
                } else {
                    IconName::Computer
                }
            }
            Self::Interface => {
                if active {
                    IconName::BottomAppBarFill
                } else {
                    IconName::BottomAppBar
                }
            }
            Self::Storage => IconName::Storage,
        }
    }

    pub const ALL: &'static [Self] = &[
        Self::Quick,
        Self::Network,
        Self::Bluetooth,
        Self::Bar,
        Self::Desktop,
        Self::Interface,
        Self::Storage,
    ];
}

#[derive(Debug, Clone)]
pub struct SettingsPageDescriptor {
    pub category: SettingsCategory,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct SettingsPageRegistry {
    pages: Vec<SettingsPageDescriptor>,
}

impl SettingsPageRegistry {
    pub fn discover() -> Self {
        let pages = SettingsCategory::ALL
            .iter()
            .map(|category| SettingsPageDescriptor {
                category: *category,
                label: category.label().to_owned(),
            })
            .collect::<Vec<_>>();
        Self { pages }
    }

    pub fn pages(&self) -> &[SettingsPageDescriptor] {
        &self.pages
    }
}

/// Standalone Settings Application View.
pub struct SettingsView {
    pub active_category: SettingsCategory,
    pub page_registry: SettingsPageRegistry,
    pub rail_collapsed: bool,
}

impl SettingsView {
    pub fn new() -> Self {
        let page_registry = SettingsPageRegistry::discover();
        Self {
            active_category: SettingsCategory::default(),
            page_registry,
            rail_collapsed: false,
        }
    }

    pub fn view(window: &mut Window, cx: &mut App) -> Entity<shilpo_ui::Root> {
        #[cfg(target_os = "linux")]
        {
            register_desktop_entry();
            update_desktop_icon_for_theme(cx);
        }

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
        let active_label = active.label();

        h_flex()
            .size_full()
            .bg(cx.theme().surface)
            .text_color(cx.theme().on_surface)
            // Left Navigation Sidebar (M3 Expressive Navigation Rail)
            .child({
                let menu_button = NavigationRailMenuButton::new("rail-toggle")
                    .collapsed(self.rail_collapsed)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.rail_collapsed = !this.rail_collapsed;
                        cx.notify();
                    }));

                let rail_header =
                    NavigationRailHeader::new("settings-rail-header").child(menu_button);

                let rail_items: Vec<_> = self
                    .page_registry
                    .pages()
                    .iter()
                    .cloned()
                    .enumerate()
                    .map(|(index, page)| {
                        let is_active = active == page.category;
                        let icon = page.category.icon(is_active);
                        let category = page.category;
                        NavigationRailItem::new(("settings-page", index))
                            .icon(icon)
                            .label(page.label)
                            .selected(is_active)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.active_category = category;
                                cx.notify();
                            }))
                    })
                    .collect();

                NavigationRail::new("settings-nav-rail")
                    .collapsed(self.rail_collapsed)
                    .header(rail_header)
                    .items(rail_items)
            })
            // Main Content Area (Pocket Card UI matching Storybook gallery)
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .py_3()
                    .pr_3()
                    .pl_1()
                    .child(
                        v_flex()
                            .id(gpui::ElementId::Name(gpui::SharedString::from(format!(
                                "settings-page-content-{:?}",
                                active
                            ))))
                            .size_full()
                            .bg(cx.theme().surface_container_low)
                            .rounded_2xl()
                            .p_6()
                            .gap_4()
                            .child(
                                h_flex()
                                    .gap_3()
                                    .items_center()
                                    .child(
                                        shilpo_ui::Icon::new(active.icon(true))
                                            .size(gpui::px(28.))
                                            .text_color(cx.theme().primary),
                                    )
                                    .child(
                                        div()
                                            .text_xl()
                                            .font_bold()
                                            .text_color(cx.theme().on_surface)
                                            .child(active_label),
                                    ),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().on_surface_variant)
                                    .child(format!(
                                        "Configure {} settings and system preferences.",
                                        active_label
                                    )),
                            )
                            .child(
                                v_flex()
                                    .flex_1()
                                    .items_center()
                                    .justify_center()
                                    .gap_3()
                                    .p_8()
                                    .rounded_xl()
                                    .border_1()
                                    .border_color(cx.theme().outline_variant)
                                    .bg(cx.theme().surface_container)
                                    .child(
                                        shilpo_ui::Icon::new(active.icon(false))
                                            .size(gpui::px(48.))
                                            .text_color(cx.theme().on_surface_variant),
                                    )
                                    .child(
                                        div()
                                            .text_base()
                                            .font_semibold()
                                            .text_color(cx.theme().on_surface)
                                            .child(format!("{} Settings", active_label)),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().on_surface_variant)
                                            .child(
                                                "Settings page options will be added here in future updates.",
                                            ),
                                    ),
                            ),
                    ),
            )
    }
}

#[cfg(target_os = "linux")]
pub fn register_desktop_entry() {
    if let Some(home) = dirs::home_dir() {
        let apps_dir = home.join(".local/share/applications");
        let icons_scalable_dir = home.join(".local/share/icons/hicolor/scalable/apps");

        let _ = std::fs::create_dir_all(&apps_dir);
        let _ = std::fs::create_dir_all(&icons_scalable_dir);

        let desktop_file = apps_dir.join("org.shilpo.settings.desktop");
        let icon_svg_file = icons_scalable_dir.join("org.shilpo.settings.svg");

        let desktop_content = include_str!("../resources/org.shilpo.settings.desktop");
        let icon_svg_content = include_bytes!("../resources/org.shilpo.settings.svg");

        let _ = std::fs::write(&desktop_file, desktop_content);
        let _ = std::fs::write(&icon_svg_file, icon_svg_content);
    }
}

#[cfg(target_os = "linux")]
pub fn update_desktop_icon_for_theme(cx: &App) {
    if let Some(home) = dirs::home_dir() {
        let icons_scalable_dir = home.join(".local/share/icons/hicolor/scalable/apps");
        let pixmaps_dir = home.join(".local/share/pixmaps");
        let _ = std::fs::create_dir_all(&icons_scalable_dir);
        let _ = std::fs::create_dir_all(&pixmaps_dir);

        let icon_svg_file = icons_scalable_dir.join("org.shilpo.settings.svg");

        let is_dark = cx.theme().mode.is_dark();
        let bg_hsla = if is_dark {
            cx.theme().surface_container_high
        } else {
            cx.theme().primary_container
        };
        let bg_rgb = bg_hsla.to_rgb();
        let bg_color = format!(
            "#{:02x}{:02x}{:02x}",
            (bg_rgb.r * 255.0) as u8,
            (bg_rgb.g * 255.0) as u8,
            (bg_rgb.b * 255.0) as u8
        );

        let primary_rgb = cx.theme().primary.to_rgb();
        let glyph_color = format!(
            "#{:02x}{:02x}{:02x}",
            (primary_rgb.r * 255.0) as u8,
            (primary_rgb.g * 255.0) as u8,
            (primary_rgb.b * 255.0) as u8
        );

        let svg_content = format!(
            r#"<svg width="512" height="512" viewBox="0 0 512 512" fill="none" xmlns="http://www.w3.org/2000/svg">
    <rect width="512" height="512" rx="160" fill="{bg_color}"/>
    <g transform="translate(64, 448) scale(0.4)">
        <path fill="{glyph_color}" d="M433-80q-27 0-46.5-18T363-142l-9-66q-13-5-24.5-12T307-235l-62 26q-25 11-50 2t-39-32l-47-82q-14-23-8-49t27-43l53-40q-1-7-1-13.5v-27q0-6.5 1-13.5l-53-40q-21-17-27-43t8-49l47-82q14-23 39-32t50 2l62 26q11-8 23-15t24-12l9-66q4-26 23.5-44t46.5-18h94q27 0 46.5 18t23.5 44l9 66q13 5 24.5 12t22.5 15l62-26q25-11 50-2t39 32l47 82q14-23 8 49t-27 43l-53 40q1 7 1 13.5v27q0 6.5-2 13.5l53 40q21 17 27 43t-8 49l-48 82q-14 23-39 32t-50-2l-60-26q-11 8-23 15t-24 12l-9 66q-4 26-23.5 44T527-80h-94Zm49-260q58 0 99-41t41-99q0-58-41-99t-99-41q-59 0-99.5 41T342-480q0 58 40.5 99t99.5 41Z"/>
    </g>
</svg>"#
        );

        let _ = std::fs::write(&icon_svg_file, &svg_content);

        cx.background_executor()
            .spawn(async move {
                for size in [512, 256, 128, 64, 48, 32] {
                    let size_dir =
                        home.join(format!(".local/share/icons/hicolor/{size}x{size}/apps"));
                    let _ = std::fs::create_dir_all(&size_dir);
                    let png_file = size_dir.join("org.shilpo.settings.png");
                    let _ = std::process::Command::new("rsvg-convert")
                        .args([
                            "-w",
                            &size.to_string(),
                            "-h",
                            &size.to_string(),
                            icon_svg_file.to_str().unwrap(),
                            "-o",
                            png_file.to_str().unwrap(),
                        ])
                        .status();
                }

                let pixmap_png = pixmaps_dir.join("org.shilpo.settings.png");
                let _ = std::process::Command::new("rsvg-convert")
                    .args([
                        "-w",
                        "512",
                        "-h",
                        "512",
                        icon_svg_file.to_str().unwrap(),
                        "-o",
                        pixmap_png.to_str().unwrap(),
                    ])
                    .status();

                let _ = std::process::Command::new("gtk-update-icon-cache")
                    .args([
                        "-f",
                        "-t",
                        home.join(".local/share/icons/hicolor").to_str().unwrap(),
                    ])
                    .status();
            })
            .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_categories() {
        assert_eq!(SettingsCategory::ALL.len(), 7);
        assert_eq!(SettingsCategory::Quick.label(), "Quick");
        assert_eq!(SettingsCategory::Network.label(), "Network");
        assert_eq!(SettingsCategory::Bluetooth.label(), "Bluetooth");
        assert_eq!(SettingsCategory::Bar.label(), "Bar");
        assert_eq!(SettingsCategory::Desktop.label(), "Desktop");
        assert_eq!(SettingsCategory::Interface.label(), "Interface");
        assert_eq!(SettingsCategory::Storage.label(), "Storage");
    }
}
