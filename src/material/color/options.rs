// Request configuration for the single public, 2026-entry scheme interface.

use crate::material::color::dynamiccolor::{Platform, Variant};
use crate::material::color::palettes::TonalPalette;

/// Scheme inputs other than the one-or-more source ARGB colors.
///
/// Contrast is forwarded unmodified to the canonical dynamic-color rules.
/// Palette overrides are used as supplied; any omitted palette is generated
/// from the source color(s) and selected variant.
#[derive(Clone)]
pub struct SchemeOptions {
    pub(crate) variant: Variant,
    pub(crate) is_dark: bool,
    pub(crate) contrast_level: f64,
    pub(crate) platform: Platform,
    pub(crate) primary_palette: Option<TonalPalette>,
    pub(crate) secondary_palette: Option<TonalPalette>,
    pub(crate) tertiary_palette: Option<TonalPalette>,
    pub(crate) neutral_palette: Option<TonalPalette>,
    pub(crate) neutral_variant_palette: Option<TonalPalette>,
    pub(crate) error_palette: Option<TonalPalette>,
}

impl SchemeOptions {
    /// Standard contrast on a phone/tablet platform.
    pub fn new(variant: Variant, is_dark: bool) -> Self {
        Self {
            variant,
            is_dark,
            contrast_level: 0.0,
            platform: Platform::Phone,
            primary_palette: None,
            secondary_palette: None,
            tertiary_palette: None,
            neutral_palette: None,
            neutral_variant_palette: None,
            error_palette: None,
        }
    }

    /// Set the contrast request without range clamping.
    pub fn with_contrast_level(mut self, contrast_level: f64) -> Self {
        self.contrast_level = contrast_level;
        self
    }

    pub fn with_platform(mut self, platform: Platform) -> Self {
        self.platform = platform;
        self
    }

    pub fn with_primary_palette(mut self, palette: TonalPalette) -> Self {
        self.primary_palette = Some(palette);
        self
    }

    pub fn with_secondary_palette(mut self, palette: TonalPalette) -> Self {
        self.secondary_palette = Some(palette);
        self
    }

    pub fn with_tertiary_palette(mut self, palette: TonalPalette) -> Self {
        self.tertiary_palette = Some(palette);
        self
    }

    pub fn with_neutral_palette(mut self, palette: TonalPalette) -> Self {
        self.neutral_palette = Some(palette);
        self
    }

    pub fn with_neutral_variant_palette(mut self, palette: TonalPalette) -> Self {
        self.neutral_variant_palette = Some(palette);
        self
    }

    pub fn with_error_palette(mut self, palette: TonalPalette) -> Self {
        self.error_palette = Some(palette);
        self
    }

    pub fn variant(&self) -> Variant {
        self.variant
    }

    pub fn is_dark(&self) -> bool {
        self.is_dark
    }

    pub fn contrast_level(&self) -> f64 {
        self.contrast_level
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }
}

impl Default for SchemeOptions {
    fn default() -> Self {
        Self::new(Variant::TonalSpot, false)
    }
}
