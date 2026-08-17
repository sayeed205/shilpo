use gpui::{App, Global, Hsla, Pixels, SharedString, px};

use crate::{
    highlighter::HighlightTheme, list::ListSettings, notification::NotificationSettings,
    scroll::ScrollbarShow, sheet::SheetSettings,
};
pub mod state;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

pub use state::*;

mod color;
mod theme_color;

pub use color::*;
pub use theme_color::*;

const DEFAULT_SOURCE_ARGB: u32 = 0xff006c4c;

pub fn init(cx: &mut App) {
    init_with_source(DEFAULT_SOURCE_ARGB, cx);
}

pub fn init_with_source(source_argb: u32, cx: &mut App) {
    cx.set_global(Theme::new(source_argb));
    Theme::sync_scrollbar_appearance(cx);
}

pub trait ActiveTheme {
    fn theme(&self) -> &Theme;
}

impl ActiveTheme for App {
    fn theme(&self) -> &Theme {
        Theme::global(self)
    }
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub colors: ThemeColor,
    pub highlight_theme: Arc<HighlightTheme>,
    /// User-selected appearance preference.
    pub mode: ThemeMode,
    /// Resolved scheme used to build `colors`. Never `System`.
    effective_mode: ThemeMode,
    pub source_argb: u32,
    pub scheme_variant: SchemeVariant,
    pub font_family: SharedString,
    pub font_size: Pixels,
    pub mono_font_family: SharedString,
    pub mono_font_size: Pixels,
    pub radius: Pixels,
    pub radius_lg: Pixels,
    pub shadow: bool,
    pub transparent: Hsla,
    pub scrollbar_show: ScrollbarShow,
    pub notification: NotificationSettings,
    pub tile_grid_size: Pixels,
    pub tile_shadow: bool,
    pub tile_radius: Pixels,
    pub list: ListSettings,
    pub sheet: SheetSettings,
}

impl Default for Theme {
    fn default() -> Self {
        Self::new(DEFAULT_SOURCE_ARGB)
    }
}

impl Deref for Theme {
    type Target = ThemeColor;
    fn deref(&self) -> &Self::Target {
        &self.colors
    }
}

impl DerefMut for Theme {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.colors
    }
}

impl Global for Theme {}

impl Theme {
    pub fn new(source_argb: u32) -> Self {
        Self {
            colors: material_theme(source_argb, false),
            highlight_theme: HighlightTheme::default_light(),
            mode: ThemeMode::System,
            effective_mode: ThemeMode::Light,
            source_argb,
            scheme_variant: SchemeVariant::Auto,
            font_family: ".SystemUIFont".into(),
            font_size: px(16.),
            mono_font_family: if cfg!(target_os = "macos") {
                "Menlo".into()
            } else if cfg!(target_os = "windows") {
                "Consolas".into()
            } else {
                "DejaVu Sans Mono".into()
            },
            mono_font_size: px(13.),
            radius: px(6.),
            radius_lg: px(8.),
            shadow: true,
            transparent: Hsla::transparent_black(),
            scrollbar_show: ScrollbarShow::default(),
            notification: NotificationSettings::default(),
            tile_grid_size: px(8.),
            tile_shadow: true,
            tile_radius: px(0.),
            list: ListSettings::default(),
            sheet: SheetSettings::default(),
        }
    }

    pub fn apply_state(&mut self, state: &ThemeState) {
        self.source_argb = state.source_argb;
        self.scheme_variant = state.scheme_variant;
        self.mode = state.selected_mode;
        self.effective_mode = state.resolved_mode;
        self.colors = material_theme_with_variant(
            state.source_argb,
            state.scheme_variant,
            state.resolved_mode.is_dark(),
        );
        self.highlight_theme = if state.resolved_mode.is_dark() {
            HighlightTheme::default_dark()
        } else {
            HighlightTheme::default_light()
        };
    }

    pub fn set_source_argb(&mut self, source_argb: u32) {
        self.source_argb = source_argb;
        self.colors = material_theme_with_variant(
            source_argb,
            self.scheme_variant,
            self.effective_mode.is_dark(),
        );
    }

    pub fn global(cx: &App) -> &Theme {
        cx.global::<Theme>()
    }
    pub fn global_mut(cx: &mut App) -> &mut Theme {
        cx.global_mut::<Theme>()
    }
    pub fn is_dark(&self) -> bool {
        self.effective_mode.is_dark()
    }

    /// Returns user-selected appearance preference.
    pub fn selected_mode(&self) -> ThemeMode {
        self.mode
    }

    /// Returns currently resolved light/dark scheme.
    pub fn effective_mode(&self) -> ThemeMode {
        self.effective_mode
    }

    pub fn set_mode(&mut self, mode: ThemeMode) {
        self.mode = mode;
        if mode != ThemeMode::System {
            self.set_effective_mode(mode);
        }
    }

    pub fn set_selected_mode(&mut self, mode: ThemeMode) {
        self.set_mode(mode);
    }

    fn set_effective_mode(&mut self, mode: ThemeMode) {
        debug_assert!(mode != ThemeMode::System);
        self.effective_mode = mode;
        self.colors = material_theme(self.source_argb, mode.is_dark());
        self.highlight_theme = if mode.is_dark() {
            HighlightTheme::default_dark()
        } else {
            HighlightTheme::default_light()
        };
    }

    pub fn sync_scrollbar_appearance(cx: &mut App) {
        Theme::global_mut(cx).scrollbar_show = if cx.should_auto_hide_scrollbars() {
            ScrollbarShow::Scrolling
        } else {
            ScrollbarShow::Hover
        };
    }

    pub fn input_background(&self) -> Hsla {
        self.surface
    }

    pub(crate) fn editor_background(&self) -> Hsla {
        self.highlight_theme
            .style
            .editor_background
            .unwrap_or(self.surface)
    }
}

/// Available theme variants including high-contrast accessibility themes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeVariant {
    #[default]
    Dark,
    Light,
    HighContrastDark,
    HighContrastLight,
}

fn relative_luminance_component(c: f32) -> f32 {
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Calculates WCAG 2.1 relative luminance for an (R, G, B) tuple in 0..=255.
pub fn relative_luminance(r: u8, g: u8, b: u8) -> f32 {
    let r_lin = relative_luminance_component(r as f32 / 255.0);
    let g_lin = relative_luminance_component(g as f32 / 255.0);
    let b_lin = relative_luminance_component(b as f32 / 255.0);
    0.2126 * r_lin + 0.7152 * g_lin + 0.0722 * b_lin
}

/// Calculates contrast ratio (1.0..=21.0) between foreground and background colors.
pub fn calculate_contrast_ratio(fg: (u8, u8, u8), bg: (u8, u8, u8)) -> f32 {
    let l1 = relative_luminance(fg.0, fg.1, fg.2);
    let l2 = relative_luminance(bg.0, bg.1, bg.2);
    let (brighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (brighter + 0.05) / (darker + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_variant_and_contrast_validation() {
        assert_eq!(ThemeVariant::default(), ThemeVariant::Dark);

        let max_contrast = calculate_contrast_ratio((0, 0, 0), (255, 255, 255));
        assert!((max_contrast - 21.0).abs() < 0.1);

        let min_contrast = calculate_contrast_ratio((0, 0, 0), (0, 0, 0));
        assert!((min_contrast - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_visible_focus_indicators_and_disabled_keyboard_policy() {
        let focus_contrast = calculate_contrast_ratio((0, 108, 76), (255, 255, 255));
        assert!(focus_contrast > 3.0);
    }

    #[test]
    fn test_apply_state() {
        let mut theme = Theme::default();
        let mut state = ThemeState::default();
        state.source_argb = 0xff123456;
        state.selected_mode = ThemeMode::Dark;
        state.resolved_mode = ThemeMode::Dark;

        theme.apply_state(&state);
        assert_eq!(theme.source_argb, 0xff123456);
        assert_eq!(theme.selected_mode(), ThemeMode::Dark);
        assert_eq!(theme.effective_mode(), ThemeMode::Dark);
        assert!(theme.is_dark());
    }
}
