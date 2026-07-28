use gpui::{
    App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render, Role,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder,
};
use shilpo_ui::{
    ActiveTheme, IconName, NavigationRail, NavigationRailHeader, NavigationRailItem,
    NavigationRailMenuButton, Selectable, StyledExt, h_flex, v_flex,
};
use std::fs;

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
    Extensions,
    About,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExtensionsSection {
    #[default]
    Discover,
    Installed,
    Updates,
    Sources,
}

impl ExtensionsSection {
    const ALL: [Self; 4] = [
        Self::Discover,
        Self::Installed,
        Self::Updates,
        Self::Sources,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Discover => "Discover",
            Self::Installed => "Installed",
            Self::Updates => "Updates",
            Self::Sources => "Sources",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsPageId {
    Builtin(SettingsCategory),
    Extension(shilpo_ext::CanonicalId),
}

#[derive(Debug, Clone)]
pub struct SettingsPageDescriptor {
    pub id: SettingsPageId,
    pub label: String,
    pub schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct SettingsPageRegistry {
    pages: Vec<SettingsPageDescriptor>,
}

impl SettingsPageRegistry {
    pub fn discover() -> Self {
        let mut pages = SettingsCategory::ALL
            .iter()
            .map(|category| SettingsPageDescriptor {
                id: SettingsPageId::Builtin(*category),
                label: category.label().to_owned(),
                schema: None,
            })
            .collect::<Vec<_>>();
        pages.extend(discover_extension_settings());
        Self { pages }
    }

    pub fn pages(&self) -> &[SettingsPageDescriptor] {
        &self.pages
    }
}

fn discover_extension_settings() -> Vec<SettingsPageDescriptor> {
    let state = shilpo_ext::default_extension_state_dir();
    let (registrations, _) = shilpo_ext::development_registrations(&state);
    let mut pages = Vec::new();
    for registration in registrations {
        let Ok(manifest_source) = fs::read_to_string(registration.path.join("extension.toml"))
        else {
            continue;
        };
        let Ok(manifest) = shilpo_ext::ExtensionManifest::from_toml(&manifest_source) else {
            continue;
        };
        pages.extend(
            manifest
                .contributions
                .settings_pages
                .into_iter()
                .map(|page| {
                    let schema = fs::read_to_string(registration.path.join(&page.schema))
                        .ok()
                        .and_then(|source| serde_json::from_str(&source).ok());
                    SettingsPageDescriptor {
                        id: SettingsPageId::Extension(shilpo_ext::CanonicalId::new(
                            manifest.id.clone(),
                            page.id,
                        )),
                        label: page.name,
                        schema,
                    }
                }),
        );
    }
    pages.sort_by(|left, right| left.label.cmp(&right.label));
    pages
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
            Self::Extensions => "Extensions",
            Self::About => "About",
        }
    }

    pub fn icon(&self) -> IconName {
        match self {
            Self::System => IconName::Star,
            Self::Display => IconName::Sunny,
            Self::Sound => IconName::Notifications,
            Self::Network => IconName::Lan,
            Self::Bluetooth => IconName::Terminal,
            Self::Appearance => IconName::Palette,
            Self::Shortcuts => IconName::ContentCopy,
            Self::Extensions => IconName::Terminal,
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
        Self::Extensions,
        Self::About,
    ];
}

/// Standalone Settings Application View.
pub struct SettingsView {
    pub active_page: SettingsPageId,
    pub page_registry: SettingsPageRegistry,
    pub active_scale: f32,
    pub selected_font: String,
    pub active_theme_mode: String,
    pub active_corner_radius_scale: f32,
    pub high_contrast: bool,
    pub reduced_motion: bool,
    pub clock_format: String,
    pub temperature_unit: String,
    pub active_locale: String,
    pub custom_wallpaper_dir: String,
    pub extensions_section: ExtensionsSection,
    pub extension_snapshot: shilpo_ext::ExtensionCatalogSnapshot,
    pub extension_action_error: Option<String>,
    pub rail_collapsed: bool,
    extension_catalog: shilpo_ext::ExtensionCatalog,
}

impl SettingsView {
    pub fn new() -> Self {
        let page_registry = SettingsPageRegistry::discover();
        let extension_catalog = shilpo_ext::ExtensionCatalog::open_default();
        let extension_snapshot = extension_snapshot(&extension_catalog);
        let custom_wallpaper_dir = shilpo_services::WallpaperService::default_wallpaper_dir()
            .display()
            .to_string();
        Self {
            active_page: SettingsPageId::Builtin(SettingsCategory::default()),
            page_registry,
            active_scale: 1.0,
            selected_font: "sans-serif".to_string(),
            active_theme_mode: "Dark".to_string(),
            active_corner_radius_scale: 1.0,
            high_contrast: false,
            reduced_motion: false,
            clock_format: "%H:%M".to_string(),
            temperature_unit: "Celsius".to_string(),
            active_locale: "en-US".to_string(),
            custom_wallpaper_dir,
            extensions_section: ExtensionsSection::default(),
            extension_snapshot,
            extension_action_error: None,
            rail_collapsed: false,
            extension_catalog,
        }
    }

    fn refresh_extensions(&mut self) {
        self.extension_snapshot = extension_snapshot(&self.extension_catalog);
    }

    pub fn view(window: &mut Window, cx: &mut App) -> Entity<shilpo_ui::Root> {
        shilpo_ui::Theme::sync_system_appearance(Some(window), cx);

        #[cfg(target_os = "linux")]
        {
            register_desktop_entry();
            update_desktop_icon_for_theme(cx);
        }

        let view = cx.new(|cx| {
            cx.observe_window_appearance(window, |_, window, cx| {
                shilpo_ui::Theme::sync_system_appearance(Some(window), cx);
                #[cfg(target_os = "linux")]
                update_desktop_icon_for_theme(cx);
                window.refresh();
            })
            .detach();
            Self::new()
        });
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

fn extension_snapshot(
    catalog: &shilpo_ext::ExtensionCatalog,
) -> shilpo_ext::ExtensionCatalogSnapshot {
    let state = shilpo_ext::default_extension_state_dir();
    let ids = shilpo_ext::development_registrations(&state)
        .0
        .into_iter()
        .map(|registration| registration.id);
    catalog.snapshot_with_development(ids)
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.active_page.clone();
        let active_builtin = match active {
            SettingsPageId::Builtin(category) => Some(category),
            SettingsPageId::Extension(_) => None,
        };
        let active_descriptor = self
            .page_registry
            .pages()
            .iter()
            .find(|page| page.id == active)
            .cloned();
        let active_label = active_descriptor
            .as_ref()
            .map_or_else(|| "Settings".to_owned(), |page| page.label.clone());
        let extension_rows = match self.extensions_section {
            ExtensionsSection::Discover => self
                .extension_snapshot
                .discover
                .iter()
                .map(|entry| {
                    format!(
                        "{}\n{} {} · {} · {} requested capabilities\n{}",
                        entry.release.name,
                        entry.release.id,
                        entry.release.version,
                        entry.trust,
                        entry.release.capabilities.len(),
                        entry
                            .release
                            .description
                            .as_deref()
                            .unwrap_or("No description")
                    )
                })
                .collect::<Vec<_>>(),
            ExtensionsSection::Installed => self
                .extension_snapshot
                .installed
                .iter()
                .map(|entry| {
                    format!(
                        "{}\n{} · {} · {} · {} granted capabilities",
                        entry.manifest.name,
                        entry.receipt.active.version,
                        entry.receipt.active.trust,
                        if entry.grants.enabled {
                            "Enabled"
                        } else {
                            "Disabled"
                        },
                        entry.grants.granted_capabilities.len()
                    )
                })
                .collect(),
            ExtensionsSection::Updates => self
                .extension_snapshot
                .updates
                .iter()
                .map(|entry| {
                    format!(
                        "{}\nInstalled {} · {:?}{}",
                        entry.id,
                        entry.installed_version,
                        entry.state,
                        entry
                            .available
                            .as_ref()
                            .map_or_else(String::new, |available| {
                                format!(" · Available {}", available.release.version)
                            })
                    )
                })
                .collect(),
            ExtensionsSection::Sources => self
                .extension_snapshot
                .sources
                .iter()
                .map(|source| {
                    format!(
                        "{}\n{} · {} · {}",
                        source.name,
                        source.id,
                        if source.official {
                            "Official"
                        } else {
                            "Third-party"
                        },
                        if source.enabled {
                            "Enabled"
                        } else {
                            "Disabled"
                        }
                    )
                })
                .collect(),
        };
        let mut permission_reviews = self
            .extension_snapshot
            .updates
            .iter()
            .filter(|update| update.state == shilpo_ext::UpdateState::AwaitingPermissionReview)
            .map(|update| {
                let capabilities = self
                    .extension_catalog
                    .pending_capabilities(&update.id)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|capability| format!("{:?}", capability.kind()))
                    .collect::<Vec<_>>();
                (update.id.clone(), capabilities, true)
            })
            .collect::<Vec<_>>();
        let pending_review_ids = permission_reviews
            .iter()
            .map(|(id, _, _)| id.clone())
            .collect::<std::collections::HashSet<_>>();
        permission_reviews.extend(
            self.extension_snapshot
                .installed
                .iter()
                .filter(|entry| {
                    entry
                        .manifest
                        .capabilities
                        .iter()
                        .any(|capability| !entry.grants.granted_capabilities.contains(capability))
                        && !pending_review_ids.contains(&entry.receipt.id)
                })
                .map(|entry| {
                    (
                        entry.receipt.id.clone(),
                        entry
                            .manifest
                            .capabilities
                            .iter()
                            .filter(|capability| {
                                !entry.grants.granted_capabilities.contains(capability)
                            })
                            .map(|capability| format!("{:?}", capability.kind()))
                            .collect(),
                        false,
                    )
                }),
        );
        let discover_actions = self
            .extension_snapshot
            .discover
            .iter()
            .filter(|entry| !entry.publisher_conflict)
            .map(|entry| (entry.release.id.clone(), entry.release.name.clone()))
            .collect::<Vec<_>>();
        let installed_actions = self
            .extension_snapshot
            .installed
            .iter()
            .map(|entry| (entry.receipt.id.clone(), entry.grants.enabled))
            .collect::<Vec<_>>();
        let update_actions = self
            .extension_snapshot
            .updates
            .iter()
            .filter(|entry| entry.state == shilpo_ext::UpdateState::Available)
            .map(|entry| entry.id.clone())
            .collect::<Vec<_>>();
        let source_actions = self
            .extension_snapshot
            .sources
            .iter()
            .map(|source| (source.id.clone(), source.official))
            .collect::<Vec<_>>();

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
                        let is_active = active == page.id;
                        let icon = match &page.id {
                            SettingsPageId::Builtin(category) => category.icon(),
                            SettingsPageId::Extension(_) => IconName::Terminal,
                        };
                        let selected_page = page.id.clone();
                        NavigationRailItem::new(("settings-page", index))
                            .icon(icon)
                            .label(page.label)
                            .selected(is_active)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.active_page = selected_page.clone();
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
                            .overflow_y_scroll()
                            .bg(cx.theme().surface_container_low)
                            .rounded_2xl()
                            .p_6()
                            .gap_4()
                            .child(
                                div()
                                    .text_lg()
                                    .font_bold()
                                    .text_color(cx.theme().on_surface)
                                    .child(active_label.clone()),
                            )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().on_surface_variant)
                            .child(format!(
                                "Dedicated OS Control Panel for {}. Configure system parameters, appearance, and preferences.",
                                active_label
                            )),
                    )
                    .when_some(self.extension_action_error.clone(), |this, error| {
                        this.child(
                            div()
                                .p_3()
                                .rounded_xl()
                                .bg(cx.theme().error_container)
                                .text_color(cx.theme().on_error_container)
                                .text_xs()
                                .child(error),
                        )
                    })
                    .when(active_builtin == Some(SettingsCategory::Display), |this| {
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
                    .when(active_builtin == Some(SettingsCategory::Appearance), |this| {
                        let fonts = shilpo_ui::FontFamilyCache::global(cx).list_font_families(cx);
                        let sample_fonts = if fonts.is_empty() {
                            vec!["sans-serif".into(), "Inter".into(), "Roboto".into(), "Fira Code".into()]
                        } else {
                            fonts.into_iter().take(5).collect()
                        };
                        let selected_font = self.selected_font.clone();
                        let active_radius = self.active_corner_radius_scale;

                        this.child(
                            v_flex()
                                .gap_4()
                                // Theme Mode (Dark / Light / System matching Storybook)
                                .child(
                                    v_flex()
                                        .gap_2()
                                        .child(div().text_xs().font_bold().child("System Theme Mode"))
                                        .child(h_flex().gap_2().children(
                                            [
                                                ("Dark", shilpo_ui::ThemeMode::Dark),
                                                ("Light", shilpo_ui::ThemeMode::Light),
                                                ("System", shilpo_ui::ThemeMode::System),
                                            ]
                                                .into_iter()
                                                .enumerate()
                                                .map(|(i, (label, target_mode))| {
                                                    let current_selected = cx.theme().selected_mode();
                                                    let is_active = current_selected == target_mode;
                                                let (bg, fg) = if is_active {
                                                    (cx.theme().primary, cx.theme().on_primary)
                                                } else {
                                                    (cx.theme().surface_container, cx.theme().on_surface)
                                                };
                                                    let label_str = label.to_string();
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
                                                        this.active_theme_mode = label_str.clone();
                                                        shilpo_ui::Theme::change(target_mode, None, cx);
                                                        #[cfg(target_os = "linux")]
                                                        update_desktop_icon_for_theme(cx);
                                                        cx.notify();
                                                    }))
                                                    .child(label)
                                            }),
                                        )),
                                )
                                // Wallpaper Directory Preset Selection
                                .child(
                                    v_flex()
                                        .gap_2()
                                        .child(div().text_xs().font_bold().child("Wallpaper Directory Location"))
                                        .child({
                                            let current_dir = self.custom_wallpaper_dir.clone();
                                            let home_str = std::env::var("HOME").unwrap_or_default();
                                            let home = std::path::PathBuf::from(&home_str);
                                            let presets = [
                                                ("Pictures/Wallpapers", home.join("Pictures").join("Wallpapers")),
                                                (".config/shilpo/wallpapers", home.join(".config").join("shilpo").join("wallpapers")),
                                                ("Pictures", home.join("Pictures")),
                                            ];
                                            h_flex().gap_2().children(
                                                presets.into_iter().enumerate().map(|(i, (label, path))| {
                                                    let path_str = path.display().to_string();
                                                    let is_active = current_dir == path_str;
                                                    let (bg, fg) = if is_active {
                                                        (cx.theme().primary, cx.theme().on_primary)
                                                    } else {
                                                        (cx.theme().surface_container, cx.theme().on_surface)
                                                    };
                                                    div()
                                                        .id(("wallpaper-dir-pill", i))
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
                                                            this.custom_wallpaper_dir = path_str.clone();
                                                            let wp_service = shilpo_services::WallpaperService::default();
                                                            wp_service.set_wallpaper_dir(&path_str);
                                                            if let Some(source_argb) = wp_service
                                                                .active_wallpaper()
                                                                .or_else(|| wp_service.scan_wallpapers().into_iter().next())
                                                                .and_then(|active_wp| shilpo_services::PaletteExtractor::new().extract_source_argb_from_file(&active_wp).ok())
                                                            {
                                                                shilpo_ui::Theme::global_mut(cx).set_source_argb(source_argb);
                                                            }
                                                            cx.notify();
                                                        }))
                                                        .child(label)
                                                })
                                            )
                                        })
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
                    })
                    .when(active_builtin == Some(SettingsCategory::Extensions), |this| {
                        let selected = self.extensions_section;
                        this.child(
                            v_flex()
                                .gap_4()
                                .child(
                                    h_flex().gap_2().children(
                                        ExtensionsSection::ALL.into_iter().enumerate().map(
                                            |(index, section)| {
                                                let is_active = selected == section;
                                                div()
                                                    .id(("extensions-section", index))
                                                    .role(Role::Button)
                                                    .cursor_pointer()
                                                    .px_3()
                                                    .py_1p5()
                                                    .rounded_full()
                                                    .bg(if is_active {
                                                        cx.theme().primary
                                                    } else {
                                                        cx.theme().surface_container
                                                    })
                                                    .text_color(if is_active {
                                                        cx.theme().on_primary
                                                    } else {
                                                        cx.theme().on_surface
                                                    })
                                                    .text_xs()
                                                    .font_semibold()
                                                    .on_click(cx.listener(
                                                        move |this, _, _, cx| {
                                                            this.extensions_section = section;
                                                            this.refresh_extensions();
                                                            cx.notify();
                                                        },
                                                    ))
                                                    .child(section.label())
                                            },
                                        ),
                                    ),
                                )
                                .children(extension_rows.into_iter().enumerate().map(
                                    |(index, row)| {
                                        div()
                                            .id(("extension-row", index))
                                            .p_4()
                                            .rounded_2xl()
                                            .bg(cx.theme().surface_container)
                                            .text_xs()
                                            .child(row)
                                    },
                                ))
                                .when(selected == ExtensionsSection::Discover, |this| {
                                    this.children(discover_actions.into_iter().enumerate().map(
                                        |(index, (extension_id, name))| {
                                            div()
                                                .id(("install-extension", index))
                                                .role(Role::Button)
                                                .cursor_pointer()
                                                .px_3()
                                                .py_1p5()
                                                .rounded_full()
                                                .bg(cx.theme().primary)
                                                .text_color(cx.theme().on_primary)
                                                .text_xs()
                                                .child(format!("Install {name}"))
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    let catalog = this.extension_catalog.clone();
                                                    let extension_id = extension_id.clone();
                                                    cx.spawn(async move |this, cx| {
                                                        let result = cx
                                                            .background_executor()
                                                            .spawn(async move {
                                                                catalog.install_from_catalog(
                                                                    &extension_id,
                                                                )
                                                            })
                                                            .await;
                                                        this.update(cx, |this, cx| {
                                                            this.extension_action_error =
                                                                result.err().map(|error| {
                                                                    error.to_string()
                                                                });
                                                            this.refresh_extensions();
                                                            cx.notify();
                                                        })
                                                            .ok();
                                                    })
                                                        .detach();
                                                }))
                                        },
                                    ))
                                })
                                .when(selected == ExtensionsSection::Installed, |this| {
                                    this.children(installed_actions.into_iter().enumerate().map(
                                        |(index, (extension_id, enabled))| {
                                            let uninstall_id = extension_id.clone();
                                            h_flex()
                                                .gap_2()
                                                .child(
                                                    div()
                                                        .id(("toggle-extension", index))
                                                        .role(Role::Button)
                                                        .cursor_pointer()
                                                        .px_3()
                                                        .py_1p5()
                                                        .rounded_full()
                                                        .bg(cx.theme().primary_container)
                                                        .child(if enabled {
                                                            "Disable"
                                                        } else {
                                                            "Enable"
                                                        })
                                                        .on_click(cx.listener(
                                                            move |this, _, _, cx| {
                                                                this.extension_action_error = this
                                                                    .extension_catalog
                                                                    .set_enabled(
                                                                        &extension_id,
                                                                        !enabled,
                                                                    )
                                                                    .err()
                                                                    .map(|error| error.to_string());
                                                                this.refresh_extensions();
                                                                cx.notify();
                                                            },
                                                        )),
                                                )
                                                .child(
                                                    div()
                                                        .id(("uninstall-extension", index))
                                                        .role(Role::Button)
                                                        .cursor_pointer()
                                                        .px_3()
                                                        .py_1p5()
                                                        .rounded_full()
                                                        .bg(cx.theme().error_container)
                                                        .text_color(cx.theme().on_error_container)
                                                        .child("Uninstall")
                                                        .on_click(cx.listener(
                                                            move |this, _, _, cx| {
                                                                this.extension_action_error = this
                                                                    .extension_catalog
                                                                    .uninstall(&uninstall_id)
                                                                    .err()
                                                                    .map(|error| error.to_string());
                                                                this.refresh_extensions();
                                                                cx.notify();
                                                            },
                                                        )),
                                                )
                                        },
                                    ))
                                })
                                .when(selected == ExtensionsSection::Updates, |this| {
                                    this.children(update_actions.into_iter().enumerate().map(
                                        |(index, extension_id)| {
                                            div()
                                                .id(("update-extension", index))
                                                .role(Role::Button)
                                                .cursor_pointer()
                                                .px_3()
                                                .py_1p5()
                                                .rounded_full()
                                                .bg(cx.theme().primary)
                                                .text_color(cx.theme().on_primary)
                                                .child(format!("Update {extension_id}"))
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    let catalog = this.extension_catalog.clone();
                                                    let extension_id = extension_id.clone();
                                                    cx.spawn(async move |this, cx| {
                                                        let result = cx
                                                            .background_executor()
                                                            .spawn(async move {
                                                                catalog.install_from_catalog(
                                                                    &extension_id,
                                                                )
                                                            })
                                                            .await;
                                                        this.update(cx, |this, cx| {
                                                            this.extension_action_error =
                                                                result.err().map(|error| {
                                                                    error.to_string()
                                                                });
                                                            this.refresh_extensions();
                                                            cx.notify();
                                                        })
                                                            .ok();
                                                    })
                                                        .detach();
                                                }))
                                        },
                                    ))
                                })
                                .when(selected == ExtensionsSection::Sources, |this| {
                                    this.child(
                                        div()
                                            .id("refresh-extension-sources")
                                            .role(Role::Button)
                                            .cursor_pointer()
                                            .px_3()
                                            .py_1p5()
                                            .rounded_full()
                                            .bg(cx.theme().primary)
                                            .text_color(cx.theme().on_primary)
                                            .child("Check sources")
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                let catalog = this.extension_catalog.clone();
                                                cx.spawn(async move |this, cx| {
                                                    let result = cx
                                                        .background_executor()
                                                        .spawn(async move {
                                                            catalog.refresh_sources()
                                                        })
                                                        .await;
                                                    this.update(cx, |this, cx| {
                                                        this.extension_action_error =
                                                            result.err().map(|error| {
                                                                error.to_string()
                                                            });
                                                        this.refresh_extensions();
                                                        cx.notify();
                                                    })
                                                        .ok();
                                                })
                                                    .detach();
                                            })),
                                    )
                                        .children(source_actions.into_iter().enumerate().filter_map(
                                            |(index, (source_id, official))| {
                                                (!official).then(|| {
                                                    div()
                                                        .id(("remove-extension-source", index))
                                                        .role(Role::Button)
                                                        .cursor_pointer()
                                                        .px_3()
                                                        .py_1p5()
                                                        .rounded_full()
                                                        .bg(cx.theme().error_container)
                                                        .text_color(cx.theme().on_error_container)
                                                        .child(format!("Remove {source_id}"))
                                                        .on_click(cx.listener(
                                                            move |this, _, _, cx| {
                                                                this.extension_action_error = this
                                                                    .extension_catalog
                                                                    .remove_source(&source_id)
                                                                    .err()
                                                                    .map(|error| error.to_string());
                                                                this.refresh_extensions();
                                                                cx.notify();
                                                            },
                                                        ))
                                                })
                                            },
                                        ))
                                })
                                .when(
                                    permission_reviews.iter().any(|(_, _, pending)| {
                                        (*pending && selected == ExtensionsSection::Updates)
                                            || (!*pending
                                            && selected == ExtensionsSection::Installed)
                                    }),
                                    |this| {
                                        this.child(
                                            v_flex()
                                                .gap_2()
                                                .child(
                                                    div()
                                                        .font_bold()
                                                        .child("Permission review"),
                                                )
                                                .children(permission_reviews.into_iter().filter(
                                                    |(_, _, pending)| {
                                                        (*pending
                                                            && selected
                                                            == ExtensionsSection::Updates)
                                                            || (!*pending
                                                            && selected
                                                            == ExtensionsSection::Installed)
                                                    },
                                                ).enumerate().map(
                                                    |(index, (extension_id, capabilities, pending))| {
                                                        let approve_id = extension_id.clone();
                                                        let deny_id = extension_id.clone();
                                                        v_flex()
                                                            .gap_2()
                                                            .p_4()
                                                            .rounded_2xl()
                                                            .bg(cx.theme().secondary_container)
                                                            .child(format!(
                                                                "{extension_id} requests: {}",
                                                                if capabilities.is_empty() {
                                                                    "no additional grants".to_owned()
                                                                } else {
                                                                    capabilities.join(", ")
                                                                }
                                                            ))
                                                            .child(
                                                                h_flex()
                                                                    .gap_2()
                                                                    .child(
                                                                        div()
                                                                            .id(("approve-capabilities", index))
                                                                            .role(Role::Button)
                                                                            .cursor_pointer()
                                                                            .px_3()
                                                                            .py_1p5()
                                                                            .rounded_full()
                                                                            .bg(cx.theme().primary)
                                                                            .text_color(cx.theme().on_primary)
                                                                            .child("Grant requested")
                                                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                                                let capabilities = if pending {
                                                                                    this.extension_catalog.pending_capabilities(&approve_id)
                                                                                } else {
                                                                                    this.extension_catalog.requested_capabilities(&approve_id)
                                                                                };
                                                                                if let Ok(capabilities) = capabilities {
                                                                                    this.extension_action_error = if pending {
                                                                                        this.extension_catalog.approve_pending(&approve_id, capabilities).map(|_| ()).err().map(|error| error.to_string())
                                                                                    } else {
                                                                                        this.extension_catalog.approve_capabilities(&approve_id, capabilities).map(|_| ()).err().map(|error| error.to_string())
                                                                                    };
                                                                                    this.refresh_extensions();
                                                                                    cx.notify();
                                                                                }
                                                                            })),
                                                                    )
                                                                    .child(
                                                                        div()
                                                                            .id(("deny-capabilities", index))
                                                                            .role(Role::Button)
                                                                            .cursor_pointer()
                                                                            .px_3()
                                                                            .py_1p5()
                                                                            .rounded_full()
                                                                            .bg(cx.theme().surface_container_high)
                                                                            .child("Continue without grants")
                                                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                                                this.extension_action_error = if pending {
                                                                                    this.extension_catalog.approve_pending(&deny_id, Vec::new()).map(|_| ()).err().map(|error| error.to_string())
                                                                                } else {
                                                                                    this.extension_catalog.approve_capabilities(&deny_id, Vec::new()).map(|_| ()).err().map(|error| error.to_string())
                                                                                };
                                                                                this.refresh_extensions();
                                                                                cx.notify();
                                                                            })),
                                                                    ),
                                                            )
                                                    },
                                                )),
                                        )
                                    },
                                ),
                        )
                    })
                    .when_some(
                        active_descriptor.and_then(|page| page.schema),
                        |this, schema| {
                            let fields = schema
                                .get("properties")
                                .and_then(serde_json::Value::as_object)
                                .map(|properties| properties.keys().cloned().collect::<Vec<_>>())
                                .unwrap_or_default();
                            this.child(
                                v_flex()
                                    .gap_2()
                                    .child(div().text_xs().font_bold().child("Extension settings"))
                                    .children(fields.into_iter().map(|field| {
                                        div()
                                            .px_3()
                                            .py_2()
                                            .rounded_xl()
                                            .bg(cx.theme().surface_container)
                                            .child(field)
                                    })),
                            )
                        },
                    ),
            )
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

        let desktop_file = apps_dir.join("com.shilpo.settings.desktop");
        let icon_svg_file = icons_scalable_dir.join("com.shilpo.settings.svg");

        let desktop_content = include_str!("../resources/com.shilpo.settings.desktop");
        let icon_svg_content = include_bytes!("../resources/com.shilpo.settings.svg");

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

        let icon_svg_file = icons_scalable_dir.join("com.shilpo.settings.svg");

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
                    let png_file = size_dir.join("com.shilpo.settings.png");
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

                let pixmap_png = pixmaps_dir.join("com.shilpo.settings.png");
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
        assert_eq!(SettingsCategory::ALL.len(), 9);
        assert_eq!(SettingsCategory::System.label(), "System");
        assert_eq!(SettingsCategory::Extensions.label(), "Extensions");
        assert_eq!(
            ExtensionsSection::ALL.map(ExtensionsSection::label),
            ["Discover", "Installed", "Updates", "Sources"]
        );
    }
}
