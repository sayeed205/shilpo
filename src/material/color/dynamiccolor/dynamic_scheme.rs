
use crate::material::color::hct::Hct;
use crate::material::color::palettes::TonalPalette;

use super::dynamic_scheme_palettes::{get_palettes_spec, maybe_fallback_spec_version};
use super::Variant;
use crate::material::color::options::SchemeOptions;

/// Platform identifier for dynamic scheme.
/// Only used in the 2025 spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Platform {
    /// Phone/tablet platform
    #[default]
    Phone,
    /// Smartwatch platform
    Watch,
}

/// Internal effective specification used by the 2026-only request API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpecVersion {
    /// 2021 Material Design 3 spec
    Spec2021,
    /// 2025 Material Design 3 spec
    Spec2025,
    /// 2026 Material Color spec, selected only for CMF.
    #[default]
    Spec2026,
}

/// Options for creating a DynamicScheme.
#[derive(Clone)]
pub struct DynamicSchemeOptions {
    /// The source color of the theme as an HCT color
    pub source_color_hct: Hct,
    /// Source colors in input order. The first entry is the primary seed.
    pub source_color_hcts: Vec<Hct>,
    /// The variant, or style, of the theme
    pub variant: Variant,
    /// Value from -1 to 1. -1 represents minimum contrast, 0 represents standard, 1 represents maximum contrast
    pub contrast_level: f64,
    /// Whether the scheme is in dark mode or light mode
    pub is_dark: bool,
    /// The platform on which this scheme is intended to be used
    pub platform: Option<Platform>,
    /// The version of the design spec that this scheme is based on
    pub spec_version: Option<SpecVersion>,
    /// Optional custom primary palette
    pub primary_palette: Option<TonalPalette>,
    /// Optional custom secondary palette
    pub secondary_palette: Option<TonalPalette>,
    /// Optional custom tertiary palette
    pub tertiary_palette: Option<TonalPalette>,
    /// Optional custom neutral palette
    pub neutral_palette: Option<TonalPalette>,
    /// Optional custom neutral variant palette
    pub neutral_variant_palette: Option<TonalPalette>,
    /// Optional custom error palette
    pub error_palette: Option<TonalPalette>,
}

impl DynamicSchemeOptions {
    /// Create new DynamicSchemeOptions with required fields.
    pub fn new(
        source_color_hct: Hct,
        variant: Variant,
        contrast_level: f64,
        is_dark: bool,
    ) -> Self {
        DynamicSchemeOptions {
            source_color_hct,
            source_color_hcts: vec![source_color_hct],
            variant,
            contrast_level,
            is_dark,
            platform: None,
            spec_version: None,
            primary_palette: None,
            secondary_palette: None,
            tertiary_palette: None,
            neutral_palette: None,
            neutral_variant_palette: None,
            error_palette: None,
        }
    }
}

/// A dynamic color scheme generated from a source color.
///
/// Constructed by a set of values representing the current UI state (such as
/// whether or not it's dark theme, what the theme style is, etc.), and
/// provides a set of TonalPalettes that can create colors that fit in
/// with the theme style. Used by DynamicColor to resolve into a color.
#[derive(Clone)]
pub struct DynamicScheme {
    /// The source color of the theme as an HCT color
    pub source_color_hct: Hct,

    /// Source colors in input order; the primary source is element zero.
    pub source_color_hcts: Vec<Hct>,

    /// The source color of the theme as an ARGB 32-bit integer
    pub source_color_argb: u32,

    /// The variant, or style, of the theme
    pub variant: Variant,

    /// Value from -1 to 1. -1 represents minimum contrast, 0 represents standard, 1 represents maximum contrast
    pub contrast_level: f64,

    /// Whether the scheme is in dark mode or light mode
    pub is_dark: bool,

    /// The platform on which this scheme is intended to be used
    pub platform: Platform,

    /// The version of the design spec that this scheme is based on
    pub spec_version: SpecVersion,

    /// Given a tone, produces a color. Hue and chroma of the color are specified in the design specification of the variant. Usually colorful.
    pub primary_palette: TonalPalette,

    /// Given a tone, produces a color. Hue and chroma of the color are specified in the design specification of the variant. Usually less colorful.
    pub secondary_palette: TonalPalette,

    /// Given a tone, produces a color. Hue and chroma of the color are specified in the design specification of the variant. Usually a different hue from primary and colorful.
    pub tertiary_palette: TonalPalette,

    /// Given a tone, produces a color. Hue and chroma of the color are specified in the design specification of the variant. Usually not colorful at all, intended for background & surface colors.
    pub neutral_palette: TonalPalette,

    /// Given a tone, produces a color. Hue and chroma of the color are specified in the design specification of the variant. Usually not colorful, but slightly more colorful than Neutral. Intended for backgrounds & surfaces.
    pub neutral_variant_palette: TonalPalette,

    /// Given a tone, produces a reddish, colorful, color.
    pub error_palette: TonalPalette,
}

impl DynamicScheme {
    /// Build a checked public request. The caller validates the non-empty
    /// source list before entering this implementation.
    pub(crate) fn from_options(source_color_hcts: Vec<Hct>, options: SchemeOptions) -> Self {
        let source_color_hct = *source_color_hcts
            .first()
            .expect("Scheme validates that at least one source color is supplied");
        let mut dynamic_options = DynamicSchemeOptions::new(
            source_color_hct,
            options.variant,
            options.contrast_level,
            options.is_dark,
        );
        dynamic_options.source_color_hcts = source_color_hcts;
        dynamic_options.platform = Some(options.platform);
        dynamic_options.spec_version = Some(SpecVersion::Spec2026);
        dynamic_options.primary_palette = options.primary_palette;
        dynamic_options.secondary_palette = options.secondary_palette;
        dynamic_options.tertiary_palette = options.tertiary_palette;
        dynamic_options.neutral_palette = options.neutral_palette;
        dynamic_options.neutral_variant_palette = options.neutral_variant_palette;
        dynamic_options.error_palette = options.error_palette;
        Self::new(dynamic_options)
    }

    /// Create a new DynamicScheme from options.
    ///
    /// Uses spec-versioned palette generation delegates based on the variant
    /// and spec version. The delegate handles variant-specific, platform-aware,
    /// and dark/light mode adjustments.
    ///
    /// # Arguments
    /// * `options` - Configuration for the scheme including source color, variant,
    ///   contrast level, dark mode, platform, and optional custom palettes.
    ///
    /// # Returns
    /// A fully configured DynamicScheme with all palettes generated.
    pub fn new(options: DynamicSchemeOptions) -> Self {
        let source_color_hcts = if options.source_color_hcts.is_empty() {
            vec![options.source_color_hct]
        } else {
            options.source_color_hcts.clone()
        };
        let source_color_hct = source_color_hcts[0];
        let source_color_argb = source_color_hct.to_int();
        let platform = options.platform.unwrap_or_default();
        let requested_spec = options.spec_version.unwrap_or(SpecVersion::Spec2026);

        // Determine the effective spec version (may fall back to 2021 for some variants)
        let spec_version = maybe_fallback_spec_version(requested_spec, options.variant);
        let spec = get_palettes_spec(spec_version);

        // Use provided palettes or generate via spec delegate
        let cmf_secondary = source_color_hcts.get(1).copied().unwrap_or(source_color_hct);
        let cmf_default_palette = |slot: usize| match slot {
            0 => TonalPalette::from_hue_and_chroma(
                source_color_hct.hue(),
                source_color_hct.chroma(),
            ),
            1 => TonalPalette::from_hue_and_chroma(
                source_color_hct.hue(),
                source_color_hct.chroma() * 0.5,
            ),
            2 if source_color_hct.to_int() == cmf_secondary.to_int() => {
                TonalPalette::from_hue_and_chroma(
                    source_color_hct.hue(),
                    source_color_hct.chroma() * 0.75,
                )
            }
            2 => TonalPalette::from_hue_and_chroma(cmf_secondary.hue(), cmf_secondary.chroma()),
            3 | 4 => TonalPalette::from_hue_and_chroma(
                source_color_hct.hue(),
                source_color_hct.chroma() * 0.2,
            ),
            _ => TonalPalette::from_hue_and_chroma(
                cmf_error_hue(source_color_hct.hue(), cmf_secondary.hue()),
                source_color_hct.chroma().max(50.0),
            ),
        };

        let primary_palette = options.primary_palette.unwrap_or_else(|| {
            if options.variant == Variant::Cmf {
                return cmf_default_palette(0);
            }
            spec.get_primary_palette(
                options.variant,
                &source_color_hct,
                options.is_dark,
                platform,
                options.contrast_level,
            )
        });

        let secondary_palette = options.secondary_palette.unwrap_or_else(|| {
            if options.variant == Variant::Cmf {
                return cmf_default_palette(1);
            }
            spec.get_secondary_palette(
                options.variant,
                &source_color_hct,
                options.is_dark,
                platform,
                options.contrast_level,
            )
        });

        let tertiary_palette = options.tertiary_palette.unwrap_or_else(|| {
            if options.variant == Variant::Cmf {
                return cmf_default_palette(2);
            }
            spec.get_tertiary_palette(
                options.variant,
                &source_color_hct,
                options.is_dark,
                platform,
                options.contrast_level,
            )
        });

        let neutral_palette = options.neutral_palette.unwrap_or_else(|| {
            if options.variant == Variant::Cmf {
                return cmf_default_palette(3);
            }
            spec.get_neutral_palette(
                options.variant,
                &source_color_hct,
                options.is_dark,
                platform,
                options.contrast_level,
            )
        });

        let neutral_variant_palette = options.neutral_variant_palette.unwrap_or_else(|| {
            if options.variant == Variant::Cmf {
                return cmf_default_palette(4);
            }
            spec.get_neutral_variant_palette(
                options.variant,
                &source_color_hct,
                options.is_dark,
                platform,
                options.contrast_level,
            )
        });

        let error_palette = options.error_palette.unwrap_or_else(|| {
            if options.variant == Variant::Cmf {
                return cmf_default_palette(5);
            }
            spec.get_error_palette(
                options.variant,
                &source_color_hct,
                options.is_dark,
                platform,
                options.contrast_level,
            )
            .unwrap_or_else(|| TonalPalette::from_hue_and_chroma(25.0, 84.0))
        });

        DynamicScheme {
            source_color_hct,
            source_color_hcts,
            source_color_argb,
            variant: options.variant,
            contrast_level: options.contrast_level,
            is_dark: options.is_dark,
            platform,
            spec_version,
            primary_palette,
            secondary_palette,
            tertiary_palette,
            neutral_palette,
            neutral_variant_palette,
            error_palette,
        }
    }
}

/// Canonical CMF error-hue lookup from the 2026 `SchemeCmf` constructor.
pub(crate) fn cmf_error_hue(primary_hue: f64, tertiary_hue: f64) -> f64 {
    if primary_hue <= 8.0 {
        if tertiary_hue <= 24.0 { 28.0 } else if tertiary_hue <= 32.0 { 16.0 } else { 20.0 }
    } else if primary_hue <= 16.0 {
        if tertiary_hue <= 24.0 { 32.0 } else if tertiary_hue <= 32.0 { 20.0 } else { 24.0 }
    } else if primary_hue <= 20.0 {
        if tertiary_hue <= 28.0 { 32.0 } else if tertiary_hue <= 32.0 { 24.0 } else { 28.0 }
    } else if primary_hue <= 28.0 {
        if tertiary_hue <= 24.0 { 32.0 } else { 16.0 }
    } else if primary_hue <= 32.0 {
        if tertiary_hue <= 20.0 { 24.0 } else if tertiary_hue <= 28.0 { 16.0 } else { 20.0 }
    } else if primary_hue <= 40.0 {
        if tertiary_hue > 20.0 && tertiary_hue <= 28.0 { 16.0 } else { 24.0 }
    } else if primary_hue <= 152.0 {
        if tertiary_hue > 24.0 && tertiary_hue <= 36.0 { 20.0 } else { 32.0 }
    } else if primary_hue <= 272.0 {
        if tertiary_hue > 20.0 && tertiary_hue <= 28.0 { 16.0 } else { 24.0 }
    } else if tertiary_hue > 12.0 && tertiary_hue <= 28.0 {
        32.0
    } else {
        16.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_default() {
        assert_eq!(Platform::default(), Platform::Phone);
    }

    #[test]
    fn test_spec_version_default() {
        assert_eq!(SpecVersion::default(), SpecVersion::Spec2026);
    }

    #[test]
    fn test_dynamic_scheme_options_new() {
        let hct = Hct::from_int(0xFF0000FF); // Blue
        let options = DynamicSchemeOptions::new(hct, Variant::TonalSpot, 0.0, false);

        assert_eq!(options.variant, Variant::TonalSpot);
        assert_eq!(options.contrast_level, 0.0);
        assert!(!options.is_dark);
        assert!(options.platform.is_none());
        assert!(options.spec_version.is_none());
    }

    #[test]
    fn test_dynamic_scheme_new_light_mode() {
        let hct = Hct::from_int(0xFF0000FF); // Blue
        let options = DynamicSchemeOptions::new(hct, Variant::TonalSpot, 0.0, false);
        let scheme = DynamicScheme::new(options);

        assert_eq!(scheme.variant, Variant::TonalSpot);
        assert_eq!(scheme.contrast_level, 0.0);
        assert!(!scheme.is_dark);
        assert_eq!(scheme.platform, Platform::Phone);
        assert_eq!(scheme.spec_version, SpecVersion::Spec2025);
        assert_eq!(scheme.source_color_argb, 0xFF0000FF);
    }

    #[test]
    fn test_dynamic_scheme_new_dark_mode() {
        let hct = Hct::from_int(0xFFFF0000); // Red
        let options = DynamicSchemeOptions::new(hct, Variant::Vibrant, 0.0, true);
        let scheme = DynamicScheme::new(options);

        assert_eq!(scheme.variant, Variant::Vibrant);
        assert!(scheme.is_dark);
    }

    #[test]
    fn test_dynamic_scheme_contrast_levels() {
        let hct = Hct::from_int(0xFF00FF00); // Green

        let low_contrast = DynamicScheme::new(DynamicSchemeOptions::new(
            hct,
            Variant::TonalSpot,
            -1.0,
            false,
        ));
        assert_eq!(low_contrast.contrast_level, -1.0);

        let normal_contrast = DynamicScheme::new(DynamicSchemeOptions::new(
            hct,
            Variant::TonalSpot,
            0.0,
            false,
        ));
        assert_eq!(normal_contrast.contrast_level, 0.0);

        let high_contrast = DynamicScheme::new(DynamicSchemeOptions::new(
            hct,
            Variant::TonalSpot,
            1.0,
            false,
        ));
        assert_eq!(high_contrast.contrast_level, 1.0);
    }

    #[test]
    fn test_dynamic_scheme_custom_palettes() {
        let hct = Hct::from_int(0xFF0000FF);
        let custom_primary = TonalPalette::from_hue_and_chroma(120.0, 50.0);

        let mut options = DynamicSchemeOptions::new(hct, Variant::TonalSpot, 0.0, false);
        options.primary_palette = Some(custom_primary.clone());

        let scheme = DynamicScheme::new(options);

        assert_eq!(scheme.primary_palette.hue(), custom_primary.hue());
        assert_eq!(scheme.primary_palette.chroma(), custom_primary.chroma());
    }

    #[test]
    fn test_dynamic_scheme_all_variants() {
        let hct = Hct::from_int(0xFF0000FF);
        let variants = [
            Variant::Monochrome,
            Variant::Neutral,
            Variant::TonalSpot,
            Variant::Vibrant,
            Variant::Expressive,
        ];

        for variant in &variants {
            let options = DynamicSchemeOptions::new(hct, *variant, 0.0, false);
            let scheme = DynamicScheme::new(options);
            assert_eq!(scheme.variant, *variant);
        }
    }

    #[test]
    fn test_dynamic_scheme_clone() {
        let hct = Hct::from_int(0xFF0000FF);
        let options = DynamicSchemeOptions::new(hct, Variant::TonalSpot, 0.0, false);
        let scheme1 = DynamicScheme::new(options);
        let scheme2 = scheme1.clone();

        assert_eq!(scheme1.source_color_argb, scheme2.source_color_argb);
        assert_eq!(scheme1.variant, scheme2.variant);
        assert_eq!(scheme1.is_dark, scheme2.is_dark);
    }

    #[test]
    fn test_platform_variants() {
        assert_eq!(Platform::Phone, Platform::Phone);
        assert_eq!(Platform::Watch, Platform::Watch);
        assert_ne!(Platform::Phone, Platform::Watch);
    }

    #[test]
    fn test_spec_version_variants() {
        assert_eq!(SpecVersion::Spec2021, SpecVersion::Spec2021);
        assert_eq!(SpecVersion::Spec2025, SpecVersion::Spec2025);
        assert_ne!(SpecVersion::Spec2021, SpecVersion::Spec2025);
    }
}
