// <FILE>crates/mcu-dynamiccolor/src/dynamic_scheme_palettes.rs</FILE> - <DESC>Palette generation delegates for DynamicScheme</DESC>
// <VERS>VERSION: 2.0.0</VERS>
// <WCTX>OFPF refactor: Split large file into trait + separate impl modules</WCTX>
// <CLOG>Extract 2021/2025 implementations to impl_palettes_2021.rs and impl_palettes_2025.rs; keep trait, helpers, factory</CLOG>

//! Palette generation delegates for DynamicScheme.
//!
//! This module implements spec-versioned palette generation logic that varies
//! based on variant, platform, dark mode, and contrast level.
//!
//! The implementations are split into separate modules:
//! - [`impl_palettes_2021`](crate::impl_palettes_2021) - 2021 spec implementation
//! - [`impl_palettes_2025`](crate::impl_palettes_2025) - 2025 spec implementation

use super::{Platform, SpecVersion, Variant};
use crate::material_color::hct::Hct;
use crate::material_color::palettes::TonalPalette;
use crate::material_color::utils::math::sanitize_degrees_double;

// Re-export implementation structs from their modules
pub use super::impl_palettes_2021::DynamicSchemePalettesDelegateImpl2021;
pub use super::impl_palettes_2025::DynamicSchemePalettesDelegateImpl2025;

/// Delegate for generating palettes based on scheme configuration.
///
/// Different spec versions may produce different palettes for the same variant
/// and source color. This trait defines the palette generation contract.
pub trait DynamicSchemePalettesDelegate {
    /// Generate the primary palette for the given scheme configuration.
    fn get_primary_palette(
        &self,
        variant: Variant,
        source: &Hct,
        is_dark: bool,
        platform: Platform,
        contrast: f64,
    ) -> TonalPalette;

    /// Generate the secondary palette for the given scheme configuration.
    fn get_secondary_palette(
        &self,
        variant: Variant,
        source: &Hct,
        is_dark: bool,
        platform: Platform,
        contrast: f64,
    ) -> TonalPalette;

    /// Generate the tertiary palette for the given scheme configuration.
    fn get_tertiary_palette(
        &self,
        variant: Variant,
        source: &Hct,
        is_dark: bool,
        platform: Platform,
        contrast: f64,
    ) -> TonalPalette;

    /// Generate the neutral palette for the given scheme configuration.
    fn get_neutral_palette(
        &self,
        variant: Variant,
        source: &Hct,
        is_dark: bool,
        platform: Platform,
        contrast: f64,
    ) -> TonalPalette;

    /// Generate the neutral variant palette for the given scheme configuration.
    fn get_neutral_variant_palette(
        &self,
        variant: Variant,
        source: &Hct,
        is_dark: bool,
        platform: Platform,
        contrast: f64,
    ) -> TonalPalette;

    /// Generate the error palette for the given scheme configuration.
    /// Returns None if the default error palette should be used.
    fn get_error_palette(
        &self,
        variant: Variant,
        source: &Hct,
        is_dark: bool,
        platform: Platform,
        contrast: f64,
    ) -> Option<TonalPalette>;
}

// ============================================================================
// Helper Constants and Functions (shared by both implementations)
// ============================================================================

/// Hue breakpoints for expressive variant rotations.
pub const HUES: [f64; 9] = [0.0, 21.0, 51.0, 121.0, 151.0, 191.0, 271.0, 321.0, 360.0];

/// Secondary palette hue rotations for expressive variant.
pub const SECONDARY_ROTATIONS_EXPRESSIVE: [f64; 9] =
    [45.0, 95.0, 45.0, 20.0, 45.0, 90.0, 45.0, 45.0, 45.0];

/// Tertiary palette hue rotations for expressive variant.
pub const TERTIARY_ROTATIONS_EXPRESSIVE: [f64; 9] =
    [120.0, 120.0, 20.0, 45.0, 20.0, 15.0, 20.0, 120.0, 120.0];

/// Hue rotations for vibrant variant.
pub const VIBRANT_SECONDARY_ROTATIONS: [f64; 9] =
    [18.0, 15.0, 10.0, 12.0, 15.0, 18.0, 15.0, 12.0, 12.0];
pub const VIBRANT_TERTIARY_ROTATIONS: [f64; 9] =
    [35.0, 30.0, 20.0, 25.0, 30.0, 35.0, 30.0, 25.0, 25.0];

/// Check if a hue is in the blue range (approximately 200-280 degrees).
pub fn is_blue_hue(hue: f64) -> bool {
    hue >= 200.0 && hue < 280.0
}

/// Get a rotated hue based on piecewise function.
///
/// # Arguments
/// * `source` - The source HCT color
/// * `breakpoints` - Array of hue breakpoints
/// * `rotations` - Array of rotation values for each segment
///
/// # Returns
/// The rotated hue value normalized to 0-360.
pub fn get_rotated_hue(source: &Hct, breakpoints: &[f64], rotations: &[f64]) -> f64 {
    let hue = source.hue();
    if breakpoints.is_empty() || rotations.is_empty() {
        return hue;
    }

    let rotation = get_piecewise_value(hue, breakpoints, rotations);
    sanitize_degrees_double(hue + rotation)
}

/// Get a piecewise value based on breakpoints.
///
/// Finds the rotation value for the segment containing the given hue.
pub fn get_piecewise_value(hue: f64, breakpoints: &[f64], values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }

    let n = breakpoints.len().saturating_sub(1).min(values.len());
    if n == 0 {
        return values.first().copied().unwrap_or(0.0);
    }

    // Find the segment containing the hue
    for i in (0..n).rev() {
        if hue >= breakpoints[i] {
            return values[i];
        }
    }

    values.last().copied().unwrap_or(0.0)
}

// ============================================================================
// Spec Factory Function
// ============================================================================

/// Get the palette delegate for a spec version.
///
/// # Arguments
/// * `spec_version` - The specification version
///
/// # Returns
/// A static reference to the appropriate delegate implementation.
pub fn get_palettes_spec(spec_version: SpecVersion) -> &'static dyn DynamicSchemePalettesDelegate {
    static SPEC_2021: DynamicSchemePalettesDelegateImpl2021 = DynamicSchemePalettesDelegateImpl2021;
    static SPEC_2025: DynamicSchemePalettesDelegateImpl2025 = DynamicSchemePalettesDelegateImpl2025;

    match spec_version {
        SpecVersion::Spec2025 => &SPEC_2025,
        // The 2026 palette delegate inherits the 2025 rules for ordinary
        // variants; CMF's six palettes are constructed from its source list by
        // DynamicScheme before consulting this delegate.
        SpecVersion::Spec2026 => &SPEC_2025,
        SpecVersion::Spec2021 => &SPEC_2021,
    }
}

/// Potentially fall back to 2021 spec for variants that don't support 2025 features.
///
/// # Arguments
/// * `spec` - The requested spec version
/// * `variant` - The variant being used
///
/// # Returns
/// The effective spec version to use (may be 2021 if variant doesn't support 2025).
pub fn maybe_fallback_spec_version(spec: SpecVersion, variant: Variant) -> SpecVersion {
    if variant == Variant::Cmf {
        return SpecVersion::Spec2026;
    }
    match variant {
        // Canonical 2026 spec uses the 2025 delegate for these four variants.
        Variant::Expressive | Variant::Vibrant | Variant::TonalSpot | Variant::Neutral => {
            if spec == SpecVersion::Spec2026 { SpecVersion::Spec2025 } else { spec }
        }
        // Other variants retain the canonical 2021 palette and color rules.
        _ => SpecVersion::Spec2021,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::approx::assert_relative_eq;

    fn blue_hct() -> Hct {
        Hct::from_int(0xFF0000FF)
    }

    fn red_hct() -> Hct {
        Hct::from_int(0xFFFF0000)
    }

    fn green_hct() -> Hct {
        Hct::from_int(0xFF00FF00)
    }

    #[test]
    fn test_is_blue_hue() {
        assert!(is_blue_hue(220.0));
        assert!(is_blue_hue(240.0));
        assert!(!is_blue_hue(0.0));
        assert!(!is_blue_hue(120.0));
        assert!(!is_blue_hue(300.0));
    }

    #[test]
    fn test_get_piecewise_value() {
        let breakpoints = [0.0, 90.0, 180.0, 270.0, 360.0];
        let values = [10.0, 20.0, 30.0, 40.0];

        assert_relative_eq!(get_piecewise_value(0.0, &breakpoints, &values), 10.0);
        assert_relative_eq!(get_piecewise_value(45.0, &breakpoints, &values), 10.0);
        assert_relative_eq!(get_piecewise_value(100.0, &breakpoints, &values), 20.0);
        assert_relative_eq!(get_piecewise_value(200.0, &breakpoints, &values), 30.0);
        assert_relative_eq!(get_piecewise_value(300.0, &breakpoints, &values), 40.0);
    }

    #[test]
    fn test_2021_primary_tonal_spot() {
        let delegate = DynamicSchemePalettesDelegateImpl2021;
        let source = blue_hct();

        let palette =
            delegate.get_primary_palette(Variant::TonalSpot, &source, false, Platform::Phone, 0.0);

        assert_relative_eq!(palette.hue(), source.hue(), epsilon = 0.1);
        assert_relative_eq!(palette.chroma(), 36.0, epsilon = 0.1);
    }

    #[test]
    fn test_2021_primary_monochrome() {
        let delegate = DynamicSchemePalettesDelegateImpl2021;
        let source = red_hct();

        let palette =
            delegate.get_primary_palette(Variant::Monochrome, &source, false, Platform::Phone, 0.0);

        assert_relative_eq!(palette.chroma(), 0.0, epsilon = 0.1);
    }

    #[test]
    fn test_2021_primary_content() {
        let delegate = DynamicSchemePalettesDelegateImpl2021;
        let source = green_hct();

        let palette =
            delegate.get_primary_palette(Variant::Content, &source, false, Platform::Phone, 0.0);

        // Content uses source chroma
        assert_relative_eq!(palette.hue(), source.hue(), epsilon = 0.1);
        assert_relative_eq!(palette.chroma(), source.chroma(), epsilon = 0.1);
    }

    #[test]
    fn test_2021_error_returns_none() {
        let delegate = DynamicSchemePalettesDelegateImpl2021;
        let source = blue_hct();

        let error =
            delegate.get_error_palette(Variant::TonalSpot, &source, false, Platform::Phone, 0.0);

        assert!(error.is_none());
    }

    #[test]
    fn test_2025_primary_tonal_spot_dark() {
        let delegate = DynamicSchemePalettesDelegateImpl2025;
        let source = blue_hct();

        let palette = delegate.get_primary_palette(
            Variant::TonalSpot,
            &source,
            true, // dark mode
            Platform::Phone,
            0.0,
        );

        // 2025 phone + dark = 26.0 chroma
        assert_relative_eq!(palette.chroma(), 26.0, epsilon = 0.1);
    }

    #[test]
    fn test_2025_primary_tonal_spot_light() {
        let delegate = DynamicSchemePalettesDelegateImpl2025;
        let source = blue_hct();

        let palette = delegate.get_primary_palette(
            Variant::TonalSpot,
            &source,
            false, // light mode
            Platform::Phone,
            0.0,
        );

        // 2025 phone + light = 32.0 chroma
        assert_relative_eq!(palette.chroma(), 32.0, epsilon = 0.1);
    }

    #[test]
    fn test_2025_falls_back_for_unsupported_variants() {
        let delegate = DynamicSchemePalettesDelegateImpl2025;
        let source = blue_hct();

        // Monochrome falls back to 2021
        let palette =
            delegate.get_primary_palette(Variant::Monochrome, &source, false, Platform::Phone, 0.0);

        assert_relative_eq!(palette.chroma(), 0.0, epsilon = 0.1);
    }

    #[test]
    fn test_maybe_fallback_spec_version() {
        // 2025-supported variants stay 2025
        assert_eq!(
            maybe_fallback_spec_version(SpecVersion::Spec2025, Variant::TonalSpot),
            SpecVersion::Spec2025
        );
        assert_eq!(
            maybe_fallback_spec_version(SpecVersion::Spec2025, Variant::Expressive),
            SpecVersion::Spec2025
        );

        // Non-2025 variants fall back to 2021
        assert_eq!(
            maybe_fallback_spec_version(SpecVersion::Spec2025, Variant::Monochrome),
            SpecVersion::Spec2021
        );
        assert_eq!(
            maybe_fallback_spec_version(SpecVersion::Spec2025, Variant::Content),
            SpecVersion::Spec2021
        );

        // 2021 stays 2021
        assert_eq!(
            maybe_fallback_spec_version(SpecVersion::Spec2021, Variant::TonalSpot),
            SpecVersion::Spec2021
        );
    }

    #[test]
    fn test_get_palettes_spec_returns_correct_delegate() {
        let _spec_2021 = get_palettes_spec(SpecVersion::Spec2021);
        let _spec_2025 = get_palettes_spec(SpecVersion::Spec2025);
        // Just verify they don't panic and return something
    }

    #[test]
    fn test_all_variants_produce_palettes_2021() {
        let delegate = DynamicSchemePalettesDelegateImpl2021;
        let source = blue_hct();

        let variants = [
            Variant::Monochrome,
            Variant::Neutral,
            Variant::TonalSpot,
            Variant::Vibrant,
            Variant::Expressive,
            Variant::Fidelity,
            Variant::Content,
            Variant::Rainbow,
            Variant::FruitSalad,
        ];

        for variant in variants {
            let primary =
                delegate.get_primary_palette(variant, &source, false, Platform::Phone, 0.0);
            let secondary =
                delegate.get_secondary_palette(variant, &source, false, Platform::Phone, 0.0);
            let tertiary =
                delegate.get_tertiary_palette(variant, &source, false, Platform::Phone, 0.0);
            let neutral =
                delegate.get_neutral_palette(variant, &source, false, Platform::Phone, 0.0);
            let neutral_variant =
                delegate.get_neutral_variant_palette(variant, &source, false, Platform::Phone, 0.0);

            // Just verify we get palettes without panicking
            assert!(primary.hue() >= 0.0);
            assert!(secondary.hue() >= 0.0);
            assert!(tertiary.hue() >= 0.0);
            assert!(neutral.hue() >= 0.0);
            assert!(neutral_variant.hue() >= 0.0);
        }
    }

    #[test]
    fn test_2025_error_palette_returns_some_for_supported_variants() {
        let delegate = DynamicSchemePalettesDelegateImpl2025;
        let source = blue_hct();

        let error =
            delegate.get_error_palette(Variant::TonalSpot, &source, false, Platform::Phone, 0.0);

        assert!(error.is_some());
        let palette = error.unwrap();
        // The canonical 2025 phone Tonal Spot error palette uses chroma 60.
        assert_relative_eq!(palette.chroma(), 60.0, epsilon = 0.1);
    }

    #[test]
    fn test_2025_error_palette_returns_none_for_unsupported_variants() {
        let delegate = DynamicSchemePalettesDelegateImpl2025;
        let source = blue_hct();

        let error =
            delegate.get_error_palette(Variant::Monochrome, &source, false, Platform::Phone, 0.0);

        assert!(error.is_none());
    }
}

// <FILE>crates/mcu-dynamiccolor/src/dynamic_scheme_palettes.rs</FILE> - <DESC>Palette generation delegates for DynamicScheme</DESC>
// <VERS>END OF VERSION: 2.0.0</VERS>
