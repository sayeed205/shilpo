// <FILE>crates/mcu-dynamiccolor/src/color_calculation.rs</FILE> - <DESC>ColorCalculationDelegate trait and spec-versioned implementations</DESC>
// <VERS>VERSION: 3.0.0</VERS>
// <WCTX>OFPF refactor: Split large file into trait + separate impl modules</WCTX>
// <CLOG>Extract 2021/2025 implementations to impl_calc_2021.rs and impl_calc_2025.rs; keep trait, statics, factory</CLOG>

//! # Color Calculation Delegate
//!
//! Provides the `ColorCalculationDelegate` trait and spec-versioned implementations
//! for calculating DynamicColor tones and HCT values.
//!
//! Different Material Design spec versions (2021 vs 2025) have different calculation
//! algorithms for resolving dynamic colors. This module provides:
//!
//! - [`ColorCalculationDelegate`] - Trait defining the calculation interface
//! - [`ColorCalculationDelegate2021`] - Implementation for 2021 spec
//! - [`ColorCalculationDelegate2025`] - Implementation for 2025 spec
//! - [`get_spec`] - Dispatcher function to get the appropriate delegate
//!
//! The implementations are split into separate modules:
//! - [`impl_calc_2021`](crate::impl_calc_2021) - 2021 spec implementation
//! - [`impl_calc_2025`](crate::impl_calc_2025) - 2025 spec implementation
//!
//! ## Example
//!
//! ```ignore
//! use mcu_dynamiccolor::{SpecVersion, color_calculation::get_spec};
//!
//! let delegate = get_spec(SpecVersion::Spec2021);
//! let tone = delegate.get_tone(&scheme, &color);
//! let hct = delegate.get_hct(&scheme, &color);
//! ```

use crate::material_color::hct::Hct;

use super::{DynamicColor, DynamicScheme, SpecVersion};

// Re-export implementation structs from their modules
pub use super::impl_calc_2021::ColorCalculationDelegate2021;
pub use super::impl_calc_2025::ColorCalculationDelegate2025;

/// Delegate for calculating DynamicColor tones and HCT values.
///
/// Different spec versions have different calculation algorithms. The 2021 spec
/// uses contrast curves and tone delta pairs for adjustments, while the 2025 spec
/// adds additional logic for dim variants and platform-specific behavior.
///
/// This trait allows the calculation logic to be cleanly separated by spec version
/// while maintaining a common interface.
pub trait ColorCalculationDelegate: Send + Sync {
    /// Calculate the HCT color for a DynamicColor in the given scheme.
    ///
    /// # Arguments
    ///
    /// * `scheme` - The DynamicScheme providing UI state context
    /// * `color` - The DynamicColor to resolve
    ///
    /// # Returns
    ///
    /// The resolved HCT color
    fn get_hct(&self, scheme: &DynamicScheme, color: &DynamicColor) -> Hct;

    /// Calculate the tone for a DynamicColor in the given scheme.
    ///
    /// The tone calculation may involve:
    /// - Contrast adjustments against background(s)
    /// - Tone delta pair constraints
    /// - Contrast level modifications
    /// - Spec-specific adjustments (e.g., dim variants in 2025)
    ///
    /// # Arguments
    ///
    /// * `scheme` - The DynamicScheme providing UI state context
    /// * `color` - The DynamicColor to resolve
    ///
    /// # Returns
    ///
    /// The resolved tone value (0.0 to 100.0)
    fn get_tone(&self, scheme: &DynamicScheme, color: &DynamicColor) -> f64;
}

/// Static instance of the 2021 spec delegate.
static SPEC_2021: ColorCalculationDelegate2021 = ColorCalculationDelegate2021;

/// Static instance of the 2025 spec delegate.
static SPEC_2025: ColorCalculationDelegate2025 = ColorCalculationDelegate2025;

/// Returns the ColorCalculationDelegate for the given spec version.
///
/// This dispatcher function provides access to the appropriate calculation
/// delegate based on the spec version. The delegates are statically allocated
/// for efficiency.
///
/// # Arguments
///
/// * `spec_version` - The Material Design spec version
///
/// # Returns
///
/// A reference to the appropriate `ColorCalculationDelegate` implementation
///
/// # Example
///
/// ```ignore
/// use mcu_dynamiccolor::{SpecVersion, color_calculation::get_spec};
///
/// let delegate_2021 = get_spec(SpecVersion::Spec2021);
/// let delegate_2025 = get_spec(SpecVersion::Spec2025);
/// ```
pub fn get_spec(spec_version: SpecVersion) -> &'static dyn ColorCalculationDelegate {
    match spec_version {
        SpecVersion::Spec2021 => &SPEC_2021,
        SpecVersion::Spec2025 => &SPEC_2025,
        SpecVersion::Spec2026 => &SPEC_2025,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material_color::dynamiccolor::{ContrastCurve, DynamicSchemeOptions, ToneDeltaPair, Variant};

    fn create_test_scheme() -> DynamicScheme {
        let source_color = Hct::from_int(0xFF0000FF); // Blue
        let options = DynamicSchemeOptions::new(source_color, Variant::TonalSpot, 0.0, false);
        DynamicScheme::new(options)
    }

    fn create_test_color() -> DynamicColor {
        DynamicColor::from_palette(
            "test",
            |scheme: &DynamicScheme| scheme.primary_palette.clone(),
            Some(|_scheme: &DynamicScheme| 40.0),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<ContrastCurve>>,
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    #[test]
    fn test_get_spec_returns_correct_delegate() {
        let delegate_2021 = get_spec(SpecVersion::Spec2021);
        let delegate_2025 = get_spec(SpecVersion::Spec2025);

        // Both should be valid references
        let scheme = create_test_scheme();
        let color = create_test_color();

        // Basic sanity check - both delegates should return valid tones
        let tone_2021 = delegate_2021.get_tone(&scheme, &color);
        let tone_2025 = delegate_2025.get_tone(&scheme, &color);

        assert!(tone_2021 >= 0.0 && tone_2021 <= 100.0);
        assert!(tone_2025 >= 0.0 && tone_2025 <= 100.0);
    }

    #[test]
    fn test_delegate_2021_get_tone() {
        let delegate = ColorCalculationDelegate2021;
        let scheme = create_test_scheme();
        let color = create_test_color();

        let tone = delegate.get_tone(&scheme, &color);
        // The test color has a fixed tone of 40.0
        assert_eq!(tone, 40.0);
    }

    #[test]
    fn test_delegate_2025_get_tone() {
        let delegate = ColorCalculationDelegate2025;
        let scheme = create_test_scheme();
        let color = create_test_color();

        let tone = delegate.get_tone(&scheme, &color);
        // The test color has a fixed tone of 40.0
        assert_eq!(tone, 40.0);
    }

    #[test]
    fn test_delegate_2021_get_hct() {
        let delegate = ColorCalculationDelegate2021;
        let scheme = create_test_scheme();
        let color = create_test_color();

        let hct = delegate.get_hct(&scheme, &color);
        // HCT should be valid
        assert!(hct.tone() >= 0.0 && hct.tone() <= 100.0);
        assert!(hct.hue() >= 0.0 && hct.hue() < 360.0);
        assert!(hct.chroma() >= 0.0);
    }

    #[test]
    fn test_delegate_2025_get_hct() {
        let delegate = ColorCalculationDelegate2025;
        let scheme = create_test_scheme();
        let color = create_test_color();

        let hct = delegate.get_hct(&scheme, &color);
        // HCT should be valid
        assert!(hct.tone() >= 0.0 && hct.tone() <= 100.0);
        assert!(hct.hue() >= 0.0 && hct.hue() < 360.0);
        assert!(hct.chroma() >= 0.0);
    }

    #[test]
    fn test_delegate_2021_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ColorCalculationDelegate2021>();
    }

    #[test]
    fn test_delegate_2025_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ColorCalculationDelegate2025>();
    }

    #[test]
    fn test_static_delegates_are_accessible() {
        // Ensure static delegates can be accessed
        let _: &dyn ColorCalculationDelegate = &SPEC_2021;
        let _: &dyn ColorCalculationDelegate = &SPEC_2025;
    }
}

// <FILE>crates/mcu-dynamiccolor/src/color_calculation.rs</FILE> - <DESC>ColorCalculationDelegate trait and spec-versioned implementations</DESC>
// <VERS>END OF VERSION: 3.0.0</VERS>
